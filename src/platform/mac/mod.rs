mod display;
mod util;
mod view;

use crate::{platform::PlatformOs, Error, MouseCursor, Options, Point, Size};
use objc2_app_kit::NSView;
use std::{ffi::c_void, fmt};
use view::{OsWindowCommand, OsWindowHandle, OsWindowView};

#[derive(Clone)]
pub struct Os {}

#[derive(Clone, Copy, Eq, PartialEq, Debug)]
pub struct RawHandle {
    view: *mut NSView,
}

impl RawHandle {
    pub unsafe fn from_raw(view: *mut c_void) -> Self {
        Self { view: view as _ }
    }

    pub fn view(&self) -> *mut c_void {
        self.view as _
    }
}

unsafe impl Sync for RawHandle {}
unsafe impl Send for RawHandle {}

impl PlatformOs for Os {
    type Window = OsWindowHandle;
    type Handle = RawHandle;

    fn create() -> Result<Self, Error> {
        Ok(Self {})
    }

    fn open_window(&self, options: Options) -> Result<Self::Window, Error> {
        OsWindowView::open(options)
    }

    fn close_window(&self, window: &Self::Window) {
        window.post(OsWindowCommand::Close);
    }

    fn set_window_size(&self, window: &Self::Window, size: Size) {
        window.post(OsWindowCommand::SetSize(size));
    }

    fn set_window_position(&self, window: &Self::Window, position: Point) {
        window.post(OsWindowCommand::SetPosition(position));
    }

    fn set_window_cursor_icon(&self, window: &Self::Window, cursor: MouseCursor) {
        window.post(OsWindowCommand::SetCursorIcon(cursor));
    }

    fn set_window_cursor_position(&self, window: &Self::Window, cursor: Point) {
        window.post(OsWindowCommand::SetCursorPosition(cursor));
    }

    fn set_window_visible(&self, window: &Self::Window, visible: bool) {
        window.post(OsWindowCommand::SetVisible(visible));
    }

    fn set_window_keyboard_focus(&self, window: &Self::Window, keyboard: bool) {
        window.post(OsWindowCommand::SetKeyboardInput(keyboard));
    }

    fn get_window_handle(&self, window: &Self::Window) -> RawHandle {
        window.raw_handle()
    }

    fn get_clipboard_text(&self) -> Option<String> {
        util::get_clipboard_text()
    }

    fn set_clipboard_text(&self, text: &str) -> bool {
        util::set_clipboard_text(text)
    }

    fn open_url(&self, url: &str) -> bool {
        util::spawn_detached(std::process::Command::new("/usr/bin/open").arg(url)).is_ok()
    }
}

impl fmt::Debug for Os {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Os(MacOs)")
    }
}
