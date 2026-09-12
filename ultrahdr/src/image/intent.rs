//! Encoder-facing enums: registration intents, output container and tuning preset.

use crate::sys;

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
