use super::connection::Connection;
use super::gl::GlContext;
use super::util;
use crate::platform::x11::connection::ATOM_PICOVIEW_WAKEUP;
use crate::platform::x11::util::get_cursor;
use crate::platform::{OpenMode, PlatformWaker, PlatformWindow};
use crate::{
    Error, Event, Modifiers, MouseButton, MouseCursor, Point, Size, WakeupError, Window,
    WindowBuilder, WindowFactory, WindowWaker, rwh_06,
};
use libc::c_ulong;
use std::cell::{Cell, RefCell};
use std::ffi::CString;
use std::mem::zeroed;
use std::ptr::null_mut;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};
use x11::xlib::{
    Button1Mask, Button2Mask, Button3Mask, Button4Mask, Button5Mask, ButtonPress, ButtonPressMask,
    ButtonRelease, ButtonReleaseMask, CWCursor, CWEventMask, CWHeight, CWWidth, CWX, CWY,
    ClientMessage, ClientMessageData, ConfigureNotify, CopyFromParent, DestroyNotify,
    FocusChangeMask, FocusIn, FocusOut, InputOutput, KeyPress, KeyPressMask, KeyRelease,
    KeyReleaseMask, LeaveNotify, LeaveWindowMask, MotionNotify, NoEventMask, NotifyNormal,
    PMaxSize, PMinSize, PSize, PointerMotionMask, PropModeReplace, StructureNotifyMask,
    XChangeProperty, XChangeWindowAttributes, XClientMessageEvent, XConfigureWindow, XCreateWindow,
    XDestroyWindow, XEvent, XFlush, XFree, XMapWindow, XSendEvent, XSetWMName, XSetWMNormalHints,
    XSetWMProtocols, XSetWindowAttributes, XSizeHints, XStringListToTextProperty, XSync,
    XTextProperty, XUnmapWindow, XWarpPointer, XWindowChanges,
};

unsafe impl Send for WindowImpl {}

pub struct WindowImpl {
    window_id: c_ulong,

    connection: Connection,
    waker: Arc<WindowWakerImpl>,

    is_closed: Cell<bool>,
    is_destroyed: Cell<bool>,
    is_resizeable: bool,
    refresh_interval: Duration,

    last_modifiers: Cell<Modifiers>,
    last_cursor: Cell<MouseCursor>,
    last_cursor_in_bounds: Cell<bool>,
    last_window_position: Cell<Option<Point>>,
    last_window_size: Cell<Option<Size>>,
    last_window_visible: Cell<bool>,

    #[allow(clippy::type_complexity)]
    handler: RefCell<Option<Box<dyn FnMut(Event)>>>,
    gl_context: Option<GlContext>,
}

pub struct WindowWakerImpl {
    window_id: c_ulong,
    connection: Arc<Mutex<Connection>>,
}

