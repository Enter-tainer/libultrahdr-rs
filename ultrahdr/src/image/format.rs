//! Pixel layouts and the decoder output profiles built on them.

use crate::error::{Error, Result};
use crate::sys;

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
    pub(crate) fn min_stride(self, width: u32) -> u32 {
        width.div_ceil(self.cols_div)
    }

    /// Bytes required for `height` rows of `stride` samples.
    pub(crate) fn required_len(self, width: u32, height: u32, stride: u32) -> Result<usize> {
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

/// Fully specified combination of decoded pixel format and transfer function.
///
/// libultrahdr only accepts four output pairings — every other combination of
/// [`PixelFormat`] and [`ColorTransfer`](crate::ColorTransfer) is rejected at decode time with
/// `UHDR_CODEC_INVALID_PARAM`. Bundling the two into one enum makes the invalid combinations
/// unrepresentable, so a decoder configured through this type cannot fail on the pairing.
///
/// This is the output-side counterpart of [`Codec`](crate::Codec), which selects the *encoded*
/// container.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
#[non_exhaustive]
pub enum DecodedOutput {
    /// 16-bit half-float RGBA carrying linear light (`RgbaHalfFloat` + `Linear`).
    ///
    /// The default output of a fresh decoder, matching `uhdr_reset_decoder`.
    #[default]
    LinearF16,
    /// 10-bit RGBA1010102 carrying SMPTE ST 2084 PQ (`Rgba1010102` + `Pq`).
    Pq1010102,
    /// 10-bit RGBA1010102 carrying hybrid log-gamma (`Rgba1010102` + `Hlg`).
    Hlg1010102,
    /// 8-bit RGBA8888 carrying sRGB (`Rgba8888` + `Srgb`).
    Srgb8888,
}

impl DecodedOutput {
    /// The pixel layout of this output.
    pub const fn format(self) -> PixelFormat {
        match self {
            Self::LinearF16 => PixelFormat::RgbaHalfFloat,
            Self::Pq1010102 | Self::Hlg1010102 => PixelFormat::Rgba1010102,
            Self::Srgb8888 => PixelFormat::Rgba8888,
        }
    }

    /// The transfer function of this output.
    pub const fn transfer(self) -> crate::image::aspects::ColorTransfer {
        use crate::image::aspects::ColorTransfer;
        match self {
            Self::LinearF16 => ColorTransfer::Linear,
            Self::Pq1010102 => ColorTransfer::Pq,
            Self::Hlg1010102 => ColorTransfer::Hlg,
            Self::Srgb8888 => ColorTransfer::Srgb,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
    fn decoded_output_pairs_format_and_transfer() {
        use crate::ColorTransfer;

        assert_eq!(
            DecodedOutput::LinearF16.format(),
            PixelFormat::RgbaHalfFloat
        );
        assert_eq!(DecodedOutput::LinearF16.transfer(), ColorTransfer::Linear);
        assert_eq!(DecodedOutput::Pq1010102.format(), PixelFormat::Rgba1010102);
        assert_eq!(DecodedOutput::Pq1010102.transfer(), ColorTransfer::Pq);
        assert_eq!(DecodedOutput::Hlg1010102.format(), PixelFormat::Rgba1010102);
        assert_eq!(DecodedOutput::Hlg1010102.transfer(), ColorTransfer::Hlg);
        assert_eq!(DecodedOutput::Srgb8888.format(), PixelFormat::Rgba8888);
        assert_eq!(DecodedOutput::Srgb8888.transfer(), ColorTransfer::Srgb);

        // The default matches `uhdr_reset_decoder`'s reset state.
        assert_eq!(DecodedOutput::default(), DecodedOutput::LinearF16);

        // Every profile maps to a distinct packed format + transfer pair, and the pairing table
        // covers exactly the combinations `uhdr_decode` accepts.
        let pairs: Vec<_> = [
            DecodedOutput::LinearF16,
            DecodedOutput::Pq1010102,
            DecodedOutput::Hlg1010102,
            DecodedOutput::Srgb8888,
        ]
        .iter()
        .map(|out| (out.format(), out.transfer()))
        .collect();
        let formats: std::collections::HashSet<_> = pairs.iter().map(|(fmt, _)| *fmt).collect();
        let transfers: std::collections::HashSet<_> = pairs.iter().map(|(_, ct)| *ct).collect();
        assert_eq!(formats.len(), 3, "three distinct pixel formats");
        assert_eq!(transfers.len(), 4, "four distinct transfers");
    }
}
