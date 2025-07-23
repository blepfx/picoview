mod connection;
mod gl;
mod util;
mod window_hook;
mod window_main;

pub unsafe fn open_window(options: crate::WindowBuilder) -> Result<(), crate::Error> {
    unsafe { window_main::WindowMain::open(options) }
}
