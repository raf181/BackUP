//go:build windows

package main

import (
    "io/fs"
    "os"
    "golang.org/x/sys/windows"
)

// openFileSequentialRead opens file with FILE_FLAG_SEQUENTIAL_SCAN for better cache behavior.
func openFileSequentialRead(path string) (*os.File, error) {
    p, err := windows.UTF16PtrFromString(path)
    if err != nil {
        return nil, err
    }
    handle, err := windows.CreateFile(
        p,
        windows.GENERIC_READ,
        windows.FILE_SHARE_READ|windows.FILE_SHARE_WRITE|windows.FILE_SHARE_DELETE,
        nil,
        windows.OPEN_EXISTING,
        windows.FILE_ATTRIBUTE_NORMAL|windows.FILE_FLAG_SEQUENTIAL_SCAN,
        0,
    )
    if err != nil {
        return nil, err
    }
    return os.NewFile(uintptr(handle), path), nil
}

// openFileSequentialWrite opens/creates destination with sequential flag.
func openFileSequentialWrite(path string, perm fs.FileMode) (*os.File, error) {
    // Ensure directory exists using os before CreateFile
    if err := os.MkdirAll(filepathDir(path), 0o755); err != nil {
        return nil, err
    }
    p, err := windows.UTF16PtrFromString(path)
    if err != nil {
        return nil, err
    }
    handle, err := windows.CreateFile(
        p,
        windows.GENERIC_WRITE|windows.GENERIC_READ,
        windows.FILE_SHARE_READ,
        nil,
        windows.CREATE_ALWAYS,
        windows.FILE_ATTRIBUTE_NORMAL|windows.FILE_FLAG_SEQUENTIAL_SCAN,
        0,
    )
    if err != nil {
        return nil, err
    }
    f := os.NewFile(uintptr(handle), path)
    // Apply permissions best-effort (Windows ignores POSIX perms, but keep parity)
    _ = f.Chmod(perm)
    return f, nil
}

// filepathDir avoids importing path/filepath here to keep imports minimal
func filepathDir(p string) string {
    // Use windows APIs to find last separator
    s := []rune(p)
    idx := -1
    for i := len(s) - 1; i >= 0; i-- {
        if s[i] == '\\' || s[i] == '/' {
            idx = i
            break
        }
    }
    if idx <= 0 {
        return "."
    }
    return string(s[:idx])
}
