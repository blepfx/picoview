mod gl;
mod shared;
mod util;
mod vsync;
mod window;

pub unsafe fn open_window(
    options: crate::WindowBuilder,
    mode: super::OpenMode,
) -> Result<(), crate::Error> {
    unsafe { window::WindowImpl::open(options, mode) }
}
