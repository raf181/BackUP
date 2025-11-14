package main

import (
	"bufio"
	"context"
	"encoding/json"
	"flag"
	"fmt"
	"io"
	"io/fs"
	"os"
	"os/exec"
	"os/signal"
	"path/filepath"
	"runtime"
	"sort"
	"strconv"
	"strings"
	"sync"
	"sync/atomic"
	"syscall"
	"time"

	tea "github.com/charmbracelet/bubbletea"
	"github.com/charmbracelet/lipgloss"
)

type Tier struct {
	Name     string   `json:"name"`
	Priority int      `json:"priority"`
	Patterns []string `json:"patterns"`
}

type FileInfoRec struct {
	Path     string
	Size     int64
	MTime    time.Time
	Priority int
}

type ManifestRec struct {
	Src      string  `json:"src"`
	Dst      string  `json:"dst"`
	Size     int64   `json:"size"`
	MTime    int64   `json:"mtime"`
	Priority int     `json:"priority"`
	Status   string  `json:"status"`
	Message  string  `json:"message"`
	Ts       float64 `json:"ts"`
}

var (
	excludedDirNames = map[string]struct{}{
		".git": {}, ".hg": {}, ".svn": {}, "node_modules": {}, "__pycache__": {}, ".cache": {}, ".npm": {}, ".gradle": {}, ".m2": {},
		".venv": {}, "venv": {}, "env": {}, ".tox": {}, ".idea": {}, ".vscode": {}, ".DS_Store": {},
	}
	excludedGlobs = []string{
		"*/.Trash/*", "*/.local/share/Trash/*", "*/.thumbnails/*", "*/Temp/*", "*/tmp/*",
	}
)

// fastSSDMode toggles runtime heuristics for very fast SSD/NVMe devices.
var fastSSDMode bool
var noProgress bool
var boostMode bool

