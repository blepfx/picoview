mod connection;
mod gl;
mod util;
mod window;

pub unsafe fn open_window(options: crate::WindowBuilder) -> Result<(), crate::Error> {
    unsafe { window::OsWindow::open(options) }
}
