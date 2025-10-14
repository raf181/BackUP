# USB Backuper (Go)

A fast, importance-based backuper you can run directly from this USB.

Features
- Fast scan using os.ReadDir stack traversal
- Importance tiers by file patterns (editable in importance_profile.json)
- Selection that fills the USB free space prioritizing important/smaller files
- Concurrent copy with resume (skips same-size files) and manifest log

Build (host needs Go 1.21+)

```
cd /run/media/anoam/Ultra/BackUP
go build -o backuper
```

Quick test (dry run)

```
./backuper --sources "$HOME" --dry-run
```

Common flags
- --sources "/home/user/Documents,/mnt/data" (comma-separated)
- --objective count|space
- --exclude "*/tmp/*,*/.cache/*" (comma-separated globs)
- --profile importance_profile.json
- --dest-subdir my_backup
- --dry-run
- --resume
- --workers 2
- --reserve 67108864

Safety
- Auto-excludes this USB from scanning.
- Skips symlinks and special files.
- Skips already-copied files with identical size.

Notes
- Adjust importance tiers in importance_profile.json; higher priority first.
- count: prefers smaller files within each tier to maximize count.
- space: prefers larger files within each tier to fill capacity.

TUI (interactive)
 - The project now includes an interactive TUI powered by Charm (Bubble Tea + Lipgloss).
 - If your terminal supports it, the program will open an interactive progress view automatically when running without --no-progress.
 - To avoid the TUI and run in plain-console mode, use the --no-progress flag.