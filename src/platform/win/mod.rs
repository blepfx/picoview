mod cursor;
mod pacer;
mod util;
mod window_hook;
mod window_main;

use self::window_main::{WindowHandle, WindowMain};
use crate::{platform::PlatformWindow, Command, Error, Options};
use raw_window_handle::{RawDisplayHandle, RawWindowHandle, WindowsDisplayHandle};

#[derive(Clone)]
pub struct Window(WindowHandle);

impl PlatformWindow for Window {
    fn open(options: Options) -> Result<Self, Error> {
        Ok(Self(WindowMain::open(options)?))
    }

    fn post(&self, command: Command) {
        self.0.post(command);
    }

    fn raw_window_handle(&self) -> RawWindowHandle {
        RawWindowHandle::Win32(self.0.handle())
    }

    fn raw_display_handle(&self) -> RawDisplayHandle {
        RawDisplayHandle::Windows(WindowsDisplayHandle::new())
    }
}
