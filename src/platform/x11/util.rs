use crate::{MouseCursor, Point};
use std::{
    ffi::c_ulong,
    os::unix::process::CommandExt,
    process::{Command, Stdio},
};
use x11::xlib::*;

pub fn open_url(path: &str) -> bool {
    fn spawn_detached(cmd: &mut Command) -> std::io::Result<()> {
        cmd.stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null());

        unsafe {
            cmd.pre_exec(move || {
                match libc::fork() {
                    -1 => return Err(std::io::Error::last_os_error()),
                    0 => (),
                    _ => libc::_exit(0),
                }

                if libc::setsid() == -1 {
                    return Err(std::io::Error::last_os_error());
                }

                Ok(())
            });
        }

        cmd.spawn().map(|_| ())
    }

    if spawn_detached(Command::new("xdg-open").arg(path)).is_ok() {
        return true;
    }

    if spawn_detached(Command::new("gio").args(["open", path])).is_ok() {
        return true;
    }

    if spawn_detached(Command::new("gnome-open").arg(path)).is_ok() {
        return true;
    }

    if spawn_detached(Command::new("kde-open").arg(path)).is_ok() {
        return true;
    }

    false
}

pub fn window_position(conn: &Connection, window_id: c_ulong) -> Option<Point> {
    let mut x = 0;
    let mut y = 0;

    let status = unsafe {
        XTranslateCoordinates(
            conn.display(),
            window_id,
            XDefaultRootWindow(conn.display()),
            0,
            0,
            &mut x,
            &mut y,
            &mut 0,
        )
    };

    if status != 0 {
        Some(Point {
            x: x as f32,
            y: y as f32,
        })
    } else {
        None
    }
}

pub use connection::*;
pub use cursor::*;
pub use events::*;
pub use info::*;
pub use keyboard::*;
pub use selection::*;
pub use visual::*;

mod connection {
    use raw_window_handle::XlibDisplayHandle;
    use std::{
        cell::RefCell,
        collections::HashMap,
        ffi::{CStr, c_char, c_ulong},
        ptr::NonNull,
        rc::Rc,
        sync::{LazyLock, Mutex},
    };
    use x11::xlib::*;

    #[derive(Clone)]
    pub struct Connection(Rc<ConnectionInner>);

    impl Connection {
        pub fn open() -> Option<Self> {
            unsafe {
                let display = XOpenDisplay(std::ptr::null());
                if display.is_null() {
                    return None;
                }

                XSetErrorHandler(Some(error_handler));

                ERRORS_FOR_EACH_DISPLAY
                    .lock()
                    .expect("poisoned")
                    .insert(display as usize, None);

                Some(Self(Rc::new(ConnectionInner { display })))
            }
        }

        pub fn last_error(&self) -> Result<(), String> {
            let err = ERRORS_FOR_EACH_DISPLAY
                .lock()
                .expect("poisoned")
                .get_mut(&(self.0.display as usize))
                .and_then(|x| x.take());

            match err {
                Some(err) => Err(err),
                None => Ok(()),
            }
        }

        pub fn display_handle(&self) -> XlibDisplayHandle {
            unsafe {
                XlibDisplayHandle::new(
                    NonNull::new(self.display() as *mut _),
                    XDefaultScreen(self.display()) as _,
                )
            }
        }

        pub fn display(&self) -> *mut Display {
            self.0.display
        }

        pub fn atom(&self, name: &'static CStr) -> c_ulong {
            ATOM_CACHE.with_borrow_mut(|cache| {
                *cache
                    .entry(name.as_ptr() as usize)
                    .or_insert_with(|| unsafe { XInternAtom(self.display(), name.as_ptr(), 0) })
            })
        }
    }

    struct ConnectionInner {
        display: *mut Display,
    }

    impl Drop for ConnectionInner {
        fn drop(&mut self) {
            unsafe {
                ERRORS_FOR_EACH_DISPLAY
                    .lock()
                    .expect("poisoned")
                    .remove(&(self.display as usize));

                XCloseDisplay(self.display);
            }
        }
    }

    static ERRORS_FOR_EACH_DISPLAY: LazyLock<Mutex<HashMap<usize, Option<String>>>> =
        LazyLock::new(|| Mutex::new(HashMap::new()));

    thread_local! {
        static ATOM_CACHE: RefCell<HashMap<usize, c_ulong>> = RefCell::new(HashMap::new());
    }

