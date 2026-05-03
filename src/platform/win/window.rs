use super::gl::GlContext;
use super::hook::KeyboardHook;
use super::util::*;
use super::vsync::VSyncThread;
use crate::platform::win::dnd::DropTargetImpl;
use crate::platform::*;
use crate::*;
use std::cell::{Cell, RefCell};
use std::collections::VecDeque;
use std::mem::{size_of, zeroed};
use std::num::NonZeroIsize;
use std::ptr::{null, null_mut};
use std::rc::Rc;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use windows_sys::Win32::Foundation::{
    HWND, LPARAM, LRESULT, OLE_E_WRONGCOMPOBJ, POINT, RECT, RPC_E_CHANGED_MODE, WPARAM,
};
use windows_sys::Win32::Graphics::Dwm::{
    DWM_BB_BLURREGION, DWM_BB_ENABLE, DWM_BLURBEHIND, DwmEnableBlurBehindWindow,
};
use windows_sys::Win32::Graphics::Gdi::{
    ClientToScreen, CreateRectRgn, DeleteObject, GetUpdateRect, ScreenToClient, ValidateRgn,
};
use windows_sys::Win32::System::Ole::{
    CF_HDROP, CF_UNICODETEXT, OleInitialize, RegisterDragDrop, RevokeDragDrop,
};
use windows_sys::Win32::UI::Controls::WM_MOUSELEAVE;
use windows_sys::Win32::UI::Input::KeyboardAndMouse::*;
use windows_sys::Win32::UI::Shell::ShellExecuteW;
use windows_sys::Win32::UI::WindowsAndMessaging::*;

/// Sent by Vsync thread, triggers [`Event::WindowFrame`] event
pub const WM_USER_VSYNC: u32 = WM_USER + 1;
/// Sent by [`PlatformWindow::close`] and received in the wnd_proc, closes the
/// window
pub const WM_USER_KILL_WINDOW: u32 = WM_USER + 2;
/// Sent by the [`KeyboardHook`] when a key event is captured
/// Same wParam/lParam data as in native WM_KEYDOWN/WM_KEYUP messages
pub const WM_USER_KEY_DOWN: u32 = WM_USER + 3;
/// See [`WM_USER_KEY_DOWN`]
pub const WM_USER_KEY_UP: u32 = WM_USER + 4;
/// Sent by [`WindowWakerImpl::wakeup`] to wake up the event loop
pub const WM_USER_WAKEUP: u32 = WM_USER + 5;
/// Sent by [`DropTargetImpl`] when a drop enters the window, triggers
/// [`Event::DragEnter`] event.
pub const WM_USER_DND_ENTER: u32 = WM_USER + 6;
/// Sent by [`DropTargetImpl`] when a drop hovers over the window, triggers
/// [`Event::DragMove`] event.
pub const WM_USER_DND_HOVER: u32 = WM_USER + 7;
/// Sent by [`DropTargetImpl`] when a drop leaves the window, triggers
/// [`Event::DragLeave`] event.
pub const WM_USER_DND_LEAVE: u32 = WM_USER + 8;
/// Sent by [`DropTargetImpl`] when a drop is performed, triggers
/// [`Event::DragAccept`] event.
pub const WM_USER_DND_ACCEPT: u32 = WM_USER + 9;

pub struct WindowImpl {
    gl_context: Option<GlContext>,
    waker: Arc<WindowWakerImpl>,

    #[allow(clippy::type_complexity)]
    event_handler: RefCell<Option<Box<dyn FnMut(Event)>>>,
    event_queue: RefCell<VecDeque<Event<'static>>>,

    window_hwnd: HWND,
    window_class: u16,

    _drop_target: Arc<DropTargetImpl>,
    keyboard_hook: Rc<KeyboardHook>,
    vsync_thread: VSyncThread,

    is_blocking: bool,
    is_resizable: bool,
    min_max_window_size: Cell<(POINT, POINT)>,

    state_focused: Cell<bool>,
    state_current_modifiers: Cell<Modifiers>,
    state_current_cursor: Cell<HCURSOR>,
    state_mouse_capture: Cell<u32>,

    cursor_cache: CursorCache,
}

pub struct WindowWakerImpl {
    window_hwnd: HWND,
    window_open: AtomicBool,
}

