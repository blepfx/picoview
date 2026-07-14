use crate::platform::PlatformOpenGl;
use crate::platform::win::util::error::Win32Error;
use crate::platform::win::util::wgl::{
    create_context_arb, create_context_fallback, create_pixel_format_arb,
    create_pixel_format_fallback, try_set_swap_interval,
};
use crate::{MakeCurrentError, OpenGlError, SwapBuffersError};
use std::ffi::{CStr, c_void};
use std::ptr::{null, null_mut};
use windows_sys::Win32::Foundation::{FreeLibrary, HMODULE, HWND};
use windows_sys::Win32::Graphics::Gdi::{GetDC, HDC, ReleaseDC};
use windows_sys::Win32::Graphics::OpenGL::{
    HGLRC, SetPixelFormat, SwapBuffers, wglDeleteContext, wglGetCurrentContext, wglGetProcAddress,
    wglMakeCurrent,
};
use windows_sys::Win32::System::LibraryLoader::{GetProcAddress, LoadLibraryA};

/// WGL based [`PlatformOpenGl`] implementation
pub struct GlContext {
    /// The window our context was created for
    hwnd: HWND,
    /// Window device context
    hdc: HDC,
    /// Window OpenGL context
    hglrc: HGLRC,
    /// Windows OpenGL module (used as a fallback for `wglGetProcAddress` when
    /// it returns null)
    hmodule: HMODULE,
}

impl GlContext {
    pub unsafe fn new(hwnd: HWND, config: crate::GlConfig) -> Result<Self, OpenGlError> {
        unsafe {
            let hmodule = LoadLibraryA(c"opengl32.dll".as_ptr() as _);
            if hmodule.is_null() {
                return Err(Win32Error::last_error().into());
            }

            let hdc = GetDC(hwnd);
            if hdc.is_null() {
                return Err(Win32Error::last_error().into());
            }

            let (format_id, format_desc) = create_pixel_format_arb(hdc, &config)
                .or_else(|_| create_pixel_format_fallback(hdc, &config))
                .map_err(|_| {
                    ReleaseDC(hwnd, hdc);
                    FreeLibrary(hmodule);
                    OpenGlError::FormatUnsupported
                })?;

            SetPixelFormat(hdc, format_id, &format_desc);

            let hglrc = create_context_arb(hdc, &config)
                .or_else(|_| create_context_fallback(hdc))
                .map_err(|_| {
                    ReleaseDC(hwnd, hdc);
                    FreeLibrary(hmodule);
                    OpenGlError::VersionUnsupported
                })?;

            try_set_swap_interval(hdc, hglrc, 0);

            Ok(Self {
                hwnd,
                hdc,
                hglrc,
                hmodule,
            })
        }
    }
}

impl PlatformOpenGl for GlContext {
    fn get_proc_address(&self, symbol: &CStr) -> *const c_void {
        unsafe {
            wglGetProcAddress(symbol.as_ptr() as *const _)
                .or_else(|| GetProcAddress(self.hmodule, symbol.as_ptr() as *const _))
                .map(|x| x as *const c_void)
                .unwrap_or(null())
        }
    }

    fn swap_buffers(&self) -> Result<(), SwapBuffersError> {
        unsafe { SwapBuffers(self.hdc) };
        Ok(())
    }

    fn make_current(&self, current: bool) -> Result<(), MakeCurrentError> {
        unsafe {
            let context = wglGetCurrentContext();
            if (current && context == self.hglrc) || (!current && context != self.hglrc) {
                // already in the requested state, we okay!
                return Ok(());
            }

            let result =
                wglMakeCurrent(self.hdc, if current { self.hglrc } else { null_mut() }) != 0;

            if result {
                Ok(())
            } else {
                Err(MakeCurrentError)
            }
        }
    }
}

impl Drop for GlContext {
    fn drop(&mut self) {
        unsafe {
            wglMakeCurrent(null_mut(), null_mut());
            wglDeleteContext(self.hglrc);
            ReleaseDC(self.hwnd, self.hdc);
            FreeLibrary(self.hmodule);
        }
    }
}