func main() {
	// Flags
	sourcesFlag := flag.String("sources", defaultHome(), "Comma-separated source directories to scan")
	objective := flag.String("objective", "count", "Selection objective: count|space")
	excludeFlag := flag.String("exclude", "", "Comma-separated extra exclude glob patterns (full path)")
	profile := flag.String("profile", "importance_profile.json", "Importance profile JSON path (on USB or absolute)")
	destSubdir := flag.String("dest-subdir", "", "Destination subfolder on USB; if empty, auto-named unless --resume")
	dryRun := flag.Bool("dry-run", false, "Plan only, do not copy")
	resume := flag.Bool("resume", false, "Resume into existing dest-subdir (no new dir)")
	workers := flag.Int("workers", 0, "Concurrent copy workers (0=auto: all CPU cores)")
	reserve := flag.Int64("reserve", 0, "Reserve bytes to leave free on USB (default 0 for maximum space)")
	noProg := flag.Bool("no-progress", false, "Disable progress UI/log updates (max throughput mode)")
	fastSSD := flag.Bool("fast-ssd", false, "Optimize copy heuristics for very fast SSD/NVMe (fewer syscalls on large files)")
	boost := flag.Bool("boost", false, "High-performance mode: raise process priority, enable fast-ssd heuristics, keep GUI")
	flag.Parse()

	if *noProg {
		noProgress = true
	}

	if *boost {
		boostMode = true
	}

	if *fastSSD || boostMode {
		fastSSDMode = true
		// Adjust thresholds for high-throughput media: treat more files as "small" to collapse loop overhead
		// and lower threshold for direct kernel-assisted copy path.
		smallFileThreshold = 512 << 10      // 512 KiB
		largeFileDirectThreshold = 16 << 20 // 16 MiB
	}
	if boostMode {
		// Slightly aggressive: ensure direct threshold not above 16 MiB
		if largeFileDirectThreshold > 16<<20 {
			largeFileDirectThreshold = 16 << 20
		}
		// Elevate process priority best-effort
		elevatePriority()
	}

	usbRoot, err := usbRoot()
	mustNoErr(err)

	free := usableFreeSpace(usbRoot, *reserve)
	destDir := *destSubdir
	if destDir == "" && !*resume {
		destDir = "backup_" + time.Now().Format("20060102_150405")
	}
	if destDir != "" {
		// Validate destSubdir to prevent path traversal attacks
		// It should not contain ".." or start with "/" or "\\"
		if strings.Contains(destDir, "..") || strings.HasPrefix(destDir, string(os.PathSeparator)) || strings.HasPrefix(destDir, "/") {
			fail(fmt.Errorf("invalid destination subdirectory: path traversal detected"))
		}
		destDir = filepath.Join(usbRoot, destDir)
		// Verify the result is still under usbRoot after joining
		realDestDir, err := filepath.Abs(destDir)
		realUsbRoot, err2 := filepath.Abs(usbRoot)
		if err != nil || err2 != nil || !strings.HasPrefix(realDestDir, realUsbRoot) {
			fail(fmt.Errorf("destination directory is outside USB root"))
		}
	} else {
		destDir = usbRoot
	}
	mustNoErr(os.MkdirAll(destDir, 0o755))

	// Load importance tiers
	profilePath := *profile
	if !filepath.IsAbs(profilePath) {
		profilePath = filepath.Join(usbRoot, profilePath)
	}
	// Validate profile path to prevent path traversal when used with USB root
	if !filepath.IsAbs(*profile) {
		// If relative path, ensure it doesn't escape usbRoot
		realProfilePath, err := filepath.Abs(profilePath)
		realUsbRoot, err2 := filepath.Abs(usbRoot)
		if err != nil || err2 != nil || !strings.HasPrefix(realProfilePath, realUsbRoot) {
			fmt.Fprintf(os.Stderr, "warning: profile path escapes USB root, using default\n")
			profilePath = filepath.Join(usbRoot, "importance_profile.json")
		}
	}
	tiers, _ := loadImportanceProfile(profilePath)

	fmt.Printf("USB root: %s\n", usbRoot)
	fmt.Printf("Destination: %s\n", destDir)
	fmt.Printf("Free space (usable): %s\n", humanSize(free))

	// Parse sources and excludes
	sources := splitNonEmpty(*sourcesFlag)
	excludes := append([]string{}, excludedGlobs...)
	excludes = append(excludes, splitNonEmpty(*excludeFlag)...)

	// Create cancellable context and handle Ctrl+C
	ctx, cancel := context.WithCancel(context.Background())
	defer cancel()

	sigCh := make(chan os.Signal, 1)
	signal.Notify(sigCh, os.Interrupt, syscall.SIGTERM)
	go func() {
		first := true
		for s := range sigCh {
			_ = s
			if first {
				// request graceful shutdown
				fmt.Fprintln(os.Stderr, "\nInterrupt received, stopping gracefully...")
				cancel()
				first = false
			} else {
				// second signal: force exit
				fmt.Fprintln(os.Stderr, "Second interrupt, exiting")
				os.Exit(1)
			}
		}
	}()

	// Initialize TUI early so nicer output is visible from the start
	var tui *TUI
	if !*noProg {
		tui = NewTUI(cancel)
		// Ensure Close is called on exit
		defer tui.Close()
	}

	// Scan
	t0 := time.Now()
	if tui != nil {
		tui.AppendLog("Starting scan...")
	}
	files := scanSources(ctx, sources, tiers, excludes, usbRoot, tui)
	t1 := time.Since(t0)
	var totalBytes int64
	for _, f := range files {
		totalBytes += f.Size
	}
	fmt.Printf("Scanned %d files in %.2fs (%s total)\n", len(files), t1.Seconds(), humanSize(totalBytes))

	// Select
	selected, used := selectFiles(files, free, *objective)
	fmt.Printf("Selected %d files totalling %s (objective: %s)\n", len(selected), humanSize(used), *objective)

	// Plans
	plans := make([][2]string, 0, len(selected)) // [src, dst]
	for _, fi := range selected {
		rel := relativeDestPath(fi.Path, sources)
		dst := filepath.Join(destDir, rel)
		plans = append(plans, [2]string{fi.Path, dst})
	}

	// Filter existing same-size
	toCopy := make([][2]string, 0, len(plans))
	skippedExisting := 0
	for _, p := range plans {
		src, dst := p[0], p[1]
		if st, err := os.Stat(dst); err == nil {
			if st.Mode().IsRegular() {
				if sst, err2 := os.Stat(src); err2 == nil && sst.Size() == st.Size() {
					skippedExisting++
					continue
				}
			}
		}
		toCopy = append(toCopy, p)
	}

	var toCopyBytes int64
	for _, p := range toCopy {
		if st, err := os.Stat(p[0]); err == nil {
			toCopyBytes += st.Size()
		}
	}
	fmt.Printf("Already present (same size): %d files\n", skippedExisting)
	fmt.Printf("To copy now: %d files, %s\n", len(toCopy), humanSize(toCopyBytes))

	manifestPath := filepath.Join(destDir, "backup-manifest.jsonl")
	if *dryRun {
		// summarize by top priorities
		counts := map[int]int{}
		for _, f := range selected {
			counts[f.Priority]++
		}
		type kv struct{ P, C int }
		list := make([]kv, 0, len(counts))
		for p, c := range counts {
			list = append(list, kv{p, c})
		}
		sort.Slice(list, func(i, j int) bool { return list[i].P > list[j].P })
		if len(list) > 5 {
			list = list[:5]
		}
		fmt.Printf("Plan by priority (top 5): %v\n", list)
		fmt.Println("Dry run complete. No files were copied.")
		return
	}

	// Copy concurrently
	w := *workers
	if w <= 0 {
		w = runtime.NumCPU()
	}
	if w < 1 {
		w = 1
	}
	fmt.Printf("Starting copy with %d worker(s)...\n", w)
	start := time.Now()
	copied, errorsN := copyAll(ctx, toCopy, manifestPath, w, tui)
	fmt.Printf("Copy complete in %.2fs: copied=%d, skipped=%d, errors=%d\n", time.Since(start).Seconds(), copied, skippedExisting, errorsN)
}

func defaultHome() string {
	if h, err := os.UserHomeDir(); err == nil {
		return h
	}
	return "/"
}

func usbRoot() (string, error) {
	exe, err := os.Executable()
	if err != nil {
		return "", err
	}
	return filepath.Dir(exe), nil
}

func usableFreeSpace(path string, reserve int64) int64 {
	// Cross-platform disk space detection
	if runtime.GOOS == "windows" {
		return getWindowsFreeSpace(path, reserve)
	}
	return getUnixFreeSpace(path, reserve)
}

func loadImportanceProfile(path string) ([]Tier, error) {
	f, err := os.Open(path)
	if err != nil {
		return defaultProfile(), err
	}
	defer f.Close()
	var raw struct {
		Tiers []Tier `json:"tiers"`
	}
	if err := json.NewDecoder(f).Decode(&raw); err != nil {
		return defaultProfile(), err
	}
	sort.Slice(raw.Tiers, func(i, j int) bool { return raw.Tiers[i].Priority > raw.Tiers[j].Priority })
	return raw.Tiers, nil
}

