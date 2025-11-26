use crate::{Key, Modifiers, MouseCursor};
use objc2::runtime::ProtocolObject;
use objc2::sel;
use objc2::{
    ClassType, msg_send,
    rc::{Retained, autoreleasepool},
    runtime::{MessageReceiver, Sel},
};
use objc2_app_kit::{
    NSCursor, NSEventModifierFlags, NSHorizontalDirections, NSPasteboard, NSPasteboardTypeString,
    NSVerticalDirections,
};
use objc2_foundation::{NSArray, NSString};
use std::os::unix::process::CommandExt;
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::SystemTime;

fn try_get_cursor(selector: Sel) -> Retained<NSCursor> {
    unsafe {
        let class = NSCursor::class();
        if objc2::msg_send![class, respondsToSelector: selector] {
            let cursor: *mut NSCursor = class.send_message(selector, ());
            if let Some(cursor) = Retained::retain(cursor) {
                return cursor;
            }
        }

        NSCursor::arrowCursor()
    }
}

pub fn random_id() -> u32 {
    static STATE: AtomicU32 = AtomicU32::new(1);
    STATE
        .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |r| {
            let time = SystemTime::now()
                .duration_since(SystemTime::now())
                .unwrap_or_default()
                .as_nanos() as u32;

            let r = r ^ time;
            Some((r >> 1) ^ ((r & 1).wrapping_neg() & 0x80200003))
        })
        .unwrap_or_default()
}

pub fn get_clipboard_text() -> Option<String> {
    unsafe {
        autoreleasepool(|_| {
            let pasteboard: Option<Retained<NSPasteboard>> =
                msg_send![NSPasteboard::class(), generalPasteboard];
            let pasteboard = pasteboard?;
            let contents = pasteboard.pasteboardItems()?;

            for item in contents {
                if let Some(string) = item.stringForType(NSPasteboardTypeString) {
                    return Some(string.to_string());
                }
            }

            None
        })
    }
}

