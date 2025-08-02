use crate::{Error, MouseCursor};
use std::{
    cell::RefCell,
    collections::{HashMap, hash_map::Entry},
    ffi::{CStr, c_int, c_void},
    os::fd::{AsFd, AsRawFd},
    sync::{
        Arc, Mutex, Weak,
        mpsc::{Receiver, Sender, channel},
    },
    thread,
    time::{Duration, Instant},
};
use x11_dl::xlib::{Display, XErrorEvent, Xlib};
use x11rb::{
    connection::{Connection as XConnection, RequestConnection},
    cursor::Handle,
    errors::ConnectionError,
    protocol::{
        Event, present,
        xproto::{ConnectionExt, Screen, Window},
    },
    resource_manager,
    xcb_ffi::XCBConnection,
};

x11rb::atom_manager! {
    pub Atoms: AtomsCookie {
        _NET_WM_NAME,
        _NET_WM_WINDOW_TYPE,
        _NET_WM_WINDOW_TYPE_DOCK,
        WM_PROTOCOLS,
        WM_DELETE_WINDOW,

        UTF8_STRING,
        UTF8_MIME_0: b"text/plain;charset=utf-8",
        UTF8_MIME_1: b"text/plain;charset=UTF-8",
        // Text in ISO Latin-1 encoding
        // See: https://tronche.com/gui/x/icccm/sec-2.html#s-2.6.2
        STRING,
        // Text in unknown encoding
        // See: https://tronche.com/gui/x/icccm/sec-2.html#s-2.6.2
        TEXT,
        TEXT_MIME_UNKNOWN: b"text/plain",
    }
}

thread_local! {
    static CURRENT_X11_ERROR: RefCell<Option<XErrorEvent>> = const { RefCell::new(None) };
}

unsafe impl Send for Connection {}
unsafe impl Sync for Connection {}

pub type WindowHandler = Box<dyn FnMut(Option<&Event>) + Send + 'static>;
pub struct Connection {
    connection: XCBConnection,
    display: *mut Display,
    screen: c_int,

    loop_manual: bool,
    loop_sender: Sender<(Window, Option<WindowHandler>)>,

    atoms: Atoms,
    cursor_handle: Handle,
    cursor_cache: Mutex<CursorCache>,

    xlib: Box<Xlib>,
}

impl Connection {
    pub fn get() -> Result<Arc<Self>, Error> {
        static INSTANCE: Mutex<Weak<Connection>> = Mutex::new(Weak::new());

        let mut lock = INSTANCE.lock().unwrap();
        if let Some(conn) = lock.upgrade() {
            return Ok(conn);
        }

        let conn = Self::create()?;
        *lock = Arc::downgrade(&conn);
        Ok(conn)
    }

    pub fn last_error(&self) -> Option<String> {
        let error = CURRENT_X11_ERROR.with(|error| error.borrow_mut().take())?;

        unsafe {
            let mut buf = [0; 255];
            (self.xlib.XGetErrorText)(
                error.display,
                error.error_code.into(),
                buf.as_mut_ptr().cast(),
                (buf.len() - 1) as i32,
            );
            buf[buf.len() - 1] = 0;
            Some(
                CStr::from_ptr(buf.as_mut_ptr().cast())
                    .to_string_lossy()
                    .into(),
            )
        }
    }

    pub fn xcb(&self) -> &XCBConnection {
        &self.connection
    }

    pub fn atoms(&self) -> &Atoms {
        &self.atoms
    }

    pub fn flush(&self) -> bool {
        self.connection.flush().is_ok()
    }

    pub fn default_screen_index(&self) -> c_int {
        self.screen
    }

    pub fn default_root(&self) -> &Screen {
        &self.xcb().setup().roots[self.default_screen_index() as usize]
    }

    pub fn raw_connection(&self) -> *mut c_void {
        self.connection.get_raw_xcb_connection()
    }

    pub fn raw_display(&self) -> *mut Display {
        self.display
    }