    unsafe extern "C" fn error_handler(dpy: *mut Display, err: *mut XErrorEvent) -> i32 {
        let mut map = ERRORS_FOR_EACH_DISPLAY.lock().expect("poisoned");
        let Some(conn) = map.get_mut(&(dpy as usize)) else {
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
                (buf.len() - 1) as i32,
            );
            buf[254] = 0;
            conn.replace(CStr::from_ptr(buf.as_mut_ptr()).to_string_lossy().into());
        }

        0
    }
}

mod selection {
    use super::Connection;
    use std::{
        array::from_fn,
        ffi::{OsStr, OsString, c_char, c_int, c_ulong},
        mem::zeroed,
        os::unix::ffi::OsStrExt,
        path::PathBuf,
        ptr::null_mut,
    };
    use x11::xlib::*;

    #[derive(Debug, PartialEq, Eq, Clone, Copy)]
    pub enum SelectionError {
        Empty,
        Recursive,
    }

    pub fn encode_uri_list(files: &[PathBuf]) -> OsString {
        let mut ret = OsString::new();
        for file in files {
            if !ret.is_empty() {
                ret.push("\r\n");
            }
            ret.push("file://");
            ret.push(file.as_os_str());
        }

        ret
    }

    pub fn decode_uri_list(list: &OsStr) -> Vec<PathBuf> {
        fn percent_decode(bytes: &[u8]) -> Vec<u8> {
            let mut iter = bytes.iter();
            let mut result = Vec::with_capacity(bytes.len());

            while let Some(&b) = iter.next() {
                if b == b'%' {
                    let [high, low] = from_fn(|_| {
                        iter.next()
                            .copied()
                            .map(char::from)
                            .and_then(|c| c.to_digit(16))
                    });

                    if let (Some(high), Some(low)) = (high, low) {
                        result.push((high * 16 + low) as u8);
                    }
                } else {
                    result.push(b);
                }
            }

            result
        }

        list.as_bytes()
            .split(|&b| b == b'\n')
            .filter(|line| !line.is_empty() && !line.starts_with(b"#"))
            .map(|line| {
                let line = line.strip_prefix(b"file://").unwrap_or(line);
                let line = line.strip_suffix(b"\r").unwrap_or(line);
                PathBuf::from(OsStr::from_bytes(&percent_decode(line)))
            })
            .collect()
    }

    pub fn request_selection<R>(
        conn: &Connection,
        window: c_ulong,
        selection: c_ulong,
        property: c_ulong,
        target: c_ulong,
        f: impl FnOnce(&[u8]) -> R,
    ) -> Result<R, SelectionError> {
        unsafe extern "C" fn event_filter(
            _: *mut Display,
            e: *mut XEvent,
            _: *mut c_char,
        ) -> c_int {
            unsafe { ((*e).type_ == SelectionNotify) as _ }
        }

        unsafe {
            let owner = XGetSelectionOwner(conn.display(), selection);
            if owner == 0 {
                return Err(SelectionError::Empty);
            } else if window == owner {
                return Err(SelectionError::Recursive);
            }

            let result = XConvertSelection(
                conn.display(),
                selection,
                target,
                property,
                window,
                CurrentTime,
            );

            if result == 0 {
                return Err(SelectionError::Empty);
            }

            XSync(conn.display(), 0);

            let event = {
                let mut event = zeroed();
                XIfEvent(conn.display(), &mut event, Some(event_filter), null_mut());
                event.selection
            };

            if event.property == 0 || event.selection != selection || event.target != target {
                return Err(SelectionError::Empty);
            }

            let mut target = 0;
            let mut format = 0;
            let mut size = 0;
            let mut nitems = 0;
            let mut data = null_mut();

            let result = XGetWindowProperty(
                conn.display(),
                event.requestor,
                event.property,
                0,
                !0,
                0,
                AnyPropertyType as _,
                &mut target,
                &mut format,
                &mut size,
                &mut nitems,
                &mut data,
            );

            if result != 0 || data.is_null() {
                return Err(SelectionError::Empty);
            }

            let result = f(std::slice::from_raw_parts(data as *const u8, size as usize));
            XFree(data as *mut _);
            Ok(result)
        }
    }
}

mod visual {
    use super::Connection;
    use std::{ffi::c_int, mem::zeroed, ptr::null_mut};
    use x11::{glx::GLXFBConfig, xlib::*};

    pub struct VisualConfig {
        pub fb_config: GLXFBConfig,
        pub depth: c_int,
        pub visual: *mut Visual,
    }

