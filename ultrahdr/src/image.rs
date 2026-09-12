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

use crate::error::{Error, Result};
use crate::sys;
use std::ffi::c_void;

/// Pixel layout of an uncompressed image or gain map.
///
/// The names are the Rust-facing counterparts of the `uhdr_img_fmt` constants.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum PixelFormat {
    /// 10-bit 4:2:0 YCbCr, semi-planar, full-range luma in the MSBs (`24bppYCbCrP010`).
    P010,
    /// 8-bit 4:2:0 YCbCr, planar (`12bppYCbCr420`).
    Yuv420,
    /// 8-bit monochrome (`8bppYCbCr400`); also used by single-channel gain maps.
    Mono8,
    /// 8-bit interleaved RGBA (`32bppRGBA8888`).
    Rgba8888,
    /// 16-bit half-float RGBA (`64bppRGBAHalfFloat`).
    RgbaHalfFloat,
    /// 10-bit interleaved RGBA, 1010102 (`32bppRGBA1010102`).
    Rgba1010102,
    /// 8-bit 4:4:4 YCbCr, planar (`24bppYCbCr444`).
    Yuv444,
    /// 8-bit 4:2:2 YCbCr, planar (`16bppYCbCr422`).
    Yuv422,
    /// 8-bit 4:4:0 YCbCr, planar (`16bppYCbCr440`).
    Yuv440,
    /// 8-bit 4:1:1 YCbCr, planar (`12bppYCbCr411`).
    Yuv411,
    /// 8-bit 4:1:0 YCbCr, planar (`10bppYCbCr410`).
    Yuv410,
    /// 8-bit interleaved RGB (`24bppRGB888`).
    Rgb888,
    /// 10-bit 4:4:4 YCbCr, planar, 16-bit words (`30bppYCbCr444`).
    Yuv444P10,
}

impl PixelFormat {
    /// Bytes per pixel, for formats stored in a single interleaved plane.
    ///
    /// Returns `None` for planar and semi-planar formats ([`PixelFormat::P010`] included), which
    /// need more than one buffer.
    pub fn bytes_per_pixel(self) -> Option<usize> {
        match self.geometry()? {
            [Some(geom), None, None] => Some(geom.unit_bytes),
            _ => None,
        }
    }

    /// Whether the format is stored in a single interleaved plane.
    pub fn is_packed(self) -> bool {
        self.plane_count() == 1
    }

    /// Number of planes the format uses (1 for packed formats, up to 3 for planar ones).
    pub fn plane_count(self) -> usize {
        self.geometry()
            .map_or(0, |planes| planes.iter().filter(|p| p.is_some()).count())
    }

    /// Plane layout of the format, indexed by [`Plane::index`].
    pub(crate) fn geometry(self) -> Option<[Option<PlaneGeom>; 3]> {
        const PACKED: PlaneGeom = PlaneGeom::packed(1);
        let g = |cols_div, rows_div, unit_bytes| {
            Some(PlaneGeom {
                cols_div,
                rows_div,
                unit_bytes,
            })
        };
        Some(match self {
            Self::P010 => [
                Some(PlaneGeom::packed(2)),
                // Interleaved UV: `stride` is in luma pixels and stores U/V pairs.
                g(1, 2, 2),
                None,
            ],
            Self::Yuv420 => [Some(PACKED), g(2, 2, 1), g(2, 2, 1)],
            Self::Mono8 => [Some(PACKED), None, None],
            Self::Rgba8888 => [g(1, 1, 4), None, None],
            Self::RgbaHalfFloat => [g(1, 1, 8), None, None],
            Self::Rgba1010102 => [g(1, 1, 4), None, None],
            Self::Yuv444 => [Some(PACKED), g(1, 1, 1), g(1, 1, 1)],
            Self::Yuv422 => [Some(PACKED), g(2, 1, 1), g(2, 1, 1)],
            Self::Yuv440 => [Some(PACKED), g(1, 2, 1), g(1, 2, 1)],
            Self::Yuv411 => [Some(PACKED), g(4, 1, 1), g(4, 1, 1)],
            Self::Yuv410 => [Some(PACKED), g(4, 2, 1), g(4, 2, 1)],
            Self::Rgb888 => [g(1, 1, 3), None, None],
            Self::Yuv444P10 => [g(1, 1, 2), g(1, 1, 2), g(1, 1, 2)],
        })
    }

    pub(crate) const fn to_sys(self) -> sys::uhdr_img_fmt_t {
        use sys::uhdr_img_fmt::*;
        match self {
            Self::P010 => UHDR_IMG_FMT_24bppYCbCrP010,
            Self::Yuv420 => UHDR_IMG_FMT_12bppYCbCr420,
            Self::Mono8 => UHDR_IMG_FMT_8bppYCbCr400,
            Self::Rgba8888 => UHDR_IMG_FMT_32bppRGBA8888,
            Self::RgbaHalfFloat => UHDR_IMG_FMT_64bppRGBAHalfFloat,
            Self::Rgba1010102 => UHDR_IMG_FMT_32bppRGBA1010102,
            Self::Yuv444 => UHDR_IMG_FMT_24bppYCbCr444,
            Self::Yuv422 => UHDR_IMG_FMT_16bppYCbCr422,
            Self::Yuv440 => UHDR_IMG_FMT_16bppYCbCr440,
            Self::Yuv411 => UHDR_IMG_FMT_12bppYCbCr411,
            Self::Yuv410 => UHDR_IMG_FMT_10bppYCbCr410,
            Self::Rgb888 => UHDR_IMG_FMT_24bppRGB888,
            Self::Yuv444P10 => UHDR_IMG_FMT_30bppYCbCr444,
        }
    }

