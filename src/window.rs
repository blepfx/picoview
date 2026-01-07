use crate::{Error, Event, GlConfig, MouseCursor, Point, Size, WakeupError, platform, rwh_06};
use std::{fmt::Debug, ops::Range, sync::Arc};

// the reason this is a box is because making this with traits is extremely
// annoying, especially when closures are involved
// https://github.com/rust-lang/rust/issues/70263
//
// performance overhead of dynamic dispatch is extremely low anyway

/// A function that constructs _an event handler_ for a window.
/// Must be `Send` and `'static`.
///
/// An event handler is a boxed closure of type `FnMut(Event) + 'a`
/// where `'a` is the lifetime of the window.
pub type WindowFactory =
    Box<dyn for<'a> FnOnce(Window<'a>) -> Box<dyn FnMut(Event) + 'a> + Send + 'static>;

/// A builder for opening new windows.
#[non_exhaustive]
#[must_use = "`WindowBuilder` does nothing until you call one of the open methods"]
pub struct WindowBuilder {
    /// Whether the window is initially visible
    pub visible: bool,

    /// Whether the window has decorations (title bar, borders, etc)
    pub decorations: bool,

    /// Whether the window client area is transparent (premultiplied alpha)
    pub transparent: bool,

    /// The window title
    pub title: String,

    /// The initial window size in physical pixels
    pub size: Size,

    /// The minimum and maximum size of the window if resizable
    pub resizable: Option<Range<Size>>,

    /// The initial window position in physical pixels
    pub position: Option<Point>,

    /// Requested OpenGL configuration, if any.
    pub opengl: Option<GlConfig>,

    /// The factory function that creates the event handler for the window
    pub factory: WindowFactory,
}

/// A thread-safe handle that can be used to wake up an associated event loop.
#[derive(Clone)]
pub struct WindowWaker(pub(crate) Arc<dyn platform::PlatformWaker>);

/// A handle to an open window.
///
/// It is only valid while the window is open and only accessible from the event
/// loop of that window.
#[derive(Clone, Copy)]
pub struct Window<'a>(pub(crate) &'a dyn platform::PlatformWindow);

impl<'a> Window<'a> {
    /// Get a [`WindowWaker`] that can be used to wake up the current event loop
    /// by sending a [`Event::Wakeup`](`crate::Event::Wakeup`) event.
    pub fn waker(&self) -> WindowWaker {
        self.0.waker()
    }

    /// Close the window and exit its event loop.
    pub fn close(&self) {
        self.0.close();
    }

    /// Set the window title.
    pub fn set_title(&self, title: &str) {
        self.0.set_title(title);
    }

    /// Set the cursor icon that is shown when hovering over the window.
    pub fn set_cursor_icon(&self, icon: MouseCursor) {
        self.0.set_cursor_icon(icon);
    }

    /// Warp the mouse cursor to the given position within the window.
    ///
    /// Position is in physical pixels, with (0, 0) being the top-left corner of
    /// the client area.
    pub fn set_cursor_position(&self, pos: impl Into<Point>) {
        self.0.set_cursor_position(pos.into());
    }

    /// Set the window size (size of client area) in physical pixels.
    pub fn set_size(&self, size: impl Into<Size>) {
        self.0.set_size(size.into());
    }

    /// Set the window position (position of client area) in physical pixels
    /// relative to the origin (top-left corner) of the coordinate system.
    ///
    /// The coordinate system depends on how the window was created:
    /// - For top-level windows, it is the screen coordinate system, with (0, 0)
    ///   being the top-left corner of the primary monitor.
    /// - For embedded windows, it is the coordinate system of the parent
    ///   window, with (0, 0) being the top-left corner of the parent window's
    ///   client area.
    /// - For transient windows, it is the coordinate system of the parent
    ///   window, with (0, 0) being the top-left corner of the parent window's
    ///   client area.
    ///
    /// If not specified, the window will be centered on the screen or parent
    /// window (or positioned at (0, 0) if embedded)
    pub fn set_position(&self, pos: impl Into<Point>) {
        self.0.set_position(pos.into());
    }

    /// Set whether the window is visible.
    pub fn set_visible(&self, visible: bool) {
        self.0.set_visible(visible);
    }

    /// Open the given URL or file path in the system's default application.
    pub fn open_url(&self, url: &str) -> bool {
        self.0.open_url(url)
    }

    /// Get the current text contents of the system clipboard, if any.
    pub fn get_clipboard_text(&self) -> Option<String> {
        self.0.get_clipboard_text()
    }

    /// Set the current text contents of the system clipboard.
    ///
    /// Returns `true` on success, `false` otherwise.
    pub fn set_clipboard_text(&self, text: &str) -> bool {
        self.0.set_clipboard_text(text)
    }
}

