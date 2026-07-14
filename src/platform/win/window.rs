use super::gl::GlContext;
use crate::platform::win::dnd::DropTargetImpl;
use crate::platform::win::util::cursor::WinCursor;
use crate::platform::win::util::dpi::DpiContext;
use crate::platform::win::util::error::Win32Error;
use crate::platform::win::util::exchange::{
    Clipboard, decode_hdrop, encode_drop_effect, encode_hdrop,
};
use crate::platform::win::util::keyboard::{KeyboardHook, query_modifiers, scan_code_to_key};
use crate::platform::win::util::vsync::VSyncThread;
use crate::platform::win::util::widestr::WideString;
use crate::platform::win::util::window::{WindowProc, create_window, hinstance};
use crate::platform::*;
use raw_window_handle::RawWindowHandle;
use std::cell::{Cell, RefCell};
use std::collections::VecDeque;
use std::mem::{size_of, zeroed};
use std::num::NonZeroIsize;
use std::ptr::{null, null_mut};
use std::rc::Rc;
use std::sync::{Arc, RwLock};
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

/// Sent by Vsync thread, triggers [`WindowHandler::frame`] event
pub const WM_USER_VSYNC: u32 = WM_USER + 1;
/// Sent by [`PlatformWindow::close`] and received in the wnd_proc, closes the
/// window
pub const WM_USER_CLOSE_WINDOW: u32 = WM_USER + 2;
/// Sent by the [`KeyboardHook`] when a key event is captured
/// Same wParam/lParam data as in native WM_KEYDOWN/WM_KEYUP messages
pub const WM_USER_KEY_DOWN: u32 = WM_USER + 3;
/// See [`WM_USER_KEY_DOWN`]
pub const WM_USER_KEY_UP: u32 = WM_USER + 4;
/// Sent by the [`KeyboardHook`] when a modifier key state _maybe_ changes,
/// used for [`WindowHandler::key_modifiers`] event.
pub const WM_USER_KEY_MODIFIERS: u32 = WM_USER + 5;
/// Sent by [`WindowWakerImpl::wakeup`] to wake up the event loop
pub const WM_USER_WAKEUP: u32 = WM_USER + 6;
/// Sent by [`DropTargetImpl`] when a drop enters the window, triggers
/// [`WindowHandler::drag_enter`] event.
pub const WM_USER_DND_ENTER: u32 = WM_USER + 7;
/// Sent by [`DropTargetImpl`] when a drop hovers over the window, triggers
/// [`WindowHandler::drag_move`] event.
pub const WM_USER_DND_HOVER: u32 = WM_USER + 8;
/// Sent by [`DropTargetImpl`] when a drop leaves the window, triggers
/// [`WindowHandler::drag_leave`] event.
pub const WM_USER_DND_LEAVE: u32 = WM_USER + 9;
/// Sent by [`DropTargetImpl`] when a drop is performed, triggers
/// [`WindowHandler::drag_accept`] event.
pub const WM_USER_DND_ACCEPT: u32 = WM_USER + 10;

/// A Win32 implementation of a [`PlatformWindow`].
pub struct WindowImpl {
    /// The [`PlatformWaker`] for this window, used to wake up the event loop
    /// from any thread
    waker: Arc<WindowWakerImpl>,
    /// Current OpenGL context for this window, if requested. Or an error if the
    /// context could not be created.
    gl_context: Result<GlContext, OpenGlError>,
    /// Dynamically loaded DPI management functions, used for HiDPI support.
    dpi_context: DpiContext,
    /// Thread that waits for VSync blanks and sends a message to the window to
    /// trigger [`WindowHandler::frame`] event.
    vsync_thread: VSyncThread,
    /// COM based drag-and-drop handler, needed to access the new DnD API,
    /// unfortunately..
    _drop_target: Arc<DropTargetImpl>,
    /// Thread-local keyboard hook for this window.
    _keyboard_hook: KeyboardHook,

