use super::connection::Connection;
use super::gl::GlContext;
use super::util;
use crate::{
    Error, Event, EventHandler, EventResponse, Modifiers, MouseButton, MouseCursor, Point,
    RawHandle, Size, Window, WindowBuilder,
};
use std::mem::replace;
use std::sync::Arc;
use std::sync::mpsc::{SyncSender, sync_channel};
use x11rb::connection::Connection as XConnection;
use x11rb::properties::WmSizeHints;
use x11rb::protocol::present::CompleteKind;
use x11rb::protocol::present::{self, ConnectionExt as ConnectionExt3};
use x11rb::protocol::xproto::KeyButMask;
use x11rb::{
    COPY_DEPTH_FROM_PARENT, COPY_FROM_PARENT,
    protocol::{
        Event as XEvent,
        xproto::{
            AtomEnum, ChangeWindowAttributesAux, ConfigureWindowAux, ConnectionExt,
            CreateWindowAux, EventMask, GrabMode, PropMode, WindowClass,
        },
    },
    wrapper::ConnectionExt as ConnectionExt2,
};

unsafe impl Send for OsWindow {}

struct OsWindowInner {
    window_id: u32,
    connection: Arc<Connection>,

    is_closed: bool,
    on_closed: SyncSender<()>,

    last_modifiers: Modifiers,
    last_cursor: MouseCursor,
    last_window_position: Option<Point>,
    last_keyboard_focus: bool,
}

pub struct OsWindow {
    inner: OsWindowInner,
    handler: EventHandler,
    gl_context: Option<GlContext>,
}

impl OsWindow {
    pub unsafe fn open(options: WindowBuilder) -> Result<(), Error> {
        unsafe {
            let connection = Connection::get()?;

            let parent_window_id = match options.parent {
                Some(RawHandle::X11 { window, .. }) => window,
                Some(_) => return Err(Error::PlatformError("invalid parent handle".into())),
                None => connection.default_root().root,
            };

            let window_id = connection
                .xcb()
                .generate_id()
                .map_err(|_| Error::PlatformError("X11 connection error".into()))?;

            connection
                .xcb()
                .create_window(
                    COPY_DEPTH_FROM_PARENT,
                    window_id,
                    parent_window_id,
                    0,
                    0,
                    options.size.width as _,
                    options.size.height as _,
                    0,
                    WindowClass::INPUT_OUTPUT,
                    COPY_FROM_PARENT,
                    &CreateWindowAux::new().event_mask(
                        EventMask::EXPOSURE
                            | EventMask::BUTTON_PRESS
                            | EventMask::BUTTON_RELEASE
                            | EventMask::STRUCTURE_NOTIFY
                            | EventMask::KEY_PRESS
                            | EventMask::KEY_RELEASE
                            | EventMask::LEAVE_WINDOW
                            | EventMask::POINTER_MOTION
                            | EventMask::FOCUS_CHANGE,
                    ),
                )
                .map_err(|_| Error::PlatformError("X11 connection error".into()))?;

            connection
                .xcb()
                .change_property32(
                    PropMode::REPLACE,
                    window_id,
                    connection.atoms().WM_PROTOCOLS,
                    AtomEnum::ATOM,
                    &[connection.atoms().WM_DELETE_WINDOW],
                )
                .map_err(|_| Error::PlatformError("X11 connection error".into()))?;

            let mut size_hints = WmSizeHints::new();
            size_hints.base_size = Some((options.size.width as _, options.size.height as _));
            size_hints.max_size = size_hints.base_size;
            size_hints.min_size = size_hints.base_size;
            let _ = size_hints.set(connection.xcb(), window_id, AtomEnum::WM_NORMAL_HINTS);

            let _ = connection.xcb().change_property8(
                PropMode::REPLACE,
                window_id,
                AtomEnum::WM_NAME,
                AtomEnum::STRING,
                options.title.as_bytes(),
            );

            if !options.decorations {
                connection
                    .xcb()
                    .change_property32(
                        PropMode::REPLACE,
                        window_id,
                        connection.atoms()._NET_WM_WINDOW_TYPE,
                        AtomEnum::ATOM,
                        &[connection.atoms()._NET_WM_WINDOW_TYPE_DOCK],
                    )
                    .map_err(|_| Error::PlatformError("X11 connection error".into()))?;
            }

            if options.visible {
                connection
                    .xcb()
                    .map_window(window_id)
                    .map_err(|_| Error::PlatformError("X11 connection error".into()))?;
            }

            let gl_context = if let Some(config) = options.opengl {
                match GlContext::new(connection.clone(), window_id as _, config) {
                    Ok(gl) => Some(gl),
                    Err(_) if config.optional => None,
                    Err(e) => return Err(e),
                }
            } else {
                None
            };

            if !connection.is_manual_tick() {
                let event_id = connection
                    .xcb()
                    .generate_id()
                    .map_err(|_| Error::PlatformError("X11 connection error".into()))?;
                connection
                    .xcb()
                    .present_select_input(event_id, window_id, present::EventMask::COMPLETE_NOTIFY)
                    .map_err(|_| Error::PlatformError("X11 connection error".into()))?;
                connection
                    .xcb()
                    .present_notify_msc(window_id, 0, 0, 1, 0)
                    .map_err(|_| Error::PlatformError("X11 connection error".into()))?;
            }

            if !connection.flush() {
                return Err(Error::PlatformError("X11 connection error".into()));
            }

            let (on_closed, when_closed) = sync_channel(0);
            let mut window = Self {
                handler: options.handler,
                gl_context,

                inner: OsWindowInner {
                    window_id,
                    connection: connection.clone(),

                    on_closed,
                    is_closed: false,

                    last_modifiers: Modifiers::empty(),
                    last_cursor: MouseCursor::Default,
                    last_window_position: None,
                    last_keyboard_focus: false,
                },
            };

            window.send_event(Event::WindowOpen);

            connection.add_window(
                window_id,
                Box::new(move |event| match event {
                    Some(event) => {
                        window.handle_event(event);
                    }
                    None => {
                        window.handle_frame();
                    }
                }),
            );

            if options.parent.is_none() {
                let _ = when_closed.recv();
            }

            Ok(())
        }
    }

