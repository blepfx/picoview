use super::gl::GlContext;
use super::hook::KeyboardHook;
use super::util::*;
use super::vsync::VSyncThread;
use crate::platform::win::dnd::DropTargetImpl;
use crate::platform::*;
use raw_window_handle::RawWindowHandle;
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

/// Sent by Vsync thread, triggers [`WindowHandler::frame`] event
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
/// [`WindowHandler::drag_enter`] event.
pub const WM_USER_DND_ENTER: u32 = WM_USER + 6;
/// Sent by [`DropTargetImpl`] when a drop hovers over the window, triggers
/// [`WindowHandler::drag_move`] event.
pub const WM_USER_DND_HOVER: u32 = WM_USER + 7;
/// Sent by [`DropTargetImpl`] when a drop leaves the window, triggers
/// [`WindowHandler::drag_leave`] event.
pub const WM_USER_DND_LEAVE: u32 = WM_USER + 8;
/// Sent by [`DropTargetImpl`] when a drop is performed, triggers
/// [`WindowHandler::drag_accept`] event.
pub const WM_USER_DND_ACCEPT: u32 = WM_USER + 9;

/// A Win32 implementation of a [`PlatformWindow`].
pub struct WindowImpl {
    /// The [`PlatformWaker`] for this window, used to wake up the event loop
    /// from any thread
    waker: Arc<WindowWakerImpl>,
    /// Current OpenGL context for this window, if requested. Or an error if the
    /// context could not be created.
    gl_context: Result<GlContext, OpenGlError>,

    /// `winapi` is inherently reentrant, so we have to make sure that we don't
    /// call the event handler while it is already borrowed (otherwise we
    /// would panic).
    ///
    /// Instead, we put the event into a queue so we can call it later once the
    /// event handler is free again.
    #[allow(clippy::type_complexity)]
    event_deferred: RefCell<VecDeque<Box<dyn FnOnce(&Self, &mut dyn WindowHandler)>>>,
    /// The event handler for this window, processes our events.
    event_handler: RefCell<Option<Box<dyn WindowHandler>>>,

    /// The HWND for this window
    window_hwnd: HWND,
    /// The unique class we created just for this window, so we can unregister
    /// it when the window is closed
    window_class: u16,

    /// COM based drag-and-drop handler, needed to access the new DnD API,
    /// unfortunately..
    _drop_target: Arc<DropTargetImpl>,
    /// Thread-local keyboard hook for this window.
    keyboard_hook: Rc<KeyboardHook>,
    /// Thread that waits for VSync blanks and sends a message to the window to
    /// trigger [`WindowHandler::frame`] event.
    vsync_thread: VSyncThread,

    /// The mode in which the window was opened
    open_mode: OpenMode,

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
    current_mouse_cursor: Cell<HCURSOR>,
    /// The number of mouse button pressed - mouse button releases, used for
    /// automatic cursor capture and release.
    current_mouse_capture: Cell<u32>,
    /// The current system scale for the window (in DPI).
    current_dpi_scale: Cell<u32>,

    /// Cache of preloaded cursors, used for querying the right system-provided
    /// cursor icon from a [`MouseCursor`]
    cursor_cache: CursorCache,
}

/// Win32 implementation of a [`PlatformWaker`].
pub struct WindowWakerImpl {
    /// The HWND of the window to wake up
    window_hwnd: HWND,
    /// Whether the window is still open. // TODO: possible race condition?
    window_open: AtomicBool,
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

            // S_FALSE is okay here if OleInitialize was already called on the current
            // thread. OleInitialize is needed for things like Drag and Drop.
            let ole_result = OleInitialize(null());
            let ole_success = ole_result != OLE_E_WRONGCOMPOBJ && ole_result != RPC_E_CHANGED_MODE;

            // register a new window class for our window with unique id
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

            check_error(window_class != 0, "RegisterClassW")?;

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

            // set dpi awareness for the window (well restore it later)
            let prev_dpi_awareness =
                try_set_thread_dpi_awareness(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE);

            // new window! zero size for now
            let hwnd = CreateWindowExW(
                0,
                window_class as _,
                [0].as_ptr() as _,
                dwstyle,
                CW_USEDEFAULT,
                CW_USEDEFAULT,
                0,
                0,
                parent as _,
                null_mut(),
                hinstance(),
                null(),
            );

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
            let gl_context = options
                .opengl
                .map(|config| GlContext::new(hwnd, config))
                .unwrap_or_else(|| Err(OpenGlError("no OpenGl config was provided".to_string())));

