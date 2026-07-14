use crate::platform::win::window::{WM_USER_KEY_DOWN, WM_USER_KEY_MODIFIERS, WM_USER_KEY_UP};
use crate::{Key, Modifiers};
use std::cell::{Cell, RefCell};
use std::collections::HashSet;
use std::mem::zeroed;
use std::ptr::null_mut;
use std::rc::{Rc, Weak};
use windows_sys::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
use windows_sys::Win32::System::Threading::GetCurrentThreadId;
use windows_sys::Win32::UI::Input::KeyboardAndMouse::*;
use windows_sys::Win32::UI::WindowsAndMessaging::*;

/// Query the current modifier state from the thread-local OS state.
pub fn query_modifiers() -> Modifiers {
    fn is_held(key: VIRTUAL_KEY) -> bool {
        unsafe { GetKeyState(key as _) & !0x1 != 0 }
    }

    fn is_toggled(key: VIRTUAL_KEY) -> bool {
        unsafe { GetKeyState(key as _) & 0x1 != 0 }
    }

    Modifiers {
        shift: is_held(VK_SHIFT),
        ctrl: is_held(VK_CONTROL),
        alt: is_held(VK_MENU),
        meta: is_held(VK_LWIN) || is_held(VK_RWIN),
        caps_lock: is_toggled(VK_CAPITAL),
        num_lock: is_toggled(VK_NUMLOCK),
        scroll_lock: is_toggled(VK_SCROLL),
    }
}

/// Converts a scan code provided by a [`WM_KEYUP`] or [`WM_KEYDOWN`] message
/// into a [`Key`].
pub fn scan_code_to_key(scan_code: u32) -> Option<Key> {
    use Key::*;
    Some(match scan_code {
        0x1 => Escape,
        0x2 => D1,
        0x3 => D2,
        0x4 => D3,
        0x5 => D4,
        0x6 => D5,
        0x7 => D6,
        0x8 => D7,
        0x9 => D8,
        0xA => D9,
        0xB => D0,
        0xC => Minus,
        0xD => Equal,
        0xE => Backspace,
        0xF => Tab,
        0x10 => Q,
        0x11 => W,
        0x12 => E,
        0x13 => R,
        0x14 => T,
        0x15 => Y,
        0x16 => U,
        0x17 => I,
        0x18 => O,
        0x19 => P,
        0x1A => BracketLeft,
        0x1B => BracketRight,
        0x1C => Enter,
        0x1D => ControlLeft,
        0x1E => A,
        0x1F => S,
        0x20 => D,
        0x21 => F,
        0x22 => G,
        0x23 => H,
        0x24 => J,
        0x25 => K,
        0x26 => L,
        0x27 => Semicolon,
        0x28 => Quote,
        0x29 => Backquote,
        0x2A => ShiftLeft,
        0x2B => Backslash,
        0x2C => Z,
        0x2D => X,
        0x2E => C,
        0x2F => V,
        0x30 => B,
        0x31 => N,
        0x32 => M,
        0x33 => Comma,
        0x34 => Period,
        0x35 => Slash,
        0x36 => ShiftRight,
        0x37 => NumpadMultiply,
        0x38 => AltLeft,
        0x39 => Space,
        0x3A => CapsLock,
        0x3B => F1,
        0x3C => F2,
        0x3D => F3,
        0x3E => F4,
        0x3F => F5,
        0x40 => F6,
        0x41 => F7,
        0x42 => F8,
        0x43 => F9,
        0x44 => F10,
        0x46 => ScrollLock,
        0x47 => Numpad7,
        0x48 => Numpad8,
        0x49 => Numpad9,
        0x4A => NumpadSubtract,
        0x4B => Numpad4,
        0x4C => Numpad5,
        0x4D => Numpad6,
        0x4E => NumpadAdd,
        0x4F => Numpad1,
        0x50 => Numpad2,
        0x51 => Numpad3,
        0x52 => Numpad0,
        0x53 => NumpadDecimal,
        0x54 => PrintScreen,
        0x57 => F11,
        0x58 => F12,
        0x59 => NumpadEqual,
        0x7E => NumpadComma,
        0x11C => NumpadEnter,
        0x11D => ControlRight,
        0x135 => NumpadDivide,
        0x137 => PrintScreen,
        0x138 => AltRight,
        0x145 => NumLock,
        0x147 => Home,
        0x148 => ArrowUp,
        0x149 => PageUp,
        0x14B => ArrowLeft,
        0x14D => ArrowRight,
        0x14F => End,
        0x150 => ArrowDown,
        0x151 => PageDown,
        0x152 => Insert,
        0x153 => Delete,
        0x15B => MetaLeft,
        0x15C => MetaRight,
        0x15D => ContextMenu,
        _ => return None,
    })
}

