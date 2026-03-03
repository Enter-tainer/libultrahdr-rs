use crate::color::Color;
use crate::types::GainMapMetadata;

/// Small epsilon offset to avoid log2(0) in gain computation.
const HDR_OFFSET: f32 = 1e-7;
const SDR_OFFSET: f32 = 1e-7;

/// Compute the log2 gain ratio between HDR and SDR luminance values.
///
/// Port of `computeGain()` from gainmapmath.cpp.
pub fn compute_gain(sdr: f32, hdr: f32) -> f32 {
    let gain = ((hdr + HDR_OFFSET) / (sdr + SDR_OFFSET)).log2();
    // Clamp gain for very dark SDR pixels to prevent blow-out during reconstruction.
    if sdr < 2.0 / 255.0 {
        gain.min(2.3)
    } else {
        gain
    }
}

/// Encode the gain ratio between SDR and HDR luminance into a quantized u8 value.
///
/// Port of `encodeGain()` from gainmapmath.cpp.
/// Currently unused — kept for API completeness with libultrahdr.
#[allow(dead_code)]
pub fn encode_gain(y_sdr: f32, y_hdr: f32, metadata: &GainMapMetadata, channel: usize) -> u8 {
    let mut gain = if y_sdr > 0.0 { y_hdr / y_sdr } else { 1.0 };

    gain = gain.clamp(
        metadata.min_content_boost[channel],
        metadata.max_content_boost[channel],
    );

    let log2_min = metadata.min_content_boost[channel].log2();
    let log2_max = metadata.max_content_boost[channel].log2();
    let gain_normalized = (gain.log2() - log2_min) / (log2_max - log2_min);
    let gain_gamma = gain_normalized.powf(metadata.gamma[channel]);
    (gain_gamma * 255.0) as u8
}

/// Affine-map a log2 gain value into a quantized u8 with optional gamma.
///
/// Port of `affineMapGain()` from gainmapmath.cpp.
/// Currently unused — kept for API completeness with libultrahdr.
#[allow(dead_code)]
pub fn affine_map_gain(gainlog2: f32, mingainlog2: f32, maxgainlog2: f32, gamma: f32) -> u8 {
    let mut mapped = (gainlog2 - mingainlog2) / (maxgainlog2 - mingainlog2);
    if gamma != 1.0 {
        mapped = mapped.powf(gamma);
    }
    mapped *= 255.0;
    (mapped + 0.5).clamp(0.0, 255.0) as u8
}

/// Apply a single-channel gain map value to reconstruct HDR color from SDR.
///
/// Port of `applyGain(Color, float, metadata)` from gainmapmath.cpp.
pub fn apply_gain_single(color: Color, gain: f32, metadata: &GainMapMetadata) -> Color {
    let mut g = gain;
    if metadata.gamma[0] != 1.0 {
        g = g.powf(1.0 / metadata.gamma[0]);
    }
    let log_boost = metadata.min_content_boost[0].log2() * (1.0 - g)
        + metadata.max_content_boost[0].log2() * g;
    let gain_factor = log_boost.exp2();
    (color + metadata.offset_sdr[0]) * gain_factor - metadata.offset_hdr[0]
}

