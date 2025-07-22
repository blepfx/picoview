use super::{
    connection::Connection,
    gl::GlContext,
    util::{
        assert, generate_guid, get_modifiers_async, hinstance, scan_code_to_key, to_widestring,
    },
    window_hook::WindowKeyboardHook,
};
use crate::{
    Error, Event, EventHandler, EventResponse, Modifiers, MouseButton, MouseCursor, Options, Point,
    RawHandle, Size, Style,
    platform::win::util::{from_widestring, run_event_loop},
};
use std::{
    cell::{Cell, RefCell},
    mem::size_of,
    ptr::{copy_nonoverlapping, null, null_mut},
    rc::Rc,
    sync::Arc,
};
use windows_sys::Win32::{
    Foundation::{HWND, LPARAM, LRESULT, POINT, RECT, WPARAM},
    Graphics::Gdi::ClientToScreen,
    System::{
        Com::CoInitialize,
        DataExchange::{
            CloseClipboard, EmptyClipboard, GetClipboardData, OpenClipboard, SetClipboardData,
        },
        Memory::{GMEM_MOVEABLE, GlobalAlloc, GlobalLock, GlobalUnlock},
        Ole::CF_UNICODETEXT,
    },
    UI::{
        Controls::WM_MOUSELEAVE,
        Input::KeyboardAndMouse::{
            GetFocus, SetCapture, SetFocus, TME_LEAVE, TRACKMOUSEEVENT, TrackMouseEvent,
        },
        Shell::ShellExecuteW,
        WindowsAndMessaging::{
            AdjustWindowRectEx, CW_USEDEFAULT, CreateWindowExW, DefWindowProcW, DestroyWindow,
            GWL_EXSTYLE, GWL_STYLE, GWLP_USERDATA, GetWindowLongPtrW, GetWindowLongW, HCURSOR,
            HTCLIENT, IDC_ARROW, LWA_ALPHA, LoadCursorW, PostMessageW, RegisterClassW,
            SW_SHOWDEFAULT, SWP_HIDEWINDOW, SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE, SWP_NOZORDER,
            SWP_SHOWWINDOW, SetCursor, SetCursorPos, SetLayeredWindowAttributes, SetWindowLongPtrW,
            SetWindowPos, ShowCursor, USER_DEFAULT_SCREEN_DPI, UnregisterClassW, WHEEL_DELTA,
            WM_CLOSE, WM_DESTROY, WM_DPICHANGED, WM_KILLFOCUS, WM_LBUTTONDOWN, WM_LBUTTONUP,
            WM_MBUTTONDOWN, WM_MBUTTONUP, WM_MOUSEHWHEEL, WM_MOUSEMOVE, WM_MOUSEWHEEL,
            WM_RBUTTONDOWN, WM_RBUTTONUP, WM_SETCURSOR, WM_SETFOCUS, WM_USER, WM_XBUTTONDOWN,
            WM_XBUTTONUP, WNDCLASSW, WS_BORDER, WS_CAPTION, WS_CHILD, WS_EX_LAYERED,
            WS_MINIMIZEBOX, WS_POPUP, WS_SYSMENU, WS_VISIBLE, XBUTTON1, XBUTTON2,
        },
    },
};

pub const WM_USER_FRAME_PACER: u32 = WM_USER + 1;
pub const WM_USER_KILL_WINDOW: u32 = WM_USER + 2;
pub const WM_USER_HOOK_KEYUP: u32 = WM_USER + 3;
pub const WM_USER_HOOK_KEYDOWN: u32 = WM_USER + 4;
pub const WM_USER_HOOK_KILLFOCUS: u32 = WM_USER + 5;

pub struct WindowMain {
    connection: Arc<Connection>,

    window_hwnd: HWND,
    window_class: u16,
    window_hook: WindowKeyboardHook,

