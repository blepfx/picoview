use crate::{GlConfig, GlContext, RawHandle, Window};
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

bitflags! {
    #[derive(Clone, Copy, Eq, PartialEq, Debug)]
    pub struct Style: u16 {
        const VISIBLE = 1 << 0;
        const BORDER = 1 << 1;
        const TRANSPARENT = 1 << 2;
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

pub struct Options {
    pub parent: Option<RawHandle>,
    pub handler: EventHandler,
    pub style: Style,
    pub size: Size,
    pub position: Option<Point>,
    pub opengl: Option<GlConfig>,
}

pub type EventHandler = Box<dyn FnMut(Event, &mut Window) -> EventResponse + Send>;

#[derive(Debug)]
pub enum Error {
    PlatformError(String),
    OpenGlError(String),
}
