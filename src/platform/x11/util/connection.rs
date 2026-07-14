use raw_window_handle::XlibDisplayHandle;
use std::cell::RefCell;
use std::collections::HashMap;
use std::ffi::{CStr, c_char, c_ulong};
use std::ptr::{NonNull, null_mut};
use std::rc::Rc;
use std::sync::Mutex;
use std::time::Duration;
use x11::xlib::*;

/// Wait for events with an optional timeout and return the number of
/// pending events after the wait.
pub fn wait_for_events(conn: &Connection, timeout: Option<Duration>) -> Result<u32, String> {
    unsafe {
        let timespec = timeout.map(|timeout| libc::timespec {
            tv_sec: timeout.as_secs().try_into().unwrap_or(i64::MAX),
            tv_nsec: timeout.subsec_nanos().into(),
        });

        let result = libc::ppoll(
            &mut libc::pollfd {
                fd: XConnectionNumber(conn.as_raw()) as _,
                events: libc::POLLIN,
                revents: 0,
            },
            1 as _,
            timespec
                .as_ref()
                .map(|x| x as *const _)
                .unwrap_or(null_mut()),
            null_mut(),
        );

        if result == -1 {
            return Err(std::io::Error::last_os_error().to_string());
        }

        Ok(XPending(conn.as_raw()) as u32)
    }
}

/// A cloneable handle to an X11 display connection. The connection is
/// automatically closed when all handles are dropped.
#[derive(Clone)]
pub struct Connection(Rc<ConnectionInner>);

impl Connection {
    /// Open a new connection to the X server. Returns `None` if the
    /// connection could not be established.
    pub fn open() -> Option<Self> {
        unsafe {
            let display = XOpenDisplay(std::ptr::null());
            if display.is_null() {
                return None;
            }

            GlobalState::with(|global| {
                if !global.closed {
                    global.errors.insert(display.addr(), None);
                }
            });

            Some(Self(Rc::new(ConnectionInner {
                display,
                atoms: RefCell::new(HashMap::new()),
            })))
        }
    }

    /// Get the last error that occurred on this display connection.
    ///
    /// Does not synchronize, so the errors could be delayed arbitrarily.
    pub fn async_last_error(&self) -> Result<(), String> {
        let error = GlobalState::with(|global| {
            global
                .errors
                .get_mut(&(self.0.display.addr()))
                .and_then(|x| x.take())
        });

        match error {
            Some(error) => Err(error),
            None => Ok(()),
        }
    }

    /// Get the last error that occurred on this display connection.
    ///
    /// This synchronizes with the X server, so it may block for a while.
    pub fn last_error(&self) -> Result<(), String> {
        unsafe {
            XSync(self.as_raw(), 0);
        }

        self.async_last_error()
    }

    /// Get a raw-window-handle display handle to this connection.
    pub fn display_handle(&self) -> XlibDisplayHandle {
        unsafe {
            XlibDisplayHandle::new(
                NonNull::new(self.as_raw() as *mut _),
                XDefaultScreen(self.as_raw()) as _,
            )
        }
    }

    /// Get the raw `Display` pointer for this connection for `xlib` calls
    pub fn as_raw(&self) -> *mut Display {
        self.0.display
    }

    /// Get the atom for the given name, caching it for future calls
    pub fn atom(&self, name: &'static CStr) -> c_ulong {
        *self
            .0
            .atoms
            .borrow_mut()
            .entry(name.as_ptr().addr())
            .or_insert_with(|| unsafe { XInternAtom(self.as_raw(), name.as_ptr(), 0) })
    }
}

/// Internal data for a single connection. Drop is called when all
/// [`Connection`] handles go out of scope.
struct ConnectionInner {
    display: *mut Display,
    atoms: RefCell<HashMap<usize, c_ulong>>,
}

impl Drop for ConnectionInner {
    fn drop(&mut self) {
        GlobalState::with(|global| {
            if global.closed {
                // if the global state is closed, we don't want to call XCloseDisplay because it
                // will cause a use-after-free
                return;
            }

            global.errors.remove(&self.display.addr());
            unsafe {
                XCloseDisplay(self.display);
            }
        });
    }
}

/// Global Xlib state, used for global error handling (because error handler
/// is global for some reason) and a use-after-free workaround (see
/// [`Self::closed`] field)
struct GlobalState {
    errors: HashMap<usize, Option<String>>,

    // NOTE: this is a stupid workaround for an Xlib bug (?) where
    // libX11 calls XFreeThreads on dtor
    // which happens _before_ non-main threads are exited, causing
    // a use-after-free
    closed: bool,
}

impl GlobalState {
    fn with<R>(f: impl FnOnce(&mut Self) -> R) -> R {
        static GLOBAL: Mutex<Option<GlobalState>> = Mutex::new(None);
        f(GLOBAL.lock().expect("poisoned").get_or_insert_with(|| {
            unsafe {
                XSetErrorHandler(Some(error_handler));
                libc::atexit(exit_handler);
            }

            Self {
                errors: HashMap::new(),
                closed: false,
            }
        }))
    }
}

extern "C" fn exit_handler() {
    GlobalState::with(|global| {
        // we dont want to keep any memory allocated after this point, especially
        // because when used as a plugin (as a dylib), the static memory will NOT be
        // unloaded automatically
        global.errors = HashMap::new();
        global.closed = true;
    });
}

unsafe extern "C" fn error_handler(dpy: *mut Display, err: *mut XErrorEvent) -> i32 {
    GlobalState::with(|global| {
        let Some(conn) = global.errors.get_mut(&(dpy as usize)) else {
            return 0;
        };

        if conn.is_some() {
            return 0;
        }

        unsafe {
            let mut buf = [0 as c_char; 255];

            XGetErrorText(
                (*err).display,
                (*err).error_code.into(),
                buf.as_mut_ptr(),
                254, // leave space for null terminator
            );

            buf[254] = 0; // force null terminator just in case
            conn.replace(CStr::from_ptr(buf.as_mut_ptr()).to_string_lossy().into());
        }

        0
    })
}