unsafe impl Send for WindowWakerImpl {}
unsafe impl Sync for WindowWakerImpl {}

impl WindowImpl {
    pub unsafe fn open(options: WindowBuilder, mode: OpenMode) -> Result<WindowWaker, WindowError> {
        unsafe {
            let parent = match mode.handle() {
                None => null_mut(),
                Some(rwh_06::RawWindowHandle::Win32(window)) => window.hwnd.get() as HWND,
                Some(_) => return Err(WindowError::InvalidParent),
            };

            // S_FALSE is okay here if OleInitialize was already called on the current
            // thread.
            let ole_result = OleInitialize(null());
            let ole_success = ole_result != OLE_E_WRONGCOMPOBJ && ole_result != RPC_E_CHANGED_MODE;

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

            check_error(window_class != 0, "RegisterClassW")?;

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
                None if matches!(mode, OpenMode::Embedded(..)) => (0, 0),
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

            // set dpi awareness for the window (well restore it later)
            let prev_dpi_awareness =
                try_set_thread_dpi_awareness(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE);

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

            // restore previous dpi awareness
            if let Some(prev_dpi_awareness) = prev_dpi_awareness {
                try_set_thread_dpi_awareness(prev_dpi_awareness);
            }

            check_error(!hwnd.is_null(), "CreateWindowExW")?;

            // enable transparency if requested
            if options.transparent {
                let region = CreateRectRgn(0, 0, -1, -1);
                let bb = DWM_BLURBEHIND {
                    dwFlags: DWM_BB_ENABLE | DWM_BB_BLURREGION,
                    fEnable: true.into(),
                    hRgnBlur: region,
                    fTransitionOnMaximized: false.into(),
                };

                DwmEnableBlurBehindWindow(hwnd, &bb);
                DeleteObject(region);
            }

            // accept drag and drop
            let drop_target = DropTargetImpl::new(hwnd);
            if ole_success {
                let result = RegisterDragDrop(hwnd, DropTargetImpl::as_raw(&drop_target) as _);
                check_error(result == 0, "RegisterDragDrop")?;
            }

            // new gl context if requested
            let gl_context = match options.opengl {
                Some(config) => GlContext::new(hwnd, config).ok(),
                None => None,
            };

            // install the keyboard hook and register our window to it, so we could capture
            // key events even when the window is not focused. keyboard hooks are shared on
            // a per-thread basis and gets deregistered when all [`KeyboardHook`] instances
            // gets dropped
            let keyboard_hook = KeyboardHook::install();
            keyboard_hook.add_window(hwnd);

            let cursor_cache = CursorCache::load();

            let window = Box::new(Self {
                waker: Arc::new(WindowWakerImpl {
                    window_hwnd: hwnd,
                    window_open: AtomicBool::new(true),
                }),

                state_mouse_capture: Cell::new(0),
                state_current_cursor: Cell::new(cursor_cache.get_closest(MouseCursor::Default)),
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

                gl_context,
                cursor_cache,

                event_handler: RefCell::new(None),
                event_queue: RefCell::new(VecDeque::new()),

                _drop_target: drop_target,
                keyboard_hook,
                vsync_thread: VSyncThread::new(hwnd),
            });

            // SAFETY: we erase the lifetime of WindowImpl; it should be safe to do so
            // because:
            //  - because our window instance is boxed, it has a stable address for the
            //    whole lifetime of the window
            //  - we manually dispose of our handler before WindowImpl gets dropped (see
            //    drop impl)
            //  - we promise to not move WindowImpl (and by extension the handler) to a
            //    different thread (as that would violate the handler's !Send requirement)
            window
                .event_handler
                .replace(Some((options.factory)(Window(&*(&*window as *const Self)))));

            window.send_event(Event::WindowFocus { focus: true });
            window.send_event(Event::WindowScale {
                scale: try_get_dpi_for_window(hwnd)
                    .map_or(1.0, |dpi| dpi as f32 / USER_DEFAULT_SCREEN_DPI as f32),
            });

            let waker = window.waker();
            SetWindowLongPtrW(hwnd, GWLP_USERDATA, Box::into_raw(window) as _);

            if let OpenMode::Blocking = mode {
                // our favorite - win32 event pump
                let mut msg: MSG = std::mem::zeroed();
                while GetMessageW(&mut msg, null_mut(), 0, 0) > 0 {
                    TranslateMessage(&msg);
                    DispatchMessageW(&msg);
                }
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

        // remove the window from the keyboard hook
        self.keyboard_hook.remove_window(self.window_hwnd);

        unsafe {
            RevokeDragDrop(self.window_hwnd);
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

    fn opengl(&self) -> Option<&dyn PlatformOpenGl> {
        self.gl_context.as_ref().map(|c| c as &dyn PlatformOpenGl)
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
            .set(self.cursor_cache.get_closest(cursor));
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

    fn get_clipboard(&self) -> Exchange {
        unsafe {
            let clipboard = match Clipboard::open(self.window_hwnd) {
                Some(clipboard) => clipboard,
                None => return Exchange::Empty,
            };

            if let Some(files) = clipboard.get(CF_HDROP, |hdrop| decode_hdrop(hdrop as _)) {
                return Exchange::Files(files);
            }

            if let Some(text) = clipboard.get(CF_UNICODETEXT, |data| from_widestring(data as _)) {
                return Exchange::Text(text);
            }

            Exchange::Empty
        }
    }

    fn set_clipboard(&self, data: Exchange) -> bool {
        unsafe {
            let clipboard = match Clipboard::open(self.window_hwnd) {
                Some(clipboard) => clipboard,
                None => return false,
            };

            match data {
                Exchange::Empty => clipboard.empty(),
                Exchange::Text(text) => {
                    clipboard.set(CF_UNICODETEXT, &to_widestring(&text));
                }
                Exchange::Files(files) => {
                    clipboard.set(CF_HDROP, &encode_hdrop(&files));
                }
            }

            true
        }
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
            Err(WakeupError)
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
                    point: Point { x, y },
                });

                0
            }

            WM_CLOSE => {
                window.send_event(Event::WindowClose);
                0
            }

            WM_SHOWWINDOW => {
                window.vsync_thread.notify_display_change();
                0
            }

            WM_DISPLAYCHANGE => {
                window.vsync_thread.notify_display_change();
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

                // we do not resize the window here because we want to keep the _physical_ size
                // of the window unchanged, and let the application decide how to handle DPI
                // changes.
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
                let wheel_delta = (wparam >> 16) as i16;
                let wheel_delta = wheel_delta as f32 / WHEEL_DELTA as f32;

                window.send_event_defer(Event::MouseScroll {
                    x: if msg == WM_MOUSEWHEEL {
                        0.0
                    } else {
                        wheel_delta
                    },
                    y: if msg == WM_MOUSEWHEEL {
                        -wheel_delta
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

                let (x, y) = (
                    (lparam & 0xFFFF) as i16 as i32,
                    ((lparam >> 16) & 0xFFFF) as i16 as i32,
                );

                let mut absolute = POINT { x, y };
                ClientToScreen(hwnd, &mut absolute);

                window.send_event_defer(Event::MouseMove {
                    absolute: Point {
                        x: absolute.x as f32,
                        y: absolute.y as f32,
                    },
                    relative: Point {
                        x: x as f32,
                        y: y as f32,
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

            WM_USER_DND_ENTER => {
                let mut point = (lparam as *const POINT).read();
                if ScreenToClient(hwnd, &mut point) == 0 {
                    return 0;
                }

                window.send_event_defer(Event::DragEnter {
                    data: DropTargetImpl::decode_data_object(wparam as _),
                    point: Point {
                        x: point.x as f32,
                        y: point.y as f32,
                    },
                });
                0
            }

            WM_USER_DND_HOVER => {
                let mut point = (lparam as *const POINT).read();
                if ScreenToClient(hwnd, &mut point) == 0 {
                    return 0;
                }

                window.send_event_defer(Event::DragMove {
                    point: Point {
                        x: point.x as f32,
                        y: point.y as f32,
                    },
                });

                0
            }

            WM_USER_DND_ACCEPT => {
                window.send_event_defer(Event::DragAccept);
                0
            }

            WM_USER_DND_LEAVE => {
                window.send_event_defer(Event::DragLeave);
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

                window.send_event(Event::WindowFrame);
                window.vsync_thread.notify_frame_finished();
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
