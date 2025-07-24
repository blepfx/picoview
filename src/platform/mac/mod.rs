mod display;
mod util;
mod view;

pub unsafe fn open_window(
    options: crate::WindowBuilder,
    mode: super::OpenMode,
) -> Result<(), crate::Error> {
    unsafe {
        match mode {
            super::OpenMode::Blocking => view::OsWindowView::open_blocking(options),
            super::OpenMode::Embedded(parent) => view::OsWindowView::open_embedded(options, parent),
        }
    }
}
