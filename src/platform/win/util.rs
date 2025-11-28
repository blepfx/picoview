use crate::{Error, Key, Modifiers, Size};
use std::{
    ffi::{CStr, OsString},
    os::windows::ffi::OsStrExt,
    ptr::null_mut,
};
use windows_sys::{
    Win32::{
        Foundation::{GetLastError, HINSTANCE, HWND, POINT, RECT},
        System::{
            Com::CoCreateGuid,
            Diagnostics::Debug::{
                FORMAT_MESSAGE_ALLOCATE_BUFFER, FORMAT_MESSAGE_FROM_SYSTEM,
                FORMAT_MESSAGE_IGNORE_INSERTS, FormatMessageW,
            },
            LibraryLoader::{GetProcAddress, LoadLibraryA},
            SystemServices::IMAGE_DOS_HEADER,
        },
        UI::{
            Input::KeyboardAndMouse::{
                GetAsyncKeyState, GetKeyState, VIRTUAL_KEY, VK_CAPITAL, VK_CONTROL, VK_LWIN,
                VK_MENU, VK_NUMLOCK, VK_RWIN, VK_SCROLL, VK_SHIFT,
            },
            WindowsAndMessaging::{
                AdjustWindowRectEx, DispatchMessageW, GetMessageW, MSG, TranslateMessage,
                WINDOW_STYLE,
            },
        },
    },
    core::{GUID, PWSTR},
};

pub unsafe fn load_function_dynamic<A, R>(
    module: &CStr,
    function: &CStr,
) -> Option<unsafe fn(A) -> R> {
    unsafe {
        let lib = LoadLibraryA(module.as_ptr() as *const _);
        if lib.is_null() {
            None
        } else {
            let proc = GetProcAddress(lib, function.as_ptr() as *const _);
            proc.map(|x| std::mem::transmute(x))
        }
    }
}

pub fn generate_guid() -> String {
    unsafe {
        let mut guid = std::mem::zeroed::<GUID>();
        CoCreateGuid(&mut guid);

        format!(
            "{:0X}-{:0X}-{:0X}-{:0X}{:0X}-{:0X}{:0X}{:0X}{:0X}{:0X}{:0X}",
            guid.data1,
            guid.data2,
            guid.data3,
            guid.data4[0],
            guid.data4[1],
            guid.data4[2],
            guid.data4[3],
            guid.data4[4],
            guid.data4[5],
            guid.data4[6],
            guid.data4[7]
        )
    }
}

