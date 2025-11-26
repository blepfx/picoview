#![doc = include_str!("../README.md")]
#![allow(clippy::identity_op)]
#![deny(clippy::unwrap_used)]
// #![warn(missing_docs)]
// TODO: setup clippy

mod data;
mod opengl;
mod platform;
mod window;

pub use data::*;
pub use opengl::*;
pub use window::*;

pub use raw_window_handle as rwh_06;