    pub fn load_cursor(&self, cursor: MouseCursor) -> u32 {
        self.cursor_cache.lock().unwrap().get(
            &self.connection,
            self.screen as usize,
            &self.cursor_handle,
            cursor,
        )
    }

    pub fn is_manual_tick(&self) -> bool {
        self.loop_manual
    }

    pub fn add_window_pacer(&self, window: Window, handler: WindowHandler) {
        let _ = self.loop_sender.send((window, Some(handler)));
    }

    pub fn remove_window_pacer(&self, window: Window) {
        let _ = self.loop_sender.send((window, None));
    }

    fn create() -> Result<Arc<Self>, Error> {
        unsafe extern "C" fn error_handler(_dpy: *mut Display, err: *mut XErrorEvent) -> i32 {
            CURRENT_X11_ERROR.with(|error| {
                let mut error = error.borrow_mut();
                match error.as_mut() {
                    Some(_) => 1,
                    None => {
                        unsafe {
                            *error = Some(err.read());
                        }
                        0
                    }
                }
            })
        }
        unsafe {
            let xlib_xcb = x11_dl::xlib_xcb::Xlib_xcb::open().map_err(|_| {
                Error::PlatformError(
                    "No libx11-xcb found, you might need to install a dependency".into(),
                )
            })?;
            let xlib = x11_dl::xlib::Xlib::open().map_err(|_| {
                Error::PlatformError(
                    "No libx11 found, you might need to install a dependency".into(),
                )
            })?;

            let display = (xlib.XOpenDisplay)(std::ptr::null());
            if display.is_null() {
                return Err(Error::PlatformError("Failed to open X11 display".into()));
            }

            let xcb_connection = (xlib_xcb.XGetXCBConnection)(display);
            if xcb_connection.is_null() {
                return Err(Error::PlatformError("Failed to open XCB connection".into()));
            }

            (xlib.XSetErrorHandler)(Some(error_handler));
            (xlib_xcb.XSetEventQueueOwner)(
                display,
                x11_dl::xlib_xcb::XEventQueueOwner::XCBOwnsEventQueue,
            );
            let screen = (xlib.XDefaultScreen)(display);

            let connection = XCBConnection::from_raw_xcb_connection(xcb_connection as _, false)
                .map_err(|_| Error::PlatformError("X11 connection error".into()))?;
            let atoms = Atoms::new(&connection)
                .map_err(|_| Error::PlatformError("X11 connection error".into()))?
                .reply()
                .map_err(|_| Error::PlatformError("X11 connection error".into()))?;
            let resources = resource_manager::new_from_default(&connection)
                .map_err(|_| Error::PlatformError("X11 connection error".into()))?;
            let cursor_handle = Handle::new(&connection, screen as usize, &resources)
                .map_err(|_| Error::PlatformError("X11 connection error".into()))?
                .reply()
                .map_err(|_| Error::PlatformError("X11 connection error".into()))?;

            let ext_present = connection
                .extension_information(present::X11_EXTENSION_NAME)
                .ok()
                .flatten()
                .is_some();

            let (loop_sender, loop_receiver) = channel();
            let connection = Arc::new(Self {
                xlib: Box::new(xlib),

                connection,
                display,
                screen,

                loop_manual: !ext_present,
                loop_sender,

                atoms,
                cursor_handle,
                cursor_cache: Mutex::new(CursorCache::new()),
            });

            run_event_loop(Arc::downgrade(&connection), loop_receiver);

            Ok(connection)
        }
    }

    fn poll_event_single(
        &self,
        timeout: Option<Duration>,
    ) -> Result<Option<Event>, ConnectionError> {
        if let Some(event) = self.connection.poll_for_event()? {
            return Ok(Some(event));
        }

        let mut fd = libc::pollfd {
            fd: self.connection.as_fd().as_raw_fd(),
            events: libc::POLLIN,
            revents: 0,
        };

        let result = unsafe {
            libc::poll(
                &mut fd,
                1 as _,
                timeout.map(|x| x.subsec_millis() as i32).unwrap_or(-1) as _,
            )
        };

        if result == -1 {
            return Err(ConnectionError::IoError(std::io::Error::last_os_error()));
        }

        if fd.revents & libc::POLLIN != 0 {
            self.connection.poll_for_event()
        } else {
            Ok(None)
        }
    }
}