func defaultProfile() []Tier {
	return []Tier{
		{Name: "Documents", Priority: 100, Patterns: []string{"*.pdf", "*.doc", "*.docx", "*.odt", "*.rtf", "*.txt", "*.md", "*.xls", "*.xlsx", "*.ods", "*.csv", "*.tsv", "*.ppt", "*.pptx"}},
		{Name: "Project Files", Priority: 95, Patterns: []string{"*.tex", "*.ipynb", "*.py", "*.R", "*.m", "*.java", "*.cs", "*.cpp", "*.c", "*.ts", "*.js"}},
		{Name: "Images", Priority: 90, Patterns: []string{"*.jpg", "*.jpeg", "*.png", "*.gif", "*.tiff", "*.bmp", "*.heic", "*.webp"}},
		{Name: "Audio", Priority: 60, Patterns: []string{"*.mp3", "*.m4a", "*.flac", "*.wav", "*.aac", "*.ogg"}},
		{Name: "Videos", Priority: 50, Patterns: []string{"*.mp4", "*.mov", "*.avi", "*.mkv", "*.webm"}},
		{Name: "Archives", Priority: 40, Patterns: []string{"*.zip", "*.tar", "*.gz", "*.bz2", "*.xz", "*.7z", "*.rar"}},
		{Name: "Everything else", Priority: 10, Patterns: []string{"*"}},
	}
}

func splitNonEmpty(s string) []string {
	if strings.TrimSpace(s) == "" {
		return nil
	}
	parts := strings.Split(s, ",")
	out := make([]string, 0, len(parts))
	for _, p := range parts {
		p = strings.TrimSpace(p)
		if p != "" {
			out = append(out, p)
		}
	}
	return out
}

func humanSize(n int64) string {
	units := []string{"B", "KB", "MB", "GB", "TB"}
	i := 0
	x := float64(n)
	for x >= 1024 && i < len(units)-1 {
		x /= 1024
		i++
	}
	return fmt.Sprintf("%.2f %s", x, units[i])
}

func scanSources(ctx context.Context, sources []string, tiers []Tier, excludes []string, autoExcludeRoot string, tui *TUI) []FileInfoRec {
	if len(tiers) == 0 {
		tiers = defaultProfile()
	}
	autoExcludeRoot, _ = filepath.Abs(autoExcludeRoot)
	var out []FileInfoRec
	lowers := lowerAll(excludes)
	// progress counters for scan
	var scanned int64
	lastReport := time.Now()
	for _, src := range sources {
		select {
		case <-ctx.Done():
			if tui != nil {
				tui.AppendLog("Scan cancelled")
			}
			return out
		default:
		}
		src = expandPath(src)
		if st, err := os.Stat(src); err != nil || !st.IsDir() {
			continue
		}
		absSrc, _ := filepath.Abs(src)
		if prefixOf(absSrc, autoExcludeRoot) {
			fmt.Printf("Auto-excluded (USB): %s\n", src)
			continue
		}
		stack := []string{absSrc}
		for len(stack) > 0 {
			cur := stack[len(stack)-1]
			stack = stack[:len(stack)-1]
			entries, err := os.ReadDir(cur)
			if err != nil {
				continue
			}
			for _, e := range entries {
				select {
				case <-ctx.Done():
					if tui != nil {
						tui.AppendLog("Scan cancelled")
					}
					return out
				default:
				}
				name := e.Name()
				full := filepath.Join(cur, name)
				if e.IsDir() {
					if _, skip := excludedDirNames[name]; skip {
						continue
					}
					if matchAny(full, excludes) {
						continue
					}
					stack = append(stack, full)
				} else {
					if (e.Type() & fs.ModeSymlink) != 0 {
						continue
					}
					info, err := e.Info()
					if err != nil {
						continue
					}
					if !info.Mode().IsRegular() {
						continue
					}
					if matchAny(strings.ToLower(full), lowers) {
						continue
					}
					pr := priorityFor(full, tiers)
					out = append(out, FileInfoRec{Path: full, Size: info.Size(), MTime: info.ModTime(), Priority: pr})
					scanned++
					if tui != nil && time.Since(lastReport) > 500*time.Millisecond {
						tui.AppendLog(fmt.Sprintf("Scanning: %d files found...", scanned))
						lastReport = time.Now()
					}
				}
			}
		}
	}
	return out
}

func lowerAll(in []string) []string {
	out := make([]string, len(in))
	for i, s := range in {
		out[i] = strings.ToLower(s)
	}
	return out
}

func matchAny(path string, patterns []string) bool {
	p := path
	for _, pat := range patterns {
		if ok, _ := filepath.Match(pat, p); ok {
			return true
		}
	}
	return false
}

func priorityFor(path string, tiers []Tier) int {
	p := strings.ToLower(path)
	base := strings.ToLower(filepath.Base(path))
	for _, t := range tiers {
		for _, pat := range t.Patterns {
			pl := strings.ToLower(pat)
			if ok, _ := filepath.Match(pl, base); ok {
				return t.Priority
			}
			if ok, _ := filepath.Match(pl, p); ok {
				return t.Priority
			}
		}
	}
	return 0
}

func selectFiles(files []FileInfoRec, capacity int64, objective string) ([]FileInfoRec, int64) {
	byPr := map[int][]FileInfoRec{}
	for _, f := range files {
		if f.Size > 0 {
			byPr[f.Priority] = append(byPr[f.Priority], f)
		}
	}
	var selected []FileInfoRec
	var used int64
	var prs []int
	for p := range byPr {
		prs = append(prs, p)
	}
	sort.Slice(prs, func(i, j int) bool { return prs[i] > prs[j] })
	for _, pr := range prs {
		items := byPr[pr]
		if objective == "count" {
			sort.Slice(items, func(i, j int) bool { return items[i].Size < items[j].Size })
		} else {
			sort.Slice(items, func(i, j int) bool { return items[i].Size > items[j].Size })
		}
		for _, f := range items {
			if used+f.Size <= capacity {
				selected = append(selected, f)
				used += f.Size
			}
		}
	}
	return selected, used
}