    /// The HWND for this window
    hwnd: HWND,
    /// The mode in which the window was opened
    open_mode: OpenMode,

    /// Windows API is inherently reentrant, so we have to make sure that we
    /// don't call the event handler while it is already borrowed (otherwise
    /// we would panic).
    ///
    /// Instead, we put the event into a queue so we can call it later once the
    /// event handler is free again.
    ///
    /// Same queue is used to defer events that are sent while the event handler
    /// is being initialized, so that we can send events to the handler as
    /// soon as it is ready.
    #[allow(clippy::type_complexity)]
    event_deferred: RefCell<VecDeque<Box<dyn FnOnce(&Self, &mut dyn WindowHandler)>>>,
    /// The event handler for this window, processes our events.
    event_handler: RefCell<Option<Box<dyn WindowHandler>>>,

    /// The last size of the window, used to detect size changes
    current_window_size: Cell<Size>,
    /// The last window position, used to detect position changes
    current_window_position: Cell<Point>,
    /// The current window style, used for window client area calculations.
    ///
    /// Stored as (DW_STYLE, DW_EXSTYLE)
    current_window_style: Cell<(u32, u32)>,
    /// The last window visibility state.
    current_window_visibility: Cell<WindowVisibility>,
    /// The current maximum size of the window, used to enforce size constraints
    current_max_window_size: Cell<Size>,
    /// The current minimum size of the window, used to enforce size constraints
    current_min_window_size: Cell<Size>,
    /// The current focus state of the window, used to detect focus changes
    current_window_focused: Cell<bool>,
    /// The current modifiers state of the window, used to detect modifier
    /// changes
    current_key_modifiers: Cell<Modifiers>,
    /// The current mouse cursor of the window, used to detect cursor changes
    current_mouse_cursor: Cell<(MouseCursor, WinCursor)>,
    /// The number of mouse button pressed - mouse button releases, used for
    /// automatic cursor capture and release.
    current_mouse_capture: Cell<u32>,
    /// The current mouse position of the window, used to detect mouse movement
    current_mouse_position: Cell<Option<Point>>,
    /// The current system scale for the window (in DPI).
    current_dpi_scale: Cell<u32>,
}

/// Win32 implementation of a [`PlatformWaker`].
pub struct WindowWakerImpl {
    /// The HWND of the window to wake up. We store it in a `RwLock` so we can
    /// clean-up the handle when the window is closed, and avoid sending
    /// messages to a closed window.
    window_hwnd: RwLock<HWND>,
}

unsafe impl Send for WindowWakerImpl {}
unsafe impl Sync for WindowWakerImpl {}

