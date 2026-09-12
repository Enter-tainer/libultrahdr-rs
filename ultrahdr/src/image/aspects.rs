//! Colour descriptions: gamut, transfer function and sample range.

use crate::error::{Error, Result};
use crate::sys;

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

#[cfg(test)]
mod tests {
    use super::*;

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
}
