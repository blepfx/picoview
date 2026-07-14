use crate::DropEffect;
use std::ffi::OsString;
use std::marker::PhantomData;
use std::os::windows::ffi::{OsStrExt, OsStringExt};
use std::path::PathBuf;
use std::ptr::{copy_nonoverlapping, null_mut};
use windows_sys::Win32::Foundation::{HWND, POINT};
use windows_sys::Win32::System::DataExchange::{
    CloseClipboard, EmptyClipboard, GetClipboardData, OpenClipboard, SetClipboardData,
};
use windows_sys::Win32::System::Memory::{
    GMEM_MOVEABLE, GlobalAlloc, GlobalLock, GlobalSize, GlobalUnlock,
};
use windows_sys::Win32::System::Ole::{
    CLIPBOARD_FORMAT, DROPEFFECT_COPY, DROPEFFECT_LINK, DROPEFFECT_MOVE, DROPEFFECT_NONE,
};
use windows_sys::Win32::UI::Shell::{DROPFILES, DragQueryFileW, HDROP};

pub struct Clipboard(PhantomData<*const ()>);

impl Clipboard {
    /// Opens the clipboard for the given window, returning a [`Clipboard`]
    /// guard.
    ///
    /// # Safety
    /// - The `hwnd` must be a valid window handle for the lifetime of the
    ///   clipboard object.
    pub unsafe fn open(hwnd: HWND) -> Option<Self> {
        unsafe {
            if OpenClipboard(hwnd) != 0 {
                Some(Self(PhantomData))
            } else {
                None
            }
        }
    }

    /// Empties the clipboard, removing all data.
    pub fn empty(&self) {
        unsafe {
            EmptyClipboard();
        }
    }

    /// Gets the clipboard data for the given format. Returns `None` if the data
    /// is not available.
    pub fn get<R>(&self, format: CLIPBOARD_FORMAT, callback: impl FnOnce(&[u8]) -> R) -> Option<R> {
        unsafe {
            let data = GetClipboardData(format as _);
            if !data.is_null() {
                let data = GlobalLock(data);
                let result = if !data.is_null() {
                    let size = GlobalSize(data as _);
                    let slice = std::slice::from_raw_parts(data as *const u8, size);
                    Some(callback(slice))
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

    /// Sets the clipboard data for the given format. Returns `false` if the
    /// data could not be set.
    ///
    /// # Safety
    /// - The data must match the specified format.
    pub unsafe fn set(&self, format: CLIPBOARD_FORMAT, data: &[u8]) -> bool {
        unsafe {
            let buf = GlobalAlloc(GMEM_MOVEABLE, std::mem::size_of_val(data));
            if buf.is_null() {
                return false;
            }

            let buf = GlobalLock(buf) as *mut u8;
            if buf.is_null() {
                return false;
            }

            copy_nonoverlapping(data.as_ptr(), buf, data.len());

            if GlobalUnlock(buf as *mut _) == 0 {
                return false;
            }

            if SetClipboardData(format as _, buf as *mut _).is_null() {
                return false;
            }

            true
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

/// Encodes a list of paths into an [`HDROP`] structure, which can be used to
/// set the clipboard data or for drag-and-drop operations.
pub fn encode_hdrop(paths: &[PathBuf]) -> Vec<u8> {
    let mut result = Vec::new();

    unsafe {
        let dropfiles = DROPFILES {
            pFiles: std::mem::size_of::<DROPFILES>() as u32,
            pt: POINT { x: 0, y: 0 },
            fNC: 0,
            fWide: 1,
        };

        // append the header
        result.extend_from_slice(std::slice::from_raw_parts(
            &dropfiles as *const DROPFILES as *const u8,
            std::mem::size_of::<DROPFILES>(),
        ));
    }

    // append the paths
    for path in paths {
        result.extend(path.as_os_str().encode_wide().flat_map(|x| x.to_ne_bytes()));
        result.extend(0u16.to_ne_bytes()); // null terminator for the path (wide char)
    }

    result.extend(0u16.to_ne_bytes()); // double null terminator for the end (wide char)
    result
}

/// Decode a [`HDROP`] object into a list of [`PathBuf`]s.
pub unsafe fn decode_hdrop(hdrop: HDROP) -> Vec<PathBuf> {
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

/// Convert from [`DropEffect`] to the corresponding Windows [`DROPEFFECT`]
/// value.
pub fn encode_drop_effect(effect: DropEffect) -> u32 {
    match effect {
        DropEffect::Reject => DROPEFFECT_NONE,
        DropEffect::Copy => DROPEFFECT_COPY,
        DropEffect::Move => DROPEFFECT_MOVE,
        DropEffect::Link => DROPEFFECT_LINK,
        DropEffect::Generic => DROPEFFECT_COPY | DROPEFFECT_MOVE | DROPEFFECT_LINK,
    }
}