impl WindowImpl {
    pub unsafe fn open(options: WindowBuilder, mode: OpenMode) -> Result<WindowWaker, WindowError> {
        unsafe {
            let parent = match mode {
                OpenMode::Blocking => null_mut(),
                OpenMode::Embedded(RawWindowHandle::Win32(window)) => window.hwnd.get() as HWND,
                OpenMode::Transient(RawWindowHandle::Win32(window)) => window.hwnd.get() as HWND,
                _ => return Err(WindowError::InvalidParent),
            };

            let dwstyle = {
                let mut dwstyle = 0;

                match mode {
                    OpenMode::Blocking | OpenMode::Transient(..) => {
                        dwstyle |= WS_OVERLAPPEDWINDOW;
                    }

                    OpenMode::Embedded(..) => {
                        dwstyle |= WS_CHILD;
                    }
                }

                dwstyle
            };

            // S_FALSE is okay here if OleInitialize was already called on the current
            // thread. OleInitialize is needed for things like Drag and Drop.
            let ole_result = OleInitialize(null());
            let ole_success = ole_result != OLE_E_WRONGCOMPOBJ && ole_result != RPC_E_CHANGED_MODE;

            // set dpi awareness for the window (well restore it later)
            // we need it here so the window becomes DPI aware and window factory runs in
            // DPI aware mode (so calls to set_size and friends work correctly)
            let dpi_context = DpiContext::new();
            let _dpi_awareness = dpi_context.enter_per_monitor_aware_v2();

            let window = create_window(dwstyle, parent, |hwnd| {
                // enable transparency if requested
                if options.transparent {
                    let region = CreateRectRgn(0, 0, -1, -1);
                    let bb = DWM_BLURBEHIND {
                        dwFlags: DWM_BB_ENABLE | DWM_BB_BLURREGION,
                        fEnable: true.into(),
                        hRgnBlur: region,
                        fTransitionOnMaximized: false.into(),
                    };

                    if !region.is_null() {
                        DwmEnableBlurBehindWindow(hwnd, &bb);
                        DeleteObject(region);
                    }
                }

                // accept drag and drop
                let drop_target = DropTargetImpl::new(hwnd);
                if ole_success {
                    let result = RegisterDragDrop(hwnd, DropTargetImpl::as_raw(&drop_target) as _);
                    if result != 0 {
                        return Err(Win32Error::last_error().with_context("RegisterDragDrop"));
                    }
                }

                // new gl context if requested
                let gl_context = options
                    .opengl
                    .map(|config| GlContext::new(hwnd, config))
                    .unwrap_or_else(|| Err(OpenGlError::NotRequested));

                // construct our window data, here we store all our state accessible from
                // [`WindowProc::window_proc`]
                Ok(Rc::new(Self {
                    waker: Arc::new(WindowWakerImpl {
                        window_hwnd: RwLock::new(hwnd),
                    }),

                    current_dpi_scale: Cell::new(
                        dpi_context
                            .dpi_for_window(hwnd)
                            .unwrap_or(USER_DEFAULT_SCREEN_DPI),
                    ),
                    current_mouse_capture: Cell::new(0),
                    current_mouse_cursor: Cell::new((
                        MouseCursor::Default,
                        MouseCursor::Default.into(),
                    )),
                    current_key_modifiers: Cell::new(Modifiers::default()),
                    current_window_focused: Cell::new(false),

                    current_window_size: Cell::new(Size::default()),
                    current_window_position: Cell::new(Point::default()),
                    current_window_style: Cell::new((dwstyle, 0)),
                    current_window_visibility: Cell::new(WindowVisibility::Normal),
                    current_min_window_size: Cell::new(Size::MIN),
                    current_max_window_size: Cell::new(Size::MAX),
                    current_mouse_position: Cell::new(None),

                    hwnd,
                    open_mode: mode,

                    event_handler: RefCell::new(None),
                    event_deferred: RefCell::new(VecDeque::new()),

                    gl_context,
                    // the other one is in use, just make a new one, should be cheap
                    dpi_context: DpiContext::new(),
                    vsync_thread: VSyncThread::new(hwnd),
                    _keyboard_hook: KeyboardHook::new(hwnd),
                    _drop_target: drop_target,
                }))
            })?;

            // SAFETY: we erase the lifetime of WindowImpl; it should be safe to do so
            // because:
            //  - because our window instance is rc'd, it has a stable address for the whole
            //    lifetime of the window
            //  - we manually dispose of our handler before WindowImpl gets dropped (see
            //    drop impl)
            //  - we promise to not move WindowImpl (and by extension the handler) to a
            //    different thread (as that would violate the handler's !Send requirement)
            // initialize our event handler
            let handler = match (options.factory)(Window(&*(&*window as *const Self))) {
                Ok(handler) => handler,
                Err(error) => return Err(WindowError::Factory(error)),
            };

            // start accepting events
            window.event_handler.replace(Some(handler));
            // pull any events that were queued during initialization
            window.deferred_event(|_, _| {});

            // emit initial events: key modifiers
            window.current_key_modifiers.set(query_modifiers());
            window.deferred_event(|window, e| e.key_modifiers(window.current_key_modifiers.get()));

            if let OpenMode::Blocking = mode {
                // our favorite - win32 event pump
                let mut msg: MSG = std::mem::zeroed();
                while GetMessageW(&mut msg, null_mut(), 0, 0) > 0 {
                    TranslateMessage(&msg);
                    DispatchMessageW(&msg);
                }
            }

            Ok(window.waker())
        }
    }

