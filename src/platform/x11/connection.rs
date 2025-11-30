use crate::{Error, MouseCursor};
use std::{
    cell::RefCell,
    collections::HashMap,
    ffi::{CStr, c_int, c_void},
    os::fd::AsRawFd,
    ptr::null,
    sync::{Arc, Mutex},
    time::Duration,
};
use x11_dl::xlib::{Display, XErrorEvent, Xlib};
use x11rb::{
    connection::Connection as XConnection,
    cursor::Handle,
    errors::{ConnectionError, ReplyError},
    protocol::{
        Event,
        randr::ConnectionExt,
        xproto::{ConnectionExt as RandrConnectionExt, Screen},
    },
    resource_manager,
    xcb_ffi::XCBConnection,
};

x11rb::atom_manager! {
    pub Atoms: AtomsCookie {
        _MOTIF_WM_HINTS,
        _NET_WM_NAME,
        _NET_WM_WINDOW_TYPE,
        _NET_WM_WINDOW_TYPE_NORMAL,
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

        PICOVIEW_WAKEUP,
    }
}

thread_local! {
    static CURRENT_X11_ERROR: RefCell<Option<XErrorEvent>> = const { RefCell::new(None) };
}

unsafe impl Send for Connection {}
unsafe impl Sync for Connection {}

pub struct Connection {
    connection: XCBConnection,
    display: *mut Display,
    screen: c_int,

    atoms: Atoms,
    cursor_handle: Handle,
    cursor_cache: Mutex<CursorCache>,
    resource_manager: resource_manager::Database,

    xlib: Box<Xlib>,
}

impl Connection {
    pub fn create() -> Result<Arc<Self>, Error> {
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
                .map_err(|e| Error::PlatformError(e.to_string()))?;
            let atoms = Atoms::new(&connection)
                .map_err(|e| Error::PlatformError(e.to_string()))?
                .reply()
                .map_err(|e| Error::PlatformError(e.to_string()))?;
            let resource_manager = resource_manager::new_from_default(&connection)
                .map_err(|e| Error::PlatformError(e.to_string()))?;
            let cursor_handle = Handle::new(&connection, screen as usize, &resource_manager)
                .map_err(|e| Error::PlatformError(e.to_string()))?
                .reply()
                .map_err(|e| Error::PlatformError(e.to_string()))?;

            let connection = Arc::new(Self {
                xlib: Box::new(xlib),

                connection,
                display,
                screen,

                atoms,
                cursor_handle,
                cursor_cache: Mutex::new(CursorCache::new()),
                resource_manager,
            });

            Ok(connection)
        }
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

    pub fn xlib(&self) -> &Xlib {
        &self.xlib
    }

    pub fn atoms(&self) -> &Atoms {
        &self.atoms
    }

    pub fn flush(&self) -> Result<(), ConnectionError> {
        self.connection.flush()
    }

    pub fn default_screen_index(&self) -> c_int {
        self.screen
    }

    pub fn default_root(&self) -> &Screen {
        &self.xcb().setup().roots[self.default_screen_index() as usize]
    }

    pub fn os_scale_dpi(&self) -> u32 {
        self.resource_manager
            .get_value::<u32>("Xft.dpi", "")
            .ok()
            .flatten()
            .unwrap_or(96)
    }

    pub fn refresh_rate(&self) -> Result<Option<f64>, ReplyError> {
        let screen_resources = self
            .connection
            .randr_get_screen_resources_current(self.default_root().root)?
            .reply()?;

        let mut max_rate: Option<f64> = None;
        for crtc in screen_resources.crtcs.iter() {
            let crtc_info = self
                .connection
                .randr_get_crtc_info(*crtc, screen_resources.config_timestamp)?
                .reply()?;

            if crtc_info.mode != 0 {
                for mode in screen_resources.modes.iter() {
                    if mode.id == crtc_info.mode {
                        let rate =
                            mode.dot_clock as f64 / (mode.htotal as f64 * mode.vtotal as f64);
                        max_rate = max_rate.map(|prev| prev.max(rate)).or(Some(rate));
                    }
                }
            }
        }

        Ok(max_rate)
    }

    pub fn raw_connection(&self) -> *mut c_void {
        self.connection.get_raw_xcb_connection()
    }

    pub fn raw_display(&self) -> *mut Display {
        self.display
    }

    pub fn load_cursor(&self, cursor: MouseCursor) -> u32 {
        self.cursor_cache.lock().expect("poisoned").get_cached(
            &self.connection,
            self.screen as usize,
            &self.cursor_handle,
            cursor,
        )
    }

