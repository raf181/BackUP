//go:build windows

package main

import (
	"golang.org/x/sys/windows"
)

// elevatePriority attempts to raise the process priority to HIGH_PRIORITY_CLASS.
// Best-effort: failures are silently ignored.
func elevatePriority() {
	h, err := windows.GetCurrentProcess()
	if err != nil {
		return
	}
	// HIGH_PRIORITY_CLASS = 0x00000080
	const HIGH_PRIORITY_CLASS = 0x00000080
	_ = windows.SetPriorityClass(h, HIGH_PRIORITY_CLASS)
}
