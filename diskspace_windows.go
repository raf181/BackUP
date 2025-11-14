// +build windows

package main

import (
	"fmt"
	"os"
	"os/exec"
	"path/filepath"
	"strconv"
	"strings"
	"syscall"
	"unsafe"
)

func getUnixFreeSpace(path string, reserve int64) int64 {
	// This function should not be called on Windows, but provide a fallback
	return 0
}

func getWindowsFreeSpace(path string, reserve int64) int64 {
	// Get the root path of the drive
	absPath, err := filepath.Abs(path)
	if err != nil {
		return 0
	}

	// Get volume name (like "C:")
	volume := filepath.VolumeName(absPath)
	if volume == "" {
		return 0
	}

	// Ensure we have the root path format (e.g., "C:\\")
	root := volume + string(os.PathSeparator)

	// Use Windows GetDiskFreeSpaceEx API
	free, err := getDiskFreeSpaceEx(root)
	if err != nil {
		// Fallback: try to get space info using alternative method
		if fallbackFree := getFallbackDiskSpace(root); fallbackFree > 0 {
			free = fallbackFree
		} else {
			// Last resort: return a conservative estimate
			return 1024 * 1024 * 1024 // 1GB
		}
	}

	free -= reserve
	if free < 0 {
		free = 0
	}
	return free
}

func getDiskFreeSpaceEx(rootPath string) (int64, error) {
	kernel32 := syscall.NewLazyDLL("kernel32.dll")
	getDiskFreeSpaceEx := kernel32.NewProc("GetDiskFreeSpaceExW")

	// Convert path to UTF-16
	pathPtr, err := syscall.UTF16PtrFromString(rootPath)
	if err != nil {
		return 0, err
	}

	var freeBytesAvailable, totalNumberOfBytes, totalNumberOfFreeBytes uint64

	// Call GetDiskFreeSpaceExW using unsafe pointers (necessary for Windows API)
	// This is a justified use of unsafe for platform-specific interop
	r1, _, err := getDiskFreeSpaceEx.Call(
		uintptr(unsafe.Pointer(pathPtr)),
		uintptr(unsafe.Pointer(&freeBytesAvailable)),
		uintptr(unsafe.Pointer(&totalNumberOfBytes)),
		uintptr(unsafe.Pointer(&totalNumberOfFreeBytes)),
	)

	if r1 == 0 {
		return 0, fmt.Errorf("GetDiskFreeSpaceEx failed: %w", err)
	}

	// Return free bytes available to the user (considers quotas)
	return int64(freeBytesAvailable), nil
}

func getFallbackDiskSpace(rootPath string) int64 {
	// Fallback method using PowerShell
	cmd := exec.Command("powershell", "-Command",
		fmt.Sprintf("(Get-WmiObject -Class Win32_LogicalDisk -Filter \"DeviceID='%s'\").FreeSpace",
			strings.TrimSuffix(rootPath, string(os.PathSeparator))))

	output, err := cmd.Output()
	if err != nil {
		return 0
	}

	freeSpaceStr := strings.TrimSpace(string(output))
	if freeSpace, err := strconv.ParseInt(freeSpaceStr, 10, 64); err == nil {
		return freeSpace
	}

	return 0
}