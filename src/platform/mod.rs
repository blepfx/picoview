cfg_select! {
    target_os = "linux" => {
        pub mod x11;
        pub use x11::*;
    },

    target_os = "windows" => {
        pub mod win;
        pub use win::*;
    },

    target_os = "macos" => {
        pub mod mac;
        pub use mac::*;
    },

    _ => {
        pub mod none;
        pub use none::*;
    },
}

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

unsafe impl Send for OpenMode {}

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
    fn opengl(&self) -> Option<&dyn PlatformOpenGl>;

    fn set_title(&self, title: &str);
    fn set_cursor_icon(&self, icon: MouseCursor);
    fn set_cursor_position(&self, pos: Point);
    fn set_size(&self, size: Size);
    fn set_position(&self, pos: Point);
    fn set_visible(&self, visible: bool);

    fn open_url(&self, url: &str) -> bool;

    fn get_clipboard(&self) -> Exchange;
    fn set_clipboard(&self, data: Exchange) -> bool;
}

pub trait PlatformOpenGl {
    fn swap_buffers(&self) -> Result<(), SwapBuffersError>;
    fn make_current(&self, current: bool) -> Result<(), MakeCurrentError>;
    fn get_proc_address(&self, name: &CStr) -> *const c_void;
}

pub trait PlatformWaker: Send + Sync + 'static {
    fn wakeup(&self) -> Result<(), WakeupError>;
}

impl PlatformWaker for () {
    fn wakeup(&self) -> Result<(), WakeupError> {
        Err(WakeupError)
    }
}