impl Drop for Connection {
    fn drop(&mut self) {
        unsafe {
            (self.xlib.XCloseDisplay)(self.display);
        }
    }
}

fn run_event_loop(
    connection: Weak<Connection>,
    handler_receiver: Receiver<(Window, Option<WindowHandler>)>,
) {
    const FRAME_INTERVAL: Duration = Duration::from_micros(16_666);
    const FRAME_PADDING: Duration = Duration::from_micros(200);

    let mut event_timer = Instant::now();
    let mut event_handlers = HashMap::new();

    thread::spawn(move || {
        while let Some(connection) = connection.upgrade() {
            let deadline = if connection.is_manual_tick() {
                if Instant::now() >= event_timer {
                    event_timer = Instant::max(
                        event_timer + FRAME_INTERVAL,
                        Instant::now() - FRAME_INTERVAL,
                    );
                }
                Some(event_timer)
            } else {
                None
            };

            let timeout =
                deadline.map(|t| t.saturating_duration_since(Instant::now()) + FRAME_PADDING);
            let event = match connection.poll_event_single(timeout) {
                Ok(event) => event,
                Err(e) => panic!("x11 event loop error: {:?}", e),
            };

            while let Some((window, handler)) = handler_receiver.try_recv().ok() {
                match handler {
                    Some(handler) => event_handlers.insert(window, handler),
                    None => event_handlers.remove(&window),
                };
            }

            match event.as_ref().and_then(event_window_id) {
                Some(window) => {
                    if let Some(handler) = event_handlers.get_mut(&window) {
                        handler(event.as_ref());
                    }
                }
                None => {
                    for (_, handler) in event_handlers.iter_mut() {
                        handler(event.as_ref());
                    }
                }
            };
        }
    });
}

struct CursorCache {
    map: HashMap<MouseCursor, u32>,
}

impl CursorCache {
    fn new() -> Self {
        Self {
            map: HashMap::default(),
        }
    }

    fn get(
        &mut self,
        conn: &XCBConnection,
        screen: usize,
        handle: &Handle,
        cursor: MouseCursor,
    ) -> u32 {
        match self.map.entry(cursor) {
            Entry::Occupied(entry) => *entry.get(),
            Entry::Vacant(entry) => {
                let cursor = Self::load(conn, screen, handle, cursor)
                    .or_else(|| Self::load(conn, screen, handle, MouseCursor::Default))
                    .unwrap_or(x11rb::NONE);

                entry.insert(cursor);
                cursor
            }
        }
    }

