use std::{
    ffi::{CStr, c_void},
    fmt::Debug,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GlVersion {
    Core(u32, u32),
    Compat(u32, u32),
    ES(u32, u32),
}

#[allow(non_camel_case_types)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GlFormat {
    RGB8,
    RGBA8,

    RGB8_D24,
    RGBA8_D24,

    RGB8_D24_S8,
    RGBA8_D24_S8,
}

impl GlFormat {
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

#[derive(Debug, Clone, Copy)]
pub struct GlConfig {
    pub version: GlVersion,

    pub double_buffer: bool,
    pub debug: bool,
    pub srgb: bool,
    pub optional: bool,

    pub format: GlFormat,
    pub msaa_count: u32,
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

pub trait GlContext: Debug {
    fn swap_buffers(&self);
    fn get_proc_address(&self, name: &CStr) -> *const c_void;
    fn make_current(&self, current: bool) -> bool;
}
