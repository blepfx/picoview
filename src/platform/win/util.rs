use std::{ffi::OsString, mem::size_of, os::windows::ffi::OsStrExt};
use windows::Win32::{
    Foundation::HINSTANCE,
    System::{
        Com::CoCreateGuid,
        SystemInformation::{
            VerSetConditionMask, VerifyVersionInfoW, OSVERSIONINFOEXW, VER_MAJORVERSION,
            VER_MINORVERSION, _WIN32_WINNT_WIN10,
        },
        SystemServices::{IMAGE_DOS_HEADER, VER_GREATER_EQUAL},
    },
};
use windows_core::GUID;

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

pub fn generate_guid() -> String {
    unsafe {
        let guid = CoCreateGuid().unwrap_or_default();
        format!(
            "{:0X}-{:0X}-{:0X}-{:0X}{:0X}-{:0X}{:0X}{:0X}{:0X}{:0X}{:0X}\0",
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

pub fn to_widestring(str: &str) -> Vec<u16> {
    OsString::from(str).encode_wide().chain([0]).collect()
}

extern "C" {
    static __ImageBase: IMAGE_DOS_HEADER;
}

pub fn hinstance() -> HINSTANCE {
    unsafe { HINSTANCE(&__ImageBase as *const IMAGE_DOS_HEADER as _) }
}