    fn load(
        conn: &XCBConnection,
        screen: usize,
        handle: &Handle,
        cursor: MouseCursor,
    ) -> Option<u32> {
        macro_rules! load {
            ($($l:literal),*) => {
                Self::load_named(conn, handle, &[$($l),*])
            };
        }

        match cursor {
            MouseCursor::Default => load!("left_ptr"),
            MouseCursor::Hand => load!("hand2", "hand1"),
            MouseCursor::HandGrabbing => load!("closedhand", "grabbing"),
            MouseCursor::Help => load!("question_arrow"),
            MouseCursor::Hidden => Self::create_empty(conn, screen),
            MouseCursor::Text => load!("text", "xterm"),
            MouseCursor::VerticalText => load!("vertical-text"),
            MouseCursor::Working => load!("watch"),
            MouseCursor::PtrWorking => load!("left_ptr_watch"),
            MouseCursor::NotAllowed => load!("crossed_circle"),
            MouseCursor::PtrNotAllowed => load!("no-drop", "crossed_circle"),
            MouseCursor::ZoomIn => load!("zoom-in"),
            MouseCursor::ZoomOut => load!("zoom-out"),
            MouseCursor::Alias => load!("link"),
            MouseCursor::Copy => load!("copy"),
            MouseCursor::Move => load!("move"),
            MouseCursor::AllScroll => load!("all-scroll"),
            MouseCursor::Cell => load!("plus"),
            MouseCursor::Crosshair => load!("crosshair"),
            MouseCursor::EResize => load!("right_side"),
            MouseCursor::NResize => load!("top_side"),
            MouseCursor::NeResize => load!("top_right_corner"),
            MouseCursor::NwResize => load!("top_left_corner"),
            MouseCursor::SResize => load!("bottom_side"),
            MouseCursor::SeResize => load!("bottom_right_corner"),
            MouseCursor::SwResize => load!("bottom_left_corner"),
            MouseCursor::WResize => load!("left_side"),
            MouseCursor::EwResize => load!("h_double_arrow"),
            MouseCursor::NsResize => load!("v_double_arrow"),
            MouseCursor::NwseResize => load!("bd_double_arrow", "size_bdiag"),
            MouseCursor::NeswResize => load!("fd_double_arrow", "size_fdiag"),
            MouseCursor::ColResize => load!("split_h", "h_double_arrow"),
            MouseCursor::RowResize => load!("split_v", "v_double_arrow"),
        }
    }

    fn create_empty(conn: &XCBConnection, screen: usize) -> Option<u32> {
        let cursor_id = conn.generate_id().ok()?;
        let pixmap_id = conn.generate_id().ok()?;
        let root_window = conn.setup().roots[screen].root;

        conn.create_pixmap(1, pixmap_id, root_window, 1, 1).ok()?;
        conn.create_cursor(cursor_id, pixmap_id, pixmap_id, 0, 0, 0, 0, 0, 0, 0, 0)
            .ok()?;
        conn.free_pixmap(pixmap_id).ok()?;

        Some(cursor_id)
    }

    fn load_named(conn: &XCBConnection, cursor_handle: &Handle, names: &[&str]) -> Option<u32> {
        for name in names {
            match cursor_handle.load_cursor(conn, name) {
                Ok(cursor) if cursor != x11rb::NONE => return Some(cursor),
                _ => continue,
            }
        }

        None
    }
}

fn event_window_id(event: &Event) -> Option<Window> {
    Some(match event {
        Event::ButtonPress(e) => e.event,
        Event::ButtonRelease(e) => e.event,
        Event::CirculateNotify(e) => e.window,
        Event::CirculateRequest(e) => e.window,
        Event::ClientMessage(e) => e.window,
        Event::ColormapNotify(e) => e.window,
        Event::ConfigureNotify(e) => e.window,
        Event::ConfigureRequest(e) => e.window,
        Event::CreateNotify(e) => e.window,
        Event::DestroyNotify(e) => e.window,
        Event::EnterNotify(e) => e.event,
        Event::Expose(e) => e.window,
        Event::FocusIn(e) => e.event,
        Event::FocusOut(e) => e.event,
        Event::GravityNotify(e) => e.window,
        Event::KeyPress(e) => e.event,
        Event::KeyRelease(e) => e.event,
        Event::LeaveNotify(e) => e.event,
        Event::MapNotify(e) => e.window,
        Event::MapRequest(e) => e.window,
        Event::MotionNotify(e) => e.event,
        Event::PropertyNotify(e) => e.window,
        Event::ReparentNotify(e) => e.window,
        Event::ResizeRequest(e) => e.window,
        Event::UnmapNotify(e) => e.window,
        Event::VisibilityNotify(e) => e.window,
        Event::PresentCompleteNotify(e) => e.window,
        Event::PresentConfigureNotify(e) => e.window,
        Event::PresentIdleNotify(e) => e.window,
        Event::PresentRedirectNotify(e) => e.window,
        Event::XfixesCursorNotify(e) => e.window,
        Event::XfixesSelectionNotify(e) => e.window,
        _ => return None,
    })
}