    fn handle_frame(&mut self) {
        if self.inner.is_closed {
            return;
        }

        if let Some(gl) = self.gl_context.as_mut() {
            unsafe {
                if gl.set_current(true) {
                    (self.handler)(Event::WindowFrame { gl: Some(gl) }, Window(&mut self.inner));
                    gl.set_current(false);
                } else {
                    self.send_event(Event::WindowFrame { gl: None });
                }
            }
        } else {
            self.send_event(Event::WindowFrame { gl: None });
        }
    }

    fn handle_event(&mut self, event: &XEvent) {
        if self.inner.is_closed {
            return;
        }

        match event {
            XEvent::ClientMessage(event) => {
                if event.format == 32
                    && event.data.as_data32()[0] == self.inner.connection.atoms().WM_DELETE_WINDOW
                {
                    self.send_event(Event::WindowClose);
                }
            }

            XEvent::ButtonPress(e) => {
                self.handle_modifiers(util::keymask2mods(e.state));

                let position = Point {
                    x: e.event_x as f32,
                    y: e.event_y as f32,
                };

                let event = match e.detail {
                    1 => Event::MouseDown {
                        button: MouseButton::Left,
                    },
                    2 => Event::MouseDown {
                        button: MouseButton::Middle,
                    },
                    3 => Event::MouseDown {
                        button: MouseButton::Right,
                    },
                    8 => Event::MouseDown {
                        button: MouseButton::Back,
                    },
                    9 => Event::MouseDown {
                        button: MouseButton::Forward,
                    },
                    4 => Event::MouseScroll { x: 0.0, y: 1.0 },
                    5 => Event::MouseScroll { x: 0.0, y: -1.0 },
                    6 => Event::MouseScroll { x: 1.0, y: 0.0 },
                    7 => Event::MouseScroll { x: -1.0, y: 0.0 },
                    _ => return,
                };

                self.send_event(Event::MouseMove {
                    cursor: Some(position),
                });
                self.send_event(event);
            }

            XEvent::ButtonRelease(e) => {
                self.handle_modifiers(util::keymask2mods(e.state));

                let position = Point {
                    x: e.event_x as f32,
                    y: e.event_y as f32,
                };

                let button = match e.detail {
                    1 => MouseButton::Left,
                    2 => MouseButton::Middle,
                    3 => MouseButton::Right,
                    8 => MouseButton::Back,
                    9 => MouseButton::Forward,
                    _ => return,
                };

                self.send_event(Event::MouseMove {
                    cursor: Some(position),
                });
                self.send_event(Event::MouseUp { button });
            }

            XEvent::KeyPress(e) => {
                self.handle_modifiers(util::keymask2mods(e.state) | util::hwcode2mods(e.detail));

                if let Some(key) = util::hwcode2key(e.detail) {
                    self.send_event(Event::KeyDown { key });
                }
            }

            XEvent::KeyRelease(e) => {
                self.handle_modifiers(util::keymask2mods(e.state) - util::hwcode2mods(e.detail));

                if let Some(key) = util::hwcode2key(e.detail) {
                    self.send_event(Event::KeyUp { key });
                }
            }

            XEvent::MotionNotify(e) => {
                self.handle_modifiers(util::keymask2mods(e.state));
                self.send_event(Event::MouseMove {
                    cursor: Some(Point {
                        x: e.event_x as f32,
                        y: e.event_y as f32,
                    }),
                });
            }

            XEvent::LeaveNotify(e) => {
                let grabbed = e.state.intersects(
                    KeyButMask::BUTTON1
                        | KeyButMask::BUTTON2
                        | KeyButMask::BUTTON3
                        | KeyButMask::BUTTON4
                        | KeyButMask::BUTTON5,
                );

                if grabbed {
                    return;
                }

                self.send_event(Event::MouseMove { cursor: None });
            }

            XEvent::FocusIn(_) => {
                self.send_event(Event::WindowFocus);
            }

            XEvent::FocusOut(_) => {
                self.send_event(Event::WindowBlur);
            }

            XEvent::PresentCompleteNotify(e) if !self.inner.connection.is_manual_tick() => {
                if e.kind == CompleteKind::NOTIFY_MSC {
                    self.handle_frame();
                    self.inner
                        .connection
                        .xcb()
                        .present_notify_msc(self.inner.window_id, 0, 0, 1, 0)
                        .ok();
                    self.inner.connection.flush();
                }
            }

            XEvent::Expose(e) => {
                self.send_event(Event::WindowInvalidate {
                    top: e.y as u32,
                    left: e.x as u32,
                    bottom: e.y as u32 + e.height as u32,
                    right: e.x as u32 + e.width as u32,
                });
            }

            XEvent::DestroyNotify(_) => {
                self.inner.is_closed = true;
                self.inner.on_closed.try_send(()).ok();
            }

            _ => {}
        }
    }

