//! Safe, idiomatic Rust bindings for Google's [`libultrahdr`] gain map codec.
//!
//! [`libultrahdr`] reads and writes UltraHDR images: a JPEG stream that carries an
//! SDR base image plus a gain map, allowing a renderer to reconstruct an HDR rendition. This crate
//! wraps the C API with owning image types, Rust enums instead of C constants and descriptive
//! errors, while [`sys`] re-exports the raw bindings for anything not covered here.
//!
//! # Overview
//!
//! - [`Encoder`] turns raw or compressed input images into an UltraHDR stream. Inputs are copied,
//!   so [`image::RawImage`] and [`image::CompressedImage`] have no lifetimes.
//! - [`Decoder`] probes a stream for its size, gain map metadata and embedded EXIF/ICC blocks, then
//!   decodes it into packed pixels. A decoder is one-shot: it may be configured until it is probed,
//!   and its configuration is frozen afterwards until [`Decoder::reset`].
//! - [`image`] holds the pixel formats, colour descriptions and image descriptors.
//! - [`gainmap`] holds [`GainMapMetadata`], [`edit`] holds the [`Mirror`] and [`Rotation`] effects
//!   shared by both directions, and [`error`] holds [`Error`] and [`Result`].
//!
//! # Quick start
//!
//! Encoding a single HDR image (the SDR rendition is tone-mapped by libultrahdr):
//!
//! ```no_run
//! use ultrahdr::{
//!     Codec, ColorAspects, ColorGamut, ColorRange, ColorTransfer, Encoder, ImageLabel,
//!     PixelFormat, RawImage,
//! };
//!
//! # fn main() -> ultrahdr::Result<()> {
//! let aspects = ColorAspects::new(ColorGamut::DisplayP3, ColorTransfer::Pq, ColorRange::Full);
//! let mut hdr = RawImage::new(PixelFormat::Rgba1010102, 1920, 1080, aspects)?;
//! // ... fill hdr.data_mut().expect("packed") with pixels ...
//!
//! let mut enc = Encoder::new()?;
//! enc.set_raw_image(ImageLabel::Hdr, &hdr)?;
//! enc.set_quality(ImageLabel::Hdr, 95)?;
//! enc.set_output_format(Codec::Jpeg)?;
//! enc.encode()?;
//!
//! let stream = enc.encoded_stream().expect("no output");
//! assert!(ultrahdr::is_uhdr_image(stream.bytes()));
//! # Ok(())
//! # }
//! ```
//!
//! Decoding a stream back to linear-float pixels while preserving its metadata:
//!
//! ```no_run
//! use ultrahdr::{ColorTransfer, CompressedImage, Decoder, PixelFormat};
//!
//! # fn main() -> ultrahdr::Result<()> {
//! # let bytes: Vec<u8> = Vec::new();
//! assert!(ultrahdr::is_uhdr_image(&bytes));
//!
//! let mut dec = Decoder::new()?;
//! dec.set_image(&CompressedImage::new(bytes.as_slice()))?;
//! dec.set_output_format(PixelFormat::Rgba1010102)?;
//! dec.set_output_transfer(ColorTransfer::Pq)?;
//!
//! let info = dec.info()?;
//! let pixels = dec.decode()?;
//! println!("{}x{} -> {} bytes", info.width, info.height, pixels.to_owned_image().data.len());
//! # Ok(())
//! # }
//! ```
//!
//! # Errors
//!
//! Fallible calls return [`Result`]. The [`Error`] enum distinguishes invalid parameters, invalid
//! codec state, allocation failures and unsupported features, and carries the detail string
//! reported by libultrahdr ([`Error::detail`]).
//!
//! [`libultrahdr`]: https://github.com/google/libultrahdr

#![warn(missing_docs)]

pub mod decoder;
pub mod edit;
pub mod encoder;
pub mod error;
pub mod gainmap;
pub mod image;

pub use decoder::{Decoder, ImageInfo, is_uhdr_image};
pub use edit::{Mirror, Rotation};
pub use encoder::Encoder;
pub use error::{Error, Result};
pub use gainmap::{GainMapMetadata, SDR_WHITE_NITS};
pub use image::{
    Codec, ColorAspects, ColorGamut, ColorRange, ColorTransfer, CompressedImage, DecodedImage,
    DecodedView, EncodedImage, EncodedView, ImageLabel, MemBlockView, PixelFormat, Plane, Preset,
    RawImage,
};

/// Raw FFI bindings, re-exported for cases the safe layer does not cover.
///
/// The types here mirror the C API (`uhdr_*`) and are not subject to semver guarantees beyond what
/// the upstream project offers.
#[doc(no_inline)]
pub use ultrahdr_sys as sys;

/// Version of the linked libultrahdr as `(major, minor, patch)`.
pub const LIB_VERSION: (u32, u32, u32) = (
    sys::UHDR_LIB_VER_MAJOR,
    sys::UHDR_LIB_VER_MINOR,
    sys::UHDR_LIB_VER_PATCH,
);

/// Version of the linked libultrahdr as a `major.minor.patch` string.
pub fn version_string() -> String {
    format!("{}.{}.{}", LIB_VERSION.0, LIB_VERSION.1, LIB_VERSION.2)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn linked_library_version_is_reported() {
        assert_eq!(
            version_string(),
            format!("{}.{}.{}", LIB_VERSION.0, LIB_VERSION.1, LIB_VERSION.2)
        );
        // The crate is written against the libultrahdr 2.x API surface.
        assert!(LIB_VERSION.0 >= 2, "unexpected libultrahdr {LIB_VERSION:?}");
    }

    #[test]
    fn enum_conversions_round_trip() {
        use sys::uhdr_img_fmt_t as Fmt;

        assert_eq!(
            Fmt::from(PixelFormat::Rgba1010102),
            Fmt::UHDR_IMG_FMT_32bppRGBA1010102
        );
        assert_eq!(
            PixelFormat::try_from(Fmt::UHDR_IMG_FMT_24bppYCbCrP010),
            Ok(PixelFormat::P010)
        );
        assert!(PixelFormat::try_from(Fmt::UHDR_IMG_FMT_UNSPECIFIED).is_err());

        assert_eq!(
            sys::uhdr_color_gamut_t::from(ColorGamut::DisplayP3),
            sys::uhdr_color_gamut_t::UHDR_CG_DISPLAY_P3
        );
        assert!(ColorGamut::try_from(sys::uhdr_color_gamut_t::UHDR_CG_UNSPECIFIED).is_err());
        assert!(ColorTransfer::try_from(sys::uhdr_color_transfer_t::UHDR_CT_UNSPECIFIED).is_err());
        assert!(ColorRange::try_from(sys::uhdr_color_range_t::UHDR_CR_UNSPECIFIED).is_err());

        assert_eq!(
            ImageLabel::GainMap.to_sys(),
            sys::uhdr_img_label_t::UHDR_GAIN_MAP_IMG
        );
        assert!(Codec::Jpeg.to_sys() == sys::uhdr_codec_t::UHDR_CODEC_JPG);
        assert!(Preset::Realtime.to_sys() == sys::uhdr_enc_preset_t::UHDR_USAGE_REALTIME);
        assert_eq!(
            sys::uhdr_mirror_direction_t::from(Mirror::Horizontal),
            sys::uhdr_mirror_direction_t::UHDR_MIRROR_HORIZONTAL
        );
        assert_eq!(Rotation::Deg270.degrees(), 270);
    }
}