            // install the keyboard hook and register our window to it, so we could capture
            // key events even when the window is not focused. keyboard hooks are shared on
            // a per-thread basis and gets deregistered when all [`KeyboardHook`] instances
            // gets dropped
            let keyboard_hook = KeyboardHook::install();
            keyboard_hook.add_window(hwnd);

            // preload our cursors
            let cursor_cache = CursorCache::load();

            // construct our window data, here we store all our state and the event handler
            // to be called later
            let window = Rc::new(Self {
                waker: Arc::new(WindowWakerImpl {
                    window_hwnd: hwnd,
                    window_open: AtomicBool::new(true),
                }),

                current_dpi_scale: Cell::new(
                    try_get_dpi_for_window(hwnd).unwrap_or(USER_DEFAULT_SCREEN_DPI),
                ),
                current_mouse_capture: Cell::new(0),
                current_mouse_cursor: Cell::new(cursor_cache.get_closest(MouseCursor::Default)),
                current_key_modifiers: Cell::new(Modifiers::default()),
                current_window_focused: Cell::new(false),

                current_window_size: Cell::new(Size::default()),
                current_window_position: Cell::new(Point::default()),
                current_window_style: Cell::new((dwstyle, 0)),
                current_window_visibility: Cell::new(WindowVisibility::Normal),
                current_min_window_size: Cell::new(Size::MIN),
                current_max_window_size: Cell::new(Size::MAX),

                window_class,
                window_hwnd: hwnd,

                open_mode: mode,

                gl_context,
                cursor_cache,

                event_handler: RefCell::new(None),
                event_deferred: RefCell::new(VecDeque::new()),

                _drop_target: drop_target,
                keyboard_hook,
                vsync_thread: VSyncThread::new(hwnd),
            });

            // store our window data as the userdata for later retrieval
            SetWindowLongPtrW(hwnd, GWLP_USERDATA, Rc::into_raw(window.clone()) as _);

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

            // restore previous dpi awareness, has to be done here because the event handler
            // may call set_size and friends, and they have to run in dpi-aware mode
            if let Some(prev_dpi_awareness) = prev_dpi_awareness {
                try_set_thread_dpi_awareness(prev_dpi_awareness);
            }

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
        unsafe {
            let mut rect = RECT { ..zeroed() };
            let (dwstyle, dwexstyle) = self.current_window_style.get();
            if AdjustWindowRectEx(&mut rect, dwstyle, 0, dwexstyle) == 0 {
                return input;
            }

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
}

impl Drop for WindowImpl {
    fn drop(&mut self) {
        // subsequent wakeups should fail
        self.waker.window_open.store(false, Ordering::Release);

        // drop the handler here, so it could do clean up when the window is still alive
        // will ignore any events sent after this point, as the handler is gone
        self.event_handler.take();

        // remove the window from the keyboard hook
        self.keyboard_hook.remove_window(self.window_hwnd);

        // winapi cleanup stuff
        unsafe {
            RevokeDragDrop(self.window_hwnd);
            SetWindowLongPtrW(self.window_hwnd, GWLP_USERDATA, 0);
            UnregisterClassW(self.window_class as _, hinstance());
        }
    }
}

impl PlatformWindow for WindowImpl {
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

    fn close(&self) {
        unsafe {
            PostMessageW(self.window_hwnd, WM_USER_KILL_WINDOW, 0, 0);
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
            let window_title = to_widestring(title);
            SetWindowTextW(self.window_hwnd, window_title.as_ptr() as _);
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

            SetWindowLongW(self.window_hwnd, GWL_STYLE, style as _);
            self.current_window_style
                .update(|(_, exstyle)| (style, exstyle));

            // force a resize (restyling keeps the outer size while changing the inner size,
            // so we need to resize the window to keep the client size the same)
            self.set_size(self.current_window_size.replace(Size::default()));
        }
    }

