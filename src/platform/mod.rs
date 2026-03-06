#[cfg(target_os = "linux")]
pub mod x11;

#[cfg(target_os = "linux")]
pub use x11::*;

#[cfg(target_os = "windows")]
pub mod win;
#[cfg(target_os = "windows")]
pub use win::*;

#[cfg(target_os = "macos")]
pub mod mac;
#[cfg(target_os = "macos")]
pub use mac::*;

use crate::{
    Exchange, MakeCurrentError, MouseCursor, Point, Size, SwapBuffersError, WakeupError,
    WindowWaker, rwh_06,
};
use std::ffi::{CStr, c_void};

#[derive(Clone, Copy)]
pub enum OpenMode {
    Blocking,
    Embedded(rwh_06::RawWindowHandle),
    Transient(rwh_06::RawWindowHandle),
}

impl OpenMode {
    #[allow(dead_code)]
    pub fn handle(&self) -> Option<rwh_06::RawWindowHandle> {
        match self {
            OpenMode::Blocking => None,
            OpenMode::Embedded(handle) => Some(*handle),
            OpenMode::Transient(handle) => Some(*handle),
        }
    }
}

pub trait PlatformWindow /* : !Send + !Sync */ {
    fn window_handle(&self) -> rwh_06::RawWindowHandle;
    fn display_handle(&self) -> rwh_06::RawDisplayHandle;

    fn close(&self);
    fn waker(&self) -> WindowWaker;

    fn set_title(&self, title: &str);
    fn set_cursor_icon(&self, icon: MouseCursor);
    fn set_cursor_position(&self, pos: Point);
    fn set_size(&self, size: Size);
    fn set_position(&self, pos: Point);
    fn set_visible(&self, visible: bool);

    fn open_url(&self, url: &str) -> bool;

    fn get_clipboard(&self) -> Exchange;
    fn set_clipboard(&self, data: Exchange) -> bool;

    fn is_opengl_supported(&self) -> bool;
    fn opengl_swap_buffers(&self) -> Result<(), SwapBuffersError>;
    fn opengl_make_current(&self, current: bool) -> Result<(), MakeCurrentError>;
    fn opengl_get_proc_address(&self, name: &CStr) -> *const c_void;
}

pub trait PlatformWaker: Send + Sync + 'static {
    fn wakeup(&self) -> Result<(), WakeupError>;
}

impl PlatformWaker for () {
    fn wakeup(&self) -> Result<(), WakeupError> {
        Err(WakeupError)
    }
}
