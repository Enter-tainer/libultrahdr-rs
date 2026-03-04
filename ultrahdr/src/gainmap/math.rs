use crate::color::Color;
use crate::types::GainMapMetadata;

/// Small epsilon offset to avoid log2(0) in gain computation.
const HDR_OFFSET: f32 = 1e-7;
const SDR_OFFSET: f32 = 1e-7;

// ---------------------------------------------------------------------------
// GainLut — pre-computed gain factor table
// ---------------------------------------------------------------------------

const GAIN_LUT_SIZE: usize = 1024;

/// Pre-computed gain factor LUT that replaces per-pixel log2/exp2/powf
/// in `apply_gain_single` / `apply_gain_multi`.
pub struct GainLut {
    /// Per-channel gain factor tables (1024 entries each).
    tables: [[f32; GAIN_LUT_SIZE]; 3],
    /// Per-channel 1/gamma (cached for the gamma de-quantization step).
    gamma_inv: [f32; 3],
    /// Per-channel offset_sdr from metadata.
    offset_sdr: [f32; 3],
    /// Per-channel offset_hdr from metadata.
    offset_hdr: [f32; 3],
}

impl GainLut {
    /// Build a gain factor LUT from gain map metadata and a display boost weight.
    pub fn new(metadata: &GainMapMetadata, weight: f32) -> Self {
        let mut tables = [[0.0f32; GAIN_LUT_SIZE]; 3];
        let mut gamma_inv = [1.0f32; 3];

        for ch in 0..3 {
            let log2_min = metadata.min_content_boost[ch].log2();
            let log2_max = metadata.max_content_boost[ch].log2();
            gamma_inv[ch] = if metadata.gamma[ch] != 1.0 {
                1.0 / metadata.gamma[ch]
            } else {
                1.0
            };

            for (idx, entry) in tables[ch].iter_mut().enumerate().take(GAIN_LUT_SIZE) {
                let g = idx as f32 / (GAIN_LUT_SIZE - 1) as f32;
                let log_boost = log2_min * (1.0 - g) + log2_max * g;
                *entry = (log_boost * weight).exp2();
            }
        }

        Self {
            tables,
            gamma_inv,
            offset_sdr: metadata.offset_sdr,
            offset_hdr: metadata.offset_hdr,
        }
    }

    /// Look up the pre-computed gain factor for a given gain value and channel.
    #[inline(always)]
    fn gain_factor(&self, gain: f32, channel: usize) -> f32 {
        let g = if self.gamma_inv[channel] != 1.0 {
            gain.powf(self.gamma_inv[channel])
        } else {
            gain
        };
        let idx = (g * (GAIN_LUT_SIZE - 1) as f32 + 0.5) as usize;
        self.tables[channel][idx.min(GAIN_LUT_SIZE - 1)]
    }

    /// Public accessor for gain_factor (used by SIMD path).
    #[inline(always)]
    pub fn gain_factor_pub(&self, gain: f32, ch: usize) -> f32 {
        self.gain_factor(gain, ch)
    }

    /// Get per-channel SDR offsets.
    pub fn offset_sdr(&self) -> [f32; 3] {
        self.offset_sdr
    }

    /// Get per-channel HDR offsets.
    pub fn offset_hdr(&self) -> [f32; 3] {
        self.offset_hdr
    }

    /// Apply a single-channel gain via LUT lookup.
    #[inline(always)]
    pub fn apply_single(&self, color: Color, gain: f32) -> Color {
        let factor = self.gain_factor(gain, 0);
        (color + self.offset_sdr[0]) * factor - self.offset_hdr[0]
    }

