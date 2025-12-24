use super::{
    gl::GlContext,
    shared::Win32Shared,
    util::{
        check_error, from_widestring, generate_guid, get_modifiers, hinstance, run_event_loop,
        scan_code_to_key, to_widestring,
    },
};
use crate::{
    Error, Event, Modifiers, MouseButton, MouseCursor, Point, Size, WakeupError, Window,
    WindowBuilder, WindowWaker,
    platform::{
        OpenMode, PlatformWaker, PlatformWindow,
        win::{util::window_size_from_client_size, vsync::VSyncCallback},
    },
    rwh_06,
};
use std::{
    cell::{Cell, RefCell},
    collections::VecDeque,
    mem::{size_of, zeroed},
    num::NonZeroIsize,
    ptr::{copy_nonoverlapping, null, null_mut},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
};
use windows_sys::Win32::{
    Foundation::{HWND, LPARAM, LRESULT, POINT, RECT, WPARAM},
    Graphics::Gdi::{ClientToScreen, GetUpdateRect, ValidateRgn},
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
            SetCapture, SetFocus, TME_LEAVE, TRACKMOUSEEVENT, TrackMouseEvent,
        },
        Shell::ShellExecuteW,
        WindowsAndMessaging::{
            CW_USEDEFAULT, CreateWindowExW, DefWindowProcW, DestroyWindow, GWL_STYLE,
            GWLP_USERDATA, GWLP_WNDPROC, GetClientRect, GetDesktopWindow, GetWindowLongPtrW,
            GetWindowLongW, HCURSOR, HTCLIENT, IDC_ARROW, LoadCursorW, MINMAXINFO, PostMessageW,
            PostQuitMessage, RegisterClassW, SW_SHOWDEFAULT, SWP_HIDEWINDOW, SWP_NOACTIVATE,
            SWP_NOMOVE, SWP_NOSIZE, SWP_NOZORDER, SWP_SHOWWINDOW, SendMessageW, SetCursor,
            SetCursorPos, SetWindowLongPtrW, SetWindowPos, SetWindowTextW, ShowCursor,
            USER_DEFAULT_SCREEN_DPI, UnregisterClassW, WHEEL_DELTA, WM_CLOSE, WM_DESTROY,
            WM_DISPLAYCHANGE, WM_DPICHANGED, WM_GETMINMAXINFO, WM_KILLFOCUS, WM_LBUTTONDOWN,
            WM_LBUTTONUP, WM_MBUTTONDOWN, WM_MBUTTONUP, WM_MOUSEHWHEEL, WM_MOUSEMOVE,
            WM_MOUSEWHEEL, WM_MOVE, WM_PAINT, WM_RBUTTONDOWN, WM_RBUTTONUP, WM_SETCURSOR,
            WM_SETFOCUS, WM_SHOWWINDOW, WM_SIZE, WM_USER, WM_XBUTTONDOWN, WM_XBUTTONUP, WNDCLASSW,
            WS_CHILD, WS_MAXIMIZEBOX, WS_OVERLAPPEDWINDOW, WS_POPUP, WS_SIZEBOX, WS_VISIBLE,
            XBUTTON1, XBUTTON2,
        },
    },
};

pub const WM_USER_VSYNC: u32 = WM_USER + 1;
pub const WM_USER_KILL_WINDOW: u32 = WM_USER + 2;
pub const WM_USER_KEY_DOWN: u32 = WM_USER + 3;
pub const WM_USER_KEY_UP: u32 = WM_USER + 4;
pub const WM_USER_WAKEUP: u32 = WM_USER + 5;

pub struct WindowImpl {
    gl_context: Option<GlContext>,

    #[allow(clippy::type_complexity)]
    event_handler: RefCell<Option<Box<dyn FnMut(Event)>>>,
    event_queue: RefCell<VecDeque<Event<'static>>>,

    shared: Arc<Win32Shared>,
    waker: Arc<WindowWakerImpl>,

    window_hwnd: HWND,
    window_class: u16,
    vsync_callback: VSyncCallback,

    is_blocking: bool,
    is_resizable: bool,
    min_max_window_size: Cell<(POINT, POINT)>,

