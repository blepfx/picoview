use crate::MouseCursor;
use std::ptr::null_mut;
use windows_sys::Win32::UI::WindowsAndMessaging::*;
use windows_sys::core::PCWSTR;

/// A wrapper around a Windows cursor handle.
#[derive(Default, Clone, Copy)]
pub struct WinCursor(HCURSOR);

impl WinCursor {
    /// Creates a new cursor from a shared Windows cursor.
    pub unsafe fn shared(cursor: PCWSTR) -> Self {
        unsafe { Self(LoadCursorW(null_mut(), cursor)) }
    }

    /// Sets the current cursor icon to this.
    pub fn apply(&self) {
        unsafe {
            SetCursor(self.0);
        }
    }
}

impl From<MouseCursor> for WinCursor {
    fn from(value: MouseCursor) -> Self {
        unsafe {
            match value {
                MouseCursor::Default => Self::shared(IDC_ARROW),
                MouseCursor::Hidden => Self::default(),
                MouseCursor::Hand => Self::shared(IDC_HAND),
                MouseCursor::HandGrabbing => Self::shared(IDC_HAND), // fallback
                MouseCursor::Help => Self::shared(IDC_HELP),
                MouseCursor::Text => Self::shared(IDC_IBEAM),
                MouseCursor::VerticalText => Self::shared(IDC_IBEAM), // fallback
                MouseCursor::Working => Self::shared(IDC_WAIT),
                MouseCursor::PtrWorking => Self::shared(IDC_APPSTARTING),
                MouseCursor::NotAllowed => Self::shared(IDC_NO),
                MouseCursor::PtrNotAllowed => Self::shared(IDC_NO), // fallback
                MouseCursor::ZoomIn => Self::shared(IDC_SIZEALL),   // fallback
                MouseCursor::ZoomOut => Self::shared(IDC_SIZEALL),  // fallback
                MouseCursor::Alias => Self::shared(IDC_ARROW),      // fallback
                MouseCursor::Copy => Self::shared(IDC_ARROW),       // fallback
                MouseCursor::Move => Self::shared(IDC_SIZEALL),
                MouseCursor::Cell => Self::shared(IDC_CROSS), // fallback
                MouseCursor::Crosshair => Self::shared(IDC_CROSS),

                MouseCursor::EResize => Self::shared(IDC_SIZEWE), // fallback
                MouseCursor::NResize => Self::shared(IDC_SIZENS), // fallback
                MouseCursor::NeResize => Self::shared(IDC_SIZENESW), // fallback
                MouseCursor::NwResize => Self::shared(IDC_SIZENWSE), // fallback
                MouseCursor::SResize => Self::shared(IDC_SIZENS), // fallback
                MouseCursor::SeResize => Self::shared(IDC_SIZENWSE), // fallback
                MouseCursor::SwResize => Self::shared(IDC_SIZENESW), // fallback
                MouseCursor::WResize => Self::shared(IDC_SIZEWE), // fallback
                MouseCursor::EwResize => Self::shared(IDC_SIZEWE),
                MouseCursor::NsResize => Self::shared(IDC_SIZENS),
                MouseCursor::NwseResize => Self::shared(IDC_SIZENWSE),
                MouseCursor::NeswResize => Self::shared(IDC_SIZENESW),

                // https://learn.microsoft.com/en-us/windows/win32/menurc/about-cursors
                MouseCursor::RowResize => Self::shared(32652 as *const _),
                MouseCursor::ColResize => Self::shared(32653 as *const _),
                MouseCursor::AllScroll => Self::shared(32654 as *const _),
            }
        }
    }
}
