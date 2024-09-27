mod cursor;
mod event_loop;

use crate::{
    platform::{PlatformCommand, PlatformWindow},
    Error, Event, EventResponse, Options, RawWindowHandle,
};
use raw_window_handle::XlibWindowHandle;
use std::{sync::mpsc::Sender, thread};

#[derive(Clone)]
pub struct Window {
    handle: XlibWindowHandle,
    commands: Sender<PlatformCommand>,
}

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

    fn raw_handle(&self) -> RawWindowHandle {
        RawWindowHandle::Xlib(self.handle)
    }

    fn post(&self, command: PlatformCommand) {
        let _ = self.commands.send(command);
    }
}
