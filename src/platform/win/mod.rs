mod connection;
mod gl;
mod util;
mod window_hook;
mod window_main;

pub fn open_window(options: crate::Options) -> Result<(), crate::Error> {
    window_main::WindowMain::open(options)
}
