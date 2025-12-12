//! Safe Rust bindings for Google's [`libultrahdr`](https://github.com/google/libultrahdr),
//! the reference implementation of UltraHDR gain-map JPEG encoding/decoding.
//!
//! The crate exposes a small, stateful API that mirrors the C library while handling
//! memory ownership and validation for you:
//! - [`Decoder`] reads JPEGs, exposes gain-map metadata (if present), and produces packed
//!   pixel views.
//! - [`Encoder`] writes UltraHDR or plain JPEGs from packed pixel buffers or compressed
//!   inputs.
//! - [`RawImage`], [`CompressedImage`], and [`DecodedPackedView`] describe image buffers
//!   without requiring you to depend on [`sys`] directly.
//!
//! For a higher-level walkthrough, see `examples/ultrahdr_app.rs` in this crate and the
//! CLI in the companion `ultrahdr-bake` package.

/// Low-level bindings to `libultrahdr`. Most users should favor the safe wrappers
/// re-exported from this crate.
pub use ultrahdr_sys as sys;

mod decoder;
mod encoder;
mod error;
mod types;

pub use decoder::Decoder;
pub use encoder::Encoder;
pub use error::{Error, Result};
pub use types::*;
