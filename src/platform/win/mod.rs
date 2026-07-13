/// Drag and drop COM interface implementation.
mod dnd;
/// OpenGL context creation and management.
mod gl;
/// Various utility functions.
mod util;
/// Our main window implementation.
mod window;

pub unsafe fn open_window(
    options: crate::WindowBuilder,
    mode: super::OpenMode,
) -> Result<crate::WindowWaker, crate::WindowError> {
    unsafe { window::WindowImpl::open(options, mode) }
}
