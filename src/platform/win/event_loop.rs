use super::{
    pacer::PacerThread,
    util::{generate_guid, hinstance, is_windows10_or_greater, to_widestring},
    Window,
};
use crate::{
    Command, Decoration, Error, Event, EventResponse, Modifiers, MouseButton, Options, Point,
};
use raw_window_handle::{RawWindowHandle, Win32WindowHandle};
use std::{
    cell::RefCell,
    mem::{self, size_of},
    num::{NonZero, NonZeroIsize},
    ptr::{null, null_mut},
    rc::Rc,
    sync::{mpsc::sync_channel, Arc},
    thread,
};
use windows::Win32::{
    Foundation::{FALSE, HINSTANCE, HWND, LPARAM, LRESULT, POINT, RECT, WPARAM},
    Graphics::Gdi::{ScreenToClient, HBRUSH},
    System::{SystemServices::IMAGE_DOS_HEADER, Threading::GetCurrentThreadId},
    UI::{
        Controls::WM_MOUSELEAVE,
        HiDpi::{SetThreadDpiAwarenessContext, DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE},
        Input::KeyboardAndMouse::{
            GetAsyncKeyState, SetCapture, TrackMouseEvent, TME_LEAVE, TRACKMOUSEEVENT, VK_CAPITAL,
            VK_CONTROL, VK_LWIN, VK_MENU, VK_NUMLOCK, VK_SCROLL, VK_SHIFT,
        },
        WindowsAndMessaging::{
            AdjustWindowRectEx, CallNextHookEx, CreateWindowExW, DefWindowProcW, DispatchMessageW,
            GetMessageW, GetWindowLongPtrW, LoadCursorW, PostMessageW, RegisterClassW,
            SetWindowLongPtrW, SetWindowsHookExW, ShowWindow, TranslateMessage,
            UnhookWindowsHookEx, UnregisterClassW, CS_OWNDC, GWLP_USERDATA, HHOOK, HICON, HMENU,
            IDC_ARROW, MOUSEHOOKSTRUCTEX, MSG, SW_SHOW, WHEEL_DELTA, WH_MOUSE, WINDOW_EX_STYLE,
            WINDOW_STYLE, WM_CLOSE, WM_CREATE, WM_DESTROY, WM_LBUTTONDOWN, WM_LBUTTONUP,
            WM_MBUTTONDOWN, WM_MBUTTONUP, WM_MOUSEHWHEEL, WM_MOUSEMOVE, WM_MOUSEWHEEL, WM_MOVE,
            WM_QUIT, WM_RBUTTONDOWN, WM_RBUTTONUP, WM_SHOWWINDOW, WM_USER, WM_XBUTTONDOWN,
            WM_XBUTTONUP, WNDCLASSW, WS_BORDER, WS_CAPTION, WS_CHILD, WS_CLIPSIBLINGS, WS_DLGFRAME,
            WS_MAXIMIZEBOX, WS_MINIMIZEBOX, WS_OVERLAPPED, WS_OVERLAPPEDWINDOW, WS_POPUP,
            WS_POPUPWINDOW, WS_SIZEBOX, WS_THICKFRAME, WS_VISIBLE, XBUTTON1, XBUTTON2,
        },
    },
};
use windows_core::PCWSTR;

pub const WM_USER_FRAME_TIMER: u32 = WM_USER + 1;
pub const WM_USER_KEY_DOWN: u32 = WM_USER + 2;
pub const WM_USER_KEY_UP: u32 = WM_USER + 3;
pub const WM_USER_MESSAGE: u32 = WM_USER + 4;

pub struct EventLoop {
    shared: Arc<SharedData>,
    handler: Box<dyn FnMut(Event) -> EventResponse + Send>,

    window_handle: HWND,
    window_class: u16,

    mouse_capture: u32,

    pacer: PacerThread,
}

unsafe impl Send for SharedData {}
unsafe impl Sync for SharedData {}
pub struct SharedData {
    hwnd: HWND,
}