    pub(crate) fn from_sys(raw: sys::uhdr_img_fmt_t) -> Option<Self> {
        use sys::uhdr_img_fmt::*;
        Some(match raw {
            UHDR_IMG_FMT_24bppYCbCrP010 => Self::P010,
            UHDR_IMG_FMT_12bppYCbCr420 => Self::Yuv420,
            UHDR_IMG_FMT_8bppYCbCr400 => Self::Mono8,
            UHDR_IMG_FMT_32bppRGBA8888 => Self::Rgba8888,
            UHDR_IMG_FMT_64bppRGBAHalfFloat => Self::RgbaHalfFloat,
            UHDR_IMG_FMT_32bppRGBA1010102 => Self::Rgba1010102,
            UHDR_IMG_FMT_24bppYCbCr444 => Self::Yuv444,
            UHDR_IMG_FMT_16bppYCbCr422 => Self::Yuv422,
            UHDR_IMG_FMT_16bppYCbCr440 => Self::Yuv440,
            UHDR_IMG_FMT_12bppYCbCr411 => Self::Yuv411,
            UHDR_IMG_FMT_10bppYCbCr410 => Self::Yuv410,
            UHDR_IMG_FMT_24bppRGB888 => Self::Rgb888,
            UHDR_IMG_FMT_30bppYCbCr444 => Self::Yuv444P10,
            _ => return None,
        })
    }
}

impl From<PixelFormat> for sys::uhdr_img_fmt_t {
    fn from(value: PixelFormat) -> Self {
        value.to_sys()
    }
}

impl TryFrom<sys::uhdr_img_fmt_t> for PixelFormat {
    type Error = Error;

    fn try_from(value: sys::uhdr_img_fmt_t) -> Result<Self> {
        Self::from_sys(value)
            .ok_or_else(|| Error::invalid_parameter(format!("unsupported pixel format {value:?}")))
    }
}

/// Plane selector for planar and semi-planar formats.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Plane {
    /// Luma plane, or the single packed plane of interleaved formats.
    Y,
    /// Cb plane, or the interleaved UV plane of [`PixelFormat::P010`].
    U,
    /// Cr plane.
    V,
}

impl Plane {
    /// Index of the plane in the C `uhdr_raw_image` layout.
    pub const fn index(self) -> usize {
        match self {
            Self::Y => 0,
            Self::U => 1,
            Self::V => 2,
        }
    }
}

/// Layout of a single plane: how its rows and columns are subsampled.
#[derive(Debug, Clone, Copy)]
pub(crate) struct PlaneGeom {
    cols_div: u32,
    rows_div: u32,
    unit_bytes: usize,
}

impl PlaneGeom {
    const fn packed(unit_bytes: usize) -> Self {
        Self {
            cols_div: 1,
            rows_div: 1,
            unit_bytes,
        }
    }

    /// Smallest valid stride for a `width`-pixel image, in samples of this plane.
    fn min_stride(self, width: u32) -> u32 {
        width.div_ceil(self.cols_div)
    }

    /// Bytes required for `height` rows of `stride` samples.
    fn required_len(self, width: u32, height: u32, stride: u32) -> Result<usize> {
        if stride < self.min_stride(width) {
            return Err(Error::invalid_parameter(format!(
                "stride {stride} is smaller than the minimum {} for this plane",
                self.min_stride(width)
            )));
        }
        let rows = height.div_ceil(self.rows_div) as usize;
        (stride as usize)
            .checked_mul(self.unit_bytes)
            .and_then(|row_bytes| row_bytes.checked_mul(rows))
            .ok_or_else(|| Error::invalid_parameter("buffer size overflow"))
    }
}

/// Colour gamut (primaries) of an image.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum ColorGamut {
    /// ITU-R BT.709.
    Bt709,
    /// Display P3 (DCI-P3 primaries, D65 white point).
    DisplayP3,
    /// ITU-R BT.2100 (Rec. 2020 primaries).
    Bt2100,
}

impl ColorGamut {
    pub(crate) const fn to_sys(self) -> sys::uhdr_color_gamut_t {
        use sys::uhdr_color_gamut::*;
        match self {
            Self::Bt709 => UHDR_CG_BT_709,
            Self::DisplayP3 => UHDR_CG_DISPLAY_P3,
            Self::Bt2100 => UHDR_CG_BT_2100,
        }
    }
}

impl From<ColorGamut> for sys::uhdr_color_gamut_t {
    fn from(value: ColorGamut) -> Self {
        value.to_sys()
    }
}

impl TryFrom<sys::uhdr_color_gamut_t> for ColorGamut {
    type Error = Error;

    fn try_from(value: sys::uhdr_color_gamut_t) -> Result<Self> {
        use sys::uhdr_color_gamut::*;
        match value {
            UHDR_CG_BT_709 => Ok(Self::Bt709),
            UHDR_CG_DISPLAY_P3 => Ok(Self::DisplayP3),
            UHDR_CG_BT_2100 => Ok(Self::Bt2100),
            other => Err(Error::invalid_parameter(format!(
                "unspecified or unknown colour gamut {other:?}"
            ))),
        }
    }
}

/// Transfer function (EOTF/OETF) of an image.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum ColorTransfer {
    /// Linear light.
    Linear,
    /// Hybrid log-gamma.
    Hlg,
    /// SMPTE ST 2084 perceptual quantizer.
    Pq,
    /// sRGB / Rec.709 gamma.
    Srgb,
}

impl ColorTransfer {
    pub(crate) const fn to_sys(self) -> sys::uhdr_color_transfer_t {
        use sys::uhdr_color_transfer::*;
        match self {
            Self::Linear => UHDR_CT_LINEAR,
            Self::Hlg => UHDR_CT_HLG,
            Self::Pq => UHDR_CT_PQ,
            Self::Srgb => UHDR_CT_SRGB,
        }
    }
}

