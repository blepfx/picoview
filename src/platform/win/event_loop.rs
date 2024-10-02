use super::Window;
use crate::{platform::PlatformCommand, Error, Event, EventResponse, Options, Point};
use raw_window_handle::{RawWindowHandle, Win32WindowHandle};
use std::{
    cell::RefCell,
    ffi::OsString,
    mem::size_of,
    num::NonZeroIsize,
    os::windows::ffi::OsStrExt,
    ptr::{null, null_mut},
    rc::Rc,
    sync::Arc,
    time::SystemTime,
};
use windows::Win32::{
    Foundation::{HINSTANCE, HWND, LPARAM, LRESULT, POINT, WPARAM},
    Graphics::Gdi::{ScreenToClient, HBRUSH},
    System::SystemServices::IMAGE_DOS_HEADER,
    UI::{
        Controls::WM_MOUSELEAVE,
        Input::KeyboardAndMouse::{TrackMouseEvent, TME_LEAVE, TRACKMOUSEEVENT},
        WindowsAndMessaging::{
            CreateWindowExW, DefWindowProcW, GetWindowLongPtrW, LoadCursorW, PostMessageW,
            RegisterClassW, SetWindowLongPtrW, UnregisterClassW, CS_OWNDC, GWLP_USERDATA, HICON,
            HMENU, IDC_ARROW, WHEEL_DELTA, WINDOW_EX_STYLE, WINDOW_STYLE, WM_CREATE, WM_DESTROY,
            WM_LBUTTONDOWN, WM_LBUTTONUP, WM_MBUTTONDOWN, WM_MBUTTONUP, WM_MOUSEHWHEEL,
            WM_MOUSEMOVE, WM_MOUSEWHEEL, WM_RBUTTONDOWN, WM_RBUTTONUP, WM_SHOWWINDOW,
            WM_XBUTTONDOWN, WM_XBUTTONUP, WNDCLASSW, WS_CAPTION, WS_CHILD, WS_CLIPSIBLINGS,
            WS_MAXIMIZEBOX, WS_MINIMIZEBOX, WS_POPUPWINDOW, WS_SIZEBOX, WS_VISIBLE,
        },
    },
};
use windows_core::PCWSTR;

pub struct EventLoop {
    window: Window,
    handler: Box<dyn FnMut(&Window, Event) -> EventResponse + Send>,

    window_class: u16,
}

pub struct SharedData {
    handle: Win32WindowHandle,
}

