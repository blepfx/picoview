use crate::platform::win::util::Win32Error;
use crate::platform::win::util::window::create_window;
use crate::{GlConfig, GlVersion, OpenGlError};
use std::collections::HashSet;
use std::ffi::{CStr, c_char};
use std::mem::{size_of, zeroed};
use std::ptr::null_mut;
use std::sync::OnceLock;
use windows_sys::Win32::Graphics::Gdi::{GetDC, HDC, ReleaseDC};
use windows_sys::Win32::Graphics::OpenGL::{
    ChoosePixelFormat, DescribePixelFormat, HGLRC, PFD_DEPTH_DONTCARE, PFD_DOUBLEBUFFER,
    PFD_DOUBLEBUFFER_DONTCARE, PFD_DRAW_TO_WINDOW, PFD_MAIN_PLANE, PFD_SUPPORT_OPENGL,
    PFD_TYPE_RGBA, PIXELFORMATDESCRIPTOR, SetPixelFormat, wglCreateContext, wglDeleteContext,
    wglGetProcAddress, wglMakeCurrent,
};

pub const WGL_CONTEXT_MAJOR_VERSION_ARB: i32 = 0x2091;
pub const WGL_CONTEXT_MINOR_VERSION_ARB: i32 = 0x2092;
pub const WGL_CONTEXT_PROFILE_MASK_ARB: i32 = 0x9126;
pub const WGL_CONTEXT_FLAGS_ARB: i32 = 0x2094;

pub const WGL_CONTEXT_DEBUG_BIT_ARB: i32 = 0x00000001;
pub const WGL_CONTEXT_CORE_PROFILE_BIT_ARB: i32 = 0x00000001;
pub const WGL_CONTEXT_COMPATIBILITY_PROFILE_BIT_ARB: i32 = 0x00000002;
pub const WGL_CONTEXT_ES2_PROFILE_BIT_EXT: i32 = 0x00000004;

pub const WGL_DRAW_TO_WINDOW_ARB: i32 = 0x2001;
pub const WGL_ACCELERATION_ARB: i32 = 0x2003;
pub const WGL_SUPPORT_OPENGL_ARB: i32 = 0x2010;
pub const WGL_DOUBLE_BUFFER_ARB: i32 = 0x2011;
pub const WGL_PIXEL_TYPE_ARB: i32 = 0x2013;
pub const WGL_RED_BITS_ARB: i32 = 0x2015;
pub const WGL_GREEN_BITS_ARB: i32 = 0x2017;
pub const WGL_BLUE_BITS_ARB: i32 = 0x2019;
pub const WGL_ALPHA_BITS_ARB: i32 = 0x201B;
pub const WGL_DEPTH_BITS_ARB: i32 = 0x2022;
pub const WGL_STENCIL_BITS_ARB: i32 = 0x2023;
pub const WGL_FULL_ACCELERATION_ARB: i32 = 0x2027;
pub const WGL_GENERIC_ACCELERATION_ARB: i32 = 0x2026;
pub const WGL_TYPE_RGBA_ARB: i32 = 0x202B;
pub const WGL_SAMPLE_BUFFERS_ARB: i32 = 0x2041;
pub const WGL_SAMPLES_ARB: i32 = 0x2042;
pub const WGL_FRAMEBUFFER_SRGB_CAPABLE_ARB: i32 = 0x20A9;

pub type WglCreateContextAttribsARB = unsafe extern "system" fn(HDC, HGLRC, *const i32) -> HGLRC;
pub type WglChoosePixelFormatARB =
    unsafe extern "system" fn(HDC, *const i32, *const f32, u32, *mut i32, *mut u32) -> i32;
pub type WglSwapIntervalEXT = unsafe extern "system" fn(i32) -> i32;
pub type WglGetExtensionsStringEXT = unsafe extern "system" fn() -> *const c_char;
pub type WglGetExtensionsStringARB = unsafe extern "system" fn(HDC) -> *const c_char;

/// Set the swap interval for the current OpenGL context, if supported.
///
/// # Safety
/// - The `hdc` and `hglrc` must be valid device context and OpenGL context
///   handles for the lifetime of the call.
pub unsafe fn try_set_swap_interval(hdc: HDC, hglrc: HGLRC, interval: i32) {
    unsafe {
        let wgl = WglExtensions::get();
        let Some(swap_interval) = wgl.swap_interval else {
            return;
        };

        wglMakeCurrent(hdc, hglrc);
        swap_interval(interval);
        wglMakeCurrent(hdc, null_mut());
    }
}

