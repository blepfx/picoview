use super::connection::Connection;
use super::gl::GlContext;
use super::util;
use crate::platform::OpenMode;
use crate::platform::x11::util::check_error;
use crate::{
    Error, Event, Modifiers, MouseButton, MouseCursor, Point, Size, Window, WindowBuilder,
    WindowHandler, rwh_06,
};
use std::cell::{Cell, RefCell};
use std::num::NonZero;
use std::ptr::NonNull;
use std::sync::Arc;
use std::sync::mpsc::{SyncSender, sync_channel};
use x11rb::connection::Connection as XConnection;
use x11rb::properties::{WmSizeHints, WmSizeHintsSpecification};
use x11rb::protocol::present::CompleteKind;
use x11rb::protocol::present::{self, ConnectionExt as ConnectionExtPresent};
use x11rb::protocol::xproto::KeyButMask;
use x11rb::{
    COPY_DEPTH_FROM_PARENT, COPY_FROM_PARENT,
    protocol::{
        Event as XEvent,
        xproto::{
            AtomEnum, ChangeWindowAttributesAux, ConfigureWindowAux,
            ConnectionExt as ConnectionExtXProto, CreateWindowAux, EventMask, PropMode,
            WindowClass,
        },
    },
    wrapper::ConnectionExt as ConnectionExtWrapper,
};

unsafe impl Send for WindowImpl {}

pub struct WindowImpl {
    window_id: u32,
    connection: Arc<Connection>,

    is_closed: Cell<bool>,
    is_destroyed: Cell<bool>,
    is_resizeable: bool,
    on_closed: SyncSender<()>,

    last_modifiers: Cell<Modifiers>,
    last_cursor: Cell<MouseCursor>,
    last_window_position: Cell<Option<Point>>,
    last_window_size: Cell<Option<Size>>,
    last_window_visible: Cell<bool>,

    handler: RefCell<Option<Box<dyn WindowHandler>>>,
    gl_context: Option<GlContext>,
}