    /// Run a closure with exclusive access to the window's event handler.
    ///
    /// Panics if [`Self::non_reentrant_event`] is called inside of another
    /// [`Self::non_reentrant_event`]. To safely post a task, use
    /// [`Self::deferred_event`].
    fn non_reentrant_event<R>(&self, call: impl FnOnce(&mut dyn WindowHandler) -> R) -> Option<R> {
        let mut handler = self
            .event_handler
            .try_borrow_mut()
            .expect("unhandled callback reentrancy");

        // handler might be None if the window is being dropped, in which case we return
        // None
        if let Some(handler) = handler.as_mut() {
            let result = Some(call(&mut **handler));

            loop {
                // event_queue must NOT be borrowed while calling the handler, so we have to
                // reborrow it every time
                let Some(event) = self.event_deferred.borrow_mut().pop_front() else {
                    break;
                };

                event(self, &mut **handler);
            }

            result
        } else {
            None
        }
    }

    /// Run a closure with exclusive access to the window's event handler.
    ///
    /// Unlike [`Self::non_reentrant_event`], this function will not panic if
    /// called inside of another [`Self::non_reentrant_event`]. Instead, the
    /// closure will be deferred and run later.
    ///
    /// For that reason it cannot return a value, and the closure must be
    /// `'static`.
    fn deferred_event(&self, task: impl FnOnce(&Self, &mut dyn WindowHandler) + 'static) {
        if self
            .event_handler
            .try_borrow_mut()
            .is_ok_and(|x| x.is_some())
        {
            self.non_reentrant_event(|handler| task(self, handler));
        } else {
            self.event_deferred.borrow_mut().push_back(Box::new(task));
        }
    }

    /// Convert a client size to a window size or vice-versa, taking into
    /// account the current window style and extended style.
    pub fn convert_client(&self, input: Rect, from_client: bool) -> Rect {
        let (dwstyle, dwexstyle) = self.current_window_style.get();
        let Some(rect) = self.dpi_context.adjust_window_rect_ex_for_dpi(
            RECT::default(),
            dwstyle,
            dwexstyle,
            false,
            self.current_dpi_scale.get(),
        ) else {
            return input;
        };

        if from_client {
            Rect {
                top: input.top.saturating_add(rect.top),
                left: input.left.saturating_add(rect.left),
                bottom: input.bottom.saturating_add(rect.bottom),
                right: input.right.saturating_add(rect.right),
            }
        } else {
            Rect {
                top: input.top.saturating_sub(rect.top),
                left: input.left.saturating_sub(rect.left),
                bottom: input.bottom.saturating_sub(rect.bottom),
                right: input.right.saturating_sub(rect.right),
            }
        }
    }
}

impl Drop for WindowImpl {
    fn drop(&mut self) {
        // subsequent wakeups should fail
        *self.waker.window_hwnd.write().expect("lock poisoned") = null_mut();

        // drop the handler here, so it could do clean up when the window is still alive
        // will ignore any events sent after this point, as the handler is gone
        self.event_handler.take();

        // winapi cleanup stuff
        unsafe {
            RevokeDragDrop(self.hwnd);
        }
    }
}