impl From<ColorTransfer> for sys::uhdr_color_transfer_t {
    fn from(value: ColorTransfer) -> Self {
        value.to_sys()
    }
}

impl TryFrom<sys::uhdr_color_transfer_t> for ColorTransfer {
    type Error = Error;

    fn try_from(value: sys::uhdr_color_transfer_t) -> Result<Self> {
        use sys::uhdr_color_transfer::*;
        match value {
            UHDR_CT_LINEAR => Ok(Self::Linear),
            UHDR_CT_HLG => Ok(Self::Hlg),
            UHDR_CT_PQ => Ok(Self::Pq),
            UHDR_CT_SRGB => Ok(Self::Srgb),
            other => Err(Error::invalid_parameter(format!(
                "unspecified or unknown colour transfer {other:?}"
            ))),
        }
    }
}

/// Chroma sample range of an image.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum ColorRange {
    /// Studio/limited range (Y in 16..235, chroma in 16..240).
    Limited,
    /// Full range.
    Full,
}

impl ColorRange {
    pub(crate) const fn to_sys(self) -> sys::uhdr_color_range_t {
        use sys::uhdr_color_range::*;
        match self {
            Self::Limited => UHDR_CR_LIMITED_RANGE,
            Self::Full => UHDR_CR_FULL_RANGE,
        }
    }
}

impl From<ColorRange> for sys::uhdr_color_range_t {
    fn from(value: ColorRange) -> Self {
        value.to_sys()
    }
}

impl TryFrom<sys::uhdr_color_range_t> for ColorRange {
    type Error = Error;

    fn try_from(value: sys::uhdr_color_range_t) -> Result<Self> {
        use sys::uhdr_color_range::*;
        match value {
            UHDR_CR_LIMITED_RANGE => Ok(Self::Limited),
            UHDR_CR_FULL_RANGE => Ok(Self::Full),
            other => Err(Error::invalid_parameter(format!(
                "unspecified or unknown colour range {other:?}"
            ))),
        }
    }
}

/// Colour description of an image.
///
/// `None` means "not signalled by the stream", which the C API spells
/// `UHDR_CG_UNSPECIFIED` / `UHDR_CT_UNSPECIFIED` / `UHDR_CR_UNSPECIFIED`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ColorAspects {
    /// Colour gamut, if signalled.
    pub gamut: Option<ColorGamut>,
    /// Transfer function, if signalled.
    pub transfer: Option<ColorTransfer>,
    /// Chroma sample range, if signalled.
    pub range: Option<ColorRange>,
}

impl ColorAspects {
    /// No aspect is signalled (`ColorAspects::default()`).
    pub const UNSPECIFIED: Self = Self {
        gamut: None,
        transfer: None,
        range: None,
    };

    /// Fully specified aspects.
    pub const fn new(gamut: ColorGamut, transfer: ColorTransfer, range: ColorRange) -> Self {
        Self {
            gamut: Some(gamut),
            transfer: Some(transfer),
            range: Some(range),
        }
    }

    /// Set the gamut, keeping the other aspects.
    pub const fn with_gamut(mut self, gamut: ColorGamut) -> Self {
        self.gamut = Some(gamut);
        self
    }

    /// Set the transfer function, keeping the other aspects.
    pub const fn with_transfer(mut self, transfer: ColorTransfer) -> Self {
        self.transfer = Some(transfer);
        self
    }

    /// Set the chroma range, keeping the other aspects.
    pub const fn with_range(mut self, range: ColorRange) -> Self {
        self.range = Some(range);
        self
    }

    pub(crate) const fn to_sys(
        self,
    ) -> (
        sys::uhdr_color_gamut_t,
        sys::uhdr_color_transfer_t,
        sys::uhdr_color_range_t,
    ) {
        use sys::uhdr_color_gamut::UHDR_CG_UNSPECIFIED;
        use sys::uhdr_color_range::UHDR_CR_UNSPECIFIED;
        use sys::uhdr_color_transfer::UHDR_CT_UNSPECIFIED;
        (
            match self.gamut {
                Some(gamut) => gamut.to_sys(),
                None => UHDR_CG_UNSPECIFIED,
            },
            match self.transfer {
                Some(transfer) => transfer.to_sys(),
                None => UHDR_CT_UNSPECIFIED,
            },
            match self.range {
                Some(range) => range.to_sys(),
                None => UHDR_CR_UNSPECIFIED,
            },
        )
    }

    pub(crate) fn from_sys(
        gamut: sys::uhdr_color_gamut_t,
        transfer: sys::uhdr_color_transfer_t,
        range: sys::uhdr_color_range_t,
    ) -> Self {
        Self {
            gamut: ColorGamut::try_from(gamut).ok(),
            transfer: ColorTransfer::try_from(transfer).ok(),
            range: ColorRange::try_from(range).ok(),
        }
    }
}

/// Intent a raw or compressed image is registered under.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum ImageLabel {
    /// HDR rendition, fed to the encoder to derive the gain map.
    Hdr,
    /// SDR rendition, fed to the encoder to derive the gain map.
    Sdr,
    /// Base (SDR) rendition of an already compressed UltraHDR stream.
    Base,
    /// Gain map of an already compressed UltraHDR stream.
    GainMap,
}

impl ImageLabel {
    pub(crate) const fn to_sys(self) -> sys::uhdr_img_label_t {
        use sys::uhdr_img_label::*;
        match self {
            Self::Hdr => UHDR_HDR_IMG,
            Self::Sdr => UHDR_SDR_IMG,
            Self::Base => UHDR_BASE_IMG,
            Self::GainMap => UHDR_GAIN_MAP_IMG,
        }
    }
}