func relativeDestPath(src string, bases []string) string {
	srcAbs, _ := filepath.Abs(src)
	best := ""
	for _, b := range bases {
		bAbs, _ := filepath.Abs(expandPath(b))
		if prefixOf(srcAbs, bAbs) && len(bAbs) > len(best) {
			best = bAbs
		}
	}
	if best == "" {
		return filepath.Base(srcAbs)
	}
	rel, err := filepath.Rel(best, srcAbs)
	if err != nil || strings.HasPrefix(rel, "..") {
		return filepath.Base(srcAbs)
	}
	return rel
}

func prefixOf(path, base string) bool {
	if path == base {
		return true
	}
	p := filepath.Clean(path)
	b := filepath.Clean(base)
	if len(b) == 0 || len(p) < len(b) {
		return false
	}
	if p == b {
		return true
	}
	if strings.HasPrefix(p, b+string(os.PathSeparator)) {
		return true
	}
	return false
}

func copyAll(ctx context.Context, pairs [][2]string, manifestPath string, workers int, tui *TUI) (int, int) {
	jobs := make(chan [2]string, workers*2)
	var wg sync.WaitGroup
	var mu sync.Mutex
	copied := 0
	errorsN := 0
	// Compute total bytes to copy
	var totalBytes int64
	for _, p := range pairs {
		if st, err := os.Stat(p[0]); err == nil {
			totalBytes += st.Size()
		}
	}
	// Progress aggregator
	agg := &progressAgg{total: totalBytes, start: time.Now()}
	// UI / ticker setup
	stopCh := make(chan struct{})
	interactive := !noProgress && isTTY()
	var logsCh chan string
	if interactive {
		logsCh = make(chan string, 1024)
		if tui == nil {
			// Create a no-op cancel func if tui wasn't created in main
			noopCancel := func() {}
			tui = NewTUI(noopCancel)
			defer tui.Close()
		}
		tui.DrawStatic()
		// UI loop: single writer to terminal
		go func() {
			ticker := time.NewTicker(200 * time.Millisecond)
			defer ticker.Stop()
			for {
				select {
				case <-stopCh:
					// final paint and close
					tui.DrawTop(agg)
					tui.Close()
					return
				case msg := <-logsCh:
					// drain burst
					for {
						tui.AppendLog(msg)
						select {
						case msg = <-logsCh:
							continue
						default:
						}
						break
					}
					tui.DrawLogs()
				case <-ticker.C:
					tui.DrawTop(agg)
				}
			}
		}()
	} else {
		// Non-interactive: print total line each second
		go func() {
			ticker := time.NewTicker(1 * time.Second)
			defer ticker.Stop()
			for {
				select {
				case <-stopCh:
					return
				case <-ticker.C:
					done := agg.Done()
					elapsed := time.Since(agg.start).Seconds()
					speed := float64(0)
					if elapsed > 0 {
						speed = float64(done) / elapsed
					}
					remaining := agg.total - done
					eta := "--:--:--"
					if speed > 1 {
						eta = formatETA(float64(remaining) / speed)
					}
					mu.Lock()
					fmt.Printf("[TOTAL] %s / %s (%.1f%%) | %s/s | ETA %s\n", humanSize(done), humanSize(agg.total), percent(done, agg.total), humanSize(int64(speed)), eta)
					mu.Unlock()
				}
			}
		}()
	}
	mf, err := os.OpenFile(manifestPath, os.O_CREATE|os.O_WRONLY|os.O_APPEND, 0o644)
	if err != nil {
		// Log error but continue - manifest is optional
		fmt.Fprintf(os.Stderr, "warning: failed to open manifest file: %v\n", err)
		return copied, errorsN
	}
	mw := bufio.NewWriter(mf)
	writeManifest := func(rec ManifestRec) {
		b, err := json.Marshal(rec)
		if err != nil {
			// Log JSON marshaling error but continue
			fmt.Fprintf(os.Stderr, "warning: failed to marshal manifest record: %v\n", err)
			return
		}
		if _, err := mw.Write(b); err != nil {
			fmt.Fprintf(os.Stderr, "warning: failed to write manifest: %v\n", err)
			return
		}
		if err := mw.WriteByte('\n'); err != nil {
			fmt.Fprintf(os.Stderr, "warning: failed to write manifest newline: %v\n", err)
			return
		}
	}
	worker := func() {
		defer wg.Done()
		for p := range jobs {
			src, dst := p[0], p[1]
			select {
			case <-ctx.Done():
				// interrupted
				mu.Lock()
				errorsN++
				rec := ManifestRec{Src: src, Dst: dst, Size: 0, MTime: 0, Priority: 0, Status: "cancelled", Message: "interrupted", Ts: float64(time.Now().UnixNano()) / 1e9}
				writeManifest(rec)
				mu.Unlock()
				continue
			default:
			}
			status, msg := copyOneWithProgress(ctx, src, dst, agg, &mu, logsCh, interactive)
			st, _ := os.Stat(src)
			mu.Lock()
			if status == "copied" {
				copied++
			} else if status == "error" {
				errorsN++
			}
			rec := ManifestRec{Src: src, Dst: dst, Size: safeSize(st), MTime: safeMTime(st), Priority: 0, Status: status, Message: msg, Ts: float64(time.Now().UnixNano()) / 1e9}
			writeManifest(rec)
			mu.Unlock()
		}
	}
	for i := 0; i < workers; i++ {
		wg.Add(1)
		go worker()
	}
	for _, p := range pairs {
		jobs <- p
	}
	close(jobs)
	wg.Wait()
	close(stopCh)
	if err := mw.Flush(); err != nil {
		fmt.Fprintf(os.Stderr, "warning: failed to flush manifest: %v\n", err)
	}
	if err := mf.Close(); err != nil {
		fmt.Fprintf(os.Stderr, "warning: failed to close manifest file: %v\n", err)
	}
	return copied, errorsN
}