impl WindowWaker {
    /// Wake up the associated window, emitting a
    /// [`Event::Wakeup`](`crate::Event::Wakeup`) event.
    ///
    /// Returns [`WakeupError::Disconnected`] if the window has already been
    /// closed.
    pub fn wakeup(&self) -> Result<(), WakeupError> {
        self.0.wakeup()
    }
}

impl WindowBuilder {
    /// Create a new [`WindowBuilder`] with the given event handler factory and
    /// default parameters
    pub fn new(
        factory: impl for<'a> FnOnce(Window<'a>) -> Box<dyn FnMut(Event) + 'a> + Send + 'static,
    ) -> Self {
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
            factory: Box::new(factory),
        }
    }

    /// Set whether the window has decorations (title bar, borders, etc)
    ///
    /// `true` by default
    pub fn with_decorations(self, decorations: bool) -> Self {
        Self {
            decorations,
            ..self
        }
    }

    /// Set whether the window client area is transparent (premultiplied alpha)
    ///
    /// `false` by default
    pub fn with_transparency(self, transparent: bool) -> Self {
        Self {
            transparent,
            ..self
        }
    }

    /// Set whether the window is visible upon creation
    ///
    /// `true` by default
    pub fn with_visible(self, visible: bool) -> Self {
        Self { visible, ..self }
    }

    /// Set the initial window title
    pub fn with_title(self, title: impl ToString) -> Self {
        Self {
            title: title.to_string(),
            ..self
        }
    }

    /// Set the initial window size _in physical pixels_
    pub fn with_size(self, size: impl Into<Size>) -> Self {
        Self {
            size: size.into(),
            ..self
        }
    }

    /// Set the initial window position relative to the origin.
    ///
    /// If not specified, the window will be centered on the screen or parent
    /// window (or positioned at 0, 0 if embedded)
    ///
    /// See [`Window::set_position`] for details on coordinate system.
    pub fn with_position(self, position: impl Into<Point>) -> Self {
        Self {
            position: Some(position.into()),
            ..self
        }
    }

    /// Set the minimum and maximum resizable size of the window
    ///
    /// If not set, the window will not be resizable by the user and can only be
    /// resized via [`Window::set_size`].
    pub fn with_resizable(self, min: impl Into<Size>, max: impl Into<Size>) -> Self {
        Self {
            resizable: Some(min.into()..max.into()),
            ..self
        }
    }

    /// Request an OpenGL context with the given configuration
    pub fn with_opengl(self, opengl: GlConfig) -> Self {
        Self {
            opengl: Some(opengl),
            ..self
        }
    }

    /// Open a top-level window. Blocks until the window is closed.
    ///
    /// Returns `Err` if the window could not be created or if an error occurred
    /// during the lifetime of the window.
    pub fn open_blocking(self) -> Result<(), Error> {
        unsafe { platform::open_window(self, platform::OpenMode::Blocking).map(|_| ()) }
    }

    /// Open a transient window attached to the given parent window. Unlike
    /// [`WindowBuilder::open_blocking`] this function does not block, this is
    /// achieved by hooking into the parent's OS event loop.
    ///
    /// A transient window is a window that can be moved independently of its
    /// parent window (like a popup or a dialog) and does not get clipped by it.
    /// It is always on top of its parent window and is hidden when the
    /// parent window is minimized or closed.
    ///
    /// Returns `Err` if the window could not be created or if the parent window
    /// handle is invalid, otherwise returns a [`WindowWaker`] associated with
    /// the newly created window.
    pub fn open_transient<W>(self, parent: W) -> Result<WindowWaker, Error>
    where
        W: rwh_06::HasWindowHandle,
    {
        let handle = parent
            .window_handle()
            .map_err(|_| Error::InvalidParent)?
            .as_raw();

        unsafe { platform::open_window(self, platform::OpenMode::Transient(handle)) }
    }

    /// Open an embedded window attached to the given parent window. Unlike
    /// [`WindowBuilder::open_blocking`] this function does not block, this is
    /// achieved by hooking into the parent's OS event loop.
    ///
    /// It is used for embedding a window inside another window (for example,
    /// plugins). The embedded window is clipped to the bounds of the parent
    /// window and moves with it.
    ///
    /// Returns `Err` if the window could not be created or if the parent window
    /// handle is invalid, otherwise returns a [`WindowWaker`] associated with
    /// the newly created window.
    pub fn open_embedded<W>(self, parent: W) -> Result<WindowWaker, Error>
    where
        W: rwh_06::HasWindowHandle,
    {
        let handle = parent
            .window_handle()
            .map_err(|_| Error::InvalidParent)?
            .as_raw();

        unsafe { platform::open_window(self, platform::OpenMode::Embedded(handle)) }
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

impl Debug for WindowWaker {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_tuple("WindowWaker").finish_non_exhaustive()
    }
}

impl Default for WindowWaker {
    /// Create a dummy [`WindowWaker`] that does not belong to any window.
    fn default() -> Self {
        Self(Arc::new(()))
    }
}