impl From<ImageLabel> for sys::uhdr_img_label_t {
    fn from(value: ImageLabel) -> Self {
        value.to_sys()
    }
}

/// Output format produced by [`Encoder::set_output_format`](crate::Encoder::set_output_format).
///
/// Only JPEG is available. HEIF/HEIC and AVIF are implemented in libheif, which is LGPL-3.0 and
/// therefore not linked into this crate; see the `heif` note in the README. The enum stays
/// `#[non_exhaustive]` so a container format can be added if a compatibly licensed implementation
/// appears.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum Codec {
    /// Single JPEG holding the SDR base image plus the gain map.
    Jpeg,
}

impl Codec {
    pub(crate) const fn to_sys(self) -> sys::uhdr_codec_t {
        use sys::uhdr_codec::*;
        match self {
            Self::Jpeg => UHDR_CODEC_JPG,
        }
    }
}

impl From<Codec> for sys::uhdr_codec_t {
    fn from(value: Codec) -> Self {
        value.to_sys()
    }
}

/// Encoder tuning preset.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum Preset {
    /// Favour encoding speed.
    Realtime,
    /// Favour image quality.
    BestQuality,
}

impl Preset {
    pub(crate) const fn to_sys(self) -> sys::uhdr_enc_preset_t {
        use sys::uhdr_enc_preset::*;
        match self {
            Self::Realtime => UHDR_USAGE_REALTIME,
            Self::BestQuality => UHDR_USAGE_BEST_QUALITY,
        }
    }
}

impl From<Preset> for sys::uhdr_enc_preset_t {
    fn from(value: Preset) -> Self {
        value.to_sys()
    }
}

/// An uncompressed image or gain map.
///
/// The buffer is owned by this type; `RawImage` can be built from scratch ([`RawImage::new`]) or
/// wrap existing bytes ([`RawImage::from_packed`], [`RawImage::from_planes`]). The encoder copies
/// the pixels when the image is registered, so a `RawImage` may be dropped right after
/// [`Encoder::set_raw_image`](crate::Encoder::set_raw_image).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RawImage {
    format: PixelFormat,
    width: u32,
    height: u32,
    aspects: ColorAspects,
    planes: [Option<Vec<u8>>; 3],
    strides: [u32; 3],
}

impl RawImage {
    /// Allocate a zeroed image with natural (tightly packed) strides.
    ///
    /// Every plane the [`PixelFormat`] uses is allocated; fill them through [`RawImage::data_mut`]
    /// or [`RawImage::plane_mut`].
    pub fn new(
        format: PixelFormat,
        width: u32,
        height: u32,
        aspects: ColorAspects,
    ) -> Result<Self> {
        let geometry = format
            .geometry()
            .ok_or_else(|| Error::invalid_parameter("unsupported pixel format"))?;
        let mut planes: [Option<Vec<u8>>; 3] = [None, None, None];
        let mut strides = [0u32; 3];
        for (index, geom) in geometry.iter().enumerate() {
            if let Some(geom) = geom {
                let stride = geom.min_stride(width);
                planes[index] = Some(vec![0u8; geom.required_len(width, height, stride)?]);
                strides[index] = stride;
            }
        }
        Ok(Self {
            format,
            width,
            height,
            aspects,
            planes,
            strides,
        })
    }

    /// Wrap a single, tightly packed buffer.
    ///
    /// Returns an error for planar formats (use [`RawImage::from_planes`]) or when `data` is
    /// shorter than the format requires.
    pub fn from_packed(
        format: PixelFormat,
        width: u32,
        height: u32,
        data: impl Into<Vec<u8>>,
        aspects: ColorAspects,
    ) -> Result<Self> {
        let geometry = format
            .geometry()
            .ok_or_else(|| Error::invalid_parameter("unsupported pixel format"))?;
        let [Some(geom), None, None] = geometry else {
            return Err(Error::invalid_parameter(
                "format is planar; use `RawImage::from_planes` instead",
            ));
        };
        let data = data.into();
        let stride = geom.min_stride(width);
        ensure_capacity(
            &data,
            geom.required_len(width, height, stride)?,
            "packed plane",
        )?;
        Ok(Self {
            format,
            width,
            height,
            aspects,
            planes: [Some(data), None, None],
            strides: [stride, 0, 0],
        })
    }

    /// Wrap caller-provided planes with explicit strides, in samples of each plane.
    ///
    /// This is the escape hatch for padded buffers and for formats with more than one plane. Each
    /// present plane must be at least `stride * bytes_per_sample * ceil(height / rows_div)` bytes
    /// and `stride` must be at least the plane's minimum stride.
    pub fn from_planes(
        format: PixelFormat,
        width: u32,
        height: u32,
        planes: [Option<Vec<u8>>; 3],
        strides: [u32; 3],
        aspects: ColorAspects,
    ) -> Result<Self> {
        let geometry = format
            .geometry()
            .ok_or_else(|| Error::invalid_parameter("unsupported pixel format"))?;
        for (index, (plane, geom)) in planes.iter().zip(geometry).enumerate() {
            match (plane, geom) {
                (Some(data), Some(geom)) => {
                    ensure_capacity(
                        data,
                        geom.required_len(width, height, strides[index])?,
                        "plane",
                    )?;
                }
                (Some(_), None) => {
                    return Err(Error::invalid_parameter(format!(
                        "plane {index} is not part of {format:?}"
                    )));
                }
                (None, Some(_)) => {
                    return Err(Error::invalid_parameter(format!(
                        "plane {index} is required by {format:?}"
                    )));
                }
                (None, None) => {}
            }
        }
        if planes.iter().all(Option::is_none) {
            return Err(Error::invalid_parameter("no planes supplied"));
        }
        Ok(Self {
            format,
            width,
            height,
            aspects,
            planes,
            strides,
        })
    }

