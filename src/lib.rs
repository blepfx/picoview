#![doc = include_str!("../README.md")]
#![cfg_attr(docsrs, feature(doc_cfg))]
#![allow(clippy::identity_op)]
#![deny(clippy::unwrap_used, clippy::unimplemented, clippy::indexing_slicing)]
#![warn(
    missing_docs,
    missing_copy_implementations,
    missing_debug_implementations,
    rust_2018_idioms,
    clippy::missing_panics_doc,
    clippy::missing_errors_doc,
    clippy::missing_safety_doc,
    clippy::transmute_ptr_to_ptr,
    clippy::invalid_upcast_comparisons
)]

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
