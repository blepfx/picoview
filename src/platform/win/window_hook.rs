use super::{
    util::{generate_guid, hinstance, run_event_loop, to_widestring},
    window_main::{
        WM_USER_HOOK_CHAR, WM_USER_HOOK_KEYDOWN, WM_USER_HOOK_KEYUP, WM_USER_HOOK_KILLFOCUS,
    },
};
use crate::Error;
use std::{
    ptr::{null, null_mut},
    thread::spawn,
};
use windows::{
    core::PCWSTR,
    Win32::{
        Foundation::{HWND, LPARAM, LRESULT, WPARAM},
        Graphics::Gdi::HBRUSH,
        UI::WindowsAndMessaging::{
            CreateWindowExW, DefWindowProcW, DestroyWindow, GetWindowLongPtrW, PostMessageW,
            RegisterClassW, SetWindowLongPtrW, UnregisterClassW, CS_OWNDC, GWLP_USERDATA, HCURSOR,
            HICON, HMENU, WM_CHAR, WM_KEYDOWN, WM_KEYUP, WM_KILLFOCUS, WNDCLASSW, WS_CHILD,
            WS_EX_NOACTIVATE,
        },
    },
};

pub struct WindowKeyboardHook {
    hwnd: HWND,
    window_class: u16,
}

impl WindowKeyboardHook {
    pub fn new(hwnd_main: HWND) -> Result<Self, Error> {
        let class_name = to_widestring(&format!("picoview-keyboard-{}", generate_guid()));
        let window_class_attributes = WNDCLASSW {
            style: CS_OWNDC,
            lpfnWndProc: Some(wnd_proc),
            cbClsExtra: 0,
            cbWndExtra: 0,
            hInstance: hinstance(),
            hIcon: HICON(null_mut()),
            hCursor: HCURSOR(null_mut()),
            hbrBackground: HBRUSH(null_mut()),
            lpszMenuName: PCWSTR(null()),
            lpszClassName: PCWSTR(class_name.as_ptr()),
        };

        let window_class = unsafe { RegisterClassW(&window_class_attributes) };
        if window_class == 0 {
            return Err(Error::PlatformError(
                "Failed to register window class".into(),
            ));
        }

        let hwnd = unsafe {
            CreateWindowExW(
                WS_EX_NOACTIVATE,
                PCWSTR(window_class as _),
                PCWSTR(null()),
                WS_CHILD,
                0,
                0,
                0,
                0,
                hwnd_main,
                HMENU(null_mut()),
                hinstance(),
                None,
            )
            .unwrap()
        };

        unsafe { SetWindowLongPtrW(hwnd, GWLP_USERDATA, hwnd_main.0 as _) };

        spawn({
            let hwnd = hwnd.0 as usize;
            move || unsafe {
                run_event_loop(HWND(hwnd as _));
            }
        });

        Ok(Self { hwnd, window_class })
    }

    pub fn handle(&self) -> HWND {
        self.hwnd
    }
}

impl Drop for WindowKeyboardHook {
    fn drop(&mut self) {
        unsafe {
            let _ = DestroyWindow(self.hwnd);
            let _ = UnregisterClassW(PCWSTR(self.window_class as _), hinstance());
        }
    }
}

unsafe extern "system" fn wnd_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    let hwnd_main = unsafe { HWND(GetWindowLongPtrW(hwnd, GWLP_USERDATA) as _) };

    match msg {
        WM_CHAR => {
            let _ = PostMessageW(hwnd_main, WM_USER_HOOK_CHAR, wparam, lparam);
            LRESULT(0)
        }

        WM_KEYDOWN => {
            let _ = PostMessageW(hwnd_main, WM_USER_HOOK_KEYDOWN, wparam, lparam);
            LRESULT(0)
        }

        WM_KEYUP => {
            let _ = PostMessageW(hwnd_main, WM_USER_HOOK_KEYUP, wparam, lparam);
            LRESULT(0)
        }

        WM_KILLFOCUS => {
            let _ = PostMessageW(hwnd_main, WM_USER_HOOK_KILLFOCUS, wparam, lparam);
            LRESULT(0)
        }

        _ => DefWindowProcW(hwnd, msg, wparam, lparam),
    }
}
