use crate::{GlConfig, GlContext, platform};
use bitflags::bitflags;
use std::{fmt::Debug, path::PathBuf};

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
    WindowClose,
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
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RawHandle {
    X11 { window: std::ffi::c_uint },
    Win { hwnd: *mut std::ffi::c_void },
}

unsafe impl Send for RawHandle {}
unsafe impl Sync for RawHandle {}

pub type EventHandler = Box<dyn FnMut(Event, Window) -> EventResponse + Send>;

#[non_exhaustive]
pub struct WindowBuilder {
    pub visible: bool,
    pub decorations: bool,
    pub transparent: bool,
    pub blur: bool,

    pub title: String,
    pub handler: EventHandler,
    pub size: Size,
    pub position: Option<Point>,
    pub opengl: Option<GlConfig>,

    pub(crate) parent: Option<RawHandle>,
}

impl WindowBuilder {
    pub fn new<T: FnMut(Event, Window) -> EventResponse + Send + 'static>(handler: T) -> Self {
        Self {
            visible: true,
            decorations: true,
            transparent: false,
            blur: false,
            title: String::new(),
            parent: None,
            size: Size {
                width: 200,
                height: 200,
            },
            position: None,
            opengl: None,
            handler: Box::new(handler),
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
        platform::open_window(Self {
            parent: None,
            ..self
        })
    }

    pub fn open_parented(self, parent: RawHandle) -> Result<(), Error> {
        platform::open_window(Self {
            parent: Some(parent),
            ..self
        })
    }
}

#[repr(transparent)]
pub struct Window<'a>(pub(crate) &'a mut dyn platform::OsWindow);

impl<'a> Window<'a> {
    pub fn close(&mut self) {
        self.0.close();
    }

    pub fn handle(&self) -> RawHandle {
        self.0.handle()
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

    pub fn set_keyboard_input(&mut self, focus: bool) {
        self.0.set_keyboard_input(focus);
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
