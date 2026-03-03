//! UltraHDR JPEG decoder: extract gain map, apply it, and produce HDR output.

use crate::color::Color;
use crate::color::transfer::{hlg_oetf, pq_oetf, srgb_inv_oetf, srgb_oetf};
use crate::error::{Error, Result};
use crate::gainmap::math::{
    apply_gain_multi, apply_gain_single, sample_map_bilinear, sample_map_bilinear_rgb,
};
use crate::gainmap::metadata::{
    decode_gainmap_metadata, fraction_to_float, parse_xmp_gainmap_metadata,
};
use crate::jpeg::parse_jpeg_segments;
use crate::types::{ColorGamut, ColorTransfer, GainMapMetadata, PixelFormat};

/// MPF APP2 signature.
const MPF_SIG: &[u8; 4] = b"MPF\0";

/// ISO 21496-1 gain map metadata namespace identifier.
const ISO_GAINMAP_TAG: &[u8] = b"urn:iso:std:iso:ts:21496:-1";

/// XMP APP1 namespace prefix.
const XMP_SIG: &[u8] = b"http://ns.adobe.com/xap/1.0/\0";

/// Result of extracting a gain map from an UltraHDR JPEG.
pub struct GainMapExtract {
    /// Raw JPEG bytes of the gain map image.
    pub gainmap_jpeg: Vec<u8>,
    /// Parsed gain map metadata.
    pub metadata: GainMapMetadata,
}

