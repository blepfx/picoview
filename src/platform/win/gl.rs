use super::util::{generate_guid, hinstance, to_widestring};
use crate::Error;
use std::{
    collections::HashSet,
    ffi::{CStr, c_char, c_void},
    fmt::Debug,
    mem::{size_of, zeroed},
    ptr::{null, null_mut},
    sync::OnceLock,
};
use windows_sys::{
    Win32::{
        Foundation::{FreeLibrary, HMODULE, HWND, PROC},
        Graphics::{
            Gdi::{GetDC, HDC, ReleaseDC},
            OpenGL::{
                ChoosePixelFormat, DescribePixelFormat, HGLRC, PFD_DOUBLEBUFFER,
                PFD_DRAW_TO_WINDOW, PFD_MAIN_PLANE, PFD_SUPPORT_OPENGL, PFD_TYPE_RGBA,
                PIXELFORMATDESCRIPTOR, SetPixelFormat, SwapBuffers, wglCreateContext,
                wglDeleteContext, wglGetProcAddress, wglMakeCurrent,
            },
        },
        System::LibraryLoader::{GetProcAddress, LoadLibraryA},
        UI::WindowsAndMessaging::{
            CS_OWNDC, CW_USEDEFAULT, CreateWindowExW, DefWindowProcW, DestroyWindow,
            RegisterClassW, UnregisterClassW, WNDCLASSW,
        },
    },
    core::PCWSTR,
};

const WGL_CONTEXT_MAJOR_VERSION_ARB: i32 = 0x2091;
const WGL_CONTEXT_MINOR_VERSION_ARB: i32 = 0x2092;
const WGL_CONTEXT_PROFILE_MASK_ARB: i32 = 0x9126;
const WGL_CONTEXT_FLAGS_ARB: i32 = 0x2094;

const WGL_CONTEXT_DEBUG_BIT_ARB: i32 = 0x00000001;
const WGL_CONTEXT_CORE_PROFILE_BIT_ARB: i32 = 0x00000001;
const WGL_CONTEXT_COMPATIBILITY_PROFILE_BIT_ARB: i32 = 0x00000002;
const WGL_CONTEXT_ES2_PROFILE_BIT_EXT: i32 = 0x00000004;

const WGL_DRAW_TO_WINDOW_ARB: i32 = 0x2001;
const WGL_ACCELERATION_ARB: i32 = 0x2003;
const WGL_SUPPORT_OPENGL_ARB: i32 = 0x2010;
const WGL_DOUBLE_BUFFER_ARB: i32 = 0x2011;
const WGL_PIXEL_TYPE_ARB: i32 = 0x2013;
const WGL_RED_BITS_ARB: i32 = 0x2015;
const WGL_GREEN_BITS_ARB: i32 = 0x2017;
const WGL_BLUE_BITS_ARB: i32 = 0x2019;
const WGL_ALPHA_BITS_ARB: i32 = 0x201B;
const WGL_DEPTH_BITS_ARB: i32 = 0x2022;
const WGL_STENCIL_BITS_ARB: i32 = 0x2023;
const WGL_FULL_ACCELERATION_ARB: i32 = 0x2027;
const WGL_TYPE_RGBA_ARB: i32 = 0x202B;
const WGL_SAMPLE_BUFFERS_ARB: i32 = 0x2041;
const WGL_SAMPLES_ARB: i32 = 0x2042;
const WGL_FRAMEBUFFER_SRGB_CAPABLE_ARB: i32 = 0x20A9;

type WglCreateContextAttribsARB = unsafe extern "system" fn(HDC, HGLRC, *const i32) -> HGLRC;
type WglChoosePixelFormatARB =
    unsafe extern "system" fn(HDC, *const i32, *const f32, u32, *mut i32, *mut u32) -> i32;
type WglSwapIntervalEXT = unsafe extern "system" fn(i32) -> i32;
type WglGetExtensionsStringEXT = unsafe extern "system" fn() -> *const c_char;
type WglGetExtensionsStringARB = unsafe extern "system" fn(HDC) -> *const c_char;

pub struct GlContext {
    hwnd: HWND,
    hdc: HDC,
    hglrc: HGLRC,
    gl_library: HMODULE,
}

