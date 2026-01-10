use crate::Error;
use objc2::rc::Retained;
use objc2_core_foundation::{
    CFRetained, CFRunLoop, CFRunLoopSource, CFRunLoopSourceContext, kCFRunLoopCommonModes,
};
use objc2_core_video::{CVDisplayLink, CVOptionFlags, CVReturn, CVTimeStamp};
use std::ffi::c_void;
use std::ptr::{NonNull, null_mut};
use std::rc::Rc;

extern "C-unwind" fn callback(
    _display_link: NonNull<CVDisplayLink>,
    _in_now: NonNull<CVTimeStamp>,
    _in_output_time: NonNull<CVTimeStamp>,
    _flags_in: CVOptionFlags,
    _flags_out: NonNull<CVOptionFlags>,
    display_link_context: *mut c_void,
) -> CVReturn {
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
    (state.runner)();
}

struct DisplayState {
    runner: Box<dyn Fn()>,
}

pub struct DisplayLink {
    link: Retained<CVDisplayLink>,
    source: CFRetained<CFRunLoopSource>,
}

impl DisplayLink {
    #[allow(deprecated)] // smh
    pub fn new(runner: Box<dyn Fn()>) -> Result<DisplayLink, Error> {
        unsafe {
            let state = Rc::new(DisplayState { runner });
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

            let source = CFRunLoopSource::new(None, 0, &mut context)
                .ok_or_else(|| Error::PlatformError("CFRunLoopSource::new".to_owned()))?;
            let run_loop = CFRunLoop::main()
                .ok_or_else(|| Error::PlatformError("CFRunLoop::main".to_owned()))?;
            run_loop.add_source(Some(&source), kCFRunLoopCommonModes);

            let mut link = null_mut();
            let result =
                CVDisplayLink::create_with_active_cg_displays(NonNull::from_mut(&mut link));
            if result != 0 || link.is_null() {
                return Err(Error::PlatformError(format!(
                    "CVDisplayLink::create_with_active_cg_displays: {}",
                    result
                )));
            }

            let link = Retained::from_raw(link).unwrap_unchecked();

            let result =
                link.set_output_callback(Some(callback), &*source as *const _ as *mut c_void);

            if result != 0 {
                return Err(Error::PlatformError(format!(
                    "CVDisplayLink::set_output_callback: {}",
                    result
                )));
            }

            let result = link.start();
            if result != 0 {
                return Err(Error::PlatformError(format!(
                    "CVDisplayLink::start: {}",
                    result
                )));
            }

            Ok(DisplayLink { link, source })
        }
    }
}

impl Drop for DisplayLink {
    #[allow(deprecated)]
    fn drop(&mut self) {
        self.link.stop();
        self.source.invalidate();
    }
}