/// Create a fallback OpenGL context using the legacy `wglCreateContext`
/// function, if available.
///
/// # Safety
/// - The `hdc` must be a valid device context handle for the lifetime of the
///   call.
pub unsafe fn create_context_fallback(hdc: HDC) -> Result<HGLRC, OpenGlError> {
    unsafe {
        let ptr = wglCreateContext(hdc);
        if ptr.is_null() {
            Err(OpenGlError::VersionUnsupported)
        } else {
            Ok(ptr)
        }
    }
}

/// Create an OpenGL context using the `wglCreateContextAttribsARB` function, if
/// available.
///
/// # Safety
/// - The `hdc` must be a valid device context handle for the lifetime of the
///   call.
pub unsafe fn create_context_arb(hdc: HDC, config: &GlConfig) -> Result<HGLRC, OpenGlError> {
    unsafe {
        let wgl = WglExtensions::get();

        let create_context_attribs = wgl
            .create_context_attribs
            .ok_or(OpenGlError::VersionUnsupported)?;

        let ctx_attribs = {
            let mut ctx_attribs = vec![];

            if config.debug {
                ctx_attribs.extend_from_slice(&[WGL_CONTEXT_FLAGS_ARB, WGL_CONTEXT_DEBUG_BIT_ARB]);
            }

            match config.version {
                GlVersion::Core(major, minor) => {
                    ctx_attribs.extend_from_slice(&[
                        WGL_CONTEXT_MAJOR_VERSION_ARB,
                        major as i32,
                        WGL_CONTEXT_MINOR_VERSION_ARB,
                        minor as i32,
                        WGL_CONTEXT_PROFILE_MASK_ARB,
                        WGL_CONTEXT_CORE_PROFILE_BIT_ARB,
                    ]);
                }
                GlVersion::Compat(major, minor) => {
                    ctx_attribs.extend_from_slice(&[
                        WGL_CONTEXT_MAJOR_VERSION_ARB,
                        major as i32,
                        WGL_CONTEXT_MINOR_VERSION_ARB,
                        minor as i32,
                        WGL_CONTEXT_PROFILE_MASK_ARB,
                        WGL_CONTEXT_COMPATIBILITY_PROFILE_BIT_ARB,
                    ]);
                }
                GlVersion::ES(major, minor) => {
                    if !wgl.ext_context_es_profile {
                        return Err(OpenGlError::VersionUnsupported);
                    }

                    ctx_attribs.extend_from_slice(&[
                        WGL_CONTEXT_MAJOR_VERSION_ARB,
                        major as i32,
                        WGL_CONTEXT_MINOR_VERSION_ARB,
                        minor as i32,
                        WGL_CONTEXT_PROFILE_MASK_ARB,
                        WGL_CONTEXT_ES2_PROFILE_BIT_EXT,
                    ]);
                }
            }

            ctx_attribs.push(0);
            ctx_attribs
        };

        let context = (create_context_attribs)(hdc, null_mut(), ctx_attribs.as_ptr());
        if context.is_null() {
            Err(OpenGlError::VersionUnsupported)
        } else {
            Ok(context)
        }
    }
}

/// Create a pixel format for the given device context using the legacy
/// `ChoosePixelFormat` function, if available.
///
/// # Safety
/// - The `hdc` must be a valid device context handle for the lifetime of the
///   call.
pub unsafe fn create_pixel_format_fallback(
    hdc: HDC,
    config: &crate::GlConfig,
) -> Result<(i32, PIXELFORMATDESCRIPTOR), OpenGlError> {
    unsafe {
        let (red, green, blue, alpha, depth, stencil) = config.format.as_rgbads();

        let pfd = PIXELFORMATDESCRIPTOR {
            nSize: size_of::<PIXELFORMATDESCRIPTOR>() as u16,
            nVersion: 1,
            dwFlags: PFD_DRAW_TO_WINDOW
                | PFD_SUPPORT_OPENGL
                | (PFD_DOUBLEBUFFER * config.double_buffer as u32),
            iPixelType: PFD_TYPE_RGBA,
            cColorBits: (red + green + blue) as _,
            cAlphaBits: alpha as _,
            cDepthBits: depth as _,
            cStencilBits: stencil as _,
            iLayerType: PFD_MAIN_PLANE as _,
            ..zeroed()
        };

        let pixel_format = ChoosePixelFormat(hdc, &pfd);
        if pixel_format == 0 {
            Err(OpenGlError::FormatUnsupported)
        } else {
            Ok((pixel_format, pfd))
        }
    }
}

