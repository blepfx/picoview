use crate::*;
use std::error::Error;
use std::fmt::Debug;
use std::sync::Arc;

/// A window handler, the object that processes all incoming events for a single
/// window.
pub trait WindowHandler {
    /// A wakeup event triggered by a call to
    /// [`WindowWaker::wakeup`]
    fn wakeup(&mut self) {}

    /// Frame event. You should redraw the window in response to this event.
    ///
    /// This event is sent at the refresh rate of the display (typically 60 Hz),
    /// on a best-effort basis (might use an unsynchronized timer depending on
    /// the platform).
    fn frame(&mut self) {}

    /// User requested to close the window (by clicking the close button, or
    /// pressing Alt+F4, etc)
    ///
    /// To actually close the window, you have to call
    /// [`Window::close`].
    fn close(&mut self) {}

    /// Damage event. Request to redraw the specificed region as soon as
    /// possible.
    fn damage(&mut self, region: Rect) {
        let _ = region;
    }

    /// The window gained or lost focus.
    fn focus_changed(&mut self, focus: bool) {
        let _ = focus;
    }

    /// Size of a window has changed.
    ///
    /// The size provided is the new size of the client area in physical pixels.
    fn size_changed(&mut self, size: Size) {
        let _ = size;
    }

    /// The scale factor of a window has changed.
    ///
    /// The scale factor is the ratio of physical pixels to logical pixels.
    fn scale_changed(&mut self, scale: f64) {
        let _ = scale;
    }

    /// The position of a window has changed.
    ///
    /// The position provided is the new position of the client area in physical
    /// pixels relative to the origin (top-left corner) of the coordinate
    /// system (screen or parent window).
    fn position_changed(&mut self, position: Point) {
        let _ = position;
    }

    /// The mouse cursor left the window.
    ///
    /// Note that there is no corresponding event for when the mouse enters the
    /// window, you can track that yourself by checking for [`Self::mouse_move`]
    /// events.
    fn mouse_leave(&mut self) {}

    /// A mouse button was pressed or released at position provided by the last
    /// call to [`Self::mouse_move`]
    fn mouse_press(&mut self, button: MouseButton, pressed: bool) {
        let _ = (button, pressed);
    }

    /// The mouse cursor was moved within the window.
    fn mouse_move(&mut self, point: Point) {
        let _ = point;
    }

    /// The mouse wheel was scrolled (can also represent touchpad scrolling).
    ///
    /// `picoview` normalizes scroll events to a consistent unit across
    /// platforms.
    fn mouse_scroll(&mut self, x: f64, y: f64) {
        let _ = (x, y);
    }

    /// A rotation gesture was performed (for example, a two-finger rotation on
    /// a touchpad).
    fn gesture_rotate(&mut self, angle: f64) {
        let _ = angle;
    }

    /// A zoom gesture was performed (for example, a two-finger pinch on a
    /// touchpad).
    fn gesture_zoom(&mut self, scale: f64) {
        let _ = scale;
    }

    /// The state of the modifier keys (Shift, Ctrl, Alt, etc.) changed.
    fn key_modifiers(&mut self, modifiers: Modifiers) {
        let _ = modifiers;
    }

    /// A key was pressed or released.
    ///
    /// Return `true` if the event was handled and should not be propagated to
    /// the parent (if this window is embedded in another window)
    fn key_press(&mut self, key: Key, pressed: bool) -> bool {
        let _ = (key, pressed);
        false
    }

    /// Drag-and-drop data was dragged into the window, the position will be
    /// reported via [`Self::drag_move`] events until the drag-and-drop
    /// operation is cancelled or completed.
    ///
    /// Return `true` if the drag-and-drop operation is accepted and should
    /// continue, or `false` to reject it.
    fn drag_enter(&mut self, data: Exchange, point: Point) -> DropEffect {
        let _ = (data, point);
        DropEffect::Reject
    }

