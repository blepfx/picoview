use super::{
    cursor::CursorCache,
    pacer::PacerThread,
    util::{generate_guid, hinstance, is_windows10_or_greater, run_event_loop, to_widestring},
    window_hook::WindowKeyboardHook,
};
use crate::{
    Command, Error, Event, EventResponse, MouseButton, MouseCursor, Options, Point, Style,
};
use raw_window_handle::{RawWindowHandle, Win32WindowHandle};
use std::{
    cell::RefCell,
    ffi::OsString,
    mem::size_of,
    num::NonZero,
    os::windows::ffi::OsStringExt,
    ptr::{null, null_mut},
    rc::Rc,
    sync::mpsc::{sync_channel, Receiver, SyncSender},
    thread,
};
use windows::Win32::{
    Foundation::{HINSTANCE, HWND, LPARAM, LRESULT, POINT, RECT, WPARAM},
    Graphics::Gdi::{ClientToScreen, HBRUSH},
    UI::{
        Controls::WM_MOUSELEAVE,
        HiDpi::{SetThreadDpiAwarenessContext, DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE},
        Input::KeyboardAndMouse::{
            GetFocus, SetCapture, SetFocus, TrackMouseEvent, TME_LEAVE, TRACKMOUSEEVENT,
        },
        WindowsAndMessaging::{
            AdjustWindowRectEx, CreateWindowExW, DefWindowProcW, DestroyWindow, GetWindowLongPtrW,
            LoadCursorW, RegisterClassW, SendMessageW, SetCursor, SetCursorPos, SetWindowLongPtrW,
            SetWindowPos, ShowCursor, UnregisterClassW, CS_OWNDC, CW_USEDEFAULT, GWLP_USERDATA,
            HCURSOR, HICON, HMENU, HTCLIENT, IDC_ARROW, SWP_NOMOVE, SWP_NOSIZE, SWP_NOZORDER,
            WHEEL_DELTA, WINDOW_EX_STYLE, WINDOW_STYLE, WM_CLOSE, WM_DESTROY, WM_DPICHANGED,
            WM_KILLFOCUS, WM_LBUTTONDOWN, WM_LBUTTONUP, WM_MBUTTONDOWN, WM_MBUTTONUP,
            WM_MOUSEHWHEEL, WM_MOUSEMOVE, WM_MOUSEWHEEL, WM_MOVE, WM_RBUTTONDOWN, WM_RBUTTONUP,
            WM_SETCURSOR, WM_SETFOCUS, WM_USER, WM_XBUTTONDOWN, WM_XBUTTONUP, WNDCLASSW, WS_CHILD,
            WS_DLGFRAME, WS_MINIMIZEBOX, WS_OVERLAPPED, WS_POPUP, WS_SYSMENU, WS_VISIBLE, XBUTTON1,
            XBUTTON2,
        },
    },
};
use windows_core::PCWSTR;

pub const WM_USER_FRAME_TIMER: u32 = WM_USER + 1;
pub const WM_USER_PULL_COMMANDS: u32 = WM_USER + 2;
pub const WM_USER_HOOK_CHAR: u32 = WM_USER + 3;
pub const WM_USER_HOOK_KEYUP: u32 = WM_USER + 4;
pub const WM_USER_HOOK_KEYDOWN: u32 = WM_USER + 5;
pub const WM_USER_HOOK_KILLFOCUS: u32 = WM_USER + 6;

pub struct WindowMain {
    handler: Box<dyn FnMut(Event) -> EventResponse + Send>,
    cmd_recv: Receiver<Command>,

    mouse_capture: u32,

    window_class: u16,
    window_handle: HWND,

    window_focused_user: bool,
    window_focused_keyboard: bool,

    cursor_cache: CursorCache,
    cursor_current: Option<HCURSOR>,

    thread_pacer: PacerThread,
    window_hook: WindowKeyboardHook,
}

unsafe impl Send for WindowHandle {}
unsafe impl Sync for WindowHandle {}

#[derive(Clone)]
pub struct WindowHandle {
    hwnd: HWND,
    cmd_send: SyncSender<Command>,
}

