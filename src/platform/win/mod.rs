mod event_loop;
mod pacer;
mod util;

use self::event_loop::{EventLoop, SharedData};
use crate::{platform::PlatformWindow, Command, Error, Options};
use raw_window_handle::{RawDisplayHandle, RawWindowHandle, WindowsDisplayHandle};
use std::sync::Arc;

#[derive(Clone)]
pub struct Window {
    shared: Arc<SharedData>,
}

impl PlatformWindow for Window {
    fn open(options: Options) -> Result<Self, Error> {
        Ok(Self {
            shared: EventLoop::open(options)?,
        })
    }

    fn post(&self, command: Command) {
        self.shared.post(command);
    }

    fn raw_window_handle(&self) -> RawWindowHandle {
        RawWindowHandle::Win32(self.shared.handle())
    }

    fn raw_display_handle(&self) -> RawDisplayHandle {
        RawDisplayHandle::Windows(WindowsDisplayHandle::new())
    }
}
