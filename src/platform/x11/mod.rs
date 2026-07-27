mod gl;
mod util;
mod window;

pub unsafe fn open_window(
    options: crate::WindowBuilder<'_>,
    mode: super::OpenMode,
) -> Result<(), crate::WindowError> {
    unsafe { window::WindowImpl::open(options, mode) }
}
