mod data;
mod opengl;
mod platform;

pub use data::*;
pub use opengl::*;

pub fn open_window(options: Options) -> Result<(), Error> {
    platform::open_window(options)
}

unsafe impl Send for RawHandle {}
unsafe impl Sync for RawHandle {}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RawHandle {
    X11 { window: std::ffi::c_uint },
    Win { hwnd: *mut std::ffi::c_void },
}

impl RawHandle {
    pub fn as_x11(self) -> Option<std::ffi::c_uint> {
        match self {
            Self::X11 { window } => Some(window),
            _ => None,
        }
    }

    pub fn as_win(self) -> Option<*mut std::ffi::c_void> {
        match self {
            Self::Win { hwnd } => Some(hwnd),
            _ => None,
        }
    }
}

#[repr(transparent)]
pub struct Window(dyn platform::OsWindow);

impl Window {
    pub(crate) fn from_inner(inner: &mut dyn platform::OsWindow) -> &mut Window {
        unsafe { std::mem::transmute(inner) }
    }
}

impl Window {
    pub fn close(&mut self) {
        self.0.close();
    }

    pub fn handle(&self) -> RawHandle {
        self.0.handle()
    }

    pub fn set_title(&mut self, title: &str) {
        self.0.set_title(title);
    }

    pub fn set_cursor_icon(&mut self, icon: MouseCursor) {
        self.0.set_cursor_icon(icon);
    }

    pub fn set_cursor_position(&mut self, pos: Point) {
        self.0.set_cursor_position(pos);
    }

    pub fn set_size(&mut self, size: Size) {
        self.0.set_size(size);
    }

    pub fn set_position(&mut self, pos: Point) {
        self.0.set_position(pos);
    }

    pub fn set_visible(&mut self, visible: bool) {
        self.0.set_visible(visible);
    }

    pub fn set_keyboard_input(&mut self, focus: bool) {
        self.0.set_keyboard_input(focus);
    }

    pub fn open_url(&mut self, url: &str) -> bool {
        self.0.open_url(url)
    }

    pub fn get_clipboard_text(&mut self) -> Option<String> {
        self.0.get_clipboard_text()
    }

    pub fn set_clipboard_text(&mut self, text: &str) -> bool {
        self.0.set_clipboard_text(text)
    }
}
