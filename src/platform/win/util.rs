/// DPI awareness management.
pub mod dpi;
/// Keyboard utilities and event capture.
pub mod keyboard;
/// Vertical synchronization thread.
pub mod vsync;
/// WGL utilities for OpenGL context creation.
pub mod wgl;
/// Window and class creation and management.
pub mod window;

use std::ffi::OsString;
use std::os::windows::ffi::OsStrExt;
use std::ptr::null_mut;
use windows_sys::Win32::Foundation::GetLastError;
use windows_sys::Win32::System::Diagnostics::Debug::{
    FORMAT_MESSAGE_ALLOCATE_BUFFER, FORMAT_MESSAGE_FROM_SYSTEM, FORMAT_MESSAGE_IGNORE_INSERTS,
    FormatMessageW,
};
use windows_sys::core::PWSTR;

pub fn to_widestring(str: &str) -> Vec<u16> {
    OsString::from(str).encode_wide().chain([0]).collect()
}

pub unsafe fn from_widestring(wide: *const u16) -> String {
    unsafe {
        let mut i = 0;
        loop {
            if wide.add(i).read() == 0 {
                let data = std::slice::from_raw_parts(wide, i);
                return String::from_utf16_lossy(data);
            }

            i += 1;
        }
    }
}

pub use clipboard::*;
pub use cursor::*;
pub use dpi::*;
pub use win32_bullshit::*;

use crate::WindowError;

mod clipboard {
    use crate::DropEffect;
    use std::ffi::{OsString, c_void};
    use std::marker::PhantomData;
    use std::os::windows::ffi::{OsStrExt, OsStringExt};
    use std::path::PathBuf;
    use std::ptr::{copy_nonoverlapping, null_mut};
    use windows_sys::Win32::Foundation::{HWND, POINT};
    use windows_sys::Win32::System::DataExchange::*;
    use windows_sys::Win32::System::Memory::*;
    use windows_sys::Win32::System::Ole::*;
    use windows_sys::Win32::UI::Shell::*;

    pub fn encode_dnd_effect(effect: DropEffect) -> u32 {
        match effect {
            DropEffect::Reject => DROPEFFECT_NONE,
            DropEffect::Copy => DROPEFFECT_COPY,
            DropEffect::Move => DROPEFFECT_MOVE,
            DropEffect::Link => DROPEFFECT_LINK,
            DropEffect::Generic => DROPEFFECT_COPY | DROPEFFECT_MOVE | DROPEFFECT_LINK,
        }
    }

    pub unsafe fn decode_hdrop(hdrop: *mut c_void) -> Vec<PathBuf> {
        unsafe {
            let num_files = DragQueryFileW(hdrop, u32::MAX, null_mut(), 0);
            (0..num_files)
                .map(|i| {
                    let len = DragQueryFileW(hdrop, i, null_mut(), 0) + 1;
                    let mut buf = vec![0u16; len as usize];
                    let len = DragQueryFileW(hdrop, i, buf.as_mut_ptr(), len);
                    let buf = buf.get(..len as usize).unwrap_or(buf.as_slice());
                    PathBuf::from(OsString::from_wide(buf))
                })
                .collect::<Vec<_>>()
        }
    }

    pub fn encode_hdrop(paths: &[PathBuf]) -> Vec<u16> {
        let mut result = Vec::new();

        unsafe {
            let dropfiles = DROPFILES {
                pFiles: std::mem::size_of::<DROPFILES>() as u32,
                pt: POINT { x: 0, y: 0 },
                fNC: 0,
                fWide: 1,
            };

            result.extend_from_slice(std::slice::from_raw_parts(
                &dropfiles as *const DROPFILES as *const u16,
                std::mem::size_of::<DROPFILES>() / 2,
            ));
        }

        for path in paths {
            result.extend(OsString::from(path).encode_wide());
            result.push(0);
        }

        result.push(0);
        result
    }

    pub struct Clipboard(PhantomData<*const ()>);

    impl Clipboard {
        pub unsafe fn open(hwnd: HWND) -> Option<Self> {
            unsafe {
                if OpenClipboard(hwnd) != 0 {
                    Some(Self(PhantomData))
                } else {
                    None
                }
            }
        }

