use super::connection::Connection;
use super::gl::GlContext;
use super::util;
use crate::platform::x11::connection::ATOM_PICOVIEW_WAKEUP;
use crate::platform::x11::util::{
    VisualConfig, get_position_relative, query_cursor, request_clipboard,
};
use crate::platform::{OpenMode, PlatformWaker, PlatformWindow};
use crate::{
    Event, Modifiers, MouseButton, MouseCursor, Point, Size, WakeupError, Window, WindowBuilder,
    WindowError, WindowFactory, WindowWaker, rwh_06,
};
use libc::c_ulong;
use std::cell::{Cell, RefCell};
use std::ffi::CString;
use std::mem::zeroed;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};
use x11::xlib::*;

unsafe impl Send for WindowImpl {}

pub struct WindowImpl {
    window_id: c_ulong,
    window_parent: Cell<c_ulong>,

    connection: Connection,
    waker: Arc<WindowWakerImpl>,
    refresh_interval: Duration,

    is_closed: Cell<bool>,
    is_destroyed: Cell<bool>,
    is_resizable: bool,

    last_modifiers: Cell<Modifiers>,
    last_cursor: Cell<MouseCursor>,
    last_cursor_in_bounds: Cell<bool>,
    last_window_position: Cell<Option<Point>>,
    last_window_size: Cell<Option<Size>>,
    last_window_visible: Cell<bool>,
    last_window_focused: Cell<bool>,

    clipboard_text_selection: RefCell<CString>,

    #[allow(clippy::type_complexity)]
    handler: RefCell<Option<Box<dyn FnMut(Event)>>>,
    gl_context: Option<GlContext>,
}

pub struct WindowWakerImpl {
    window_id: c_ulong,
    connection: Arc<Mutex<Connection>>,
}

