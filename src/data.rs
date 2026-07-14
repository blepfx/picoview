use std::path::PathBuf;

#[allow(unused_imports)] // docs
use crate::*;

/// A fractional point in physical pixels with top-left origin
#[derive(Copy, Clone, Debug, PartialEq, Default)]
pub struct Point {
    /// The x coordinate
    pub x: f64,

    /// The y coordinate
    pub y: f64,
}

/// A pixel-aligned size in physical pixels
#[derive(Copy, Clone, Debug, PartialEq, Default)]
pub struct Size {
    /// The width in physical pixels
    pub width: u32,

    /// The height in physical pixels
    pub height: u32,
}

/// A pixel-aligned rectangle in physical pixels.
#[derive(Copy, Clone, Debug, PartialEq, Default)]
pub struct Rect {
    /// The Y coordinate of the top-left corner of the rectangle
    pub top: i32,
    /// The X coordinate of the top-left corner of the rectangle
    pub left: i32,
    /// The Y coordinate of the bottom-right corner of the rectangle
    pub bottom: i32,
    /// The X coordinate of the bottom-right corner of the rectangle
    pub right: i32,
}

impl Size {
    /// Minimum possible size (0, 0)
    pub const MIN: Self = Self {
        width: 0,
        height: 0,
    };

    /// Maximum possible size
    pub const MAX: Self = Self {
        width: u32::MAX,
        height: u32::MAX,
    };

    /// Create a new [`Size`] from logical pixels and a scale factor.
    #[must_use]
    #[inline]
    pub fn from_logical(width: f64, height: f64, scale: f64) -> Self {
        Self {
            width: (width * scale).round() as u32,
            height: (height * scale).round() as u32,
        }
    }

    /// Convert this [`Size`] to logical pixels using a scale factor.
    #[must_use]
    #[inline]
    pub fn to_logical(&self, scale: f64) -> (f64, f64) {
        (self.width as f64 / scale, self.height as f64 / scale)
    }
}

impl Rect {
    /// Create a new [`Rect`] from the coordinates of its top-left corner and
    /// its size.
    #[must_use]
    #[inline]
    pub fn from_xywh(x: i32, y: i32, width: u32, height: u32) -> Self {
        Self {
            top: y,
            left: x,
            bottom: y.saturating_add_unsigned(height),
            right: x.saturating_add_unsigned(width),
        }
    }

    /// Create a new [`Rect`] with the origin at (0, 0) and the given size.
    #[must_use]
    #[inline]
    pub fn from_size(size: Size) -> Self {
        Self {
            top: 0,
            left: 0,
            bottom: size.height.try_into().unwrap_or(i32::MAX),
            right: size.width.try_into().unwrap_or(i32::MAX),
        }
    }

    /// Size of the rectangle.
    #[must_use]
    #[inline]
    pub fn size(&self) -> Size {
        Size {
            width: (self.right - self.left).try_into().unwrap_or(0),
            height: (self.bottom - self.top).try_into().unwrap_or(0),
        }
    }

    /// Origin of the rectangle (top-left corner).
    #[must_use]
    #[inline]
    pub fn origin(&self) -> Point {
        Point {
            x: self.left as f64,
            y: self.top as f64,
        }
    }

    /// Offset the rectangle by the given amounts in the x and y directions.
    #[must_use]
    #[inline]
    pub fn offset(&self, dx: i32, dy: i32) -> Self {
        Self {
            top: self.top.saturating_add(dy),
            left: self.left.saturating_add(dx),
            bottom: self.bottom.saturating_add(dy),
            right: self.right.saturating_add(dx),
        }
    }
}

impl From<(u32, u32)> for Size {
    #[inline]
    fn from((width, height): (u32, u32)) -> Self {
        Self { width, height }
    }
}

impl From<(u32, u32)> for Point {
    #[inline]
    fn from((x, y): (u32, u32)) -> Self {
        Self {
            x: x as f64,
            y: y as f64,
        }
    }
}

impl From<(i32, i32)> for Point {
    #[inline]
    fn from((x, y): (i32, i32)) -> Self {
        Self {
            x: x as f64,
            y: y as f64,
        }
    }
}

impl From<(f64, f64)> for Point {
    #[inline]
    fn from((x, y): (f64, f64)) -> Self {
        Self { x, y }
    }
}

impl From<(f32, f32)> for Point {
    #[inline]
    fn from((x, y): (f32, f32)) -> Self {
        Self {
            x: x as f64,
            y: y as f64,
        }
    }
}

/// The visibility state of a window.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum WindowVisibility {
    /// The window is in its normal state (not minimized or maximized)
    Normal,
    /// The window is hidden (unmapped)
    Hidden,
    /// The window is minimized (iconified)
    Minimized,
    /// The window is occluded (hidden under another window or not visible on
    /// the screen)
    Occluded,
}

/// A mouse button.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
#[non_exhaustive]
pub enum MouseButton {
    /// Left mouse button
    Left = 0,
    /// Right mouse button
    Right,
    /// Middle mouse button (usually the scroll wheel button)
    Middle,
    /// Forward mouse button (usually the 4th button)
    Forward,
    /// Back mouse button (usually the 5th button)
    Back,
}

/// A mouse cursor icon that is predefined by the platform.
///
/// Not all platforms support all cursor types, in which case a closest matching
/// cursor is used.
#[derive(Clone, Copy, Default, Debug, Eq, PartialEq, Hash)]
#[repr(u8)]
#[allow(missing_docs)]
#[non_exhaustive]
pub enum MouseCursor {
    #[default]
    Default,
    Hidden,

    Hand,
    HandGrabbing,
    Help,

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

/// Key modifier flags that are tracked separately from key events
#[derive(Clone, Copy, Debug, PartialEq, Default)]
#[non_exhaustive]
pub struct Modifiers {
    /// Alt key is held down (Option key on Mac)
    pub alt: bool,
    /// Control key is held down (Command key on Mac)
    pub ctrl: bool,
    /// Meta key is held down (Control key on Mac)
    pub meta: bool,
    /// Shift key is held down
    pub shift: bool,
    /// Scroll lock is active
    pub scroll_lock: bool,
    /// Num lock is active
    pub num_lock: bool,
    /// Caps lock is active
    pub caps_lock: bool,
}

/// A logical key of a keyboard.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[allow(missing_docs)]
#[non_exhaustive]
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

/// A data exchange format for clipboard and drag-and-drop operations.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub enum Exchange {
    /// No data in the clipboard
    Empty,

    /// Plain text data
    Text(String),

    /// A list of files (for example, a list of files from a file explorer)
    Files(Vec<PathBuf>),
}

impl From<String> for Exchange {
    fn from(s: String) -> Self {
        Exchange::Text(s)
    }
}

impl From<&str> for Exchange {
    fn from(s: &str) -> Self {
        Exchange::Text(s.to_string())
    }
}

impl From<Vec<PathBuf>> for Exchange {
    fn from(paths: Vec<PathBuf>) -> Self {
        Exchange::Files(paths)
    }
}

impl From<&[PathBuf]> for Exchange {
    fn from(paths: &[PathBuf]) -> Self {
        Exchange::Files(paths.to_vec())
    }
}

/// The effect a drag-and-drop operation is expected to have
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
#[repr(u8)]
pub enum DropEffect {
    /// Operation rejected.
    Reject,
    /// Copy.
    Copy,
    /// Move.
    Move,
    /// Link.
    Link,
    /// Operation accepted (generic).
    Generic,
}
