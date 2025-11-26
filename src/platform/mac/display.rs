use objc2::MainThreadMarker;
use objc2_core_foundation::{
    CFRetained, CFRunLoop, CFRunLoopSource, CFRunLoopSourceContext, kCFRunLoopCommonModes,
};
use objc2_foundation::NSPoint;
use std::ffi::c_void;
use std::ptr::null_mut;
use std::rc::Rc;

use crate::Error;

extern "C" fn callback(
    _display_link: *mut c_void,
    _in_now: *mut c_void,
    _in_output_time: *mut c_void,
    _flags_in: u64,
    _flags_out: *mut u64,
    display_link_context: *mut c_void,
) -> CVResult {
    unsafe {
        let source = &*(display_link_context as *const _ as *const CFRunLoopSource);
        source.signal();

        if let Some(run_loop) = CFRunLoop::main() {
            run_loop.wake_up();
        }
    }

    0
}

extern "C-unwind" fn retain(info: *const c_void) -> *const c_void {
    unsafe { Rc::increment_strong_count(info as *const DisplayState) };
    info
}

extern "C-unwind" fn release(info: *const c_void) {
    unsafe { Rc::decrement_strong_count(info as *const DisplayState) };
}

extern "C-unwind" fn perform(info: *mut c_void) {
    let state = unsafe { &*(info as *const DisplayState) };
    (state.runner)(state.display_id);
}

struct DisplayState {
    runner: Rc<dyn Fn(u32)>,
    display_id: u32,
}

pub struct DisplayLink {
    link: *mut c_void,
    source: CFRetained<CFRunLoopSource>,
}

impl DisplayLink {
    pub fn new(runner: Rc<dyn Fn(u32)>, display_id: u32) -> Result<DisplayLink, Error> {
        let state = Rc::new(DisplayState { runner, display_id });

        let mut context = CFRunLoopSourceContext {
            version: 0,
            info: Rc::into_raw(state) as *mut c_void,
            retain: Some(retain),
            release: Some(release),
            copyDescription: None,
            equal: None,
            hash: None,
            schedule: None,
            cancel: None,
            perform: Some(perform),
        };

        unsafe {
            CGMainDisplayID();

            let source = CFRunLoopSource::new(None, 0, &mut context)
                .ok_or_else(|| Error::PlatformError("CFRunLoopSource::new".to_owned()))?;
            let run_loop = CFRunLoop::main()
                .ok_or_else(|| Error::PlatformError("CFRunLoop::main".to_owned()))?;
            run_loop.add_source(Some(&source), kCFRunLoopCommonModes);

            let mut link = null_mut();
            let result = CVDisplayLinkCreateWithCGDisplay(display_id, &mut link);
            if result != 0 || link.is_null() {
                return Err(Error::PlatformError(format!(
                    "CVDisplayLinkCreateWithCGDisplay: {}",
                    result
                )));
            }

            let result =
                CVDisplayLinkSetOutputCallback(link, callback, &*source as *const _ as *mut c_void);
            if result != 0 {
                return Err(Error::PlatformError(format!(
                    "CVDisplayLinkSetOutputCallback: {}",
                    result
                )));
            }

            let result = CVDisplayLinkStart(link);
            if result != 0 {
                return Err(Error::PlatformError(format!(
                    "CVDisplayLinkStart: {}",
                    result
                )));
            }

            Ok(DisplayLink { link, source })
        }
    }
}

impl Drop for DisplayLink {
    fn drop(&mut self) {
        unsafe {
            CVDisplayLinkStop(self.link);
            CVDisplayLinkRelease(self.link);

            self.source.invalidate();
        }
    }
}

#[inline]
pub fn warp_mouse_cursor_position(point: NSPoint, _main_thread: MainThreadMarker) -> bool {
    unsafe { CGWarpMouseCursorPosition(point) == 0 }
}

type CVResult = i32;
type CVDisplayLinkOutputCallback = unsafe extern "C" fn(
    display_link: *mut c_void,
    in_now: *mut c_void,
    in_output_time: *mut c_void,
    flags_in: u64,
    flags_out: *mut u64,
    display_link_context: *mut c_void,
) -> CVResult;

//TODO: replace this with objc2?
#[allow(clippy::duplicated_attributes)] // ?
#[link(name = "CoreFoundation", kind = "framework")]
#[link(name = "CoreVideo", kind = "framework")]
unsafe extern "C" {
    pub fn CGMainDisplayID() -> u32;
    fn CGWarpMouseCursorPosition(point: NSPoint) -> CVResult;

    fn CVDisplayLinkCreateWithCGDisplay(
        displayID: u32,
        display_link_out: *mut *mut c_void,
    ) -> CVResult;

    fn CVDisplayLinkSetOutputCallback(
        display_link: *mut c_void,
        callback: CVDisplayLinkOutputCallback,
        user_info: *mut c_void,
    ) -> CVResult;

    fn CVDisplayLinkStart(display_link: *mut c_void) -> CVResult;
    fn CVDisplayLinkStop(display_link: *mut c_void) -> CVResult;
    fn CVDisplayLinkRelease(display_link: *mut c_void);
}