func safeSize(fi os.FileInfo) int64 {
	if fi == nil {
		return 0
	}
	return fi.Size()
}
func safeMTime(fi os.FileInfo) int64 {
	if fi == nil {
		return 0
	}
	return fi.ModTime().Unix()
}

func copyOneWithProgress(ctx context.Context, src, dst string, agg *progressAgg, mu *sync.Mutex, logsCh chan string, interactive bool) (string, string) {
	if err := os.MkdirAll(filepath.Dir(dst), 0o755); err != nil {
		return "error", err.Error()
	}
	if dstSt, err := os.Stat(dst); err == nil {
		if srcSt, err2 := os.Stat(src); err2 == nil {
			if dstSt.Size() == srcSt.Size() {
				return "skipped", "exists-same-size"
			}
		}
	}
	tmp := dst + ".part"
	_ = os.Remove(tmp)
	// announce start
	if logsCh != nil {
		name := filepath.Base(src)
		if st, err := os.Stat(src); err == nil {
			select {
			case logsCh <- fmt.Sprintf("Start: %s (%s)", name, humanSize(st.Size())):
			default:
			}
		} else {
			select {
			case logsCh <- fmt.Sprintf("Start: %s", name):
			default:
			}
		}
	} else if !interactive {
		fmt.Printf("Start: %s\n", filepath.Base(src))
	}
	if err := copyFileWithProgress(ctx, src, tmp, agg, mu, logsCh, interactive); err != nil {
		_ = os.Remove(tmp)
		return "error", err.Error()
	}
	if err := os.Rename(tmp, dst); err != nil {
		_ = os.Remove(tmp)
		return "error", err.Error()
	}
	if logsCh != nil {
		select {
		case logsCh <- fmt.Sprintf("Done: %s", filepath.Base(src)):
		default:
		}
	} else if !interactive {
		fmt.Printf("Done: %s\n", filepath.Base(src))
	}
	return "copied", "ok"
}

// copyFileWithProgress used instead of legacy copyFile

type progressAgg struct {
	total int64
	done  int64 // atomic
	start time.Time
}

// --- Copy performance helpers ---
// Large reusable buffers significantly reduce syscalls and improve throughput on HDD/USB.
var copyBufPool = sync.Pool{New: func() any {
	// 8 MiB buffer strikes a good balance for spinning disks and USB drives
	b := make([]byte, 8<<20)
	return &b
}}

// Threshold under which we treat a file as "small" and copy via a single read/write.
// Default 256 KiB; may be increased at runtime (fast SSD mode) for further syscall reduction.
var smallFileThreshold = 256 << 10 // 256 KiB (runtime adjustable)

// Threshold above which (and when in fastSSDMode) we use a direct io.Copy path to allow
// the runtime to leverage platform copy accelerations (e.g., copy_file_range / sendfile /
// system-level block cloning) for large files, minimizing user-space read/write loops.
var largeFileDirectThreshold int64 = 32 << 20 // 32 MiB default (runtime adjustable)

// A separate pool for small-file buffers to avoid retaining large 8 MiB slices when
// copying many tiny files (which would waste memory / cache).
var smallCopyBufPool = sync.Pool{New: func() any {
	b := make([]byte, smallFileThreshold)
	return &b
}}

func bufPoolGet() *[]byte { return copyBufPool.Get().(*[]byte) }
func bufPoolPut(b *[]byte) {
	if b != nil {
		copyBufPool.Put(b)
	}
}
func smallBufPoolGet() *[]byte { return smallCopyBufPool.Get().(*[]byte) }
func smallBufPoolPut(b *[]byte) {
	if b != nil {
		smallCopyBufPool.Put(b)
	}
}

// Platform-specific openFileSequentialRead/openFileSequentialWrite are implemented
// in open_unix.go and open_windows.go.

func (p *progressAgg) Add(n int64) { atomic.AddInt64(&p.done, n) }
func (p *progressAgg) Done() int64 { return atomic.LoadInt64(&p.done) }