    /// Apply per-channel gain via LUT lookup.
    #[inline(always)]
    pub fn apply_multi(&self, color: Color, gains: [f32; 3]) -> Color {
        let fr = self.gain_factor(gains[0], 0);
        let fg = self.gain_factor(gains[1], 1);
        let fb = self.gain_factor(gains[2], 2);
        Color::new(
            (color.r + self.offset_sdr[0]) * fr - self.offset_hdr[0],
            (color.g + self.offset_sdr[1]) * fg - self.offset_hdr[1],
            (color.b + self.offset_sdr[2]) * fb - self.offset_hdr[2],
        )
    }
}

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
/// Port of `applyGain(Color, float, metadata, gainmapWeight)` from gainmapmath.cpp.
/// The `weight` parameter controls how much of the gain map to apply:
/// weight=0 means SDR (no boost), weight=1 means full HDR.
/// Weight is applied to the log-domain boost: `exp2(logBoost * weight)`.
pub fn apply_gain_single(
    color: Color,
    gain: f32,
    metadata: &GainMapMetadata,
    weight: f32,
) -> Color {
    let mut g = gain;
    if metadata.gamma[0] != 1.0 {
        g = g.powf(1.0 / metadata.gamma[0]);
    }
    let log_boost =
        metadata.min_content_boost[0].log2() * (1.0 - g) + metadata.max_content_boost[0].log2() * g;
    let gain_factor = (log_boost * weight).exp2();
    (color + metadata.offset_sdr[0]) * gain_factor - metadata.offset_hdr[0]
}

/// Apply per-channel gain map values to reconstruct HDR color from SDR.
///
/// Port of `applyGain(Color, Color, metadata, gainmapWeight)` from gainmapmath.cpp.
/// The `weight` parameter controls how much of the gain map to apply:
/// weight=0 means SDR (no boost), weight=1 means full HDR.
/// Weight is applied to the log-domain boost: `exp2(logBoost * weight)`.
pub fn apply_gain_multi(
    color: Color,
    gain_rgb: [f32; 3],
    metadata: &GainMapMetadata,
    weight: f32,
) -> Color {
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

    let log_r = metadata.min_content_boost[0].log2() * (1.0 - gr)
        + metadata.max_content_boost[0].log2() * gr;
    let log_g = metadata.min_content_boost[1].log2() * (1.0 - gg)
        + metadata.max_content_boost[1].log2() * gg;
    let log_b = metadata.min_content_boost[2].log2() * (1.0 - gb)
        + metadata.max_content_boost[2].log2() * gb;

    Color::new(
        (color.r + metadata.offset_sdr[0]) * (log_r * weight).exp2() - metadata.offset_hdr[0],
        (color.g + metadata.offset_sdr[1]) * (log_g * weight).exp2() - metadata.offset_hdr[1],
        (color.b + metadata.offset_sdr[2]) * (log_b * weight).exp2() - metadata.offset_hdr[2],
    )
}

// ---------------------------------------------------------------------------
// ShepardsIDW — pre-computed weight tables for fast gain map sampling
// ---------------------------------------------------------------------------

/// Pre-computed Shepard's inverse distance weighting tables.
///
/// Port of `ShepardsIDW` from C++ libultrahdr gainmapmath.h.
/// When the map scale factor is an integer, weights can be pre-computed once
/// and reused for every pixel, avoiding per-pixel sqrt operations.
pub struct ShepardsIDW {
    scale_factor: usize,
    /// Normal weights (both right and bottom neighbors exist).
    weights: Vec<f32>,
    /// No-right-neighbor weights (right edge of map).
    weights_nr: Vec<f32>,
    /// No-bottom-neighbor weights (bottom edge of map).
    weights_nb: Vec<f32>,
    /// Corner weights (no right, no bottom).
    weights_c: Vec<f32>,
}

impl ShepardsIDW {
    /// Build weight tables for a given integer scale factor.
    pub fn new(scale_factor: usize) -> Self {
        let size = scale_factor * scale_factor * 4;
        let mut weights = vec![0.0f32; size];
        let mut weights_nr = vec![0.0f32; size];
        let mut weights_nb = vec![0.0f32; size];
        let mut weights_c = vec![0.0f32; size];

        Self::fill(&mut weights, scale_factor, 1, 1);
        Self::fill(&mut weights_nr, scale_factor, 0, 1);
        Self::fill(&mut weights_nb, scale_factor, 1, 0);
        Self::fill(&mut weights_c, scale_factor, 0, 0);

        Self {
            scale_factor,
            weights,
            weights_nr,
            weights_nb,
            weights_c,
        }
    }