    /// Drag-and-drop data was dragged within the window
    fn drag_move(&mut self, point: Point) -> DropEffect {
        let _ = point;
        DropEffect::Reject
    }

    /// Drag-and-drop data was dragged out of the window, or the drag-and-drop
    /// operation was cancelled.
    fn drag_leave(&mut self) {}

    /// Drag-and-drop data was released into the window at the last
    /// [`Self::drag_move`] position with the data provided by the last
    /// [`Self::drag_enter`].
    fn drag_accept(&mut self) -> DropEffect {
        DropEffect::Reject
    }
}

impl WindowHandler for () {}

// the reason this is a box is because making this with traits is extremely
// annoying, especially when closures are involved
// https://github.com/rust-lang/rust/issues/70263
//
// performance overhead of dynamic dispatch is extremely low anyway

/// A function that constructs _an event handler_ for a window.
///
/// The factory must be `Send` and `'static`.
///
/// Optionally, the factory can return an error if it fails to initialize for
/// some reason. The error will be propagated to the caller as
/// [`WindowError::Factory`].
pub type WindowFactory = Box<
    dyn for<'a> FnOnce(
            Window<'a>,
        ) -> Result<Box<dyn WindowHandler + 'a>, Box<dyn Error + Send + Sync>>
        + Send
        + 'static,
>;

/// A builder for opening new windows.
///
/// By default, a window has a size of 0, invisible, resizable, decorated, not
/// transparent, and has a default position.
///
/// To set the size, position, and visibility of the window, you must call the
/// corresponding methods on the [`Window`] object once the window is created.
#[non_exhaustive]
#[must_use = "`WindowBuilder` does nothing until you call one of the open methods"]
pub struct WindowBuilder {
    /// Whether the window client area is transparent (premultiplied alpha)
    pub transparent: bool,

    /// The requested OpenGL configuration for the window, if any
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

    /// Get the OpenGL context associated with the window, if present.
    pub fn opengl(&self) -> Result<GlContext<'a>, OpenGlError> {
        self.0.opengl().map(GlContext)
    }

    /// Close the window and exit its event loop.
    pub fn close(&self) {
        self.0.close();
    }

    /// Get the current scale factor of the window, which is the ratio of
    /// physical pixels to logical pixels.
    ///
    /// For example, a scale factor of 2.0 means that 1 logical pixel is equal
    /// to 2 physical pixels.
    ///
    /// This is a hint, and it is safe to ignore it and use physical pixels
    /// instead. However, some platforms use logical pixels for everything
    /// so this might be useful for interfacing with other libraries that expect
    /// logical pixels.
    ///
    /// Another reason to use the provided scale factor is more consistent user
    /// experience between different platforms/configurations/applications.
    ///
    /// If changed, a call [`WindowHandler::window_scale`] will be emitted.
    pub fn scale(&self) -> f64 {
        self.0.scale()
    }

    /// Set the window title.
    pub fn set_title(&self, title: &str) {
        self.0.set_title(title);
    }

    /// Set the cursor icon that is shown when hovering over the window.
    ///
    /// Safe to call every frame, the backend will only update the cursor if it
    /// has changed.
    pub fn set_cursor_icon(&self, icon: MouseCursor) {
        self.0.set_cursor_icon(icon);
    }

    /// Set whether the window has decorations (title bar, borders, etc)
    ///
    /// Does nothing when opened with [`WindowBuilder::open_embedded`].
    pub fn set_decorations(&self, decorations: bool) {
        self.0.set_decorations(decorations);
    }

    /// Warp the mouse cursor to the given position within the window.
    ///
    /// Position is in physical pixels, with (0, 0) being the top-left corner of
    /// the client area.
    pub fn set_cursor_position(&self, pos: impl Into<Point>) {
        self.0.set_cursor_position(pos.into());
    }

    /// Set the size of the client area in physical pixels.
    pub fn set_size(&self, size: impl Into<Size>) {
        self.0.set_size(size.into());
    }

    /// Sets the minimum size of the window in physical pixels.
    ///
    /// Used to restrict the user from resizing the window below a certain size.
    pub fn set_min_size(&self, min: impl Into<Size>) {
        self.0.set_min_size(min.into());
    }

    /// Sets the maximum size of the window in physical pixels.
    ///
    /// Used to restrict the user from resizing the window above a certain size.
    pub fn set_max_size(&self, max: impl Into<Size>) {
        self.0.set_max_size(max.into());
    }

    /// Set the window position (position of client area) in physical pixels
    /// relative to the origin (top-left corner) of the coordinate system.
    ///
    /// The coordinate system depends on how the window was created:
    /// - For top-level windows or transient windows, it is the screen
    ///   coordinate system, with (0, 0) being the top-left corner of the
    ///   primary monitor.
    /// - For embedded windows, it is the coordinate system of the parent
    ///   window, with (0, 0) being the top-left corner of the parent window's
    ///   client area.
    ///
    /// If not specified, the window will be centered on the screen or parent
    /// window (or positioned at (0, 0) if embedded)
    ///
    /// The coordinate system is X+ right, Y+ down
    pub fn set_position(&self, pos: impl Into<Point>) {
        self.0.set_position(pos.into());
    }

    /// Set whether the window is visible.
    pub fn set_visible(&self, visible: bool) {
        self.0.set_visible(visible);
    }

    /// Open the given URL or file path in the system's default application.
    ///
    /// Returns `true` if the action was handled by the OS
    pub fn open_url(&self, url: &str) -> bool {
        self.0.open_url(url)
    }

    /// Set the current text contents of the system clipboard.
    ///
    /// Returns `true` if the action was handled by the OS
    pub fn set_clipboard(&self, data: Exchange) -> bool {
        self.0.set_clipboard(data)
    }

    /// Get the current text contents of the system clipboard, if any.
    pub fn get_clipboard(&self) -> Exchange {
        self.0.get_clipboard()
    }
}