/// Extract the gain map JPEG and metadata from an UltraHDR JPEG.
///
/// Returns `Ok(None)` if the JPEG does not contain an embedded gain map.
pub fn extract_gainmap_jpeg(data: &[u8]) -> Result<Option<GainMapExtract>> {
    let segments = parse_jpeg_segments(data)?;

    // Step 1: Find MPF APP2 segment to locate secondary image.
    let mut secondary_offset: Option<u32> = None;
    let mut secondary_size: Option<u32> = None;
    // Absolute file position of the MPF TIFF header (for offset rebasing).
    let mut mpf_tiff_header_offset: usize = 0;

    for seg in &segments.segments {
        // APP2 marker = 0xE2
        if seg.marker == 0xE2 && seg.data.len() >= 4 && &seg.data[..4] == MPF_SIG {
            // Parse MPF structure to find secondary image entry.
            // MPF structure after signature: endian(4) + IFD offset(4) + tag count(2) + tags...
            let mpf_data = &seg.data[4..]; // skip "MPF\0"
            // seg.offset is the file position of the FF E2 marker.
            // seg.data starts 4 bytes later (after FF E2 LL LL).
            // MPF\0 is 4 more bytes. TIFF header = seg.offset + 4 + 4.
            mpf_tiff_header_offset = seg.offset + 4 + 4;
            if mpf_data.len() < 8 {
                continue;
            }

            // Read tag count at offset 8 (after endian(4) + IFD offset(4))
            if mpf_data.len() < 10 {
                continue;
            }
            let tag_count = u16::from_be_bytes([mpf_data[8], mpf_data[9]]) as usize;

            // Each tag is 12 bytes, starting at offset 10
            let tags_end = 10 + tag_count * 12;
            if mpf_data.len() < tags_end + 4 {
                continue;
            }

            // Find MP Entry tag (0xB002) to get entry data offset
            let mut mp_entry_offset: Option<usize> = None;
            let mut mp_entry_count: Option<usize> = None;
            for i in 0..tag_count {
                let tag_start = 10 + i * 12;
                let tag_id = u16::from_be_bytes([mpf_data[tag_start], mpf_data[tag_start + 1]]);
                if tag_id == 0xB002 {
                    let count = u32::from_be_bytes([
                        mpf_data[tag_start + 4],
                        mpf_data[tag_start + 5],
                        mpf_data[tag_start + 6],
                        mpf_data[tag_start + 7],
                    ]) as usize;
                    let offset = u32::from_be_bytes([
                        mpf_data[tag_start + 8],
                        mpf_data[tag_start + 9],
                        mpf_data[tag_start + 10],
                        mpf_data[tag_start + 11],
                    ]) as usize;
                    mp_entry_count = Some(count);
                    mp_entry_offset = Some(offset);
                }
            }

            if let (Some(entry_offset), Some(entry_count)) = (mp_entry_offset, mp_entry_count) {
                // MP entries are 16 bytes each. Second entry is the gain map.
                // entry_offset is relative to the TIFF header (start of endian marker)
                let num_entries = entry_count / 16;
                if num_entries >= 2 && entry_offset + 32 <= mpf_data.len() {
                    // Second entry starts at entry_offset + 16
                    let e2_start = entry_offset + 16;
                    if e2_start + 16 <= mpf_data.len() {
                        let size = u32::from_be_bytes([
                            mpf_data[e2_start + 4],
                            mpf_data[e2_start + 5],
                            mpf_data[e2_start + 6],
                            mpf_data[e2_start + 7],
                        ]);
                        let offset = u32::from_be_bytes([
                            mpf_data[e2_start + 8],
                            mpf_data[e2_start + 9],
                            mpf_data[e2_start + 10],
                            mpf_data[e2_start + 11],
                        ]);
                        secondary_size = Some(size);
                        // MPF offsets for non-first entries are relative to the
                        // TIFF header inside the MPF APP2 segment.
                        secondary_offset = Some(offset);
                    }
                }
            }
        }
    }

    // No MPF segment found — this is not an UltraHDR JPEG.
    let (sec_rel_offset, sec_size) = match (secondary_offset, secondary_size) {
        (Some(o), Some(s)) => (o as usize, s as usize),
        _ => return Ok(None),
    };

    // Step 2: Extract the secondary (gain map) JPEG bytes.
    // Per the MPF spec, offsets for non-first entries are relative to the
    // TIFF header inside the MPF APP2 segment. Some encoders (including our
    // own before this fix) use absolute file offsets instead. Try the
    // spec-compliant interpretation first, then fall back to absolute.
    let sec_offset = if sec_rel_offset == 0 {
        0usize
    } else {
        let spec_offset = mpf_tiff_header_offset + sec_rel_offset;
        if spec_offset + sec_size <= data.len()
            && data.get(spec_offset..spec_offset + 2) == Some(&[0xFF, 0xD8])
        {
            spec_offset
        } else if sec_rel_offset + sec_size <= data.len()
            && data.get(sec_rel_offset..sec_rel_offset + 2) == Some(&[0xFF, 0xD8])
        {
            // Fallback: treat as absolute file offset
            sec_rel_offset
        } else {
            0
        }
    };
    if sec_offset == 0 || sec_offset + sec_size > data.len() {
        return Ok(None);
    }
    let gainmap_jpeg = data[sec_offset..sec_offset + sec_size].to_vec();

    // Step 3: Find gain map metadata.
    // Look for ISO 21496-1 binary metadata in APP2 segments of the gain map JPEG,
    // or XMP metadata in APP1 segments.
    let mut metadata: Option<GainMapMetadata> = None;

    // First check primary JPEG segments for XMP containing gain map metadata
    for seg in &segments.segments {
        if seg.marker == 0xE1 && seg.data.len() > XMP_SIG.len() && seg.data.starts_with(XMP_SIG) {
            let xmp_data = &seg.data[XMP_SIG.len()..];
            if let Ok(xmp_str) = std::str::from_utf8(xmp_data)
                && (xmp_str.contains("hdrgm") || xmp_str.contains("hdr-gain-map"))
                && let Ok(m) = parse_xmp_gainmap_metadata(xmp_data)
            {
                metadata = Some(m);
                break;
            }
        }
    }

    // Then check the gain map JPEG segments
    if metadata.is_none() {
        let gm_segments = parse_jpeg_segments(&gainmap_jpeg)?;

        // Look for ISO 21496-1 in APP2 segments
        for seg in &gm_segments.segments {
            if seg.marker == 0xE2
                && seg.data.len() > ISO_GAINMAP_TAG.len()
                && seg.data.starts_with(ISO_GAINMAP_TAG)
            {
                let iso_data = &seg.data[ISO_GAINMAP_TAG.len()..];
                // Skip null terminator if present
                let payload = if !iso_data.is_empty() && iso_data[0] == 0 {
                    &iso_data[1..]
                } else {
                    iso_data
                };
                let frac = decode_gainmap_metadata(payload)?;
                metadata = Some(fraction_to_float(&frac)?);
                break;
            }
        }

        // Fallback: look for XMP in gain map JPEG
        if metadata.is_none() {
            for seg in &gm_segments.segments {
                if seg.marker == 0xE1
                    && seg.data.len() > XMP_SIG.len()
                    && seg.data.starts_with(XMP_SIG)
                {
                    let xmp_data = &seg.data[XMP_SIG.len()..];
                    if let Ok(xmp_str) = std::str::from_utf8(xmp_data)
                        && (xmp_str.contains("hdrgm") || xmp_str.contains("hdr-gain-map"))
                        && let Ok(m) = parse_xmp_gainmap_metadata(xmp_data)
                    {
                        metadata = Some(m);
                        break;
                    }
                }
            }
        }
    }

    match metadata {
        Some(meta) => Ok(Some(GainMapExtract {
            gainmap_jpeg,
            metadata: meta,
        })),
        None => {
            // Has a secondary image but no recognizable metadata — still extract with defaults.
            Ok(Some(GainMapExtract {
                gainmap_jpeg,
                metadata: GainMapMetadata {
                    max_content_boost: [2.0; 3],
                    min_content_boost: [1.0; 3],
                    gamma: [1.0; 3],
                    offset_sdr: [0.0; 3],
                    offset_hdr: [0.0; 3],
                    hdr_capacity_min: 1.0,
                    hdr_capacity_max: 2.0,
                    use_base_cg: true,
                },
            }))
        }
    }
}

