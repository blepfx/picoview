use super::window::{WM_USER_KEY_DOWN, WindowImpl};
use crate::platform::win::window::WM_USER_KEY_UP;
use std::{
    cell::Cell,
    mem::zeroed,
    ptr::null_mut,
    rc::{Rc, Weak},
};
use windows_sys::Win32::{
    Foundation::{LPARAM, LRESULT, WPARAM},
    System::Threading::GetCurrentThreadId,
    UI::WindowsAndMessaging::*,
};

thread_local! {
    static HOOK: Cell<Weak<KeyboardHook>> = const { Cell::new(Weak::new()) };
}

pub struct KeyboardHook {
    hhook: HHOOK,
}

impl KeyboardHook {
    pub fn install() -> Rc<Self> {
        // take the hook
        let hook = match HOOK.replace(Weak::new()).upgrade() {
            Some(hook) => hook,

            // if we dont have one, create it
            None => Rc::new(Self {
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

impl Drop for KeyboardHook {
    fn drop(&mut self) {
        // unhook the thread hook
        unsafe { windows_sys::Win32::UI::WindowsAndMessaging::UnhookWindowsHookEx(self.hhook) };

        // so we can deallocate the Rc memory afterwards
        HOOK.set(Weak::new());
    }
}

unsafe extern "system" fn keyboard_hook_proc(
    n_code: i32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    unsafe {
        if n_code == HC_ACTION as i32 && wparam == PM_REMOVE as usize {
            let message = lparam as *mut MSG;

            if matches!((*message).message, WM_KEYDOWN | WM_KEYUP) {
                while WindowImpl::is_our_window((*message).hwnd) {
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

                    if capture {
                        *message = MSG {
                            message: WM_USER,
                            ..zeroed()
                        };

                        return 0;
                    } else {
                        let parent = GetParent((*message).hwnd);
                        if parent.is_null() {
                            break;
                        } else {
                            (*message).hwnd = parent;
                        }
                    }
                }
            }
        }

        CallNextHookEx(null_mut(), n_code, wparam, lparam)
    }
}
