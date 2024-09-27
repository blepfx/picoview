use super::keyboard::{hwcode2key, hwcode2mods, keymask2mods};
use super::{cursor::CursorCache, Window};
use crate::{
    platform::PlatformCommand, Error, Event, EventResponse, MouseButton, MouseCursor, Options,
    Point,
};
use crate::{Decoration, Modifiers};
use nix::poll::{poll, PollFd, PollFlags};
use raw_window_handle::{RawWindowHandle, XlibDisplayHandle, XlibWindowHandle};
use std::ptr::NonNull;
use std::sync::mpsc::{channel, Receiver};
use std::time::{Duration, Instant};
use x11rb::connection::RequestConnection;
use x11rb::protocol::present::{self, ConnectionExt as ConnectionExt3};
use x11rb::{
    connection::Connection,
    cursor::Handle,
    errors::ConnectionError,
    protocol::{
        xproto::{
            AtomEnum, ChangeWindowAttributesAux, ConfigureWindowAux, ConnectionExt,
            CreateWindowAux, EventMask, GrabMode, PropMode, WindowClass,
        },
        Event as XEvent,
    },
    resource_manager,
    wrapper::ConnectionExt as ConnectionExt2,
    xcb_ffi::XCBConnection,
    COPY_DEPTH_FROM_PARENT, COPY_FROM_PARENT,
};

x11rb::atom_manager! {
    pub Atoms: AtomsCookie {
        UTF8_STRING,
        _NET_WM_NAME,
        _NET_WM_WINDOW_TYPE,
        _NET_WM_WINDOW_TYPE_DOCK,
        WM_PROTOCOLS,
        WM_DELETE_WINDOW,
    }
}

unsafe impl Send for EventLoop {}
pub struct EventLoop {
    window: Window,

    handler: Box<dyn FnMut(&Window, Event) -> EventResponse + Send>,
    commands: Receiver<PlatformCommand>,

    connection: XCBConnection,
    atoms: Atoms,
    cursor_handle: Handle,
    cursor_cache: CursorCache,
    ext_present: bool,

    last_modifiers: Modifiers,
    last_cursor: MouseCursor,
    last_window_position: Option<Point>,
    event_loop_running: bool,
    event_loop_timer: Instant,
}

impl EventLoop {
    pub fn new(
        options: Options,
        handler: Box<dyn FnMut(&Window, Event) -> EventResponse + Send>,
    ) -> Result<Self, Error> {
        let display = unsafe { x11::xlib::XOpenDisplay(std::ptr::null()) };
        assert!(!display.is_null());

        let xcb_connection = unsafe { x11::xlib_xcb::XGetXCBConnection(display) };
        assert!(!xcb_connection.is_null());

        let screen = unsafe { x11::xlib::XDefaultScreen(display) } as usize;
        let connection =
            unsafe { XCBConnection::from_raw_xcb_connection(xcb_connection as _, true).unwrap() };

        let atoms = Atoms::new(&connection).unwrap().reply().unwrap();
        let resources = resource_manager::new_from_default(&connection).unwrap();
        let cursor_handle = Handle::new(&connection, screen, &resources)
            .unwrap()
            .reply()
            .unwrap();

        let ext_present = connection
            .extension_information(present::X11_EXTENSION_NAME)
            .unwrap()
            .is_some();

        unsafe {
            x11::xlib_xcb::XSetEventQueueOwner(
                display,
                x11::xlib_xcb::XEventQueueOwner::XCBOwnsEventQueue,
            )
        };

        let parent_window_id = match options.parent {
            Some(RawWindowHandle::Xlib(parent_window_handle)) => parent_window_handle.window as u32,
            Some(RawWindowHandle::Xcb(parent_window_handle)) => parent_window_handle.window.get(),
            None => connection.setup().roots[screen as usize].root,
            _ => {
                return Err(Error::PlatformError("Not an X11 window".into()));
            }
        };

        let window_id = connection.generate_id().unwrap();
        connection
            .create_window(
                COPY_DEPTH_FROM_PARENT,
                window_id,
                parent_window_id,
                0,
                0,
                1 as _,
                1 as _,
                0,
                WindowClass::INPUT_OUTPUT,
                COPY_FROM_PARENT,
                &CreateWindowAux::new().event_mask(
                    EventMask::BUTTON_PRESS
                        | EventMask::BUTTON_RELEASE
                        | EventMask::KEY_PRESS
                        | EventMask::KEY_RELEASE
                        | EventMask::LEAVE_WINDOW
                        | EventMask::POINTER_MOTION
                        | EventMask::FOCUS_CHANGE,
                ),
            )
            .unwrap();

        connection
            .change_property32(
                PropMode::REPLACE,
                window_id,
                atoms.WM_PROTOCOLS,
                AtomEnum::ATOM,
                &[atoms.WM_DELETE_WINDOW],
            )
            .unwrap();

        if options.decoration == Decoration::Dock {
            connection
                .change_property32(
                    PropMode::REPLACE,
                    window_id,
                    atoms._NET_WM_WINDOW_TYPE,
                    AtomEnum::ATOM,
                    &[atoms._NET_WM_WINDOW_TYPE_DOCK],
                )
                .unwrap();
        }

        if ext_present {
            let event_id = connection.generate_id().unwrap();
            connection
                .present_select_input(event_id, window_id, present::EventMask::COMPLETE_NOTIFY)
                .unwrap();
            connection
                .present_notify_msc(window_id, 0, 0, 1, 0)
                .unwrap();
        }

        connection.flush().unwrap();

        let window_handle = XlibWindowHandle::new(window_id as _);
        let display_handle =
            XlibDisplayHandle::new(Some(NonNull::new(display as _).unwrap()), screen as i32);
        let (sender, receiver) = channel();

        Ok(Self {
            commands: receiver,
            handler,
            window: Window {
                window: window_handle,
                display: display_handle,
                commands: sender,
            },

            connection,
            atoms,
            cursor_handle,
            cursor_cache: CursorCache::new(),
            ext_present,

            last_modifiers: Modifiers::empty(),
            last_cursor: MouseCursor::Default,
            last_window_position: None,
            event_loop_running: true,
            event_loop_timer: Instant::now(),
        })
    }

