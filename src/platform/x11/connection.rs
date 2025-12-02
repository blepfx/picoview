use crate::Error;
use libc::c_ulong;
use raw_window_handle::XlibDisplayHandle;
use std::{
    cell::RefCell,
    collections::HashMap,
    ffi::CStr,
    marker::PhantomData,
    mem::zeroed,
    os::raw::c_int,
    ptr::{NonNull, null, null_mut},
    str::FromStr,
    sync::{LazyLock, Mutex},
    time::Duration,
};
use x11::{
    xcursor::XcursorLibraryLoadCursor,
    xlib::{
        Display, XCloseDisplay, XColor, XConnectionNumber, XCreateBitmapFromData,
        XCreatePixmapCursor, XDefaultScreen, XErrorEvent, XEvent, XFreeCursor, XFreePixmap,
        XGetErrorText, XInternAtom, XNextEvent, XOpenDisplay, XPending, XResourceManagerString,
        XRootWindow, XSetErrorHandler, XrmDestroyDatabase, XrmGetResource, XrmGetStringDatabase,
        XrmValue,
    },
    xrandr::{
        XRRFreeCrtcInfo, XRRFreeScreenResources, XRRGetCrtcInfo, XRRGetScreenResourcesCurrent,
        XRRQueryExtension,
    },
};

pub const ATOM_PICOVIEW_WAKEUP: &CStr = c"PICOVIEW_WAKEUP";

unsafe impl Send for Connection {}
pub struct Connection {
    display: *mut Display,
    screen: c_int,

    cursor_empty: RefCell<Option<c_ulong>>,
    cursor_cache: RefCell<HashMap<usize, c_ulong>>,
    atom_cache: RefCell<HashMap<usize, c_ulong>>,

    unsync: PhantomData<*mut ()>,
}

impl Connection {
    pub fn create() -> Result<Self, Error> {
        unsafe {
            let display = XOpenDisplay(std::ptr::null());
            if display.is_null() {
                return Err(Error::PlatformError("Failed to open X11 display".into()));
            }

            XSetErrorHandler(Some(error_handler));
            XInternAtom(display, ATOM_PICOVIEW_WAKEUP.as_ptr() as _, 1);

            let screen = XDefaultScreen(display);
            let connection = Self {
                display,
                screen,

                cursor_empty: RefCell::new(None),
                cursor_cache: RefCell::new(HashMap::new()),
                atom_cache: RefCell::new(HashMap::new()),

                unsync: PhantomData,
            };

            ERRORS_FOR_EACH_DISPLAY
                .lock()
                .expect("poisoned")
                .insert(display as usize, None);

            Ok(connection)
        }
    }

    pub fn check_error(&self) -> Result<(), String> {
        let err = ERRORS_FOR_EACH_DISPLAY
            .lock()
            .expect("poisoned")
            .get_mut(&(self.display as usize))
            .and_then(|x| x.take());

        match err {
            Some(err) => Err(err),
            None => Ok(()),
        }
    }

    pub fn display(&self) -> *mut Display {
        self.display
    }

    pub fn screen(&self) -> c_int {
        self.screen
    }

    pub fn default_root(&self) -> u64 {
        unsafe { XRootWindow(self.display, self.screen) }
    }

    pub fn scale_dpi(&self) -> Option<f32> {
        unsafe {
            let rms = XResourceManagerString(self.display);
            if rms.is_null() {
                return None;
            }

            let db = XrmGetStringDatabase(rms);
            if db.is_null() {
                return None;
            }

            let mut type_ = null_mut();
            let mut value = XrmValue { ..zeroed() };
            let result = XrmGetResource(
                db,
                c"Xft.dpi".as_ptr(),
                c"Xft.Dpi".as_ptr(),
                &mut type_,
                &mut value,
            );

            if result == 0
                || type_.is_null()
                || CStr::from_ptr(type_) != c"String"
                || value.addr.is_null()
            {
                XrmDestroyDatabase(db);
                return None;
            }

            let string = CStr::from_ptr(value.addr).to_string_lossy();
            let Ok(value) = f32::from_str(&string) else {
                XrmDestroyDatabase(db);
                return None;
            };

            XrmDestroyDatabase(db);
            Some(value)
        }
    }