pub fn set_clipboard_text(text: &str) -> bool {
    unsafe {
        let pasteboard: Option<Retained<NSPasteboard>> =
            msg_send![NSPasteboard::class(), generalPasteboard];
        let pasteboard = match pasteboard {
            Some(pb) => pb,
            None => return false,
        };

        pasteboard.clearContents();
        let string_array = NSArray::from_retained_slice(&[ProtocolObject::from_retained(
            NSString::from_str(text),
        )]);
        pasteboard.writeObjects(&string_array)
    }
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

pub fn get_cursor(cursor: MouseCursor) -> Option<Retained<NSCursor>> {
    unsafe {
        Some(match cursor {
            MouseCursor::Hidden => return None,
            MouseCursor::Default => NSCursor::arrowCursor(),
            MouseCursor::Help => try_get_cursor(sel!(_helpCursor)),
            MouseCursor::Working => try_get_cursor(sel!(_waitCursor)),
            MouseCursor::PtrWorking => try_get_cursor(sel!(_busyButClickableCursor)),
            MouseCursor::Cell => NSCursor::crosshairCursor(),
            MouseCursor::Crosshair => NSCursor::crosshairCursor(),
            MouseCursor::Text => NSCursor::IBeamCursor(),
            MouseCursor::VerticalText => NSCursor::IBeamCursorForVerticalLayout(),
            MouseCursor::Alias => NSCursor::dragLinkCursor(),
            MouseCursor::Copy => NSCursor::dragCopyCursor(),
            MouseCursor::Move => NSCursor::openHandCursor(),
            MouseCursor::NotAllowed => NSCursor::operationNotAllowedCursor(),
            MouseCursor::PtrNotAllowed => NSCursor::operationNotAllowedCursor(),
            MouseCursor::Hand => NSCursor::openHandCursor(),
            MouseCursor::HandGrabbing => NSCursor::closedHandCursor(),
            MouseCursor::EResize => {
                NSCursor::columnResizeCursorInDirections(NSHorizontalDirections::Right)
            }
            MouseCursor::NResize => NSCursor::rowResizeCursorInDirections(NSVerticalDirections::Up),
            MouseCursor::NeResize => try_get_cursor(sel!(_windowResizeNorthEastCursor)),
            MouseCursor::NwResize => try_get_cursor(sel!(_windowResizeNorthWestCursor)),
            MouseCursor::SResize => {
                NSCursor::rowResizeCursorInDirections(NSVerticalDirections::Down)
            }
            MouseCursor::SeResize => try_get_cursor(sel!(_windowResizeSouthEastCursor)),
            MouseCursor::SwResize => try_get_cursor(sel!(_windowResizeSouthWestCursor)),
            MouseCursor::WResize => {
                NSCursor::columnResizeCursorInDirections(NSHorizontalDirections::Left)
            }
            MouseCursor::EwResize => {
                NSCursor::columnResizeCursorInDirections(NSHorizontalDirections::All)
            }
            MouseCursor::NsResize => {
                NSCursor::rowResizeCursorInDirections(NSVerticalDirections::All)
            }
            MouseCursor::NeswResize => try_get_cursor(sel!(_windowResizeNorthEastSouthWestCursor)),
            MouseCursor::NwseResize => try_get_cursor(sel!(_windowResizeNorthWestSouthEastCursor)),
            MouseCursor::ColResize => NSCursor::columnResizeCursor(),
            MouseCursor::RowResize => NSCursor::rowResizeCursor(),
            MouseCursor::AllScroll => NSCursor::openHandCursor(),
            MouseCursor::ZoomIn => NSCursor::zoomInCursor(),
            MouseCursor::ZoomOut => NSCursor::zoomOutCursor(),
        })
    }
}

pub fn flags2mods(flags: NSEventModifierFlags) -> Modifiers {
    const MODMAP: &[(NSEventModifierFlags, Modifiers)] = &[
        (NSEventModifierFlags::CapsLock, Modifiers::CAPS_LOCK),
        (NSEventModifierFlags::Command, Modifiers::META),
        (NSEventModifierFlags::Control, Modifiers::CTRL),
        (NSEventModifierFlags::Option, Modifiers::ALT),
        (NSEventModifierFlags::Shift, Modifiers::SHIFT),
    ];

    let mut modifiers = Modifiers::empty();
    for (flag, modifier) in MODMAP {
        if flags.contains(*flag) {
            modifiers.insert(*modifier);
        }
    }

    modifiers
}

pub fn keycode2key(key: u16) -> Option<Key> {
    Some(match key {
        0x00 => Key::A,
        0x01 => Key::S,
        0x02 => Key::D,
        0x03 => Key::F,
        0x04 => Key::H,
        0x05 => Key::G,
        0x06 => Key::Z,
        0x07 => Key::X,
        0x08 => Key::C,
        0x09 => Key::V,
        0x0b => Key::B,
        0x0c => Key::Q,
        0x0d => Key::W,
        0x0e => Key::E,
        0x0f => Key::R,
        0x10 => Key::Y,
        0x11 => Key::T,
        0x12 => Key::D1,
        0x13 => Key::D2,
        0x14 => Key::D3,
        0x15 => Key::D4,
        0x16 => Key::D6,
        0x17 => Key::D5,
        0x18 => Key::Equal,
        0x19 => Key::D9,
        0x1a => Key::D7,
        0x1b => Key::Minus,
        0x1c => Key::D8,
        0x1d => Key::D0,
        0x1e => Key::BracketRight,
        0x1f => Key::O,
        0x20 => Key::U,
        0x21 => Key::BracketLeft,
        0x22 => Key::I,
        0x23 => Key::P,
        0x24 => Key::Enter,
        0x25 => Key::L,
        0x26 => Key::J,
        0x27 => Key::Quote,
        0x28 => Key::K,
        0x29 => Key::Semicolon,
        0x2a => Key::Backslash,
        0x2b => Key::Comma,
        0x2c => Key::Slash,
        0x2d => Key::N,
        0x2e => Key::M,
        0x2f => Key::Period,
        0x30 => Key::Tab,
        0x31 => Key::Space,
        0x32 => Key::Backquote,
        0x33 => Key::Backspace,
        0x34 => Key::NumpadEnter,
        0x35 => Key::Escape,
        0x36 => Key::MetaRight,
        0x37 => Key::MetaLeft,
        0x38 => Key::ShiftLeft,
        0x39 => Key::CapsLock,
        0x3a => Key::AltLeft,
        0x3b => Key::ControlLeft,
        0x3c => Key::ShiftRight,
        0x3d => Key::AltRight,
        0x3e => Key::ControlRight,
        0x3f => Key::Fn, // No events fired
        0x41 => Key::NumpadDecimal,
        0x43 => Key::NumpadMultiply,
        0x45 => Key::NumpadAdd,
        0x47 => Key::NumLock,
        0x4b => Key::NumpadDivide,
        0x4c => Key::NumpadEnter,
        0x4e => Key::NumpadSubtract,
        0x51 => Key::NumpadEqual,
        0x52 => Key::Numpad0,
        0x53 => Key::Numpad1,
        0x54 => Key::Numpad2,
        0x55 => Key::Numpad3,
        0x56 => Key::Numpad4,
        0x57 => Key::Numpad5,
        0x58 => Key::Numpad6,
        0x59 => Key::Numpad7,
        0x5b => Key::Numpad8,
        0x5c => Key::Numpad9,
        0x5f => Key::NumpadComma,
        0x60 => Key::F5,
        0x61 => Key::F6,
        0x62 => Key::F7,
        0x63 => Key::F3,
        0x64 => Key::F8,
        0x65 => Key::F9,
        0x67 => Key::F11,
        0x69 => Key::PrintScreen,
        0x6d => Key::F10,
        0x6e => Key::ContextMenu,
        0x6f => Key::F12,
        0x73 => Key::Home,
        0x74 => Key::PageUp,
        0x75 => Key::Delete,
        0x76 => Key::F4,
        0x77 => Key::End,
        0x78 => Key::F2,
        0x79 => Key::PageDown,
        0x7a => Key::F1,
        0x7b => Key::ArrowLeft,
        0x7c => Key::ArrowRight,
        0x7d => Key::ArrowDown,
        0x7e => Key::ArrowUp,
        _ => return None,
    })
}