impl WindowImpl {
    pub unsafe fn open(options: WindowBuilder, mode: OpenMode) -> Result<WindowWaker, WindowError> {
        unsafe {
            let connection = Connection::create()?;
            let window_parent = match mode.handle() {
                None => connection.default_root(),
                Some(rwh_06::RawWindowHandle::Xlib(handle)) => handle.window,
                Some(rwh_06::RawWindowHandle::Xcb(handle)) => handle.window.get() as u64,
                _ => return Err(WindowError::InvalidParent),
            };

            let visual_info = options
                .opengl
                .as_ref()
                .and_then(|config| {
                    GlContext::find_best_config(&connection, config, options.transparent).ok()
                })
                .or_else(|| {
                    VisualConfig::try_new_true_color(
                        &connection,
                        if options.transparent { 32 } else { 24 },
                    )
                })
                .unwrap_or(VisualConfig::copy_from_parent());

            let colormap = XCreateColormap(
                connection.display(),
                connection.default_root(),
                visual_info.visual,
                AllocNone,
            );

            let window_id = XCreateWindow(
                connection.display(),
                match mode {
                    OpenMode::Embedded(..) => window_parent,
                    _ => connection.default_root(),
                },
                0,
                0,
                options.size.width as _,
                options.size.height as _,
                0,
                visual_info.depth,
                InputOutput as u32,
                visual_info.visual,
                CWEventMask | CWColormap | CWBorderPixel,
                &mut XSetWindowAttributes {
                    border_pixel: 0,
                    colormap,
                    event_mask: ButtonPressMask
                        | ButtonReleaseMask
                        | StructureNotifyMask
                        | PropertyChangeMask
                        | KeyPressMask
                        | KeyReleaseMask
                        | LeaveWindowMask
                        | PointerMotionMask
                        | FocusChangeMask
                        | ExposureMask,
                    ..zeroed()
                },
            );

            if let OpenMode::Transient(..) = mode {
                XSetTransientForHint(connection.display(), window_id, window_parent);
            }

            XSetWMProtocols(
                connection.display(),
                window_id,
                &mut connection.atom(c"WM_DELETE_WINDOW"),
                1,
            );

            // resize stuff
            {
                let (min_size, max_size) = match options.resizable.clone() {
                    None => (options.size, options.size),
                    Some(range) => (range.start, range.end),
                };

                XSetWMNormalHints(
                    connection.display(),
                    window_id,
                    &mut XSizeHints {
                        flags: PMinSize | PMaxSize | PSize,
                        width: options.size.width.try_into().unwrap_or(i32::MAX),
                        height: options.size.height.try_into().unwrap_or(i32::MAX),
                        min_width: min_size.width.try_into().unwrap_or(i32::MAX),
                        min_height: min_size.height.try_into().unwrap_or(i32::MAX),
                        max_width: max_size.width.try_into().unwrap_or(i32::MAX),
                        max_height: max_size.height.try_into().unwrap_or(i32::MAX),
                        ..zeroed()
                    },
                );
            }

            // window title stuff
            {
                let title = CString::new(options.title)
                    .map_err(|e| WindowError::Platform(e.to_string()))?;
                let mut text = XTextProperty { ..zeroed() };
                let status =
                    XStringListToTextProperty(&mut (title.as_ptr() as *mut _), 1, &mut text);
                if status != 0 {
                    XSetWMName(connection.display(), window_id, &mut text);
                    XFree(text.value as *mut _);
                }
            }

            // decoration stuff
            {
                let data: [u32; 1] = [if options.decorations {
                    connection.atom(c"_NET_WM_WINDOW_TYPE_NORMAL") as u32
                } else {
                    connection.atom(c"_NET_WM_WINDOW_TYPE_DOCK") as u32
                }];

                XChangeProperty(
                    connection.display(),
                    window_id,
                    connection.atom(c"_NET_WM_WINDOW_TYPE"),
                    connection.atom(c"ATOM"),
                    32,
                    PropModeReplace,
                    data.as_ptr() as *mut _,
                    data.len() as _,
                );

                let data: [u32; 5] = [0b10, 0, options.decorations as u32, 0, 0];

                XChangeProperty(
                    connection.display(),
                    window_id,
                    connection.atom(c"_MOTIF_WM_HINTS"),
                    connection.atom(c"ATOM"),
                    32,
                    PropModeReplace,
                    data.as_ptr() as *mut _,
                    data.len() as _,
                );
            }

            if options.visible {
                XMapWindow(connection.display(), window_id);
            }

            if let Some(position) = options.position {
                XConfigureWindow(
                    connection.display(),
                    window_id,
                    (CWX | CWY) as _,
                    &mut XWindowChanges {
                        x: position.x as i32,
                        y: position.y as i32,
                        ..zeroed()
                    },
                );
            }

            let gl_context = if let Some(config) = options.opengl {
                GlContext::new(&connection, window_id as _, config, visual_info.fb_config).ok()
            } else {
                None
            };

            let refresh_interval =
                Duration::from_secs_f64(1.0 / connection.refresh_rate().unwrap_or(60.0));

            XSync(connection.display(), 0);
            connection.check_error().map_err(WindowError::Platform)?;

            let window = Box::new(Self {
                window_id,
                window_parent: Cell::new(window_parent),

                connection,
                waker: Arc::new(WindowWakerImpl {
                    window_id,
                    connection: Arc::new(Mutex::new(Connection::create()?)),
                }),

                is_closed: Cell::new(false),
                is_destroyed: Cell::new(false),
                is_resizable: options.resizable.is_some(),
                refresh_interval,

                last_modifiers: Cell::new(Modifiers::empty()),
                last_cursor: Cell::new(MouseCursor::Default),
                last_window_position: Cell::new(None),
                last_window_size: Cell::new(None),
                last_window_visible: Cell::new(options.visible),
                last_window_focused: Cell::new(false),
                last_cursor_in_bounds: Cell::new(false),

                clipboard_text_selection: RefCell::new(CString::default()),

                handler: RefCell::new(None),
                gl_context,
            });

            match mode {
                OpenMode::Blocking => {
                    window.run_event_loop(options.factory)?;
                    Ok(WindowWaker::default())
                }
                OpenMode::Embedded(..) | OpenMode::Transient(..) => {
                    let waker = window.waker();
                    thread::spawn(|| window.run_event_loop(options.factory).ok());
                    Ok(waker)
                }
            }
        }
    }

