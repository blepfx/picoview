mod connection;
mod gl;
mod util;
mod window;

pub unsafe fn open_window(
    options: crate::WindowBuilder,
    mode: super::OpenMode,
) -> Result<(), crate::Error> {
    unsafe { window::WindowImpl::open(options, mode) }
}
