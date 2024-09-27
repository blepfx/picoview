use crate::{
    platform::{self, PlatformCommand, PlatformWindow},
    MouseButton, MouseCursor, Point, RawWindowHandle, Size,
};
use std::path::PathBuf;

#[derive(Debug)]
pub enum Event<'a> {
    WindowFocus,
    WindowBlur,
    WindowClose,

    MouseMove(Option<Point>),
    MouseDown(MouseButton),
    MouseUp(MouseButton),
    MouseScroll { x: f32, y: f32 },

    KeyInput(&'a str),
    KeyDown(),
    KeyUp(),

    Frame,

    DragHover { files: &'a [PathBuf] },
    DragAccept { files: &'a [PathBuf] },
    DragCancel,
}

#[derive(Clone, Copy, Debug)]
pub enum EventResponse {
    Ignored,
    Captured,
    AcceptDrop(DropOperation),
}

#[derive(Clone, Copy, Debug)]
pub enum DropOperation {
    None,
    Copy,
    Move,
    Link,
}

#[derive(Clone, Debug)]
pub struct Options {
    pub parent: Option<RawWindowHandle>,
}

#[derive(Debug)]
pub enum Error {
    PlatformError(String),
}

#[derive(Clone)]
#[repr(transparent)]
pub struct Window(platform::Window);

impl Window {
    fn from_ref(inner: &platform::Window) -> &Self {
        unsafe { std::mem::transmute(inner) }
    }

    pub fn open(
        options: Options,
        mut handler: impl FnMut(&Self, Event) -> EventResponse + Send + 'static,
    ) -> Result<Self, Error> {
        platform::Window::open(
            options,
            Box::new(move |window, event| handler(Self::from_ref(window), event)),
        )
        .map(Self)
    }

    pub fn set_cursor_icon(&self, cursor: MouseCursor) {
        self.0.post(PlatformCommand::SetCursorIcon(cursor));
    }

    pub fn set_cursor_position(&self, position: Point) {
        self.0.post(PlatformCommand::SetCursorPosition(position));
    }

    pub fn set_position(&self, position: Point) {
        self.0.post(PlatformCommand::SetPosition(position));
    }

    pub fn set_size(&self, size: Size) {
        self.0.post(PlatformCommand::SetSize(size));
    }

    pub fn set_title(&self, title: String) {
        self.0.post(PlatformCommand::SetTitle(title));
    }

    pub fn set_visible(&self, visible: bool) {
        self.0.post(PlatformCommand::SetVisible(visible));
    }

    pub fn set_keyboard_input(&self, request: bool) {
        self.0.post(PlatformCommand::SetKeyboardInput(request));
    }

    pub fn close(&self) {
        self.0.post(PlatformCommand::Close);
    }

    pub fn raw_handle(&self) -> RawWindowHandle {
        self.0.raw_handle()
    }
}