    state_focused_user: Cell<bool>,
    state_focused_keyboard: Cell<bool>,
    state_current_modifiers: Cell<Modifiers>,
    state_current_cursor: Cell<HCURSOR>,
    state_mouse_capture: Cell<u32>,

    handler: RefCell<EventHandler>,
    gl_context: Option<GlContext>,
}

impl WindowMain {
    pub fn open(options: Options) -> Result<(), Error> {
        unsafe {
            let parent = match option.parent {
                Some(RawHandle::Win { hwnd }) => hwnd,
                Some(_) => return Err(Error::PlatformError("invalid parent handle".into())),
                None => null_mut(),
            };

            let connection = Connection::get()?;

            let com_init = CoInitialize(null());
            assert(com_init == 0, "com sta init")?;

            connection.try_set_thread_dpi_awareness_monitor_aware();

            let class_name = to_widestring(&format!("picoview-{}", generate_guid()));
            let window_class = RegisterClassW(&WNDCLASSW {
                style: 0,
                lpfnWndProc: Some(wnd_proc),
                cbClsExtra: 0,
                cbWndExtra: 0,
                hInstance: hinstance(),
                hIcon: null_mut(),
                hCursor: LoadCursorW(null_mut(), IDC_ARROW),
                hbrBackground: null_mut(),
                lpszMenuName: null(),
                lpszClassName: class_name.as_ptr(),
            });
            assert(window_class != 0, "main window class")?;

            let (dwstyle, dwexstyle) = {
                let mut dwstyle = 0;
                let mut dwexstyle = 0;

                if options.style.contains(Style::VISIBLE) {
                    dwstyle |= WS_VISIBLE;
                }

                if options.style.contains(Style::BORDER) {
                    dwstyle |= WS_POPUP | WS_CAPTION | WS_BORDER | WS_SYSMENU | WS_MINIMIZEBOX;
                } else if options.parent.is_none() {
                    dwstyle |= WS_POPUP;
                }

                if options.style.contains(Style::TRANSPARENT) {
                    dwexstyle |= WS_EX_LAYERED;
                }

                if options.parent.is_some() {
                    dwstyle |= WS_CHILD;
                }

                (dwstyle, dwexstyle)
            };

            let point = options.position.unwrap_or(Point { x: 0.0, y: 0.0 });
            let mut rect = RECT {
                left: point.x as i32,
                top: point.y as i32,
                right: point.x as i32 + options.size.width as i32,
                bottom: point.y as i32 + options.size.height as i32,
            };

            AdjustWindowRectEx(&mut rect, dwstyle, 0, dwexstyle);

            let hwnd = CreateWindowExW(
                dwexstyle,
                window_class as _,
                null(),
                dwstyle,
                options.position.map(|_| rect.left).unwrap_or(CW_USEDEFAULT),
                options.position.map(|_| rect.top).unwrap_or(CW_USEDEFAULT),
                rect.right - rect.left,
                rect.bottom - rect.top,
                parent as _,
                null_mut(),
                hinstance(),
                null(),
            );
            assert(!hwnd.is_null(), "main window create")?;

            if options.style.contains(Style::TRANSPARENT) {
                SetLayeredWindowAttributes(hwnd, 0, 255, LWA_ALPHA);
            }

            let window_hook = WindowKeyboardHook::new(hwnd)?;
            let gl_context = match options.opengl {
                Some(config) => match GlContext::new(hwnd, config) {
                    Ok(gl) => Some(gl),
                    Err(_) if config.optional => None,
                    Err(e) => return Err(e),
                },
                None => None,
            };

            let event_loop = Rc::new(Self {
                connection: connection.clone(),

                state_mouse_capture: Cell::new(0),
                state_current_cursor: Cell::new(connection.load_cursor(MouseCursor::Default)),
                state_focused_user: Cell::new(GetFocus() == hwnd),
                state_focused_keyboard: Cell::new(false),
                state_current_modifiers: Cell::new(Modifiers::empty()),

                window_class,
                window_hook,
                window_hwnd: hwnd,

                gl_context,
                handler: RefCell::new(options.handler),
            });

            event_loop.send_event(Event::WindowOpen);
            event_loop.send_event(Event::WindowScale {
                scale: connection.try_get_dpi_for_window(hwnd) as f32
                    / USER_DEFAULT_SCREEN_DPI as f32,
            });

            SetWindowLongPtrW(hwnd, GWLP_USERDATA, Rc::into_raw(event_loop) as _);
            connection.register_pacer(hwnd);

            if options.parent.is_none() {
                run_event_loop(null_mut());
            }

            Ok(())
        }
    }

