use crate::platform::PlatformOpenGl;
use crate::platform::x11::util::{Connection, VisualConfig};
use crate::{GlConfig, GlVersion, MakeCurrentError, OpenGlError, SwapBuffersError};
use std::collections::HashSet;
use std::ffi::{CStr, c_void};
use std::os::raw::{c_int, c_ulong};
use std::ptr::{null, null_mut};
use x11::glx::*;
use x11::xlib::{Bool, Display, XDefaultScreen, XFree, XSync};
use x11::xrender::XRenderFindVisualFormat;

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

/// A GLX [`PlatformOpenGl`] implementation.
/// Used for our X11 window implementation.
pub struct GlContext {
    /// The window the context was created for.
    window: c_ulong,

    /// The GLX context itself.
    context: GLXContext,

    /// The X11 connection, used for keeping it alive (some drivers crash if the
    /// connection is closed before we destroy the GL context)
    connection: Connection,
}

impl GlContext {
    /// Checks the GLX version and returns the major and minor version, as well
    /// as a set of supported extensions.
    ///
    /// Returns `None` if the version could not be queried.
    pub unsafe fn get_version_info(
        connection: &Connection,
    ) -> Option<(u8, u8, HashSet<&'static str>)> {
        unsafe {
            let (mut major, mut minor) = (0, 0);
            if glXQueryVersion(connection.display(), &mut major, &mut minor) == 0 {
                return None;
            }

            let extensions = glXGetClientString(connection.display(), GLX_EXTENSIONS);
            let extensions = if extensions.is_null() {
                HashSet::new()
            } else if let Ok(extensions) = CStr::from_ptr(extensions).to_str() {
                extensions.split(' ').collect::<HashSet<_>>()
            } else {
                HashSet::new()
            };

            Some((major as u8, minor as u8, extensions))
        }
    }

    /// Find the best available visual config for the given OpenGL
    /// configuration.
    ///
    /// Returns `None` if no suitable config could be found.
    pub fn find_best_config(
        connection: &Connection,
        config: &GlConfig,
        transparent: bool,
    ) -> Option<VisualConfig> {
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
                XDefaultScreen(connection.display()),
                fb_attribs.as_ptr(),
                &mut n_configs,
            );

            if n_configs <= 0 || fb_config_list.is_null() {
                return None;
            }

            let mut preferred_config = 0;
            for i in 0..n_configs {
                let config = *fb_config_list.add(i as usize);
                let visual = glXGetVisualFromFBConfig(connection.display(), config);
                let format = XRenderFindVisualFormat(connection.display(), (*visual).visual);

                if transparent && (*format).direct.alphaMask > 0 {
                    preferred_config = i;
                }

                XFree(visual as *mut _);
            }

            let config = *fb_config_list.add(preferred_config as usize);
            let visual = glXGetVisualFromFBConfig(connection.display(), config);

            let config = VisualConfig {
                fb_config: config,
                depth: (*visual).depth,
                visual: (*visual).visual,
            };

            XFree(visual as *mut _);
            XFree(fb_config_list as *mut _);
            Some(config)
        }
    }

    /// Creates a GLX context for the given window and visual config.
    #[allow(non_snake_case)]
    pub unsafe fn new(
        connection: Connection,
        window: c_ulong,
        config: GlConfig,
        fb_config: GLXFBConfig,
    ) -> Result<GlContext, OpenGlError> {
        if fb_config.is_null() {
            return Err(OpenGlError("no matching config".into()));
        }

        unsafe {
            let (_, _, extensions) = Self::get_version_info(&connection)
                .ok_or_else(|| OpenGlError("call to glXQueryVersion failed".into()))?;
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

            let mut context = if let Some(glXCreateContextAttribsARB) = glXCreateContextAttribsARB {
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
                    _ => {
                        return Err(OpenGlError(
                            "requested OpenGL ES version is not supported".into(),
                        ));
                    }
                };

                glXCreateContextAttribsARB(
                    connection.display(),
                    fb_config,
                    std::ptr::null_mut(),
                    1,
                    ctx_attribs.as_ptr(),
                )
            } else {
                null_mut()
            };

            if context.is_null() {
                let fb_visual = glXGetVisualFromFBConfig(connection.display(), fb_config);
                if fb_visual.is_null() {
                    return Err(OpenGlError("glXGetVisualFromFBConfig returned null".into()));
                }

                context = glXCreateContext(connection.display(), fb_visual, null_mut(), 1);
                XFree(fb_visual as *mut _);
            };

            if context.is_null() {
                return Err(OpenGlError("glXCreateContext returned null".into()));
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
            connection.last_error().map_err(OpenGlError)?;

            Ok(GlContext {
                window,
                context,
                connection,
            })
        }
    }
}

impl Drop for GlContext {
    fn drop(&mut self) {
        unsafe {
            glXMakeCurrent(self.connection.display(), 0, std::ptr::null_mut());
            glXDestroyContext(self.connection.display(), self.context);
        }
    }
}

impl PlatformOpenGl for GlContext {
    fn get_proc_address(&self, symbol: &CStr) -> *const c_void {
        unsafe {
            glXGetProcAddress(symbol.as_ptr() as *const u8)
                .map(|x| x as *const c_void)
                .unwrap_or(null())
        }
    }

    fn swap_buffers(&self) -> Result<(), SwapBuffersError> {
        unsafe {
            glXSwapBuffers(self.connection.display(), self.window);
            Ok(())
        }
    }

    fn make_current(&self, current: bool) -> Result<(), MakeCurrentError> {
        unsafe {
            let context = glXGetCurrentContext();
            if (current && context == self.context) || (!current && context != self.context) {
                // already in the requested state, we okay!
                return Ok(());
            }

            let result = {
                if current {
                    glXMakeCurrent(self.connection.display(), self.window, self.context)
                } else {
                    glXMakeCurrent(self.connection.display(), 0, std::ptr::null_mut())
                }
            };

            if result == 0 {
                Err(MakeCurrentError)
            } else {
                Ok(())
            }
        }
    }
}
