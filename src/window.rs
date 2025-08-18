use crate::{
    GlConfig, GlContext,
    platform::{self, OsWindow},
    rwh_06,
};
use bitflags::bitflags;
use std::{
    cell::{Cell, RefCell},
    fmt::Debug,
    path::PathBuf,
    rc::{Rc, Weak},
};

#[derive(Clone, Copy, Default, Debug, Eq, PartialEq, Hash)]
#[repr(u8)]
pub enum MouseCursor {
    #[default]
    Default,

    Hand,
    HandGrabbing,
    Help,

    Hidden,

    Text,
    VerticalText,

    Working,
    PtrWorking,

    NotAllowed,
    PtrNotAllowed,

    ZoomIn,
    ZoomOut,

    Alias,
    Copy,
    Move,
    AllScroll,
    Cell,
    Crosshair,

    EResize,
    NResize,
    NeResize,
    NwResize,
    SResize,
    SeResize,
    SwResize,
    WResize,
    EwResize,
    NsResize,
    NwseResize,
    NeswResize,
    ColResize,
    RowResize,
}

bitflags! {
    #[derive(Clone, Copy, Eq, PartialEq, Debug)]
    pub struct Modifiers: u16 {
        const ALT = 1 << 0;
        const CTRL = 1 << 1;
        const META = 1 << 2;
        const SHIFT = 1 << 3;

        const SCROLL_LOCK = 1 << 4;
        const NUM_LOCK = 1 << 5;
        const CAPS_LOCK = 1 << 6;
    }
}

#[derive(Copy, Clone, Debug, PartialEq)]
pub struct Point {
    pub x: f32,
    pub y: f32,
}

#[derive(Copy, Clone, Debug, PartialEq)]
pub struct Size {
    pub width: u32,
    pub height: u32,
}

impl From<(u32, u32)> for Size {
    fn from((width, height): (u32, u32)) -> Self {
        Self { width, height }
    }
}

impl From<(u32, u32)> for Point {
    fn from((x, y): (u32, u32)) -> Self {
        Self {
            x: x as f32,
            y: y as f32,
        }
    }
}