/// A keyboard hook, used to capture key events in case a DAW
/// tries to capture the events meant for us.
pub struct KeyboardHook {
    hook: Rc<HookInner>,
    hwnd: HWND,
}

impl KeyboardHook {
    /// Gets the current hook for this thread if one exists, otherwise set up a
    /// new one and return it.
    ///
    /// # Safety
    /// - The `hwnd` must be a valid window handle for the lifetime of the
    ///   [`KeyboardHook`] object.
    pub unsafe fn new(hwnd: HWND) -> Self {
        // install the hook if we havent already, and keep it alive for the lifetime of
        // this window
        let hook = HookInner::get_or_install();
        // start tracking events for this window
        hook.windows.borrow_mut().insert(hwnd);
        Self { hook, hwnd }
    }
}

impl Drop for KeyboardHook {
    fn drop(&mut self) {
        // stop tracking events for this window
        self.hook.windows.borrow_mut().remove(&self.hwnd);
    }
}

thread_local! {
    static HOOK: Cell<Weak<HookInner>> = const { Cell::new(Weak::new()) };
}

/// This manages the lifetime of the hook.
struct HookInner {
    // The hook handle, used to unhook the hook when it is no longer needed
    hhook: HHOOK,
    // The set of windows that this hook will capture events for.
    windows: RefCell<HashSet<HWND>>,
}

impl HookInner {
    /// Gets the current hook for this thread if one exists, otherwise set up a
    /// new one and return it.
    fn get_or_install() -> Rc<Self> {
        // take the hook
        let hook = match HOOK.replace(Weak::new()).upgrade() {
            Some(hook) => hook,

            // if we dont have one, create it
            None => Rc::new(Self {
                windows: RefCell::new(HashSet::new()),
                hhook: unsafe {
                    SetWindowsHookExW(
                        WH_GETMESSAGE,
                        Some(keyboard_hook_proc),
                        null_mut(),
                        GetCurrentThreadId(),
                    )
                },
            }),
        };

        // so new windows get a copy of the hook
        HOOK.set(Rc::downgrade(&hook));
        hook
    }
}

impl Drop for HookInner {
    fn drop(&mut self) {
        // unhook the thread hook
        unsafe { UnhookWindowsHookEx(self.hhook) };

        // memory will stick around even after the object is dropped if there are weak
        // references to it, so we clear the thread local to make sure it can be
        // freed.

        // we do not want memory hanging around because when unloading dlls,
        // statics and thread-locals might not be dropped, causing a tiny leak.
        HOOK.set(Weak::new());
    }
}

/// Our `winapi` hook procedure.
unsafe extern "system" fn keyboard_hook_proc(msg: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    unsafe {
        if msg == HC_ACTION as i32 && wparam == PM_REMOVE as usize {
            let message = lparam as *mut MSG;

            // if its a key event...
            if matches!((*message).message, WM_KEYDOWN | WM_KEYUP) {
                let hook = HookInner::get_or_install(); // should be already installed, just a query.

                // send modifier change messages (we do it here because WM_KEYDOWN and WM_KEYUP
                // can be consumed by the host, and polling it per frame is not good)
                for &hwnd in hook.windows.borrow().iter() {
                    // key event happened, modifiers likely changed...
                    PostMessageW(hwnd, WM_USER_KEY_MODIFIERS, 0, 0);
                }

                // if the window is one of ours...
                while hook.windows.borrow().contains(&(*message).hwnd) {
                    // let our window handle it and see if it wants to capture it
                    let capture = SendMessageW(
                        (*message).hwnd,
                        if (*message).message == WM_KEYDOWN {
                            WM_USER_KEY_DOWN
                        } else {
                            WM_USER_KEY_UP
                        },
                        (*message).wParam,
                        (*message).lParam,
                    ) != 0;

                    // if it does, we stop the message from being dispatched (replace it with a zero
                    // message)
                    if capture {
                        *message = MSG {
                            message: WM_USER,
                            ..zeroed()
                        };

                        return 0;
                    } else {
                        // otherwise, we check the parent window to see if it wants to capture it
                        // instead
                        let parent = GetParent((*message).hwnd);
                        if parent.is_null() {
                            break;
                        } else {
                            (*message).hwnd = parent;
                        }
                    }
                }

                // if it wasn't meant for us, we let it pass through
            }
        }

        // pass the message, normal hook stuff
        CallNextHookEx(null_mut(), msg, wparam, lparam)
    }
}