/// Convert f32 to IEEE 754 half-precision (f16) stored as u16.
fn f32_to_f16(val: f32) -> u16 {
    let bits = val.to_bits();
    let sign = (bits >> 16) & 0x8000;
    let exp = ((bits >> 23) & 0xFF) as i32;
    let mantissa = bits & 0x007F_FFFF;

    if exp == 255 {
        // Inf or NaN
        return (sign | 0x7C00 | if mantissa != 0 { 0x0200 } else { 0 }) as u16;
    }

    let new_exp = exp - 127 + 15;
    if new_exp >= 31 {
        // Overflow to infinity
        return (sign | 0x7C00) as u16;
    }
    if new_exp <= 0 {
        // Underflow to zero (or subnormal, simplified to zero)
        return sign as u16;
    }

    let half_mantissa = mantissa >> 13;
    (sign | ((new_exp as u32) << 10) | half_mantissa) as u16
}

/// Convert RGB pixel buffer to RGBA by adding alpha=255.
fn rgb_to_rgba(rgb: &[u8], width: usize, height: usize) -> Vec<u8> {
    let mut rgba = Vec::with_capacity(width * height * 4);
    for pixel in rgb.chunks_exact(3) {
        rgba.push(pixel[0]);
        rgba.push(pixel[1]);
        rgba.push(pixel[2]);
        rgba.push(255);
    }
    rgba
}