impl EventLoop {
    fn create(options: Options) -> Result<Arc<SharedData>, Error> {
        unsafe {
            let parent = match options.parent {
                Some(RawWindowHandle::Win32(win)) => Some(win),
                None => None,
                _ => unreachable!(),
            };

            let class_name = to_widestring(&format!("picoview-{}", generate_guid()));
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

            let mut rect = RECT {
                left: options.position.x as i32,
                top: options.position.y as i32,
                right: (options.position.x + options.size.width) as i32,
                bottom: (options.position.y + options.size.height) as i32,
            };

            let flags = {
                let mut flags = WINDOW_STYLE(0);
                if parent.is_some() {
                    flags |= WS_CHILD;
                }

                if options.visible {
                    flags |= WS_VISIBLE;
                }

                match options.decoration {
                    Decoration::Normal => {
                        flags |= WS_OVERLAPPED | WS_DLGFRAME | WS_MINIMIZEBOX;
                    }

                    Decoration::Borderless => {
                        flags |= WS_POPUP;
                    }
                }

                flags
            };

            if parent.is_none() {
                let _ = AdjustWindowRectEx(&mut rect, flags, FALSE, WINDOW_EX_STYLE(0));
            }

            let hwnd = CreateWindowExW(
                WINDOW_EX_STYLE(0),
                PCWSTR(window_class as _),
                PCWSTR(null()),
                flags,
                rect.left,
                rect.top,
                rect.right - rect.left,
                rect.bottom - rect.top,
                HWND(parent.map(|x| x.hwnd.get()).unwrap_or(0) as _),
                HMENU(null_mut()),
                hinstance(),
                None,
            )
            .unwrap();

            if is_windows10_or_greater() {
                SetThreadDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE);
            }

            let pacer = PacerThread::new(hwnd);
            let shared = Arc::new(SharedData { hwnd });
            let event_loop = Rc::new(RefCell::new(Self {
                shared: shared.clone(),
                handler: options.handler,

                mouse_capture: 0,

                window_class,
                window_handle: hwnd,
                pacer,
            }));

            SetWindowLongPtrW(hwnd, GWLP_USERDATA, Rc::into_raw(event_loop) as _);
            ShowWindow(hwnd, SW_SHOW).unwrap();

            Ok(shared)
        }
    }

    pub fn open(options: Options) -> Result<Arc<SharedData>, Error> {
        if options.parent.is_none() {
            let (sender, receiver) = sync_channel(0);

            thread::spawn(move || match Self::create(options) {
                Ok(shared) => {
                    let _ = sender.send(Ok(shared.clone()));
                    unsafe {
                        let mut msg: MSG = std::mem::zeroed();
                        loop {
                            let status = GetMessageW(&mut msg, shared.hwnd, 0, 0);
                            if !status.as_bool() {
                                break;
                            }

                            let _ = TranslateMessage(&msg);
                            DispatchMessageW(&msg);
                        }
                    }
                }
                Err(e) => {
                    let _ = sender.send(Err(e));
                }
            });

            receiver
                .recv()
                .map_err(|_| Error::PlatformError("the thread is dead?".to_owned()))
                .and_then(|x| x)
        } else {
            Self::create(options)
        }
    }
}

impl Drop for EventLoop {
    fn drop(&mut self) {
        unsafe {
            SetWindowLongPtrW(self.shared.hwnd, GWLP_USERDATA, 0);
            let _ = UnregisterClassW(PCWSTR(self.window_class as _), hinstance());
        }
    }
}

impl SharedData {
    pub fn handle(&self) -> Win32WindowHandle {
        unsafe { Win32WindowHandle::new(NonZero::new_unchecked(self.hwnd.0 as isize)) }
    }

    pub fn post(&self, cmd: Command) {}
}

