/// Pixel layout for packed image buffers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PixelFormat {
    /// 32-bit RGBA, 8 bits per channel.
    Rgba8888,
    /// 32-bit RGBA, 10 bits per RGB + 2 bits alpha.
    Rgba1010102,
    /// 64-bit RGBA, 16-bit half-float per channel.
    RgbaF16,
}

impl PixelFormat {
    /// Bytes per pixel for this format.
    pub fn bytes_per_pixel(self) -> usize {
        match self {
            PixelFormat::Rgba8888 => 4,
            PixelFormat::Rgba1010102 => 4,
            PixelFormat::RgbaF16 => 8,
        }
    }
}

/// Scene-referred color gamut.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ColorGamut {
    /// BT.709 / sRGB.
    Bt709,
    /// Display P3.
    DisplayP3,
    /// BT.2100 / Rec.2020.
    Bt2100,
}

/// Transfer function describing the relationship between encoded and scene values.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ColorTransfer {
    /// Linear light.
    Linear,
    /// sRGB OETF.
    Srgb,
    /// Perceptual Quantizer (SMPTE ST 2084).
    Pq,
    /// Hybrid Log-Gamma (ITU-R BT.2100).
    Hlg,
}

/// Nominal SDR diffuse white used for capacity math (ISO/TS 22028-5).
pub const SDR_WHITE_NITS: f32 = 203.0;
/// HLG peak white in nits.
pub const HLG_MAX_NITS: f32 = 1000.0;
/// PQ peak white in nits.
pub const PQ_MAX_NITS: f32 = 10000.0;

/// Parsed metadata describing an embedded gain map (ISO 21496-1).
#[derive(Debug, Clone, PartialEq)]
pub struct GainMapMetadata {
    /// Maximum per-channel content boost.
    pub max_content_boost: [f32; 3],
    /// Minimum per-channel content boost.
    pub min_content_boost: [f32; 3],
    /// Per-channel gamma for the gain map.
    pub gamma: [f32; 3],
    /// Per-channel SDR offset.
    pub offset_sdr: [f32; 3],
    /// Per-channel HDR offset.
    pub offset_hdr: [f32; 3],
    /// Lower bound of HDR capacity.
    pub hdr_capacity_min: f32,
    /// Upper bound of HDR capacity.
    pub hdr_capacity_max: f32,
    /// Whether to reuse base image color gamut for gain map.
    pub use_base_cg: bool,
}

impl GainMapMetadata {
    /// Target display peak brightness in nits.
    pub fn target_display_peak_nits(&self) -> f32 {
        self.hdr_capacity_max * SDR_WHITE_NITS
    }

    /// Whether all three channels have identical metadata values.
    pub fn are_all_channels_identical(&self) -> bool {
        let eq3 = |a: &[f32; 3]| {
            (a[0] - a[1]).abs() < f32::EPSILON && (a[0] - a[2]).abs() < f32::EPSILON
        };
        eq3(&self.max_content_boost)
            && eq3(&self.min_content_boost)
            && eq3(&self.gamma)
            && eq3(&self.offset_sdr)
            && eq3(&self.offset_hdr)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pixel_format_bytes_per_pixel() {
        assert_eq!(PixelFormat::Rgba8888.bytes_per_pixel(), 4);
        assert_eq!(PixelFormat::Rgba1010102.bytes_per_pixel(), 4);
        assert_eq!(PixelFormat::RgbaF16.bytes_per_pixel(), 8);
    }

    #[test]
    fn color_gamut_debug_display() {
        let cg = ColorGamut::Bt709;
        assert_eq!(format!("{cg:?}"), "Bt709");
    }

    #[test]
    fn sdr_white_nits_constant() {
        assert!((SDR_WHITE_NITS - 203.0).abs() < f32::EPSILON);
    }

    #[test]
    fn gain_map_metadata_target_display_peak() {
        let meta = GainMapMetadata {
            max_content_boost: [4.0; 3],
            min_content_boost: [1.0; 3],
            gamma: [1.0; 3],
            offset_sdr: [0.0; 3],
            offset_hdr: [0.0; 3],
            hdr_capacity_min: 1.0,
            hdr_capacity_max: 4.0,
            use_base_cg: false,
        };
        let peak = meta.target_display_peak_nits();
        assert!((peak - 4.0 * 203.0).abs() < 0.01);
    }
}
