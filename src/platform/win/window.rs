use super::{
    gl::GlContext,
    shared::Win32Shared,
    util::{
        check_error, from_widestring, generate_guid, get_modifiers_async, hinstance,
        run_event_loop, scan_code_to_key, to_widestring,
    },
};
use crate::{
    Error, Event, Modifiers, MouseButton, MouseCursor, Point, Size, Window, WindowBuilder,
    WindowHandler,
    platform::{OpenMode, win::util::window_size_from_client_size},
    rwh_06,
};
use std::{
    cell::{Cell, RefCell},
    collections::VecDeque,
    mem::size_of,
    num::NonZeroIsize,
    ptr::{copy_nonoverlapping, null, null_mut},
    sync::Arc,
};
use windows_sys::Win32::{
    Foundation::{HWND, LPARAM, LRESULT, POINT, WPARAM},
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
        Input::KeyboardAndMouse::{SetCapture, TME_LEAVE, TRACKMOUSEEVENT, TrackMouseEvent},
        Shell::ShellExecuteW,
        WindowsAndMessaging::{
            CW_USEDEFAULT, CreateWindowExW, DefWindowProcW, DestroyWindow, GWL_STYLE,
            GWLP_USERDATA, GWLP_WNDPROC, GetWindowLongPtrW, GetWindowLongW, HCURSOR, HTCLIENT,
            IDC_ARROW, LoadCursorW, MINMAXINFO, PostMessageW, PostQuitMessage, RegisterClassW,
            SW_SHOWDEFAULT, SWP_HIDEWINDOW, SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE, SWP_NOZORDER,
            SWP_SHOWWINDOW, SetCursor, SetCursorPos, SetWindowLongPtrW, SetWindowPos,
            SetWindowTextW, ShowCursor, USER_DEFAULT_SCREEN_DPI, UnregisterClassW, WHEEL_DELTA,
            WM_DESTROY, WM_DPICHANGED, WM_GETMINMAXINFO, WM_KILLFOCUS, WM_LBUTTONDOWN,
            WM_LBUTTONUP, WM_MBUTTONDOWN, WM_MBUTTONUP, WM_MOUSEHWHEEL, WM_MOUSEMOVE,
            WM_MOUSEWHEEL, WM_MOVE, WM_RBUTTONDOWN, WM_RBUTTONUP, WM_SETCURSOR, WM_SETFOCUS,
            WM_SIZE, WM_USER, WM_XBUTTONDOWN, WM_XBUTTONUP, WNDCLASSW, WS_CHILD,
            WS_OVERLAPPEDWINDOW, WS_POPUP, WS_SIZEBOX, WS_VISIBLE, XBUTTON1, XBUTTON2,
        },
    },
};

pub const WM_USER_FRAME_PACER: u32 = WM_USER + 1;
pub const WM_USER_KILL_WINDOW: u32 = WM_USER + 2;
pub const WM_USER_KEY_DOWN: u32 = WM_USER + 3;
pub const WM_USER_KEY_UP: u32 = WM_USER + 4;

pub struct WindowImpl {
    inner: WindowInner,
    gl_context: Option<GlContext>,

    event_handler: RefCell<Box<dyn WindowHandler>>,
    event_queue: RefCell<VecDeque<Event<'static>>>,
}

pub struct WindowInner {
    shared: Arc<Win32Shared>,

    window_hwnd: HWND,
    window_class: u16,

    owns_event_loop: bool,
    min_window_size: POINT,
    max_window_size: POINT,

    state_focused: Cell<bool>,
    state_current_modifiers: Cell<Modifiers>,
    state_current_cursor: Cell<HCURSOR>,
    state_mouse_capture: Cell<u32>,
}

impl WindowImpl {
    pub unsafe fn is_our_window(hwnd: HWND) -> bool {
        unsafe { GetWindowLongPtrW(hwnd, GWLP_WNDPROC) == wnd_proc as usize as isize }
    }