        pub fn empty(&self) {
            unsafe {
                EmptyClipboard();
            }
        }

        pub fn set<T>(&self, format: u16, data: &[T]) {
            unsafe {
                let buf = GlobalAlloc(GMEM_MOVEABLE, std::mem::size_of_val(data));
                let buf = GlobalLock(buf) as *mut T;
                copy_nonoverlapping(data.as_ptr(), buf, data.len());
                GlobalUnlock(buf as *mut _);
                SetClipboardData(format as _, buf as *mut _);
            }
        }

        pub fn get<R>(&self, format: u16, f: impl FnOnce(*const u8) -> R) -> Option<R> {
            unsafe {
                let data = GetClipboardData(format as _);
                if !data.is_null() {
                    let data = GlobalLock(data);
                    let result = if !data.is_null() {
                        Some(f(data as *const u8))
                    } else {
                        None
                    };

                    GlobalUnlock(data as *mut _);
                    result
                } else {
                    None
                }
            }
        }
    }

    impl Drop for Clipboard {
        fn drop(&mut self) {
            unsafe {
                CloseClipboard();
            }
        }
    }
}

mod cursor {
    use crate::MouseCursor;
    use std::ptr::null_mut;
    use windows_sys::Win32::UI::WindowsAndMessaging::*;
    use windows_sys::core::PCWSTR;

    pub struct CursorCache {
        pub arrow: HCURSOR,
        pub cross: HCURSOR,
        pub hand: HCURSOR,
        pub help: HCURSOR,
        pub ibeam: HCURSOR,
        pub no: HCURSOR,
        pub app_starting: HCURSOR,
        pub wait: HCURSOR,

        pub size_all: HCURSOR,
        pub size_ns: HCURSOR,
        pub size_ew: HCURSOR,
        pub size_nesw: HCURSOR,
        pub size_nwse: HCURSOR,

        pub scroll_ns: HCURSOR,
        pub scroll_ew: HCURSOR,
        pub scroll_all: HCURSOR,
    }

    impl CursorCache {
        pub fn load() -> Self {
            fn load_cursor(name: PCWSTR) -> HCURSOR {
                unsafe { LoadCursorW(null_mut(), name) }
            }

            Self {
                arrow: load_cursor(IDC_ARROW),
                cross: load_cursor(IDC_CROSS),
                hand: load_cursor(IDC_HAND),
                help: load_cursor(IDC_HELP),
                ibeam: load_cursor(IDC_IBEAM),
                no: load_cursor(IDC_NO),
                wait: load_cursor(IDC_WAIT),
                app_starting: load_cursor(IDC_APPSTARTING),

                size_ns: load_cursor(IDC_SIZENS),
                size_ew: load_cursor(IDC_SIZEWE),
                size_nesw: load_cursor(IDC_SIZENESW),
                size_nwse: load_cursor(IDC_SIZENWSE),
                size_all: load_cursor(IDC_SIZEALL),

                // https://learn.microsoft.com/en-us/windows/win32/menurc/about-cursors
                scroll_ns: load_cursor(32652 as *const _),
                scroll_ew: load_cursor(32653 as *const _),
                scroll_all: load_cursor(32654 as *const _),
            }
        }

