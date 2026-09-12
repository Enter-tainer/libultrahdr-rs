//! Image descriptors: pixel formats, colour aspects and the buffers handed to the codec.
//!
//! Every image type in this module **owns** its bytes. libultrahdr copies raw and compressed
//! inputs when they are registered with an [`Encoder`](crate::Encoder) or
//! [`Decoder`](crate::Decoder), so there are no lifetimes to manage and no aliasing rules for the
//! caller to respect.
//!
//! ```no_run
//! use ultrahdr::{ColorAspects, ColorGamut, ColorRange, ColorTransfer, PixelFormat, RawImage};
//!
//! # fn main() -> ultrahdr::Result<()> {
//! let aspects = ColorAspects::new(ColorGamut::DisplayP3, ColorTransfer::Pq, ColorRange::Full);
//! let mut hdr = RawImage::new(PixelFormat::Rgba1010102, 1024, 768, aspects)?;
//! // Fill the pixels through the mutable accessors.
//! hdr.data_mut().unwrap().fill(0);
//! # Ok(())
//! # }
//! ```

mod aspects;
mod decoded;
mod format;
mod intent;
mod raw;
mod stream;

pub use aspects::{ColorAspects, ColorGamut, ColorRange, ColorTransfer};
pub use decoded::{DecodedFrame, DecodedImage, DecodedView};
pub use format::{DecodedOutput, PixelFormat, Plane};
pub use intent::{Codec, ImageLabel, Preset};
pub use raw::RawImage;
pub use stream::{CompressedImage, EncodedImage, EncodedView, MemBlockView};