    impl VisualConfig {
        pub fn copy_from_parent() -> Self {
            Self {
                depth: CopyFromParent,
                visual: null_mut(),
                fb_config: null_mut(),
            }
        }

        pub fn try_new_true_color(conn: &Connection, depth: u8) -> Option<Self> {
            let visual = unsafe {
                let mut visual = XVisualInfo { ..zeroed() };
                match XMatchVisualInfo(
                    conn.display(),
                    XDefaultScreen(conn.display()),
                    depth as _,
                    TrueColor,
                    &mut visual,
                ) {
                    0 => return None,
                    _ => visual,
                }
            };

            Some(Self {
                depth: depth as _,
                visual: visual.visual,
                fb_config: null_mut(),
            })
        }
    }
}

mod keyboard {
    use crate::{Key, Modifiers};
    use std::ffi::c_uint;
    use x11::xlib::*;

    pub fn keycode_to_key(code: c_uint) -> Option<Key> {
        Some(match code {
            0x09 => Key::Escape,
            0x0A => Key::D1,
            0x0B => Key::D2,
            0x0C => Key::D3,
            0x0D => Key::D4,
            0x0E => Key::D5,
            0x0F => Key::D6,
            0x10 => Key::D7,
            0x11 => Key::D8,
            0x12 => Key::D9,
            0x13 => Key::D0,
            0x14 => Key::Minus,
            0x15 => Key::Equal,
            0x16 => Key::Backspace,
            0x17 => Key::Tab,
            0x18 => Key::Q,
            0x19 => Key::W,
            0x1A => Key::E,
            0x1B => Key::R,
            0x1C => Key::T,
            0x1D => Key::Y,
            0x1E => Key::U,
            0x1F => Key::I,
            0x20 => Key::O,
            0x21 => Key::P,
            0x22 => Key::BracketLeft,
            0x23 => Key::BracketRight,
            0x24 => Key::Enter,
            0x25 => Key::ControlLeft,
            0x26 => Key::A,
            0x27 => Key::S,
            0x28 => Key::D,
            0x29 => Key::F,
            0x2A => Key::G,
            0x2B => Key::H,
            0x2C => Key::J,
            0x2D => Key::K,
            0x2E => Key::L,
            0x2F => Key::Semicolon,
            0x30 => Key::Quote,
            0x31 => Key::Backquote,
            0x32 => Key::ShiftLeft,
            0x33 => Key::Backslash,
            0x34 => Key::Z,
            0x35 => Key::X,
            0x36 => Key::C,
            0x37 => Key::V,
            0x38 => Key::B,
            0x39 => Key::N,
            0x3A => Key::M,
            0x3B => Key::Comma,
            0x3C => Key::Period,
            0x3D => Key::Slash,
            0x3E => Key::ShiftRight,
            0x3F => Key::NumpadMultiply,
            0x40 => Key::AltLeft,
            0x41 => Key::Space,
            0x42 => Key::CapsLock,
            0x43 => Key::F1,
            0x44 => Key::F2,
            0x45 => Key::F3,
            0x46 => Key::F4,
            0x47 => Key::F5,
            0x48 => Key::F6,
            0x49 => Key::F7,
            0x4A => Key::F8,
            0x4B => Key::F9,
            0x4C => Key::F10,
            0x4D => Key::NumLock,
            0x4E => Key::ScrollLock,
            0x4F => Key::Numpad7,
            0x50 => Key::Numpad8,
            0x51 => Key::Numpad9,
            0x52 => Key::NumpadSubtract,
            0x53 => Key::Numpad4,
            0x54 => Key::Numpad5,
            0x55 => Key::Numpad6,
            0x56 => Key::NumpadAdd,
            0x57 => Key::Numpad1,
            0x58 => Key::Numpad2,
            0x59 => Key::Numpad3,
            0x5A => Key::Numpad0,
            0x5B => Key::NumpadDecimal,
            0x5F => Key::F11,
            0x60 => Key::F12,
            0x68 => Key::NumpadEnter,
            0x69 => Key::ControlRight,
            0x6A => Key::NumpadDivide,
            0x6B => Key::PrintScreen,
            0x6C => Key::AltRight,
            0x6E => Key::Home,
            0x6F => Key::ArrowUp,
            0x70 => Key::PageUp,
            0x71 => Key::ArrowLeft,
            0x72 => Key::ArrowRight,
            0x73 => Key::End,
            0x74 => Key::ArrowDown,
            0x75 => Key::PageDown,
            0x76 => Key::Insert,
            0x77 => Key::Delete,
            0x7D => Key::NumpadEqual,
            0x81 => Key::NumpadComma,
            0x85 => Key::MetaLeft,
            0x86 => Key::MetaRight,
            0x87 => Key::ContextMenu,
            _ => return None,
        })
    }