impl GlContext {
    pub unsafe fn new(hwnd: HWND, config: crate::GlConfig) -> Result<Self, Error> {
        unsafe {
            let ext = WglExtensions::get();
            let hdc = GetDC(hwnd);
            let gl_library = LoadLibraryA(c"opengl32.dll".as_ptr() as *const _);

            let (format_id, format_desc) = create_pixel_format_arb(hdc, &config, ext)
                .or_else(|| create_pixel_format_fallback(hdc, &config))
                .ok_or_else(|| {
                    FreeLibrary(gl_library);
                    ReleaseDC(hwnd, hdc);
                    Error::OpenGlError("Failed to find a matching pixel format".to_owned())
                })?;

            SetPixelFormat(hdc, format_id, &format_desc);

            let hglrc = create_context_arb(hdc, &config, ext)
                .or_else(|| create_context_fallback(hdc))
                .ok_or_else(|| {
                    FreeLibrary(gl_library);
                    ReleaseDC(hwnd, hdc);
                    Error::OpenGlError(
                        "Failed to create a context with given requirements".to_owned(),
                    )
                })?;

            if ext.ext_swap_control
                && let Some(swap_interval) = ext.swap_interval
            {
                wglMakeCurrent(hdc, hglrc);
                (swap_interval)(0);
                wglMakeCurrent(hdc, null_mut());
            }

            Ok(Self {
                hwnd,
                hdc,
                hglrc,
                gl_library,
            })
        }
    }
}

impl Debug for GlContext {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("GlContext").finish()
    }
}

impl crate::GlContext for GlContext {
    fn swap_buffers(&self) {
        unsafe {
            SwapBuffers(self.hdc);
        }
    }

    fn get_proc_address(&self, symbol: &CStr) -> *const c_void {
        unsafe {
            wglGetProcAddress(symbol.as_ptr() as *const _)
                .filter(|&ptr| check_ptr(ptr as *const _))
                .or_else(|| GetProcAddress(self.gl_library, symbol.as_ptr() as *const _))
                .filter(|&ptr| check_ptr(ptr as *const _))
                .map(|x| x as *const c_void)
                .unwrap_or(null())
        }
    }

    fn make_current(&self, current: bool) -> bool {
        unsafe { wglMakeCurrent(self.hdc, if current { self.hglrc } else { null_mut() }) != 0 }
    }
}

impl Drop for GlContext {
    fn drop(&mut self) {
        unsafe {
            wglMakeCurrent(null_mut(), null_mut());
            wglDeleteContext(self.hglrc);
            ReleaseDC(self.hwnd, self.hdc);
            FreeLibrary(self.gl_library);
        }
    }
}

#[derive(Default)]
struct WglExtensions {
    create_context_attribs: Option<WglCreateContextAttribsARB>,
    choose_pixel_format: Option<WglChoosePixelFormatARB>,
    swap_interval: Option<WglSwapIntervalEXT>,

    ext_context_arb: bool,
    ext_context_es_profile: bool,
    ext_multisample: bool,
    ext_pixel_format_arb: bool,
    ext_framebuffer_srgb: bool,
    ext_swap_control: bool,
}