impl WindowImpl {
    pub unsafe fn open(options: WindowBuilder, mode: OpenMode) -> Result<WindowWaker, Error> {
        unsafe {
            let connection = Connection::create()?;
            let parent_window_id = match mode {
                OpenMode::Blocking => connection.default_root(),
                OpenMode::Embedded(rwh_06::RawWindowHandle::Xcb(window)) => {
                    window.window.get() as u64
                }
                OpenMode::Embedded(rwh_06::RawWindowHandle::Xlib(window)) => window.window,
                OpenMode::Embedded(_) => return Err(Error::InvalidParent),
            };

            let window_id = XCreateWindow(
                connection.display(),
                parent_window_id,
                0,
                0,
                options.size.width as _,
                options.size.height as _,
                0,
                CopyFromParent,
                InputOutput as u32,
                null_mut(),
                CWEventMask,
                &mut XSetWindowAttributes {
                    event_mask: ButtonPressMask
                        | ButtonReleaseMask
                        | StructureNotifyMask
                        | KeyPressMask
                        | KeyReleaseMask
                        | LeaveWindowMask
                        | PointerMotionMask
                        | FocusChangeMask,
                    ..zeroed()
                },
            );

            let (min_size, max_size) = match options.resizable.clone() {
                None => (options.size, options.size),
                Some(range) => (range.start, range.end),
            };

            XSetWMProtocols(
                connection.display(),
                window_id,
                &mut connection.atom(c"WM_DELETE_WINDOW"),
                1,
            );

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

            let title =
                CString::new(options.title).map_err(|e| Error::PlatformError(e.to_string()))?;
            let mut text = XTextProperty { ..zeroed() };
            let status = XStringListToTextProperty(&mut (title.as_ptr() as *mut _), 1, &mut text);
            if status != 0 {
                XSetWMName(connection.display(), window_id, &mut text);
                XFree(text.value as *mut _);
            }

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
                match GlContext::new(&connection, window_id as _, config) {
                    Ok(gl) => Some(gl),
                    Err(_) if config.optional => None,
                    Err(e) => return Err(e),
                }
            } else {
                None
            };

            let refresh_interval =
                Duration::from_secs_f64(1.0 / connection.refresh_rate().unwrap_or(60.0));

            XSync(connection.display(), 0);
            connection.check_error().map_err(Error::PlatformError)?;

            let window = Box::new(Self {
                window_id,
                connection,
                waker: Arc::new(WindowWakerImpl {
                    window_id,
                    connection: Arc::new(Mutex::new(Connection::create()?)),
                }),

                is_closed: Cell::new(false),
                is_destroyed: Cell::new(false),
                is_resizeable: options.resizable.is_some(),
                refresh_interval,

                last_modifiers: Cell::new(Modifiers::empty()),
                last_cursor: Cell::new(MouseCursor::Default),
                last_window_position: Cell::new(None),
                last_window_size: Cell::new(None),
                last_window_visible: Cell::new(options.visible),
                last_cursor_in_bounds: Cell::new(false),

                handler: RefCell::new(None),
                gl_context,
            });

            match mode {
                OpenMode::Blocking => {
                    window.run_event_loop(options.factory)?;
                    Ok(WindowWaker::default())
                }
                OpenMode::Embedded(..) => {
                    let waker = window.waker();
                    thread::spawn(|| window.run_event_loop(options.factory).ok());
                    Ok(waker)
                }
            }
        }
    }

    #[allow(clippy::boxed_local)]
    fn run_event_loop(self: Box<Self>, factory: WindowFactory) -> Result<(), Error> {
        unsafe {
            // SAFETY: we erase the lifetime of WindowImpl; it should be safe to do so because:
            //  - because our window instance is boxed, it has a stable address for the whole lifetime of the window
            //  - we manually dispose of our handler before WindowImpl gets dropped (see drop impl)
            //  - we promise to not move WindowImpl (and by extension the handler) to a different thread (as that would violate the handler's !Send requirement)
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
                        self.handle_frame();
                        Duration::ZERO
                    }
                };

                XFlush(self.connection.display());
                self.connection
                    .check_error()
                    .map_err(Error::PlatformError)?;

                let num_events = self.connection.wait_for_events(wait_time)?;
                for _ in 0..num_events {
                    let event = self.connection.next_event()?;
                    self.handle_event(event);
                }
            }

            self.destroy()
        }
    }

    fn handle_frame(&self) {
        match &self.gl_context {
            Some(context) => {
                let scope = context.scope(&self.connection);
                self.send_event(Event::WindowFrame { gl: Some(&scope) });
            }
            None => {
                self.send_event(Event::WindowFrame { gl: None });
            }
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
                        self.is_closed.set(true);
                    }

                    if event.format == 32
                        && event.message_type == self.connection.atom(c"PICOVIEW_WAKEUP") as _
                    {
                        self.send_event(Event::Wakeup);
                    }
                }
                ConfigureNotify => {
                    let event = event.configure;
                    let is_synthetic = event.type_ & 0x80 == 0;

                    let origin = Point {
                        x: event.x as f32,
                        y: event.y as f32,
                    };

                    let size = Size {
                        width: event.width as u32,
                        height: event.height as u32,
                    };

                    if !is_synthetic
                        && self.last_window_position.replace(Some(origin)) != Some(origin)
                    {
                        self.send_event(Event::WindowMove { origin });
                    }

                    if self.last_window_size.replace(Some(size)) != Some(size) {
                        self.send_event(Event::WindowResize { size });
                    }
                }
                ButtonPress | ButtonRelease => {
                    let event = event.button;

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
                    let event = event.key;
                    self.handle_event_modifiers(
                        util::keymask_to_mods(event.state) | util::keycode_to_mods(event.keycode),
                    );

                    if let Some(key) = util::keycode_to_key(event.keycode) {
                        self.send_event(Event::KeyDown {
                            key,
                            capture: &mut false, //TODO: key capture?
                        });
                    }
                }
                KeyRelease => {
                    let event = event.key;
                    self.handle_event_modifiers(
                        util::keymask_to_mods(event.state) - util::keycode_to_mods(event.keycode),
                    );

                    if let Some(key) = util::keycode_to_key(event.keycode) {
                        self.send_event(Event::KeyUp {
                            key,
                            capture: &mut false, //TODO: key capture?
                        });
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
                    if event.mode != NotifyNormal {
                        return;
                    }

                    self.send_event(Event::WindowFocus {
                        focus: event.type_ == FocusIn,
                    });
                }
                DestroyNotify => {
                    self.is_closed.set(true);
                    self.is_destroyed.set(true);
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

    fn destroy(mut self) -> Result<(), Error> {
        unsafe {
            // handler MUST be dropped BEFORE `WindowImpl` gets dropped, as handler depends on WindowImpl
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
                .map_err(Error::PlatformError)?;

            Ok(())
        }
    }
}

impl PlatformWindow for WindowImpl {
    fn close(&self) {
        self.is_closed.set(true);
    }

    fn waker(&self) -> WindowWaker {
        WindowWaker(self.waker.clone())
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
            let cursor = get_cursor(&self.connection, cursor);
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
            if self.is_resizeable {
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
        None
    }

    fn set_clipboard_text(&self, _text: &str) -> bool {
        false
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
