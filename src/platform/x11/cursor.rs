use crate::MouseCursor;
use std::collections::hash_map::Entry;
use std::collections::HashMap;
use x11rb::cursor::Handle;
use x11rb::{connection::Connection, protocol::xproto::ConnectionExt, xcb_ffi::XCBConnection};

pub struct CursorCache {
    map: HashMap<MouseCursor, u32>,
}

impl CursorCache {
    pub fn new() -> Self {
        Self {
            map: HashMap::default(),
        }
    }

    pub fn get(
        &mut self,
        conn: &XCBConnection,
        screen: usize,
        handle: &Handle,
        cursor: MouseCursor,
    ) -> u32 {
        match self.map.entry(cursor) {
            Entry::Occupied(entry) => *entry.get(),
            Entry::Vacant(entry) => {
                let cursor = load(conn, screen, handle, cursor)
                    .or_else(|| load(conn, screen, handle, MouseCursor::Default))
                    .unwrap_or(x11rb::NONE);

                entry.insert(cursor);
                cursor
            }
        }
    }
}

fn load(conn: &XCBConnection, screen: usize, handle: &Handle, cursor: MouseCursor) -> Option<u32> {
    macro_rules! load {
        ($($l:literal),*) => {
            load_named(conn, handle, &[$($l),*])
        };
    }

    match cursor {
        MouseCursor::Default => load!("left_ptr"),

        MouseCursor::Hand => load!("hand2", "hand1"),
        MouseCursor::HandGrabbing => load!("closedhand", "grabbing"),
        MouseCursor::Help => load!("question_arrow"),

        MouseCursor::Hidden => create_empty(conn, screen),

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
