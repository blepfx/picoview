use crate::{Error, Event, EventResponse, MouseCursor, Options, Point, RawWindowHandle, Size};

#[cfg(target_os = "linux")]
pub mod x11;
#[cfg(target_os = "linux")]
pub use x11::Window;

pub trait PlatformWindow: Send + Sync + Clone + Sized {
    fn open(
        options: Options,
        handler: Box<dyn FnMut(&Self, Event) -> EventResponse + Send>,
    ) -> Result<Self, Error>;
    fn post(&self, command: PlatformCommand);
    fn raw_handle(&self) -> RawWindowHandle;
}

pub enum PlatformCommand {
    SetCursorIcon(MouseCursor),
    SetCursorPosition(Point),
    SetSize(Size),
    SetTitle(String),
    SetPosition(Point),
    SetVisible(bool),
    SetKeyboardInput(bool),
    Close,
}