    pub unsafe fn open(options: WindowBuilder, mode: OpenMode) -> Result<(), Error> {
        unsafe {
            let parent = match mode {
                OpenMode::Embedded(rwh_06::RawWindowHandle::Win32(window)) => {
                    window.hwnd.get() as HWND
                }
                OpenMode::Embedded(_) => {
                    return Err(Error::InvalidParent);
                }
                OpenMode::Blocking => null_mut(),
            };

            let shared = Win32Shared::get()?;

            if parent.is_null() {
                let com_init = CoInitialize(null());
                check_error(com_init == 0, "com sta init")?;
            }

            shared.try_set_thread_dpi_awareness_monitor_aware();

            let class_name = to_widestring(&format!("picoview-{}", generate_guid()));
            let window_title = to_widestring(&options.title);

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
            check_error(window_class != 0, "main window class")?;

            let dwstyle = {
                let mut dwstyle = 0;

                match mode {
                    OpenMode::Blocking => {
                        if options.decorations {
                            dwstyle |= WS_OVERLAPPEDWINDOW;
                        } else {
                            dwstyle |= WS_POPUP;
                        }

                        if options.resizable.is_none() {
                            dwstyle &= !WS_SIZEBOX;
                        }
                    }

                    OpenMode::Embedded(..) => {
                        dwstyle |= WS_CHILD;
                    }
                }

                if options.visible {
                    dwstyle |= WS_VISIBLE;
                }

                dwstyle
            };

            let size = window_size_from_client_size(options.size, dwstyle);

            let hwnd = CreateWindowExW(
                0,
                window_class as _,
                window_title.as_ptr() as _,
                dwstyle,
                options
                    .position
                    .map(|pos| pos.x as i32)
                    .unwrap_or(CW_USEDEFAULT),
                options
                    .position
                    .map(|pos| pos.y as i32)
                    .unwrap_or(CW_USEDEFAULT),
                size.x,
                size.y,
                parent as _,
                null_mut(),
                hinstance(),
                null(),
            );
            check_error(!hwnd.is_null(), "main window create")?;

            let gl_context = match options.opengl {
                Some(config) => match GlContext::new(hwnd, config) {
                    Ok(gl) => Some(gl),
                    Err(_) if config.optional => None,
                    Err(e) => return Err(e),
                },
                None => None,
            };

            let inner = WindowInner {
                shared: shared.clone(),

                state_mouse_capture: Cell::new(0),
                state_current_cursor: Cell::new(shared.load_cursor(MouseCursor::Default)),
                state_current_modifiers: Cell::new(Modifiers::empty()),
                state_focused: Cell::new(false),

                window_class,
                window_hwnd: hwnd,

                owns_event_loop: matches!(mode, OpenMode::Blocking),
                max_window_size: window_size_from_client_size(
                    options
                        .resizable
                        .as_ref()
                        .map(|x| x.end)
                        .unwrap_or(Size::MAX),
                    dwstyle,
                ),
                min_window_size: window_size_from_client_size(
                    options
                        .resizable
                        .as_ref()
                        .map(|x| x.start)
                        .unwrap_or(Size::MIN),
                    dwstyle,
                ),
            };

            let event_loop = Box::new(Self {
                event_handler: RefCell::new((options.factory)(Window(&inner))),
                event_queue: RefCell::new(VecDeque::new()),
                gl_context,
                inner,
            });

            event_loop.send_event(Event::WindowScale {
                scale: shared.try_get_dpi_for_window(hwnd) as f32 / USER_DEFAULT_SCREEN_DPI as f32,
            });

            SetWindowLongPtrW(hwnd, GWLP_USERDATA, Box::into_raw(event_loop) as _);
            shared.register_pacer(hwnd);

            if matches!(mode, OpenMode::Blocking) {
                run_event_loop(null_mut());
            }

            Ok(())
        }
    }

    fn send_event(&self, event: Event) {
        if let Ok(mut handler) = self.event_handler.try_borrow_mut() {
            handler.on_event(event, Window(&self.inner));

            for event in self.event_queue.borrow_mut().drain(..) {
                handler.on_event(event, Window(&self.inner));
            }
        } else if cfg!(debug_assertions) {
            panic!("send_event reentrancy")
        }
    }

