mod data;
mod platform;

pub use data::*;
pub use raw_window_handle;

#[repr(transparent)]
pub struct Window(platform::Window);

impl Window {
    pub fn open(options: Options) -> Result<Self, Error> {
        platform::PlatformWindow::open(options).map(Self)
    }

    pub fn post(&self, command: Command) {
        platform::PlatformWindow::post(&self.0, command)
    }

    pub fn raw_window_handle(&self) -> raw_window_handle::RawWindowHandle {
        platform::PlatformWindow::raw_window_handle(&self.0)
    }

    pub fn raw_display_handle(&self) -> raw_window_handle::RawDisplayHandle {
        platform::PlatformWindow::raw_display_handle(&self.0)
    }
}
