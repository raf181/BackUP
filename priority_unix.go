//go:build !windows

package main

// elevatePriority is a no-op on non-Windows platforms (could adjust nice later).
func elevatePriority() {}