    fn fill(weights: &mut [f32], sf: usize, inc_r: usize, inc_b: usize) {
        for y in 0..sf {
            for x in 0..sf {
                let pos_x = x as f32 / sf as f32;
                let pos_y = y as f32 / sf as f32;
                let curr_x = 0.0f32; // floor(pos_x) is always 0 since pos_x < 1
                let curr_y = 0.0f32;
                let next_x = inc_r as f32;
                let next_y = inc_b as f32;

                let idx = y * sf * 4 + x * 4;
                let e1_dist = ((pos_x - curr_x) * (pos_x - curr_x)
                    + (pos_y - curr_y) * (pos_y - curr_y))
                    .sqrt();

                if e1_dist == 0.0 {
                    weights[idx] = 1.0;
                    weights[idx + 1] = 0.0;
                    weights[idx + 2] = 0.0;
                    weights[idx + 3] = 0.0;
                } else {
                    let e1_w = 1.0 / e1_dist;
                    let e2_dist = ((pos_x - curr_x) * (pos_x - curr_x)
                        + (pos_y - next_y) * (pos_y - next_y))
                        .sqrt();
                    let e2_w = 1.0 / e2_dist;
                    let e3_dist = ((pos_x - next_x) * (pos_x - next_x)
                        + (pos_y - curr_y) * (pos_y - curr_y))
                        .sqrt();
                    let e3_w = 1.0 / e3_dist;
                    let e4_dist = ((pos_x - next_x) * (pos_x - next_x)
                        + (pos_y - next_y) * (pos_y - next_y))
                        .sqrt();
                    let e4_w = 1.0 / e4_dist;

                    let total = e1_w + e2_w + e3_w + e4_w;
                    weights[idx] = e1_w / total;
                    weights[idx + 1] = e2_w / total;
                    weights[idx + 2] = e3_w / total;
                    weights[idx + 3] = e4_w / total;
                }
            }
        }
    }

    /// Sample a single-channel gain map using pre-computed weights.
    #[inline]
    pub fn sample(&self, map: &[u8], map_w: u32, map_h: u32, x: u32, y: u32) -> f32 {
        let sf = self.scale_factor;
        let x_lower = ((x as usize / sf) as u32).min(map_w - 1);
        let x_upper = (x_lower + 1).min(map_w - 1);
        let y_lower = ((y as usize / sf) as u32).min(map_h - 1);
        let y_upper = (y_lower + 1).min(map_h - 1);

        let e1 = map[(x_lower + y_lower * map_w) as usize] as f32 / 255.0;
        let e2 = map[(x_lower + y_upper * map_w) as usize] as f32 / 255.0;
        let e3 = map[(x_upper + y_lower * map_w) as usize] as f32 / 255.0;
        let e4 = map[(x_upper + y_upper * map_w) as usize] as f32 / 255.0;

        let offset_x = x as usize % sf;
        let offset_y = y as usize % sf;

        let w = if x_lower == x_upper && y_lower == y_upper {
            &self.weights_c
        } else if x_lower == x_upper {
            &self.weights_nr
        } else if y_lower == y_upper {
            &self.weights_nb
        } else {
            &self.weights
        };
        let wi = offset_y * sf * 4 + offset_x * 4;

        e1 * w[wi] + e2 * w[wi + 1] + e3 * w[wi + 2] + e4 * w[wi + 3]
    }

