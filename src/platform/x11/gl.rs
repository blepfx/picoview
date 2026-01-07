use super::connection::Connection;
use crate::platform::x11::util::VisualConfig;
use crate::{Error, GlConfig, GlVersion};
use std::collections::HashSet;
use std::ffi::{CStr, c_void};
use std::fmt::Debug;
use std::os::raw::{c_int, c_ulong};
use std::ptr::{null, null_mut};
use x11::glx::*;
use x11::xlib::{Bool, Display, XFree, XSync};

const GLX_FRAMEBUFFER_SRGB_CAPABLE_ARB: i32 = 0x20B2;
const CONTEXT_ES2_PROFILE_BIT_EXT: i32 = 0x00000004;

type GlXSwapIntervalEXT =
    unsafe extern "C" fn(dpy: *mut Display, drawable: GLXDrawable, interval: i32);
type GlXCreateContextAttribsARB = unsafe extern "C" fn(
    dpy: *mut Display,
    fbc: GLXFBConfig,
    share_context: GLXContext,
    direct: Bool,
    attribs: *const c_int,
) -> GLXContext;

unsafe impl Send for GlContext {}

pub struct GlContext {
    window: c_ulong,
    context: GLXContext,
}

pub struct GlContextScope<'a> {
    context: &'a GlContext,
    connection: &'a Connection,
}

impl GlContext {
    pub unsafe fn get_version_info(
        connection: &Connection,
    ) -> Result<(u8, u8, HashSet<&'static str>), Error> {
        unsafe {
            let (mut major, mut minor) = (0, 0);
            if glXQueryVersion(connection.display(), &mut major, &mut minor) == 0 {
                return Err(Error::OpenGlError("glXQueryVersion failed".into()));
            }

            let extensions = glXGetClientString(connection.display(), GLX_EXTENSIONS);
            let extensions = if extensions.is_null() {
                HashSet::new()
            } else if let Ok(extensions) = CStr::from_ptr(extensions).to_str() {
                extensions.split(' ').collect::<HashSet<_>>()
            } else {
                HashSet::new()
            };

            connection.check_error().map_err(Error::OpenGlError)?;
            Ok((major as u8, minor as u8, extensions))
        }
    }

    pub fn find_best_config(
        connection: &Connection,
        config: &GlConfig,
    ) -> Result<VisualConfig, Error> {
        unsafe {
            let (major, minor, extensions) = Self::get_version_info(connection)?;
            let (red, green, blue, alpha, depth, stencil) = config.format.as_rgbads();

            let ext_multisample =
                (major, minor) >= (1, 4) || extensions.contains("GLX_ARB_multisample");
            let ext_framebuffer_srgb = extensions.contains("GLX_ARB_framebuffer_sRGB")
                || extensions.contains("GLX_EXT_framebuffer_sRGB");

            let mut fb_attribs = vec![
                GLX_X_RENDERABLE,
                1,
                GLX_X_VISUAL_TYPE,
                GLX_TRUE_COLOR,
                GLX_DRAWABLE_TYPE,
                GLX_WINDOW_BIT,
                GLX_RENDER_TYPE,
                GLX_RGBA_BIT,
                GLX_RED_SIZE,
                red as _,
                GLX_GREEN_SIZE,
                green as _,
                GLX_BLUE_SIZE,
                blue as _,
                GLX_ALPHA_SIZE,
                alpha as _,
                GLX_DEPTH_SIZE,
                depth as _,
                GLX_STENCIL_SIZE,
                stencil as _,
                GLX_DOUBLEBUFFER,
                config.double_buffer as i32,
            ];

            if config.srgb && ext_framebuffer_srgb {
                fb_attribs.extend_from_slice(&[GLX_FRAMEBUFFER_SRGB_CAPABLE_ARB, 1]);
            }

            if config.msaa_count > 0 && ext_multisample {
                fb_attribs.extend_from_slice(&[
                    GLX_SAMPLE_BUFFERS,
                    1,
                    GLX_SAMPLES,
                    config.msaa_count as i32,
                ]);
            }

            fb_attribs.push(0);

            let mut n_configs = 0;
            let fb_config_list = glXChooseFBConfig(
                connection.display(),
                connection.screen(),
                fb_attribs.as_ptr(),
                &mut n_configs,
            );

            if n_configs <= 0 || fb_config_list.is_null() {
                return Err(Error::OpenGlError("no matching config".into()));
            }

            let fb_config = *fb_config_list;
            let fb_visual = glXGetVisualFromFBConfig(connection.display(), fb_config);
            let config = VisualConfig {
                fb_config,
                depth: (*fb_visual).depth,
                visual: (*fb_visual).visual,
            };

            XFree(fb_config_list as *mut _);
            XFree(fb_visual as *mut _);

            Ok(config)
        }
    }