    fn send_event(&self, event: Event) -> EventResponse {
        if let Ok(mut handler) = self.handler.try_borrow_mut() {
            let mut handle = self;
            handler(event, crate::Window::from_inner(&mut handle))
        } else {
            println!("what the fuck do we do here??");
            EventResponse::Rejected
        }
    }
}

impl Drop for WindowMain {
    fn drop(&mut self) {
        unsafe {
            SetWindowLongPtrW(self.window_hwnd, GWLP_USERDATA, 0);
            UnregisterClassW(self.window_class as _, hinstance());
            self.connection.unregister_pacer(self.window_hwnd);
        }
    }
}

impl<'a> crate::platform::OsWindow for &'a WindowMain {
    fn close(&mut self) {
        unsafe {
            PostMessageW(self.window_hwnd, WM_USER_KILL_WINDOW, 0, 0);
        }
    }

    fn handle(&self) -> RawHandle {
        RawHandle::Win {
            hwnd: self.window_hwnd,
        }
    }

    fn set_cursor_icon(&mut self, cursor: MouseCursor) {
        self.state_current_cursor
            .set(self.connection.load_cursor(cursor));
    }

    fn set_cursor_position(&mut self, point: Point) {
        unsafe {
            let mut point = POINT {
                x: point.x as i32,
                y: point.y as i32,
            };

            if ClientToScreen(self.window_hwnd, &mut point) != 0 {
                SetCursorPos(point.x, point.y);
            }
        }
    }

    fn set_size(&mut self, size: Size) {
        unsafe {
            let dwstyle = GetWindowLongW(self.window_hwnd, GWL_STYLE) as u32;
            let dwexstyle = GetWindowLongW(self.window_hwnd, GWL_EXSTYLE) as u32;

            let mut rect = RECT {
                left: 0,
                top: 0,
                right: size.width as i32,
                bottom: size.height as i32,
            };

            AdjustWindowRectEx(&mut rect, dwstyle, 0, dwexstyle);
            SetWindowPos(
                self.window_hwnd,
                self.window_hwnd,
                0,
                0,
                rect.right - rect.left,
                rect.bottom - rect.top,
                SWP_NOZORDER | SWP_NOMOVE | SWP_NOACTIVATE,
            );
        }
    }

    fn set_position(&mut self, point: Point) {
        unsafe {
            SetWindowPos(
                self.window_hwnd,
                self.window_hwnd,
                point.x as i32,
                point.y as i32,
                0,
                0,
                SWP_NOZORDER | SWP_NOSIZE | SWP_NOACTIVATE,
            );
        }
    }

    fn set_visible(&mut self, visible: bool) {
        unsafe {
            SetWindowPos(
                self.window_hwnd,
                self.window_hwnd,
                0,
                0,
                0,
                0,
                SWP_NOZORDER
                    | SWP_NOSIZE
                    | SWP_NOMOVE
                    | SWP_NOACTIVATE
                    | if visible {
                        SWP_SHOWWINDOW
                    } else {
                        SWP_HIDEWINDOW
                    },
            );
        }
    }

    fn set_keyboard_input(&mut self, focus: bool) {
        if self.state_focused_keyboard.replace(focus) == focus {
            return;
        }

        if self.state_focused_user.get() {
            unsafe {
                SetFocus(if focus {
                    self.window_hook.handle()
                } else {
                    self.window_hwnd
                });
            }
        }
    }

