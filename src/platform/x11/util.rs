pub mod connection;
pub mod cursor;
pub mod info;
pub mod input;
pub mod visual;

use crate::Point;
use std::ffi::c_ulong;
use std::os::unix::process::CommandExt;
use std::process::{Command, Stdio};
use x11::xlib::*;

/// Open the given URL with the default system handler. Returns `true` if we
/// successfully started a process.
///
/// Tries a bunch of different `open` commands.
pub fn open_url(path: &str) -> bool {
    /// Spawns a process in a detached state, so it won't be killed when the
    /// parent process exits.
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

/// Returns the position of the given window's client area relative to the root
/// window (the screen), or `None` if the position could not be determined.
pub fn window_position(conn: &Connection, window_id: c_ulong) -> Option<Point> {
    let mut x = 0;
    let mut y = 0;

    let status = unsafe {
        XTranslateCoordinates(
            conn.as_raw(),
            window_id,
            XDefaultRootWindow(conn.as_raw()),
            0,
            0,
            &mut x,
            &mut y,
            &mut 0,
        )
    };

    if status != 0 {
        Some(Point {
            x: x as f64,
            y: y as f64,
        })
    } else {
        None
    }
}

pub use connection::*;
pub use cursor::*;
pub use info::*;
pub use input::*;
pub use selection::*;
pub use visual::*;

mod selection {
    use super::Connection;
    use crate::{DropEffect, Exchange};
    use std::array::from_fn;
    use std::ffi::{OsStr, OsString, c_char, c_int, c_ulong};
    use std::mem::zeroed;
    use std::os::unix::ffi::OsStrExt;
    use std::path::PathBuf;
    use std::ptr::null_mut;
    use x11::xlib::*;

    /// An error that can occur when requesting a selection value.
    #[derive(Debug, PartialEq, Eq, Clone, Copy)]
    pub enum SelectionError {
        /// Selection is empty
        Empty,
        /// Selection is owned by the current window and must be handled
        /// separately to avoid a deadlock
        Reentrant,
    }

    /// Encode a list of file paths into a `text/uri-list` selection value
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

    /// Decode a list of file URIs from the given selection data
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

    /// Request a selection value (clipboard/drag-n-drop) and wait for the
    /// response.
    pub fn request_selection<R>(
        conn: &Connection,
        window: c_ulong,
        selection: c_ulong,
        property: c_ulong,
        target: c_ulong,
        timestamp: c_ulong,
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
            let owner = XGetSelectionOwner(conn.as_raw(), selection);
            if owner == 0 {
                return Err(SelectionError::Empty);
            } else if window == owner {
                return Err(SelectionError::Reentrant);
            }

            let result = XConvertSelection(
                conn.as_raw(),
                selection,
                target,
                property,
                window,
                timestamp,
            );

            if result == 0 {
                return Err(SelectionError::Empty);
            }

            XSync(conn.as_raw(), 0);

            let event = {
                let mut event = zeroed();
                XIfEvent(conn.as_raw(), &mut event, Some(event_filter), null_mut());
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
                conn.as_raw(),
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

            let result = f(std::slice::from_raw_parts(
                data as *const u8,
                size.try_into().unwrap_or(usize::MAX),
            ));

            XFree(data as *mut _);
            Ok(result)
        }
    }

    /// Read a selection via [`request_selection`] and decode it into an
    /// [`Exchange`] value.
    pub fn parse_selection(
        conn: &Connection,
        window: c_ulong,
        selection: c_ulong,
        property: c_ulong,
        timestamp: c_ulong,
    ) -> Result<Exchange, SelectionError> {
        let a_utf8_string = conn.atom(c"UTF8_STRING");
        let a_text_uri_list = conn.atom(c"text/uri-list");
        let a_text_plain = conn.atom(c"text/plain");

        for atom in [a_text_uri_list, a_text_plain, a_utf8_string, XA_STRING] {
            let result = request_selection(
                conn,
                window,
                selection,
                property,
                atom,
                timestamp,
                |slice| {
                    if atom == a_text_uri_list {
                        Exchange::Files(decode_uri_list(OsStr::from_bytes(slice)))
                    } else {
                        Exchange::Text(String::from_utf8_lossy(slice).to_string())
                    }
                },
            );

            match result {
                Ok(Exchange::Empty) => continue,
                Ok(exchange) => return Ok(exchange),
                Err(SelectionError::Empty) => continue,
                Err(SelectionError::Reentrant) => {
                    return Err(SelectionError::Reentrant);
                }
            }
        }

        Err(SelectionError::Empty)
    }

    pub fn send_xdnd_feedback(
        conn: &Connection,
        target: c_ulong,
        source: c_ulong,
        finished: bool,
        effect: DropEffect,
    ) {
        unsafe {
            XSendEvent(
                conn.as_raw(),
                source,
                0,
                0,
                &mut XEvent {
                    client_message: XClientMessageEvent {
                        type_: ClientMessage,
                        serial: 0,
                        send_event: 1,
                        display: conn.as_raw(),
                        window: source,
                        message_type: if finished {
                            conn.atom(c"XdndFinished")
                        } else {
                            conn.atom(c"XdndStatus")
                        },
                        format: 32,

                        data: {
                            let mut data = ClientMessageData::default();
                            data.set_long(0, target as _);
                            data.set_long(1, if effect == DropEffect::Reject { 0 } else { 1 }); // success
                            data.set_long(
                                2,
                                match effect {
                                    DropEffect::Move => conn.atom(c"XdndActionMove") as _,
                                    DropEffect::Link => conn.atom(c"XdndActionLink") as _,
                                    DropEffect::Generic => conn.atom(c"XdndActionPrivate") as _,
                                    DropEffect::Copy | DropEffect::Reject => {
                                        conn.atom(c"XdndActionCopy") as _
                                    }
                                },
                            );
                            data
                        },
                    },
                },
            );

            // just in case
            XFlush(conn.as_raw());
        }
    }
}
