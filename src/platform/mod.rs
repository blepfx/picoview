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

use crate::{MouseCursor, Point, Size, rwh_06};

#[derive(Clone, Copy)]
pub enum OpenMode {
    Blocking,
    Embedded(rwh_06::RawWindowHandle),
}

pub trait OsWindow {
    fn window_handle(&self) -> rwh_06::RawWindowHandle;
    fn display_handle(&self) -> rwh_06::RawDisplayHandle;

    fn close(&mut self);

    fn set_title(&mut self, title: &str);
    fn set_cursor_icon(&mut self, icon: MouseCursor);
    fn set_cursor_position(&mut self, pos: Point);
    fn set_size(&mut self, size: Size);
    fn set_position(&mut self, pos: Point);
    fn set_visible(&mut self, visible: bool);
    fn set_keyboard_input(&mut self, focus: bool);

    fn open_url(&mut self, url: &str) -> bool;

    fn get_clipboard_text(&mut self) -> Option<String>;
    fn set_clipboard_text(&mut self, text: &str) -> bool;
}
