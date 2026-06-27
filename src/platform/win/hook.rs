use super::window::{WM_USER_KEY_DOWN, WM_USER_KEY_UP};
use std::cell::{Cell, RefCell};
use std::collections::HashSet;
use std::mem::zeroed;
use std::ptr::null_mut;
use std::rc::{Rc, Weak};
use windows_sys::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
use windows_sys::Win32::System::Threading::GetCurrentThreadId;
use windows_sys::Win32::UI::WindowsAndMessaging::*;

thread_local! {
    static HOOK: Cell<Weak<KeyboardHook>> = const { Cell::new(Weak::new()) };
}

/// A keyboard hook, used to capture key events in case a DAW
/// tries to capture the events meant for us.
///
/// Note that this is a thread-local hook, so it will only capture events for
/// windows created on the same thread.
pub struct KeyboardHook {
    // The hook handle, used to unhook the hook when it is no longer needed
    hhook: HHOOK,
    // The set of windows that this hook will capture events for.
    windows: RefCell<HashSet<HWND>>,
}

impl KeyboardHook {
    /// Gets the current hook for this thread if one exists, otherwise set up a
    /// new one and return it.
    pub fn install() -> Rc<Self> {
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

    /// Adds a window to the list of windows that this hook will capture events
    /// for.
    ///
    /// The hook will send us [`WM_USER_KEY_DOWN`] and [`WM_USER_KEY_UP`]
    /// messages.
    pub fn add_window(&self, hwnd: HWND) {
        self.windows.borrow_mut().insert(hwnd);
    }

    /// Removes a window from the list of windows
    pub fn remove_window(&self, hwnd: HWND) {
        self.windows.borrow_mut().remove(&hwnd);
    }

    /// Checks if a window is in the list of windows that this hook will capture
    /// events for.
    pub fn has_window(&self, hwnd: HWND) -> bool {
        self.windows.borrow().contains(&hwnd)
    }
}

impl Drop for KeyboardHook {
    fn drop(&mut self) {
        // unhook the thread hook
        unsafe { windows_sys::Win32::UI::WindowsAndMessaging::UnhookWindowsHookEx(self.hhook) };

        // so we can deallocate the Rc memory afterwards
        HOOK.set(Weak::new());
    }
}

/// Our `winapi` hook procedure.
unsafe extern "system" fn keyboard_hook_proc(
    n_code: i32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    fn is_our_window(hwnd: HWND) -> bool {
        KeyboardHook::install().has_window(hwnd)
    }

    unsafe {
        if n_code == HC_ACTION as i32 && wparam == PM_REMOVE as usize {
            let message = lparam as *mut MSG;

            // if its a key event...
            if matches!((*message).message, WM_KEYDOWN | WM_KEYUP) {
                // if the window is one of ours...
                while is_our_window((*message).hwnd) {
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

        CallNextHookEx(null_mut(), n_code, wparam, lparam)
    }
}