    fn open_url(&mut self, url: &str) -> bool {
        let path = to_widestring(url);
        let verb = to_widestring("open");

        unsafe {
            ShellExecuteW(
                self.window_hwnd,
                verb.as_ptr(),
                path.as_ptr(),
                null(),
                null(),
                SW_SHOWDEFAULT,
            ) as usize
                > 32
        }
    }

    fn get_clipboard_text(&mut self) -> Option<String> {
        unsafe {
            if OpenClipboard(self.window_hwnd) != 0 {
                let data = GetClipboardData(CF_UNICODETEXT as _);
                let result = if !data.is_null() {
                    let data = GlobalLock(data);
                    let result = if !data.is_null() {
                        Some(from_widestring(data as *const u16))
                    } else {
                        None
                    };

                    GlobalUnlock(data);
                    result
                } else {
                    None
                };

                CloseClipboard();
                result
            } else {
                None
            }
        }
    }

    fn set_clipboard_text(&mut self, text: &str) -> bool {
        unsafe {
            if OpenClipboard(self.window_hwnd) != 0 {
                EmptyClipboard();
                let wide = to_widestring(&text);
                let buf = GlobalAlloc(GMEM_MOVEABLE, (wide.len() + 1) * size_of::<u16>());
                let buf = GlobalLock(buf) as *mut u16;
                copy_nonoverlapping(wide.as_ptr(), buf, wide.len());
                buf.add(wide.len()).write(0);
                GlobalUnlock(buf as *mut _);
                SetClipboardData(CF_UNICODETEXT as _, buf as *mut _);
                CloseClipboard();
                return true;
            }
        }

        false
    }
}

