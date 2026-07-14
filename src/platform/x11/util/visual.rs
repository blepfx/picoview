use super::Connection;
use std::mem::zeroed;
use std::ptr::null_mut;
use x11::glx::{GLXFBConfig, glXGetVisualFromFBConfig};
use x11::xlib::*;
use x11::xrender::{XRenderFindVisualFormat, XRenderPictFormat};

/// A visual config is used for creating a colormap/window
/// (and optionally an OpenGl context)
pub struct VisualConfig {
    fb_config: GLXFBConfig,
    info: XVisualInfo,
}

impl VisualConfig {
    /// Try to find a true-color visual with the given depth, if available.
    pub fn try_new_true_color(conn: &Connection, depth: u8) -> Option<Self> {
        let info = unsafe {
            let mut info = XVisualInfo { ..zeroed() };
            match XMatchVisualInfo(
                conn.as_raw(),
                XDefaultScreen(conn.as_raw()),
                depth as _,
                TrueColor,
                &mut info,
            ) {
                0 => return None,
                _ => info,
            }
        };

        Some(Self {
            info,
            fb_config: null_mut(),
        })
    }

    /// Create a visual config from a GLX framebuffer config.
    ///
    /// # Safety
    /// - The `fb_config` must be a valid GLX framebuffer config obtained from
    ///   the given connection.
    pub unsafe fn from_glx(conn: &Connection, fb_config: GLXFBConfig) -> Option<Self> {
        unsafe {
            let info_ptr = glXGetVisualFromFBConfig(conn.as_raw(), fb_config);
            if info_ptr.is_null() {
                return None;
            }

            let info = *info_ptr;
            XFree(info_ptr as *mut _);

            Some(Self { fb_config, info })
        }
    }

    /// Underlying [`XVisualInfo`].
    pub fn info(&self) -> &XVisualInfo {
        &self.info
    }

    /// GLX framebuffer config if present. Can be `null`.
    pub fn glx_config(&self) -> GLXFBConfig {
        self.fb_config
    }

    /// Get the XRender picture format for this visual.
    pub fn xrender_format(&self, conn: &Connection) -> Option<XRenderPictFormat> {
        unsafe {
            let format = XRenderFindVisualFormat(conn.as_raw(), self.info.visual);
            if format.is_null() {
                return None;
            }

            Some(*format)
        }
    }
}
