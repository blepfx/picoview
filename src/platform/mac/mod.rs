mod display;
mod util;
mod view;

pub unsafe fn open_window(
    options: crate::WindowBuilder,
    mode: super::OpenMode,
) -> Result<crate::WindowWaker, crate::Error> {
    unsafe { view::WindowView::open(options, mode) }
}
