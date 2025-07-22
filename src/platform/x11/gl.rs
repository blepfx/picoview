use super::connection::Connection;
use super::util::cstr;
use crate::{Error, GlConfig, GlVersion};
use std::collections::HashSet;
use std::ffi::{CStr, c_void};
use std::fmt::Debug;
use std::os::raw::{c_int, c_ulong};
use std::ptr::{null, null_mut};
use std::sync::Arc;
use x11_dl::glx;
use x11_dl::xlib;

const GLX_FRAMEBUFFER_SRGB_CAPABLE_ARB: i32 = 0x20B2;
const CONTEXT_ES2_PROFILE_BIT_EXT: i32 = 0x00000004;

type GlXSwapIntervalEXT = unsafe extern "C" fn(dpy: *mut xlib::Display, drawable: glx::GLXDrawable, interval: i32);
type GlXCreateContextAttribsARB = unsafe extern "C" fn(
    dpy: *mut xlib::Display,
    fbc: glx::GLXFBConfig,
    share_context: glx::GLXContext,
    direct: xlib::Bool,
    attribs: *const c_int,
) -> glx::GLXContext;

pub struct GlContext {
    window: c_ulong,
    connection: Arc<Connection>,
    context: glx::GLXContext,
    lib_glx: glx::Glx,
}