impl WindowMain {
    fn create(options: Options) -> Result<WindowHandle, Error> {
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

            let dwstyle = style2ws(options.style, parent.is_some());
            let point = options.position.unwrap_or(Point { x: 0.0, y: 0.0 });
            let mut rect = RECT {
                left: point.x as i32,
                top: point.y as i32,
                right: (point.x + options.size.width) as i32,
                bottom: (point.y + options.size.height) as i32,
            };

            let _ = AdjustWindowRectEx(&mut rect, dwstyle, false, WINDOW_EX_STYLE(0));

            let hwnd = CreateWindowExW(
                WINDOW_EX_STYLE(0),
                PCWSTR(window_class as _),
                PCWSTR(null()),
                dwstyle,
                options.position.map(|_| rect.left).unwrap_or(CW_USEDEFAULT),
                options.position.map(|_| rect.top).unwrap_or(CW_USEDEFAULT),
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

            let cursor_cache = CursorCache::load();
            let window_hook = WindowKeyboardHook::new(hwnd)?;
            let pacer = PacerThread::new(hwnd);

            let (cmd_send, cmd_recv) = sync_channel(16);
            let event_loop = Rc::new(RefCell::new(Self {
                cmd_recv,
                handler: options.handler,
                mouse_capture: 0,

                cursor_current: cursor_cache.get(MouseCursor::Default),
                cursor_cache,

                window_focused_user: GetFocus() == hwnd,
                window_focused_keyboard: false,

                window_class,
                window_handle: hwnd,
                window_hook,

                thread_pacer: pacer,
            }));

            SetWindowLongPtrW(hwnd, GWLP_USERDATA, Rc::into_raw(event_loop) as _);

            Ok(WindowHandle { hwnd, cmd_send })
        }
    }