    pub fn refresh_rate(&self) -> Option<f64> {
        unsafe {
            let has_randr = XRRQueryExtension(self.display, &mut 0, &mut 0);
            if has_randr == 0 {
                return None;
            }

            let resources = XRRGetScreenResourcesCurrent(self.display, self.default_root());
            if resources.is_null() {
                return None;
            }

            let mut max_rate: Option<f64> = None;
            for crtc in 0..(*resources).ncrtc {
                let crtc = (*resources).crtcs.add(crtc as usize).read();
                let crtc_info = XRRGetCrtcInfo(self.display, resources, crtc);

                if !crtc_info.is_null() && (*crtc_info).mode != 0 {
                    for mode in 0..(*resources).nmode {
                        let mode = (*resources).modes.add(mode as usize);

                        if (*mode).id == (*crtc_info).mode {
                            let rate = (*mode).dotClock as f64
                                / ((*mode).hTotal as f64 * (*mode).vTotal as f64);
                            max_rate = max_rate.map(|prev| prev.max(rate)).or(Some(rate));
                        }
                    }
                }

                XRRFreeCrtcInfo(crtc_info);
            }

            XRRFreeScreenResources(resources);

            max_rate
        }
    }

    pub fn display_handle(&self) -> XlibDisplayHandle {
        XlibDisplayHandle::new(NonNull::new(self.display as *mut _), self.screen)
    }

    pub fn cursor(&self, cursor: Option<&'static CStr>) -> c_ulong {
        match cursor {
            None => *self
                .cursor_empty
                .borrow_mut()
                .get_or_insert_with(|| unsafe {
                    const EMPTY: &[u8] = &[0];

                    let black = XColor { ..zeroed() };
                    let pixmap = XCreateBitmapFromData(
                        self.display,
                        self.default_root(),
                        EMPTY.as_ptr() as _,
                        1,
                        1,
                    );
                    let cursor = XCreatePixmapCursor(
                        self.display,
                        pixmap,
                        pixmap,
                        &black as *const _ as *mut _,
                        &black as *const _ as *mut _,
                        0,
                        0,
                    );

                    XFreePixmap(self.display, pixmap);
                    cursor
                }),

            Some(cursor) => *self
                .cursor_cache
                .borrow_mut()
                .entry(cursor.as_ptr() as usize)
                .or_insert_with(|| unsafe {
                    XcursorLibraryLoadCursor(self.display, cursor.as_ptr())
                }),
        }
    }

    pub fn atom(&self, atom: &'static CStr) -> c_ulong {
        *self
            .atom_cache
            .borrow_mut()
            .entry(atom.as_ptr() as usize)
            .or_insert_with(|| unsafe { XInternAtom(self.display, atom.as_ptr(), 0) })
    }

    pub fn next_event(&self) -> Result<XEvent, Error> {
        unsafe {
            let mut event = XEvent { type_: 0 };
            if XNextEvent(self.display, &mut event) != 0 {
                return Err(Error::PlatformError(
                    self.check_error()
                        .err()
                        .unwrap_or_else(|| "unknown error".to_owned()),
                ));
            }

            Ok(event)
        }
    }

    pub fn wait_for_events(&self, timeout: Duration) -> Result<u32, Error> {
        unsafe {
            if !timeout.is_zero() {
                let result = libc::ppoll(
                    &mut libc::pollfd {
                        fd: XConnectionNumber(self.display),
                        events: libc::POLLIN,
                        revents: 0,
                    },
                    1 as _,
                    &libc::timespec {
                        tv_sec: timeout.as_secs() as _,
                        tv_nsec: timeout.subsec_nanos() as _,
                    },
                    null(),
                );

                if result == -1 {
                    return Err(Error::PlatformError(
                        std::io::Error::last_os_error().to_string(),
                    ));
                }
            }

            Ok(XPending(self.display) as u32)
        }
    }
}

impl Drop for Connection {
    fn drop(&mut self) {
        unsafe {
            ERRORS_FOR_EACH_DISPLAY
                .lock()
                .expect("poisoned")
                .remove(&(self.display as usize));

            if let Some(cursor) = self.cursor_empty.take() {
                XFreeCursor(self.display, cursor);
            }

            XCloseDisplay(self.display);
        }
    }
}

static ERRORS_FOR_EACH_DISPLAY: LazyLock<Mutex<HashMap<usize, Option<String>>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));
unsafe extern "C" fn error_handler(dpy: *mut Display, err: *mut XErrorEvent) -> i32 {
    let mut map = ERRORS_FOR_EACH_DISPLAY.lock().expect("poisoned");
    let Some(conn) = map.get_mut(&(dpy as usize)) else {
        return 0;
    };

    if conn.is_some() {
        return 0;
    }

    unsafe {
        let mut buf = [0; 255];
        XGetErrorText(
            (*err).display,
            (*err).error_code.into(),
            buf.as_mut_ptr().cast(),
            (buf.len() - 1) as i32,
        );
        buf[buf.len() - 1] = 0;
        conn.replace(
            CStr::from_ptr(buf.as_mut_ptr().cast())
                .to_string_lossy()
                .into(),
        );
    }

    0
}
