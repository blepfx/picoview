use crate::platform::win::util::{Win32Error, hinstance, to_widestring};
use std::ptr::null_mut;
use std::rc::Rc;
use windows_sys::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
use windows_sys::Win32::System::Com::CoCreateGuid;
use windows_sys::Win32::UI::WindowsAndMessaging::{
    CW_USEDEFAULT, CreateWindowExW, DefWindowProcW, DestroyWindow, GCW_ATOM, GWLP_USERDATA,
    GetClassLongW, GetWindowLongPtrW, IDC_ARROW, LoadCursorW, RegisterClassW, SetWindowLongPtrW,
    UnregisterClassW, WINDOW_STYLE, WM_DESTROY, WNDCLASSW,
};
use windows_sys::core::GUID;

/// A trait that represents a window procedure, `WndProc` will be redirected
/// here. See [`create_window`] for more info.
pub trait WindowProc: 'static {
    unsafe fn window_proc(&self, hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT;
}

impl WindowProc for () {
    unsafe fn window_proc(&self, hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
        unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) }
    }
}

/// Creates a new window with the given style and parent, and calls the provided
/// closure to create the window handler.
///
/// The handler will be dropped when the window itself is destroyed
/// (`WM_DESTROY`).
///
/// # Safety
/// - Parent window must be either a valid window at the time of the call, or
///   null.
pub unsafe fn create_window<W: WindowProc, E: From<Win32Error>>(
    dwstyle: WINDOW_STYLE,
    parent: HWND,
    f: impl FnOnce(HWND) -> Result<Rc<W>, E>,
) -> Result<Rc<W>, E> {
    unsafe extern "system" fn wnd_proc<W: WindowProc>(
        hwnd: HWND,
        msg: u32,
        wparam: WPARAM,
        lparam: LPARAM,
    ) -> LRESULT {
        unsafe {
            let window = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *const W;
            if window.is_null() {
                // window not yet initialized (or already uninitialized), just pass the message
                // to the default window proc
                return DefWindowProcW(hwnd, msg, wparam, lparam);
            }

            // call the window proc
            let result = (*window).window_proc(hwnd, msg, wparam, lparam);

            // window is getting destroyed.. drop it!
            if msg == WM_DESTROY {
                // drop our userdata
                let _ = Rc::from_raw(window);
                // clear the userdata so we dont try to use it again
                SetWindowLongPtrW(hwnd, GWLP_USERDATA, 0);

                // get our window class so we can unregister it
                let class = GetClassLongW(hwnd, GCW_ATOM);
                // check just in case
                if class != 0 {
                    // unregister the window class so we dont leak it
                    UnregisterClassW(class as _, hinstance());
                }

                return 0;
            }

            result
        }
    }

    unsafe {
        // our hinstance, its always the same
        let hinstance = hinstance();

        // unique class name to avoid conflicts with other windows
        let class_name = to_widestring(&format!("picoview-{}", generate_guid()));
        let window_class = RegisterClassW(&WNDCLASSW {
            style: 0,
            lpfnWndProc: Some(wnd_proc::<W>),
            cbClsExtra: 0,
            cbWndExtra: 0,
            hInstance: hinstance,
            hIcon: null_mut(),
            hCursor: LoadCursorW(null_mut(), IDC_ARROW),
            hbrBackground: null_mut(),
            lpszMenuName: null_mut(),
            lpszClassName: class_name.as_ptr(),
        });

        // if failed, return an error
        if window_class == 0 {
            return Err(Win32Error::last_error()
                .with_context("RegisterClassW")
                .into());
        }

        // new zero size zero style window (we can resize & set it later)
        let window_hwnd = CreateWindowExW(
            0,
            window_class as _,
            [0].as_ptr() as _,
            dwstyle,
            CW_USEDEFAULT,
            CW_USEDEFAULT,
            0,
            0,
            parent,
            null_mut(),
            hinstance,
            null_mut(),
        );

        // if failed, unregister the class and return an error
        if window_hwnd.is_null() {
            UnregisterClassW(window_class as _, hinstance);
            return Err(Win32Error::last_error()
                .with_context("CreateWindowExW")
                .into());
        }

        // create our window handler... some messages will be sent immediately and
        // therefore lost, but it seems to work fine for now (nothing important is
        // lost).
        let window = match f(window_hwnd) {
            Ok(window) => window,
            Err(e) => {
                // initialization failed, cleanup and return the error
                DestroyWindow(window_hwnd);
                UnregisterClassW(window_class as _, hinstance);
                return Err(e);
            }
        };

        // set it so it is accessible from the window proc
        let result = SetWindowLongPtrW(
            window_hwnd,
            GWLP_USERDATA,
            Rc::into_raw(window.clone()) as _,
        );

        // SetWindowLongPtrW failed?
        if result != 0 {
            DestroyWindow(window_hwnd);
            UnregisterClassW(window_class as _, hinstance);
            return Err(Win32Error::last_error()
                .with_context("SetWindowLongPtrW")
                .into());
        }

        // all good, man
        Ok(window)
    }
}

/// Generates a new GUID and returns it as a string.
///
/// Used for random unique window class name generation.
fn generate_guid() -> String {
    unsafe {
        let mut guid = std::mem::zeroed::<GUID>();
        CoCreateGuid(&mut guid);

        format!(
            "{:0X}{:0X}{:0X}{:0X}{:0X}{:0X}{:0X}{:0X}{:0X}{:0X}{:0X}",
            guid.data1,
            guid.data2,
            guid.data3,
            guid.data4[0],
            guid.data4[1],
            guid.data4[2],
            guid.data4[3],
            guid.data4[4],
            guid.data4[5],
            guid.data4[6],
            guid.data4[7]
        )
    }
}
