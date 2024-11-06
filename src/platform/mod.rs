use crate::{
    raw_window_handle::{RawDisplayHandle, RawWindowHandle},
    Command, Error, Options,
};

#[cfg(target_os = "linux")]
pub mod x11;
#[cfg(target_os = "linux")]
pub use x11::Window;

#[cfg(target_os = "windows")]
pub mod win;
#[cfg(target_os = "windows")]
pub use win::Window;

pub trait PlatformWindow: Send + Sync + Clone + Sized {
    fn open(options: Options) -> Result<Self, Error>;
    fn post(&self, command: Command);
    fn raw_window_handle(&self) -> RawWindowHandle;
    fn raw_display_handle(&self) -> RawDisplayHandle;
}
