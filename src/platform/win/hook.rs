use super::window::{WM_USER_KEY_DOWN, WM_USER_KEY_UP};
use std::{
    cell::{Cell, RefCell},
    collections::HashSet,
    mem::zeroed,
    ptr::null_mut,
    rc::{Rc, Weak},
};
use windows_sys::Win32::{
    Foundation::{HWND, LPARAM, LRESULT, WPARAM},
    System::Threading::GetCurrentThreadId,
    UI::WindowsAndMessaging::*,
};

thread_local! {
    static HOOK: Cell<Weak<KeyboardHook>> = const { Cell::new(Weak::new()) };
}

pub struct KeyboardHook {
    hhook: HHOOK,
    windows: RefCell<HashSet<usize>>,
}

impl KeyboardHook {
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

    pub fn add_window(&self, hwnd: HWND) {
        self.windows.borrow_mut().insert(hwnd.addr());
    }

    pub fn remove_window(&self, hwnd: HWND) {
        self.windows.borrow_mut().remove(&hwnd.addr());
    }

    pub fn has_window(&self, hwnd: HWND) -> bool {
        self.windows.borrow().contains(&hwnd.addr())
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
    fn is_our_window(hwnd: HWND) -> bool {
        HOOK.get()
            .upgrade()
            .map_or(false, |hook| hook.has_window(hwnd))
    }

    unsafe {
        if n_code == HC_ACTION as i32 && wparam == PM_REMOVE as usize {
            let message = lparam as *mut MSG;

            if matches!((*message).message, WM_KEYDOWN | WM_KEYUP) {
                while is_our_window((*message).hwnd) {
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