/// Apply per-channel gain map values to reconstruct HDR color from SDR.
///
/// Port of `applyGain(Color, Color, metadata)` from gainmapmath.cpp.
pub fn apply_gain_multi(color: Color, gain_rgb: [f32; 3], metadata: &GainMapMetadata) -> Color {
    let mut gr = gain_rgb[0];
    let mut gg = gain_rgb[1];
    let mut gb = gain_rgb[2];
    if metadata.gamma[0] != 1.0 {
        gr = gr.powf(1.0 / metadata.gamma[0]);
    }
    if metadata.gamma[1] != 1.0 {
        gg = gg.powf(1.0 / metadata.gamma[1]);
    }
    if metadata.gamma[2] != 1.0 {
        gb = gb.powf(1.0 / metadata.gamma[2]);
    }

    let log_r =
        metadata.min_content_boost[0].log2() * (1.0 - gr) + metadata.max_content_boost[0].log2() * gr;
    let log_g =
        metadata.min_content_boost[1].log2() * (1.0 - gg) + metadata.max_content_boost[1].log2() * gg;
    let log_b =
        metadata.min_content_boost[2].log2() * (1.0 - gb) + metadata.max_content_boost[2].log2() * gb;

    Color::new(
        (color.r + metadata.offset_sdr[0]) * log_r.exp2() - metadata.offset_hdr[0],
        (color.g + metadata.offset_sdr[1]) * log_g.exp2() - metadata.offset_hdr[1],
        (color.b + metadata.offset_sdr[2]) * log_b.exp2() - metadata.offset_hdr[2],
    )
}

/// Bilinear interpolation for gain map upsampling using Shepard's inverse distance weighting.
///
/// Port of `sampleMap(map, float map_scale_factor, x, y)` from gainmapmath.cpp.
pub fn sample_map_bilinear(
    map: &[u8],
    map_w: u32,
    map_h: u32,
    scale_factor: f32,
    x: u32,
    y: u32,
) -> f32 {
    let x_map = x as f32 / scale_factor;
    let y_map = y as f32 / scale_factor;

    let x_lower = (x_map.floor() as u32).min(map_w - 1);
    let x_upper = (x_lower + 1).min(map_w - 1);
    let y_lower = (y_map.floor() as u32).min(map_h - 1);
    let y_upper = (y_lower + 1).min(map_h - 1);

    let to_float = |v: u8| v as f32 / 255.0;
    let pyth_dist = |dx: f32, dy: f32| (dx * dx + dy * dy).sqrt();

    let e1 = to_float(map[(x_lower + y_lower * map_w) as usize]);
    let e1_dist = pyth_dist(x_map - x_lower as f32, y_map - y_lower as f32);
    if e1_dist == 0.0 {
        return e1;
    }

    let e2 = to_float(map[(x_lower + y_upper * map_w) as usize]);
    let e2_dist = pyth_dist(x_map - x_lower as f32, y_map - y_upper as f32);
    if e2_dist == 0.0 {
        return e2;
    }

    let e3 = to_float(map[(x_upper + y_lower * map_w) as usize]);
    let e3_dist = pyth_dist(x_map - x_upper as f32, y_map - y_lower as f32);
    if e3_dist == 0.0 {
        return e3;
    }

    let e4 = to_float(map[(x_upper + y_upper * map_w) as usize]);
    let e4_dist = pyth_dist(x_map - x_upper as f32, y_map - y_upper as f32);
    if e4_dist == 0.0 {
        return e4;
    }

    let w1 = 1.0 / e1_dist;
    let w2 = 1.0 / e2_dist;
    let w3 = 1.0 / e3_dist;
    let w4 = 1.0 / e4_dist;
    let total = w1 + w2 + w3 + w4;

    e1 * (w1 / total) + e2 * (w2 / total) + e3 * (w3 / total) + e4 * (w4 / total)
}

/// Reinhard-style tone mapping operator.
fn reinhard_map(y_hdr: f32, headroom: f32) -> f32 {
    let out = (1.0 + y_hdr / (headroom * headroom)) / (1.0 + y_hdr);
    out * y_hdr
}

