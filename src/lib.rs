#![doc = include_str!("../README.md")]
#![allow(clippy::identity_op)]
#![deny(clippy::panic, clippy::unwrap_used)]
#![warn(clippy::todo, clippy::unimplemented)]
#![warn(missing_debug_implementations)]
#![warn(missing_docs)]

mod data;
mod error;
mod opengl;
mod platform;
mod window;

pub use data::*;
pub use error::*;
pub use opengl::*;
pub use window::*;

pub use raw_window_handle as rwh_06;
