use super::gl::GlContext;
use super::util::*;
use crate::platform::{OpenMode, PlatformOpenGl, PlatformWaker, PlatformWindow};
use crate::*;
use libc::c_ulong;
use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::ffi::{CStr, CString};
use std::mem::zeroed;
use std::os::unix::ffi::OsStrExt;
use std::sync::{Arc, RwLock};
use std::thread;
use std::time::{Duration, Instant};
use x11::xinput2::{
    XI_Enter, XI_HierarchyChanged, XI_Motion, XIAllDevices, XIDeviceEvent, XIEventMask,
    XIMaskIsSet, XISelectEvents, XISetMask,
};
use x11::xlib::*;

pub const ATOM_WAKEUP: &CStr = c"PICOVIEW_WAKEUP";

pub struct WindowImpl {
    window_id: c_ulong,
    window_parent: c_ulong,

    connection: Connection,
    waker: Arc<WindowWakerImpl>,
    refresh_interval: Duration,

    is_closed: Cell<bool>,
    is_destroyed: Cell<bool>,
    is_resizable: bool,

    last_modifiers: Cell<Modifiers>,
    last_cursor_icon: Cell<MouseCursor>,
    last_cursor_position: Cell<Option<(Point, Point)>>, // (relative, absolute)
    last_window_position: Cell<Option<Point>>,
    last_window_size: Cell<Option<Size>>,
    last_window_visible: Cell<bool>,
    last_window_focused: Cell<bool>,
    last_dragdrop_state: Cell<bool>,
    last_gesture_zoom: Cell<f32>,

    exchange_clipboard: RefCell<Exchange>,
    exchange_dragndrop: RefCell<Exchange>,

    cursor_empty: EmptyCursor,
    cursor_cache: RefCell<HashMap<MouseCursor, c_ulong>>,

    xi2_info: Option<XI2Info>,
    xi2_axes: RefCell<Vec<XI2DeviceAxis>>,

    #[allow(clippy::type_complexity)]
    handler: RefCell<Option<Box<dyn FnMut(Event)>>>,
    gl_context: Option<GlContext>,
}

pub struct WindowWakerImpl {
    window_id: c_ulong,
    display: RwLock<*mut Display>,
}

// while it is not really Send, we promise to only send it to a different thread
// once after init while the WindowImpl contains Connection which is !Send, as
// long as we move all instances of Connection to the other thread we should be
// ok TODO: maybe remove that?
unsafe impl Send for WindowImpl {}
unsafe impl Send for WindowWakerImpl {}
unsafe impl Sync for WindowWakerImpl {}

