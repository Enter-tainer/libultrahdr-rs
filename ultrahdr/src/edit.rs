//! Geometric transforms that can be added to an encoder or decoder.
//!
//! The C API names these "effects" and keeps them on the shared codec handle, so the same helpers
//! are used by both [`Encoder`](crate::Encoder) and [`Decoder`](crate::Decoder). Effects are applied
//! in the order they are added, after decoding and before encoding respectively. They may be
//! registered any time before the codec runs ([`Decoder::probe`](crate::Decoder::probe) does not
//! lock them); see the method documentation for the restrictions that apply to each direction.

use crate::error::{Error, Result, check};
use crate::sys;
use std::ptr::NonNull;

pub(crate) type RawCodec = NonNull<sys::uhdr_codec_private_t>;

/// Mirror axis for the `mirror` effect.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum Mirror {
    /// Flip the image over the x axis (top/bottom swapped).
    Vertical,
    /// Flip the image over the y axis (left/right swapped).
    Horizontal,
}

impl Mirror {
    pub(crate) const fn to_sys(self) -> sys::uhdr_mirror_direction_t {
        match self {
            Self::Vertical => sys::uhdr_mirror_direction_t::UHDR_MIRROR_VERTICAL,
            Self::Horizontal => sys::uhdr_mirror_direction_t::UHDR_MIRROR_HORIZONTAL,
        }
    }
}

impl From<Mirror> for sys::uhdr_mirror_direction_t {
    fn from(value: Mirror) -> Self {
        value.to_sys()
    }
}

/// Clockwise rotation angle for the `rotate` effect.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum Rotation {
    /// Rotate clockwise by 90 degrees.
    Deg90,
    /// Rotate clockwise by 180 degrees.
    Deg180,
    /// Rotate clockwise by 270 degrees.
    Deg270,
}

impl Rotation {
    /// Rotation angle in degrees, as expected by the C API.
    pub const fn degrees(self) -> i32 {
        match self {
            Self::Deg90 => 90,
            Self::Deg180 => 180,
            Self::Deg270 => 270,
        }
    }
}

/// Crop window in absolute, exclusive pixel coordinates.
///
/// The rectangle selects the region `left..right` × `top..bottom` of the image, so the cropped
/// output is [`width`](Self::width) × [`height`](Self::height) pixels. Coordinates beyond the image
/// bounds are clamped by libultrahdr, but an empty or inverted rectangle is rejected eagerly when
/// the effect is added.
///
/// ```
/// use ultrahdr::CropRect;
///
/// let rect = CropRect::new(2, 3, 10, 11);
/// assert_eq!(rect.width(), Some(8));
/// assert_eq!(rect.height(), Some(8));
/// assert_eq!(CropRect::new(5, 0, 4, 8).width(), None, "inverted rectangle");
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct CropRect {
    /// First retained column.
    pub left: u32,
    /// First retained row.
    pub top: u32,
    /// One past the last retained column.
    pub right: u32,
    /// One past the last retained row.
    pub bottom: u32,
}

impl CropRect {
    /// Create a rectangle from its exclusive bounds.
    pub const fn new(left: u32, top: u32, right: u32, bottom: u32) -> Self {
        Self {
            left,
            top,
            right,
            bottom,
        }
    }

    /// Width of the cropped region in pixels; `None` if the rectangle is inverted.
    ///
    /// An empty rectangle (`right == left`) yields `Some(0)` here but is still rejected when the
    /// crop effect is added.
    pub const fn width(&self) -> Option<u32> {
        self.right.checked_sub(self.left)
    }

    /// Height of the cropped region in pixels; `None` if the rectangle is inverted.
    ///
    /// An empty rectangle (`bottom == top`) yields `Some(0)` here but is still rejected when the
    /// crop effect is added.
    pub const fn height(&self) -> Option<u32> {
        self.bottom.checked_sub(self.top)
    }

    /// The C `(left, right, top, bottom)` coordinates, after validating the rectangle.
    pub(crate) fn to_sys(self) -> Result<(i32, i32, i32, i32)> {
        if self.right <= self.left || self.bottom <= self.top {
            return Err(Error::invalid_parameter(format!(
                "crop rectangle is empty: {}..={} x {}..={}",
                self.left, self.right, self.top, self.bottom
            )));
        }
        let limit = i32::MAX as u32;
        for (name, value) in [
            ("left", self.left),
            ("top", self.top),
            ("right", self.right),
            ("bottom", self.bottom),
        ] {
            if value > limit {
                return Err(Error::invalid_parameter(format!(
                    "crop {name} coordinate {value} exceeds the supported image size"
                )));
            }
        }
        Ok((
            self.left as i32,
            self.right as i32,
            self.top as i32,
            self.bottom as i32,
        ))
    }
}

/// Enable/disable GPU acceleration. A no-op unless the library was built with GLES support.
pub(crate) fn enable_gpu_acceleration(raw: RawCodec, enable: bool) -> Result<()> {
    check(unsafe { sys::uhdr_enable_gpu_acceleration(raw.as_ptr(), i32::from(enable)) })
}

pub(crate) fn add_effect_mirror(raw: RawCodec, direction: Mirror) -> Result<()> {
    check(unsafe { sys::uhdr_add_effect_mirror(raw.as_ptr(), direction.to_sys()) })
}

pub(crate) fn add_effect_rotate(raw: RawCodec, rotation: Rotation) -> Result<()> {
    check(unsafe { sys::uhdr_add_effect_rotate(raw.as_ptr(), rotation.degrees()) })
}

pub(crate) fn add_effect_crop(raw: RawCodec, rect: CropRect) -> Result<()> {
    let (left, right, top, bottom) = rect.to_sys()?;
    check(unsafe { sys::uhdr_add_effect_crop(raw.as_ptr(), left, right, top, bottom) })
}

pub(crate) fn add_effect_resize(raw: RawCodec, width: u32, height: u32) -> Result<()> {
    let limit = i32::MAX as u32;
    if width == 0 || height == 0 {
        return Err(Error::invalid_parameter(
            "resize target must be at least one pixel",
        ));
    }
    if width > limit || height > limit {
        return Err(Error::invalid_parameter(format!(
            "resize target {width}x{height} exceeds the supported image size"
        )));
    }
    check(unsafe { sys::uhdr_add_effect_resize(raw.as_ptr(), width as i32, height as i32) })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn crop_rect_reports_its_extent() {
        let rect = CropRect::new(2, 3, 10, 11);
        assert_eq!(rect.width(), Some(8));
        assert_eq!(rect.height(), Some(8));
        assert_eq!(rect.to_sys().unwrap(), (2, 10, 3, 11));

        let full = CropRect::new(0, 0, 1920, 1080);
        assert_eq!(full.to_sys().unwrap(), (0, 1920, 0, 1080));
    }

    #[test]
    fn empty_or_inverted_rectangles_are_rejected() {
        for rect in [
            CropRect::new(4, 0, 4, 8),
            CropRect::new(5, 0, 4, 8),
            CropRect::new(0, 4, 8, 4),
            CropRect::new(0, 5, 8, 4),
        ] {
            assert!(
                matches!(rect.to_sys(), Err(Error::InvalidParameter(_))),
                "{rect:?} must be rejected"
            );
        }
    }

    #[test]
    fn oversized_coordinates_are_rejected() {
        let rect = CropRect::new(0, 0, u32::MAX, 8);
        assert!(matches!(rect.to_sys(), Err(Error::InvalidParameter(_))));
    }
}