    /// Wrap planar 8-bit 4:2:0 planes with natural strides.
    pub fn yuv420(
        width: u32,
        height: u32,
        y: impl Into<Vec<u8>>,
        u: impl Into<Vec<u8>>,
        v: impl Into<Vec<u8>>,
        aspects: ColorAspects,
    ) -> Result<Self> {
        Self::from_planes(
            PixelFormat::Yuv420,
            width,
            height,
            [Some(y.into()), Some(u.into()), Some(v.into())],
            [width, width.div_ceil(2), width.div_ceil(2)],
            aspects,
        )
    }

    /// Wrap 10-bit semi-planar P010 planes (luma plus interleaved UV) with natural strides.
    pub fn p010(
        width: u32,
        height: u32,
        y: impl Into<Vec<u8>>,
        uv: impl Into<Vec<u8>>,
        aspects: ColorAspects,
    ) -> Result<Self> {
        Self::from_planes(
            PixelFormat::P010,
            width,
            height,
            [Some(y.into()), Some(uv.into()), None],
            [width, width, 0],
            aspects,
        )
    }

    /// Logical width in pixels.
    pub fn width(&self) -> u32 {
        self.width
    }

    /// Logical height in pixels.
    pub fn height(&self) -> u32 {
        self.height
    }

    /// Pixel layout.
    pub fn format(&self) -> PixelFormat {
        self.format
    }

    /// Colour description.
    pub fn aspects(&self) -> ColorAspects {
        self.aspects
    }

    /// Replace the colour description.
    pub fn set_aspects(&mut self, aspects: ColorAspects) {
        self.aspects = aspects;
    }

    /// Number of planes this image holds.
    pub fn plane_count(&self) -> usize {
        self.planes.iter().filter(|plane| plane.is_some()).count()
    }

    /// Stride of `plane` in samples, if the plane is present.
    pub fn stride(&self, plane: Plane) -> Option<u32> {
        let index = plane.index();
        self.planes[index].as_ref().map(|_| self.strides[index])
    }

    /// Borrow `plane`, if the format uses it.
    pub fn plane(&self, plane: Plane) -> Option<&[u8]> {
        self.planes[plane.index()].as_deref()
    }

    /// Mutably borrow `plane`, if the format uses it.
    pub fn plane_mut(&mut self, plane: Plane) -> Option<&mut [u8]> {
        self.planes[plane.index()].as_deref_mut()
    }

    /// Borrow the first plane: the packed plane of interleaved formats, the luma plane of planar
    /// ones.
    pub fn data(&self) -> Option<&[u8]> {
        self.plane(Plane::Y)
    }

    /// Mutably borrow the first plane, as in [`data`](Self::data).
    pub fn data_mut(&mut self) -> Option<&mut [u8]> {
        self.plane_mut(Plane::Y)
    }

    /// Build the C descriptor, refreshing plane pointers (buffers may have moved with `self`).
    ///
    /// The C API only reads raw image data (it deep-copies on registration), so handing out
    /// `*mut` pointers derived from shared borrows cannot be observed as mutation.
    pub(crate) fn as_sys(&self) -> sys::uhdr_raw_image {
        let (cg, ct, range) = self.aspects.to_sys();
        let mut planes = [std::ptr::null_mut(); 3];
        for (slot, buffer) in planes.iter_mut().zip(self.planes.iter()) {
            if let Some(buffer) = buffer {
                *slot = buffer.as_ptr() as *mut c_void;
            }
        }
        sys::uhdr_raw_image {
            fmt: self.format.to_sys(),
            cg,
            ct,
            range,
            w: self.width,
            h: self.height,
            planes,
            stride: self.strides,
        }
    }
}

fn ensure_capacity(buffer: &[u8], required: usize, what: &str) -> Result<()> {
    if buffer.len() < required {
        return Err(Error::invalid_parameter(format!(
            "{what} holds {} bytes but {required} are required",
            buffer.len()
        )));
    }
    Ok(())
}

/// A compressed JPEG image, either as encoder input or the result of encoding.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompressedImage {
    data: Vec<u8>,
    aspects: ColorAspects,
}

impl CompressedImage {
    /// Wrap an encoded stream with unspecified colour aspects.
    pub fn new(data: impl Into<Vec<u8>>) -> Self {
        Self {
            data: data.into(),
            aspects: ColorAspects::UNSPECIFIED,
        }
    }

    /// Wrap an encoded stream together with its colour description.
    pub fn with_aspects(data: impl Into<Vec<u8>>, aspects: ColorAspects) -> Self {
        Self {
            data: data.into(),
            aspects,
        }
    }

    /// The encoded bytes.
    pub fn bytes(&self) -> &[u8] {
        &self.data
    }

    /// Number of encoded bytes.
    pub fn len(&self) -> usize {
        self.data.len()
    }

    /// Whether the stream is empty.
    pub fn is_empty(&self) -> bool {
        self.data.is_empty()
    }

    /// Colour description of the stream.
    pub fn aspects(&self) -> ColorAspects {
        self.aspects
    }

    /// Replace the colour description.
    pub fn set_aspects(&mut self, aspects: ColorAspects) {
        self.aspects = aspects;
    }

    /// Build the C descriptor. The library copies the stream on registration.
    pub(crate) fn as_sys(&self) -> sys::uhdr_compressed_image {
        let (cg, ct, range) = self.aspects.to_sys();
        sys::uhdr_compressed_image {
            data: self.data.as_ptr() as *mut c_void,
            data_sz: self.data.len(),
            capacity: self.data.len(),
            cg,
            ct,
            range,
        }
    }
}