impl WindowImpl {
    pub unsafe fn open(options: WindowBuilder, mode: OpenMode) -> Result<(), Error> {
        unsafe {
            let connection = Connection::get()?;

            let parent_window_id = match mode {
                OpenMode::Embedded(rwh_06::RawWindowHandle::Xcb(window)) => window.window.get(),
                OpenMode::Embedded(rwh_06::RawWindowHandle::Xlib(window)) => window.window as u32,
                OpenMode::Embedded(_) => {
                    return Err(Error::InvalidParent);
                }
                OpenMode::Blocking => connection.default_root().root,
            };

            let window_id = connection
                .xcb()
                .generate_id()
                .map_err(|e| Error::PlatformError(e.to_string()))?;

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
                .map_err(|e| Error::PlatformError(e.to_string()))?;

            connection
                .xcb()
                .change_property32(
                    PropMode::REPLACE,
                    window_id,
                    connection.atoms().WM_PROTOCOLS,
                    AtomEnum::ATOM,
                    &[connection.atoms().WM_DELETE_WINDOW],
                )
                .map_err(|e| Error::PlatformError(e.to_string()))?;

            let (min_size, max_size) = match options.resizable.clone() {
                None => (options.size, options.size),
                Some(range) => (range.start, range.end),
            };

            let mut size_hints = WmSizeHints::new();
            size_hints.size = Some((
                WmSizeHintsSpecification::ProgramSpecified,
                options.size.width.try_into().unwrap_or(i32::MAX),
                options.size.height.try_into().unwrap_or(i32::MAX),
            ));
            size_hints.max_size = Some((
                max_size.width.try_into().unwrap_or(i32::MAX),
                max_size.height.try_into().unwrap_or(i32::MAX),
            ));
            size_hints.min_size = Some((
                min_size.width.try_into().unwrap_or(i32::MAX),
                min_size.height.try_into().unwrap_or(i32::MAX),
            ));
            size_hints
                .set(connection.xcb(), window_id, AtomEnum::WM_NORMAL_HINTS)
                .map_err(|e| Error::PlatformError(e.to_string()))?;

            connection
                .xcb()
                .change_property8(
                    PropMode::REPLACE,
                    window_id,
                    AtomEnum::WM_NAME,
                    AtomEnum::STRING,
                    options.title.as_bytes(),
                )
                .map_err(|e| Error::PlatformError(e.to_string()))?;

            connection
                .xcb()
                .change_property32(
                    PropMode::REPLACE,
                    window_id,
                    connection.atoms()._NET_WM_WINDOW_TYPE,
                    AtomEnum::ATOM,
                    &[if options.decorations {
                        connection.atoms()._NET_WM_WINDOW_TYPE_NORMAL
                    } else {
                        connection.atoms()._NET_WM_WINDOW_TYPE_DOCK
                    }],
                )
                .map_err(|e| Error::PlatformError(e.to_string()))?;

            connection
                .xcb()
                .change_property32(
                    PropMode::REPLACE,
                    window_id,
                    connection.atoms()._MOTIF_WM_HINTS,
                    AtomEnum::ATOM,
                    &[0b10, 0, options.decorations as u32, 0, 0],
                )
                .map_err(|e| Error::PlatformError(e.to_string()))?;

            if options.visible {
                connection
                    .xcb()
                    .map_window(window_id)
                    .map_err(|e| Error::PlatformError(e.to_string()))?;
            }

            if let Some(position) = options.position {
                connection
                    .xcb()
                    .configure_window(
                        window_id,
                        &ConfigureWindowAux::new()
                            .x(position.x as i32)
                            .y(position.y as i32),
                    )
                    .map_err(|e| Error::PlatformError(e.to_string()))?;
            }

            connection
                .flush()
                .map_err(|e| Error::PlatformError(e.to_string()))?;

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
                    .map_err(|e| Error::PlatformError(e.to_string()))?;
                connection
                    .xcb()
                    .present_select_input(event_id, window_id, present::EventMask::COMPLETE_NOTIFY)
                    .map_err(|e| Error::PlatformError(e.to_string()))?;
                connection
                    .xcb()
                    .present_notify_msc(window_id, 0, 0, 1, 0)
                    .map_err(|e| Error::PlatformError(e.to_string()))?;
            }

            connection
                .flush()
                .map_err(|e| Error::PlatformError(e.to_string()))?;

            let (on_closed, when_closed) = sync_channel(0);

            let window = Self {
                window_id,
                connection: connection.clone(),

                on_closed,
                is_closed: Cell::new(false),
                is_destroyed: Cell::new(false),
                is_resizeable: options.resizable.is_some(),

                last_modifiers: Cell::new(Modifiers::empty()),
                last_cursor: Cell::new(MouseCursor::Default),
                last_window_position: Cell::new(None),
                last_window_size: Cell::new(None),
                last_window_visible: Cell::new(options.visible),

                handler: RefCell::new(None),
                gl_context,
            };

            window
                .handler
                .replace(Some((options.factory)(Window(&window))));

            window.send_event(Event::WindowScale {
                scale: connection.os_scale_dpi() as f32 / 96.0,
            });

            connection.add_window_event_loop(
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

            if matches!(mode, OpenMode::Blocking) {
                let _ = when_closed.recv();
            }

            Ok(())
        }
    }

    fn handle_frame(&self) {
        if self.is_closed.get() {
            return;
        }

        self.send_event(Event::WindowFrame {
            gl: self.gl_context.as_ref().map(|x| x as &dyn crate::GlContext),
        });

        self.handle_destroy();
    }