impl WindowWaker {
    /// Wake up the associated window in a fire-and-forget fashion (without
    /// waiting for the event handler to actually process the event). Emits a
    /// [`Event::Wakeup`](`crate::Event::Wakeup`) event.
    ///
    /// Returns [`WakeupError`] if the window has already been
    /// closed.
    pub fn wakeup(&self) -> Result<(), WakeupError> {
        self.0.wakeup()
    }
}

impl WindowBuilder {
    /// Create a new [`WindowBuilder`] with the given event handler factory and
    /// default parameters
    pub fn new(
        factory: impl for<'a> FnOnce(
            Window<'a>,
        ) -> Result<
            Box<dyn WindowHandler + 'a>,
            Box<dyn Error + Send + Sync>,
        > + Send
        + 'static,
    ) -> Self {
        Self {
            transparent: false,
            opengl: None,
            factory: Box::new(factory),
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

    /// Set the OpenGL configuration for the window, if any
    pub fn with_opengl(self, config: GlConfig) -> Self {
        Self {
            opengl: Some(config),
            ..self
        }
    }

    /// Open a top-level window. Blocks until the window is closed.
    ///
    /// Returns `Err` if the window could not be created or if an error occurred
    /// during the lifetime of the window.
    pub fn open_blocking(self) -> Result<(), WindowError> {
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
    pub fn open_transient<W>(self, parent: W) -> Result<WindowWaker, WindowError>
    where
        W: rwh_06::HasWindowHandle,
    {
        let handle = parent
            .window_handle()
            .map_err(|_| WindowError::InvalidParent)?
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
    pub fn open_embedded<W>(self, parent: W) -> Result<WindowWaker, WindowError>
    where
        W: rwh_06::HasWindowHandle,
    {
        let handle = parent
            .window_handle()
            .map_err(|_| WindowError::InvalidParent)?
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
            .field("transparent", &self.transparent)
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