pub unsafe fn run_event_loop(hwnd: HWND) {
    unsafe {
        let mut msg: MSG = std::mem::zeroed();
        loop {
            if GetMessageW(&mut msg, hwnd, 0, 0) == 0 {
                break;
            }

            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
    }
}

pub fn to_widestring(str: &str) -> Vec<u16> {
    OsString::from(str).encode_wide().chain([0]).collect()
}

pub unsafe fn from_widestring(wide: *const u16) -> String {
    unsafe {
        let mut i = 0;
        loop {
            if wide.add(i).read() == 0 {
                let data = std::slice::from_raw_parts(wide, i);
                return String::from_utf16_lossy(data);
            }

            i += 1;
        }
    }
}

pub fn hinstance() -> HINSTANCE {
    unsafe extern "C" {
        unsafe static __ImageBase: IMAGE_DOS_HEADER;
    }

    unsafe { &__ImageBase as *const IMAGE_DOS_HEADER as _ }
}

pub fn check_error(assert: bool, message: &'static str) -> Result<(), crate::Error> {
    if !assert {
        unsafe {
            let error = GetLastError();
            let mut buffer = null_mut::<u16>();
            let chars = FormatMessageW(
                FORMAT_MESSAGE_ALLOCATE_BUFFER
                    | FORMAT_MESSAGE_FROM_SYSTEM
                    | FORMAT_MESSAGE_IGNORE_INSERTS,
                null_mut(),
                error,
                0,
                &mut buffer as *mut PWSTR as *mut _,
                0,
                null_mut(),
            );

            let extra = if chars == 0 || buffer.is_null() {
                None
            } else {
                let parts = std::slice::from_raw_parts(buffer, chars as _);
                Some(String::from_utf16_lossy(parts))
            };

            return Err(Error::PlatformError(match extra {
                Some(desc) => format!("{}: {:X} - {}", message, error, desc),
                None => format!("{}: {:X}", message, error),
            }));
        }
    }

    Ok(())
}

pub fn scan_code_to_key(scan_code: u32) -> Option<Key> {
    use Key::*;
    Some(match scan_code {
        0x1 => Escape,
        0x2 => D1,
        0x3 => D2,
        0x4 => D3,
        0x5 => D4,
        0x6 => D5,
        0x7 => D6,
        0x8 => D7,
        0x9 => D8,
        0xA => D9,
        0xB => D0,
        0xC => Minus,
        0xD => Equal,
        0xE => Backspace,
        0xF => Tab,
        0x10 => Q,
        0x11 => W,
        0x12 => E,
        0x13 => R,
        0x14 => T,
        0x15 => Y,
        0x16 => U,
        0x17 => I,
        0x18 => O,
        0x19 => P,
        0x1A => BracketLeft,
        0x1B => BracketRight,
        0x1C => Enter,
        0x1D => ControlLeft,
        0x1E => A,
        0x1F => S,
        0x20 => D,
        0x21 => F,
        0x22 => G,
        0x23 => H,
        0x24 => J,
        0x25 => K,
        0x26 => L,
        0x27 => Semicolon,
        0x28 => Quote,
        0x29 => Backquote,
        0x2A => ShiftLeft,
        0x2B => Backslash,
        0x2C => Z,
        0x2D => X,
        0x2E => C,
        0x2F => V,
        0x30 => B,
        0x31 => N,
        0x32 => M,
        0x33 => Comma,
        0x34 => Period,
        0x35 => Slash,
        0x36 => ShiftRight,
        0x37 => NumpadMultiply,
        0x38 => AltLeft,
        0x39 => Space,
        0x3A => CapsLock,
        0x3B => F1,
        0x3C => F2,
        0x3D => F3,
        0x3E => F4,
        0x3F => F5,
        0x40 => F6,
        0x41 => F7,
        0x42 => F8,
        0x43 => F9,
        0x44 => F10,
        0x46 => ScrollLock,
        0x47 => Numpad7,
        0x48 => Numpad8,
        0x49 => Numpad9,
        0x4A => NumpadSubtract,
        0x4B => Numpad4,
        0x4C => Numpad5,
        0x4D => Numpad6,
        0x4E => NumpadAdd,
        0x4F => Numpad1,
        0x50 => Numpad2,
        0x51 => Numpad3,
        0x52 => Numpad0,
        0x53 => NumpadDecimal,
        0x54 => PrintScreen,
        0x57 => F11,
        0x58 => F12,
        0x59 => NumpadEqual,
        0x7E => NumpadComma,
        0x11C => NumpadEnter,
        0x11D => ControlRight,
        0x135 => NumpadDivide,
        0x137 => PrintScreen,
        0x138 => AltRight,
        0x145 => NumLock,
        0x147 => Home,
        0x148 => ArrowUp,
        0x149 => PageUp,
        0x14B => ArrowLeft,
        0x14D => ArrowRight,
        0x14F => End,
        0x150 => ArrowDown,
        0x151 => PageDown,
        0x152 => Insert,
        0x153 => Delete,
        0x15B => MetaLeft,
        0x15C => MetaRight,
        0x15D => ContextMenu,
        _ => return None,
    })
}

pub unsafe fn get_modifiers_async() -> Modifiers {
    const KEY_MODIFIERS: &[(VIRTUAL_KEY, Modifiers)] = &[
        (VK_SHIFT, Modifiers::SHIFT),
        (VK_CONTROL, Modifiers::CTRL),
        (VK_MENU, Modifiers::ALT),
        (VK_LWIN, Modifiers::META),
        (VK_RWIN, Modifiers::META),
    ];

    const TOGGLE_MODIFIERS: &[(VIRTUAL_KEY, Modifiers)] = &[
        (VK_CAPITAL, Modifiers::CAPS_LOCK),
        (VK_NUMLOCK, Modifiers::NUM_LOCK),
        (VK_SCROLL, Modifiers::SCROLL_LOCK),
    ];

    let mut state = Modifiers::empty();

    unsafe {
        for &(key, mods) in KEY_MODIFIERS {
            if GetAsyncKeyState(key as _) != 0 {
                state.insert(mods);
            }
        }

        for &(key, mods) in TOGGLE_MODIFIERS {
            if GetKeyState(key as _) & 0x1 != 0 {
                state.insert(mods);
            }
        }
    }

    state
}

pub fn window_size_from_client_size(size: Size, dwstyle: WINDOW_STYLE) -> POINT {
    unsafe {
        let mut rect = RECT {
            left: 0,
            top: 0,
            right: size.width.try_into().unwrap_or(i32::MAX / 2),
            bottom: size.height.try_into().unwrap_or(i32::MAX / 2),
        };

        AdjustWindowRectEx(&mut rect, dwstyle, 0, 0);

        POINT {
            x: rect.right - rect.left,
            y: rect.bottom - rect.top,
        }
    }
}