impl WglExtensions {
    fn get() -> &'static Self {
        static CACHE: OnceLock<WglExtensions> = OnceLock::new();
        CACHE.get_or_init(|| unsafe { Self::create() })
    }

    unsafe fn create() -> WglExtensions {
        unsafe {
            let class_name = to_widestring(&format!("picoview-dummy-{}", generate_guid()));
            let window_class = RegisterClassW(&WNDCLASSW {
                style: CS_OWNDC,
                lpfnWndProc: Some(DefWindowProcW),
                hInstance: hinstance(),
                lpszClassName: class_name.as_ptr(),
                ..std::mem::zeroed()
            });

            if window_class == 0 {
                return WglExtensions::default();
            }

            let hwnd = CreateWindowExW(
                0,
                window_class as PCWSTR,
                [0].as_ptr(),
                0,
                CW_USEDEFAULT,
                CW_USEDEFAULT,
                CW_USEDEFAULT,
                CW_USEDEFAULT,
                null_mut(),
                null_mut(),
                hinstance(),
                null_mut(),
            );

            if hwnd.is_null() {
                return WglExtensions::default();
            }

            let hdc = GetDC(hwnd);
            let pfd = PIXELFORMATDESCRIPTOR {
                nSize: std::mem::size_of::<PIXELFORMATDESCRIPTOR>() as u16,
                nVersion: 1,
                dwFlags: PFD_DRAW_TO_WINDOW | PFD_SUPPORT_OPENGL | PFD_DOUBLEBUFFER,
                iPixelType: PFD_TYPE_RGBA,
                cColorBits: 32,
                cAlphaBits: 8,
                cDepthBits: 24,
                cStencilBits: 8,
                iLayerType: PFD_MAIN_PLANE as _,
                ..std::mem::zeroed()
            };

            SetPixelFormat(hdc, ChoosePixelFormat(hdc, &pfd), &pfd);

            let hglrc = wglCreateContext(hdc);
            if hglrc.is_null() {
                ReleaseDC(hwnd, hdc);
                UnregisterClassW(window_class as PCWSTR, hinstance());
                DestroyWindow(hwnd);
                return WglExtensions::default();
            }

            wglMakeCurrent(hdc, hglrc);

            macro_rules! load_fn {
                ($type:ident, $lit:literal) => {
                    std::mem::transmute::<PROC, Option<$type>>(wglGetProcAddress(
                        concat!($lit, "\0").as_ptr() as *const _,
                    ))
                    .filter(|&ptr| check_ptr(ptr as *const _))
                };
            }

            let extensions: HashSet<String> = {
                let get_extensions_string_ext =
                    load_fn!(WglGetExtensionsStringEXT, "wglGetExtensionsStringEXT");
                let get_extensions_string_arb =
                    load_fn!(WglGetExtensionsStringARB, "wglGetExtensionsStringARB");

                let extension_str = get_extensions_string_ext
                    .map(|f| f())
                    .or_else(|| get_extensions_string_arb.map(|f| f(hdc)))
                    .map(|x| CStr::from_ptr(x))
                    .unwrap_or_default()
                    .to_string_lossy();

                extension_str.split(' ').map(ToString::to_string).collect()
            };

            let context = Self {
                create_context_attribs: load_fn!(
                    WglCreateContextAttribsARB,
                    "wglCreateContextAttribsARB"
                ),
                choose_pixel_format: load_fn!(WglChoosePixelFormatARB, "wglChoosePixelFormatARB"),
                swap_interval: load_fn!(WglSwapIntervalEXT, "wglSwapIntervalEXT"),

                ext_context_arb: extensions.contains("WGL_ARB_create_context"),
                ext_context_es_profile: extensions.contains("WGL_EXT_create_context_es_profile")
                    || extensions.contains("WGL_EXT_create_context_es2_profile"),

                ext_multisample: extensions.contains("WGL_ARB_multisample"),
                ext_pixel_format_arb: extensions.contains("WGL_ARB_pixel_format"),
                ext_framebuffer_srgb: extensions.contains("WGL_ARB_framebuffer_sRGB")
                    || extensions.contains("WGL_EXT_framebuffer_sRGB"),
                ext_swap_control: extensions.contains("WGL_EXT_swap_control"),
            };

            wglMakeCurrent(hdc, null_mut());
            wglDeleteContext(hglrc);
            ReleaseDC(hwnd, hdc);
            UnregisterClassW(window_class as PCWSTR, hinstance());
            DestroyWindow(hwnd);

            context
        }
    }
}

fn check_ptr(ptr: *const c_void) -> bool {
    let ptr = ptr as usize;
    ptr >= 8 && ptr != usize::MAX
}

fn create_context_fallback(hdc: HDC) -> Option<HGLRC> {
    unsafe {
        let ptr = wglCreateContext(hdc);
        if ptr.is_null() { None } else { Some(ptr) }
    }
}

/*  "WGL_ARB_multisample" => Extensions::MULTISAMPLE,
"WGL_ARB_framebuffer_sRGB"
| "WGL_EXT_framebuffer_sRGB"
| "WGL_EXT_colorspace" => Extensions::FRAMEBUFFER_SRGB,
"WGL_EXT_create_context_es2_profile"
| "WGL_EXT_create_context_es_profile" => Extensions::ES_CONTEXT,
"WGL_EXT_swap_control" => Extensions::SWAP_CONTROL,
"WGL_ARB_create_context" => Extensions::CREATE_CONTEXT,
"WGL_ARB_pixel_format" => Extensions::PIXEL_FORMAT,
_ => continue, */