impl WindowProc for WindowImpl {
    unsafe fn window_proc(&self, hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
        // enter DPI aware context, who knows what the host thread is doing.
        let _dpi_awareness = self.dpi_context.enter_per_monitor_aware_v2();

        unsafe {
            match msg {
                WM_DESTROY => {
                    // exit the event loop if we are in blocking mode
                    if let OpenMode::Blocking = self.open_mode {
                        PostQuitMessage(0);
                    }

                    return 0;
                }

                WM_CLOSE => {
                    self.deferred_event(|_, e| e.close_requested());
                    return 0;
                }

                WM_DISPLAYCHANGE => {
                    self.vsync_thread.notify_display_change();
                }

                WM_WINDOWPOSCHANGED => {
                    let info = lparam as *const WINDOWPOS;

                    if (*info).flags & SWP_SHOWWINDOW != 0 {
                        // just in case, we might be on a new display
                        self.vsync_thread.notify_display_change();
                    }

                    let visibility = if (*info).flags & SWP_HIDEWINDOW != 0 {
                        WindowVisibility::Hidden
                    } else if (*info).flags & SWP_SHOWWINDOW != 0 {
                        WindowVisibility::Normal
                    } else if (*info).x == -32000 && (*info).y == -32000 {
                        WindowVisibility::Minimized
                    } else if self.current_window_visibility.get() == WindowVisibility::Hidden {
                        WindowVisibility::Hidden
                    } else {
                        WindowVisibility::Normal
                    };

                    let rect = self.convert_client(
                        Rect {
                            left: (*info).x,
                            top: (*info).y,
                            right: (*info).x.saturating_add((*info).cx),
                            bottom: (*info).y.saturating_add((*info).cy),
                        },
                        false,
                    );

                    // update window visibility
                    if self.current_window_visibility.replace(visibility) != visibility {
                        self.deferred_event(move |_, e| {
                            e.visibility_changed(visibility); // dont wanna miss any updates
                        });
                    }

                    // update window position
                    if visibility != WindowVisibility::Minimized
                        && self.current_window_position.replace(rect.origin()) != rect.origin()
                    {
                        self.deferred_event(move |window, e| {
                            // fine if we miss an update and get a new value instead
                            // because we do not capture anything, the closure will be zero-sized
                            // and not allocate
                            e.position_changed(window.current_window_position.get())
                        });
                    }

                    // update window size
                    if self.current_window_size.replace(rect.size()) != rect.size() {
                        self.deferred_event(move |window, e| {
                            e.size_changed(window.current_window_size.get()) // same as with position
                        });
                    }

                    return 0;
                }

                WM_DPICHANGED => {
                    self.current_dpi_scale.set((wparam & 0xFFFF) as u32);
                    self.deferred_event(|window, e| e.scale_changed(window.scale()));
                    return 0;
                }

                WM_STYLECHANGED => {
                    let dwstyle = GetWindowLongW(self.hwnd, GWL_STYLE) as u32;
                    let dwexstyle = GetWindowLongW(self.hwnd, GWL_EXSTYLE) as u32;
                    self.current_window_style.set((dwstyle, dwexstyle));
                }

                WM_MOUSEMOVE | WM_LBUTTONDOWN | WM_RBUTTONDOWN | WM_MBUTTONDOWN
                | WM_XBUTTONDOWN | WM_LBUTTONUP | WM_RBUTTONUP | WM_MBUTTONUP | WM_XBUTTONUP => {
                    if self.current_mouse_position.get().is_none() {
                        // mouse just entered the window, start tracking mouse leave events
                        let _ = TrackMouseEvent(&mut TRACKMOUSEEVENT {
                            cbSize: size_of::<TRACKMOUSEEVENT>() as u32,
                            dwFlags: TME_LEAVE,
                            hwndTrack: self.hwnd,
                            dwHoverTime: 0,
                        });
                    }

                    let point = Point {
                        x: (lparam & 0xFFFF) as i16 as f64,
                        y: ((lparam >> 16) & 0xFFFF) as i16 as f64,
                    };

                    // update cursor position
                    if self.current_mouse_position.replace(Some(point)) != Some(point) {
                        self.deferred_event(move |window, e| {
                            if let Some(point) = window.current_mouse_position.get() {
                                // fine if we miss an update and get a new value instead
                                // because we do not capture anything, the closure will be
                                // zero-sized and not allocate
                                e.mouse_move(point)
                            };
                        });
                    }

                    // if its a click event
                    if msg != WM_MOUSEMOVE {
                        let button = match msg {
                            WM_LBUTTONUP | WM_LBUTTONDOWN => Some(MouseButton::Left),
                            WM_RBUTTONUP | WM_RBUTTONDOWN => Some(MouseButton::Right),
                            WM_MBUTTONUP | WM_MBUTTONDOWN => Some(MouseButton::Middle),
                            WM_XBUTTONUP | WM_XBUTTONDOWN => match ((wparam >> 16) & 0xffff) as u16
                            {
                                XBUTTON1 => Some(MouseButton::Back),
                                XBUTTON2 => Some(MouseButton::Forward),
                                _ => None,
                            },
                            _ => None,
                        };

                        let down = matches!(
                            msg,
                            WM_LBUTTONDOWN | WM_RBUTTONDOWN | WM_MBUTTONDOWN | WM_XBUTTONDOWN
                        );

                        if let Some(button) = button {
                            self.deferred_event(move |_, e| e.mouse_press(button, down));
                        }

                        if down {
                            self.current_mouse_capture.update(|x| x + 1);
                            if self.current_mouse_capture.get() == 1 {
                                SetCapture(self.hwnd);
                                SetFocus(self.hwnd);
                            }
                        } else {
                            self.current_mouse_capture.update(|x| x.saturating_sub(1));
                            if self.current_mouse_capture.get() == 0 {
                                ReleaseCapture();
                            }
                        }
                    }
                }

                WM_MOUSEWHEEL | WM_MOUSEHWHEEL => {
                    let delta = (wparam >> 16) as i16;
                    let delta = delta as f64 / WHEEL_DELTA as f64;

                    let x = if msg == WM_MOUSEWHEEL { 0.0 } else { delta };
                    let y = if msg == WM_MOUSEWHEEL { -delta } else { 0.0 };

                    self.deferred_event(move |_, e| e.mouse_scroll(x, y));
                }

                WM_MOUSELEAVE => {
                    self.current_mouse_position.set(None);
                    self.deferred_event(move |_, e| e.mouse_leave());
                }

                WM_SETCURSOR if lparam as u32 & 0xffff == HTCLIENT => {
                    let (_, cursor) = self.current_mouse_cursor.get();
                    cursor.apply();
                    return 1;
                }

                WM_GETMINMAXINFO => {
                    let info = lparam as *mut MINMAXINFO;
                    let min = self.current_min_window_size.get();
                    let max = self.current_max_window_size.get();
                    (*info).ptMinTrackSize = POINT {
                        x: min.width.try_into().unwrap_or(i32::MAX),
                        y: min.height.try_into().unwrap_or(i32::MAX),
                    };
                    (*info).ptMaxTrackSize = POINT {
                        x: max.width.try_into().unwrap_or(i32::MAX),
                        y: max.height.try_into().unwrap_or(i32::MAX),
                    };
                    (*info).ptMaxSize = (*info).ptMaxTrackSize;
                    return 0;
                }

                WM_SETFOCUS if !self.current_window_focused.replace(true) => {
                    self.deferred_event(|_, e| e.focus_changed(true));
                }

                WM_KILLFOCUS if self.current_window_focused.replace(false) => {
                    self.deferred_event(|_, e| e.focus_changed(false));
                }

                WM_PAINT => {
                    let mut rect = RECT { ..zeroed() };
                    if GetUpdateRect(self.hwnd, &mut rect, 0) != 0 {
                        let rect = Rect {
                            left: rect.left,
                            top: rect.top,
                            right: rect.right,
                            bottom: rect.bottom,
                        };

                        self.deferred_event(move |_, e| e.damage(rect));
                        ValidateRgn(self.hwnd, null_mut());
                    }

                    return 0;
                }

                WM_USER_DND_ENTER => {
                    let mut point = (lparam as *const POINT).read();
                    if ScreenToClient(self.hwnd, &mut point) == 0 {
                        return 0;
                    }

                    let data = DropTargetImpl::decode_data_object(wparam as _);
                    let point = Point {
                        x: point.x as f64,
                        y: point.y as f64,
                    };

                    let effect = self
                        .non_reentrant_event(|e| e.drag_enter(data, point))
                        .unwrap_or(DropEffect::Reject);

                    return encode_drop_effect(effect) as _;
                }

                WM_USER_DND_HOVER => {
                    let mut point = (lparam as *const POINT).read();
                    if ScreenToClient(self.hwnd, &mut point) == 0 {
                        return 0;
                    }

                    let point = Point {
                        x: point.x as f64,
                        y: point.y as f64,
                    };

                    let effect = self
                        .non_reentrant_event(|e| e.drag_move(point))
                        .unwrap_or(DropEffect::Reject);

                    return encode_drop_effect(effect) as _;
                }

                WM_USER_DND_ACCEPT => {
                    let effect = self
                        .non_reentrant_event(|e| e.drag_accept())
                        .unwrap_or(DropEffect::Reject);

                    return encode_drop_effect(effect) as _;
                }

                WM_USER_DND_LEAVE => {
                    self.deferred_event(|_, e| e.drag_leave());
                    return 0;
                }

                WM_USER_KEY_MODIFIERS => {
                    let modifiers = query_modifiers();
                    if self.current_key_modifiers.replace(modifiers) != modifiers {
                        self.deferred_event(move |window, e| {
                            e.key_modifiers(window.current_key_modifiers.get())
                        });
                    }
                }

                WM_USER_KEY_DOWN | WM_USER_KEY_UP => {
                    let scan_code = ((lparam & 0x1ff_0000) >> 16) as u32;
                    let Some(key) = scan_code_to_key(scan_code) else {
                        return 0;
                    };

                    let capture = self
                        .non_reentrant_event(|handler| {
                            handler.key_press(key, msg == WM_USER_KEY_DOWN)
                        })
                        .unwrap_or(false);

                    return if capture { 1 } else { 0 };
                }

                WM_USER_VSYNC => {
                    // this closure is zero-sized and does not allocate, so we wouldn't alloc every
                    // frame. we have to defer here because we use
                    // `SendNotifyMessage` and this could sometimes be called while the event
                    // handler is borrowed, which would panic.
                    self.deferred_event(|window, e| {
                        e.frame();
                        window.vsync_thread.notify_frame_finished();
                    });

                    return 0;
                }

                WM_USER_WAKEUP => {
                    self.deferred_event(|_, e| e.wakeup());
                    return 0;
                }

                WM_USER_CLOSE_WINDOW => {
                    DestroyWindow(self.hwnd);
                    return 0;
                }

                _ => {}
            }

            DefWindowProcW(hwnd, msg, wparam, lparam)
        }
    }
}