impl WindowImpl {
    pub unsafe fn open(options: WindowBuilder, mode: OpenMode) -> Result<WindowWaker, WindowError> {
        unsafe {
            let connection = Connection::open().ok_or_else(|| {
                WindowError::Platform("Failed to connect to X server".to_string())
            })?;

            let default_root = XDefaultRootWindow(connection.display());
            let window_parent = match mode.handle() {
                None => default_root,
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
                default_root,
                visual_info.visual,
                AllocNone,
            );

            let window_id = XCreateWindow(
                connection.display(),
                match mode {
                    OpenMode::Embedded(..) => window_parent,
                    _ => default_root,
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

            // transient hint (its not really a "parent" in the traditional sense)
            if let OpenMode::Transient(..) = mode {
                XSetTransientForHint(connection.display(), window_id, window_parent);
            }

            // ask for delete window messages
            XSetWMProtocols(
                connection.display(),
                window_id,
                &mut connection.atom(c"WM_DELETE_WINDOW"),
                1,
            );

            // xinput2 stuff
            let (xi2_info, xi2_axes) = match XI2Info::query(&connection) {
                Some(info) => {
                    let mut mask = [0; 4];
                    XISetMask(&mut mask, XI_Enter);
                    XISetMask(&mut mask, XI_Motion);
                    XISetMask(&mut mask, XI_HierarchyChanged);
                    XISetMask(&mut mask, XI_GesturePinchBegin);
                    XISetMask(&mut mask, XI_GesturePinchUpdate);
                    XISetMask(&mut mask, XI_GesturePinchEnd);
                    XISelectEvents(
                        connection.display(),
                        window_id,
                        &mut XIEventMask {
                            deviceid: XIAllDevices,
                            mask_len: 4,
                            mask: mask.as_mut_ptr(),
                        },
                        1,
                    );

                    (Some(info), XI2DeviceAxis::list(&connection))
                }

                None => (None, Vec::new()),
            };

            // drag n drop stuff
            {
                let data = [5u32];
                XChangeProperty(
                    connection.display(),
                    window_id,
                    connection.atom(c"XdndAware"),
                    connection.atom(c"ATOM"),
                    32,
                    PropModeReplace,
                    data.as_ptr() as *mut _,
                    data.len() as _,
                );
            }

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
                GlContext::new(
                    connection.clone(),
                    window_id as _,
                    config,
                    visual_info.fb_config,
                )
                .ok()
            } else {
                None
            };

            let refresh_interval =
                Duration::from_secs_f64(1.0 / query_refresh_rate(&connection).unwrap_or(60.0));

            XSync(connection.display(), 0);
            connection.last_error().map_err(WindowError::Platform)?;

            let window = Box::new(Self {
                window_id,
                window_parent,

                waker: Arc::new(WindowWakerImpl {
                    display: RwLock::new(connection.display()),
                    window_id,
                }),

                is_closed: Cell::new(false),
                is_destroyed: Cell::new(false),
                is_resizable: options.resizable.is_some(),
                refresh_interval,

                last_modifiers: Cell::new(Modifiers::empty()),
                last_cursor_icon: Cell::new(MouseCursor::Default),
                last_cursor_position: Cell::new(None),
                last_window_position: Cell::new(None),
                last_window_size: Cell::new(None),
                last_window_visible: Cell::new(options.visible),
                last_window_focused: Cell::new(false),
                last_dragdrop_state: Cell::new(false),
                last_gesture_zoom: Cell::new(1.0),

                exchange_clipboard: RefCell::new(Exchange::Empty),
                exchange_dragndrop: RefCell::new(Exchange::Empty),

                xi2_info,
                xi2_axes: RefCell::new(xi2_axes),

                cursor_empty: EmptyCursor::new(connection.clone()),
                cursor_cache: RefCell::new(HashMap::new()),

                handler: RefCell::new(None),
                gl_context,
                connection,
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

            if let Some(dpi) = query_scale_dpi(&self.connection) {
                self.send_event(Event::WindowScale { scale: dpi / 96.0 });
            }

            // main loop
            let mut next_frame = Instant::now();
            while !self.is_closed.get() {
                let curr_frame = Instant::now();
                let wait_time = match next_frame.checked_duration_since(curr_frame) {
                    Some(wait_time) => wait_time,
                    None => {
                        self.send_event(Event::WindowFrame);
                        next_frame = (next_frame + self.refresh_interval).max(curr_frame);
                        Duration::ZERO
                    }
                };

                XFlush(self.connection.display());

                self.connection
                    .last_error()
                    .map_err(WindowError::Platform)?;

                let num_events = wait_for_events(&self.connection, Some(wait_time))
                    .map_err(WindowError::Platform)?;

                for _ in 0..num_events {
                    let mut event = XEvent { type_: 0 };

                    if XNextEvent(self.connection.display(), &mut event) == 0 {
                        self.handle_event(event);
                    }
                }
            }

            self.destroy()
        }
    }

    #[allow(non_upper_case_globals)]
    fn handle_event(&self, event: XEvent) {
        unsafe {
            match event.type_ {
                GenericEvent => {
                    let mut event = event.generic_event_cookie;
                    let is_xi2 = self
                        .xi2_info
                        .as_ref()
                        .is_some_and(|info| event.extension == info.ext_opcode);

                    if is_xi2 {
                        if XGetEventData(self.connection.display(), &mut event) == 0 {
                            return;
                        }

                        match event.evtype {
                            XI_Motion => {
                                let event = &*(event.data as *const XIDeviceEvent);
                                let mask = std::slice::from_raw_parts(
                                    event.valuators.mask,
                                    event.valuators.mask_len as usize,
                                );

                                if event.sourceid == event.deviceid {
                                    self.handle_event_motion(
                                        event.event_x as f32,
                                        event.event_y as f32,
                                        event.root_x as f32,
                                        event.root_y as f32,
                                    );

                                    let mut scroll_x = 0.0;
                                    let mut scroll_y = 0.0;

                                    let mut values = event.valuators.values;
                                    for i in 0..event.valuators.mask_len * 8 {
                                        if !XIMaskIsSet(mask, i) {
                                            continue;
                                        }

                                        let value = {
                                            let value = *values;
                                            values = values.offset(1);
                                            value
                                        };

                                        if let Some(axis) =
                                            self.xi2_axes.borrow_mut().iter_mut().find(|axis| {
                                                axis.source_id == event.sourceid
                                                    && axis.valuator == i
                                            })
                                        {
                                            let delta = axis.track_position(value);
                                            if axis.is_horizontal {
                                                scroll_x += delta;
                                            } else {
                                                scroll_y += delta;
                                            }
                                        }
                                    }

                                    if scroll_x != 0.0 || scroll_y != 0.0 {
                                        self.send_event(Event::MouseScroll {
                                            x: scroll_x as f32,
                                            y: scroll_y as f32,
                                        });
                                    }
                                }
                            }

                            XI_Enter => {
                                for device in self.xi2_axes.borrow_mut().iter_mut() {
                                    device.reset_position(&self.connection);
                                }
                            }

                            XI_HierarchyChanged => {
                                self.xi2_axes.replace(XI2DeviceAxis::list(&self.connection));
                            }

                            XI_GesturePinchBegin | XI_GesturePinchUpdate | XI_GesturePinchEnd => {
                                let event = &*(event.data as *const XIGesturePinchEvent);

                                if event.header.evtype == XI_GesturePinchBegin {
                                    self.last_gesture_zoom.set(1.0);
                                }

                                let new_zoom = event.scale as f32;
                                let old_zoom = self.last_gesture_zoom.replace(new_zoom);

                                if new_zoom != old_zoom {
                                    self.send_event(Event::GestureZoom {
                                        scale: new_zoom / old_zoom,
                                    });
                                }

                                if event.delta_angle != 0.0 {
                                    self.send_event(Event::GestureRotate {
                                        angle: event.delta_angle as f32,
                                    });
                                }
                            }

                            _ => {}
                        }

                        XFreeEventData(self.connection.display(), &mut event);
                    }
                }

                ClientMessage => {
                    let event = event.client_message;
                    if event.format == 32
                        && event.message_type == self.connection.atom(c"WM_PROTOCOLS") as _
                        && event.data.get_long(0) == self.connection.atom(c"WM_DELETE_WINDOW") as _
                    {
                        self.send_event(Event::WindowClose);
                    }

                    if event.format == 32
                        && event.message_type == self.connection.atom(ATOM_WAKEUP) as _
                    {
                        self.send_event(Event::Wakeup);
                    }

                    if event.format == 32
                        && event.message_type == self.connection.atom(c"XdndPosition") as _
                    {
                        let Some(origin) = self.last_window_position.get() else {
                            return;
                        };

                        let (x, y) = {
                            let packed = event.data.get_long(2);
                            ((packed >> 16) as u16 as i16, packed as u16 as i16)
                        };

                        let point = Point {
                            x: x as f32 - origin.x,
                            y: y as f32 - origin.y,
                        };

                        if !self.last_dragdrop_state.replace(true) {
                            let timestamp = event.data.get_long(3) as c_ulong;
                            let data = match parse_selection(
                                &self.connection,
                                self.window_id,
                                self.connection.atom(c"XdndSelection"),
                                self.connection.atom(c"XdndSelection"),
                                timestamp,
                            ) {
                                Ok(exchange) => exchange,
                                Err(SelectionError::Empty) => Exchange::Empty,
                                Err(SelectionError::Recursive) => {
                                    self.exchange_dragndrop.borrow().clone()
                                }
                            };

                            self.send_event(Event::DragEnter { data, point });
                        } else {
                            self.send_event(Event::DragMove { point });
                        }

                        send_xdnd_feedback(
                            &self.connection,
                            self.window_id,
                            event.data.get_long(0) as c_ulong,
                            false,
                        );
                    }

                    if event.format == 32
                        && event.message_type == self.connection.atom(c"XdndLeave") as _
                    {
                        self.send_event(Event::DragLeave);
                        self.last_dragdrop_state.set(false);
                    }

                    if event.format == 32
                        && event.message_type == self.connection.atom(c"XdndDrop") as _
                    {
                        self.send_event(Event::DragAccept);
                        self.last_dragdrop_state.set(false);

                        send_xdnd_feedback(
                            &self.connection,
                            self.window_id,
                            event.data.get_long(0) as c_ulong,
                            true,
                        );
                    }
                }

                ConfigureNotify => {
                    let event = event.configure;
                    let size = Size {
                        width: event.width as u32,
                        height: event.height as u32,
                    };

                    if let Some(point) = window_position(&self.connection, self.window_id)
                        && self.last_window_position.replace(Some(point)) != Some(point)
                    {
                        self.send_event(Event::WindowMove { point });
                    }

                    if self.last_window_size.replace(Some(size)) != Some(size) {
                        self.send_event(Event::WindowResize { size });
                    }
                }

                ButtonPress | ButtonRelease => {
                    let event = event.button;
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

                        4..=7 if event.type_ == ButtonPress && self.xi2_info.is_none() => {
                            let (x, y) = match event.button {
                                4 => (0.0, -1.0),
                                5 => (0.0, 1.0),
                                6 => (-1.0, 0.0),
                                7 => (1.0, 0.0),
                                _ => return,
                            };

                            Event::MouseScroll { x, y }
                        }

                        _ => return,
                    };

                    if event.type_ == ButtonPress {
                        XSetInputFocus(
                            self.connection.display(),
                            self.window_id,
                            RevertToParent,
                            CurrentTime,
                        );
                    }

                    self.handle_event_modifiers(keymask_to_mods(event.state));
                    self.handle_event_motion(
                        event.x as f32,
                        event.y as f32,
                        event.x_root as f32,
                        event.y_root as f32,
                    );
                    self.send_event(result);
                }

                KeyPress | KeyRelease => {
                    let event = event.key;

                    self.handle_event_modifiers(keymask_to_mods(event.state));

                    if let Some(key) = keycode_to_key(event.keycode) {
                        let mut capture = false;

                        if event.type_ == KeyPress {
                            self.send_event(Event::KeyDown {
                                key,
                                capture: &mut capture,
                            });
                        } else {
                            self.send_event(Event::KeyUp {
                                key,
                                capture: &mut capture,
                            });
                        }

                        if !capture {
                            XSendEvent(
                                self.connection.display(),
                                self.window_parent,
                                1,
                                match event.type_ {
                                    KeyPress => KeyPressMask,
                                    _ => KeyReleaseMask,
                                },
                                &mut XEvent {
                                    key: XKeyEvent {
                                        window: self.window_parent,
                                        ..event
                                    },
                                },
                            );
                        }
                    }
                }

                MotionNotify => {
                    let event = event.motion;
                    self.handle_event_modifiers(keymask_to_mods(event.state));
                    self.handle_event_motion(
                        event.x as f32,
                        event.y as f32,
                        event.x_root as f32,
                        event.y_root as f32,
                    );
                }

                LeaveNotify => {
                    const ANY_BUTTON: u32 =
                        Button1Mask | Button2Mask | Button3Mask | Button4Mask | Button5Mask;

                    let event = event.crossing;
                    self.handle_event_modifiers(keymask_to_mods(event.state));
                    self.handle_event_motion(
                        event.x as f32,
                        event.y as f32,
                        event.x_root as f32,
                        event.y_root as f32,
                    );

                    let grabbed = (event.state & ANY_BUTTON) != 0;
                    if grabbed || self.last_cursor_position.replace(None).is_none() {
                        return;
                    }

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
                    let exchange = &*self.exchange_clipboard.borrow();
                    let event = event.selection_request;

                    if event.selection != self.connection.atom(c"CLIPBOARD") {
                        return;
                    }

                    let a_targets = self.connection.atom(c"TARGETS");
                    let a_utf8_string = self.connection.atom(c"UTF8_STRING");
                    let a_text_plain = self.connection.atom(c"text/plain");
                    let a_text_uri_list = self.connection.atom(c"text/uri-list");

                    if event.property != 0 {
                        if event.target == a_targets {
                            let atom = match exchange {
                                Exchange::Files(_) => a_text_uri_list,
                                Exchange::Empty | Exchange::Text(_) => a_utf8_string,
                            };

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
                        } else if (event.target == a_utf8_string
                            || event.target == a_text_plain
                            || event.target == XA_STRING)
                            && let Exchange::Text(text) = exchange
                        {
                            XChangeProperty(
                                self.connection.display(),
                                event.requestor,
                                event.property,
                                event.target,
                                8,
                                PropModeReplace,
                                text.as_ptr(),
                                text.len() as i32,
                            );
                        } else if event.target == a_text_uri_list
                            && let Exchange::Files(files) = exchange
                        {
                            let list = encode_uri_list(files);
                            XChangeProperty(
                                self.connection.display(),
                                event.requestor,
                                event.property,
                                event.target,
                                8,
                                PropModeReplace,
                                list.as_bytes().as_ptr(),
                                list.len() as i32,
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

                    // just in case
                    XFlush(self.connection.display());
                }

                _ => {}
            }
        }
    }

    fn handle_event_motion(&self, x: f32, y: f32, x_root: f32, y_root: f32) {
        let relative = Point { x, y };
        let absolute = Point {
            x: x_root,
            y: y_root,
        };

        if self
            .last_cursor_position
            .replace(Some((relative, absolute)))
            != Some((relative, absolute))
        {
            self.send_event(Event::MouseMove { relative, absolute });
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

    fn destroy(self) -> Result<(), WindowError> {
        unsafe {
            // we set waker display to null before sending the close event, to ensure that
            // any pending wakeups that get triggered by the close event will be
            // ignored, preventing a potential use-after-free in the
            // `WindowWakerImpl::wakeup` method
            if let Ok(mut display) = self.waker.display.write() {
                *display = std::ptr::null_mut();
            }

            // handler MUST be dropped BEFORE `WindowImpl` gets dropped, as handler depends
            // on WindowImpl
            self.handler.take();

            // kill the window itself
            if !self.is_destroyed.get() {
                XDestroyWindow(self.connection.display(), self.window_id);
            }

            // sync & error check
            XSync(self.connection.display(), 0);
            self.connection
                .last_error()
                .map_err(WindowError::Platform)?;

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

    fn opengl(&self) -> Option<&dyn PlatformOpenGl> {
        self.gl_context.as_ref().map(|gl| gl as &dyn PlatformOpenGl)
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
        if self.is_closed.get() || self.last_cursor_icon.replace(cursor) == cursor {
            return;
        }

        unsafe {
            let cursor = match cursor {
                MouseCursor::Hidden => self.cursor_empty.cursor(),
                cursor => *self
                    .cursor_cache
                    .borrow_mut()
                    .entry(cursor)
                    .or_insert_with(|| {
                        load_cursor_by_enum(&self.connection, cursor).unwrap_or_else(|| {
                            load_cursor_by_enum(&self.connection, MouseCursor::Default)
                                .unwrap_or_else(|| self.cursor_empty.cursor())
                        })
                    }),
            };

            XChangeWindowAttributes(
                self.connection.display(),
                self.window_id,
                CWCursor,
                &mut XSetWindowAttributes { cursor, ..zeroed() },
            );
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
        open_url(url)
    }

    fn get_clipboard(&self) -> Exchange {
        let a_clipboard = self.connection.atom(c"CLIPBOARD");
        let a_xsel_data = self.connection.atom(c"XSEL_DATA");

        match parse_selection(
            &self.connection,
            self.window_id,
            a_clipboard,
            a_xsel_data,
            CurrentTime,
        ) {
            Ok(exchange) => exchange,
            Err(SelectionError::Empty) => Exchange::Empty,
            Err(SelectionError::Recursive) => self.exchange_clipboard.borrow().clone(),
        }
    }

    fn set_clipboard(&self, data: Exchange) -> bool {
        let is_empty = matches!(data, Exchange::Empty);

        *self.exchange_clipboard.borrow_mut() = data;

        unsafe {
            XSetSelectionOwner(
                self.connection.display(),
                self.connection.atom(c"CLIPBOARD"),
                if is_empty { 0 } else { self.window_id },
                CurrentTime,
            );
        }

        true
    }
}

impl PlatformWaker for WindowWakerImpl {
    fn wakeup(&self) -> Result<(), WakeupError> {
        let display = self.display.read().map_err(|_| WakeupError)?;

        if display.is_null() {
            return Err(WakeupError);
        }

        unsafe {
            XSendEvent(
                *display,
                self.window_id,
                1,
                NoEventMask,
                &mut XEvent {
                    client_message: XClientMessageEvent {
                        type_: ClientMessage,
                        serial: 0,
                        send_event: 1,
                        display: *display,
                        window: self.window_id,
                        message_type: XInternAtom(*display, ATOM_WAKEUP.as_ptr(), 0),
                        format: 32,
                        data: ClientMessageData::new(),
                    },
                },
            );
            XFlush(*display);
        }

        Ok(())
    }
}
