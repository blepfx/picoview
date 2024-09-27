mod cursor;
mod event_loop;
mod keyboard;

use crate::{
    platform::{PlatformCommand, PlatformWindow},
    Error, Event, EventResponse, Options,
};
use raw_window_handle::{RawDisplayHandle, RawWindowHandle, XlibDisplayHandle, XlibWindowHandle};
use std::{sync::mpsc::Sender, thread};

#[derive(Clone)]
pub struct Window {
    window: XlibWindowHandle,
    display: XlibDisplayHandle,
    commands: Sender<PlatformCommand>,
}

unsafe impl Send for Window {}
unsafe impl Sync for Window {}

impl PlatformWindow for Window {
    fn open(
        options: Options,
        handler: Box<dyn FnMut(&Self, Event) -> EventResponse + Send>,
    ) -> Result<Self, Error> {
        let mut event_loop = event_loop::EventLoop::new(options, handler)?;
        let window = event_loop.window();
        thread::spawn(move || event_loop.run());

        Ok(window)
    }

    fn raw_window_handle(&self) -> RawWindowHandle {
        RawWindowHandle::Xlib(self.window)
    }

    fn raw_display_handle(&self) -> RawDisplayHandle {
        RawDisplayHandle::Xlib(self.display)
    }

    fn post(&self, command: PlatformCommand) {
        let _ = self.commands.send(command);
    }
}
