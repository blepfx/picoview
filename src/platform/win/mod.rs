mod event_loop;

use self::event_loop::{EventLoop, SharedData};
use crate::{
    platform::{PlatformCommand, PlatformWindow},
    Error, Event, EventResponse, Options,
};
use raw_window_handle::{RawDisplayHandle, RawWindowHandle, WindowsDisplayHandle};
use std::sync::Arc;

#[derive(Clone)]
pub struct Window {
    shared: Arc<SharedData>,
}

impl PlatformWindow for Window {
    fn open(
        options: Options,
        handler: Box<dyn FnMut(&Self, Event) -> EventResponse + Send>,
    ) -> Result<Self, Error> {
        EventLoop::open(options, handler)
    }

    fn post(&self, command: PlatformCommand) {
        self.shared.post(command);
    }

    fn raw_window_handle(&self) -> RawWindowHandle {
        RawWindowHandle::Win32(self.shared.handle())
    }

    fn raw_display_handle(&self) -> RawDisplayHandle {
        RawDisplayHandle::Windows(WindowsDisplayHandle::new())
    }
}
