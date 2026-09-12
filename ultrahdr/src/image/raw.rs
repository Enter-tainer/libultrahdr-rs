//! The owning uncompressed image type.

use crate::error::{Error, Result};
use crate::image::aspects::ColorAspects;
use crate::image::format::{PixelFormat, Plane};
use crate::sys;
use std::ffi::c_void;

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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::image::aspects::{ColorGamut, ColorRange, ColorTransfer};

    const ASPECTS: ColorAspects =
        ColorAspects::new(ColorGamut::DisplayP3, ColorTransfer::Pq, ColorRange::Full);

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
}