    pub fn open(options: Options) -> Result<WindowHandle, Error> {
        if options.parent.is_none() {
            let (sender, receiver) = sync_channel(0);

            thread::spawn(move || match Self::create(options) {
                Ok(handle) => {
                    let hwnd = handle.hwnd;
                    let _ = sender.send(Ok(handle));

                    unsafe {
                        run_event_loop(hwnd);
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

impl Drop for WindowMain {
    fn drop(&mut self) {
        unsafe {
            SetWindowLongPtrW(self.window_handle, GWLP_USERDATA, 0);
            let _ = UnregisterClassW(PCWSTR(self.window_class as _), hinstance());
        }
    }
}

impl WindowHandle {
    pub fn handle(&self) -> Win32WindowHandle {
        unsafe { Win32WindowHandle::new(NonZero::new_unchecked(self.hwnd.0 as isize)) }
    }

    pub fn post(&self, cmd: Command) {
        let _ = self.cmd_send.send(cmd);
        unsafe {
            SendMessageW(self.hwnd, WM_USER_PULL_COMMANDS, WPARAM(0), LPARAM(0));
        }
    }
}

unsafe extern "system" fn wnd_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    let window_ptr = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *const RefCell<WindowMain>;
    if window_ptr.is_null() {
        return DefWindowProcW(hwnd, msg, wparam, lparam);
    }

    match msg {
        WM_DESTROY => {
            drop(Rc::from_raw(window_ptr));
            LRESULT(0)
        }

        WM_CLOSE => {
            let mut window = (*window_ptr).borrow_mut();
            (window.handler)(Event::WindowClose);

            LRESULT(0)
        }

        WM_LBUTTONDOWN | WM_RBUTTONDOWN | WM_MBUTTONDOWN | WM_XBUTTONDOWN => {
            let mut window = (*window_ptr).borrow_mut();

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
            let mut window = (*window_ptr).borrow_mut();

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
            let mut window = (*window_ptr).borrow_mut();

            let wheel_delta: i16 = (wparam.0 >> 16) as i16;
            let wheel_delta = wheel_delta as f32 / WHEEL_DELTA as f32;

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

            LRESULT(0)
        }

        WM_MOUSELEAVE => {
            let mut window = (*window_ptr).borrow_mut();
            (window.handler)(Event::MouseMove(None));

            LRESULT(0)
        }

        WM_MOUSEMOVE => {
            let mut window = (*window_ptr).borrow_mut();

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
            let window = (*window_ptr).borrow_mut();
            window.thread_pacer.mark_moved();

            LRESULT(0)
        }

        WM_SETCURSOR => {
            let window = (*window_ptr).borrow_mut();
            if lparam.0 as u32 & 0xffff == HTCLIENT {
                match window.cursor_current {
                    Some(hcursor) => {
                        SetCursor(hcursor);
                        ShowCursor(true);
                    }
                    None => {
                        ShowCursor(false);
                    }
                }
            }

            LRESULT(0)
        }

        WM_DPICHANGED => LRESULT(0),

        WM_SETFOCUS => {
            let mut window = (*window_ptr).borrow_mut();

            if !window.window_focused_user {
                window.window_focused_user = true;
                (window.handler)(Event::WindowFocus);
            }

            if window.window_focused_keyboard {
                let _ = SetFocus(window.window_hook.handle());
            }

            LRESULT(0)
        }

        WM_KILLFOCUS | WM_USER_HOOK_KILLFOCUS => {
            if let Ok(mut window) = (*window_ptr).try_borrow_mut() {
                let target = HWND(wparam.0 as _);
                if target != window.window_handle
                    && target != window.window_hook.handle()
                    && window.window_focused_user
                {
                    window.window_focused_user = false;
                    (window.handler)(Event::WindowBlur);
                }
            }

            LRESULT(0)
        }

        WM_USER_HOOK_CHAR => {
            let mut window = (*window_ptr).borrow_mut();
            let string = OsString::from_wide(&[wparam.0 as _]);
            (window.handler)(Event::KeyChar(&string.to_string_lossy()));

            LRESULT(0)
        }

        WM_USER_HOOK_KEYDOWN | WM_USER_HOOK_KEYUP => {
            let mut window = (*window_ptr).borrow_mut();
            let string = OsString::from_wide(&[wparam.0 as _]);
            (window.handler)(Event::KeyChar(&string.to_string_lossy()));

            LRESULT(0)
        }

        WM_USER_FRAME_TIMER => {
            let mut window = (*window_ptr).borrow_mut();
            (window.handler)(Event::Frame);

            LRESULT(0)
        }

        WM_USER_PULL_COMMANDS => {
            let mut window = (*window_ptr).borrow_mut();

            while let Ok(cmd) = window.cmd_recv.try_recv() {
                match cmd {
                    Command::SetCursorIcon(cursor) => {
                        window.cursor_current = window.cursor_cache.get(cursor);
                    }

                    Command::SetCursorPosition(point) => {
                        let mut point = POINT {
                            x: point.x as i32,
                            y: point.y as i32,
                        };

                        if ClientToScreen(window.window_handle, &mut point).as_bool() {
                            let _ = SetCursorPos(point.x, point.y);
                        }
                    }

                    Command::SetSize(size) => {
                        let _ = SetWindowPos(
                            window.window_handle,
                            window.window_handle,
                            0,
                            0,
                            size.width as i32,
                            size.height as i32,
                            SWP_NOZORDER | SWP_NOMOVE,
                        );
                    }

                    Command::SetPosition(point) => {
                        let _ = SetWindowPos(
                            window.window_handle,
                            window.window_handle,
                            point.x as i32,
                            point.y as i32,
                            0,
                            0,
                            SWP_NOZORDER | SWP_NOSIZE,
                        );
                    }

                    Command::SetStyle(style) => {}

                    Command::SetKeyboardInput(focus) => {
                        window.window_focused_keyboard = focus;
                        if window.window_focused_user {
                            if focus {
                                let _ = SetFocus(window.window_hook.handle());
                            } else {
                                let _ = SetFocus(window.window_handle);
                            }
                        }
                    }

                    Command::Close => {
                        let _ = DestroyWindow(window.window_handle);
                    }
                }
            }

            LRESULT(0)
        }

        _ => DefWindowProcW(hwnd, msg, wparam, lparam),
    }
}

fn style2ws(style: Style, child: bool) -> WINDOW_STYLE {
    match (style, child) {
        (Style::Decorated, true) => {
            WS_VISIBLE | WS_CHILD | WS_DLGFRAME | WS_SYSMENU | WS_MINIMIZEBOX
        }
        (Style::Decorated, false) => WS_VISIBLE | WS_DLGFRAME | WS_SYSMENU | WS_MINIMIZEBOX,

        (Style::Borderless, true) => WS_CHILD | WS_VISIBLE,
        (Style::Borderless, false) => WS_POPUP | WS_VISIBLE,

        (Style::Hidden, true) => WS_CHILD,
        (Style::Hidden, false) => WS_OVERLAPPED,

        (Style::BorderlessShadow, true) => todo!(),
        (Style::BorderlessShadow, false) => todo!(),
    }
}