    pub fn keycode_to_mods(code: c_uint) -> Modifiers {
        match code {
            0x25 | 0x69 => Modifiers::CTRL,
            0x32 | 0x3E => Modifiers::SHIFT,
            0x85 | 0x86 => Modifiers::META,
            0x40 | 0x6C => Modifiers::ALT,
            _ => Modifiers::empty(),
        }
    }

    pub fn keymask_to_mods(mods: c_uint) -> Modifiers {
        const MAP: &[(c_uint, Modifiers)] = &[
            (ShiftMask, Modifiers::SHIFT),
            (ControlMask, Modifiers::CTRL),
            (Mod1Mask, Modifiers::ALT),
            (Mod4Mask, Modifiers::META),
            (Mod2Mask, Modifiers::NUM_LOCK),
            (LockMask, Modifiers::CAPS_LOCK),
        ];

        let mut ret = Modifiers::empty();
        for (mask, modifiers) in MAP {
            if (mods & *mask) != 0 {
                ret |= *modifiers;
            }
        }
        ret
    }
}

mod info {
    use super::Connection;
    use std::{ffi::CStr, mem::zeroed, ptr::null_mut, str::FromStr};
    use x11::{xlib::*, xrandr::*};

    pub fn query_scale_dpi(conn: &Connection) -> Option<f32> {
        unsafe {
            let rms = XResourceManagerString(conn.display());
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

    pub fn query_refresh_rate(conn: &Connection) -> Option<f64> {
        unsafe {
            let has_randr = XRRQueryExtension(conn.display(), &mut 0, &mut 0);
            if has_randr == 0 {
                return None;
            }

            let resources =
                XRRGetScreenResourcesCurrent(conn.display(), XDefaultRootWindow(conn.display()));
            if resources.is_null() {
                return None;
            }

            let mut max_rate: Option<f64> = None;
            for crtc in 0..(*resources).ncrtc {
                let crtc = (*resources).crtcs.add(crtc as usize).read();
                let crtc_info = XRRGetCrtcInfo(conn.display(), resources, crtc);

                if !crtc_info.is_null() && (*crtc_info).mode != 0 {
                    for mode in 0..(*resources).nmode {
                        let mode = (*resources).modes.add(mode as usize);

                        if (*mode).id == (*crtc_info).mode {
                            let rate = (*mode).dotClock as f64
                                / ((*mode).hTotal as f64 * (*mode).vTotal as f64);

                            //xvfb reports it as NaN
                            if rate.is_finite() {
                                max_rate = max_rate.map(|prev| prev.max(rate)).or(Some(rate));
                            }
                        }
                    }
                }

                XRRFreeCrtcInfo(crtc_info);
            }

            XRRFreeScreenResources(resources);

            max_rate
        }
    }
}

mod cursor {
    use super::Connection;
    use std::{
        ffi::{CStr, c_ulong},
        mem::zeroed,
    };
    use x11::{xcursor::XcursorLibraryLoadCursor, xlib::*};

    pub struct EmptyCursor {
        conn: Connection,
        cursor: c_ulong,
    }

    impl EmptyCursor {
        pub fn new(conn: Connection) -> Self {
            unsafe {
                const EMPTY: &[u8] = &[0];

                let black = XColor { ..zeroed() };
                let pixmap = XCreateBitmapFromData(
                    conn.display(),
                    XDefaultRootWindow(conn.display()),
                    EMPTY.as_ptr() as _,
                    1,
                    1,
                );

                let cursor = XCreatePixmapCursor(
                    conn.display(),
                    pixmap,
                    pixmap,
                    &black as *const _ as *mut _,
                    &black as *const _ as *mut _,
                    0,
                    0,
                );

                XFreePixmap(conn.display(), pixmap);

                Self { conn, cursor }
            }
        }

        pub fn cursor(&self) -> c_ulong {
            self.cursor
        }
    }

    impl Drop for EmptyCursor {
        fn drop(&mut self) {
            unsafe {
                XFreeCursor(self.conn.display(), self.cursor);
            }
        }
    }

    pub fn load_cursor_by_name(conn: &Connection, name: &[&CStr]) -> Option<c_ulong> {
        for name in name {
            let cursor = unsafe { XcursorLibraryLoadCursor(conn.display(), name.as_ptr()) };
            if cursor != 0 {
                return Some(cursor);
            }
        }

        None
    }

    pub fn load_cursor_by_enum(conn: &Connection, cursor: super::MouseCursor) -> Option<c_ulong> {
        use super::MouseCursor::*;

        match cursor {
            Hidden => None,
            Default => load_cursor_by_name(conn, &[c"left_ptr"]),
            Hand => load_cursor_by_name(conn, &[c"hand2", c"hand1"]),
            HandGrabbing => load_cursor_by_name(conn, &[c"closedhand", c"grabbing"]),
            Help => load_cursor_by_name(conn, &[c"question_arrow"]),
            Text => load_cursor_by_name(conn, &[c"text", c"xterm"]),
            VerticalText => load_cursor_by_name(conn, &[c"vertical-text"]),
            Working => load_cursor_by_name(conn, &[c"watch"]),
            PtrWorking => load_cursor_by_name(conn, &[c"left_ptr_watch"]),
            NotAllowed => load_cursor_by_name(conn, &[c"crossed_circle"]),
            PtrNotAllowed => load_cursor_by_name(conn, &[c"no-drop", c"crossed_circle"]),
            ZoomIn => load_cursor_by_name(conn, &[c"zoom-in"]),
            ZoomOut => load_cursor_by_name(conn, &[c"zoom-out"]),
            Alias => load_cursor_by_name(conn, &[c"link"]),
            Copy => load_cursor_by_name(conn, &[c"copy"]),
            Move => load_cursor_by_name(conn, &[c"move"]),
            AllScroll => load_cursor_by_name(conn, &[c"all-scroll"]),
            Cell => load_cursor_by_name(conn, &[c"plus"]),
            Crosshair => load_cursor_by_name(conn, &[c"crosshair"]),
            EResize => load_cursor_by_name(conn, &[c"right_side"]),
            NResize => load_cursor_by_name(conn, &[c"top_side"]),
            NeResize => load_cursor_by_name(conn, &[c"top_right_corner"]),
            NwResize => load_cursor_by_name(conn, &[c"top_left_corner"]),
            SResize => load_cursor_by_name(conn, &[c"bottom_side"]),
            SeResize => load_cursor_by_name(conn, &[c"bottom_right_corner"]),
            SwResize => load_cursor_by_name(conn, &[c"bottom_left_corner"]),
            WResize => load_cursor_by_name(conn, &[c"left_side"]),
            EwResize => load_cursor_by_name(conn, &[c"h_double_arrow"]),
            NsResize => load_cursor_by_name(conn, &[c"v_double_arrow"]),
            NwseResize => load_cursor_by_name(conn, &[c"bd_double_arrow", c"size_bdiag"]),
            NeswResize => load_cursor_by_name(conn, &[c"fd_double_arrow", c"size_fdiag"]),
            ColResize => load_cursor_by_name(conn, &[c"split_h", c"h_double_arrow"]),
            RowResize => load_cursor_by_name(conn, &[c"split_v", c"v_double_arrow"]),
        }
    }
}

mod events {
    use super::Connection;
    use std::{ptr::null, time::Duration};
    use x11::xlib::*;

    pub fn wait_for_events(
        conn: &Connection,
        timeout: Option<Duration>,
    ) -> Result<impl ExactSizeIterator<Item = XEvent>, String> {
        unsafe {
            let timespec = timeout.map(|timeout| libc::timespec {
                tv_sec: timeout.as_secs() as _,
                tv_nsec: timeout.subsec_nanos() as _,
            });

            let result = libc::ppoll(
                &mut libc::pollfd {
                    fd: XConnectionNumber(conn.display()) as _,
                    events: libc::POLLIN,
                    revents: 0,
                },
                1 as _,
                timespec.as_ref().map(|x| x as *const _).unwrap_or(null()),
                null(),
            );

            if result == -1 {
                return Err(std::io::Error::last_os_error().to_string());
            }

            let pending = XPending(conn.display());
            Ok((0..pending).map(|_| {
                let mut event = XEvent { type_: 0 };
                XNextEvent(conn.display(), &mut event);
                event
            }))
        }
    }
}
