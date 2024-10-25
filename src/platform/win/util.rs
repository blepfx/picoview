use std::{ffi::OsString, mem::size_of, os::windows::ffi::OsStrExt};
use windows::Win32::System::{
    SystemInformation::{
        VerSetConditionMask, VerifyVersionInfoW, OSVERSIONINFOEXW, VER_MAJORVERSION,
        VER_MINORVERSION, _WIN32_WINNT_WIN10,
    },
    SystemServices::VER_GREATER_EQUAL,
};

pub fn is_windows10_or_greater() -> bool {
    is_windows_version_or_greater(_WIN32_WINNT_WIN10)
}

fn is_windows_version_or_greater(version: u32) -> bool {
    let major_version = version >> 8 & 0xFF;
    let minor_version = version & 0xFF;

    let mut version_info = OSVERSIONINFOEXW {
        dwOSVersionInfoSize: size_of::<OSVERSIONINFOEXW>() as u32,
        dwMajorVersion: major_version,
        dwMinorVersion: minor_version,

        ..Default::default()
    };

    unsafe {
        let condition_mask = VerSetConditionMask(0, VER_MAJORVERSION, VER_GREATER_EQUAL as u8);
        let condition_mask =
            VerSetConditionMask(condition_mask, VER_MINORVERSION, VER_GREATER_EQUAL as u8);

        VerifyVersionInfoW(
            &mut version_info,
            VER_MAJORVERSION | VER_MINORVERSION,
            condition_mask,
        )
        .is_ok()
    }
}

pub fn to_widestring(str: &str) -> Vec<u16> {
    OsString::from(str).encode_wide().chain([0]).collect()
}