    fn send_event_defer(&self, event: Event<'static>) {
        if self.event_handler.try_borrow_mut().is_ok() {
            self.send_event(event);
        } else {
            self.event_queue.borrow_mut().push_back(event);
        }
    }
}

impl Drop for WindowImpl {
    fn drop(&mut self) {
        // drop the handler here, so it could do clean up when the window is still alive
        drop(
            self.event_handler
                .replace(Box::new(|_: Event<'_>, _: crate::Window<'_>| {})),
        );

        unsafe {
            SetWindowLongPtrW(self.inner.window_hwnd, GWLP_USERDATA, 0);
            UnregisterClassW(self.inner.window_class as _, hinstance());
            self.inner.shared.unregister_pacer(self.inner.window_hwnd);
        }
    }
}

impl crate::platform::OsWindow for WindowInner {
    fn close(&self) {
        unsafe {
            PostMessageW(self.window_hwnd, WM_USER_KILL_WINDOW, 0, 0);
        }
    }

    fn window_handle(&self) -> rwh_06::RawWindowHandle {
        unsafe {
            let mut handle = rwh_06::Win32WindowHandle::new(NonZeroIsize::new_unchecked(
                self.window_hwnd as isize,
            ));
            handle.hinstance = NonZeroIsize::new(hinstance() as isize);
            rwh_06::RawWindowHandle::Win32(handle)
        }
    }

    fn display_handle(&self) -> rwh_06::RawDisplayHandle {
        rwh_06::RawDisplayHandle::Windows(rwh_06::WindowsDisplayHandle::new())
    }

    fn set_title(&self, title: &str) {
        unsafe {
            let window_title = to_widestring(title);
            SetWindowTextW(self.window_hwnd, window_title.as_ptr() as _);
        }
    }

    fn set_cursor_icon(&self, cursor: MouseCursor) {
        self.state_current_cursor
            .set(self.shared.load_cursor(cursor));
    }