func copyFileWithProgress(ctx context.Context, src, dst string, agg *progressAgg, mu *sync.Mutex, logsCh chan string, interactive bool) error {
	// Use OS-optimized open for better throughput
	in, err := openFileSequentialRead(src)
	if err != nil {
		return err
	}
	defer in.Close()
	st, err := in.Stat()
	if err != nil {
		return err
	}
	out, err := openFileSequentialWrite(dst, st.Mode().Perm())
	if err != nil {
		return err
	}
	defer out.Close()
	// Preallocate destination size when possible to reduce fragmentation.
	_ = out.Truncate(st.Size())

	// Fast path for small files: single read + single write.
	if st.Size() <= int64(smallFileThreshold) {
		started := time.Now()
		name := filepath.Base(src)
		// Zero-sized file fast path
		if st.Size() == 0 {
			// Nothing to read/write; still finalize times for consistency
			_ = os.Chtimes(dst, time.Now(), st.ModTime())
			if agg != nil {
				agg.Add(0)
			}
			// Log final (mirrors large path final message construction)
			final := fmt.Sprintf("%s done: %s in %0.2fs (%s/s)", name, humanSize(0), 0.00, humanSize(0))
			if logsCh != nil {
				select {
				case logsCh <- final:
				default:
				}
			} else if !interactive {
				mu.Lock()
				fmt.Printf("[FILE] %s\n", final)
				mu.Unlock()
			}
			return nil
		}
		// Acquire small buffer sized for threshold; only use first n bytes
		bufPtr := smallBufPoolGet()
		defer smallBufPoolPut(bufPtr)
		buf := *bufPtr
		n := int(st.Size())
		if n > len(buf) { // defensive (should not happen)
			buf = make([]byte, n)
		}
		if _, err := io.ReadFull(in, buf[:n]); err != nil {
			return err
		}
		select {
		case <-ctx.Done():
			return fmt.Errorf("cancelled")
		default:
		}
		if _, err := out.Write(buf[:n]); err != nil {
			return err
		}
		if agg != nil {
			agg.Add(int64(n))
		}
		_ = os.Chtimes(dst, time.Now(), st.ModTime())
		dur := time.Since(started).Seconds()
		spd := float64(0)
		if dur > 0 {
			spd = float64(n) / dur
		}
		if !noProgress {
			final := fmt.Sprintf("%s done: %s in %0.2fs (%s/s)", name, humanSize(int64(n)), dur, humanSize(int64(spd)))
			if logsCh != nil {
				select {
				case logsCh <- final:
				default:
				}
			} else if !interactive {
				mu.Lock()
				fmt.Printf("[FILE] %s\n", final)
				mu.Unlock()
			}
		}
		return nil
	}

	// Large fast path (fast SSD mode only): rely on io.Copy to exploit optimized kernel paths.
	if fastSSDMode && st.Size() >= largeFileDirectThreshold {
		started := time.Now()
		name := filepath.Base(src)
		// Perform copy in one call; io.Copy will attempt to use optimized syscalls.
		n, err := io.Copy(out, in)
		if err != nil {
			return err
		}
		select {
		case <-ctx.Done():
			return fmt.Errorf("cancelled")
		default:
		}
		if agg != nil {
			agg.Add(n)
		}
		_ = os.Chtimes(dst, time.Now(), st.ModTime())
		dur := time.Since(started).Seconds()
		spd := float64(0)
		if dur > 0 {
			spd = float64(n) / dur
		}
		if !noProgress {
			final := fmt.Sprintf("%s done: %s in %0.2fs (%s/s)", name, humanSize(n), dur, humanSize(int64(spd)))
			if logsCh != nil {
				select {
				case logsCh <- final:
				default:
				}
			} else if !interactive {
				mu.Lock()
				fmt.Printf("[FILE] %s\n", final)
				mu.Unlock()
			}
		}
		return nil
	}
	// Reuse a large buffer to reduce syscalls and improve throughput
	bufPtr := bufPoolGet()
	defer bufPoolPut(bufPtr)
	buf := *bufPtr
	var done int64
	started := time.Now()
	lastPrint := time.Time{}
	name := filepath.Base(src)
	for {
		nr, er := in.Read(buf)
		if nr > 0 {
			nw, ew := out.Write(buf[:nr])
			if ew != nil {
				return ew
			}
			if nw < nr {
				return io.ErrShortWrite
			}
			done += int64(nw)
			if agg != nil {
				agg.Add(int64(nw))
			}
			select {
			case <-ctx.Done():
				return fmt.Errorf("cancelled")
			default:
			}
			// Throttled per-file progress (1s)
			now := time.Now()
			if !noProgress && now.Sub(lastPrint) >= time.Second {
				elapsed := now.Sub(started).Seconds()
				speed := float64(0)
				if elapsed > 0 {
					speed = float64(done) / elapsed
				}
				remaining := st.Size() - done
				eta := "--:--:--"
				if speed > 1 {
					eta = formatETA(float64(remaining) / speed)
				}
				line := fmt.Sprintf("%s %5.1f%% | %s/s | ETA %s", name, percent(done, st.Size()), humanSize(int64(speed)), eta)
				if logsCh != nil {
					select {
					case logsCh <- line:
					default:
					}
				} else if !interactive {
					mu.Lock()
					fmt.Printf("[FILE] %s\n", line)
					mu.Unlock()
				}
				lastPrint = now
			}
		}
		if er != nil {
			if er == io.EOF {
				break
			}
			return er
		}
	}
	// Finalize times
	_ = os.Chtimes(dst, time.Now(), st.ModTime())
	dur := time.Since(started).Seconds()
	spd := float64(0)
	if dur > 0 {
		spd = float64(done) / dur
	}
	if !noProgress {
		final := fmt.Sprintf("%s done: %s in %0.2fs (%s/s)", name, humanSize(done), dur, humanSize(int64(spd)))
		if logsCh != nil {
			select {
			case logsCh <- final:
			default:
			}
		} else if !interactive {
			mu.Lock()
			fmt.Printf("[FILE] %s\n", final)
			mu.Unlock()
		}
	}
	return nil
}

func percent(done, total int64) float64 {
	if total <= 0 {
		return 0
	}
	return float64(done) * 100.0 / float64(total)
}

func formatETA(sec float64) string {
	if sec < 0 {
		sec = 0
	}
	s := int64(sec + 0.5)
	h := s / 3600
	m := (s % 3600) / 60
	ss := s % 60
	if h > 99 {
		h = 99
	} // cap to 99 hours for display
	return fmt.Sprintf("%02d:%02d:%02d", h, m, ss)
}

// --- Console helpers for a static TOTAL line ---
func isTTY() bool {
	fi, err := os.Stdout.Stat()
	if err != nil {
		return false
	}
	return (fi.Mode() & os.ModeCharDevice) != 0
}

func printTotalLine(line string) {
	if isTTY() {
		// Carriage return + clear line + print without newline
		fmt.Printf("\r\x1b[2K%s", line)
	} else {
		// Non-interactive: just print lines normally
		fmt.Println(line)
	}
}

func formatTotalLine(agg *progressAgg) string {
	done := agg.Done()
	elapsed := time.Since(agg.start).Seconds()
	speed := float64(0)
	if elapsed > 0 {
		speed = float64(done) / elapsed
	}
	remaining := agg.total - done
	eta := "--:--:--"
	if speed > 1 {
		eta = formatETA(float64(remaining) / speed)
	}
	return fmt.Sprintf("[TOTAL] %s / %s (%.1f%%) | %s/s | ETA %s",
		humanSize(done), humanSize(agg.total), percent(done, agg.total), humanSize(int64(speed)), eta)
}