impl PlatformWindow for WindowImpl {
    fn window_handle(&self) -> rwh_06::RawWindowHandle {
        unsafe {
            let mut handle =
                rwh_06::Win32WindowHandle::new(NonZeroIsize::new_unchecked(self.hwnd as isize));
            handle.hinstance = NonZeroIsize::new(hinstance() as isize);
            rwh_06::RawWindowHandle::Win32(handle)
        }
    }

    fn display_handle(&self) -> rwh_06::RawDisplayHandle {
        rwh_06::RawDisplayHandle::Windows(rwh_06::WindowsDisplayHandle::new())
    }

    fn close(&self) {
        unsafe {
            PostMessageW(self.hwnd, WM_USER_CLOSE_WINDOW, 0, 0);
        }
    }

    fn waker(&self) -> WindowWaker {
        WindowWaker(self.waker.clone())
    }

    fn opengl(&self) -> Result<&dyn PlatformOpenGl, OpenGlError> {
        match &self.gl_context {
            Ok(gl) => Ok(gl),
            Err(err) => Err(err.clone()),
        }
    }

    fn scale(&self) -> f64 {
        self.current_dpi_scale.get() as f64 / USER_DEFAULT_SCREEN_DPI as f64
    }

    fn set_title(&self, title: &str) {
        unsafe {
            let title = WideString::from(title);
            SetWindowTextW(self.hwnd, title.as_ptr());
        }
    }