    #[allow(clippy::boxed_local)]
    fn run_event_loop(self: Box<Self>, factory: WindowFactory) -> Result<(), WindowError> {
        unsafe {
            // SAFETY: we erase the lifetime of WindowImpl; it should be safe to do so
            // because:
            //  - because our window instance is boxed, it has a stable address for the
            //    whole lifetime of the window
            //  - we manually dispose of our handler before WindowImpl gets dropped (see
            //    drop impl)
            //  - we promise to not move WindowImpl (and by extension the handler) to a
            //    different thread (as that would violate the handler's !Send requirement)
            self.handler
                .replace(Some((factory)(Window(&*(&*self as *const Self)))));

            self.send_event(Event::WindowScale {
                scale: self.connection.scale_dpi().map_or(1.0, |x| x / 96.0),
            });

            // main loop
            let mut next_frame = Instant::now();
            while !self.is_closed.get() {
                let curr_frame = Instant::now();
                let wait_time = match next_frame.checked_duration_since(curr_frame) {
                    Some(wait_time) => wait_time,
                    None => {
                        next_frame = (next_frame + self.refresh_interval).max(curr_frame);
                        self.send_event(Event::WindowFrame);
                        Duration::ZERO
                    }
                };

                XFlush(self.connection.display());
                self.connection
                    .check_error()
                    .map_err(WindowError::Platform)?;

                let num_events = self.connection.wait_for_events(Some(wait_time))?;
                for _ in 0..num_events {
                    let event = self.connection.next_event()?;
                    self.handle_event(event);
                }

                if num_events != 0 {
                    XFlush(self.connection.display());
                }
            }

            self.destroy()
        }
    }

