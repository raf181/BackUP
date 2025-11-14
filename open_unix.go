//go:build linux

package main

import (
	"io/fs"
	"os"

	"golang.org/x/sys/unix"
)

// openFileSequentialRead opens a file and hints the kernel for sequential access.
func openFileSequentialRead(path string) (*os.File, error) {
	f, err := os.Open(path)
	if err != nil {
		return nil, err
	}
	// Best-effort hints; ignore errors if not supported
	fd := int(f.Fd())
	_ = unix.Fadvise(fd, 0, 0, unix.FADV_SEQUENTIAL)
	_ = unix.Fadvise(fd, 0, 0, unix.FADV_WILLNEED)
	return f, nil
}

// openFileSequentialWrite opens a destination file and hints sequential writes.
func openFileSequentialWrite(path string, perm fs.FileMode) (*os.File, error) {
	f, err := os.OpenFile(path, os.O_CREATE|os.O_WRONLY|os.O_TRUNC, perm)
	if err != nil {
		return nil, err
	}
	fd := int(f.Fd())
	_ = unix.Fadvise(fd, 0, 0, unix.FADV_SEQUENTIAL)
	return f, nil
}
