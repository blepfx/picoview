use crate::{Key, Modifiers, MouseCursor, platform::x11::connection::Connection};
use libc::{c_int, c_uint};
use std::{
    ffi::CStr,
    mem::zeroed,
    os::unix::process::CommandExt,
    process::{Command, Stdio},
    ptr::null_mut,
};
use x11::{
    glx::GLXFBConfig,
    xlib::{
        ControlMask, CopyFromParent, LockMask, Mod1Mask, Mod2Mask, Mod4Mask, ShiftMask, TrueColor,
        Visual, XMatchVisualInfo, XVisualInfo,
    },
};

pub fn open_url(path: &str) -> bool {
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

pub fn spawn_detached(cmd: &mut Command) -> std::io::Result<()> {
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

pub fn keycode_to_key(code: u32) -> Option<Key> {
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

pub fn keycode_to_mods(code: u32) -> Modifiers {
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

pub fn get_cursor(conn: &Connection, cursor: MouseCursor) -> u64 {
    fn load(conn: &Connection, names: &[&'static CStr]) -> u64 {
        for name in names {
            let cursor = conn.cursor(Some(*name));
            if cursor != 0 {
                return cursor;
            }
        }

        conn.cursor(Some(c"left_ptr"))
    }

    match cursor {
        MouseCursor::Default => load(conn, &[c"left_ptr"]),
        MouseCursor::Hand => load(conn, &[c"hand2", c"hand1"]),
        MouseCursor::HandGrabbing => load(conn, &[c"closedhand", c"grabbing"]),
        MouseCursor::Help => load(conn, &[c"question_arrow"]),
        MouseCursor::Hidden => conn.cursor(None),
        MouseCursor::Text => load(conn, &[c"text", c"xterm"]),
        MouseCursor::VerticalText => load(conn, &[c"vertical-text"]),
        MouseCursor::Working => load(conn, &[c"watch"]),
        MouseCursor::PtrWorking => load(conn, &[c"left_ptr_watch"]),
        MouseCursor::NotAllowed => load(conn, &[c"crossed_circle"]),
        MouseCursor::PtrNotAllowed => load(conn, &[c"no-drop", c"crossed_circle"]),
        MouseCursor::ZoomIn => load(conn, &[c"zoom-in"]),
        MouseCursor::ZoomOut => load(conn, &[c"zoom-out"]),
        MouseCursor::Alias => load(conn, &[c"link"]),
        MouseCursor::Copy => load(conn, &[c"copy"]),
        MouseCursor::Move => load(conn, &[c"move"]),
        MouseCursor::AllScroll => load(conn, &[c"all-scroll"]),
        MouseCursor::Cell => load(conn, &[c"plus"]),
        MouseCursor::Crosshair => load(conn, &[c"crosshair"]),
        MouseCursor::EResize => load(conn, &[c"right_side"]),
        MouseCursor::NResize => load(conn, &[c"top_side"]),
        MouseCursor::NeResize => load(conn, &[c"top_right_corner"]),
        MouseCursor::NwResize => load(conn, &[c"top_left_corner"]),
        MouseCursor::SResize => load(conn, &[c"bottom_side"]),
        MouseCursor::SeResize => load(conn, &[c"bottom_right_corner"]),
        MouseCursor::SwResize => load(conn, &[c"bottom_left_corner"]),
        MouseCursor::WResize => load(conn, &[c"left_side"]),
        MouseCursor::EwResize => load(conn, &[c"h_double_arrow"]),
        MouseCursor::NsResize => load(conn, &[c"v_double_arrow"]),
        MouseCursor::NwseResize => load(conn, &[c"bd_double_arrow", c"size_bdiag"]),
        MouseCursor::NeswResize => load(conn, &[c"fd_double_arrow", c"size_fdiag"]),
        MouseCursor::ColResize => load(conn, &[c"split_h", c"h_double_arrow"]),
        MouseCursor::RowResize => load(conn, &[c"split_v", c"v_double_arrow"]),
    }
}

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
                conn.screen(),
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