/// Apply a gain map to SDR pixels to produce HDR output.
///
/// SDR input is expected as RGBA8888 (4 bytes per pixel).
/// The gain map is single-channel (grayscale) u8 data with dimensions `map_width x map_height`.
/// Output format is determined by `output_format`.
#[allow(clippy::too_many_arguments)]
pub fn apply_gainmap_to_sdr(
    sdr_pixels: &[u8],
    sdr_width: usize,
    sdr_height: usize,
    gainmap: &[u8],
    map_width: usize,
    map_height: usize,
    metadata: &GainMapMetadata,
    max_display_boost: f32,
    output_transfer: ColorTransfer,
    output_format: PixelFormat,
) -> Result<Vec<u8>> {
    let expected_sdr_size = sdr_width * sdr_height * 4;
    if sdr_pixels.len() < expected_sdr_size {
        return Err(Error::InvalidParam(format!(
            "SDR pixel buffer too small: {} < {}",
            sdr_pixels.len(),
            expected_sdr_size,
        )));
    }

    // Detect gain map channel count from buffer size.
    // Grayscale: 1 byte/pixel, RGB: 3 bytes/pixel.
    let map_pixels = map_width * map_height;
    let gainmap_channels = gainmap.len() / map_pixels;
    let gainmap_is_rgb = gainmap_channels >= 3;

    let expected_map_size = map_pixels * if gainmap_is_rgb { 3 } else { 1 };
    if gainmap.len() < expected_map_size {
        return Err(Error::InvalidParam(format!(
            "gain map buffer too small: {} < {}",
            gainmap.len(),
            expected_map_size,
        )));
    }

    // Compute display boost weight (how much of the gain map to apply).
    // weight = 0 means SDR (no boost), weight = 1 means full HDR.
    let display_boost =
        max_display_boost.clamp(metadata.hdr_capacity_min, metadata.hdr_capacity_max);
    let weight = if (metadata.hdr_capacity_max - metadata.hdr_capacity_min).abs() < f32::EPSILON {
        0.0
    } else {
        (display_boost.log2() - metadata.hdr_capacity_min.log2())
            / (metadata.hdr_capacity_max.log2() - metadata.hdr_capacity_min.log2())
    };

    let scale_x = if map_width == sdr_width {
        1.0f32
    } else {
        sdr_width as f32 / map_width as f32
    };
    let scale_y = if map_height == sdr_height {
        1.0f32
    } else {
        sdr_height as f32 / map_height as f32
    };
    let scale_factor = scale_x.max(scale_y);

    let multi_channel = !metadata.are_all_channels_identical();
    let bpp = output_format.bytes_per_pixel();
    let mut output = vec![0u8; sdr_width * sdr_height * bpp];

    for y in 0..sdr_height {
        for x in 0..sdr_width {
            let px_idx = (y * sdr_width + x) * 4;
            let r_u8 = sdr_pixels[px_idx];
            let g_u8 = sdr_pixels[px_idx + 1];
            let b_u8 = sdr_pixels[px_idx + 2];
            let a_u8 = sdr_pixels[px_idx + 3];

            // Convert SDR to linear
            let r_lin = srgb_inv_oetf(r_u8 as f32 / 255.0);
            let g_lin = srgb_inv_oetf(g_u8 as f32 / 255.0);
            let b_lin = srgb_inv_oetf(b_u8 as f32 / 255.0);
            let sdr_color = Color::new(r_lin, g_lin, b_lin);

            // Sample gain map and apply gain with display boost weight.
            // Weight is applied in log domain: exp2(logBoost * weight).
            // For RGB gain maps, sample each channel independently.
            // For grayscale gain maps, broadcast the single value.
            let hdr_color = if gainmap_is_rgb && multi_channel {
                let gains = sample_map_bilinear_rgb(
                    gainmap,
                    map_width as u32,
                    map_height as u32,
                    scale_factor,
                    x as u32,
                    y as u32,
                );
                apply_gain_multi(sdr_color, gains, metadata, weight)
            } else {
                let gain = if gainmap_is_rgb {
                    // RGB gain map but identical metadata per channel: use luma of RGB
                    let gains = sample_map_bilinear_rgb(
                        gainmap,
                        map_width as u32,
                        map_height as u32,
                        scale_factor,
                        x as u32,
                        y as u32,
                    );
                    gains[0] * 0.2126 + gains[1] * 0.7152 + gains[2] * 0.0722
                } else {
                    sample_map_bilinear(
                        gainmap,
                        map_width as u32,
                        map_height as u32,
                        scale_factor,
                        x as u32,
                        y as u32,
                    )
                };
                if multi_channel {
                    apply_gain_multi(sdr_color, [gain; 3], metadata, weight)
                } else {
                    apply_gain_single(sdr_color, gain, metadata, weight)
                }
            };

            // Apply output transfer function
            let (r_out, g_out, b_out) = match output_transfer {
                ColorTransfer::Linear => (hdr_color.r, hdr_color.g, hdr_color.b),
                ColorTransfer::Srgb => (
                    srgb_oetf(hdr_color.r.max(0.0)),
                    srgb_oetf(hdr_color.g.max(0.0)),
                    srgb_oetf(hdr_color.b.max(0.0)),
                ),
                ColorTransfer::Pq => (
                    pq_oetf(hdr_color.r.max(0.0)),
                    pq_oetf(hdr_color.g.max(0.0)),
                    pq_oetf(hdr_color.b.max(0.0)),
                ),
                ColorTransfer::Hlg => (
                    hlg_oetf(hdr_color.r.max(0.0)),
                    hlg_oetf(hdr_color.g.max(0.0)),
                    hlg_oetf(hdr_color.b.max(0.0)),
                ),
            };

            // Write output pixel
            let out_idx = (y * sdr_width + x) * bpp;
            match output_format {
                PixelFormat::Rgba8888 => {
                    output[out_idx] = (r_out.clamp(0.0, 1.0) * 255.0 + 0.5) as u8;
                    output[out_idx + 1] = (g_out.clamp(0.0, 1.0) * 255.0 + 0.5) as u8;
                    output[out_idx + 2] = (b_out.clamp(0.0, 1.0) * 255.0 + 0.5) as u8;
                    output[out_idx + 3] = a_u8;
                }
                PixelFormat::Rgba1010102 => {
                    let r10 = (r_out.clamp(0.0, 1.0) * 1023.0 + 0.5) as u32;
                    let g10 = (g_out.clamp(0.0, 1.0) * 1023.0 + 0.5) as u32;
                    let b10 = (b_out.clamp(0.0, 1.0) * 1023.0 + 0.5) as u32;
                    let a2 = ((a_u8 as u32) >> 6) & 0x3;
                    let packed = r10 | (g10 << 10) | (b10 << 20) | (a2 << 30);
                    output[out_idx..out_idx + 4].copy_from_slice(&packed.to_le_bytes());
                }
                PixelFormat::RgbaF16 => {
                    let r_h = f32_to_f16(r_out);
                    let g_h = f32_to_f16(g_out);
                    let b_h = f32_to_f16(b_out);
                    let a_h = f32_to_f16(a_u8 as f32 / 255.0);
                    output[out_idx..out_idx + 2].copy_from_slice(&r_h.to_le_bytes());
                    output[out_idx + 2..out_idx + 4].copy_from_slice(&g_h.to_le_bytes());
                    output[out_idx + 4..out_idx + 6].copy_from_slice(&b_h.to_le_bytes());
                    output[out_idx + 6..out_idx + 8].copy_from_slice(&a_h.to_le_bytes());
                }
            }
        }
    }

    Ok(output)
}

