mod display;
mod util;
mod view;

pub unsafe fn open_window(options: crate::WindowBuilder) -> Result<(), crate::Error> {
    unsafe { view::OsWindowView::open(options) }
}