    #[allow(non_upper_case_globals)]
    fn handle_event(&self, event: XEvent) {
        unsafe {
            match event.type_ {
                ClientMessage => {
                    let event = event.client_message;
                    if event.format == 32
                        && event.message_type == self.connection.atom(c"WM_PROTOCOLS") as _
                        && event.data.get_long(0) == self.connection.atom(c"WM_DELETE_WINDOW") as _
                    {
                        self.send_event(Event::WindowClose);
                    }

                    if event.format == 32
                        && event.message_type == self.connection.atom(c"PICOVIEW_WAKEUP") as _
                    {
                        self.send_event(Event::Wakeup);
                    }
                }

                ReparentNotify => {
                    let event = event.reparent;
                    self.window_parent.set(event.parent);
                }

                ConfigureNotify => {
                    let event = event.configure;

                    {
                        if let (Some(relative), Some(absolute)) = (
                            get_position_relative(
                                &self.connection,
                                self.window_parent.get(),
                                self.window_id,
                            ),
                            get_position_relative(
                                &self.connection,
                                self.connection.default_root(),
                                self.window_id,
                            ),
                        ) {
                            self.last_window_position.set(Some(relative));
                            self.send_event(Event::WindowMove { relative, absolute });
                        };
                    }

                    let size = Size {
                        width: event.width as u32,
                        height: event.height as u32,
                    };

                    if self.last_window_size.replace(Some(size)) != Some(size) {
                        self.send_event(Event::WindowResize { size });
                    }
                }

                ButtonPress | ButtonRelease => {
                    let event = event.button;
                    if event.type_ == ButtonPress {
                        XSetInputFocus(
                            self.connection.display(),
                            self.window_id,
                            RevertToParent,
                            CurrentTime,
                        );
                    }

                    self.handle_event_modifiers(util::keymask_to_mods(event.state));

                    let result = match event.button {
                        1 | 2 | 3 | 8 | 9 => {
                            let button = match event.button {
                                1 => MouseButton::Left,
                                2 => MouseButton::Middle,
                                3 => MouseButton::Right,
                                8 => MouseButton::Back,
                                9 => MouseButton::Forward,
                                _ => return,
                            };

                            if event.type_ == ButtonPress {
                                Event::MouseDown { button }
                            } else {
                                Event::MouseUp { button }
                            }
                        }

                        4..=7 if event.type_ == ButtonPress => match event.button {
                            4 => Event::MouseScroll { x: 0.0, y: 1.0 },
                            5 => Event::MouseScroll { x: 0.0, y: -1.0 },
                            6 => Event::MouseScroll { x: 1.0, y: 0.0 },
                            7 => Event::MouseScroll { x: -1.0, y: 0.0 },
                            _ => return,
                        },

                        _ => return,
                    };

                    self.send_event(Event::MouseMove {
                        relative: Point {
                            x: event.x as f32,
                            y: event.y as f32,
                        },
                        absolute: Point {
                            x: event.x_root as f32,
                            y: event.y_root as f32,
                        },
                    });

                    self.send_event(result);
                }
                KeyPress => {
                    let mut event = event.key;
                    self.handle_event_modifiers(
                        util::keymask_to_mods(event.state) | util::keycode_to_mods(event.keycode),
                    );

                    if let Some(key) = util::keycode_to_key(event.keycode) {
                        let mut capture = false;
                        self.send_event(Event::KeyDown {
                            key,
                            capture: &mut capture,
                        });

                        if !capture {
                            event.window = self.window_parent.get();
                            XSendEvent(
                                self.connection.display(),
                                self.window_parent.get(),
                                1,
                                KeyPressMask,
                                &mut XEvent { key: event },
                            );
                        }
                    }
                }
                KeyRelease => {
                    let mut event = event.key;
                    self.handle_event_modifiers(
                        util::keymask_to_mods(event.state) - util::keycode_to_mods(event.keycode),
                    );

                    if let Some(key) = util::keycode_to_key(event.keycode) {
                        let mut capture = false;
                        self.send_event(Event::KeyUp {
                            key,
                            capture: &mut capture,
                        });

                        if !capture {
                            event.window = self.window_parent.get();
                            XSendEvent(
                                self.connection.display(),
                                self.window_parent.get(),
                                1,
                                KeyReleaseMask,
                                &mut XEvent { key: event },
                            );
                        }
                    }
                }
                MotionNotify => {
                    let event = event.motion;
                    self.last_cursor_in_bounds.set(true);
                    self.handle_event_modifiers(util::keymask_to_mods(event.state));
                    self.send_event(Event::MouseMove {
                        relative: Point {
                            x: event.x as f32,
                            y: event.y as f32,
                        },
                        absolute: Point {
                            x: event.x_root as f32,
                            y: event.y_root as f32,
                        },
                    });
                }
                LeaveNotify => {
                    const ANY_BUTTON: u32 =
                        Button1Mask | Button2Mask | Button3Mask | Button4Mask | Button5Mask;

                    let event = event.crossing;

                    self.handle_event_modifiers(util::keymask_to_mods(event.state));

                    let grabbed = (event.state & ANY_BUTTON) != 0;
                    if grabbed || !self.last_cursor_in_bounds.replace(false) {
                        return;
                    }

                    self.send_event(Event::MouseMove {
                        relative: Point {
                            x: event.x as f32,
                            y: event.y as f32,
                        },
                        absolute: Point {
                            x: event.x_root as f32,
                            y: event.y_root as f32,
                        },
                    });
                    self.send_event(Event::MouseLeave);
                }
                FocusIn | FocusOut => {
                    let event = event.focus_change;
                    let focus = event.type_ == FocusIn;

                    if event.mode != NotifyNormal || event.window != self.window_id {
                        return;
                    }

                    if self.last_window_focused.replace(focus) != focus {
                        self.send_event(Event::WindowFocus { focus });
                    }
                }
                DestroyNotify => {
                    self.is_closed.set(true);
                    self.is_destroyed.set(true);
                }
                Expose => {
                    let event = event.expose;

                    self.send_event(Event::WindowDamage {
                        x: event.x.try_into().unwrap_or(0),
                        y: event.y.try_into().unwrap_or(0),
                        w: event.width.try_into().unwrap_or(0),
                        h: event.height.try_into().unwrap_or(0),
                    });
                }

                SelectionRequest => {
                    let text = self.clipboard_text_selection.borrow();
                    let event = event.selection_request;
                    if event.selection != self.connection.atom(c"CLIPBOARD") {
                        return;
                    }

                    if event.property != 0 {
                        if event.target == self.connection.atom(c"TARGETS") {
                            let atom = self.connection.atom(c"UTF8_STRING") as u32;
                            XChangeProperty(
                                self.connection.display(),
                                event.requestor,
                                event.property,
                                XA_ATOM,
                                32,
                                PropModeReplace,
                                &atom as *const _ as *const u8,
                                1,
                            );
                        } else if event.target == self.connection.atom(c"UTF8_STRING")
                            || event.target == XA_STRING
                        {
                            XChangeProperty(
                                self.connection.display(),
                                event.requestor,
                                event.property,
                                event.target,
                                8,
                                PropModeReplace,
                                text.as_bytes().as_ptr(),
                                text.as_bytes().len() as i32,
                            );
                        }
                    }

                    XSendEvent(
                        self.connection.display(),
                        event.requestor,
                        0,
                        NoEventMask,
                        &mut XEvent {
                            selection: x11::xlib::XSelectionEvent {
                                type_: SelectionNotify,
                                serial: 0,
                                send_event: 1,
                                display: event.display,
                                requestor: event.requestor,
                                selection: event.selection,
                                target: event.target,
                                property: event.property,
                                time: event.time,
                            },
                        },
                    );
                }

                _ => {}
            }
        }
    }

