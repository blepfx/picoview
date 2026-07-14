use crate::MouseCursor;
use crate::platform::x11::util::Connection;
use std::ffi::{CStr, c_ulong};
use std::mem::zeroed;
use x11::xcursor::XcursorLibraryLoadCursor;
use x11::xlib::{
    XColor, XCreateBitmapFromData, XCreatePixmapCursor, XDefaultRootWindow, XFreeCursor,
    XFreePixmap,
};

/// A handle to an X11 cursor.
pub struct X11Cursor {
    conn: Connection,
    cursor: c_ulong,
}

impl X11Cursor {
    /// Creates a new empty (1x1 transparent) cursor that can be used to
    /// hide the mouse cursor.
    pub fn empty(conn: Connection) -> Self {
        unsafe {
            const EMPTY: &[u8] = &[0];

            let black = XColor { ..zeroed() };
            let pixmap = XCreateBitmapFromData(
                conn.as_raw(),
                XDefaultRootWindow(conn.as_raw()),
                EMPTY.as_ptr() as _,
                1,
                1,
            );

            let cursor = XCreatePixmapCursor(
                conn.as_raw(),
                pixmap,
                pixmap,
                &black as *const _ as *mut _,
                &black as *const _ as *mut _,
                0,
                0,
            );

            XFreePixmap(conn.as_raw(), pixmap);

            Self { conn, cursor }
        }
    }

    /// Loads a predefined cursor from the Xcursor library by name. Returns
    /// `None` if the cursor could not be found.
    pub fn load_by_name(conn: Connection, name: &[&CStr]) -> Option<Self> {
        for name in name {
            let cursor = unsafe { XcursorLibraryLoadCursor(conn.as_raw(), name.as_ptr()) };
            if cursor != 0 {
                return Some(Self { conn, cursor });
            }
        }

        None
    }

    /// Loads a cursor corresponding to the given [`MouseCursor`] variant.
    pub fn load(conn: Connection, cursor: MouseCursor) -> Option<Self> {
        use MouseCursor::*;

        match cursor {
            Hidden => Some(Self::empty(conn)),
            Default => Self::load_by_name(conn, &[c"left_ptr"]),
            Hand => Self::load_by_name(conn, &[c"hand2", c"hand1"]),
            HandGrabbing => Self::load_by_name(conn, &[c"closedhand", c"grabbing"]),
            Help => Self::load_by_name(conn, &[c"question_arrow"]),
            Text => Self::load_by_name(conn, &[c"text", c"xterm"]),
            VerticalText => Self::load_by_name(conn, &[c"vertical-text"]),
            Working => Self::load_by_name(conn, &[c"watch"]),
            PtrWorking => Self::load_by_name(conn, &[c"left_ptr_watch"]),
            NotAllowed => Self::load_by_name(conn, &[c"crossed_circle"]),
            PtrNotAllowed => Self::load_by_name(conn, &[c"no-drop", c"crossed_circle"]),
            ZoomIn => Self::load_by_name(conn, &[c"zoom-in"]),
            ZoomOut => Self::load_by_name(conn, &[c"zoom-out"]),
            Alias => Self::load_by_name(conn, &[c"link"]),
            Copy => Self::load_by_name(conn, &[c"copy"]),
            Move => Self::load_by_name(conn, &[c"move"]),
            AllScroll => Self::load_by_name(conn, &[c"all-scroll"]),
            Cell => Self::load_by_name(conn, &[c"plus"]),
            Crosshair => Self::load_by_name(conn, &[c"crosshair"]),
            EResize => Self::load_by_name(conn, &[c"right_side"]),
            NResize => Self::load_by_name(conn, &[c"top_side"]),
            NeResize => Self::load_by_name(conn, &[c"top_right_corner"]),
            NwResize => Self::load_by_name(conn, &[c"top_left_corner"]),
            SResize => Self::load_by_name(conn, &[c"bottom_side"]),
            SeResize => Self::load_by_name(conn, &[c"bottom_right_corner"]),
            SwResize => Self::load_by_name(conn, &[c"bottom_left_corner"]),
            WResize => Self::load_by_name(conn, &[c"left_side"]),
            EwResize => Self::load_by_name(conn, &[c"h_double_arrow"]),
            NsResize => Self::load_by_name(conn, &[c"v_double_arrow"]),
            NwseResize => Self::load_by_name(conn, &[c"bd_double_arrow", c"size_bdiag"]),
            NeswResize => Self::load_by_name(conn, &[c"fd_double_arrow", c"size_fdiag"]),
            ColResize => Self::load_by_name(conn, &[c"split_h", c"h_double_arrow"]),
            RowResize => Self::load_by_name(conn, &[c"split_v", c"v_double_arrow"]),
        }
    }

    pub fn as_raw(&self) -> c_ulong {
        self.cursor
    }
}

impl Drop for X11Cursor {
    fn drop(&mut self) {
        unsafe {
            XFreeCursor(self.conn.as_raw(), self.cursor);
        }
    }
}