/// Global tone mapping from HDR to SDR using Reinhard operator.
///
/// Returns `(tone_mapped_rgb, y_sdr, y_hdr)`.
/// Port of `globalTonemap()` from jpegr.cpp.
pub fn global_tonemap(
    rgb: [f32; 3],
    headroom: f32,
    is_normalized: bool,
) -> ([f32; 3], f32, f32) {
    // Scale to headroom to get HDR values referenced to SDR white.
    let rgb_hdr: [f32; 3] = if is_normalized {
        [rgb[0] * headroom, rgb[1] * headroom, rgb[2] * headroom]
    } else {
        rgb
    };

    // Apply Reinhard tone mapping to compress [0, headroom] to [0, 1].
    let max_hdr = rgb_hdr[0].max(rgb_hdr[1]).max(rgb_hdr[2]);
    let max_sdr = reinhard_map(max_hdr, headroom);

    let rgb_sdr = if max_hdr > 0.0 {
        let ratio = max_sdr / max_hdr;
        [
            if rgb_hdr[0] > 0.0 { rgb_hdr[0] * ratio } else { 0.0 },
            if rgb_hdr[1] > 0.0 { rgb_hdr[1] * ratio } else { 0.0 },
            if rgb_hdr[2] > 0.0 { rgb_hdr[2] * ratio } else { 0.0 },
        ]
    } else {
        [0.0, 0.0, 0.0]
    };

    (rgb_sdr, max_sdr, max_hdr)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn default_test_metadata() -> GainMapMetadata {
        GainMapMetadata {
            max_content_boost: [4.0; 3],
            min_content_boost: [1.0; 3],
            gamma: [1.0; 3],
            offset_sdr: [1.0 / 64.0; 3],
            offset_hdr: [1.0 / 64.0; 3],
            hdr_capacity_min: 1.0,
            hdr_capacity_max: 4.0,
            use_base_cg: false,
        }
    }

    // Task 17: Gain map computation
    #[test]
    fn compute_gain_equal_sdr_hdr() {
        let gain = compute_gain(0.5, 0.5);
        assert!(gain.abs() < 0.01);
    }

    #[test]
    fn compute_gain_hdr_brighter() {
        let gain = compute_gain(0.5, 1.0);
        assert!((gain - 1.0).abs() < 0.1);
    }

    #[test]
    fn encode_gain_clamps_to_u8() {
        let meta = default_test_metadata();
        // encode_gain returns u8, so the value is always in [0, 255]
        let _g: u8 = encode_gain(0.5, 2.0, &meta, 0);
    }

    #[test]
    fn apply_gain_identity() {
        use crate::color::Color;
        let meta = default_test_metadata();
        let pixel = Color::new(0.5, 0.5, 0.5);
        let result = apply_gain_single(pixel, 0.0, &meta);
        assert!((result.r - 0.5).abs() < 0.01);
    }

    // Task 18: Gain map sampling
    #[test]
    fn sample_map_exact_pixel() {
        let map_data: Vec<u8> = vec![0, 64, 128, 255]; // 2x2 map
        let value = sample_map_bilinear(&map_data, 2, 2, 2.0, 0, 0);
        assert!((value - 0.0 / 255.0).abs() < 0.01);
    }

    #[test]
    fn sample_map_between_pixels() {
        let map_data: Vec<u8> = vec![0, 255, 0, 255]; // 2x2 map
        let value = sample_map_bilinear(&map_data, 2, 2, 2.0, 1, 1);
        assert!((value - 0.5).abs() < 0.2);
    }

    // Task 19: Global tone mapping
    #[test]
    fn global_tonemap_preserves_black() {
        let (rgb_out, _, _) = global_tonemap([0.0, 0.0, 0.0], 4.0, true);
        assert!(rgb_out[0].abs() < 1e-6);
        assert!(rgb_out[1].abs() < 1e-6);
        assert!(rgb_out[2].abs() < 1e-6);
    }

    #[test]
    fn global_tonemap_maps_to_01() {
        let (rgb_out, _, _) = global_tonemap([1.0, 1.0, 1.0], 4.0, true);
        assert!(rgb_out[0] >= 0.0 && rgb_out[0] <= 1.0);
        assert!(rgb_out[1] >= 0.0 && rgb_out[1] <= 1.0);
        assert!(rgb_out[2] >= 0.0 && rgb_out[2] <= 1.0);
    }
}