impl EventLoop {
    pub fn open(
        options: Options,
        handler: Box<dyn FnMut(&Window, Event<'_>) -> EventResponse + Send>,
    ) -> Result<Window, Error> {
        unsafe {
            let parent_handle = match options.parent {
                Some(RawWindowHandle::Win32(win)) => Some(win),
                None => None,
                _ => unreachable!(),
            };

            let class_name = to_widestring(&format!(
                "picoview-{}",
                SystemTime::now()
                    .duration_since(SystemTime::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_nanos()
            ));

            let window_class_attributes = WNDCLASSW {
                style: CS_OWNDC,
                lpfnWndProc: Some(wnd_proc),
                cbClsExtra: 0,
                cbWndExtra: 0,
                hInstance: hinstance(),
                hIcon: HICON(null_mut()),
                hCursor: LoadCursorW(HINSTANCE(null_mut()), IDC_ARROW).unwrap(),
                hbrBackground: HBRUSH(null_mut()),
                lpszMenuName: PCWSTR(null()),
                lpszClassName: PCWSTR(class_name.as_ptr()),
            };

            let window_class = RegisterClassW(&window_class_attributes);
            if window_class == 0 {
                return Err(Error::PlatformError(
                    "Failed to register window class".into(),
                ));
            }

            let hwnd = CreateWindowExW(
                WINDOW_EX_STYLE(0),
                PCWSTR(window_class as _),
                PCWSTR(null()),
                if parent_handle.is_some() {
                    WS_VISIBLE | WS_CHILD
                } else {
                    WS_POPUPWINDOW
                        | WS_CAPTION
                        | WS_VISIBLE
                        | WS_SIZEBOX
                        | WS_MINIMIZEBOX
                        | WS_MAXIMIZEBOX
                        | WS_CLIPSIBLINGS
                },
                0,
                0,
                100 as i32,
                100 as i32,
                HWND(parent_handle.map(|x| x.hwnd.get()).unwrap_or(0) as _),
                HMENU(null_mut()),
                hinstance(),
                None,
            )
            .unwrap();

            TrackMouseEvent(&mut TRACKMOUSEEVENT {
                cbSize: size_of::<TRACKMOUSEEVENT>() as u32,
                dwFlags: TME_LEAVE,
                hwndTrack: hwnd,
                dwHoverTime: 0,
            })
            .unwrap();

            let window_handle = Win32WindowHandle::new(NonZeroIsize::new(hwnd.0 as _).unwrap());

            let window = Window {
                shared: Arc::new(SharedData {
                    handle: window_handle,
                }),
            };

            let event_loop = Rc::new(RefCell::new(Self {
                window: window.clone(),
                handler,

                window_class,
            }));

            SetWindowLongPtrW(hwnd, GWLP_USERDATA, Rc::into_raw(event_loop) as _);

            Ok(window)
        }
    }

    fn send_event(&mut self, event: Event) -> EventResponse {
        (self.handler)(&self.window, event)
    }
}

impl Drop for EventLoop {
    fn drop(&mut self) {
        unsafe {
            let hwnd = HWND(self.window.shared.handle.hwnd.get() as _);
            SetWindowLongPtrW(hwnd, GWLP_USERDATA, 0);
            UnregisterClassW(PCWSTR(self.window_class as _), hinstance()).unwrap();
        }
    }
}

impl SharedData {
    pub fn handle(&self) -> Win32WindowHandle {
        self.handle
    }

    pub fn post(&self, cmd: PlatformCommand) {}
}

unsafe extern "system" fn wnd_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    println!("wndproc {}", msg);

    let window_ptr = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *const RefCell<EventLoop>;
    if window_ptr.is_null() {
        return DefWindowProcW(hwnd, msg, wparam, lparam);
    }

    if msg == WM_DESTROY {
        drop(Rc::from_raw(window_ptr));
        return LRESULT(0);
    }

    if msg == WM_CREATE {
        let _ = PostMessageW(hwnd, WM_SHOWWINDOW, WPARAM(0), LPARAM(0));
        return LRESULT(0);
    }

    let mut window = (&*window_ptr).borrow_mut();

    match msg {
        WM_LBUTTONDOWN | WM_RBUTTONDOWN | WM_MBUTTONDOWN | WM_XBUTTONDOWN => LRESULT(0),

        WM_LBUTTONUP | WM_RBUTTONUP | WM_MBUTTONUP | WM_XBUTTONUP => LRESULT(0),

        WM_MOUSEWHEEL | WM_MOUSEHWHEEL => {
            let wheel_delta: i16 = (wparam.0 >> 16) as i16;
            let wheel_delta = wheel_delta as f32 / WHEEL_DELTA as f32;

            let x: i16 = ((lparam.0 as usize) & 0xFFFF) as i16;
            let y: i16 = ((lparam.0 as usize) >> 16) as i16;
            let mut position = POINT {
                x: x as i32,
                y: y as i32,
            };

            if ScreenToClient(hwnd, &mut position).as_bool() {
                window.send_event(Event::MouseMove(Some(Point {
                    x: position.x as f32,
                    y: position.y as f32,
                })));

                window.send_event(Event::MouseScroll {
                    x: if msg == WM_MOUSEWHEEL {
                        0.0
                    } else {
                        wheel_delta
                    },

                    y: if msg == WM_MOUSEWHEEL {
                        wheel_delta
                    } else {
                        0.0
                    },
                });
            }

            LRESULT(0)
        }

        WM_MOUSELEAVE => {
            window.send_event(Event::MouseMove(None));
            LRESULT(0)
        }

        WM_MOUSEMOVE => {
            window.send_event(Event::MouseMove(Some(lparam2point(lparam))));
            LRESULT(0)
        }

        // WM_USER_KEY_DOWN => {
        //     let string = OsString::from_wide(&[wparam.0 as _]);
        //     window.send_event(Event::KeyDown {
        //         text: string.to_string_lossy().to_string(),
        //     });
        //     LRESULT(0)
        // }

        // WM_USER_KEY_UP => {
        //     let string = OsString::from_wide(&[wparam.0 as _]);
        //     window.send_event(Event::KeyUp {
        //         text: string.to_string_lossy().to_string(),
        //     });
        //     LRESULT(0)
        // }

        // WM_USER_FRAME_TIMER => {
        //     // Check modifiers
        //     for &key in MODIFIERS.iter() {
        //         let pressed = GetAsyncKeyState(key.0 as _) != 0;
        //         let was_pressed = window.modifier_pressed[&key.0];

        //         if pressed != was_pressed {
        //             window.modifier_pressed.insert(key.0, pressed);

        //             let string = OsString::from_wide(&[key.0 as _]);
        //             let text = string.to_string_lossy().to_string();

        //             if pressed {
        //                 window.send_event(Event::KeyDown { text });
        //             } else {
        //                 window.send_event(Event::KeyUp { text });
        //             }
        //         }
        //     }

        //     window.send_event(Event::Draw);
        //     LRESULT(0)
        // }
        _ => DefWindowProcW(hwnd, msg, wparam, lparam),
    }
}

// magic stuff

extern "C" {
    static __ImageBase: IMAGE_DOS_HEADER;
}

fn hinstance() -> HINSTANCE {
    unsafe { HINSTANCE(&__ImageBase as *const IMAGE_DOS_HEADER as _) }
}

fn to_widestring(str: &str) -> Vec<u16> {
    OsString::from(str).encode_wide().chain([0]).collect()
}

fn lparam2point(lparam: LPARAM) -> Point {
    Point {
        x: (lparam.0 & 0xFFFF) as i16 as f32,
        y: ((lparam.0 >> 16) & 0xFFFF) as i16 as f32,
    }
}