    fn handle_modifiers(&mut self, modifiers: Modifiers) {
        if modifiers != self.inner.last_modifiers {
            self.inner.last_modifiers = modifiers;
            self.send_event(Event::KeyModifiers { modifiers });
        }
    }

    fn send_event(&mut self, e: Event) -> EventResponse {
        (self.handler)(e, Window(&mut self.inner))
    }
}

impl crate::platform::OsWindow for OsWindowInner {
    fn close(&mut self) {
        if replace(&mut self.is_closed, true) {
            return;
        }

        self.connection.remove_window(self.window_id);
        let _ = self.connection.xcb().destroy_window(self.window_id);
        self.connection.flush();
    }

    fn handle(&self) -> RawHandle {
        RawHandle::X11 {
            window: self.window_id,
        }
    }

    fn set_title(&mut self, title: &str) {
        let _ = self.connection.xcb().change_property8(
            PropMode::REPLACE,
            self.window_id,
            AtomEnum::WM_NAME,
            AtomEnum::STRING,
            title.as_bytes(),
        );

        self.connection.flush();
    }

    fn set_cursor_icon(&mut self, cursor: MouseCursor) {
        if self.is_closed || replace(&mut self.last_cursor, cursor) == cursor {
            return;
        }

        let xid = self.connection.load_cursor(cursor);
        if xid != 0 {
            let _ = self.connection.xcb().change_window_attributes(
                self.window_id,
                &ChangeWindowAttributesAux::new().cursor(xid),
            );
            self.connection.flush();
        }
    }

    fn set_cursor_position(&mut self, point: Point) {
        if self.is_closed {
            return;
        }

        let _ = self.connection.xcb().warp_pointer(
            x11rb::NONE,
            self.window_id,
            0,
            0,
            0,
            0,
            point.x.round() as i16,
            point.y.round() as i16,
        );
        self.connection.flush();
    }

    fn set_size(&mut self, size: Size) {
        if self.is_closed {
            return;
        }

        let _ = self.connection.xcb().configure_window(
            self.window_id,
            &ConfigureWindowAux::new()
                .width(size.width as u32)
                .height(size.height as u32),
        );

        let mut size_hints = WmSizeHints::new();
        size_hints.base_size = Some((size.width as _, size.height as _));
        size_hints.max_size = size_hints.base_size;
        size_hints.min_size = size_hints.base_size;
        let _ = size_hints.set(
            self.connection.xcb(),
            self.window_id,
            AtomEnum::WM_NORMAL_HINTS,
        );

        self.connection.flush();
    }

    fn set_position(&mut self, point: Point) {
        if self.is_closed {
            return;
        }

        let _ = self.connection.xcb().configure_window(
            self.window_id,
            &ConfigureWindowAux::new()
                .x(point.x as i32)
                .y(point.y as i32),
        );
        self.connection.flush();
        self.last_window_position = Some(point);
    }

    fn set_visible(&mut self, visible: bool) {
        if self.is_closed {
            return;
        }

        if visible {
            let _ = self.connection.xcb().map_window(self.window_id);
            if let Some(point) = self.last_window_position {
                let _ = self.connection.xcb().configure_window(
                    self.window_id,
                    &ConfigureWindowAux::new()
                        .x(point.x as i32)
                        .y(point.y as i32),
                );
            }
        } else {
            let _ = self.connection.xcb().unmap_window(self.window_id);
        }

        self.connection.flush();
    }

    fn set_keyboard_input(&mut self, focus: bool) {
        if self.is_closed || replace(&mut self.last_keyboard_focus, focus) == focus {
            return;
        }

        if focus {
            let _ = self.connection.xcb().grab_keyboard(
                false,
                self.window_id,
                x11rb::CURRENT_TIME,
                GrabMode::ASYNC,
                GrabMode::ASYNC,
            );
        } else {
            let _ = self.connection.xcb().ungrab_keyboard(x11rb::CURRENT_TIME);
        }

        self.connection.flush();
    }

    fn open_url(&mut self, url: &str) -> bool {
        util::open_url(url)
    }

    fn get_clipboard_text(&mut self) -> Option<String> {
        None
    }

    fn set_clipboard_text(&mut self, _text: &str) -> bool {
        false
    }
}