// ---------- Enhanced Cross-Platform TUI ----------
// Charm-based TUI using Bubble Tea and Lip Gloss.
// We expose a small compatibility wrapper so existing code can call the same methods.

type TUI struct {
	model     *teaProgram
	logsCh    chan string
	quitCh    chan struct{}
	cancelCh  chan struct{} // signal to cancel context from UI
	prog      *tea.Program
	closeOnce sync.Once
}

type teaProgram struct {
	ready      bool
	width      int
	height     int
	total      int64
	done       int64
	start      time.Time
	logs       []string
	styles     uiStyles
	quitting   bool
	cancelFunc context.CancelFunc
}

type uiStyles struct {
	header lipgloss.Style
	box    lipgloss.Style
	bar    lipgloss.Style
	info   lipgloss.Style
	log    lipgloss.Style
	dim    lipgloss.Style
	help   lipgloss.Style
}

func NewTUI(cancelFunc context.CancelFunc) *TUI {
	p := &teaProgram{
		start:      time.Now(),
		logs:       make([]string, 0),
		cancelFunc: cancelFunc,
	}

	// Define beautiful styles with borders
	p.styles = uiStyles{
		header: lipgloss.NewStyle().Bold(true).Foreground(lipgloss.Color("#00D9FF")).MarginBottom(1),
		box: lipgloss.NewStyle().
			Border(lipgloss.RoundedBorder()).
			BorderForeground(lipgloss.Color("#874BFD")).
			Padding(0, 1),
		bar:  lipgloss.NewStyle().Foreground(lipgloss.Color("#00FF87")),
		info: lipgloss.NewStyle().Foreground(lipgloss.Color("#FAFAFA")),
		log:  lipgloss.NewStyle().Foreground(lipgloss.Color("#999999")),
		dim:  lipgloss.NewStyle().Foreground(lipgloss.Color("#666666")),
		help: lipgloss.NewStyle().Foreground(lipgloss.Color("#FFD700")).Italic(true),
	}

	tui := &TUI{
		model:    p,
		logsCh:   make(chan string, 1024),
		quitCh:   make(chan struct{}),
		cancelCh: make(chan struct{}, 1),
	}

	// Start Bubble Tea program in background and retain handle
	go func() {
		m := p
		program := tea.NewProgram(m, tea.WithAltScreen(), tea.WithMouseCellMotion())
		tui.prog = program
		_ = program.Start()
	}()

	// forward logs into model
	go func() {
		for {
			select {
			case l := <-tui.logsCh:
				p.logs = append(p.logs, l)
				if len(p.logs) > 1000 {
					p.logs = p.logs[len(p.logs)-1000:]
				}
				// Send update to program
				if tui.prog != nil {
					tui.prog.Send(logUpdateMsg{})
				}
			case <-tui.quitCh:
				return
			}
		}
	}()

	return tui
}

type logUpdateMsg struct{}
type progressUpdateMsg struct{}

// Bubbletea Model implementation with keyboard handling
func (m *teaProgram) Init() tea.Cmd {
	return tea.Batch(
		tea.EnterAltScreen,
		tickCmd(),
	)
}

func tickCmd() tea.Cmd {
	return tea.Tick(time.Millisecond*100, func(t time.Time) tea.Msg {
		return progressUpdateMsg{}
	})
}

func (m *teaProgram) Update(msg tea.Msg) (tea.Model, tea.Cmd) {
	switch msg := msg.(type) {
	case tea.KeyMsg:
		switch msg.String() {
		case "ctrl+c", "q":
			if !m.quitting {
				m.quitting = true
				// Trigger context cancellation
				if m.cancelFunc != nil {
					m.cancelFunc()
				}
			}
			return m, tea.Quit
		}
	case tea.WindowSizeMsg:
		m.width = msg.Width
		m.height = msg.Height
		m.ready = true
	case progressUpdateMsg:
		return m, tickCmd()
	case logUpdateMsg:
		// just trigger re-render
	}
	return m, nil
}

func (m *teaProgram) View() string {
	if !m.ready {
		return "Initializing..."
	}

	if m.quitting {
		return m.styles.info.Render("\n  Stopping gracefully... Please wait.\n\n")
	}

	// Calculate dimensions
	contentWidth := m.width - 4
	if contentWidth < 40 {
		contentWidth = 40
	}

	// Header
	header := m.styles.header.Render("🔄 USB Backuper - Intelligent Backup")

	// Progress section
	done := atomic.LoadInt64(&m.done)
	total := m.total
	percent := 0.0
	if total > 0 {
		percent = float64(done) * 100.0 / float64(total)
	}

	// Calculate speed
	elapsed := time.Since(m.start).Seconds()
	speed := float64(0)
	if elapsed > 0.1 {
		speed = float64(done) / elapsed
	}
	remaining := total - done
	eta := "--:--:--"
	if speed > 1 && remaining > 0 {
		eta = formatETA(float64(remaining) / speed)
	}

	// Progress bar
	barWidth := contentWidth - 12
	if barWidth < 20 {
		barWidth = 20
	}
	filled := int((percent / 100.0) * float64(barWidth))
	if filled > barWidth {
		filled = barWidth
	}

	// Color-coded progress
	barColor := "#00FF87" // green
	if percent < 33 {
		barColor = "#FF5555" // red
	} else if percent < 66 {
		barColor = "#FFD700" // yellow
	}

	filledBar := lipgloss.NewStyle().Foreground(lipgloss.Color(barColor)).Render(strings.Repeat("█", filled))
	emptyBar := m.styles.dim.Render(strings.Repeat("░", barWidth-filled))
	progressBar := fmt.Sprintf("[%s%s] %5.1f%%", filledBar, emptyBar, percent)

	// Stats
	stats := fmt.Sprintf(
		"Transferred: %s / %s\n"+
			"Speed:       %s/s\n"+
			"Elapsed:     %s\n"+
			"ETA:         %s",
		humanSize(done), humanSize(total),
		humanSize(int64(speed)),
		formatETA(elapsed),
		eta,
	)

	progressContent := progressBar + "\n\n" + m.styles.info.Render(stats)
	progressBox := m.styles.box.Width(contentWidth).Render(progressContent)

	// Activity log section
	logHeight := m.height - 18
	if logHeight < 3 {
		logHeight = 3
	}
	if logHeight > 15 {
		logHeight = 15
	}

	logContent := ""
	start := 0
	if len(m.logs) > logHeight {
		start = len(m.logs) - logHeight
	}
	for i := start; i < len(m.logs); i++ {
		line := m.logs[i]
		if len(line) > contentWidth-4 {
			line = line[:contentWidth-7] + "..."
		}
		logContent += m.styles.log.Render(line) + "\n"
	}
	if logContent == "" {
		logContent = m.styles.dim.Render("No activity yet...")
	}

	logTitle := m.styles.dim.Render("Activity Log")
	logBox := m.styles.box.Width(contentWidth).Render(logTitle + "\n" + logContent)

	// Help text
	help := m.styles.help.Render("Press 'q' or Ctrl+C to stop gracefully")

	return lipgloss.JoinVertical(lipgloss.Left,
		"",
		header,
		"",
		progressBox,
		"",
		logBox,
		"",
		help,
		"",
	)
}

