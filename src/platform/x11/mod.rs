mod connection;
mod gl;
mod util;
mod window;

pub fn open_window(options: crate::WindowBuilder) -> Result<(), crate::Error> {
    window::OsWindow::open(options)
}
