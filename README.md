# 🔄 BackUP

> Fast, intelligent USB backup tool with importance-based file prioritization

[![Build & Release](https://github.com/raf181/BackUP/actions/workflows/build.yml/badge.svg)](https://github.com/raf181/BackUP/actions/workflows/build.yml)
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](LICENSE)

## Features

- ⚡ **Fast scanning** - Efficient directory traversal with concurrent file processing
- 🎯 **Smart prioritization** - Organize backups by file importance (configurable tiers)
- 💾 **Adaptive selection** - Fills USB space intelligently, prioritizing important files
- 🔄 **Resume-capable** - Skip already-copied files automatically
- 🎨 **Interactive TUI** - Beautiful, real-time progress visualization
- 🛡️ **Safe** - Auto-excludes USB device, skips symlinks, validates file integrity
- 🌍 **Cross-platform** - Linux, Windows (Go 1.21+)

## Quick Start

### Download Pre-built Binary

Get the latest release from [Releases](https://github.com/raf181/BackUP/releases)

### Basic Usage

```bash
# Preview what will be backed up
./backuper --sources "$HOME" --dry-run

# Backup your home directory
./backuper --sources "$HOME"

# Backup multiple directories
./backuper --sources "/home/user/Documents,/home/user/Pictures"
```

## Configuration

### Importance Tiers

Edit `importance_profile.json` to customize file priorities:

```json
{
  "tiers": [
    {
      "name": "Documents",
      "priority": 100,
      "patterns": ["*.pdf", "*.doc", "*.docx", "*.txt"]
    },
    {
      "name": "Photos",
      "priority": 90,
      "patterns": ["*.jpg", "*.png", "*.heic"]
    }
  ]
}
```

Higher priority files are backed up first.

## Command-line Options

```txt
-sources string
    Comma-separated source directories (default: home directory)

-objective string
    Selection strategy: count (maximize file count) or space (maximize data) (default: "count")

-exclude string
    Comma-separated glob patterns to exclude (e.g., "*/tmp/*,*/.cache/*")

-profile string
    Path to importance_profile.json (default: "importance_profile.json")

-dest-subdir string
    Create backup in USB subdirectory (auto-named if empty)

-workers int
    Concurrent copy workers (default: CPU core count)

-reserve int64
    Bytes to reserve free on USB (default: 0)

-dry-run
    Preview selection without copying

-resume
    Resume into existing destination directory

-no-progress
    Disable interactive TUI (console mode only)

-fast-ssd
    Optimize for high-speed storage

-boost
    High-performance mode (raise priority, enable fast-ssd heuristics)
```

## Examples

```bash
# Dry run to see what gets backed up
./backuper --sources "$HOME" --dry-run

# Backup with space-filling strategy (larger files first)
./backuper --sources "$HOME" --objective space

# Exclude caches and temp files
./backuper --sources "$HOME" --exclude "*/cache/*,*/tmp/*"

# Resume previous backup
./backuper --sources "$HOME" --resume --dest-subdir backup_20231115_143022

# Reserve 1 GB free space on USB
./backuper --sources "$HOME" --reserve 1073741824

# Boost mode for fast SSDs
./backuper --sources "$HOME" --boost
```

## Building from Source

Requires Go 1.21 or later:

```bash
git clone https://github.com/raf181/BackUP.git
cd BackUP
go build -ldflags="-s -w" -trimpath -o backuper
```

### Creating Optimized Binaries

```bash
# Strip debug symbols
go build -ldflags="-s -w" -trimpath -o backuper

# Further compress with UPX
upx -9 --best backuper -o backuper.compressed
```

## How It Works

1. **Scan** - Recursively scan source directories with importance tier matching
2. **Select** - Intelligently select files to maximize importance within available space
3. **Plan** - Build manifest of source→destination mappings
4. **Copy** - Concurrently copy files with progress tracking and error handling
5. **Verify** - Log manifest with timestamps and status for each file

## Safety Features

- ✅ Auto-excludes this USB from scanning
- ✅ Skips symlinks and special files
- ✅ Skips already-copied files with matching size
- ✅ Atomic operations (using `.part` temp files)
- ✅ Detailed manifest logging (`backup-manifest.jsonl`)

## License

MIT License - see LICENSE file for details

## Contributing

Contributions welcome! Feel free to open issues and submit pull requests.