    fn set_cursor_icon(&self, cursor: MouseCursor) {
        self.current_mouse_cursor
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
            // do nothing if the size doesnt change
            if self.current_window_size.get() == size {
                return;
            }

            let size = self.convert_client(Rect::from_size(size), true).size();
            SetWindowPos(
                self.window_hwnd,
                self.window_hwnd,
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
        // get our userdata that we set in [`WindowImpl::open`]
        let window_ptr = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut WindowImpl;

        // sometimes we get messages before OR _after_ (?) the window is
        // created/destroyed be defensive here
        if window_ptr.is_null() {
            return DefWindowProcW(hwnd, msg, wparam, lparam);
        }

        if msg == WM_DESTROY {
            // exit the event loop, if we are the owner of the event loop.
            // in parented modes, the parent drives the pump.
            if matches!((*window_ptr).open_mode, OpenMode::Blocking) {
                PostQuitMessage(0);
            }

            // drop the rc, will call our [`WindowImpl::drop`].
            drop(Rc::from_raw(window_ptr));
            return 0;
        }

        let window = &*window_ptr;
        match msg {
            WM_CLOSE => {
                window.deferred_event(|_, e| e.close_requested());
                return 0;
            }

            WM_DISPLAYCHANGE => {
                window.vsync_thread.notify_display_change();
            }

            WM_WINDOWPOSCHANGED => {
                let info = lparam as *const WINDOWPOS;

                if (*info).flags & SWP_SHOWWINDOW != 0 {
                    // just in case, we might be on a new display
                    window.vsync_thread.notify_display_change();
                }

                let visibility = if (*info).flags & SWP_HIDEWINDOW != 0 {
                    WindowVisibility::Hidden
                } else if (*info).flags & SWP_SHOWWINDOW != 0 {
                    WindowVisibility::Normal
                } else if (*info).x == -32000 && (*info).y == -32000 {
                    WindowVisibility::Minimized
                } else if window.current_window_visibility.get() == WindowVisibility::Hidden {
                    WindowVisibility::Hidden
                } else {
                    WindowVisibility::Normal
                };

                let rect = window.convert_client(
                    Rect {
                        left: (*info).x,
                        top: (*info).y,
                        right: (*info).x.saturating_add((*info).cx),
                        bottom: (*info).y.saturating_add((*info).cy),
                    },
                    false,
                );

                // update window visibility
                if window.current_window_visibility.replace(visibility) != visibility {
                    window.deferred_event(move |_, e| {
                        e.visibility_changed(visibility); // dont wanna miss any updates
                    });
                }

                // update window position
                if visibility != WindowVisibility::Minimized
                    && window.current_window_position.replace(rect.origin()) != rect.origin()
                {
                    window.deferred_event(move |window, e| {
                        e.position_changed(window.current_window_position.get()) // fine if we get a new value instead
                    });
                }

                // update window size
                if window.current_window_size.replace(rect.size()) != rect.size() {
                    window.deferred_event(move |window, e| {
                        e.size_changed(window.current_window_size.get()) // same as with position
                    });
                }

                return 0;
            }

            WM_DPICHANGED => {
                let dpi = (wparam & 0xFFFF) as u32;
                window.current_dpi_scale.set(dpi);

                // force a resize to update the client size, as the window size is not changed
                // dpi change _can_ cause a border size change, so we have to update the client
                // size to reflect that. if the user wants to resize the window afterwards based
                // on the new dpi, they can do so.
                window.set_size(window.current_window_size.replace(Size::default()));
                window.deferred_event(|window, e| e.scale_changed(window.scale()));

                return 0;
            }

            WM_STYLECHANGED => {
                let dwstyle = GetWindowLongW(hwnd, GWL_STYLE) as u32;
                let dwexstyle = GetWindowLongW(hwnd, GWL_EXSTYLE) as u32;
                window.current_window_style.set((dwstyle, dwexstyle));
            }

            WM_LBUTTONDOWN | WM_RBUTTONDOWN | WM_MBUTTONDOWN | WM_XBUTTONDOWN | WM_LBUTTONUP
            | WM_RBUTTONUP | WM_MBUTTONUP | WM_XBUTTONUP => {
                let button = match msg {
                    WM_LBUTTONUP | WM_LBUTTONDOWN => Some(MouseButton::Left),
                    WM_RBUTTONUP | WM_RBUTTONDOWN => Some(MouseButton::Right),
                    WM_MBUTTONUP | WM_MBUTTONDOWN => Some(MouseButton::Middle),
                    WM_XBUTTONUP | WM_XBUTTONDOWN => match ((wparam >> 16) & 0xffff) as u16 {
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
                    window.deferred_event(move |_, e| e.mouse_press(button, down));
                }

                if down {
                    window.current_mouse_capture.update(|x| x + 1);
                    if window.current_mouse_capture.get() == 1 {
                        SetCapture(hwnd);
                        SetFocus(hwnd);
                    }
                } else {
                    window.current_mouse_capture.update(|x| x.saturating_sub(1));
                    if window.current_mouse_capture.get() == 0 {
                        ReleaseCapture();
                    }
                }
            }

            WM_MOUSEWHEEL | WM_MOUSEHWHEEL => {
                let delta = (wparam >> 16) as i16;
                let delta = delta as f64 / WHEEL_DELTA as f64;

                let x = if msg == WM_MOUSEWHEEL { 0.0 } else { delta };
                let y = if msg == WM_MOUSEWHEEL { -delta } else { 0.0 };

                window.deferred_event(move |_, e| e.mouse_scroll(x, y));
            }

            WM_MOUSELEAVE => {
                window.deferred_event(move |_, e| e.mouse_leave());
            }

            WM_MOUSEMOVE => {
                let _ = TrackMouseEvent(&mut TRACKMOUSEEVENT {
                    cbSize: size_of::<TRACKMOUSEEVENT>() as u32,
                    dwFlags: TME_LEAVE,
                    hwndTrack: hwnd,
                    dwHoverTime: 0,
                });

                let (x, y) = (
                    (lparam & 0xFFFF) as i16 as f64,
                    ((lparam >> 16) & 0xFFFF) as i16 as f64,
                );

                window.deferred_event(move |_, e| e.mouse_move(Point { x, y }));
            }

            WM_SETCURSOR if lparam as u32 & 0xffff == HTCLIENT => {
                let cursor = window.current_mouse_cursor.get();

                if cursor.is_null() {
                    ShowCursor(0);
                } else {
                    SetCursor(cursor);
                    ShowCursor(1);
                }

                return 1;
            }

            WM_GETMINMAXINFO => {
                let info = lparam as *mut MINMAXINFO;
                let min = window.current_min_window_size.get();
                let max = window.current_max_window_size.get();
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

            WM_SETFOCUS if !window.current_window_focused.replace(true) => {
                window.deferred_event(|_, e| e.focus_changed(true));
            }

            WM_KILLFOCUS if window.current_window_focused.replace(false) => {
                window.deferred_event(|_, e| e.focus_changed(false));
            }

            WM_PAINT => {
                let mut rect = RECT { ..zeroed() };
                if GetUpdateRect(hwnd, &mut rect, 0) != 0 {
                    let rect = Rect {
                        left: rect.left,
                        top: rect.top,
                        right: rect.right,
                        bottom: rect.bottom,
                    };

                    window.deferred_event(move |_, e| e.damage(rect));
                    ValidateRgn(hwnd, null_mut());
                }

                return 0;
            }

            WM_USER_DND_ENTER => {
                let mut point = (lparam as *const POINT).read();
                if ScreenToClient(hwnd, &mut point) == 0 {
                    return 0;
                }

                let data = DropTargetImpl::decode_data_object(wparam as _);
                let point = Point {
                    x: point.x as f64,
                    y: point.y as f64,
                };

                let effect = window
                    .non_reentrant_event(|e| e.drag_enter(data, point))
                    .unwrap_or(DropEffect::Reject);

                return encode_dnd_effect(effect) as _;
            }

            WM_USER_DND_HOVER => {
                let mut point = (lparam as *const POINT).read();
                if ScreenToClient(hwnd, &mut point) == 0 {
                    return 0;
                }

                let point = Point {
                    x: point.x as f64,
                    y: point.y as f64,
                };

                let effect = window
                    .non_reentrant_event(|e| e.drag_move(point))
                    .unwrap_or(DropEffect::Reject);

                return encode_dnd_effect(effect) as _;
            }

            WM_USER_DND_ACCEPT => {
                let effect = window
                    .non_reentrant_event(|e| e.drag_accept())
                    .unwrap_or(DropEffect::Reject);

                return encode_dnd_effect(effect) as _;
            }

            WM_USER_DND_LEAVE => {
                window.non_reentrant_event(|e| e.drag_leave());
                return 0;
            }

            WM_USER_KEY_DOWN | WM_USER_KEY_UP => {
                let scan_code = ((lparam & 0x1ff_0000) >> 16) as u32;
                let Some(key) = scan_code_to_key(scan_code) else {
                    return 0;
                };

                let capture = window
                    .non_reentrant_event(|handler| handler.key_press(key, msg == WM_USER_KEY_DOWN))
                    .unwrap_or(false);

                return if capture { 1 } else { 0 };
            }

            WM_USER_VSYNC => {
                let modifiers = get_modifiers();

                window.non_reentrant_event(|e| {
                    if window.current_key_modifiers.replace(modifiers) != modifiers {
                        e.key_modifiers(modifiers);
                    }

                    e.frame();
                    window.vsync_thread.notify_frame_finished();
                });

                return 0;
            }

            WM_USER_WAKEUP => {
                window.deferred_event(|_, e| e.wakeup());
                return 0;
            }

            WM_USER_KILL_WINDOW => {
                DestroyWindow(hwnd);
                return 0;
            }

            _ => {}
        }

        DefWindowProcW(hwnd, msg, wparam, lparam)
    }
}
