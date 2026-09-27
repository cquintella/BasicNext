// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! TTY window size. Same ioctl/Win32 query the interpreter used.

/// Columns and rows of stdout when it is a terminal.
#[cfg(all(
    unix,
    any(
        target_os = "linux",
        target_os = "android",
        target_os = "macos",
        target_os = "ios",
        target_os = "freebsd",
        target_os = "dragonfly",
        target_os = "openbsd",
        target_os = "netbsd"
    )
))]
#[must_use]
#[allow(unsafe_code)] // Narrow ioctl TIOCGWINSZ; no Rust memory is retained by the kernel.
pub fn terminal_dimensions() -> Option<(i128, i128)> {
    use std::os::raw::{c_int, c_ulong};

    #[repr(C)]
    struct WinSize {
        row: u16,
        col: u16,
        xpixel: u16,
        ypixel: u16,
    }

    #[cfg(any(target_os = "linux", target_os = "android"))]
    const TIOCGWINSZ: c_ulong = 0x5413;
    #[cfg(any(
        target_os = "macos",
        target_os = "ios",
        target_os = "freebsd",
        target_os = "dragonfly",
        target_os = "openbsd",
        target_os = "netbsd"
    ))]
    const TIOCGWINSZ: c_ulong = 0x4008_7468;

    unsafe extern "C" {
        fn ioctl(fd: c_int, request: c_ulong, ...) -> c_int;
    }

    let mut size = WinSize {
        row: 0,
        col: 0,
        xpixel: 0,
        ypixel: 0,
    };
    if unsafe { ioctl(1, TIOCGWINSZ, &raw mut size) } != 0 || size.col == 0 || size.row == 0 {
        return None;
    }
    Some((i128::from(size.col), i128::from(size.row)))
}

#[cfg(all(
    unix,
    not(any(
        target_os = "linux",
        target_os = "android",
        target_os = "macos",
        target_os = "ios",
        target_os = "freebsd",
        target_os = "dragonfly",
        target_os = "openbsd",
        target_os = "netbsd"
    ))
))]
#[must_use]
pub fn terminal_dimensions() -> Option<(i128, i128)> {
    None
}

#[cfg(windows)]
#[must_use]
#[allow(unsafe_code)] // Narrow BDFL-approved Win32 terminal query; no Rust memory is retained by Windows.
pub fn terminal_dimensions() -> Option<(i128, i128)> {
    #[repr(C)]
    struct Coord {
        x: i16,
        y: i16,
    }
    #[repr(C)]
    struct SmallRect {
        left: i16,
        top: i16,
        right: i16,
        bottom: i16,
    }
    #[repr(C)]
    struct ConsoleScreenBufferInfo {
        size: Coord,
        cursor_position: Coord,
        attributes: u16,
        window: SmallRect,
        maximum_window_size: Coord,
    }
    unsafe extern "system" {
        fn GetStdHandle(standard_handle: u32) -> *mut std::ffi::c_void;
        fn GetConsoleScreenBufferInfo(
            console_output: *mut std::ffi::c_void,
            console_screen_buffer_info: *mut ConsoleScreenBufferInfo,
        ) -> i32;
    }
    const STD_OUTPUT_HANDLE: u32 = u32::MAX - 10;
    let handle = unsafe { GetStdHandle(STD_OUTPUT_HANDLE) };
    if handle.is_null() || handle as isize == -1 {
        return None;
    }
    let mut info = std::mem::MaybeUninit::<ConsoleScreenBufferInfo>::uninit();
    if unsafe { GetConsoleScreenBufferInfo(handle, info.as_mut_ptr()) } == 0 {
        return None;
    }
    let info = unsafe { info.assume_init() };
    Some((
        i128::from(info.window.right) - i128::from(info.window.left) + 1,
        i128::from(info.window.bottom) - i128::from(info.window.top) + 1,
    ))
}

#[cfg(not(any(unix, windows)))]
#[must_use]
pub fn terminal_dimensions() -> Option<(i128, i128)> {
    None
}
