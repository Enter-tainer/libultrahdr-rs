//! Pure Rust implementation of UltraHDR gain-map JPEG encoding/decoding.
//!
//! This crate provides:
//! - Color space conversion utilities (sRGB, PQ, HLG, linear)
//! - Gain map metadata (ISO 21496-1) reading and writing

pub mod color;
pub mod decoder;
pub mod encoder;
pub mod error;
pub mod gainmap;
pub mod jpeg;
pub mod mpf;
pub mod types;

#[cfg(feature = "simd")]
pub(crate) mod simd;

pub use error::{Error, Result};
pub use types::*;
