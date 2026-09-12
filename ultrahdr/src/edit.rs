//! Geometric transforms that can be added to an encoder or decoder.
//!
//! The C API names these "effects" and keeps them on the shared codec handle, so the same helpers
//! are used by both [`Encoder`](crate::Encoder) and [`Decoder`](crate::Decoder). Effects are applied
//! in the order they are added, after decoding and before encoding respectively. See the method
//! documentation for the restrictions that apply to each direction.

use crate::error::{Result, check};
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

pub(crate) fn add_effect_crop(
    raw: RawCodec,
    left: i32,
    right: i32,
    top: i32,
    bottom: i32,
) -> Result<()> {
    check(unsafe { sys::uhdr_add_effect_crop(raw.as_ptr(), left, right, top, bottom) })
}

pub(crate) fn add_effect_resize(raw: RawCodec, width: i32, height: i32) -> Result<()> {
    check(unsafe { sys::uhdr_add_effect_resize(raw.as_ptr(), width, height) })
}