    fn handle_event(&self, event: &XEvent) {
        if self.is_closed.get() {
            return;
        }

        match event {
            XEvent::ClientMessage(event) => {
                if event.format == 32
                    && event.data.as_data32()[0] == self.connection.atoms().WM_DELETE_WINDOW
                {
                    self.is_closed.set(true);
                }
            }

            XEvent::ConfigureNotify(e) => {
                let is_synthetic = e.response_type & 0x80 == 0;
                let origin = Point {
                    x: e.x as f32,
                    y: e.y as f32,
                };

                let size = Size {
                    width: e.width as u32,
                    height: e.height as u32,
                };

                if !is_synthetic && self.last_window_position.replace(Some(origin)) != Some(origin)
                {
                    self.send_event(Event::WindowMove { origin });
                }

                if self.last_window_size.replace(Some(size)) != Some(size) {
                    self.send_event(Event::WindowResize { size });
                }
            }

            XEvent::ButtonPress(e) => {
                self.handle_modifiers(util::keymask2mods(e.state));

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
                    relative: Point {
                        x: e.event_x as f32,
                        y: e.event_y as f32,
                    },
                    absolute: Point {
                        x: e.root_x as f32,
                        y: e.root_y as f32,
                    },
                });
                self.send_event(event);
            }

            XEvent::ButtonRelease(e) => {
                self.handle_modifiers(util::keymask2mods(e.state));

                let button = match e.detail {
                    1 => MouseButton::Left,
                    2 => MouseButton::Middle,
                    3 => MouseButton::Right,
                    8 => MouseButton::Back,
                    9 => MouseButton::Forward,
                    _ => return,
                };

                self.send_event(Event::MouseMove {
                    relative: Point {
                        x: e.event_x as f32,
                        y: e.event_y as f32,
                    },
                    absolute: Point {
                        x: e.root_x as f32,
                        y: e.root_y as f32,
                    },
                });
                self.send_event(Event::MouseUp { button });
            }

            XEvent::KeyPress(e) => {
                self.handle_modifiers(util::keymask2mods(e.state) | util::hwcode2mods(e.detail));

                if let Some(key) = util::hwcode2key(e.detail) {
                    self.send_event(Event::KeyDown {
                        key,
                        capture: &mut false, //TODO: key capture?
                    });
                }
            }

            XEvent::KeyRelease(e) => {
                self.handle_modifiers(util::keymask2mods(e.state) - util::hwcode2mods(e.detail));

                if let Some(key) = util::hwcode2key(e.detail) {
                    self.send_event(Event::KeyUp {
                        key,
                        capture: &mut false,
                    });
                }
            }

            XEvent::MotionNotify(e) => {
                self.handle_modifiers(util::keymask2mods(e.state));
                self.send_event(Event::MouseMove {
                    relative: Point {
                        x: e.event_x as f32,
                        y: e.event_y as f32,
                    },
                    absolute: Point {
                        x: e.root_x as f32,
                        y: e.root_y as f32,
                    },
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

                self.send_event(Event::MouseLeave);
            }

            XEvent::FocusIn(_) => {
                self.send_event(Event::WindowFocus { focus: true });
            }

            XEvent::FocusOut(_) => {
                self.send_event(Event::WindowFocus { focus: false });
            }

            XEvent::PresentCompleteNotify(e) if !self.connection.is_manual_tick() => {
                if e.kind == CompleteKind::NOTIFY_MSC {
                    self.handle_frame();
                    self.connection
                        .xcb()
                        .present_notify_msc(self.window_id, 0, 0, 1, 0)
                        .ok();

                    check_error(self.connection.flush());
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
                self.is_closed.set(true);
                self.is_destroyed.set(true);
                self.on_closed.try_send(()).ok();
            }

            _ => {}
        }

        self.handle_destroy();
    }

    fn handle_modifiers(&self, modifiers: Modifiers) {
        if self.last_modifiers.replace(modifiers) != modifiers {
            self.send_event(Event::KeyModifiers { modifiers });
        }
    }

    fn handle_destroy(&self) {
        if !self.is_closed.get() || self.is_destroyed.replace(true) {
            return;
        }

        // drop the handler here, so it could do clean up when the window is still alive
        self.handler.take();
        self.connection.remove_window_event_loop(self.window_id);

        check_error(self.connection.xcb().destroy_window(self.window_id));
        check_error(self.connection.flush());
    }

    fn send_event(&self, e: Event) {
        if let Some(handler) = &mut *self.handler.borrow_mut() {
            handler.on_event(e, Window(self))
        }
    }
}

impl crate::platform::OsWindow for WindowImpl {
    fn close(&self) {
        self.is_closed.set(true);
    }