fn create_context_arb(hdc: HDC, config: &crate::GlConfig, ext: &WglExtensions) -> Option<HGLRC> {
    unsafe {
        let create_context_attribs = ext.create_context_attribs?;
        if !ext.ext_context_arb {
            return None;
        }

        let ctx_attribs = {
            let mut ctx_attribs = vec![];

            if config.debug {
                ctx_attribs.extend_from_slice(&[WGL_CONTEXT_FLAGS_ARB, WGL_CONTEXT_DEBUG_BIT_ARB]);
            }

            match config.version {
                crate::GlVersion::Core(major, minor) => {
                    ctx_attribs.extend_from_slice(&[
                        WGL_CONTEXT_MAJOR_VERSION_ARB,
                        major as i32,
                        WGL_CONTEXT_MINOR_VERSION_ARB,
                        minor as i32,
                        WGL_CONTEXT_PROFILE_MASK_ARB,
                        WGL_CONTEXT_CORE_PROFILE_BIT_ARB,
                    ]);
                }
                crate::GlVersion::Compat(major, minor) => {
                    ctx_attribs.extend_from_slice(&[
                        WGL_CONTEXT_MAJOR_VERSION_ARB,
                        major as i32,
                        WGL_CONTEXT_MINOR_VERSION_ARB,
                        minor as i32,
                        WGL_CONTEXT_PROFILE_MASK_ARB,
                        WGL_CONTEXT_COMPATIBILITY_PROFILE_BIT_ARB,
                    ]);
                }
                crate::GlVersion::ES(major, minor) => {
                    if !ext.ext_context_es_profile {
                        return None;
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
            None
        } else {
            Some(context)
        }
    }
}

fn create_pixel_format_fallback(
    hdc: HDC,
    config: &crate::GlConfig,
) -> Option<(i32, PIXELFORMATDESCRIPTOR)> {
    unsafe {
        let (red, green, blue, alpha, depth, stencil) = config.format.as_rgbads();

        let pfd = PIXELFORMATDESCRIPTOR {
            nSize: size_of::<PIXELFORMATDESCRIPTOR>() as u16,
            nVersion: 1,
            dwFlags: PFD_DRAW_TO_WINDOW
                | PFD_SUPPORT_OPENGL
                | (PFD_DOUBLEBUFFER * config.double_buffer as u32),
            iPixelType: PFD_TYPE_RGBA,
            cColorBits: (red + green + blue + alpha) as _,
            cAlphaBits: alpha as _,
            cDepthBits: depth as _,
            cStencilBits: stencil as _,
            iLayerType: PFD_MAIN_PLANE as _,
            ..zeroed()
        };

        let pixel_format = ChoosePixelFormat(hdc, &pfd);
        if pixel_format == 0 {
            None
        } else {
            Some((pixel_format, pfd))
        }
    }
}

fn create_pixel_format_arb(
    hdc: HDC,
    config: &crate::GlConfig,
    ext: &WglExtensions,
) -> Option<(i32, PIXELFORMATDESCRIPTOR)> {
    unsafe {
        let choose_pixel_format = ext.choose_pixel_format?;
        if !ext.ext_pixel_format_arb {
            return None;
        }

        let pixel_format_attribs = {
            let (red, green, blue, alpha, depth, stencil) = config.format.as_rgbads();
            #[rustfmt::skip]
            let mut pixel_format_attribs = vec![
                WGL_DRAW_TO_WINDOW_ARB, 1,
                WGL_ACCELERATION_ARB, WGL_FULL_ACCELERATION_ARB,
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

            if ext.ext_multisample {
                pixel_format_attribs.extend_from_slice(&[
                    WGL_SAMPLE_BUFFERS_ARB,
                    (config.msaa_count != 0) as i32,
                    WGL_SAMPLES_ARB,
                    config.msaa_count as i32,
                ]);
            }

            if ext.ext_framebuffer_srgb {
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
            null(),
            1,
            &mut format_id,
            &mut num_formats,
        );

        if num_formats == 0 {
            return None;
        }

        let mut pfd: PIXELFORMATDESCRIPTOR = zeroed();
        if DescribePixelFormat(
            hdc,
            format_id,
            size_of::<PIXELFORMATDESCRIPTOR>() as u32,
            &mut pfd,
        ) == 0
        {
            return None;
        }

        Some((format_id, pfd))
    }
}