    state_focused: Cell<bool>,
    state_current_modifiers: Cell<Modifiers>,
    state_current_cursor: Cell<HCURSOR>,
    state_mouse_capture: Cell<u32>,
}

pub struct WindowWakerImpl {
    window_hwnd: HWND,
    window_open: AtomicBool,
}

unsafe impl Send for WindowWakerImpl {}
unsafe impl Sync for WindowWakerImpl {}

impl WindowImpl {
    pub unsafe fn is_our_window(hwnd: HWND) -> bool {
        unsafe { GetWindowLongPtrW(hwnd, GWLP_WNDPROC) == wnd_proc as *const () as isize }
    }

    pub unsafe fn open(options: WindowBuilder, mode: OpenMode) -> Result<WindowWaker, Error> {
        unsafe {
            let shared = Win32Shared::get()?;
            let parent = match mode.handle() {
                None => null_mut(),
                Some(rwh_06::RawWindowHandle::Win32(window)) => window.hwnd.get() as HWND,
                Some(_) => return Err(Error::InvalidParent),
            };

            if let OpenMode::Blocking = mode {
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
                    OpenMode::Blocking | OpenMode::Transient(..) => {
                        if options.decorations {
                            dwstyle |= WS_OVERLAPPEDWINDOW;
                        } else {
                            dwstyle |= WS_POPUP;
                        }

                        if options.resizable.is_none() {
                            dwstyle &= !WS_MAXIMIZEBOX;
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
            let (pos_x, pos_y) = match options.position {
                Some(point) => (point.x as i32, point.y as i32),
                None if !parent.is_null() => (0, 0),
                None => {
                    let mut rect = RECT { ..zeroed() };
                    if GetClientRect(GetDesktopWindow(), &mut rect) != 0 {
                        (
                            rect.left + (rect.right - rect.left - size.x) / 2,
                            rect.top + (rect.bottom - rect.top - size.y) / 2,
                        )
                    } else {
                        (CW_USEDEFAULT, CW_USEDEFAULT)
                    }
                }
            };

            let hwnd = CreateWindowExW(
                0,
                window_class as _,
                window_title.as_ptr() as _,
                dwstyle,
                pos_x,
                pos_y,
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

            let window = Box::new(Self {
                shared: shared.clone(),
                waker: Arc::new(WindowWakerImpl {
                    window_hwnd: hwnd,
                    window_open: AtomicBool::new(true),
                }),

                state_mouse_capture: Cell::new(0),
                state_current_cursor: Cell::new(shared.load_cursor(MouseCursor::Default)),
                state_current_modifiers: Cell::new(Modifiers::empty()),
                state_focused: Cell::new(true),

                window_class,
                window_hwnd: hwnd,

                is_blocking: matches!(mode, OpenMode::Blocking),
                is_resizable: options.resizable.is_some(),
                min_max_window_size: Cell::new(
                    options
                        .resizable
                        .map(|r| {
                            (
                                window_size_from_client_size(r.start, dwstyle),
                                window_size_from_client_size(r.end, dwstyle),
                            )
                        })
                        .unwrap_or((size, size)),
                ),

                event_handler: RefCell::new(None),
                event_queue: RefCell::new(VecDeque::new()),
                gl_context,

                vsync_callback: VSyncCallback::new(hwnd, |hwnd| {
                    SendMessageW(hwnd, WM_USER_VSYNC, 0, 0);
                }),
            });

            // SAFETY: we erase the lifetime of WindowImpl; it should be safe to do so because:
            //  - because our window instance is boxed, it has a stable address for the whole lifetime of the window
            //  - we manually dispose of our handler before WindowImpl gets dropped (see drop impl)
            //  - we promise to not move WindowImpl (and by extension the handler) to a different thread (as that would violate the handler's !Send requirement)
            window
                .event_handler
                .replace(Some((options.factory)(Window(&*(&*window as *const Self)))));

            window.send_event(Event::WindowScale {
                scale: shared.try_get_dpi_for_window(hwnd) as f32 / USER_DEFAULT_SCREEN_DPI as f32,
            });

            let waker = window.waker();
            SetWindowLongPtrW(hwnd, GWLP_USERDATA, Box::into_raw(window) as _);

            if let OpenMode::Blocking = mode {
                run_event_loop(null_mut());
            }

            Ok(waker)
        }
    }

    fn send_event(&self, event: Event) {
        if let Ok(mut handler) = self.event_handler.try_borrow_mut() {
            if let Some(handler) = handler.as_mut() {
                (handler)(event);

                while let Some(event) = self.event_queue.borrow_mut().pop_front() {
                    (handler)(event);
                }
            }
        } else {
            debug_assert!(false, "send_event reentrancy");
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
        // subsequent wakeups should fail
        self.waker.window_open.store(false, Ordering::Release);

        // drop the handler here, so it could do clean up when the window is still alive
        self.event_handler.take();

        unsafe {
            SetWindowLongPtrW(self.window_hwnd, GWLP_USERDATA, 0);
            UnregisterClassW(self.window_class as _, hinstance());
        }
    }
}

impl PlatformWindow for WindowImpl {
    fn close(&self) {
        unsafe {
            PostMessageW(self.window_hwnd, WM_USER_KILL_WINDOW, 0, 0);
        }
    }

    fn waker(&self) -> WindowWaker {
        WindowWaker(self.waker.clone())
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

            if !self.is_resizable {
                self.min_max_window_size.set((size, size));
            }

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

impl PlatformWaker for WindowWakerImpl {
    fn wakeup(&self) -> Result<(), WakeupError> {
        if self.window_open.load(Ordering::Acquire) {
            unsafe {
                PostMessageW(self.window_hwnd, WM_USER_WAKEUP, 0, 0);
                Ok(())
            }
        } else {
            Err(WakeupError::Disconnected)
        }
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
            if (*window_ptr).is_blocking {
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

            WM_CLOSE => {
                window.send_event(Event::WindowClose);
                0
            }

            WM_SHOWWINDOW => {
                window.vsync_callback.notify_display_change();
                0
            }

            WM_DISPLAYCHANGE => {
                window.vsync_callback.notify_display_change();
                0
            }

            WM_SIZE => {
                let width = ((lparam >> 0) & 0xFFFF) as u32;
                let height = ((lparam >> 16) & 0xFFFF) as u32;
                window.send_event_defer(Event::WindowResize {
                    size: Size { width, height },
                });

                window.vsync_callback.notify_display_change();
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
                    .state_mouse_capture
                    .replace(window.state_mouse_capture.get() + 1)
                    == 0
                {
                    SetFocus(hwnd);
                    SetCapture(hwnd);
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

            WM_GETMINMAXINFO => {
                let info = lparam as *mut MINMAXINFO;
                let (min_size, max_size) = window.min_max_window_size.get();
                (*info).ptMinTrackSize = min_size;
                (*info).ptMaxTrackSize = max_size;
                (*info).ptMaxSize = max_size;
                0
            }

            WM_SETFOCUS => {
                if !window.state_focused.replace(true) {
                    window.send_event_defer(Event::WindowFocus { focus: true });
                }

                0
            }

            WM_KILLFOCUS => {
                if window.state_focused.replace(false) {
                    window.send_event_defer(Event::WindowFocus { focus: false });
                }

                0
            }

            WM_PAINT => {
                let mut rect = RECT { ..zeroed() };
                if GetUpdateRect(hwnd, &mut rect, 0) != 0 {
                    window.send_event_defer(Event::WindowDamage {
                        x: rect.left.try_into().unwrap_or(0),
                        y: rect.top.try_into().unwrap_or(0),
                        w: (rect.right - rect.left).try_into().unwrap_or(0),
                        h: (rect.bottom - rect.top).try_into().unwrap_or(0),
                    });
                    ValidateRgn(hwnd, null_mut());
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

            WM_USER_VSYNC => {
                let modifiers = get_modifiers();

                if window.state_current_modifiers.replace(modifiers) != modifiers {
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

            WM_USER_WAKEUP => {
                window.send_event_defer(Event::Wakeup);
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