    pub fn window(&self) -> Window {
        self.window.clone()
    }

    pub fn run(&mut self) {
        while self.event_loop_running {
            // do frame if necessary
            let timeout = if self.ext_present {
                None
            } else {
                let frame_interval = Duration::from_millis(15);
                let next_frame = self.event_loop_timer + frame_interval;
                if Instant::now() >= next_frame {
                    self.handle_frame();
                    self.event_loop_timer =
                        Instant::max(next_frame, Instant::now() - frame_interval);
                }

                Some(next_frame.duration_since(Instant::now()))
            };

            // process the user commands
            match self.commands.try_recv() {
                Ok(cmd) => self.handle_command(cmd),
                Err(_) => {}
            }

            // wait for events and then process them
            match poll_timeout(&self.connection, timeout) {
                Ok(None) => {}
                Ok(Some(event)) => self.handle_event(event),
                Err(_) => {
                    self.event_loop_running = false;
                }
            }
        }
    }

    fn handle_frame(&mut self) {
        (self.handler)(&self.window, Event::Frame);
    }

    fn handle_command(&mut self, command: PlatformCommand) {
        match command {
            PlatformCommand::SetCursorIcon(cursor) => {
                if self.last_cursor != cursor {
                    self.last_cursor = cursor;

                    let xid = self.cursor_cache.get(
                        &self.connection,
                        self.window.display.screen as _,
                        &self.cursor_handle,
                        cursor,
                    );
                    if xid != 0 {
                        let _ = self.connection.change_window_attributes(
                            self.window.window.window as _,
                            &ChangeWindowAttributesAux::new().cursor(xid),
                        );
                        let _ = self.connection.flush();
                    }
                }
            }

            PlatformCommand::SetCursorPosition(point) => {
                let _ = self.connection.warp_pointer(
                    x11rb::NONE,
                    self.window.window.window as u32,
                    0,
                    0,
                    0,
                    0,
                    point.x.round() as i16,
                    point.y.round() as i16,
                );
                let _ = self.connection.flush();
            }

            PlatformCommand::SetSize(size) => {
                let _ = self.connection.configure_window(
                    self.window.window.window as _,
                    &ConfigureWindowAux::new()
                        .width(size.width as u32)
                        .height(size.height as u32),
                );
                let _ = self.connection.flush();
            }

            PlatformCommand::SetPosition(point) => {
                let _ = self.connection.configure_window(
                    self.window.window.window as _,
                    &ConfigureWindowAux::new()
                        .x(point.x as i32)
                        .y(point.y as i32),
                );
                let _ = self.connection.flush();
                self.last_window_position = Some(point);
            }

            PlatformCommand::SetTitle(title) => {
                let _ = self.connection.change_property8(
                    PropMode::REPLACE,
                    self.window.window.window as u32,
                    AtomEnum::WM_NAME,
                    AtomEnum::STRING,
                    title.as_bytes(),
                );

                let _ = self.connection.change_property8(
                    PropMode::REPLACE,
                    self.window.window.window as u32,
                    self.atoms._NET_WM_NAME,
                    self.atoms.UTF8_STRING,
                    title.as_bytes(),
                );

                let _ = self.connection.flush();
            }

            PlatformCommand::SetVisible(true) => {
                let _ = self.connection.map_window(self.window.window.window as _);
                if let Some(point) = self.last_window_position {
                    let _ = self.connection.configure_window(
                        self.window.window.window as _,
                        &ConfigureWindowAux::new()
                            .x(point.x as i32)
                            .y(point.y as i32),
                    );
                }

                let _ = self.connection.flush();
            }

            PlatformCommand::SetVisible(false) => {
                let _ = self.connection.unmap_window(self.window.window.window as _);
                let _ = self.connection.flush();
            }

            PlatformCommand::SetKeyboardInput(true) => {
                let _ = self.connection.grab_keyboard(
                    false,
                    self.window.window.window as _,
                    x11rb::CURRENT_TIME,
                    GrabMode::ASYNC,
                    GrabMode::ASYNC,
                );
            }

            PlatformCommand::SetKeyboardInput(false) => {
                let _ = self.connection.ungrab_keyboard(x11rb::CURRENT_TIME);
            }

            PlatformCommand::Close => {
                self.event_loop_running = false;
            }
        }
    }

