use std::path::PathBuf;

#[allow(unused_imports)] // docs
use crate::*;

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

/// A fractional point in physical pixels with top-left origin
#[derive(Copy, Clone, Debug, PartialEq, Default)]
pub struct Point {
    /// The x coordinate
    pub x: f64,

    /// The y coordinate
    pub y: f64,
}

/// Integer size in physical pixels
#[derive(Copy, Clone, Debug, PartialEq, Default)]
pub struct Size {
    /// The width in physical pixels
    pub width: u32,

    /// The height in physical pixels
    pub height: u32,
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
    pub fn from_logical(width: f64, height: f64, scale: f64) -> Self {
        Self {
            width: (width * scale).round() as u32,
            height: (height * scale).round() as u32,
        }
    }

    /// Convert this [`Size`] to logical pixels using a scale factor.
    pub fn to_logical(&self, scale: f64) -> (f64, f64) {
        (self.width as f64 / scale, self.height as f64 / scale)
    }
}

impl From<(u32, u32)> for Size {
    fn from((width, height): (u32, u32)) -> Self {
        Self { width, height }
    }
}

impl From<(u32, u32)> for Point {
    fn from((x, y): (u32, u32)) -> Self {
        Self {
            x: x as f64,
            y: y as f64,
        }
    }
}

impl From<(f64, f64)> for Point {
    fn from((x, y): (f64, f64)) -> Self {
        Self { x, y }
    }
}

/// A mouse button.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
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
pub enum Exchange {
    /// No data in the clipboard
    Empty,

    /// Plain text data
    Text(String),

    /// A list of files (for example, a list of files from a file explorer)
    Files(Vec<PathBuf>),
}

/// An event generated by the windowing system and delivered to the event
/// handler.
#[derive(Debug)]
#[non_exhaustive]
pub enum Event<'a> {
    /// A wakeup event triggered by a call to
    /// [`WindowWaker::wakeup`]
    Wakeup,

    /// User requested to close the window (by clicking the close button, or
    /// pressing Alt+F4, etc)
    ///
    /// To actually close the window, you have to call
    /// [`Window::close`].
    WindowClose,

    /// The window gained or lost focus.
    ///
    /// By default, assume that the window is not focused.
    WindowFocus {
        /// `true` if the window gained focus, `false` if it lost focus
        focus: bool,
    },

    /// The window was occluded or unoccluded (for example, by another
    /// window or by minimizing).
    ///
    /// By default, assume that the window is not occluded.
    WindowOccluded {
        /// `true` if the window is now _fully_ occluded, `false` otherwise
        occluded: bool,
    },

    /// The window scale factor changed (for example, when moved to a different
    /// monitor).
    ///
    /// This is a hint that the application may want to adjust its rendering
    /// scale.
    ///
    /// Does not affect the coordinate system of positions and sizes, which are
    /// always in physical pixels.
    WindowScale {
        /// The new scale factor of the window
        scale: f64,
    },

    /// The window was resized.
    WindowResize {
        /// The new physical size of the window's client area.
        size: Size,
    },

    /// The window was moved.
    WindowMove {
        /// The new position of the window relative to the origin.
        ///
        /// See [`Window::set_position`] for
        /// details on coordinate system.
        point: Point,
    },

    /// Frame event. You should redraw the window in response to this event.
    ///
    /// This event is sent at the refresh rate of the display (typically 60 Hz),
    /// on a best-effort basis (might use an unsynchronized timer depending on
    /// the platform).
    WindowFrame,

    /// The area of the window that needs to be redrawn.
    ///
    /// This event may be sent multiple times before the next `WindowFrame`
    /// event.
    ///
    /// You can ignore this event if you redraw the window continuously.
    WindowDamage {
        /// The `x` coordinate of the top-left corner of the damaged area.
        x: u32,
        /// The `y` coordinate of the top-left corner of the damaged area.
        y: u32,
        /// The width of the damaged area.
        w: u32,
        /// The height of the damaged area.
        h: u32,
    },

    /// The mouse cursor left the window.
    ///
    /// Note that there is no corresponding event for when the mouse enters the
    /// window, you can track that yourself by checking for
    /// [`Event::MouseMove`] events.
    MouseLeave,

    /// The mouse cursor position has changed within the window.
    MouseMove {
        /// The position of the cursor relative to the window's client area.
        relative: Point,

        /// The position of the cursor relative to the entire screen.
        absolute: Point,
    },

    /// A mouse button was pressed.
    MouseDown {
        /// Which mouse button was pressed
        button: MouseButton,
    },

    /// A mouse button was released.
    MouseUp {
        /// Which mouse button was released
        button: MouseButton,
    },

    /// The mouse wheel was scrolled (can also represent touchpad scrolling).
    ///
    /// `picoview` normalizes scroll events to a consistent unit across
    /// platforms.
    MouseScroll {
        /// The amount scrolled in the horizontal direction (positive right)
        x: f64,

        /// The amount scrolled in the vertical direction (positive down)
        y: f64,
    },

    /// The state of the modifier keys (Shift, Ctrl, Alt, etc.) changed.
    KeyModifiers {
        /// The new state of the modifier keys
        modifiers: Modifiers,
    },

    /// A rotation gesture was performed (for example, a two-finger rotation on
    /// a touchpad).
    GestureRotate {
        /// The rotation angle delta in degrees (positive clockwise)
        angle: f64,
    },

    /// A zoom gesture was performed (for example, a two-finger pinch on a
    /// touchpad).
    GestureZoom {
        /// The zoom scale multiplicative delta (>1 means zooming in, <1 means
        /// zooming out)
        scale: f64,
    },

    /// A key was pressed.
    KeyDown {
        /// Which key was pressed
        key: Key,

        /// Set to `true` to indicate that the event has been handled and should
        /// not be propagated to the parent (if this window is embedded in
        /// another window)
        capture: &'a mut bool,
    },

    /// A key was released.
    KeyUp {
        /// Which key was released
        key: Key,

        /// Set to `true` to indicate that the event has been handled and should
        /// not be propagated to the parent (if this window is embedded in
        /// another window)
        capture: &'a mut bool,
    },

    /// Drag-and-drop data was dragged into the window, the position will be
    /// reported via [`Event::DragMove`] events until the drag-and-drop
    /// operation is completed (via [`Event::DragAccept`]) or cancelled (via
    /// [`Event::DragLeave`]).
    DragEnter {
        /// The data being dragged into the window
        data: Exchange,
        /// The position of the cursor relative to the window's client area.
        point: Point,
    },

    /// Drag-and-drop data was dragged within the window
    DragMove {
        /// The position of the cursor relative to the window's client area.
        point: Point,
    },

    /// Drag-and-drop data was dragged out of the window, or the drag-and-drop
    /// operation was cancelled.
    DragLeave,

    /// Drag-and-drop data was released into the window at the last
    /// [`Event::DragMove`] position with the data provided by the last
    /// [`Event::DragEnter`].
    DragAccept,
}
