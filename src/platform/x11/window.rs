use super::connection::Connection;
use super::gl::GlContext;
use super::util;
use crate::platform::OpenMode;
use crate::{
    Error, Event, Modifiers, MouseButton, MouseCursor, Point, Size, Window, WindowBuilder,
    WindowHandler, rwh_06,
};
use std::mem::replace;
use std::num::NonZero;
use std::ptr::NonNull;
use std::sync::Arc;
use std::sync::mpsc::{SyncSender, sync_channel};
use x11rb::connection::Connection as XConnection;
use x11rb::properties::WmSizeHints;
use x11rb::protocol::present::CompleteKind;
use x11rb::protocol::present::{self, ConnectionExt as ConnectionExtPresent};
use x11rb::protocol::xproto::{ColormapAlloc, KeyButMask, VisualClass};
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

struct WindowInner {
    window_id: u32,
    connection: Arc<Connection>,

    is_closed: bool,
    is_destroyed: bool,
    on_closed: SyncSender<()>,

    last_modifiers: Modifiers,
    last_cursor: MouseCursor,
    last_window_position: Option<Point>,
    last_window_size: Option<Size>,
    last_window_visible: bool,
    last_window_title: String,
}

pub struct WindowImpl {
    inner: WindowInner,
    handler: Box<dyn WindowHandler>,
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
                .map_err(|_| Error::PlatformError("X11 connection error".into()))?;

            let (visual, depth) = if options.transparent {
                connection
                    .default_root()
                    .allowed_depths
                    .iter()
                    .flat_map(|depth| {
                        depth
                            .visuals
                            .iter()
                            .map(move |visual| (visual, depth.depth))
                    })
                    .find(|(visual, depth)| visual.class == VisualClass::TRUE_COLOR && *depth == 32)
                    .map(|(visual, depth)| (visual.visual_id, depth))
                    .unwrap_or((COPY_FROM_PARENT, COPY_DEPTH_FROM_PARENT))
            } else {
                (COPY_FROM_PARENT, COPY_DEPTH_FROM_PARENT)
            };

            let colormap = if visual != COPY_FROM_PARENT {
                let colormap_id = connection
                    .xcb()
                    .generate_id()
                    .map_err(|_| Error::PlatformError("X11 connection error".into()))?;

                connection
                    .xcb()
                    .create_colormap(ColormapAlloc::NONE, colormap_id, parent_window_id, visual)
                    .map_err(|_| Error::PlatformError("X11 connection error".into()))?;

                colormap_id
            } else {
                0
            };

            connection
                .xcb()
                .create_window(
                    depth,
                    window_id,
                    parent_window_id,
                    0,
                    0,
                    options.size.width as _,
                    options.size.height as _,
                    0,
                    WindowClass::INPUT_OUTPUT,
                    visual,
                    &CreateWindowAux::new().colormap(colormap).event_mask(
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
            size_hints
                .set(connection.xcb(), window_id, AtomEnum::WM_NORMAL_HINTS)
                .map_err(|_| Error::PlatformError("X11 connection error".into()))?;

            connection
                .xcb()
                .change_property8(
                    PropMode::REPLACE,
                    window_id,
                    AtomEnum::WM_NAME,
                    AtomEnum::STRING,
                    options.title.as_bytes(),
                )
                .map_err(|_| Error::PlatformError("X11 connection error".into()))?;

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
                .map_err(|_| Error::PlatformError("X11 connection error".into()))?;

            connection
                .xcb()
                .change_property32(
                    PropMode::REPLACE,
                    window_id,
                    connection.atoms()._MOTIF_WM_HINTS,
                    AtomEnum::ATOM,
                    &[0b10, 0, options.decorations as u32, 0, 0],
                )
                .map_err(|_| Error::PlatformError("X11 connection error".into()))?;

            if options.visible {
                connection
                    .xcb()
                    .map_window(window_id)
                    .map_err(|_| Error::PlatformError("X11 connection error".into()))?;
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
                    .map_err(|_| Error::PlatformError("X11 connection error".into()))?;
            }

            if !connection.flush() {
                return Err(Error::PlatformError("X11 connection error".into()));
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
            let mut inner = WindowInner {
                window_id,
                connection: connection.clone(),

                on_closed,
                is_closed: false,
                is_destroyed: false,

                last_modifiers: Modifiers::empty(),
                last_cursor: MouseCursor::Default,
                last_window_position: None,
                last_window_size: None,
                last_window_visible: options.visible,
                last_window_title: options.title,
            };

            let mut window = Self {
                handler: (options.factory)(Window(&mut inner)),
                gl_context,
                inner,
            };

            window.send_event(Event::WindowScale {
                scale: connection.os_scale_dpi() as f32 / 96.0,
            });

            connection.add_window_pacer(
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

    fn handle_frame(&mut self) {
        if self.inner.is_closed {
            return;
        }

        self.handler.on_event(
            Event::WindowFrame {
                gl: self.gl_context.as_ref().map(|x| x as &dyn crate::GlContext),
            },
            Window(&mut self.inner),
        );

        self.handle_destroy();
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
                    self.inner.is_closed = true;
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

                if !is_synthetic
                    && replace(&mut self.inner.last_window_position, Some(origin)) != Some(origin)
                {
                    self.send_event(Event::WindowMove { origin });
                }

                if replace(&mut self.inner.last_window_size, Some(size)) != Some(size) {
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
                self.inner.is_destroyed = true;
                self.inner.on_closed.try_send(()).ok();
            }

            _ => {}
        }

        self.handle_destroy();
    }

    fn handle_modifiers(&mut self, modifiers: Modifiers) {
        if modifiers != self.inner.last_modifiers {
            self.inner.last_modifiers = modifiers;
            self.send_event(Event::KeyModifiers { modifiers });
        }
    }

    fn handle_destroy(&mut self) {
        if !self.inner.is_closed {
            return;
        }

        if replace(&mut self.inner.is_destroyed, true) {
            return;
        }

        // drop the handler here, so it could do clean up when the window is still alive
        self.handler = Box::new(|_: Event<'_>, _: Window<'_>| {});
        self.inner
            .connection
            .remove_window_pacer(self.inner.window_id);
        let _ = self
            .inner
            .connection
            .xcb()
            .destroy_window(self.inner.window_id);
        self.inner.connection.flush();
    }

    fn send_event(&mut self, e: Event) {
        self.handler.on_event(e, Window(&mut self.inner))
    }
}

impl crate::platform::OsWindow for WindowInner {
    fn close(&mut self) {
        self.is_closed = true;
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

    fn set_title(&mut self, title: &str) {
        if self.is_closed || title == self.last_window_title {
            return;
        }

        self.last_window_title = title.to_owned();

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
        if self.is_closed || replace(&mut self.last_window_size, Some(size)) == Some(size) {
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
        if self.is_closed || replace(&mut self.last_window_position, Some(point)) == Some(point) {
            return;
        }

        let _ = self.connection.xcb().configure_window(
            self.window_id,
            &ConfigureWindowAux::new()
                .x(point.x as i32)
                .y(point.y as i32),
        );
        self.connection.flush();
    }

    fn set_visible(&mut self, visible: bool) {
        if self.is_closed || replace(&mut self.last_window_visible, visible) == visible {
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
