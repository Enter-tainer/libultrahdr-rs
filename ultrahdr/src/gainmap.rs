//! Gain map metadata and the constants that describe it.

use crate::sys;

/// Luminance of SDR reference white in nits, as assumed by libultrahdr.
///
/// Multiplying a gain map capacity by this value yields the corresponding display brightness, see
/// [`GainMapMetadata::target_display_peak_nits`].
pub const SDR_WHITE_NITS: f32 = 203.0;

/// Gain map metadata carried by an UltraHDR stream (ISO 21496-1).
///
/// Obtained from [`Decoder::gainmap_metadata`](crate::Decoder::gainmap_metadata) and handed back
/// verbatim to [`Encoder::set_gainmap_image`](crate::Encoder::set_gainmap_image) when re-using a
/// pre-computed gain map.
#[derive(Debug, Clone, PartialEq)]
pub struct GainMapMetadata {
    /// Maximum per-channel gain applied by the gain map.
    pub max_content_boost: [f32; 3],
    /// Minimum per-channel gain applied by the gain map.
    pub min_content_boost: [f32; 3],
    /// Per-channel gamma used to map the base image to HDR.
    pub gamma: [f32; 3],
    /// Per-channel SDR offset.
    pub offset_sdr: [f32; 3],
    /// Per-channel HDR offset.
    pub offset_hdr: [f32; 3],
    /// Lower bound of the HDR capacity.
    pub hdr_capacity_min: f32,
    /// Upper bound of the HDR capacity.
    pub hdr_capacity_max: f32,
    /// Whether the gain map reuses the base image colour gamut.
    pub use_base_cg: bool,
}

impl GainMapMetadata {
    /// Target display peak brightness in nits (`hdr_capacity_max * SDR_WHITE_NITS`).
    pub fn target_display_peak_nits(&self) -> f32 {
        self.hdr_capacity_max * SDR_WHITE_NITS
    }

    pub(crate) fn from_sys(metadata: &sys::uhdr_gainmap_metadata) -> Self {
        Self {
            max_content_boost: metadata.max_content_boost,
            min_content_boost: metadata.min_content_boost,
            gamma: metadata.gamma,
            offset_sdr: metadata.offset_sdr,
            offset_hdr: metadata.offset_hdr,
            hdr_capacity_min: metadata.hdr_capacity_min,
            hdr_capacity_max: metadata.hdr_capacity_max,
            use_base_cg: metadata.use_base_cg != 0,
        }
    }
}

impl From<&GainMapMetadata> for sys::uhdr_gainmap_metadata {
    fn from(value: &GainMapMetadata) -> Self {
        Self {
            max_content_boost: value.max_content_boost,
            min_content_boost: value.min_content_boost,
            gamma: value.gamma,
            offset_sdr: value.offset_sdr,
            offset_hdr: value.offset_hdr,
            hdr_capacity_min: value.hdr_capacity_min,
            hdr_capacity_max: value.hdr_capacity_max,
            use_base_cg: i32::from(value.use_base_cg),
        }
    }
}

impl From<sys::uhdr_gainmap_metadata> for GainMapMetadata {
    fn from(value: sys::uhdr_gainmap_metadata) -> Self {
        Self::from_sys(&value)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn metadata_round_trips_through_the_c_layout() {
        let metadata = GainMapMetadata {
            max_content_boost: [1.0, 2.0, 3.0],
            min_content_boost: [0.5, 0.5, 0.5],
            gamma: [1.0, 1.0, 1.0],
            offset_sdr: [0.0, 0.0, 0.0],
            offset_hdr: [0.0, 0.0, 0.0],
            hdr_capacity_min: 1.0,
            hdr_capacity_max: 4.0,
            use_base_cg: true,
        };

        let raw = sys::uhdr_gainmap_metadata::from(&metadata);
        assert_eq!(raw.use_base_cg, 1);
        assert_eq!(raw.max_content_boost, metadata.max_content_boost);
        assert_eq!(GainMapMetadata::from(raw), metadata);
    }

    #[test]
    fn target_display_peak_uses_reference_white() {
        let metadata = GainMapMetadata {
            max_content_boost: [1.0; 3],
            min_content_boost: [1.0; 3],
            gamma: [1.0; 3],
            offset_sdr: [0.0; 3],
            offset_hdr: [0.0; 3],
            hdr_capacity_min: 1.0,
            hdr_capacity_max: 2.0,
            use_base_cg: false,
        };
        assert!((metadata.target_display_peak_nits() - 2.0 * SDR_WHITE_NITS).abs() < 1e-6);
    }
}