    fn window_handle(&self) -> rwh_06::RawWindowHandle {
        unsafe {
            rwh_06::RawWindowHandle::Xcb(rwh_06::XcbWindowHandle::new(NonZero::new_unchecked(
                self.window_id,
            )))
        }
    }

    fn display_handle(&self) -> rwh_06::RawDisplayHandle {
        rwh_06::RawDisplayHandle::Xcb(rwh_06::XcbDisplayHandle::new(
            NonNull::new(self.connection.raw_connection()),
            self.connection.default_screen_index(),
        ))
    }

    fn set_title(&self, title: &str) {
        if self.is_closed.get() {
            return;
        }

        check_error(self.connection.xcb().change_property8(
            PropMode::REPLACE,
            self.window_id,
            AtomEnum::WM_NAME,
            AtomEnum::STRING,
            title.as_bytes(),
        ));

        check_error(self.connection.flush());
    }

    fn set_cursor_icon(&self, cursor: MouseCursor) {
        if self.is_closed.get() || self.last_cursor.replace(cursor) == cursor {
            return;
        }

        let xid = self.connection.load_cursor(cursor);
        if xid != 0 {
            check_error(self.connection.xcb().change_window_attributes(
                self.window_id,
                &ChangeWindowAttributesAux::new().cursor(xid),
            ));

            check_error(self.connection.flush());
        }
    }

    fn set_cursor_position(&self, point: Point) {
        if self.is_closed.get() {
            return;
        }

        check_error(self.connection.xcb().warp_pointer(
            x11rb::NONE,
            self.window_id,
            0,
            0,
            0,
            0,
            point.x.round() as i16,
            point.y.round() as i16,
        ));

        check_error(self.connection.flush());
    }

    fn set_size(&self, size: Size) {
        if self.is_closed.get() || self.last_window_size.replace(Some(size)) == Some(size) {
            return;
        }

        let mut size_hints = WmSizeHints::new();
        let (w_i32, h_i32) = (
            size.width.try_into().unwrap_or(i32::MAX),
            size.height.try_into().unwrap_or(i32::MAX),
        );

        size_hints.size = Some((WmSizeHintsSpecification::ProgramSpecified, w_i32, h_i32));
        if !self.is_resizeable {
            size_hints.max_size = Some((w_i32, h_i32));
            size_hints.min_size = Some((w_i32, h_i32));
        }

        check_error(size_hints.set(
            self.connection.xcb(),
            self.window_id,
            AtomEnum::WM_NORMAL_HINTS,
        ));

        check_error(
            self.connection.xcb().configure_window(
                self.window_id,
                &ConfigureWindowAux::new()
                    .width(size.width)
                    .height(size.height),
            ),
        );

        check_error(self.connection.flush());
    }

    fn set_position(&self, point: Point) {
        if self.is_closed.get() || self.last_window_position.replace(Some(point)) == Some(point) {
            return;
        }

        check_error(
            self.connection.xcb().configure_window(
                self.window_id,
                &ConfigureWindowAux::new()
                    .x(point.x as i32)
                    .y(point.y as i32),
            ),
        );

        check_error(self.connection.flush());
    }

    fn set_visible(&self, visible: bool) {
        if self.is_closed.get() || self.last_window_visible.replace(visible) == visible {
            return;
        }

        if visible {
            let mut config = ConfigureWindowAux::new();
            if let Some(point) = self.last_window_position.get() {
                config.x = Some(point.x as i32);
                config.y = Some(point.y as i32);
            }

            if let Some(size) = self.last_window_size.get() {
                config.width = Some(size.width);
                config.height = Some(size.height);
            }

            check_error(self.connection.xcb().map_window(self.window_id));
            check_error(
                self.connection
                    .xcb()
                    .configure_window(self.window_id, &config),
            );
        } else {
            check_error(self.connection.xcb().unmap_window(self.window_id));
        }

        check_error(self.connection.flush());
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