/// Create a pixel format for the given device context using the
/// `wglChoosePixelFormatARB` function, if available.
///
/// # Safety
/// - The `hdc` must be a valid device context handle for the lifetime of the
///   call.
pub unsafe fn create_pixel_format_arb(
    hdc: HDC,
    config: &crate::GlConfig,
) -> Result<(i32, PIXELFORMATDESCRIPTOR), OpenGlError> {
    unsafe {
        let wgl = WglExtensions::get();
        let choose_pixel_format = wgl
            .choose_pixel_format
            .ok_or(OpenGlError::FormatUnsupported)?;

        let pixel_format_attribs = {
            let (red, green, blue, alpha, depth, stencil) = config.format.as_rgbads();

            #[rustfmt::skip]
            let mut pixel_format_attribs = vec![
                WGL_DRAW_TO_WINDOW_ARB, 1,
                WGL_SUPPORT_OPENGL_ARB, 1,
                WGL_DOUBLE_BUFFER_ARB, config.double_buffer as i32,
                WGL_PIXEL_TYPE_ARB, WGL_TYPE_RGBA_ARB,
                WGL_RED_BITS_ARB, red as _,
                WGL_GREEN_BITS_ARB, green as _,
                WGL_BLUE_BITS_ARB, blue as _,
                WGL_ALPHA_BITS_ARB, alpha as _,
                WGL_DEPTH_BITS_ARB, depth as _,
                WGL_STENCIL_BITS_ARB, stencil as _,
            ];

            if config.force_hardware {
                pixel_format_attribs
                    .extend_from_slice(&[WGL_ACCELERATION_ARB, WGL_FULL_ACCELERATION_ARB]);
            } else {
                pixel_format_attribs
                    .extend_from_slice(&[WGL_ACCELERATION_ARB, WGL_GENERIC_ACCELERATION_ARB]);
            }

            if wgl.ext_multisample {
                pixel_format_attribs.extend_from_slice(&[
                    WGL_SAMPLE_BUFFERS_ARB,
                    (config.msaa_count != 0) as i32,
                    WGL_SAMPLES_ARB,
                    config.msaa_count as i32,
                ]);
            }

            if wgl.ext_framebuffer_srgb {
                pixel_format_attribs
                    .extend_from_slice(&[WGL_FRAMEBUFFER_SRGB_CAPABLE_ARB, config.srgb as i32]);
            }

            pixel_format_attribs.push(0);
            pixel_format_attribs
        };

        let mut format_id = 0;
        let mut num_formats = 0;
        (choose_pixel_format)(
            hdc,
            pixel_format_attribs.as_ptr() as *const _,
            null_mut(),
            1,
            &mut format_id,
            &mut num_formats,
        );

        if num_formats == 0 {
            return Err(OpenGlError::FormatUnsupported);
        }

        let mut pfd = zeroed();
        if DescribePixelFormat(
            hdc,
            format_id,
            size_of::<PIXELFORMATDESCRIPTOR>() as u32,
            &mut pfd,
        ) == 0
        {
            return Err(OpenGlError::FormatUnsupported);
        }

        Ok((format_id, pfd))
    }
}

/// Information about supported WGL extensions and methods, computed once and
/// cached for the lifetime of the program.
#[derive(Default)]
struct WglExtensions {
    create_context_attribs: Option<WglCreateContextAttribsARB>,
    choose_pixel_format: Option<WglChoosePixelFormatARB>,
    swap_interval: Option<WglSwapIntervalEXT>,

    ext_multisample: bool,
    ext_framebuffer_srgb: bool,
    ext_context_es_profile: bool,
}