// Compatibility methods used by the rest of the code
func (t *TUI) DrawStatic() {
	// No-op: Bubble Tea will render on its own
}

func (t *TUI) DrawTop(agg *progressAgg) {
	// Update model counters
	if t == nil || t.model == nil {
		return
	}
	atomic.StoreInt64(&t.model.done, agg.Done())
	t.model.total = agg.total
	// Trigger re-render
	if t.prog != nil {
		t.prog.Send(progressUpdateMsg{})
	}
}

func (t *TUI) AppendLog(line string) {
	if t == nil {
		return
	}
	select {
	case t.logsCh <- line:
	default:
	}
}

func (t *TUI) DrawLogs() {
	// no-op; Bubble Tea renders logs
}

func (t *TUI) Close() {
	// ensure we only close once
	if t == nil {
		return
	}
	t.closeOnce.Do(func() {
		// signal goroutines to stop
		close(t.quitCh)
		// ask Bubble Tea program to quit if present
		if t.prog != nil {
			t.prog.Quit()
		}
		// leave alt screen
		fmt.Print("\x1b[?25h\x1b[2J\x1b[H\x1b[?1049l")
	})
}

// Cross-platform terminal size detection
func termSize() (int, int) {
	w, h := getTerminalSize()
	if w < 40 {
		w = 80
	}
	if h < 10 {
		h = 24
	}
	return w, h
}

func getTerminalSize() (int, int) {
	if runtime.GOOS == "windows" {
		return getWindowsTermSize()
	}
	return getUnixTermSize()
}

func getWindowsTermSize() (int, int) {
	// Try PowerShell method first
	if w, h := getPowerShellTermSize(); w > 0 && h > 0 {
		return w, h
	}

	// Try environment variables
	if w, h := getEnvTermSize(); w > 0 && h > 0 {
		return w, h
	}

	// Windows console API fallback
	if w, h := getWindowsConsoleSize(); w > 0 && h > 0 {
		return w, h
	}

	return 120, 30 // Windows default
}

func getUnixTermSize() (int, int) {
	// Try stty command
	if w, h := getSttyTermSize(); w > 0 && h > 0 {
		return w, h
	}

	// Try environment variables
	if w, h := getEnvTermSize(); w > 0 && h > 0 {
		return w, h
	}

	return 80, 24 // Unix default
}

func getPowerShellTermSize() (int, int) {
	cmd := exec.Command("powershell", "-Command", "$Host.UI.RawUI.WindowSize.Width; $Host.UI.RawUI.WindowSize.Height")
	output, err := cmd.Output()
	if err != nil {
		return 0, 0
	}

	lines := strings.Split(strings.TrimSpace(string(output)), "\n")
	if len(lines) >= 2 {
		w, err1 := strconv.Atoi(strings.TrimSpace(lines[0]))
		h, err2 := strconv.Atoi(strings.TrimSpace(lines[1]))
		if err1 == nil && err2 == nil {
			return w, h
		}
	}
	return 0, 0
}

func getSttyTermSize() (int, int) {
	cmd := exec.Command("stty", "size")
	cmd.Stdin = os.Stdin
	output, err := cmd.Output()
	if err != nil {
		return 0, 0
	}

	parts := strings.Fields(strings.TrimSpace(string(output)))
	if len(parts) == 2 {
		h, err1 := strconv.Atoi(parts[0])
		w, err2 := strconv.Atoi(parts[1])
		if err1 == nil && err2 == nil {
			return w, h
		}
	}
	return 0, 0
}

func getEnvTermSize() (int, int) {
	w, h := 0, 0
	if v := os.Getenv("COLUMNS"); v != "" {
		if n, err := strconv.Atoi(v); err == nil && n > 20 {
			w = n
		}
	}
	if v := os.Getenv("LINES"); v != "" {
		if n, err := strconv.Atoi(v); err == nil && n > 10 {
			h = n
		}
	}
	return w, h
}

func getWindowsConsoleSize() (int, int) {
	// This is a simplified approach - in a full implementation you'd use Windows Console API
	return 0, 0
}

func expandPath(p string) string {
	if strings.HasPrefix(p, "~") {
		if h, err := os.UserHomeDir(); err == nil {
			return filepath.Join(h, strings.TrimPrefix(p, "~"))
		}
	}
	return p
}

func mustNoErr(err error) {
	if err != nil {
		fail(err)
	}
}
func fail(err error) { fmt.Fprintln(os.Stderr, err); os.Exit(1) }