unsafe extern "system" fn wnd_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    let window_ptr = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *const RefCell<EventLoop>;
    if window_ptr.is_null() {
        return DefWindowProcW(hwnd, msg, wparam, lparam);
    }

    if msg == WM_DESTROY {
        drop(Rc::from_raw(window_ptr));
        return LRESULT(0);
    }

    match msg {
        WM_CLOSE => {
            let mut window = (&*window_ptr).borrow_mut();
            (window.handler)(Event::WindowClose);

            LRESULT(0)
        }

        WM_LBUTTONDOWN | WM_RBUTTONDOWN | WM_MBUTTONDOWN | WM_XBUTTONDOWN => {
            let mut window = (&*window_ptr).borrow_mut();

            let button = match msg {
                WM_LBUTTONDOWN => Some(MouseButton::Left),
                WM_RBUTTONDOWN => Some(MouseButton::Right),
                WM_MBUTTONDOWN => Some(MouseButton::Middle),
                WM_XBUTTONDOWN => match ((wparam.0 >> 16) & 0xffff) as u16 {
                    XBUTTON1 => Some(MouseButton::Back),
                    XBUTTON2 => Some(MouseButton::Forward),
                    _ => None,
                },
                _ => None,
            };

            if let Some(button) = button {
                (window.handler)(Event::MouseDown(button));
            }

            window.mouse_capture += 1;
            if window.mouse_capture == 1 {
                SetCapture(window.window_handle);
            }

            LRESULT(0)
        }

        WM_LBUTTONUP | WM_RBUTTONUP | WM_MBUTTONUP | WM_XBUTTONUP => {
            let mut window = (&*window_ptr).borrow_mut();

            let button = match msg {
                WM_LBUTTONUP => Some(MouseButton::Left),
                WM_RBUTTONUP => Some(MouseButton::Right),
                WM_MBUTTONUP => Some(MouseButton::Middle),
                WM_XBUTTONUP => match ((wparam.0 >> 16) & 0xffff) as u16 {
                    XBUTTON1 => Some(MouseButton::Back),
                    XBUTTON2 => Some(MouseButton::Forward),
                    _ => None,
                },
                _ => None,
            };

            if let Some(button) = button {
                (window.handler)(Event::MouseUp(button));
            }

            window.mouse_capture = window.mouse_capture.saturating_sub(1);
            if window.mouse_capture == 0 {
                SetCapture(HWND(null_mut()));
            }

            LRESULT(0)
        }

        WM_MOUSEWHEEL | WM_MOUSEHWHEEL => {
            let mut window = (&*window_ptr).borrow_mut();

            let wheel_delta: i16 = (wparam.0 >> 16) as i16;
            let wheel_delta = wheel_delta as f32 / WHEEL_DELTA as f32;
            let x: i16 = ((lparam.0 as usize) & 0xFFFF) as i16;
            let y: i16 = ((lparam.0 as usize) >> 16) as i16;
            let mut position = POINT {
                x: x as i32,
                y: y as i32,
            };

            if ScreenToClient(hwnd, &mut position).as_bool() {
                (window.handler)(Event::MouseScroll {
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
            let mut window = (&*window_ptr).borrow_mut();
            (window.handler)(Event::MouseMove(None));

            LRESULT(0)
        }

        WM_MOUSEMOVE => {
            let mut window = (&*window_ptr).borrow_mut();

            let _ = TrackMouseEvent(&mut TRACKMOUSEEVENT {
                cbSize: size_of::<TRACKMOUSEEVENT>() as u32,
                dwFlags: TME_LEAVE,
                hwndTrack: hwnd,
                dwHoverTime: 0,
            });

            let point = Point {
                x: (lparam.0 & 0xFFFF) as i16 as f32,
                y: ((lparam.0 >> 16) & 0xFFFF) as i16 as f32,
            };

            (window.handler)(Event::MouseMove(Some(point)));

            LRESULT(0)
        }

        WM_MOVE => {
            let window = (&*window_ptr).borrow_mut();
            window.pacer.mark_moved();

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
        WM_USER_FRAME_TIMER => {
            let mut window = (&*window_ptr).borrow_mut();
            (window.handler)(Event::Frame);
            LRESULT(0)
        }

        _ => DefWindowProcW(hwnd, msg, wparam, lparam),
    }
}