    #[allow(non_snake_case)]
    pub unsafe fn new(
        connection: &Connection,
        window: c_ulong,
        config: GlConfig,
        fb_config: GLXFBConfig,
    ) -> Result<GlContext, Error> {
        if fb_config.is_null() {
            return Err(Error::OpenGlError("FBConfig is null".into()));
        }

        unsafe {
            let (_, _, extensions) = Self::get_version_info(connection)?;
            let ext_es_support = extensions.contains("GLX_EXT_create_context_es2_profile")
                || extensions.contains("GLX_EXT_create_context_es_profile");
            let ext_context = extensions.contains("GLX_ARB_create_context");
            let ext_swap_control = extensions.contains("GLX_ARB_create_context");

            let glXCreateContextAttribsARB = ext_context
                .then(|| {
                    glXGetProcAddress(c"glXCreateContextAttribsARB".as_ptr() as *const _)
                        .map(|addr| std::mem::transmute::<_, GlXCreateContextAttribsARB>(addr))
                })
                .flatten();

            let context = if let Some(glXCreateContextAttribsARB) = glXCreateContextAttribsARB {
                let ctx_attribs = match config.version {
                    GlVersion::Core(major, minor) => [
                        arb::GLX_CONTEXT_MAJOR_VERSION_ARB,
                        major as i32,
                        arb::GLX_CONTEXT_MINOR_VERSION_ARB,
                        minor as i32,
                        arb::GLX_CONTEXT_PROFILE_MASK_ARB,
                        arb::GLX_CONTEXT_CORE_PROFILE_BIT_ARB,
                        arb::GLX_CONTEXT_FLAGS_ARB,
                        arb::GLX_CONTEXT_DEBUG_BIT_ARB * config.debug as i32,
                        0,
                    ],
                    GlVersion::Compat(major, minor) => [
                        arb::GLX_CONTEXT_MAJOR_VERSION_ARB,
                        major as i32,
                        arb::GLX_CONTEXT_MINOR_VERSION_ARB,
                        minor as i32,
                        arb::GLX_CONTEXT_PROFILE_MASK_ARB,
                        arb::GLX_CONTEXT_COMPATIBILITY_PROFILE_BIT_ARB,
                        arb::GLX_CONTEXT_FLAGS_ARB,
                        arb::GLX_CONTEXT_DEBUG_BIT_ARB * config.debug as i32,
                        0,
                    ],
                    GlVersion::ES(major, minor) if ext_es_support => [
                        arb::GLX_CONTEXT_MAJOR_VERSION_ARB,
                        major as i32,
                        arb::GLX_CONTEXT_MINOR_VERSION_ARB,
                        minor as i32,
                        arb::GLX_CONTEXT_PROFILE_MASK_ARB,
                        CONTEXT_ES2_PROFILE_BIT_EXT,
                        arb::GLX_CONTEXT_FLAGS_ARB,
                        arb::GLX_CONTEXT_DEBUG_BIT_ARB * config.debug as i32,
                        0,
                    ],
                    _ => return Err(Error::OpenGlError("No ES support extension".into())),
                };

                glXCreateContextAttribsARB(
                    connection.display(),
                    fb_config,
                    std::ptr::null_mut(),
                    1,
                    ctx_attribs.as_ptr(),
                )
            } else {
                let fb_visual = glXGetVisualFromFBConfig(connection.display(), fb_config);
                if fb_visual.is_null() {
                    return Err(Error::OpenGlError(
                        "glXGetVisualFromFBConfig returned null".into(),
                    ));
                }

                let context =
                    glXCreateContext(connection.display(), fb_visual, std::ptr::null_mut(), 1);
                XFree(fb_visual as *mut _);
                context
            };

            if context.is_null() {
                return Err(Error::OpenGlError("GLX context creation error".into()));
            }

            if ext_swap_control {
                let glXSwapIntervalEXT =
                    glXGetProcAddress(c"glXSwapIntervalEXT".as_ptr() as *const _)
                        .map(|addr| std::mem::transmute::<_, GlXSwapIntervalEXT>(addr));

                if let Some(glXSwapIntervalEXT) = glXSwapIntervalEXT
                    && glXMakeCurrent(connection.display(), window, context) != 0
                {
                    glXSwapIntervalEXT(connection.display(), window, 0);
                    glXMakeCurrent(connection.display(), 0, null_mut());
                }
            }

            XSync(connection.display(), 0);
            connection.check_error().map_err(Error::OpenGlError)?;

            Ok(GlContext { window, context })
        }
    }

    pub fn scope<'a>(&'a self, connection: &'a Connection) -> GlContextScope<'a> {
        GlContextScope {
            context: self,
            connection,
        }
    }

    pub fn close(self, connection: &Connection) {
        unsafe {
            glXMakeCurrent(connection.display(), 0, std::ptr::null_mut());
            glXDestroyContext(connection.display(), self.context);
        }
    }
}

impl<'a> crate::GlContext for GlContextScope<'a> {
    fn get_proc_address(&self, symbol: &CStr) -> *const c_void {
        unsafe {
            glXGetProcAddress(symbol.as_ptr() as *const u8)
                .map(|x| x as *const c_void)
                .unwrap_or(null())
        }
    }

    fn swap_buffers(&self) {
        unsafe {
            glXSwapBuffers(self.connection.display(), self.context.window);
        }
    }

    fn make_current(&self, current: bool) -> bool {
        unsafe {
            let result = {
                if current {
                    glXMakeCurrent(
                        self.connection.display(),
                        self.context.window,
                        self.context.context,
                    )
                } else {
                    glXMakeCurrent(self.connection.display(), 0, std::ptr::null_mut())
                }
            };

            result != 0
        }
    }
}

impl<'a> Debug for GlContextScope<'a> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("GlContext")
            .field("window", &self.context.window)
            .field("context", &self.context.context)
            .finish_non_exhaustive()
    }
}