impl WglExtensions {
    /// Get the cached WGL methods or load them if needed.
    fn get() -> &'static Self {
        static CACHE: OnceLock<WglExtensions> = OnceLock::new();
        CACHE.get_or_init(Self::create)
    }

    /// Query the WGL extensions and methods supported by the current system.
    ///
    /// This has to be done by making a temporary window, OpenGL context, and
    /// then querying the extensions, unfortunately. This is expensive but only
    /// needs to be done once per program execution.
    ///
    /// This is required if we want to have access to fancier features, like
    /// better context creation and pixel format selection.
    fn create() -> WglExtensions {
        unsafe {
            let mut result = WglExtensions::default();

            let _ = create_window::<(), Win32Error>(0, null_mut(), |hwnd| {
                let hdc = GetDC(hwnd);
                let pfd = PIXELFORMATDESCRIPTOR {
                    nSize: std::mem::size_of::<PIXELFORMATDESCRIPTOR>() as u16,
                    nVersion: 1,
                    dwFlags: PFD_DRAW_TO_WINDOW
                        | PFD_SUPPORT_OPENGL
                        | PFD_DOUBLEBUFFER_DONTCARE
                        | PFD_DEPTH_DONTCARE,
                    iPixelType: PFD_TYPE_RGBA,
                    cColorBits: 24,
                    cAlphaBits: 8,
                    cDepthBits: 24,
                    cStencilBits: 8,
                    iLayerType: PFD_MAIN_PLANE as _,
                    ..std::mem::zeroed()
                };

                let pfi = ChoosePixelFormat(hdc, &pfd);
                if pfi == 0 {
                    ReleaseDC(hwnd, hdc);
                    return Err(Win32Error::last_error().with_context("ChoosePixelFormat"));
                }

                SetPixelFormat(hdc, pfi, &pfd);

                let hglrc = wglCreateContext(hdc);
                if hglrc.is_null() {
                    ReleaseDC(hwnd, hdc);
                    return Err(Win32Error::last_error().with_context("wglCreateContext"));
                }

                wglMakeCurrent(hdc, hglrc);

                let extensions: HashSet<String> = {
                    let get_extensions_string_ext =
                        wgl_proc::<WglGetExtensionsStringEXT>(c"wglGetExtensionsStringEXT");
                    let get_extensions_string_arb =
                        wgl_proc::<WglGetExtensionsStringARB>(c"wglGetExtensionsStringARB");

                    let extension_str = get_extensions_string_ext
                        .map(|f| f())
                        .or_else(|| get_extensions_string_arb.map(|f| f(hdc)))
                        .map(|x| CStr::from_ptr(x))
                        .unwrap_or_default()
                        .to_string_lossy();

                    extension_str.split(' ').map(ToString::to_string).collect()
                };

                result = WglExtensions {
                    create_context_attribs: extensions
                        .contains("WGL_ARB_create_context")
                        .then(|| {
                            wgl_proc::<WglCreateContextAttribsARB>(c"wglCreateContextAttribsARB")
                        })
                        .flatten(),

                    choose_pixel_format: extensions
                        .contains("WGL_ARB_pixel_format")
                        .then(|| wgl_proc::<WglChoosePixelFormatARB>(c"wglChoosePixelFormatARB"))
                        .flatten(),

                    swap_interval: extensions
                        .contains("WGL_EXT_swap_control")
                        .then(|| wgl_proc::<WglSwapIntervalEXT>(c"wglSwapIntervalEXT"))
                        .flatten(),

                    ext_context_es_profile: extensions
                        .contains("WGL_EXT_create_context_es_profile")
                        || extensions.contains("WGL_EXT_create_context_es2_profile"),
                    ext_multisample: extensions.contains("WGL_ARB_multisample"),
                    ext_framebuffer_srgb: extensions.contains("WGL_ARB_framebuffer_sRGB")
                        || extensions.contains("WGL_EXT_framebuffer_sRGB"),
                };

                wglMakeCurrent(hdc, null_mut());
                wglDeleteContext(hglrc);
                ReleaseDC(hwnd, hdc);

                // we return an error because we are done and we want to cleanup the window.
                Err(Win32Error::last_error())
            });

            result
        }
    }
}

/// Load a WGL function pointer by name, returning None if it is not found.
///
/// # Safety
/// - Function signature must match the actual function signature of the WGL
///   function being loaded.
unsafe fn wgl_proc<T>(name: &CStr) -> Option<T> {
    unsafe { wglGetProcAddress(name.as_ptr() as _).map(|p| std::mem::transmute_copy(&p)) }
}