    fn set_decorations(&self, decorations: bool) {
        unsafe {
            if matches!(self.open_mode, OpenMode::Embedded(..)) {
                return;
            }

            let mut style = self.current_window_style.get().0;
            if decorations {
                style |= WS_OVERLAPPEDWINDOW;
                style &= !WS_POPUP;
            } else {
                style &= !WS_OVERLAPPEDWINDOW;
                style |= WS_POPUP;
            }

            SetWindowLongW(self.hwnd, GWL_STYLE, style as _);
            self.current_window_style
                .update(|(_, exstyle)| (style, exstyle));

            // force a resize (restyling keeps the outer size while changing the inner size,
            // so we need to resize the window to keep the client size the same)
            self.set_size(self.current_window_size.replace(Size::default()));
        }
    }

    fn set_cursor_icon(&self, cursor: MouseCursor) {
        if self.current_mouse_cursor.get().0 != cursor {
            self.current_mouse_cursor.set((cursor, cursor.into()));
        }
    }

    fn set_cursor_position(&self, point: Point) {
        unsafe {
            let mut point = POINT {
                x: point.x as i32,
                y: point.y as i32,
            };

            if ClientToScreen(self.hwnd, &mut point) != 0 {
                SetCursorPos(point.x, point.y);
            }
        }
    }

