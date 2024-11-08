use bitflags::bitflags;
use raw_window_handle::RawWindowHandle;
use std::path::PathBuf;

#[derive(Clone, Copy, Default, Debug, Eq, PartialEq, Hash)]
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
    pub width: f32,
    pub height: f32,
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
pub enum Event<'a> {
    WindowFocus,
    WindowBlur,
    WindowClose,

    MouseMove(Option<Point>),
    MouseDown(MouseButton),
    MouseUp(MouseButton),
    MouseScroll { x: f32, y: f32 },

    KeyModifiers(Modifiers),
    KeyChar(&'a str),
    KeyDown(Key),
    KeyUp(Key),

    Frame,

    DragHover { files: &'a [PathBuf] },
    DragAccept { files: &'a [PathBuf] },
    DragCancel,
}

pub enum Command {
    SetCursorIcon(MouseCursor),
    SetCursorPosition(Point),
    SetSize(Size),
    SetPosition(Point),
    SetStyle(Style),
    SetKeyboardInput(bool),
    Close,
}

#[derive(PartialEq, Eq, Clone, Copy, Debug)]
pub enum Style {
    Decorated,
    Borderless,
    BorderlessShadow,
    Hidden,
}

#[derive(Clone, Copy, Debug)]
pub enum EventResponse {
    Ignored,
    Captured,
    AcceptDrop(DropOperation),
}

#[derive(Clone, Copy, Debug)]
pub enum DropOperation {
    None,
    Copy,
    Move,
    Link,
}

unsafe impl Send for Options {}
pub struct Options {
    pub style: Style,
    pub parent: Option<RawWindowHandle>,
    pub size: Size,
    pub position: Option<Point>,
    pub handler: Box<dyn FnMut(Event) -> EventResponse + Send>,
}

#[derive(Debug)]
pub enum Error {
    PlatformError(String),
}