    pub fn poll_event_timeout(
        &self,
        timeout: Option<Duration>,
    ) -> Result<Option<Event>, ConnectionError> {
        let timeout = match timeout {
            Some(timeout) => timeout,
            None => return self.connection.wait_for_event().map(Some),
        };

        if let Some(event) = self.connection.poll_for_event()? {
            return Ok(Some(event));
        }

        let result = unsafe {
            libc::ppoll(
                &mut libc::pollfd {
                    fd: self.connection.as_raw_fd(),
                    events: libc::POLLIN,
                    revents: 0,
                },
                1 as _,
                &libc::timespec {
                    tv_sec: timeout.as_secs() as _,
                    tv_nsec: timeout.subsec_nanos() as _,
                },
                null(),
            )
        };

        if result == -1 {
            return Err(ConnectionError::IoError(std::io::Error::last_os_error()));
        }

        if result == 0 {
            Ok(None)
        } else {
            self.connection.poll_for_event()
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

struct CursorCache {
    map: HashMap<MouseCursor, u32>,
}

impl CursorCache {
    fn new() -> Self {
        Self {
            map: HashMap::default(),
        }
    }

    fn get_cached(
        &mut self,
        conn: &XCBConnection,
        screen: usize,
        handle: &Handle,
        cursor: MouseCursor,
    ) -> u32 {
        *self
            .map
            .entry(cursor)
            .or_insert_with(|| Self::get(conn, screen, handle, cursor).unwrap_or(x11rb::NONE))
    }

    fn get(
        conn: &XCBConnection,
        screen: usize,
        handle: &Handle,
        cursor: MouseCursor,
    ) -> Option<u32> {
        match cursor {
            MouseCursor::Default => Self::load(conn, handle, &["left_ptr"]),
            MouseCursor::Hand => Self::load(conn, handle, &["hand2", "hand1"]),
            MouseCursor::HandGrabbing => Self::load(conn, handle, &["closedhand", "grabbing"]),
            MouseCursor::Help => Self::load(conn, handle, &["question_arrow"]),
            MouseCursor::Hidden => Self::create_empty(conn, screen),
            MouseCursor::Text => Self::load(conn, handle, &["text", "xterm"]),
            MouseCursor::VerticalText => Self::load(conn, handle, &["vertical-text"]),
            MouseCursor::Working => Self::load(conn, handle, &["watch"]),
            MouseCursor::PtrWorking => Self::load(conn, handle, &["left_ptr_watch"]),
            MouseCursor::NotAllowed => Self::load(conn, handle, &["crossed_circle"]),
            MouseCursor::PtrNotAllowed => Self::load(conn, handle, &["no-drop", "crossed_circle"]),
            MouseCursor::ZoomIn => Self::load(conn, handle, &["zoom-in"]),
            MouseCursor::ZoomOut => Self::load(conn, handle, &["zoom-out"]),
            MouseCursor::Alias => Self::load(conn, handle, &["link"]),
            MouseCursor::Copy => Self::load(conn, handle, &["copy"]),
            MouseCursor::Move => Self::load(conn, handle, &["move"]),
            MouseCursor::AllScroll => Self::load(conn, handle, &["all-scroll"]),
            MouseCursor::Cell => Self::load(conn, handle, &["plus"]),
            MouseCursor::Crosshair => Self::load(conn, handle, &["crosshair"]),
            MouseCursor::EResize => Self::load(conn, handle, &["right_side"]),
            MouseCursor::NResize => Self::load(conn, handle, &["top_side"]),
            MouseCursor::NeResize => Self::load(conn, handle, &["top_right_corner"]),
            MouseCursor::NwResize => Self::load(conn, handle, &["top_left_corner"]),
            MouseCursor::SResize => Self::load(conn, handle, &["bottom_side"]),
            MouseCursor::SeResize => Self::load(conn, handle, &["bottom_right_corner"]),
            MouseCursor::SwResize => Self::load(conn, handle, &["bottom_left_corner"]),
            MouseCursor::WResize => Self::load(conn, handle, &["left_side"]),
            MouseCursor::EwResize => Self::load(conn, handle, &["h_double_arrow"]),
            MouseCursor::NsResize => Self::load(conn, handle, &["v_double_arrow"]),
            MouseCursor::NwseResize => Self::load(conn, handle, &["bd_double_arrow", "size_bdiag"]),
            MouseCursor::NeswResize => Self::load(conn, handle, &["fd_double_arrow", "size_fdiag"]),
            MouseCursor::ColResize => Self::load(conn, handle, &["split_h", "h_double_arrow"]),
            MouseCursor::RowResize => Self::load(conn, handle, &["split_v", "v_double_arrow"]),
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

    fn load(conn: &XCBConnection, cursor_handle: &Handle, names: &[&str]) -> Option<u32> {
        for name in names {
            match cursor_handle.load_cursor(conn, name) {
                Ok(cursor) if cursor != x11rb::NONE => return Some(cursor),
                _ => continue,
            }
        }

        None
    }
}
