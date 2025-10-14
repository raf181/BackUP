// +build !windows

package main

import "syscall"

func getUnixFreeSpace(path string, reserve int64) int64 {
	// For Unix systems, use the statvfs system call
	var stat syscall.Statfs_t
	err := syscall.Statfs(path, &stat)
	if err != nil {
		return 0
	}

	// Calculate free space
	free := int64(stat.Bavail) * int64(stat.Bsize)
	free -= reserve
	if free < 0 {
		free = 0
	}
	return free
}

// getWindowsFreeSpace is a stub on non-Windows platforms to satisfy references.
func getWindowsFreeSpace(path string, reserve int64) int64 {
	return 0
}