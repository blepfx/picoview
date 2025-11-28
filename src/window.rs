use crate::{Error, Event, GlConfig, MouseCursor, Point, Size, platform, rwh_06};
use std::{fmt::Debug, ops::Range};

// the reason this is a box is because making this with traits is extremely annoying,
// especially when closures are involved
// https://github.com/rust-lang/rust/issues/70263
//
// performance overhead of dynamic dispatch is extremely low anyway
pub type WindowFactory =
    Box<dyn for<'a> FnOnce(Window<'a>) -> Box<dyn FnMut(Event) + 'a> + Send + 'static>;

#[non_exhaustive]
pub struct WindowBuilder {
    pub visible: bool,
    pub decorations: bool,

    pub title: String,

    pub size: Size,
    pub resizable: Option<Range<Size>>,

    pub position: Option<Point>,
    pub opengl: Option<GlConfig>,

    pub factory: WindowFactory,
}

impl WindowBuilder {
    pub fn new(
        factory: impl for<'a> FnOnce(Window<'a>) -> Box<dyn FnMut(Event) + 'a> + Send + 'static,
    ) -> Self {
        Self {
            visible: true,
            decorations: true,
            title: String::new(),

            resizable: None,
            size: Size {
                width: 200,
                height: 200,
            },

            position: None,
            opengl: None,
            factory: Box::new(factory),
        }
    }

    pub fn with_decorations(self, decorations: bool) -> Self {
        Self {
            decorations,
            ..self
        }
    }

    pub fn with_visible(self, visible: bool) -> Self {
        Self { visible, ..self }
    }

    pub fn with_title(self, title: impl ToString) -> Self {
        Self {
            title: title.to_string(),
            ..self
        }
    }

    pub fn with_size(self, size: impl Into<Size>) -> Self {
        Self {
            size: size.into(),
            ..self
        }
    }

    pub fn with_resizable(self, min: impl Into<Size>, max: impl Into<Size>) -> Self {
        Self {
            resizable: Some(min.into()..max.into()),
            ..self
        }
    }

    pub fn with_position(self, position: impl Into<Point>) -> Self {
        Self {
            position: Some(position.into()),
            ..self
        }
    }

    pub fn with_opengl(self, opengl: GlConfig) -> Self {
        Self {
            opengl: Some(opengl),
            ..self
        }
    }

    pub fn open_blocking(self) -> Result<(), Error> {
        unsafe { platform::open_window(self, platform::OpenMode::Blocking) }
    }

    pub fn open_parented(self, parent: impl rwh_06::HasWindowHandle) -> Result<(), Error> {
        let handle = parent
            .window_handle()
            .map_err(|_| Error::InvalidParent)?
            .as_raw();

        unsafe { platform::open_window(self, platform::OpenMode::Embedded(handle)) }
    }
}

#[repr(transparent)]
#[derive(Clone, Copy)]
pub struct Window<'a>(pub(crate) &'a dyn platform::OsWindow);

impl<'a> Window<'a> {
    pub fn close(&self) {
        self.0.close();
    }

    pub fn set_title(&self, title: &str) {
        self.0.set_title(title);
    }

    pub fn set_cursor_icon(&self, icon: MouseCursor) {
        self.0.set_cursor_icon(icon);
    }

    pub fn set_cursor_position(&self, pos: impl Into<Point>) {
        self.0.set_cursor_position(pos.into());
    }

    pub fn set_size(&self, size: impl Into<Size>) {
        self.0.set_size(size.into());
    }

    pub fn set_position(&self, pos: impl Into<Point>) {
        self.0.set_position(pos.into());
    }

    pub fn set_visible(&self, visible: bool) {
        self.0.set_visible(visible);
    }

    pub fn open_url(&self, url: &str) -> bool {
        self.0.open_url(url)
    }

    pub fn get_clipboard_text(&self) -> Option<String> {
        self.0.get_clipboard_text()
    }

    pub fn set_clipboard_text(&self, text: &str) -> bool {
        self.0.set_clipboard_text(text)
    }
}

impl<'a> rwh_06::HasWindowHandle for Window<'a> {
    fn window_handle(&self) -> Result<rwh_06::WindowHandle<'_>, rwh_06::HandleError> {
        unsafe { Ok(rwh_06::WindowHandle::borrow_raw(self.0.window_handle())) }
    }
}

impl<'a> rwh_06::HasDisplayHandle for Window<'a> {
    fn display_handle(&self) -> Result<rwh_06::DisplayHandle<'_>, rwh_06::HandleError> {
        unsafe { Ok(rwh_06::DisplayHandle::borrow_raw(self.0.display_handle())) }
    }
}

impl<'a> Debug for Window<'a> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_tuple("Window")
            .field(&self.0.window_handle())
            .finish()
    }
}

impl Debug for WindowBuilder {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("WindowBuilder")
            .field("visible", &self.visible)
            .field("decorations", &self.decorations)
            .field("title", &self.title)
            .field("size", &self.size)
            .field("resizable", &self.resizable)
            .field("position", &self.position)
            .field("opengl", &self.opengl)
            .finish_non_exhaustive()
    }
}
