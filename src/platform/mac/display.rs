use crate::Error;
use objc2_foundation::{NSPoint, NSRect};
use std::{
    ffi::{c_double, c_void},
    ptr::{NonNull, null_mut},
};

pub fn get_displays_with_rect(rect: NSRect) -> Result<Vec<u32>, Error> {
    const MAX_DISPLAYS: usize = 10;
    let mut displays = vec![0; MAX_DISPLAYS];
    let mut matching_displays = 0;

    unsafe {
        // needed to properly initialize coregraphics
        CGMainDisplayID();

        let result = CGGetDisplaysWithRect(
            rect,
            MAX_DISPLAYS as u32,
            displays.as_mut_ptr(),
            &mut matching_displays,
        );
        if result != 0 {
            return Err(Error::PlatformError(format!(
                "CGGetDisplaysWithRect failed: {result:?}"
            )));
        }

        if matching_displays == 0 {
            return Err(Error::PlatformError(format!(
                "CGGetDisplaysWithRect: no matching displays found"
            )));
        }
    }

    displays.resize(matching_displays as usize, 0);
    Ok(displays)
}

impl CVDisplayLink {
    pub fn create_with_active_cg_displays() -> Result<Self, Error> {
        let mut display_link_ptr = null_mut();
        unsafe {
            let result = CVDisplayLinkCreateWithActiveCGDisplays(&mut display_link_ptr);
            if result != 0 || display_link_ptr.is_null() {
                return Err(Error::PlatformError(format!(
                    "CVDisplayLinkCreateWithActiveCGDisplays failed: {result:?}"
                )));
            }

            Ok(Self(NonNull::new_unchecked(display_link_ptr)))
        }
    }

    pub fn set_output_callback(
        &mut self,
        callback: CVDisplayLinkOutputCallback,
        user_info: *mut c_void,
    ) -> Result<(), Error> {
        unsafe {
            let result = CVDisplayLinkSetOutputCallback(self.0.as_ptr(), callback, user_info);
            if result != 0 {
                return Err(Error::PlatformError(format!(
                    "CVDisplayLinkSetOutputCallback failed: {result:?}"
                )));
            }

            Ok(())
        }
    }

    pub fn set_current_display(&mut self, display: u32) -> Result<(), Error> {
        unsafe {
            let result = CVDisplayLinkSetCurrentCGDisplay(self.0.as_ptr(), display);
            if result != 0 {
                return Err(Error::PlatformError(format!(
                    "CVDisplayLinkSetCurrentCGDisplay failed: {result:?}"
                )));
            }

            Ok(())
        }
    }

    pub fn start(&mut self) -> Result<(), Error> {
        unsafe {
            let result = CVDisplayLinkStart(self.0.as_ptr());
            if result != 0 {
                return Err(Error::PlatformError(format!(
                    "CVDisplayLinkStart failed: {result:?}"
                )));
            }

            Ok(())
        }
    }
}

#[inline]
pub unsafe fn warp_mouse_cursor_position(point: NSPoint) -> bool {
    unsafe { CGWarpMouseCursorPosition(point) == 0 }
}

impl Drop for CVDisplayLink {
    fn drop(&mut self) {
        unsafe {
            CVDisplayLinkRelease(self.0.as_ptr());
        }
    }
}

#[repr(C)]
#[derive(Debug, Clone)]
pub struct CVSMPTETime {
    pub subframes: i16,
    pub subframe_divisor: i16,
    pub counter: u32,
    pub type_: u32,
    pub flags: u32,
    pub hours: i16,
    pub minutes: i16,
    pub seconds: i16,
    pub frames: i16,
}

#[repr(C)]
#[derive(Debug, Clone)]
pub struct CVTimeStamp {
    pub version: u32,
    pub video_time_scale: i32,
    pub video_time: i64,
    pub host_time: u64,
    pub rate_scalar: c_double,
    pub video_refresh_period: i64,
    pub smpte_time: CVSMPTETime,
    pub flags: u64,
    pub reserved: u64,
}

pub type CGResult = i32;

#[repr(C)]
pub struct CVDisplayLink(NonNull<c_void>);

type CVDisplayLinkOutputCallback = unsafe extern "C" fn(
    display_link: CVDisplayLink,
    in_now: *mut CVTimeStamp,
    in_output_time: *mut CVTimeStamp,
    flags_in: u64,
    flags_out: *mut u64,
    display_link_context: *mut c_void,
) -> CGResult;

#[link(name = "CoreFoundation", kind = "framework")]
#[link(name = "CoreVideo", kind = "framework")]
#[allow(improper_ctypes)]
unsafe extern "C" {
    fn CGMainDisplayID() -> u32;
    fn CGWarpMouseCursorPosition(point: NSPoint) -> CGResult;
    fn CGGetDisplaysWithRect(
        rect: NSRect,
        maxDisplays: u32,
        displays: *mut u32,
        matchingDisplayCount: *mut u32,
    ) -> CGResult;

    fn CVDisplayLinkCreateWithActiveCGDisplays(display_link_out: *mut *mut c_void) -> CGResult;

    fn CVDisplayLinkSetOutputCallback(
        display_link: *mut c_void,
        callback: CVDisplayLinkOutputCallback,
        user_info: *mut c_void,
    ) -> CGResult;

    fn CVDisplayLinkSetCurrentCGDisplay(display_link: *mut c_void, display_id: u32) -> CGResult;

    fn CVDisplayLinkStart(display_link: *mut c_void) -> CGResult;
    fn _CVDisplayLinkStop(display_link: *mut c_void) -> CGResult;
    fn CVDisplayLinkRelease(display_link: *mut c_void);
    fn _CVDisplayLinkRetain(display_link: *mut c_void) -> *mut c_void;
}