    /// Sample a 3-channel (RGB) gain map using pre-computed weights.
    #[inline]
    pub fn sample_rgb(&self, map: &[u8], map_w: u32, map_h: u32, x: u32, y: u32) -> [f32; 3] {
        let sf = self.scale_factor;
        let x_lower = ((x as usize / sf) as u32).min(map_w - 1);
        let x_upper = (x_lower + 1).min(map_w - 1);
        let y_lower = ((y as usize / sf) as u32).min(map_h - 1);
        let y_upper = (y_lower + 1).min(map_h - 1);

        let idx = |px: u32, py: u32| (px + py * map_w) as usize * 3;
        let i1 = idx(x_lower, y_lower);
        let i2 = idx(x_lower, y_upper);
        let i3 = idx(x_upper, y_lower);
        let i4 = idx(x_upper, y_upper);

        let offset_x = x as usize % sf;
        let offset_y = y as usize % sf;

        let w = if x_lower == x_upper && y_lower == y_upper {
            &self.weights_c
        } else if x_lower == x_upper {
            &self.weights_nr
        } else if y_lower == y_upper {
            &self.weights_nb
        } else {
            &self.weights
        };
        let wi = offset_y * sf * 4 + offset_x * 4;

        let tf = |v: u8| v as f32 / 255.0;
        [
            tf(map[i1]) * w[wi]
                + tf(map[i2]) * w[wi + 1]
                + tf(map[i3]) * w[wi + 2]
                + tf(map[i4]) * w[wi + 3],
            tf(map[i1 + 1]) * w[wi]
                + tf(map[i2 + 1]) * w[wi + 1]
                + tf(map[i3 + 1]) * w[wi + 2]
                + tf(map[i4 + 1]) * w[wi + 3],
            tf(map[i1 + 2]) * w[wi]
                + tf(map[i2 + 2]) * w[wi + 1]
                + tf(map[i3 + 2]) * w[wi + 2]
                + tf(map[i4 + 2]) * w[wi + 3],
        ]
    }
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

/// Bilinear interpolation for multi-channel (RGB) gain map upsampling.
///
/// Returns per-channel gain values `[r, g, b]`.
pub fn sample_map_bilinear_rgb(
    map: &[u8],
    map_w: u32,
    map_h: u32,
    scale_factor: f32,
    x: u32,
    y: u32,
) -> [f32; 3] {
    let x_map = x as f32 / scale_factor;
    let y_map = y as f32 / scale_factor;

    let x_lower = (x_map.floor() as u32).min(map_w - 1);
    let x_upper = (x_lower + 1).min(map_w - 1);
    let y_lower = (y_map.floor() as u32).min(map_h - 1);
    let y_upper = (y_lower + 1).min(map_h - 1);

    let to_float = |v: u8| v as f32 / 255.0;
    let pyth_dist = |dx: f32, dy: f32| (dx * dx + dy * dy).sqrt();

    let idx = |px: u32, py: u32| (px + py * map_w) as usize * 3;

    let i1 = idx(x_lower, y_lower);
    let e1 = [
        to_float(map[i1]),
        to_float(map[i1 + 1]),
        to_float(map[i1 + 2]),
    ];
    let e1_dist = pyth_dist(x_map - x_lower as f32, y_map - y_lower as f32);
    if e1_dist == 0.0 {
        return e1;
    }

    let i2 = idx(x_lower, y_upper);
    let e2 = [
        to_float(map[i2]),
        to_float(map[i2 + 1]),
        to_float(map[i2 + 2]),
    ];
    let e2_dist = pyth_dist(x_map - x_lower as f32, y_map - y_upper as f32);
    if e2_dist == 0.0 {
        return e2;
    }

    let i3 = idx(x_upper, y_lower);
    let e3 = [
        to_float(map[i3]),
        to_float(map[i3 + 1]),
        to_float(map[i3 + 2]),
    ];
    let e3_dist = pyth_dist(x_map - x_upper as f32, y_map - y_lower as f32);
    if e3_dist == 0.0 {
        return e3;
    }

    let i4 = idx(x_upper, y_upper);
    let e4 = [
        to_float(map[i4]),
        to_float(map[i4 + 1]),
        to_float(map[i4 + 2]),
    ];
    let e4_dist = pyth_dist(x_map - x_upper as f32, y_map - y_upper as f32);
    if e4_dist == 0.0 {
        return e4;
    }

    let w1 = 1.0 / e1_dist;
    let w2 = 1.0 / e2_dist;
    let w3 = 1.0 / e3_dist;
    let w4 = 1.0 / e4_dist;
    let total = w1 + w2 + w3 + w4;

    [
        e1[0] * (w1 / total) + e2[0] * (w2 / total) + e3[0] * (w3 / total) + e4[0] * (w4 / total),
        e1[1] * (w1 / total) + e2[1] * (w2 / total) + e3[1] * (w3 / total) + e4[1] * (w4 / total),
        e1[2] * (w1 / total) + e2[2] * (w2 / total) + e3[2] * (w3 / total) + e4[2] * (w4 / total),
    ]
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
pub fn global_tonemap(rgb: [f32; 3], headroom: f32, is_normalized: bool) -> ([f32; 3], f32, f32) {
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
            if rgb_hdr[0] > 0.0 {
                rgb_hdr[0] * ratio
            } else {
                0.0
            },
            if rgb_hdr[1] > 0.0 {
                rgb_hdr[1] * ratio
            } else {
                0.0
            },
            if rgb_hdr[2] > 0.0 {
                rgb_hdr[2] * ratio
            } else {
                0.0
            },
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
            offset_sdr: [1e-7; 3],
            offset_hdr: [1e-7; 3],
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
        let result = apply_gain_single(pixel, 0.0, &meta, 1.0);
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