impl From<(f32, f32)> for Point {
    fn from((x, y): (f32, f32)) -> Self {
        Self { x, y }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MouseButton {
    Left,
    Right,
    Middle,
    Forward,
    Back,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Key {
    Backquote,
    Backslash,
    BracketLeft,
    BracketRight,
    Comma,
    D0,
    D1,
    D2,
    D3,
    D4,
    D5,
    D6,
    D7,
    D8,
    D9,
    A,
    B,
    C,
    D,
    E,
    F,
    G,
    H,
    I,
    J,
    K,
    L,
    M,
    N,
    O,
    P,
    Q,
    R,
    S,
    T,
    U,
    V,
    W,
    X,
    Y,
    Z,
    Equal,
    Minus,
    Period,
    Quote,
    Semicolon,
    Slash,
    AltLeft,
    AltRight,
    Backspace,
    CapsLock,
    ContextMenu,
    ControlLeft,
    ControlRight,
    Enter,
    MetaLeft,
    MetaRight,
    ShiftLeft,
    ShiftRight,
    Space,
    Tab,
    Delete,
    End,
    Home,
    Insert,
    PageDown,
    PageUp,
    ArrowDown,
    ArrowLeft,
    ArrowRight,
    ArrowUp,
    NumLock,
    Numpad0,
    Numpad1,
    Numpad2,
    Numpad3,
    Numpad4,
    Numpad5,
    Numpad6,
    Numpad7,
    Numpad8,
    Numpad9,
    NumpadAdd,
    NumpadBackspace,
    NumpadClear,
    NumpadClearEntry,
    NumpadComma,
    NumpadDecimal,
    NumpadDivide,
    NumpadEnter,
    NumpadEqual,
    NumpadHash,
    NumpadMemoryAdd,
    NumpadMemoryClear,
    NumpadMemoryRecall,
    NumpadMemoryStore,
    NumpadMemorySubtract,
    NumpadMultiply,
    NumpadParenLeft,
    NumpadParenRight,
    NumpadStar,
    NumpadSubtract,
    Escape,
    Fn,
    FnLock,
    PrintScreen,
    ScrollLock,
    F1,
    F2,
    F3,
    F4,
    F5,
    F6,
    F7,
    F8,
    F9,
    F10,
    F11,
    F12,
}

#[derive(Debug)]
#[non_exhaustive]
pub enum Event<'a> {
    WindowOpen,
    WindowFocus,
    WindowBlur,
    WindowScale {
        scale: f32,
    },

    MouseMove {
        cursor: Option<Point>,
    },
    MouseDown {
        button: MouseButton,
    },
    MouseUp {
        button: MouseButton,
    },
    MouseScroll {
        x: f32,
        y: f32,
    },

    GestureRotate {
        rotate: f32,
    },
    GestureZoom {
        zoom: f32,
    },

    KeyModifiers {
        modifiers: Modifiers,
    },

    KeyDown {
        key: Key,
    },
    KeyUp {
        key: Key,
    },

    WindowInvalidate {
        top: u32,
        left: u32,
        bottom: u32,
        right: u32,
    },
    WindowFrame {
        gl: Option<&'a dyn GlContext>,
    },

    DragHover {
        files: &'a [PathBuf],
    },
    DragAccept {
        files: &'a [PathBuf],
    },
    DragCancel,
}

#[derive(Clone, Copy, Debug)]
pub enum EventResponse {
    Rejected,
    Captured,
}

#[derive(Debug)]
pub enum Error {
    PlatformError(String),
    OpenGlError(String),
    InvalidParent,
}

pub type EventHandler = Box<dyn WindowHandler>;

#[non_exhaustive]
pub struct WindowBuilder {
    pub visible: bool,
    pub decorations: bool,
    pub transparent: bool,
    pub blur: bool,

    pub title: String,
    pub constructor: Box<dyn FnOnce(Window) -> EventHandler>,
    pub size: Size,
    pub position: Option<Point>,
    pub opengl: Option<GlConfig>,
}

impl WindowBuilder {
    pub fn new<W: WindowHandler + 'static, T: FnOnce(Window) -> W + 'static>(handler: T) -> Self {
        Self {
            visible: true,
            decorations: true,
            transparent: false,
            blur: false,
            title: String::new(),
            size: Size {
                width: 200,
                height: 200,
            },
            position: None,
            opengl: None,
            constructor: Box::new(|window| Box::new((handler)(window))),
        }
    }

    pub fn with_blur(self, blur: bool) -> Self {
        Self { blur, ..self }
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

    pub fn with_title(self, title: impl Into<String>) -> Self {
        Self {
            title: title.into(),
            ..self
        }
    }

    pub fn with_size(self, size: impl Into<Size>) -> Self {
        Self {
            size: size.into(),
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

pub trait WindowHandler {
    fn window<'a>(&'a self) -> &'a Window;
    fn window_mut<'a>(&'a mut self) -> &'a mut Window;
    fn on_event(&mut self, event: Event) -> EventResponse;
}

#[repr(transparent)]
pub struct Window(pub(crate) Rc<RefCell<platform::PlatformWindow>>);
pub struct WeakHandle(pub(crate) Weak<RefCell<platform::PlatformWindow>>);

impl Window {
    pub fn handle(&self) -> WeakHandle {
        WeakHandle(Rc::downgrade(&self.0))
    }

    pub fn close(&mut self) {
        self.0.borrow_mut().close();
    }

    pub fn set_title(&mut self, title: &str) {
        self.0.borrow_mut().set_title(title);
    }

    pub fn set_cursor_icon(&mut self, icon: MouseCursor) {
        self.0.borrow_mut().set_cursor_icon(icon);
    }

    pub fn set_cursor_position(&mut self, pos: impl Into<Point>) {
        self.0.borrow_mut().set_cursor_position(pos.into());
    }

    pub fn set_size(&mut self, size: impl Into<Size>) {
        self.0.borrow_mut().set_size(size.into());
    }

    pub fn set_position(&mut self, pos: impl Into<Point>) {
        self.0.borrow_mut().set_position(pos.into());
    }

    pub fn set_visible(&mut self, visible: bool) {
        self.0.borrow_mut().set_visible(visible);
    }

    pub fn set_keyboard_input(&mut self, focus: bool) {
        self.0.borrow_mut().set_keyboard_input(focus);
    }

    pub fn open_url(&mut self, url: &str) -> bool {
        self.0.borrow_mut().open_url(url)
    }

    pub fn get_clipboard_text(&mut self) -> Option<String> {
        self.0.borrow_mut().get_clipboard_text()
    }

    pub fn set_clipboard_text(&mut self, text: &str) -> bool {
        self.0.borrow_mut().set_clipboard_text(text)
    }
}

impl<'a> rwh_06::HasWindowHandle for WeakHandle {
    fn window_handle(&self) -> Result<rwh_06::WindowHandle<'_>, rwh_06::HandleError> {
        let Some(inner) = self.0.upgrade() else {
            return Err(raw_window_handle::HandleError::Unavailable);
        };

        unsafe {
            Ok(rwh_06::WindowHandle::borrow_raw(
                inner.borrow().window_handle(),
            ))
        }
    }
}

impl<'a> rwh_06::HasDisplayHandle for WeakHandle {
    fn display_handle(&self) -> Result<rwh_06::DisplayHandle<'_>, rwh_06::HandleError> {
        let Some(inner) = self.0.upgrade() else {
            return Err(raw_window_handle::HandleError::Unavailable);
        };

        unsafe {
            Ok(rwh_06::DisplayHandle::borrow_raw(
                inner.borrow().display_handle(),
            ))
        }
    }
}