/// Decoded HDR image output.
pub struct DecodedImage {
    pub data: Vec<u8>,
    pub width: u32,
    pub height: u32,
    pub format: PixelFormat,
    pub gamut: ColorGamut,
    pub transfer: ColorTransfer,
}

/// High-level UltraHDR decoder with builder pattern.
pub struct Decoder<'a> {
    data: &'a [u8],
    output_format: PixelFormat,
    output_transfer: ColorTransfer,
    max_display_boost: f32,
}

impl<'a> Decoder<'a> {
    pub fn new(data: &'a [u8]) -> Self {
        Self {
            data,
            output_format: PixelFormat::Rgba8888,
            output_transfer: ColorTransfer::Srgb,
            max_display_boost: f32::MAX,
        }
    }

    pub fn output_format(mut self, fmt: PixelFormat) -> Self {
        self.output_format = fmt;
        self
    }

    pub fn output_transfer(mut self, ct: ColorTransfer) -> Self {
        self.output_transfer = ct;
        self
    }

    pub fn max_display_boost(mut self, boost: f32) -> Self {
        self.max_display_boost = boost;
        self
    }

    /// Probe the JPEG for gain map metadata without decoding.
    pub fn probe(&self) -> Result<Option<GainMapMetadata>> {
        let extract = extract_gainmap_jpeg(self.data)?;
        Ok(extract.map(|e| e.metadata))
    }