    fn handle_event_modifiers(&self, modifiers: Modifiers) {
        if self.last_modifiers.replace(modifiers) != modifiers {
            self.send_event(Event::KeyModifiers { modifiers });
        }
    }

    fn send_event(&self, e: Event) {
        if let Some(handler) = &mut *self.handler.borrow_mut() {
            handler(e)
        }
    }

    fn destroy(mut self) -> Result<(), WindowError> {
        unsafe {
            // handler MUST be dropped BEFORE `WindowImpl` gets dropped, as handler depends
            // on WindowImpl
            self.handler.take();

            if let Some(gl) = self.gl_context.take() {
                gl.close(&self.connection)
            }

            if !self.is_destroyed.get() {
                XDestroyWindow(self.connection.display(), self.window_id);
            }

            XSync(self.connection.display(), 0);
            self.connection
                .check_error()
                .map_err(WindowError::Platform)?;

            Ok(())
        }
    }
}

impl crate::GlContext for WindowImpl {
    fn get_proc_address(&self, name: &std::ffi::CStr) -> *const std::ffi::c_void {
        self.gl_context
            .as_ref()
            .expect("opengl unavailable")
            .scope(&self.connection)
            .get_proc_address(name)
    }

    fn make_current(&self, current: bool) -> Result<(), crate::MakeCurrentError> {
        self.gl_context
            .as_ref()
            .expect("opengl unavailable")
            .scope(&self.connection)
            .make_current(current)
    }

    fn swap_buffers(&self) -> Result<(), crate::SwapBuffersError> {
        self.gl_context
            .as_ref()
            .expect("opengl unavailable")
            .scope(&self.connection)
            .swap_buffers()
    }
}

impl PlatformWindow for WindowImpl {
    fn close(&self) {
        self.is_closed.set(true);
    }

    fn waker(&self) -> WindowWaker {
        WindowWaker(self.waker.clone())
    }

    fn opengl(&self) -> Option<&dyn crate::GlContext> {
        if self.gl_context.is_some() {
            Some(self)
        } else {
            None
        }
    }

    fn window_handle(&self) -> rwh_06::RawWindowHandle {
        rwh_06::RawWindowHandle::Xlib(rwh_06::XlibWindowHandle::new(self.window_id))
    }

    fn display_handle(&self) -> rwh_06::RawDisplayHandle {
        rwh_06::RawDisplayHandle::Xlib(self.connection.display_handle())
    }

    fn set_title(&self, title: &str) {
        if self.is_closed.get() {
            return;
        }

        if let Ok(title) = CString::new(title.to_owned()) {
            unsafe {
                let mut text = XTextProperty { ..zeroed() };
                let status =
                    XStringListToTextProperty(&mut (title.as_ptr() as *mut _), 1, &mut text);
                if status != 0 {
                    XSetWMName(self.connection.display(), self.window_id, &mut text);
                    XFree(text.value as *mut _);
                }
            }
        }
    }

    fn set_cursor_icon(&self, cursor: MouseCursor) {
        if self.is_closed.get() || self.last_cursor.replace(cursor) == cursor {
            return;
        }

        unsafe {
            let cursor = query_cursor(&self.connection, cursor);
            if cursor != 0 {
                XChangeWindowAttributes(
                    self.connection.display(),
                    self.window_id,
                    CWCursor,
                    &mut XSetWindowAttributes { cursor, ..zeroed() },
                );
            }
        }
    }

    fn set_cursor_position(&self, point: Point) {
        if self.is_closed.get() {
            return;
        }

        unsafe {
            XWarpPointer(
                self.connection.display(),
                0,
                self.window_id,
                0,
                0,
                0,
                0,
                point.x.round() as i32,
                point.y.round() as i32,
            );
        }
    }

