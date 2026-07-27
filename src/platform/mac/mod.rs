mod display;
mod gl;
mod util;
mod view;

pub unsafe fn open_window(
    options: crate::WindowBuilder<'_>,
    mode: super::OpenMode,
) -> Result<(), crate::WindowError> {
    unsafe { view::WindowImpl::open(options, mode) }
}