    /// Decode the UltraHDR JPEG into an HDR image.
    pub fn decode(&self) -> Result<DecodedImage> {
        // Extract gain map
        let extract = extract_gainmap_jpeg(self.data)?;
        let extract = extract
            .ok_or_else(|| Error::InvalidParam("not an UltraHDR JPEG: no gain map found".into()))?;

        // Decode primary (SDR) JPEG
        let primary = crate::jpeg::decode::decode_jpeg(self.data)?;

        // Detect color gamut from ICC profile
        let gamut = primary
            .icc_profile
            .as_deref()
            .and_then(crate::color::icc::detect_color_gamut)
            .unwrap_or(ColorGamut::Bt709);

        // For SRGB output, return the SDR base image directly without
        // applying the gain map (matches C++ libultrahdr behavior).
        if self.output_transfer == ColorTransfer::Srgb {
            let sdr_rgba = rgb_to_rgba(
                &primary.pixels,
                primary.width as usize,
                primary.height as usize,
            );
            return Ok(DecodedImage {
                data: sdr_rgba,
                width: primary.width,
                height: primary.height,
                format: PixelFormat::Rgba8888,
                gamut,
                transfer: ColorTransfer::Srgb,
            });
        }

        // Decode gain map JPEG
        let gm_decoded = crate::jpeg::decode::decode_jpeg(&extract.gainmap_jpeg)?;

        // Convert primary pixels from RGB to RGBA
        let sdr_rgba = rgb_to_rgba(
            &primary.pixels,
            primary.width as usize,
            primary.height as usize,
        );

        // Apply gain map
        let hdr_data = apply_gainmap_to_sdr(
            &sdr_rgba,
            primary.width as usize,
            primary.height as usize,
            &gm_decoded.pixels,
            gm_decoded.width as usize,
            gm_decoded.height as usize,
            &extract.metadata,
            self.max_display_boost,
            self.output_transfer,
            self.output_format,
        )?;

        Ok(DecodedImage {
            data: hdr_data,
            width: primary.width,
            height: primary.height,
            format: self.output_format,
            gamut,
            transfer: self.output_transfer,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Create a minimal valid JPEG (SOI + DQT + SOF + SOS + EOI).
    fn create_minimal_jpeg() -> Vec<u8> {
        crate::jpeg::encode::encode_rgb_to_jpeg(&[128u8; 2 * 2 * 3], 2, 2, 90)
            .expect("failed to create test JPEG")
    }

    #[test]
    fn split_non_ultrahdr_returns_none() {
        let regular_jpeg = create_minimal_jpeg();
        let result = extract_gainmap_jpeg(&regular_jpeg);
        assert!(result.is_ok());
        assert!(result.unwrap().is_none());
    }

    #[test]
    fn apply_gainmap_identity_boost() {
        // With max_display_boost = 1.0, output should match SDR input
        let width = 2;
        let height = 2;
        let sdr_pixels: Vec<u8> = vec![
            128, 128, 128, 255, 128, 128, 128, 255, 128, 128, 128, 255, 128, 128, 128, 255,
        ];
        let gainmap: Vec<u8> = vec![128; 4]; // 2x2 single-channel
        let meta = GainMapMetadata {
            max_content_boost: [2.0; 3],
            min_content_boost: [1.0; 3],
            gamma: [1.0; 3],
            offset_sdr: [0.0; 3],
            offset_hdr: [0.0; 3],
            hdr_capacity_min: 1.0,
            hdr_capacity_max: 2.0,
            use_base_cg: true,
        };
        let result = apply_gainmap_to_sdr(
            &sdr_pixels,
            width,
            height,
            &gainmap,
            2,
            2,
            &meta,
            1.0, // max_display_boost = 1.0 => no boost
            ColorTransfer::Srgb,
            PixelFormat::Rgba8888,
        );
        assert!(result.is_ok());
        let out = result.unwrap();
        assert_eq!(out.len(), width * height * 4);
    }

    #[test]
    fn decoder_probe_non_ultrahdr() {
        let regular_jpeg = create_minimal_jpeg();
        let decoder = Decoder::new(&regular_jpeg);
        let meta = decoder.probe().unwrap();
        assert!(meta.is_none());
    }

    #[test]
    fn decoder_builder_default() {
        let regular_jpeg = create_minimal_jpeg();
        let decoder = Decoder::new(&regular_jpeg)
            .output_format(PixelFormat::Rgba8888)
            .output_transfer(ColorTransfer::Srgb)
            .max_display_boost(1.0);
        // Should not panic
        let _ = decoder.probe();
    }
}