/// Owned encoded stream returned by [`Encoder::encoded_stream`](crate::Encoder::encoded_stream).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EncodedImage {
    /// The encoded bytes.
    pub data: Vec<u8>,
    /// Colour description attached to the stream.
    pub aspects: ColorAspects,
}

/// Borrowed view over an encoded stream owned by an [`Encoder`](crate::Encoder).
#[derive(Debug)]
pub struct EncodedView<'a> {
    image: &'a mut sys::uhdr_compressed_image,
}

impl<'a> EncodedView<'a> {
    pub(crate) fn new(image: &'a mut sys::uhdr_compressed_image) -> Self {
        Self { image }
    }

    /// The encoded bytes.
    pub fn bytes(&self) -> &'a [u8] {
        // SAFETY: libultrahdr owns the buffer for as long as the codec instance lives, and the
        // view borrows the codec.
        unsafe { std::slice::from_raw_parts(self.image.data as *const u8, self.image.data_sz) }
    }

    /// Colour description of the stream.
    pub fn aspects(&self) -> ColorAspects {
        ColorAspects::from_sys(self.image.cg, self.image.ct, self.image.range)
    }

    /// Copy the stream out of the codec.
    pub fn to_owned_image(&self) -> EncodedImage {
        EncodedImage {
            data: self.bytes().to_vec(),
            aspects: self.aspects(),
        }
    }
}

/// Owned packed pixels returned by [`Decoder`](crate::Decoder).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DecodedImage {
    /// Pixel layout, always one of the packed formats.
    pub format: PixelFormat,
    /// Colour description of the decoded pixels.
    pub aspects: ColorAspects,
    /// Width in pixels.
    pub width: u32,
    /// Height in pixels.
    pub height: u32,
    /// Tightly packed pixels, `width * height * bytes_per_pixel` bytes long.
    pub data: Vec<u8>,
}

impl DecodedImage {
    /// Iterate over the rows of the image, without padding.
    pub fn rows(&self) -> impl Iterator<Item = &[u8]> {
        let row_bytes = (self.width as usize)
            * self
                .format
                .bytes_per_pixel()
                .expect("decoded images are packed");
        self.data.chunks_exact(row_bytes)
    }

    /// Re-wrap the pixels as a [`RawImage`], e.g. to feed them to an encoder.
    ///
    /// The pixels are moved, not copied. Aspects the stream did not signal stay `None`, which the
    /// encoder rejects for raw input, so fill them in first (see [`ColorAspects`]).
    pub fn into_raw_image(self) -> Result<RawImage> {
        let Self {
            format,
            aspects,
            width,
            height,
            data,
        } = self;
        RawImage::from_packed(format, width, height, data, aspects)
    }
}

/// Borrowed view over packed pixels owned by a [`Decoder`](crate::Decoder).
#[derive(Debug)]
pub struct DecodedView<'a> {
    image: &'a mut sys::uhdr_raw_image,
    format: PixelFormat,
    bpp: usize,
}

impl<'a> DecodedView<'a> {
    pub(crate) fn new(image: &'a mut sys::uhdr_raw_image) -> Result<Self> {
        let format = PixelFormat::try_from(image.fmt)?;
        let bpp = format
            .bytes_per_pixel()
            .ok_or_else(|| Error::invalid_parameter("decoded images must be packed"))?;
        Ok(Self { image, format, bpp })
    }

    /// Logical width in pixels.
    pub fn width(&self) -> u32 {
        self.image.w
    }

    /// Logical height in pixels.
    pub fn height(&self) -> u32 {
        self.image.h
    }

    /// Pixel layout of the decoded pixels.
    pub fn format(&self) -> PixelFormat {
        self.format
    }

    /// Colour description of the decoded pixels.
    pub fn aspects(&self) -> ColorAspects {
        ColorAspects::from_sys(self.image.cg, self.image.ct, self.image.range)
    }

    /// Replace the colour description of the decoded pixels.
    ///
    /// Useful when a stream does not signal its colour aspects and the caller knows them from
    /// elsewhere (an ICC profile, for example).
    pub fn set_aspects(&mut self, aspects: ColorAspects) {
        let (cg, ct, range) = aspects.to_sys();
        self.image.cg = cg;
        self.image.ct = ct;
        self.image.range = range;
    }

    /// The C descriptor, for handing the pixels to an encoder without copying them.
    pub(crate) fn as_sys(&self) -> sys::uhdr_raw_image {
        *self.image
    }

    /// Borrow a single row, without any row padding.
    pub fn row(&self, y: usize) -> Result<&'a [u8]> {
        if y as u32 >= self.image.h {
            return Err(Error::invalid_parameter(format!(
                "row {y} is outside the image (height {})",
                self.image.h
            )));
        }
        let stride = self.image.stride[Plane::Y.index()] as usize;
        let start = y * stride * self.bpp;
        let end = start + self.width() as usize * self.bpp;
        let plane = self.image.planes[Plane::Y.index()] as *const u8;
        // SAFETY: the packed plane is `stride * height * bpp` bytes (libultrahdr guarantees
        // `stride >= width`), so `start..end` is in bounds for a valid row index.
        Ok(unsafe { std::slice::from_raw_parts(plane.add(start), end - start) })
    }

    /// Iterate over the rows of the decoded image, without row padding.
    pub fn rows(&self) -> impl Iterator<Item = Result<&'a [u8]>> + '_ {
        (0..self.height()).map(move |y| self.row(y as usize))
    }

    /// Copy the pixels out of the codec into a tightly packed buffer.
    pub fn to_owned_image(&self) -> DecodedImage {
        let width = self.width() as usize;
        let height = self.height() as usize;
        let bpp = self.bpp;
        let stride = self.image.stride[Plane::Y.index()] as usize;
        let plane = self.image.planes[Plane::Y.index()] as *const u8;
        // SAFETY: as in `row`, the plane holds `stride * height * bpp` readable bytes.
        let source = unsafe { std::slice::from_raw_parts(plane, stride * height * bpp) };
        let mut data = Vec::with_capacity(width * height * bpp);
        for y in 0..height {
            let start = y * stride * bpp;
            data.extend_from_slice(&source[start..start + width * bpp]);
        }
        DecodedImage {
            format: self.format,
            aspects: self.aspects(),
            width: width as u32,
            height: height as u32,
            data,
        }
    }
}