        pub fn get_closest(&self, cursor: MouseCursor) -> HCURSOR {
            // unfortunately winapi does not provide us with all of these,
            // so we have to improvise a bit.
            match cursor {
                MouseCursor::Default => self.arrow,
                MouseCursor::Help => self.help,
                MouseCursor::Cell => self.cross,
                MouseCursor::Crosshair => self.cross,
                MouseCursor::Text => self.ibeam,
                MouseCursor::VerticalText => self.ibeam, // fallback
                MouseCursor::Alias => self.arrow,        // fallback
                MouseCursor::Copy => self.arrow,         // fallback
                MouseCursor::Move => self.size_all,
                MouseCursor::PtrNotAllowed => self.no, // fallback
                MouseCursor::NotAllowed => self.no,
                MouseCursor::EResize => self.size_ew, // fallback
                MouseCursor::NResize => self.size_ns, // fallback
                MouseCursor::NeResize => self.size_nesw, // fallback
                MouseCursor::NwResize => self.size_nwse, // fallback
                MouseCursor::SResize => self.size_ns, // fallback
                MouseCursor::SeResize => self.size_nwse, // fallback
                MouseCursor::SwResize => self.size_nesw, // fallback
                MouseCursor::WResize => self.size_ew, // fallback
                MouseCursor::EwResize => self.size_ew,
                MouseCursor::NsResize => self.size_ns,
                MouseCursor::NeswResize => self.size_nesw,
                MouseCursor::NwseResize => self.size_nwse,
                MouseCursor::ColResize => self.scroll_ew,
                MouseCursor::RowResize => self.scroll_ns,
                MouseCursor::AllScroll => self.scroll_all,
                MouseCursor::ZoomIn => self.size_all, // fallback
                MouseCursor::ZoomOut => self.size_all, // fallback
                MouseCursor::Hand => self.hand,
                MouseCursor::HandGrabbing => self.hand, // fallback
                MouseCursor::Working => self.wait,
                MouseCursor::PtrWorking => self.app_starting,
                MouseCursor::Hidden => null_mut(),
            }
        }
    }
}

mod win32_bullshit {
    use std::ffi::CStr;
    use windows_sys::Win32::Foundation::HINSTANCE;
    use windows_sys::Win32::System::LibraryLoader::{GetProcAddress, LoadLibraryA};
    use windows_sys::Win32::System::SystemServices::IMAGE_DOS_HEADER;

    pub unsafe fn proc_address<T>(module: &CStr, function: &CStr) -> Option<T> {
        unsafe {
            let lib = LoadLibraryA(module.as_ptr() as *const _);
            if lib.is_null() {
                None
            } else {
                let proc = GetProcAddress(lib, function.as_ptr() as *const _);
                proc.map(|x| std::mem::transmute_copy(&x))
            }
        }
    }

    pub fn hinstance() -> HINSTANCE {
        unsafe extern "C" {
            unsafe static __ImageBase: IMAGE_DOS_HEADER;
        }

        unsafe { &__ImageBase as *const IMAGE_DOS_HEADER as _ }
    }
}

#[derive(Debug)]
pub struct Win32Error {
    /// The error code returned by the Windows API.
    pub code: u32,
    /// A human-readable description of the error provided by the Windows API.
    pub message: Option<String>,
    /// The context/function where the error occurred, if available.
    pub context: Option<String>,
}

impl Win32Error {
    /// Creates a new error by querying the last error from the Windows API.
    pub fn last_error() -> Self {
        unsafe {
            let code = GetLastError();
            let mut buffer = null_mut::<u16>();
            let chars = FormatMessageW(
                FORMAT_MESSAGE_ALLOCATE_BUFFER
                    | FORMAT_MESSAGE_FROM_SYSTEM
                    | FORMAT_MESSAGE_IGNORE_INSERTS,
                null_mut(),
                code,
                0,
                &mut buffer as *mut PWSTR as *mut _,
                0,
                null_mut(),
            );

            let message = if chars == 0 || buffer.is_null() {
                None
            } else {
                let parts = std::slice::from_raw_parts(buffer, chars as _);
                Some(String::from_utf16_lossy(parts))
            };

            Self {
                code,
                message,
                context: None,
            }
        }
    }

    /// Sets the context for the error, which can be useful for debugging.
    pub fn with_context(mut self, context: impl Into<String>) -> Self {
        self.context = Some(context.into());
        self
    }
}

impl From<Win32Error> for WindowError {
    fn from(err: Win32Error) -> Self {
        match (err.message, err.context) {
            (Some(message), Some(context)) => {
                WindowError::Platform(format!("{}: {} ({})", context, message, err.code))
            }
            (Some(message), None) => WindowError::Platform(format!("{} ({})", message, err.code)),
            (None, Some(context)) => {
                WindowError::Platform(format!("{}: error code {}", context, err.code))
            }
            (None, None) => WindowError::Platform(format!("Win32 error: code {}", err.code)),
        }
    }
}
