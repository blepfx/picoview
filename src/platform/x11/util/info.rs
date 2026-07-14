use super::Connection;
use std::ffi::CStr;
use std::mem::zeroed;
use std::ptr::null_mut;
use std::str::FromStr;
use x11::xlib::*;
use x11::xrandr::*;

/// Get the DPI scaling factor from X resources, if available.
pub fn query_scale_dpi(conn: &Connection) -> Option<f64> {
    unsafe {
        let rms = XResourceManagerString(conn.as_raw());
        if rms.is_null() {
            return None;
        }

        let db = XrmGetStringDatabase(rms);
        if db.is_null() {
            return None;
        }

        let mut value = XrmValue { ..zeroed() };
        let result = XrmGetResource(
            db,
            c"Xft.dpi".as_ptr(),
            c"Xft.Dpi".as_ptr(),
            &mut null_mut(),
            &mut value,
        );

        if result == 0 || value.addr.is_null() {
            XrmDestroyDatabase(db);
            return None;
        }

        let string = CStr::from_ptr(value.addr).to_string_lossy();
        let Ok(value) = f64::from_str(&string) else {
            XrmDestroyDatabase(db);
            return None;
        };

        XrmDestroyDatabase(db);
        Some(value)
    }
}

/// Get the current refresh rate of the default screen by querying the
/// XRandR extension, if available.
pub fn query_refresh_rate(conn: &Connection) -> Option<f64> {
    unsafe {
        let has_randr = XRRQueryExtension(conn.as_raw(), &mut 0, &mut 0);
        if has_randr == 0 {
            return None;
        }

        let resources =
            XRRGetScreenResourcesCurrent(conn.as_raw(), XDefaultRootWindow(conn.as_raw()));
        if resources.is_null() {
            return None;
        }

        let mut max_rate: Option<f64> = None;
        for crtc in 0..(*resources).ncrtc {
            let crtc = (*resources).crtcs.add(crtc as usize).read();
            let crtc_info = XRRGetCrtcInfo(conn.as_raw(), resources, crtc);

            if !crtc_info.is_null() && (*crtc_info).mode != 0 {
                for mode in 0..(*resources).nmode {
                    let mode = (*resources).modes.add(mode as usize);

                    if (*mode).id == (*crtc_info).mode {
                        let rate = (*mode).dotClock as f64
                            / ((*mode).hTotal as f64 * (*mode).vTotal as f64);

                        //xvfb reports it as NaN
                        if rate.is_finite() {
                            max_rate = max_rate.map(|prev| prev.max(rate)).or(Some(rate));
                        }
                    }
                }
            }

            XRRFreeCrtcInfo(crtc_info);
        }

        XRRFreeScreenResources(resources);

        max_rate
    }
}