/// Borrowed view over a length-delimited byte block owned by libultrahdr.
///
/// Used for the EXIF payload, the ICC profile and the compressed parts of a decoded stream.
#[derive(Debug)]
pub struct MemBlockView<'a> {
    data: &'a [u8],
    capacity: usize,
}

impl<'a> MemBlockView<'a> {
    pub(crate) fn new(data: *const u8, len: usize, capacity: usize) -> Result<Self> {
        if data.is_null() {
            return Err(Error::invalid_parameter("null data pointer"));
        }
        if len > capacity {
            return Err(Error::invalid_parameter(format!(
                "length {len} exceeds capacity {capacity}"
            )));
        }
        // SAFETY: the caller guarantees `data..data + len` is readable for the borrow.
        Ok(Self {
            data: unsafe { std::slice::from_raw_parts(data, len) },
            capacity,
        })
    }

    /// Size of the allocation backing the block, in bytes; always `>= len()`.
    pub fn capacity(&self) -> usize {
        self.capacity
    }

    /// Borrowed bytes.
    pub fn bytes(&self) -> &'a [u8] {
        self.data
    }

    /// Number of bytes.
    pub fn len(&self) -> usize {
        self.data.len()
    }

    /// Whether the block is empty.
    pub fn is_empty(&self) -> bool {
        self.data.is_empty()
    }

    /// Copy the bytes out of libultrahdr's buffer.
    pub fn to_vec(&self) -> Vec<u8> {
        self.data.to_vec()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const ASPECTS: ColorAspects =
        ColorAspects::new(ColorGamut::DisplayP3, ColorTransfer::Pq, ColorRange::Full);

    #[test]
    fn packed_formats_report_their_layout() {
        assert_eq!(PixelFormat::Rgba8888.bytes_per_pixel(), Some(4));
        assert_eq!(PixelFormat::Rgba1010102.bytes_per_pixel(), Some(4));
        assert_eq!(PixelFormat::RgbaHalfFloat.bytes_per_pixel(), Some(8));
        assert_eq!(PixelFormat::Rgb888.bytes_per_pixel(), Some(3));
        assert_eq!(PixelFormat::Mono8.bytes_per_pixel(), Some(1));
        // P010 is semi-planar: luma plus an interleaved UV plane.
        assert_eq!(PixelFormat::P010.bytes_per_pixel(), None);
        assert_eq!(PixelFormat::P010.plane_count(), 2);
        assert!(!PixelFormat::P010.is_packed());

        for planar in [
            PixelFormat::Yuv420,
            PixelFormat::Yuv444,
            PixelFormat::Yuv422,
            PixelFormat::Yuv440,
            PixelFormat::Yuv411,
            PixelFormat::Yuv410,
            PixelFormat::Yuv444P10,
        ] {
            assert_eq!(planar.bytes_per_pixel(), None, "{planar:?} is planar");
            assert_eq!(planar.plane_count(), 3, "{planar:?} uses three planes");
            assert!(!planar.is_packed());
        }
        assert!(PixelFormat::Rgba8888.is_packed());
        assert_eq!(PixelFormat::Rgba8888.plane_count(), 1);
    }

    #[test]
    fn new_allocates_the_geometry_of_the_format() {
        let image = RawImage::new(PixelFormat::Yuv420, 8, 6, ASPECTS).unwrap();
        assert_eq!(image.plane(Plane::Y).unwrap().len(), 8 * 6);
        assert_eq!(image.plane(Plane::U).unwrap().len(), 4 * 3);
        assert_eq!(image.stride(Plane::Y), Some(8));
        assert_eq!(image.stride(Plane::U), Some(4));

        // P010 stores 16-bit samples in a full-rate luma plane plus one interleaved UV plane.
        let image = RawImage::new(PixelFormat::P010, 8, 6, ASPECTS).unwrap();
        assert_eq!(image.plane(Plane::Y).unwrap().len(), 8 * 6 * 2);
        assert_eq!(image.plane(Plane::U).unwrap().len(), 8 * 3 * 2);
        assert_eq!(image.plane(Plane::V), None);
        assert_eq!(image.stride(Plane::U), Some(8));

        let image = RawImage::new(PixelFormat::Rgba8888, 4, 3, ASPECTS).unwrap();
        assert_eq!(image.data().unwrap().len(), 4 * 3 * 4);
        assert_eq!(image.plane_count(), 1);
    }

    #[test]
    fn packed_constructor_validates_the_buffer_size() {
        let short = RawImage::from_packed(PixelFormat::Rgba8888, 4, 4, vec![0; 63], ASPECTS);
        assert!(matches!(short, Err(Error::InvalidParameter(_))));

        let padded = RawImage::from_packed(PixelFormat::Rgba8888, 4, 4, vec![0; 65], ASPECTS);
        assert!(padded.is_ok(), "trailing bytes are harmless");

        let planar = RawImage::from_packed(PixelFormat::Yuv420, 4, 4, vec![0; 64], ASPECTS);
        assert!(matches!(planar, Err(Error::InvalidParameter(_))));
    }

    #[test]
    fn planar_constructor_checks_every_plane() {
        let ok = RawImage::from_planes(
            PixelFormat::Yuv420,
            4,
            4,
            [Some(vec![0; 16]), Some(vec![0; 4]), Some(vec![0; 4])],
            [4, 2, 2],
            ASPECTS,
        );
        assert!(ok.is_ok());
        assert_eq!(ok.unwrap().plane_count(), 3);

        // A missing plane, an extra plane and a too-small buffer are all rejected.
        let missing = RawImage::from_planes(
            PixelFormat::Yuv420,
            4,
            4,
            [Some(vec![0; 16]), Some(vec![0; 4]), None],
            [4, 2, 2],
            ASPECTS,
        );
        assert!(matches!(missing, Err(Error::InvalidParameter(_))));

        let extra = RawImage::from_planes(
            PixelFormat::Rgba8888,
            4,
            4,
            [Some(vec![0; 64]), Some(vec![0; 4]), None],
            [4, 2, 0],
            ASPECTS,
        );
        assert!(matches!(extra, Err(Error::InvalidParameter(_))));

        let short_stride = RawImage::from_planes(
            PixelFormat::Yuv420,
            4,
            4,
            [Some(vec![0; 16]), Some(vec![0; 4]), Some(vec![0; 4])],
            [4, 1, 2],
            ASPECTS,
        );
        assert!(matches!(short_stride, Err(Error::InvalidParameter(_))));

        let short_buffer = RawImage::from_planes(
            PixelFormat::Yuv420,
            4,
            4,
            [Some(vec![0; 16]), Some(vec![0; 3]), Some(vec![0; 4])],
            [4, 2, 2],
            ASPECTS,
        );
        assert!(matches!(short_buffer, Err(Error::InvalidParameter(_))));

        let empty = RawImage::from_planes(
            PixelFormat::Yuv420,
            4,
            4,
            [None, None, None],
            [0; 3],
            ASPECTS,
        );
        assert!(matches!(empty, Err(Error::InvalidParameter(_))));
    }

    #[test]
    fn planar_shorthands_use_natural_strides() {
        let image = RawImage::yuv420(4, 4, vec![0; 16], vec![0; 4], vec![0; 4], ASPECTS).unwrap();
        assert_eq!(image.format(), PixelFormat::Yuv420);
        assert_eq!(image.stride(Plane::Y), Some(4));
        assert_eq!(image.stride(Plane::V), Some(2));

        let err = RawImage::yuv420(4, 4, vec![0; 16], vec![0; 1], vec![0; 4], ASPECTS).unwrap_err();
        assert!(matches!(err, Error::InvalidParameter(_)));

        let image = RawImage::p010(4, 4, vec![0; 32], vec![0; 16], ASPECTS).unwrap();
        assert_eq!(image.format(), PixelFormat::P010);
        assert_eq!(image.stride(Plane::U), Some(4));
        assert_eq!(image.plane(Plane::U).unwrap().len(), 16);
    }

    #[test]
    fn aspects_map_to_the_c_unspecified_values() {
        let (cg, ct, range) = ColorAspects::UNSPECIFIED.to_sys();
        assert_eq!(cg, sys::uhdr_color_gamut_t::UHDR_CG_UNSPECIFIED);
        assert_eq!(ct, sys::uhdr_color_transfer_t::UHDR_CT_UNSPECIFIED);
        assert_eq!(range, sys::uhdr_color_range_t::UHDR_CR_UNSPECIFIED);

        let aspects = ColorAspects::default()
            .with_gamut(ColorGamut::Bt2100)
            .with_range(ColorRange::Limited);
        let (cg, ct, range) = aspects.to_sys();
        assert_eq!(cg, sys::uhdr_color_gamut_t::UHDR_CG_BT_2100);
        assert_eq!(ct, sys::uhdr_color_transfer_t::UHDR_CT_UNSPECIFIED);
        assert_eq!(range, sys::uhdr_color_range_t::UHDR_CR_LIMITED_RANGE);

        let round_tripped = ColorAspects::from_sys(cg, ct, range);
        assert_eq!(round_tripped.gamut, Some(ColorGamut::Bt2100));
        assert_eq!(round_tripped.transfer, None);
        assert_eq!(round_tripped.range, Some(ColorRange::Limited));
    }

    #[test]
    fn decoded_image_round_trips_into_a_raw_image() {
        let decoded = DecodedImage {
            format: PixelFormat::Rgba8888,
            aspects: ASPECTS,
            width: 2,
            height: 2,
            data: vec![7u8; 16],
        };
        let raw = decoded.into_raw_image().unwrap();
        assert_eq!(raw.width(), 2);
        assert_eq!(raw.format(), PixelFormat::Rgba8888);
        assert_eq!(raw.data().unwrap(), &[7u8; 16]);

        let broken = DecodedImage {
            format: PixelFormat::Rgba8888,
            aspects: ASPECTS,
            width: 2,
            height: 2,
            data: vec![0u8; 15],
        };
        assert!(matches!(
            broken.into_raw_image(),
            Err(Error::InvalidParameter(_))
        ));
    }

    #[test]
    fn mem_block_view_validates_bounds() {
        let data = [1u8, 2, 3, 4];
        let block = MemBlockView::new(data.as_ptr(), 4, 4).unwrap();
        assert_eq!(block.bytes(), &data);
        assert_eq!(block.len(), 4);
        assert!(!block.is_empty());
        assert_eq!(block.to_vec(), data.to_vec());

        assert!(MemBlockView::new(std::ptr::null(), 0, 0).is_err());
        assert!(MemBlockView::new(data.as_ptr(), 5, 4).is_err());
        let empty = MemBlockView::new(data.as_ptr(), 0, 4).unwrap();
        assert!(empty.is_empty());
    }
}
