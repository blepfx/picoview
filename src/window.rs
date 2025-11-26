use crate::{Error, Event, GlConfig, MouseCursor, Point, Size, platform, rwh_06};
use std::ops::Range;

pub trait WindowHandler: 'static {
    fn on_event(&mut self, event: Event, window: Window);
}

impl<H: FnMut(Event, Window) + 'static> WindowHandler for H {
    fn on_event(&mut self, event: Event, window: Window) {
        (self)(event, window);
    }
}

#[non_exhaustive]
pub struct WindowBuilder {
    pub visible: bool,
    pub decorations: bool,
    pub transparent: bool,

    pub title: String,

    pub size: Size,
    pub resizable: Option<Range<Size>>,

    pub position: Option<Point>,
    pub opengl: Option<GlConfig>,

    #[allow(clippy::type_complexity)]
    pub factory: Box<dyn (FnOnce(Window) -> Box<dyn WindowHandler>) + Send>,
}

impl WindowBuilder {
    pub fn new<W: WindowHandler>(factory: impl FnOnce(Window) -> W + Send + 'static) -> Self {
        Self {
            visible: true,
            decorations: true,
            transparent: false,
            title: String::new(),

            resizable: None,
            size: Size {
                width: 200,
                height: 200,
            },

            position: None,
            opengl: None,
            factory: Box::new(|w| Box::new((factory)(w))),
        }
    }

    pub fn with_transparency(self, transparent: bool) -> Self {
        Self {
            transparent,
            ..self
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
pub struct Window<'a>(pub(crate) &'a mut dyn platform::OsWindow);

impl<'a> Window<'a> {
    pub fn close(&mut self) {
        self.0.close();
    }

    pub fn set_title(&mut self, title: &str) {
        self.0.set_title(title);
    }

    pub fn set_cursor_icon(&mut self, icon: MouseCursor) {
        self.0.set_cursor_icon(icon);
    }

    pub fn set_cursor_position(&mut self, pos: impl Into<Point>) {
        self.0.set_cursor_position(pos.into());
    }

    pub fn set_size(&mut self, size: impl Into<Size>) {
        self.0.set_size(size.into());
    }

    pub fn set_position(&mut self, pos: impl Into<Point>) {
        self.0.set_position(pos.into());
    }

    pub fn set_visible(&mut self, visible: bool) {
        self.0.set_visible(visible);
    }

    pub fn open_url(&mut self, url: &str) -> bool {
        self.0.open_url(url)
    }

    pub fn get_clipboard_text(&mut self) -> Option<String> {
        self.0.get_clipboard_text()
    }

    pub fn set_clipboard_text(&mut self, text: &str) -> bool {
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