    fn set_size(&self, size: Size) {
        if self.is_closed.get() || self.last_window_size.replace(Some(size)) == Some(size) {
            return;
        }

        let (width, height) = (
            size.width.try_into().unwrap_or(i32::MAX),
            size.height.try_into().unwrap_or(i32::MAX),
        );

        unsafe {
            if self.is_resizable {
                XSetWMNormalHints(
                    self.connection.display(),
                    self.window_id,
                    &mut XSizeHints {
                        flags: PSize,
                        width,
                        height,
                        ..zeroed()
                    },
                );
            } else {
                XSetWMNormalHints(
                    self.connection.display(),
                    self.window_id,
                    &mut XSizeHints {
                        flags: PSize | PMaxSize | PMinSize,
                        width,
                        height,
                        max_width: width,
                        max_height: height,
                        min_width: width,
                        min_height: height,
                        ..zeroed()
                    },
                );
            }

            XConfigureWindow(
                self.connection.display(),
                self.window_id,
                (CWWidth | CWHeight) as _,
                &mut XWindowChanges {
                    width,
                    height,
                    ..zeroed()
                },
            );
        }
    }

    fn set_position(&self, point: Point) {
        if self.is_closed.get() || self.last_window_position.replace(Some(point)) == Some(point) {
            return;
        }

        unsafe {
            XConfigureWindow(
                self.connection.display(),
                self.window_id,
                (CWX | CWY) as _,
                &mut XWindowChanges {
                    x: point.x as i32,
                    y: point.y as i32,
                    ..zeroed()
                },
            );
        }
    }

    fn set_visible(&self, visible: bool) {
        if self.is_closed.get() || self.last_window_visible.replace(visible) == visible {
            return;
        }

        unsafe {
            if visible {
                if let Some(point) = self.last_window_position.get() {
                    XConfigureWindow(
                        self.connection.display(),
                        self.window_id,
                        (CWX | CWY) as _,
                        &mut XWindowChanges {
                            x: point.x as i32,
                            y: point.y as i32,
                            ..zeroed()
                        },
                    );
                }

                if let Some(size) = self.last_window_size.get() {
                    XConfigureWindow(
                        self.connection.display(),
                        self.window_id,
                        (CWWidth | CWHeight) as _,
                        &mut XWindowChanges {
                            width: size.width.try_into().unwrap_or(i32::MAX),
                            height: size.height.try_into().unwrap_or(i32::MAX),
                            ..zeroed()
                        },
                    );
                }

                XMapWindow(self.connection.display(), self.window_id);
            } else {
                XUnmapWindow(self.connection.display(), self.window_id);
            }
        }
    }

    fn open_url(&self, url: &str) -> bool {
        util::open_url(url)
    }

    fn get_clipboard_text(&self) -> Option<String> {
        // TODO: verify if correct
        for atom in [self.connection.atom(c"UTF8_STRING"), XA_STRING] {
            if let Some(text) = request_clipboard(
                &self.connection,
                self.window_id,
                atom,
                |data, size| unsafe {
                    let slice = std::slice::from_raw_parts(data, size);
                    String::from_utf8_lossy(slice).into_owned()
                },
            ) {
                return Some(text);
            }
        }

        None
    }

    fn set_clipboard_text(&self, text: &str) -> bool {
        *self.clipboard_text_selection.borrow_mut() = match CString::new(text) {
            Ok(cstring) => cstring,
            Err(_) => return false,
        };

        unsafe {
            XSetSelectionOwner(
                self.connection.display(),
                self.connection.atom(c"CLIPBOARD"),
                self.window_id,
                CurrentTime,
            );
        }

        true
    }
}

impl PlatformWaker for WindowWakerImpl {
    fn wakeup(&self) -> Result<(), WakeupError> {
        let conn = self.connection.lock().expect("poisoned");

        unsafe {
            XSendEvent(
                conn.display(),
                self.window_id,
                1,
                NoEventMask,
                &mut XEvent {
                    client_message: XClientMessageEvent {
                        type_: ClientMessage,
                        serial: 0,
                        send_event: 1,
                        display: conn.display(),
                        window: self.window_id,
                        message_type: conn.atom(ATOM_PICOVIEW_WAKEUP),
                        format: 32,
                        data: ClientMessageData::new(),
                    },
                },
            );
            XFlush(conn.display());
        }

        // TODO: check if we are dead?
        Ok(())
    }
}
