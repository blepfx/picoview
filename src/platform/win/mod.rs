mod gl;
mod shared;
mod util;
mod vsync;
mod window;

pub unsafe fn open_window(
    options: crate::WindowBuilder,
    mode: super::OpenMode,
) -> Result<crate::WindowWaker, crate::WindowError> {
    unsafe { window::WindowImpl::open(options, mode) }
}
