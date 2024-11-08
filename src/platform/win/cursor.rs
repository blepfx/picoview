use crate::MouseCursor;
use std::ptr::null_mut;
use windows::{
    core::PCWSTR,
    Win32::{
        Foundation::HINSTANCE,
        UI::WindowsAndMessaging::{
            LoadCursorW, HCURSOR, IDC_ARROW, IDC_CROSS, IDC_HAND, IDC_HELP, IDC_IBEAM, IDC_NO,
            IDC_SIZEALL, IDC_SIZENESW, IDC_SIZENS, IDC_SIZENWSE, IDC_SIZEWE, IDC_WAIT,
        },
    },
};

pub struct CursorCache {
    arrow: HCURSOR,
    cross: HCURSOR,
    hand: HCURSOR,
    help: HCURSOR,
    ibeam: HCURSOR,
    no: HCURSOR,
    size_all: HCURSOR,
    size_ns: HCURSOR,
    size_ew: HCURSOR,
    size_nesw: HCURSOR,
    size_nwse: HCURSOR,
    wait: HCURSOR,
}

impl CursorCache {
    pub fn load() -> Self {
        Self {
            arrow: Self::load_cursor(IDC_ARROW),
            cross: Self::load_cursor(IDC_CROSS),
            hand: Self::load_cursor(IDC_HAND),
            help: Self::load_cursor(IDC_HELP),
            ibeam: Self::load_cursor(IDC_IBEAM),
            size_all: Self::load_cursor(IDC_SIZEALL),
            no: Self::load_cursor(IDC_NO),
            size_ns: Self::load_cursor(IDC_SIZENS),
            size_ew: Self::load_cursor(IDC_SIZEWE),
            size_nesw: Self::load_cursor(IDC_SIZENESW),
            size_nwse: Self::load_cursor(IDC_SIZENWSE),
            wait: Self::load_cursor(IDC_WAIT),
        }
    }

    fn load_cursor(name: PCWSTR) -> HCURSOR {
        let handle = unsafe { LoadCursorW(HINSTANCE(null_mut()), name).unwrap() };

        HCURSOR(handle.0)
    }

    pub fn get(&self, cursor: MouseCursor) -> Option<HCURSOR> {
        match cursor {
            MouseCursor::Default => Some(self.arrow),
            MouseCursor::Help => Some(self.help),
            MouseCursor::Cell => Some(self.cross),
            MouseCursor::Crosshair => Some(self.cross),
            MouseCursor::Text => Some(self.ibeam),
            MouseCursor::VerticalText => Some(self.ibeam), // TODO
            MouseCursor::Alias => Some(self.arrow),        // TODO
            MouseCursor::Copy => Some(self.arrow),         // TODO
            MouseCursor::Move => Some(self.size_all),
            MouseCursor::PtrNotAllowed => Some(self.no),
            MouseCursor::NotAllowed => Some(self.no),
            MouseCursor::EResize => Some(self.size_ew),
            MouseCursor::NResize => Some(self.size_ns),
            MouseCursor::NeResize => Some(self.size_nesw),
            MouseCursor::NwResize => Some(self.size_nwse),
            MouseCursor::SResize => Some(self.size_ns),
            MouseCursor::SeResize => Some(self.size_nwse),
            MouseCursor::SwResize => Some(self.size_nesw),
            MouseCursor::WResize => Some(self.size_ew),
            MouseCursor::EwResize => Some(self.size_ew),
            MouseCursor::NsResize => Some(self.size_ns),
            MouseCursor::NeswResize => Some(self.size_nesw),
            MouseCursor::NwseResize => Some(self.size_nwse),
            MouseCursor::ColResize => Some(self.size_ew), // TODO
            MouseCursor::RowResize => Some(self.size_ns), // TODO
            MouseCursor::AllScroll => Some(self.size_all),
            MouseCursor::ZoomIn => Some(self.size_all), // TODO
            MouseCursor::ZoomOut => Some(self.size_all), // TODO
            MouseCursor::Hand => Some(self.hand),
            MouseCursor::HandGrabbing => Some(self.size_all),
            MouseCursor::Working => Some(self.wait),
            MouseCursor::PtrWorking => Some(self.wait),
            MouseCursor::Hidden => None,
        }
    }
}