    fn set_cursor_position(&self, point: Point) {
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

    fn set_size(&self, size: Size) {
        unsafe {
            let dwstyle = GetWindowLongW(self.window_hwnd, GWL_STYLE) as u32;
            let size = window_size_from_client_size(size, dwstyle);

            SetWindowPos(
                self.window_hwnd,
                self.window_hwnd,
                0,
                0,
                size.x,
                size.y,
                SWP_NOZORDER | SWP_NOMOVE | SWP_NOACTIVATE,
            );
        }
    }

    fn set_position(&self, point: Point) {
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

    fn set_visible(&self, visible: bool) {
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

    fn open_url(&self, url: &str) -> bool {
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

    fn get_clipboard_text(&self) -> Option<String> {
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

    fn set_clipboard_text(&self, text: &str) -> bool {
        unsafe {
            if OpenClipboard(self.window_hwnd) != 0 {
                EmptyClipboard();
                let wide = to_widestring(text);
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
        let window_ptr = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut WindowImpl;
        if window_ptr.is_null() {
            return DefWindowProcW(hwnd, msg, wparam, lparam);
        }

        if msg == WM_DESTROY {
            if (*window_ptr).inner.owns_event_loop {
                PostQuitMessage(0);
            }

            drop(Box::from_raw(window_ptr));
            return 0;
        }

        let window = &*window_ptr;
        match msg {
            WM_MOVE => {
                let x = ((lparam >> 0) & 0xFFFF) as i16 as f32;
                let y = ((lparam >> 16) & 0xFFFF) as i16 as f32;
                window.send_event_defer(Event::WindowMove {
                    origin: Point { x, y },
                });

                0
            }

            WM_SIZE => {
                let width = ((lparam >> 0) & 0xFFFF) as u32;
                let height = ((lparam >> 16) & 0xFFFF) as u32;
                window.send_event_defer(Event::WindowResize {
                    size: Size { width, height },
                });

                0
            }

            WM_DPICHANGED => {
                let dpi = (wparam & 0xFFFF) as u16 as u32;
                let scale = dpi as f32 / USER_DEFAULT_SCREEN_DPI as f32;
                window.send_event_defer(Event::WindowScale { scale });
                0
            }

            WM_LBUTTONDOWN | WM_RBUTTONDOWN | WM_MBUTTONDOWN | WM_XBUTTONDOWN => {
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
                    window.send_event_defer(Event::MouseDown { button });
                };

                if window
                    .inner
                    .state_mouse_capture
                    .replace(window.inner.state_mouse_capture.get() + 1)
                    == 0
                {
                    SetCapture(window.inner.window_hwnd);
                }

                0
            }

            WM_LBUTTONUP | WM_RBUTTONUP | WM_MBUTTONUP | WM_XBUTTONUP => {
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
                    window.send_event_defer(Event::MouseUp { button });
                }

                if window
                    .inner
                    .state_mouse_capture
                    .replace(window.inner.state_mouse_capture.get().saturating_sub(1))
                    != 0
                {
                    SetCapture(null_mut());
                }

                0
            }

            WM_MOUSEWHEEL | WM_MOUSEHWHEEL => {
                let wheel_delta: i16 = (wparam >> 16) as i16;
                let wheel_delta = wheel_delta as f32 / WHEEL_DELTA as f32;

                window.send_event_defer(Event::MouseScroll {
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
                window.send_event_defer(Event::MouseLeave);
                0
            }

            WM_MOUSEMOVE => {
                let _ = TrackMouseEvent(&mut TRACKMOUSEEVENT {
                    cbSize: size_of::<TRACKMOUSEEVENT>() as u32,
                    dwFlags: TME_LEAVE,
                    hwndTrack: hwnd,
                    dwHoverTime: 0,
                });

                let relative_x = (lparam & 0xFFFF) as i16;
                let relative_y = ((lparam >> 16) & 0xFFFF) as i16;

                let mut absolute = POINT {
                    x: relative_x as i32,
                    y: relative_y as i32,
                };

                ClientToScreen(hwnd, &mut absolute);

                window.send_event_defer(Event::MouseMove {
                    absolute: Point {
                        x: absolute.x as f32,
                        y: absolute.y as f32,
                    },
                    relative: Point {
                        x: relative_x as f32,
                        y: relative_y as f32,
                    },
                });
                0
            }

            WM_SETCURSOR => {
                if lparam as u32 & 0xffff == HTCLIENT {
                    let cursor = window.inner.state_current_cursor.get();

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

            WM_GETMINMAXINFO => {
                let info = lparam as *mut MINMAXINFO;
                (*info).ptMinTrackSize = window.inner.min_window_size;
                (*info).ptMaxTrackSize = window.inner.max_window_size;
                (*info).ptMaxSize = window.inner.max_window_size;

                0
            }

            WM_SETFOCUS => {
                if !window.inner.state_focused.replace(true) {
                    window.send_event_defer(Event::WindowFocus { focus: true });
                }

                0
            }

            WM_KILLFOCUS => {
                if window.inner.state_focused.replace(false) {
                    window.send_event_defer(Event::WindowFocus { focus: false });
                }

                0
            }

            WM_USER_KEY_DOWN | WM_USER_KEY_UP => {
                let scan_code = ((lparam & 0x1ff_0000) >> 16) as u32;
                let mut capture = false;

                if let Some(key) = scan_code_to_key(scan_code) {
                    if msg == WM_USER_KEY_DOWN {
                        window.send_event(Event::KeyDown {
                            key,
                            capture: &mut capture,
                        });
                    } else {
                        window.send_event(Event::KeyUp {
                            key,
                            capture: &mut capture,
                        });
                    }
                }

                if capture { 1 } else { 0 }
            }

            WM_USER_FRAME_PACER => {
                let modifiers = get_modifiers_async();

                if window.inner.state_current_modifiers.replace(modifiers) != modifiers {
                    window.send_event_defer(Event::KeyModifiers { modifiers });
                }

                window.send_event(Event::WindowFrame {
                    gl: window
                        .gl_context
                        .as_ref()
                        .map(|x| x as &dyn crate::GlContext),
                });

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