    fn set_size(&self, size: Size) {
        unsafe {
            // do nothing if the size doesnt change
            if self.current_window_size.get() == size {
                return;
            }

            let size = self.convert_client(Rect::from_size(size), true).size();
            SetWindowPos(
                self.hwnd,
                self.hwnd,
                0,
                0,
                size.width as i32,
                size.height as i32,
                SWP_NOZORDER | SWP_NOMOVE | SWP_NOACTIVATE,
            );
        }
    }

    fn set_min_size(&self, size: Size) {
        let size = self.convert_client(Rect::from_size(size), true).size();
        self.current_min_window_size.set(size);
    }

    fn set_max_size(&self, size: Size) {
        let size = self.convert_client(Rect::from_size(size), true).size();
        self.current_max_window_size.set(size);
    }

    fn set_position(&self, point: Point) {
        unsafe {
            SetWindowPos(
                self.hwnd,
                self.hwnd,
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
                self.hwnd,
                self.hwnd,
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
        let path = WideString::from(url);
        let verb = WideString::from("open");

        unsafe {
            ShellExecuteW(
                self.hwnd,
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
            let clipboard = match Clipboard::open(self.hwnd) {
                Some(clipboard) => clipboard,
                None => return Exchange::Empty,
            };

            if let Some(files) = clipboard.get(CF_HDROP, |hdrop| decode_hdrop(hdrop.as_ptr() as _))
            {
                return Exchange::Files(files);
            }

            if let Some(text) = clipboard.get(CF_UNICODETEXT, |data| {
                WideString::from_iter(data.iter().copied()).to_string_lossy()
            }) {
                return Exchange::Text(text);
            }

            Exchange::Empty
        }
    }

    fn set_clipboard(&self, data: Exchange) -> bool {
        unsafe {
            let clipboard = match Clipboard::open(self.hwnd) {
                Some(clipboard) => clipboard,
                None => return false,
            };

            match data {
                Exchange::Empty => clipboard.empty(),
                Exchange::Files(files) => {
                    clipboard.set(CF_HDROP, &encode_hdrop(&files));
                }
                Exchange::Text(text) => {
                    clipboard.set(
                        CF_UNICODETEXT,
                        WideString::from(text.as_str()).as_bytes_with_nul(),
                    );
                }
            }

            true
        }
    }
}

impl PlatformWaker for WindowWakerImpl {
    fn wakeup(&self) -> Result<(), WakeupError> {
        let guard = self.window_hwnd.read().expect("lock poisoned");

        if guard.is_null() {
            return Err(WakeupError);
        }

        unsafe {
            PostMessageW(*guard, WM_USER_WAKEUP, 0, 0);
        }

        Ok(())
    }
}
