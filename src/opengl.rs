use crate::*;
use std::ffi::{CStr, c_void};
use std::fmt;

/// A requested OpenGL version
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GlVersion {
    /// Core profile
    Core(u8, u8),

    /// Compatibility profile
    Compat(u8, u8),

    /// OpenGL ES
    ES(u8, u8),
}

/// A requested OpenGL format for the window framebuffer
#[allow(non_camel_case_types)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GlFormat {
    /// 8-bit RGB format
    RGB8,

    /// 8-bit RGBA format
    RGBA8,

    /// 8-bit RGB with 24-bit depth buffer
    RGB8_D24,

    /// 8-bit RGBA with 24-bit depth buffer
    RGBA8_D24,

    /// 8-bit RGB with 24-bit depth buffer and 8-bit stencil buffer
    RGB8_D24_S8,

    /// 8-bit RGBA with 24-bit depth buffer and 8-bit stencil buffer
    RGBA8_D24_S8,
}

impl GlFormat {
    /// Get the number of bits for red, green, blue, alpha, depth, and stencil
    /// channels respectively
    pub fn as_rgbads(self) -> (u8, u8, u8, u8, u8, u8) {
        match self {
            GlFormat::RGB8 => (8, 8, 8, 0, 0, 0),
            GlFormat::RGBA8 => (8, 8, 8, 8, 0, 0),
            GlFormat::RGB8_D24 => (8, 8, 8, 0, 24, 0),
            GlFormat::RGBA8_D24 => (8, 8, 8, 8, 24, 0),
            GlFormat::RGB8_D24_S8 => (8, 8, 8, 0, 24, 8),
            GlFormat::RGBA8_D24_S8 => (8, 8, 8, 8, 24, 8),
        }
    }
}

/// A requested OpenGL configuration for a window
#[derive(Debug, Clone, Copy)]
pub struct GlConfig {
    /// OpenGL version to request.
    ///
    /// Note that the actual version may be different (higher or lower)
    /// depending on the platform and driver support. You can query the actual
    /// version after context creation.
    pub version: GlVersion,

    /// Whether to enable debug mode extension, if available.
    ///
    /// Note that this is only a request, and the actual context may not have
    /// debug mode enabled.
    pub debug: bool,

    /// Output framebuffer format.
    ///
    /// Note that the actual format may be a superset of the requested format
    /// (e.g. requesting RGB8 may result in RGBA8 being used).
    pub format: GlFormat,

    /// Whether to use double buffering.
    ///
    /// Set to `None` if you do not care.
    pub double_buffer: Option<bool>,

    /// Whether to use hardware acceleration.
    ///
    /// Set to `None` if you do not care.
    pub hardware_acceleration: Option<bool>,

    /// Whether to perform sRGB gamma correction when writing to the output
    /// framebuffer
    pub srgb: bool,

    /// Number of samples for multisample anti-aliasing, set to 0/1 to disable
    /// MSAA
    pub msaa_count: u8,
}

impl Default for GlConfig {
    fn default() -> Self {
        Self {
            version: GlVersion::Compat(1, 1),
            double_buffer: None,
            hardware_acceleration: None,
            debug: false,
            srgb: false,
            format: GlFormat::RGBA8_D24_S8,
            msaa_count: 0,
        }
    }
}

/// OpenGL context belonging to a window
#[derive(Clone, Copy)]
pub struct GlContext<'a>(pub(crate) &'a dyn platform::PlatformOpenGl);

impl<'a> GlContext<'a> {
    /// Make this OpenGL context current or not current.
    ///
    /// Does nothing if the context is already in the requested state.
    ///
    /// # Errors    
    ///
    /// Returns [`MakeCurrentError`] if the context could not be made current.
    pub fn make_current(&self, current: bool) -> Result<(), MakeCurrentError> {
        self.0.make_current(current)
    }

    /// Swap the front and back buffers if double buffering is enabled
    ///
    /// # Notes
    ///
    /// It might be a good idea to skip drawing if the window has a size of 0
    /// (some drivers do not handle this well).
    ///
    /// # Errors
    ///
    /// Returns [`SwapBuffersError`] if the buffers could not be swapped.
    pub fn swap_buffers(&self) -> Result<(), SwapBuffersError> {
        self.0.swap_buffers()
    }

    /// Get the address of an OpenGL function by name
    pub fn get_proc_address(&self, name: &CStr) -> *const c_void {
        self.0.get_proc_address(name)
    }
}

impl<'a> fmt::Debug for GlContext<'a> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_tuple("GlContext").finish_non_exhaustive()
    }
}
