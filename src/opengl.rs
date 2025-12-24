use std::{
    ffi::{CStr, c_void},
    fmt::Debug,
};

/// A requested OpenGL version
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GlVersion {
    /// Core profile
    Core(u32, u32),

    /// Compatibility profile
    Compat(u32, u32),

    /// OpenGL ES
    ES(u32, u32),
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
    /// Get the number of bits for red, green, blue, alpha, depth, and stencil channels respectively
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
    /// OpenGL version to request
    pub version: GlVersion,

    /// OpenGL format to request
    pub format: GlFormat,

    /// Whether to use double buffering
    pub double_buffer: bool,

    /// Whether to enable debug mode extension
    pub debug: bool,

    /// Whether to perform sRGB gamma correction when writing to the output framebuffer
    pub srgb: bool,

    /// Number of samples for multisample anti-aliasing, set to 0 to disable MSAA
    pub msaa_count: u32,

    /// Do not fail if the requested configuration is not available
    ///
    /// `Event::WindowFrame` may then provide `gl: None` if no suitable context could be created
    pub optional: bool,
}

impl Default for GlConfig {
    fn default() -> Self {
        Self {
            version: GlVersion::Compat(1, 1),
            double_buffer: true,
            debug: false,
            srgb: false,
            optional: false,
            format: GlFormat::RGBA8_D24_S8,
            msaa_count: 0,
        }
    }
}

/// OpenGL context belonging to a window
pub trait GlContext: Debug {
    /// Swap the front and back buffers
    fn swap_buffers(&self);

    /// Get the address of an OpenGL function
    fn get_proc_address(&self, name: &CStr) -> *const c_void;

    /// Make the OpenGL context active or inactive on the current thread
    /// Returns true on success
    ///
    /// All OpenGL calls must be made only when the context is active for the current thread
    fn make_current(&self, current: bool) -> bool;
}
