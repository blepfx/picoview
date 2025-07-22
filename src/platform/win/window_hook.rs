use super::{
    util::{assert, generate_guid, hinstance, run_event_loop, to_widestring},
    window_main::{WM_USER_HOOK_KEYDOWN, WM_USER_HOOK_KEYUP, WM_USER_HOOK_KILLFOCUS},
};
use crate::Error;
use std::{
    ptr::{null, null_mut},
    thread::spawn,
};
use windows_sys::Win32::{
    Foundation::{HWND, LPARAM, LRESULT, WPARAM},
    UI::WindowsAndMessaging::{
        CS_OWNDC, CreateWindowExW, DefWindowProcW, DestroyWindow, GWLP_USERDATA, GetWindowLongPtrW, PostMessageW,
        RegisterClassW, SetWindowLongPtrW, UnregisterClassW, WM_KEYDOWN, WM_KEYUP, WM_KILLFOCUS, WM_SYSKEYDOWN,
        WM_SYSKEYUP, WNDCLASSW, WS_CHILD, WS_EX_NOACTIVATE,
    },
};

pub struct WindowKeyboardHook {
    hwnd: HWND,
    window_class: u16,
}

impl WindowKeyboardHook {
    pub unsafe fn new(hwnd_main: HWND) -> Result<Self, Error> {
        unsafe {
            let class_name = to_widestring(&format!("picoview-keyboard-{}", generate_guid()));
            let window_class = RegisterClassW(&WNDCLASSW {
                style: CS_OWNDC,
                lpfnWndProc: Some(wnd_proc),
                cbClsExtra: 0,
                cbWndExtra: 0,
                hInstance: hinstance(),
                hIcon: null_mut(),
                hCursor: null_mut(),
                hbrBackground: null_mut(),
                lpszMenuName: null(),
                lpszClassName: class_name.as_ptr(),
            });
            assert(window_class != 0, "hook class create")?;

            let hwnd = CreateWindowExW(
                WS_EX_NOACTIVATE,
                window_class as _,
                null(),
                WS_CHILD,
                0,
                0,
                0,
                0,
                hwnd_main,
                null_mut(),
                hinstance(),
                null(),
            );
            assert(!hwnd.is_null(), "hook window create")?;
            SetWindowLongPtrW(hwnd, GWLP_USERDATA, hwnd_main as _);
            spawn({
                let hwnd = hwnd as usize;
                move || run_event_loop(hwnd as HWND)
            });

            Ok(Self { hwnd, window_class })
        }
    }

    pub fn handle(&self) -> HWND {
        self.hwnd
    }
}

impl Drop for WindowKeyboardHook {
    fn drop(&mut self) {
        unsafe {
            DestroyWindow(self.hwnd);
            UnregisterClassW(&self.window_class, hinstance());
        }
    }
}

unsafe extern "system" fn wnd_proc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    unsafe {
        let hwnd_main = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as HWND;

        match msg {
            WM_KEYDOWN | WM_SYSKEYDOWN => {
                let _ = PostMessageW(hwnd_main, WM_USER_HOOK_KEYDOWN, wparam, lparam);
                0
            }

            WM_KEYUP | WM_SYSKEYUP => {
                let _ = PostMessageW(hwnd_main, WM_USER_HOOK_KEYUP, wparam, lparam);
                0
            }

            WM_KILLFOCUS => {
                let _ = PostMessageW(hwnd_main, WM_USER_HOOK_KILLFOCUS, wparam, lparam);
                0
            }

            _ => DefWindowProcW(hwnd, msg, wparam, lparam),
        }
    }
}