impl GlContext {
    #[allow(non_snake_case)]
    pub unsafe fn new(connection: Arc<Connection>, window: c_ulong, config: GlConfig) -> Result<GlContext, Error> {
        unsafe {
            let lib_glx = glx::Glx::open().map_err(|e| Error::OpenGlError(e.to_string()))?;

            let (version, extensions) = {
                let (mut major, mut minor) = (0, 0);
                if (lib_glx.glXQueryVersion)(connection.display(), &mut major, &mut minor) == 0 {
                    return Err(Error::OpenGlError("glXQueryVersion failed".into()));
                }

                let extensions = (lib_glx.glXGetClientString)(connection.display(), glx::GLX_EXTENSIONS as i32);
                let extensions = if extensions.is_null() {
                    HashSet::new()
                } else {
                    if let Ok(extensions) = CStr::from_ptr(extensions).to_str() {
                        extensions.split(' ').collect::<HashSet<_>>()
                    } else {
                        HashSet::new()
                    }
                };

                check_error(&connection)?;

                ((major as u8, minor as u8), extensions)
            };

            let ext_es_support = extensions.contains("GLX_EXT_create_context_es2_profile")
                || extensions.contains("GLX_EXT_create_context_es_profile");
            let ext_swap_control = extensions.contains("GLX_EXT_swap_control")
                || extensions.contains("GLX_SGI_swap_control")
                || extensions.contains("GLX_MESA_swap_control");
            let ext_multisample = version >= (1, 4) || extensions.contains("GLX_ARB_multisample");
            let ext_framebuffer_srgb =
                extensions.contains("GLX_ARB_framebuffer_sRGB") || extensions.contains("GLX_EXT_framebuffer_sRGB");

            let (fb_config, fb_visual) = {
                let (red, green, blue, alpha, depth, stencil) = config.format.as_rgbads();

                let mut fb_attribs = vec![
                    glx::GLX_X_RENDERABLE,
                    1,
                    glx::GLX_X_VISUAL_TYPE,
                    glx::GLX_TRUE_COLOR,
                    glx::GLX_DRAWABLE_TYPE,
                    glx::GLX_WINDOW_BIT,
                    glx::GLX_RENDER_TYPE,
                    glx::GLX_RGBA_BIT,
                    glx::GLX_RED_SIZE,
                    red as _,
                    glx::GLX_GREEN_SIZE,
                    green as _,
                    glx::GLX_BLUE_SIZE,
                    blue as _,
                    glx::GLX_ALPHA_SIZE,
                    alpha as _,
                    glx::GLX_DEPTH_SIZE,
                    depth as _,
                    glx::GLX_STENCIL_SIZE,
                    stencil as _,
                    glx::GLX_DOUBLEBUFFER,
                    config.double_buffer as i32,
                ];

                if ext_framebuffer_srgb && config.srgb {
                    fb_attribs.extend_from_slice(&[GLX_FRAMEBUFFER_SRGB_CAPABLE_ARB, 1]);
                }

                if ext_multisample && config.msaa_count > 0 {
                    fb_attribs.extend_from_slice(&[
                        glx::GLX_SAMPLE_BUFFERS,
                        1,
                        glx::GLX_SAMPLES,
                        config.msaa_count as i32,
                    ]);
                }

                if config.debug {
                    fb_attribs
                        .extend_from_slice(&[glx::arb::GLX_CONTEXT_FLAGS_ARB, glx::arb::GLX_CONTEXT_DEBUG_BIT_ARB]);
                }

                fb_attribs.push(0);

                let mut n_configs = 0;
                let fb_config = (lib_glx.glXChooseFBConfig)(
                    connection.display(),
                    connection.default_screen_index(),
                    fb_attribs.as_ptr(),
                    &mut n_configs,
                );

                if n_configs <= 0 || fb_config.is_null() {
                    return Err(Error::OpenGlError("no matching config".into()));
                }

                let fb_config = *fb_config;
                let fb_visual = (lib_glx.glXGetVisualFromFBConfig)(connection.display(), fb_config);
                if fb_visual.is_null() {
                    return Err(Error::OpenGlError("no matching config".into()));
                }

                check_error(&connection)?;

                (fb_config, fb_visual)
            };

            let glXCreateContextAttribsARB =
                (lib_glx.glXGetProcAddress)(cstr!("glXCreateContextAttribsARB").as_ptr() as *const _)
                    .map(|addr| std::mem::transmute::<_, GlXCreateContextAttribsARB>(addr));

            let context = if let Some(glXCreateContextAttribsARB) = glXCreateContextAttribsARB {
                #[rustfmt::skip]
            let ctx_attribs = match config.version {
                GlVersion::Core(major, minor) => [
                    glx::arb::GLX_CONTEXT_MAJOR_VERSION_ARB, major as i32,
                    glx::arb::GLX_CONTEXT_MINOR_VERSION_ARB, minor as i32,
                    glx::arb::GLX_CONTEXT_PROFILE_MASK_ARB, glx::arb::GLX_CONTEXT_CORE_PROFILE_BIT_ARB,
                    0,
                ],
                GlVersion::Compat(major, minor) => [
                    glx::arb::GLX_CONTEXT_MAJOR_VERSION_ARB, major as i32,
                    glx::arb::GLX_CONTEXT_MINOR_VERSION_ARB, minor as i32,
                    glx::arb::GLX_CONTEXT_PROFILE_MASK_ARB, glx::arb::GLX_CONTEXT_COMPATIBILITY_PROFILE_BIT_ARB,
                    0,
                ],
                GlVersion::ES(major, minor) if ext_es_support => [
                    glx::arb::GLX_CONTEXT_MAJOR_VERSION_ARB, major as i32,
                    glx::arb::GLX_CONTEXT_MINOR_VERSION_ARB, minor as i32,
                    glx::arb::GLX_CONTEXT_PROFILE_MASK_ARB, CONTEXT_ES2_PROFILE_BIT_EXT,
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
                (lib_glx.glXCreateContext)(connection.display(), fb_visual, std::ptr::null_mut(), 1)
            };

            check_error(&connection)?;

            if context.is_null() {
                return Err(Error::OpenGlError("GLX context creation error".into()));
            }

            if ext_swap_control {
                let glXSwapIntervalEXT = (lib_glx.glXGetProcAddress)(cstr!("glXSwapIntervalEXT").as_ptr() as *const _)
                    .map(|addr| std::mem::transmute::<_, GlXSwapIntervalEXT>(addr));

                if let Some(glXSwapIntervalEXT) = glXSwapIntervalEXT {
                    if (lib_glx.glXMakeCurrent)(connection.display(), window, context) != 0 {
                        glXSwapIntervalEXT(connection.display(), window, 0);
                        (lib_glx.glXMakeCurrent)(connection.display(), 0, null_mut());
                    }
                }
            }

            check_error(&connection)?;

            Ok(GlContext {
                connection,
                window,
                context,
                lib_glx,
            })
        }
    }

    pub unsafe fn set_current(&self, current: bool) -> bool {
        unsafe {
            let result = {
                if current {
                    (self.lib_glx.glXMakeCurrent)(self.connection.display(), self.window, self.context)
                } else {
                    (self.lib_glx.glXMakeCurrent)(self.connection.display(), 0, std::ptr::null_mut())
                }
            };

            result != 0
        }
    }
}

impl crate::GlContext for GlContext {
    fn get_proc_address(&self, symbol: &CStr) -> *const c_void {
        unsafe {
            (self.lib_glx.glXGetProcAddress)(symbol.as_ptr() as *const u8)
                .map(|x| x as *const c_void)
                .unwrap_or(null())
        }
    }

    fn swap_buffers(&self) {
        unsafe {
            (self.lib_glx.glXSwapBuffers)(self.connection.display(), self.window);
        }
    }
}

impl Debug for GlContext {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("GlContext")
            .field("window", &self.window)
            .field("context", &self.context)
            .finish()
    }
}

impl Drop for GlContext {
    fn drop(&mut self) {
        unsafe {
            (self.lib_glx.glXMakeCurrent)(self.connection.display(), 0, std::ptr::null_mut());
            (self.lib_glx.glXDestroyContext)(self.connection.display(), self.context);
        }
    }
}

fn check_error(conn: &Connection) -> Result<(), Error> {
    match conn.last_error() {
        Some(str) => Err(Error::OpenGlError(str)),
        None => Ok(()),
    }
}