unsafe extern "system" fn wnd_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    unsafe {
        let window_ptr = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *const WindowMain;
        if window_ptr.is_null() {
            return DefWindowProcW(hwnd, msg, wparam, lparam);
        }

        match msg {
            WM_DESTROY => {
                drop(Rc::from_raw(window_ptr));
                0
            }

            WM_CLOSE => {
                (*window_ptr).send_event(Event::WindowClose);
                0
            }

            WM_DPICHANGED => {
                let dpi = (wparam & 0xFFFF) as u16 as u32;
                let scale = dpi as f32 / USER_DEFAULT_SCREEN_DPI as f32;

                (*window_ptr).send_event(Event::WindowScale { scale });
                0
            }

            WM_LBUTTONDOWN | WM_RBUTTONDOWN | WM_MBUTTONDOWN | WM_XBUTTONDOWN => {
                let window = &*window_ptr;

                let button = match msg {
                    WM_LBUTTONDOWN => Some(MouseButton::Left),
                    WM_RBUTTONDOWN => Some(MouseButton::Right),
                    WM_MBUTTONDOWN => Some(MouseButton::Middle),
                    WM_XBUTTONDOWN => match ((wparam >> 16) & 0xffff) as u16 {
                        XBUTTON1 => Some(MouseButton::Back),
                        XBUTTON2 => Some(MouseButton::Forward),
                        _ => None,
                    },
                    _ => None,
                };

                if let Some(button) = button {
                    window.send_event(Event::MouseDown { button });
                };

                if window
                    .state_mouse_capture
                    .replace(window.state_mouse_capture.get() + 1)
                    == 0
                {
                    SetCapture(window.window_hwnd);
                }

                0
            }

            WM_LBUTTONUP | WM_RBUTTONUP | WM_MBUTTONUP | WM_XBUTTONUP => {
                let window = &*window_ptr;

                let button = match msg {
                    WM_LBUTTONUP => Some(MouseButton::Left),
                    WM_RBUTTONUP => Some(MouseButton::Right),
                    WM_MBUTTONUP => Some(MouseButton::Middle),
                    WM_XBUTTONUP => match ((wparam >> 16) & 0xffff) as u16 {
                        XBUTTON1 => Some(MouseButton::Back),
                        XBUTTON2 => Some(MouseButton::Forward),
                        _ => None,
                    },
                    _ => None,
                };

                if let Some(button) = button {
                    window.send_event(Event::MouseUp { button });
                }

                if window
                    .state_mouse_capture
                    .replace(window.state_mouse_capture.get().saturating_sub(1))
                    != 0
                {
                    SetCapture(null_mut());
                }

                0
            }

            WM_MOUSEWHEEL | WM_MOUSEHWHEEL => {
                let wheel_delta: i16 = (wparam >> 16) as i16;
                let wheel_delta = wheel_delta as f32 / WHEEL_DELTA as f32;

                (*window_ptr).send_event(Event::MouseScroll {
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
                0
            }

            WM_MOUSELEAVE => {
                (*window_ptr).send_event(Event::MouseMove { cursor: None });
                0
            }

            WM_MOUSEMOVE => {
                let _ = TrackMouseEvent(&mut TRACKMOUSEEVENT {
                    cbSize: size_of::<TRACKMOUSEEVENT>() as u32,
                    dwFlags: TME_LEAVE,
                    hwndTrack: hwnd,
                    dwHoverTime: 0,
                });

                let point = Point {
                    x: (lparam & 0xFFFF) as i16 as f32,
                    y: ((lparam >> 16) & 0xFFFF) as i16 as f32,
                };

                (*window_ptr).send_event(Event::MouseMove {
                    cursor: Some(point),
                });
                0
            }

            WM_SETCURSOR => {
                let window = &*window_ptr;

                if lparam as u32 & 0xffff == HTCLIENT {
                    let cursor = window.state_current_cursor.get();

                    if cursor.is_null() {
                        ShowCursor(0);
                    } else {
                        SetCursor(cursor);
                        ShowCursor(1);
                    }

                    1
                } else {
                    DefWindowProcW(hwnd, msg, wparam, lparam)
                }
            }

            WM_SETFOCUS => {
                let window = &*window_ptr;

                if window.state_focused_user.replace(true) == false {
                    window.send_event(Event::WindowFocus);
                }

                if window.state_focused_keyboard.get() {
                    let hwnd = window.window_hook.handle();
                    let _ = SetFocus(hwnd);
                }

                0
            }

            WM_KILLFOCUS | WM_USER_HOOK_KILLFOCUS => {
                let window = &*window_ptr;

                let target = wparam as HWND;
                if target != window.window_hwnd
                    && target != window.window_hook.handle()
                    && window.state_focused_user.replace(false) == true
                {
                    window.send_event(Event::WindowBlur);
                }

                0
            }

            WM_USER_HOOK_KEYDOWN | WM_USER_HOOK_KEYUP => {
                let window = &*window_ptr;

                let scan_code = ((lparam & 0x1ff_0000) >> 16) as u32;
                if let Some(key) = scan_code_to_key(scan_code) {
                    if msg == WM_USER_HOOK_KEYDOWN {
                        window.send_event(Event::KeyDown { key });
                    } else {
                        window.send_event(Event::KeyUp { key });
                    }
                }

                0
            }

            WM_USER_FRAME_PACER => {
                let window = &*window_ptr;

                let modifiers = get_modifiers_async();
                if window.state_current_modifiers.replace(modifiers) != modifiers {
                    window.send_event(Event::KeyModifiers { modifiers });
                }

                if let Some(context) = &window.gl_context {
                    if context.set_current(true) {
                        window.send_event(Event::WindowFrame { gl: Some(context) });
                        context.set_current(false);
                    } else {
                        window.send_event(Event::WindowFrame { gl: None });
                    }
                } else {
                    window.send_event(Event::WindowFrame { gl: None });
                }

                0
            }

            WM_USER_KILL_WINDOW => {
                DestroyWindow(hwnd);
                0
            }

            _ => DefWindowProcW(hwnd, msg, wparam, lparam),
        }
    }
}
