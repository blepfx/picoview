use super::Connection;
use crate::{Key, Modifiers};
use std::ffi::{c_int, c_uint};
use x11::xinput2::*;
use x11::xlib::*;

/// Check if the given [`KeyRelease`] event is an auto-repeat and not a
/// physical release event.
pub fn is_autorepeat_release(conn: &Connection, event: &XKeyEvent) -> bool {
    if event.type_ != KeyRelease {
        return false;
    }

    unsafe {
        let mut next = XEvent { type_: 0 };
        if XEventsQueued(conn.as_raw(), 0 /* QueuedAlready */) == 0 {
            return false;
        }

        if XPeekEvent(conn.as_raw(), &mut next) == 0 {
            return false;
        }

        if next.type_ != KeyPress {
            return false;
        }

        let next = next.key;
        if next.keycode != event.keycode || next.time != event.time {
            return false;
        }
    }

    true
}

/// Convert event key code to a `Key` enum variant, if possible.
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

/// Convert modifier mask to a set of `Modifiers` flags, if possible.
pub fn keymask_to_mods(mods: c_uint) -> Modifiers {
    Modifiers {
        alt: (mods & Mod1Mask) != 0,
        ctrl: (mods & ControlMask) != 0,
        shift: (mods & ShiftMask) != 0,
        meta: (mods & Mod4Mask) != 0,
        num_lock: (mods & Mod2Mask) != 0,
        caps_lock: (mods & LockMask) != 0,
        scroll_lock: (mods & Mod5Mask) != 0,
    }
}

/// https://codebrowser.dev/gtk/include/X11/extensions/XI2.h.html
/// Valid only for XInput 2.4+
#[allow(non_upper_case_globals)]
pub const XI_GesturePinchBegin: i32 = 27;
#[allow(non_upper_case_globals)]
pub const XI_GesturePinchUpdate: i32 = 28;
#[allow(non_upper_case_globals)]
pub const XI_GesturePinchEnd: i32 = 29;

/// Gesture pinch event, valid only for XInput 2.4+
/// Copied from https://codebrowser.dev/gtk/include/X11/extensions/XI2.h.html
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct XIGesturePinchEvent {
    pub header: XIEvent,

    pub deviceid: i32,
    pub sourceid: i32,
    pub detail: i32,
    pub root: Window,
    pub event: Window,
    pub child: Window,
    pub root_x: f64,
    pub root_y: f64,
    pub event_x: f64,
    pub event_y: f64,

    pub delta_x: f64,
    pub delta_y: f64,
    pub delta_unaccel_x: f64,
    pub delta_unaccel_y: f64,
    pub scale: f64,
    pub delta_angle: f64,

    pub mods: XIModifierState,
    pub group: XIGroupState,
}

/// Information about the XInput2 extension.
pub struct XI2Extension {
    ext_opcode: c_int,
}

/// Information about an axis of a physical input device.
#[derive(Debug)]
pub struct XI2DeviceAxis {
    /// The source id of the physical device that this axis belongs to. The
    /// device id is implied to be the same.
    pub source_id: c_int,
    /// The valuator number for this axis.
    pub valuator: c_int,
    /// What kind of fruit is this?
    pub kind: XI2AxisKind,
    /// Inverse of the increment value for this axis, used to convert from the
    /// raw axis value to a normalized value.
    pub inv_increment: f64,
    /// Last known position of the axis, if any. Used to track deltas.
    pub position: Option<f64>,
}

/// Kind of axis for a physical input device.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum XI2AxisKind {
    /// Vertical mouse/trackpad scroll
    VerticalScroll,
    /// Horizontal mouse/trackpad scroll
    HorizontalScroll,
}

impl XI2Extension {
    /// Query the XInput2 extension and return its opcode if available.
    pub fn new(conn: &Connection) -> Option<Self> {
        unsafe {
            let mut ext_opcode = 0;
            if XQueryExtension(
                conn.as_raw(),
                c"XInputExtension".as_ptr() as _,
                &mut ext_opcode,
                &mut 0,
                &mut 0,
            ) == 0
            {
                None
            } else {
                // announce that we support xinput 2.4
                // so we get [`XGesturePinchEvent`] events.
                XIQueryVersion(conn.as_raw(), &mut 2, &mut 4);
                Some(Self { ext_opcode })
            }
        }
    }

    /// Returns true if the given event is an XInput2 event for this extension.
    ///
    /// Queries the event data if it is.
    pub fn query_event(
        &self,
        conn: &Connection,
        event: &mut XGenericEventCookie,
        f: impl FnOnce(*mut XIEvent),
    ) {
        unsafe {
            if event.extension == self.ext_opcode && XGetEventData(conn.as_raw(), event) != 0 {
                f(event.data as *mut XIEvent);
                XFreeEventData(conn.as_raw(), event);
            }
        }
    }

    /// Get all available axes for physical devices.
    pub fn list_axes(&self, conn: &Connection) -> Vec<XI2DeviceAxis> {
        let mut result = Vec::new();
        xi2_list_classes_for(conn, XIAllDevices, |device, class| {
            if device.deviceid != class.sourceid {
                return; // physical devices only
            }

            if class._type == XIScrollClass {
                let info = unsafe { &*(class as *const _ as *const XIScrollClassInfo) };
                if info.scroll_type == XIScrollTypeHorizontal {
                    result.push(XI2DeviceAxis {
                        source_id: info.sourceid,
                        valuator: info.number,
                        inv_increment: info.increment.recip(),
                        position: None,
                        kind: XI2AxisKind::HorizontalScroll,
                    });
                } else if info.scroll_type == XIScrollTypeVertical {
                    result.push(XI2DeviceAxis {
                        source_id: info.sourceid,
                        valuator: info.number,
                        inv_increment: info.increment.recip(),
                        position: None,
                        kind: XI2AxisKind::VerticalScroll,
                    });
                }
            }
        });

        result
    }
}

impl XI2DeviceAxis {
    /// Reset the position of the axis to the current value reported by the
    /// device.
    pub fn reset_position(&mut self, conn: &Connection) {
        xi2_list_classes_for(conn, self.source_id, |_, class| {
            if class._type == XIValuatorClass {
                let info = unsafe { &*(class as *const _ as *const XIValuatorClassInfo) };
                if info.sourceid == self.source_id && info.number == self.valuator {
                    self.position.replace(info.value);
                }
            }
        });
    }

    /// Track the delta of the axis position since the last reset or
    /// track_position call.
    pub fn track_position(&mut self, position: f64) -> f64 {
        (position - self.position.replace(position).unwrap_or(position)) * self.inv_increment
    }
}

/// Enumerate all (device, class) pairs for the given device id or all
/// devices if `XIAllDevices` is given.
fn xi2_list_classes_for(
    conn: &Connection,
    device_id: c_int,
    mut f: impl FnMut(&XIDeviceInfo, &XIAnyClassInfo),
) {
    unsafe {
        let mut count = 0;
        let info = XIQueryDevice(conn.as_raw(), device_id, &mut count);
        if info.is_null() {
            return;
        }

        for i in 0..count {
            let device = &*info.add(i as usize);
            let classes = std::slice::from_raw_parts(device.classes, device.num_classes as usize);

            for class in classes {
                let class = &**class;
                f(device, class);
            }
        }

        XIFreeDeviceInfo(info as *mut _);
    }
}