    fn handle_event(&mut self, event: XEvent) {
        match event {
            XEvent::ClientMessage(event) => {
                if event.format == 32 && event.data.as_data32()[0] == self.atoms.WM_DELETE_WINDOW {
                    (self.handler)(&self.window, Event::WindowClose);
                }
            }

            XEvent::ButtonPress(e) => {
                self.handle_modifiers(keymask2mods(e.state));

                let position = Point {
                    x: e.event_x as f32,
                    y: e.event_y as f32,
                };

                let event = match e.detail {
                    1 => Event::MouseDown(MouseButton::Left),
                    2 => Event::MouseDown(MouseButton::Middle),
                    3 => Event::MouseDown(MouseButton::Right),
                    8 => Event::MouseDown(MouseButton::Back),
                    9 => Event::MouseDown(MouseButton::Forward),
                    4 => Event::MouseScroll { x: 0.0, y: 1.0 },
                    5 => Event::MouseScroll { x: 0.0, y: -1.0 },
                    6 => Event::MouseScroll { x: 1.0, y: 0.0 },
                    7 => Event::MouseScroll { x: -1.0, y: 0.0 },
                    _ => return,
                };

                (self.handler)(&self.window, Event::MouseMove(Some(position)));
                (self.handler)(&self.window, event);
            }

            XEvent::ButtonRelease(e) => {
                self.handle_modifiers(keymask2mods(e.state));

                let position = Point {
                    x: e.event_x as f32,
                    y: e.event_y as f32,
                };

                let event = match e.detail {
                    1 => Event::MouseUp(MouseButton::Left),
                    2 => Event::MouseUp(MouseButton::Middle),
                    3 => Event::MouseUp(MouseButton::Right),
                    8 => Event::MouseUp(MouseButton::Back),
                    9 => Event::MouseUp(MouseButton::Forward),
                    _ => return,
                };

                (self.handler)(&self.window, Event::MouseMove(Some(position)));
                (self.handler)(&self.window, event);
            }

            XEvent::KeyPress(e) => {
                self.handle_modifiers(keymask2mods(e.state) | hwcode2mods(e.detail));

                if let Some(key) = hwcode2key(e.detail) {
                    (self.handler)(&self.window, Event::KeyDown(key));
                }
            }

            XEvent::KeyRelease(e) => {
                self.handle_modifiers(keymask2mods(e.state) - hwcode2mods(e.detail));

                if let Some(key) = hwcode2key(e.detail) {
                    (self.handler)(&self.window, Event::KeyUp(key));
                }
            }

            XEvent::MotionNotify(e) => {
                self.handle_modifiers(keymask2mods(e.state));

                (self.handler)(
                    &self.window,
                    Event::MouseMove(Some(Point {
                        x: e.event_x as f32,
                        y: e.event_y as f32,
                    })),
                );
            }

            XEvent::LeaveNotify(_) => {
                (self.handler)(&self.window, Event::MouseMove(None));
            }

            XEvent::FocusIn(_) => {
                (self.handler)(&self.window, Event::WindowFocus);
            }

            XEvent::FocusOut(_) => {
                (self.handler)(&self.window, Event::WindowBlur);
            }

            XEvent::PresentCompleteNotify(_) => {
                self.handle_frame();

                let _ =
                    self.connection
                        .present_notify_msc(self.window.window.window as _, 0, 0, 1, 0);
                let _ = self.connection.flush();
            }

            _ => {}
        }
    }

    fn handle_modifiers(&mut self, mods: Modifiers) {
        if mods != self.last_modifiers {
            self.last_modifiers = mods;
            (self.handler)(&self.window, Event::KeyModifiers(mods));
        }
    }
}

fn poll_timeout(
    connection: &XCBConnection,
    timeout: Option<Duration>,
) -> Result<Option<XEvent>, ConnectionError> {
    if let Some(event) = connection.poll_for_event()? {
        return Ok(Some(event));
    }

    let mut fds = [PollFd::new(connection, PollFlags::POLLIN)];
    let timeout = timeout.map(|x| x.subsec_millis() as i32).unwrap_or(-1);

    match poll(&mut fds, timeout) {
        Ok(_) => {
            if let Some(rev) = fds[0].revents() {
                if rev.contains(PollFlags::POLLIN) {
                    return connection.poll_for_event();
                }
            }
        }
        Err(_) => {}
    }

    Ok(None)
}
