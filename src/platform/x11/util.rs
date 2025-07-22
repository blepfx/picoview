use crate::{Key, Modifiers};
use std::{
    os::unix::process::CommandExt,
    process::{Command, Stdio},
};
use x11rb::protocol::xproto::KeyButMask;

macro_rules! cstr {
    ($str:literal) => {
        #[allow(unused_unsafe)]
        unsafe {
            std::ffi::CStr::from_bytes_with_nul_unchecked(concat!($str, "\0").as_bytes())
        }
    };
}

pub(crate) use cstr;

pub fn open_url(path: &str) -> bool {
    if let Ok(()) = spawn_detached(Command::new("xdg-open").arg(&path)) {
        return true;
    }

    if let Ok(()) = spawn_detached(Command::new("gio").args(&["open", &path])) {
        return true;
    }

    if let Ok(()) = spawn_detached(Command::new("gnome-open").arg(&path)) {
        return true;
    }

    if let Ok(()) = spawn_detached(Command::new("kde-open").arg(&path)) {
        return true;
    }

    false
}

pub fn spawn_detached(cmd: &mut Command) -> std::io::Result<()> {
    cmd.stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null());

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

pub fn hwcode2key(code: u8) -> Option<Key> {
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

pub fn hwcode2mods(code: u8) -> Modifiers {
    match code {
        0x25 | 0x69 => Modifiers::CTRL,
        0x32 | 0x3E => Modifiers::SHIFT,
        0x85 | 0x86 => Modifiers::META,
        0x40 | 0x6C => Modifiers::ALT,
        _ => Modifiers::empty(),
    }
}

pub fn keymask2mods(mods: KeyButMask) -> Modifiers {
    const MAP: &[(KeyButMask, Modifiers)] = &[
        (KeyButMask::SHIFT, Modifiers::SHIFT),
        (KeyButMask::CONTROL, Modifiers::CTRL),
        (KeyButMask::MOD1, Modifiers::ALT),
        (KeyButMask::MOD4, Modifiers::META),
        (KeyButMask::MOD2, Modifiers::NUM_LOCK),
        (KeyButMask::LOCK, Modifiers::CAPS_LOCK),
    ];

    let mut ret = Modifiers::empty();
    for (mask, modifiers) in MAP {
        if mods.contains(*mask) {
            ret |= *modifiers;
        }
    }
    ret
}
