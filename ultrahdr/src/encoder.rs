//! UltraHDR JPEG encoder: generate gain maps and assemble UltraHDR JPEGs.

use crate::color::Color;
use crate::color::gamut::gamut_convert;
use crate::color::transfer::{
    hlg_inv_oetf, hlg_ootf_approx, pq_inv_oetf, reference_display_peak_nits, srgb_inv_oetf,
    srgb_oetf,
};
use crate::error::{Error, Result};
use crate::gainmap::math::{compute_gain, global_tonemap};
use crate::gainmap::metadata::{
    GainMapMetadataFrac, encode_gainmap_metadata, write_xmp_gainmap_metadata,
};
use crate::jpeg::parse_jpeg_segments;
use crate::mpf::{calculate_mpf_size, generate_mpf};
use crate::types::{ColorGamut, ColorTransfer, GainMapMetadata, PixelFormat, SDR_WHITE_NITS};

/// Generate a gain map image from SDR and HDR gamma-space pixel buffers.
///
/// Both inputs must be in their native gamma (OETF) space, RGB (3 floats per pixel).
/// SDR is expected in sRGB gamma 0-1; HDR in its native transfer function encoding.
/// Pixels are averaged in gamma space (matching C++ libultrahdr), then linearized
/// per-block before gain computation.
///
/// Returns a grayscale (or 3-channel if `multichannel`) gain map as u8 pixels,
/// plus the associated `GainMapMetadata`.
///
/// Port of `JpegR::generateGainMap()` from libultrahdr.
#[allow(clippy::too_many_arguments)]
pub fn generate_gainmap(
    sdr_gamma: &[f32],
    hdr_gamma: &[f32],
    width: u32,
    height: u32,
    sdr_gamut: ColorGamut,
    hdr_gamut: ColorGamut,
    hdr_transfer: ColorTransfer,
    scale: u32,
    multichannel: bool,
    target_peak_nits: f32,
    use_base_cg: bool,
) -> Result<(Vec<u8>, GainMapMetadata)> {
    let w = width as usize;
    let h = height as usize;
    let expected = w * h * 3;
    if sdr_gamma.len() < expected || hdr_gamma.len() < expected {
        return Err(Error::InvalidParam(format!(
            "pixel buffer too small: need {expected}, got sdr={}, hdr={}",
            sdr_gamma.len(),
            hdr_gamma.len(),
        )));
    }
    if scale == 0 {
        return Err(Error::InvalidParam("scale must be >= 1".into()));
    }

    let map_w = w.div_ceil(scale as usize);
    let map_h = h.div_ceil(scale as usize);

    let headroom = target_peak_nits / SDR_WHITE_NITS;

    // Whether we need gamut conversion for HDR pixels (C++ converts HDR to SDR gamut).
    let need_gamut_convert = sdr_gamut != hdr_gamut;

    // C++ uses hdr_white_nits for HDR nits scaling (1000 for HLG, 10000 for PQ).
    // For Linear/Srgb, we use SDR_WHITE_NITS since there's no separate peak.
    let hdr_nits_factor = match hdr_transfer {
        ColorTransfer::Linear => SDR_WHITE_NITS,
        _ => reference_display_peak_nits(hdr_transfer),
    };

    // First pass: find min/max gain across the image.
    let mut min_gain_log2 = f32::MAX;
    let mut max_gain_log2 = f32::MIN;

    // Temporary storage for per-pixel gain values.
    let channels = if multichannel { 3 } else { 1 };
    let mut gain_values = vec![0.0f32; map_w * map_h * channels];

    for my in 0..map_h {
        for mx in 0..map_w {
            // Average all pixels in the scale×scale block (matching C++ samplePixels).
            let x_start = mx * scale as usize;
            let y_start = my * scale as usize;
            let x_end = (x_start + scale as usize).min(w);
            let y_end = (y_start + scale as usize).min(h);
            let mut sdr_r = 0.0f32;
            let mut sdr_g = 0.0f32;
            let mut sdr_b = 0.0f32;
            let mut hdr_r = 0.0f32;
            let mut hdr_g = 0.0f32;
            let mut hdr_b = 0.0f32;
            let mut count = 0usize;
            for sy in y_start..y_end {
                for sx in x_start..x_end {
                    let idx = (sy * w + sx) * 3;
                    sdr_r += sdr_gamma[idx];
                    sdr_g += sdr_gamma[idx + 1];
                    sdr_b += sdr_gamma[idx + 2];
                    hdr_r += hdr_gamma[idx];
                    hdr_g += hdr_gamma[idx + 1];
                    hdr_b += hdr_gamma[idx + 2];
                    count += 1;
                }
            }
            let inv = 1.0 / count as f32;
            // Average is in gamma space (matching C++ samplePixels).
            let sdr_r_avg = sdr_r * inv;
            let sdr_g_avg = sdr_g * inv;
            let sdr_b_avg = sdr_b * inv;
            let hdr_r_avg = hdr_r * inv;
            let hdr_g_avg = hdr_g * inv;
            let hdr_b_avg = hdr_b * inv;

            // Linearize SDR (always sRGB).
            let mut sdr_r = srgb_inv_oetf(sdr_r_avg);
            let mut sdr_g = srgb_inv_oetf(sdr_g_avg);
            let mut sdr_b = srgb_inv_oetf(sdr_b_avg);

            // Linearize HDR based on transfer function.
            let (mut hdr_r, mut hdr_g, mut hdr_b) = match hdr_transfer {
                ColorTransfer::Hlg => {
                    let r = hlg_inv_oetf(hdr_r_avg);
                    let g = hlg_inv_oetf(hdr_g_avg);
                    let b = hlg_inv_oetf(hdr_b_avg);
                    let [r, g, b] = hlg_ootf_approx(r, g, b);
                    (r, g, b)
                }
                ColorTransfer::Pq => (
                    pq_inv_oetf(hdr_r_avg),
                    pq_inv_oetf(hdr_g_avg),
                    pq_inv_oetf(hdr_b_avg),
                ),
                ColorTransfer::Linear | ColorTransfer::Srgb => (
                    srgb_inv_oetf(hdr_r_avg),
                    srgb_inv_oetf(hdr_g_avg),
                    srgb_inv_oetf(hdr_b_avg),
                ),
            };

            // Gamut conversion: convert HDR to SDR gamut (C++ does this).
            if need_gamut_convert {
                let hdr_color =
                    gamut_convert(Color::new(hdr_r, hdr_g, hdr_b), hdr_gamut, sdr_gamut);
                hdr_r = hdr_color.r;
                hdr_g = hdr_color.g;
                hdr_b = hdr_color.b;
            }

            // clipNegatives: clip both SDR and HDR channels to max(0, val).
            // C++ does this after gamut conversion and before gain computation.
            sdr_r = sdr_r.max(0.0);
            sdr_g = sdr_g.max(0.0);
            sdr_b = sdr_b.max(0.0);
            hdr_r = hdr_r.max(0.0);
            hdr_g = hdr_g.max(0.0);
            hdr_b = hdr_b.max(0.0);

            let map_idx = my * map_w + mx;

            if multichannel {
                for ch in 0..3 {
                    let sdr_ch = [sdr_r, sdr_g, sdr_b][ch];
                    let hdr_ch = [hdr_r, hdr_g, hdr_b][ch];
                    let sdr_ch_nits = sdr_ch * SDR_WHITE_NITS;
                    let hdr_ch_nits = hdr_ch * hdr_nits_factor;
                    let gain = compute_gain(sdr_ch_nits, hdr_ch_nits);
                    gain_values[map_idx * 3 + ch] = gain;
                    min_gain_log2 = min_gain_log2.min(gain);
                    max_gain_log2 = max_gain_log2.max(gain);
                }
            } else {
                // C++ uses fmax(r,g,b) (use_luminance=false), not weighted luminance
                let sdr_y = sdr_r.max(sdr_g).max(sdr_b);
                let hdr_y = hdr_r.max(hdr_g).max(hdr_b);
                let sdr_y_nits = sdr_y * SDR_WHITE_NITS;
                let hdr_y_nits = hdr_y * hdr_nits_factor;
                let gain = compute_gain(sdr_y_nits, hdr_y_nits);
                gain_values[map_idx] = gain;
                min_gain_log2 = min_gain_log2.min(gain);
                max_gain_log2 = max_gain_log2.max(gain);
            }
        }
    }

    // Ensure valid range
    if min_gain_log2 == f32::MAX {
        min_gain_log2 = 0.0;
    }
    if max_gain_log2 == f32::MIN {
        max_gain_log2 = 0.0;
    }
    // Clamp gain range to [-14.3, 15.6] matching C++ generateGainMapTwoPass.
    min_gain_log2 = min_gain_log2.clamp(-14.3, 15.6);
    max_gain_log2 = max_gain_log2.clamp(-14.3, 15.6);
    // Ensure at least some range to avoid division by zero (C++ uses FLT_EPSILON + 0.1)
    if (max_gain_log2 - min_gain_log2).abs() < f32::EPSILON {
        max_gain_log2 += 0.1;
    }

    // Second pass: quantize gain values to u8.
    let map_size = map_w * map_h * channels;
    let mut gainmap = vec![0u8; map_size];

    for i in 0..map_size {
        let gain_log2 = gain_values[i];
        let normalized = (gain_log2 - min_gain_log2) / (max_gain_log2 - min_gain_log2);
        let clamped = normalized.clamp(0.0, 1.0);
        gainmap[i] = (clamped * 255.0 + 0.5) as u8;
    }

    let max_content_boost = (2.0f32).powf(max_gain_log2);
    let min_content_boost = (2.0f32).powf(min_gain_log2);
    let offset = 1e-7;

    let metadata = GainMapMetadata {
        max_content_boost: [max_content_boost; 3],
        min_content_boost: [min_content_boost; 3],
        gamma: [1.0; 3],
        offset_sdr: [offset; 3],
        offset_hdr: [offset; 3],
        hdr_capacity_min: 1.0,
        hdr_capacity_max: headroom,
        use_base_cg,
    };

    Ok((gainmap, metadata))
}

/// Convert float metadata to rational fraction representation for ISO encoding.
fn metadata_to_frac(meta: &GainMapMetadata) -> GainMapMetadataFrac {
    let denom = 10000u32;

    let to_n = |val: f32| -> i32 { (val * denom as f32) as i32 };
    let to_n_u = |val: f32| -> u32 { (val.max(0.0) * denom as f32) as u32 };

    let all_identical = meta.are_all_channels_identical();
    let ch = if all_identical { 1 } else { 3 };

    let mut frac = GainMapMetadataFrac {
        gain_map_min_n: [0; 3],
        gain_map_min_d: [denom; 3],
        gain_map_max_n: [0; 3],
        gain_map_max_d: [denom; 3],
        gain_map_gamma_n: [denom; 3],
        gain_map_gamma_d: [denom; 3],
        base_offset_n: [0; 3],
        base_offset_d: [denom; 3],
        alternate_offset_n: [0; 3],
        alternate_offset_d: [denom; 3],
        base_hdr_headroom_n: to_n_u(meta.hdr_capacity_min.log2()),
        base_hdr_headroom_d: denom,
        alternate_hdr_headroom_n: to_n_u(meta.hdr_capacity_max.log2()),
        alternate_hdr_headroom_d: denom,
        backward_direction: false,
        use_base_color_space: meta.use_base_cg,
    };

    for i in 0..ch {
        frac.gain_map_min_n[i] = to_n(meta.min_content_boost[i].log2());
        frac.gain_map_max_n[i] = to_n(meta.max_content_boost[i].log2());
        frac.gain_map_gamma_n[i] = to_n_u(meta.gamma[i]);
        frac.base_offset_n[i] = to_n(meta.offset_sdr[i]);
        frac.alternate_offset_n[i] = to_n(meta.offset_hdr[i]);
    }

    // Copy ch0 to remaining channels if single-channel
    for i in ch..3 {
        frac.gain_map_min_n[i] = frac.gain_map_min_n[0];
        frac.gain_map_max_n[i] = frac.gain_map_max_n[0];
        frac.gain_map_gamma_n[i] = frac.gain_map_gamma_n[0];
        frac.base_offset_n[i] = frac.base_offset_n[0];
        frac.alternate_offset_n[i] = frac.alternate_offset_n[0];
    }

    frac
}

/// ISO 21496-1 gain map metadata namespace identifier.
const ISO_GAINMAP_TAG: &[u8] = b"urn:iso:std:iso:ts:21496:-1";

/// XMP APP1 namespace prefix.
const XMP_SIG: &[u8] = b"http://ns.adobe.com/xap/1.0/\0";

/// Assemble an UltraHDR JPEG from a primary SDR JPEG, gain map JPEG, and metadata.
///
/// Inserts XMP metadata, ISO 21496-1 binary metadata, and MPF markers into the
/// primary JPEG, then appends the gain map JPEG as a secondary image.
///
/// Port of `JpegR::appendGainMap()` from libultrahdr.
pub fn assemble_ultrahdr(
    sdr_jpeg: &[u8],
    gainmap_jpeg: &[u8],
    metadata: &GainMapMetadata,
    xmp_override: Option<&[u8]>,
    icc_profile: Option<&[u8]>,
) -> Result<Vec<u8>> {
    // Validate inputs
    if sdr_jpeg.len() < 2 || sdr_jpeg[0] != 0xFF || sdr_jpeg[1] != 0xD8 {
        return Err(Error::InvalidParam("SDR data is not a valid JPEG".into()));
    }
    if gainmap_jpeg.len() < 2 || gainmap_jpeg[0] != 0xFF || gainmap_jpeg[1] != 0xD8 {
        return Err(Error::InvalidParam(
            "gain map data is not a valid JPEG".into(),
        ));
    }

    // Generate metadata payloads
    let xmp_data = match xmp_override {
        Some(d) => d.to_vec(),
        None => write_xmp_gainmap_metadata(metadata)?,
    };

    let frac = metadata_to_frac(metadata);
    let iso_data = encode_gainmap_metadata(&frac)?;

    // Parse the SDR JPEG to find segment positions
    let segments = parse_jpeg_segments(sdr_jpeg)?;

    // Build the output JPEG
    let mut out = Vec::with_capacity(sdr_jpeg.len() + gainmap_jpeg.len() + 1024);

    // SOI
    out.extend_from_slice(&[0xFF, 0xD8]);

    // Copy existing APP0/APP1 (EXIF) segments first, inserting our metadata after them.
    for seg in &segments.segments {
        if seg.marker == 0xE0 || seg.marker == 0xE1 {
            // Check if this is already an XMP or gain map segment; skip if so
            if seg.data.starts_with(XMP_SIG) {
                continue;
            }
            // Write this APP segment
            out.push(0xFF);
            out.push(seg.marker);
            let len = (seg.data.len() + 2) as u16;
            out.extend_from_slice(&len.to_be_bytes());
            out.extend_from_slice(&seg.data);
        } else {
            break;
        }
    }

    // Insert XMP APP1 segment with gain map metadata
    {
        let xmp_payload_len = XMP_SIG.len() + xmp_data.len();
        out.push(0xFF);
        out.push(0xE1); // APP1
        let seg_len = (xmp_payload_len + 2) as u16;
        out.extend_from_slice(&seg_len.to_be_bytes());
        out.extend_from_slice(XMP_SIG);
        out.extend_from_slice(&xmp_data);
    }

    // Insert ISO 21496-1 APP2 stub in primary image (version only, matching C++).
    // The full metadata payload goes into the secondary (gain map) image.
    {
        let iso_stub_payload_len = ISO_GAINMAP_TAG.len() + 1 + 4; // namespace + null + 4 version bytes
        out.push(0xFF);
        out.push(0xE2); // APP2
        let seg_len = (iso_stub_payload_len + 2) as u16;
        out.extend_from_slice(&seg_len.to_be_bytes());
        out.extend_from_slice(ISO_GAINMAP_TAG);
        out.push(0x00); // null terminator
        out.extend_from_slice(&[0u8; 4]); // min_version(0) + writer_version(0)
    }

    // Insert ICC profile APP2 segment if provided
    if let Some(icc) = icc_profile {
        // ICC profile APP2: "ICC_PROFILE\0" + chunk_no(1) + num_chunks(1) + data
        let icc_sig = b"ICC_PROFILE\0";
        let payload_len = icc_sig.len() + 2 + icc.len();
        out.push(0xFF);
        out.push(0xE2); // APP2
        let seg_len = (payload_len + 2) as u16;
        out.extend_from_slice(&seg_len.to_be_bytes());
        out.extend_from_slice(icc_sig);
        out.push(1); // chunk number
        out.push(1); // total chunks
        out.extend_from_slice(icc);
    }

    // Calculate where the secondary image will be and insert MPF
    // We need to know the total primary image size to set the offset correctly.
    // Strategy: build the rest of the primary first, then fixup MPF offsets.

    // Reserve space for MPF APP2 segment
    let mpf_data_size = calculate_mpf_size();
    out.push(0xFF);
    out.push(0xE2); // APP2
    let mpf_seg_len = (mpf_data_size + 2) as u16;
    out.extend_from_slice(&mpf_seg_len.to_be_bytes());
    // Placeholder MPF data
    let mpf_data_start = out.len();
    out.extend_from_slice(&vec![0u8; mpf_data_size]);

    // Copy remaining segments from the original JPEG (after the ones we already copied)
    // Find the first segment that isn't APP0/APP1
    let mut found_rest = false;
    for seg in &segments.segments {
        if seg.marker != 0xE0 && seg.marker != 0xE1 {
            // Copy from this segment's offset in the source to the end
            // But we need to handle the existing APP2 segments (skip any existing MPF)
            if seg.data.starts_with(b"MPF\0") {
                continue; // Skip existing MPF
            }
            if !found_rest {
                found_rest = true;
                // Copy everything from this segment to the end of the source JPEG
                // (including SOS and entropy data)
                let src_start = seg.offset;
                out.extend_from_slice(&sdr_jpeg[src_start..]);
                break;
            }
        }
    }

    // If we didn't find any non-APP0/APP1 segments, copy from after SOI + any APP segments
    if !found_rest {
        // Just copy everything after the segments we already processed
        let last_seg_end = segments
            .segments
            .iter()
            .rfind(|s| s.marker == 0xE0 || s.marker == 0xE1)
            .map(|s| s.offset + 2 + 2 + s.data.len()) // marker(2) + length(2) + data
            .unwrap_or(2); // after SOI
        if last_seg_end < sdr_jpeg.len() {
            out.extend_from_slice(&sdr_jpeg[last_seg_end..]);
        }
    }

    // Now fix up the MPF data with correct offsets.
    // Per MPF spec, secondary image offset is relative to the MPF TIFF header.
    // mpf_data_start points to the start of MPF payload (including "MPF\0" sig).
    // TIFF header starts 4 bytes into the MPF data (after "MPF\0").
    let primary_size = out.len() as u32;
    let mpf_tiff_header_pos = (mpf_data_start + 4) as u32;
    let secondary_offset = primary_size - mpf_tiff_header_pos;

    // Build the secondary image (gain map JPEG with ISO metadata inserted after SOI).
    let iso_secondary_seg_len = 2 + ISO_GAINMAP_TAG.len() + 1 + iso_data.len(); // APP2 length field + namespace + null + payload
    let secondary_total_size = gainmap_jpeg.len() + 2 + iso_secondary_seg_len; // +2 for FF E2 marker bytes
    let mpf = generate_mpf(
        primary_size,
        0,
        secondary_total_size as u32,
        secondary_offset,
    );
    out[mpf_data_start..mpf_data_start + mpf_data_size].copy_from_slice(&mpf);

    // Append secondary image: SOI + ISO APP2 + rest of gain map JPEG
    out.extend_from_slice(&gainmap_jpeg[..2]); // SOI (FF D8)
    // Insert ISO 21496-1 APP2 with full metadata into secondary image
    {
        let iso_payload_len = ISO_GAINMAP_TAG.len() + 1 + iso_data.len();
        out.push(0xFF);
        out.push(0xE2); // APP2
        let seg_len = (iso_payload_len + 2) as u16;
        out.extend_from_slice(&seg_len.to_be_bytes());
        out.extend_from_slice(ISO_GAINMAP_TAG);
        out.push(0x00); // null terminator
        out.extend_from_slice(&iso_data);
    }
    out.extend_from_slice(&gainmap_jpeg[2..]); // rest of gain map after SOI

    Ok(out)
}

/// Decode raw pixel data from various formats to linear RGB f32.
#[allow(dead_code)]
fn decode_pixels_to_linear(
    pixels: &[u8],
    width: u32,
    height: u32,
    format: PixelFormat,
    transfer: ColorTransfer,
    _gamut: ColorGamut,
) -> Result<Vec<f32>> {
    let w = width as usize;
    let h = height as usize;
    let bpp = format.bytes_per_pixel();
    let expected = w * h * bpp;
    if pixels.len() < expected {
        return Err(Error::InvalidParam(format!(
            "pixel buffer too small: need {expected}, got {}",
            pixels.len(),
        )));
    }

    let mut linear = vec![0.0f32; w * h * 3];
    let peak = reference_display_peak_nits(transfer);
    let scale_to_sdr = peak / SDR_WHITE_NITS;

    for i in 0..(w * h) {
        let (r, g, b) = match format {
            PixelFormat::Rgba8888 => {
                let base = i * 4;
                (
                    pixels[base] as f32 / 255.0,
                    pixels[base + 1] as f32 / 255.0,
                    pixels[base + 2] as f32 / 255.0,
                )
            }
            PixelFormat::Rgba1010102 => {
                let base = i * 4;
                let packed = u32::from_le_bytes([
                    pixels[base],
                    pixels[base + 1],
                    pixels[base + 2],
                    pixels[base + 3],
                ]);
                (
                    (packed & 0x3FF) as f32 / 1023.0,
                    ((packed >> 10) & 0x3FF) as f32 / 1023.0,
                    ((packed >> 20) & 0x3FF) as f32 / 1023.0,
                )
            }
            PixelFormat::RgbaF16 => {
                let base = i * 8;
                let r_h = u16::from_le_bytes([pixels[base], pixels[base + 1]]);
                let g_h = u16::from_le_bytes([pixels[base + 2], pixels[base + 3]]);
                let b_h = u16::from_le_bytes([pixels[base + 4], pixels[base + 5]]);
                (f16_to_f32(r_h), f16_to_f32(g_h), f16_to_f32(b_h))
            }
        };

        // Apply inverse transfer function to get linear values
        let (r_lin, g_lin, b_lin) = match transfer {
            ColorTransfer::Srgb => (srgb_inv_oetf(r), srgb_inv_oetf(g), srgb_inv_oetf(b)),
            ColorTransfer::Linear => (r * scale_to_sdr, g * scale_to_sdr, b * scale_to_sdr),
            ColorTransfer::Pq => {
                let rl = pq_inv_oetf(r) * scale_to_sdr;
                let gl = pq_inv_oetf(g) * scale_to_sdr;
                let bl = pq_inv_oetf(b) * scale_to_sdr;
                (rl, gl, bl)
            }
            ColorTransfer::Hlg => {
                let rl = hlg_inv_oetf(r);
                let gl = hlg_inv_oetf(g);
                let bl = hlg_inv_oetf(b);
                let [ro, go, bo] = hlg_ootf_approx(rl, gl, bl);
                (ro * scale_to_sdr, go * scale_to_sdr, bo * scale_to_sdr)
            }
        };

        let out_idx = i * 3;
        linear[out_idx] = r_lin;
        linear[out_idx + 1] = g_lin;
        linear[out_idx + 2] = b_lin;
    }

    Ok(linear)
}

/// Decode raw pixel data to normalized 0-1 range WITHOUT applying transfer functions.
///
/// This performs only pixel format unpacking (byte/10-bit/f16 → f32 in [0,1]).
/// The result is in the native gamma (OETF) space of the input.
fn decode_pixels_to_normalized(
    pixels: &[u8],
    width: u32,
    height: u32,
    format: PixelFormat,
) -> Result<Vec<f32>> {
    let w = width as usize;
    let h = height as usize;
    let bpp = format.bytes_per_pixel();
    let expected = w * h * bpp;
    if pixels.len() < expected {
        return Err(Error::InvalidParam(format!(
            "pixel buffer too small: need {expected}, got {}",
            pixels.len(),
        )));
    }

    let mut normalized = vec![0.0f32; w * h * 3];

    for i in 0..(w * h) {
        let (r, g, b) = match format {
            PixelFormat::Rgba8888 => {
                let base = i * 4;
                (
                    pixels[base] as f32 / 255.0,
                    pixels[base + 1] as f32 / 255.0,
                    pixels[base + 2] as f32 / 255.0,
                )
            }
            PixelFormat::Rgba1010102 => {
                let base = i * 4;
                let packed = u32::from_le_bytes([
                    pixels[base],
                    pixels[base + 1],
                    pixels[base + 2],
                    pixels[base + 3],
                ]);
                (
                    (packed & 0x3FF) as f32 / 1023.0,
                    ((packed >> 10) & 0x3FF) as f32 / 1023.0,
                    ((packed >> 20) & 0x3FF) as f32 / 1023.0,
                )
            }
            PixelFormat::RgbaF16 => {
                let base = i * 8;
                let r_h = u16::from_le_bytes([pixels[base], pixels[base + 1]]);
                let g_h = u16::from_le_bytes([pixels[base + 2], pixels[base + 3]]);
                let b_h = u16::from_le_bytes([pixels[base + 4], pixels[base + 5]]);
                (f16_to_f32(r_h), f16_to_f32(g_h), f16_to_f32(b_h))
            }
        };

        let out_idx = i * 3;
        normalized[out_idx] = r;
        normalized[out_idx + 1] = g;
        normalized[out_idx + 2] = b;
    }

    Ok(normalized)
}

/// Apply transfer function to a normalized (gamma-space) buffer, producing linear values.
///
/// Used by the auto-tonemap path which needs linear HDR for tone mapping.
fn linearize_normalized(normalized: &[f32], transfer: ColorTransfer) -> Result<Vec<f32>> {
    let peak = reference_display_peak_nits(transfer);
    let scale_to_sdr = peak / SDR_WHITE_NITS;
    let npx = normalized.len() / 3;
    let mut linear = vec![0.0f32; normalized.len()];

    for i in 0..npx {
        let idx = i * 3;
        let r = normalized[idx];
        let g = normalized[idx + 1];
        let b = normalized[idx + 2];

        let (r_lin, g_lin, b_lin) = match transfer {
            ColorTransfer::Srgb => (srgb_inv_oetf(r), srgb_inv_oetf(g), srgb_inv_oetf(b)),
            ColorTransfer::Linear => (r * scale_to_sdr, g * scale_to_sdr, b * scale_to_sdr),
            ColorTransfer::Pq => {
                let rl = pq_inv_oetf(r) * scale_to_sdr;
                let gl = pq_inv_oetf(g) * scale_to_sdr;
                let bl = pq_inv_oetf(b) * scale_to_sdr;
                (rl, gl, bl)
            }
            ColorTransfer::Hlg => {
                let rl = hlg_inv_oetf(r);
                let gl = hlg_inv_oetf(g);
                let bl = hlg_inv_oetf(b);
                let [ro, go, bo] = hlg_ootf_approx(rl, gl, bl);
                (ro * scale_to_sdr, go * scale_to_sdr, bo * scale_to_sdr)
            }
        };

        linear[idx] = r_lin;
        linear[idx + 1] = g_lin;
        linear[idx + 2] = b_lin;
    }

    Ok(linear)
}

/// Convert IEEE 754 half-precision (f16 as u16) to f32.
fn f16_to_f32(val: u16) -> f32 {
    let sign = ((val >> 15) & 1) as u32;
    let exp = ((val >> 10) & 0x1F) as u32;
    let mantissa = (val & 0x3FF) as u32;

    if exp == 0 {
        if mantissa == 0 {
            return f32::from_bits(sign << 31);
        }
        // Subnormal
        let mut m = mantissa;
        let mut e = 1u32;
        while m & 0x400 == 0 {
            m <<= 1;
            e += 1;
        }
        let f32_exp = 127 - 15 + 1 - e;
        let f32_mantissa = (m & 0x3FF) << 13;
        return f32::from_bits((sign << 31) | (f32_exp << 23) | f32_mantissa);
    }

    if exp == 31 {
        // Inf or NaN
        let f32_mantissa = mantissa << 13;
        return f32::from_bits((sign << 31) | (0xFF << 23) | f32_mantissa);
    }

    let f32_exp = (exp as i32 - 15 + 127) as u32;
    let f32_mantissa = mantissa << 13;
    f32::from_bits((sign << 31) | (f32_exp << 23) | f32_mantissa)
}

/// High-level UltraHDR encoder with builder pattern.
pub struct Encoder {
    hdr_pixels: Option<Vec<u8>>,
    hdr_width: u32,
    hdr_height: u32,
    hdr_format: PixelFormat,
    hdr_gamut: ColorGamut,
    hdr_transfer: ColorTransfer,
    sdr_jpeg: Option<Vec<u8>>,
    sdr_pixels: Option<Vec<u8>>,
    sdr_width: u32,
    sdr_height: u32,
    sdr_gamut: ColorGamut,
    quality: u8,
    gainmap_quality: u8,
    gainmap_scale: u32,
    multichannel: bool,
    target_display_peak_nits: f32,
}

impl Default for Encoder {
    fn default() -> Self {
        Self::new()
    }
}

impl Encoder {
    /// Create a new encoder with default settings.
    pub fn new() -> Self {
        Self {
            hdr_pixels: None,
            hdr_width: 0,
            hdr_height: 0,
            hdr_format: PixelFormat::Rgba8888,
            hdr_gamut: ColorGamut::Bt709,
            hdr_transfer: ColorTransfer::Srgb,
            sdr_jpeg: None,
            sdr_pixels: None,
            sdr_width: 0,
            sdr_height: 0,
            sdr_gamut: ColorGamut::Bt709,
            quality: 95,
            gainmap_quality: 85,
            gainmap_scale: 4,
            multichannel: false,
            target_display_peak_nits: 1600.0,
        }
    }

    /// Set HDR input as raw pixels.
    pub fn hdr_raw(
        mut self,
        pixels: &[u8],
        width: u32,
        height: u32,
        format: PixelFormat,
        gamut: ColorGamut,
        transfer: ColorTransfer,
    ) -> Self {
        self.hdr_pixels = Some(pixels.to_vec());
        self.hdr_width = width;
        self.hdr_height = height;
        self.hdr_format = format;
        self.hdr_gamut = gamut;
        self.hdr_transfer = transfer;
        self
    }

    /// Set SDR input as a compressed JPEG.
    pub fn sdr_compressed(mut self, jpeg: &[u8], gamut: ColorGamut) -> Self {
        self.sdr_jpeg = Some(jpeg.to_vec());
        self.sdr_gamut = gamut;
        self
    }

    /// Set SDR input as raw pixels (RGBA8888).
    ///
    /// When raw SDR is provided, the encoder uses these pixels directly
    /// (bypassing JPEG decode losses) for gain map computation, and
    /// JPEG-encodes them for the output base image.
    pub fn sdr_raw(mut self, pixels: &[u8], width: u32, height: u32, gamut: ColorGamut) -> Self {
        self.sdr_pixels = Some(pixels.to_vec());
        self.sdr_width = width;
        self.sdr_height = height;
        self.sdr_gamut = gamut;
        self
    }

    /// Set JPEG encoding quality for the primary SDR image (1-100).
    pub fn quality(mut self, q: u8) -> Self {
        self.quality = q;
        self
    }

    /// Set JPEG encoding quality for the gain map image (1-100).
    pub fn gainmap_quality(mut self, q: u8) -> Self {
        self.gainmap_quality = q;
        self
    }

    /// Set gain map downsampling scale factor (1=full, 2=half, 4=quarter).
    pub fn gainmap_scale(mut self, s: u32) -> Self {
        self.gainmap_scale = s;
        self
    }

    /// Whether to compute gain per-channel (true) or luminance-only (false).
    pub fn multichannel_gainmap(mut self, mc: bool) -> Self {
        self.multichannel = mc;
        self
    }

    /// Target display peak brightness in nits.
    pub fn target_display_peak_nits(mut self, nits: f32) -> Self {
        self.target_display_peak_nits = nits;
        self
    }

    /// Encode the UltraHDR JPEG.
    ///
    /// If no SDR JPEG is provided, automatically generates an SDR image from the
    /// HDR input using global Reinhard tone mapping (API-0 scenario).
    pub fn encode(self) -> Result<Vec<u8>> {
        let hdr_pixels = self
            .hdr_pixels
            .as_ref()
            .ok_or_else(|| Error::InvalidParam("HDR input not set".into()))?;

        // Decode HDR to normalized gamma space (no transfer function applied)
        let hdr_normalized = decode_pixels_to_normalized(
            hdr_pixels,
            self.hdr_width,
            self.hdr_height,
            self.hdr_format,
        )?;

        let w = self.hdr_width as usize;
        let h = self.hdr_height as usize;

        // Resolve SDR: use raw pixels, compressed JPEG, or auto-tonemap from HDR
        let (sdr_jpeg_data, sdr_gamma) = if let Some(sdr_pixels) = &self.sdr_pixels {
            // Raw SDR path: decode RGBA8888 directly, no JPEG losses
            if self.sdr_width != self.hdr_width || self.sdr_height != self.hdr_height {
                return Err(Error::InvalidParam(format!(
                    "SDR dimensions {}x{} don't match HDR {}x{}",
                    self.sdr_width, self.sdr_height, self.hdr_width, self.hdr_height,
                )));
            }
            let npx = w * h;
            let expected_bytes = npx * 4; // RGBA8888
            if sdr_pixels.len() < expected_bytes {
                return Err(Error::InvalidParam(format!(
                    "SDR pixel buffer too small: need {expected_bytes}, got {}",
                    sdr_pixels.len(),
                )));
            }
            // Keep SDR in sRGB gamma space (generate_gainmap averages in gamma)
            let mut gamma = vec![0.0f32; npx * 3];
            let mut sdr_rgb = vec![0u8; npx * 3];
            for i in 0..npx {
                gamma[i * 3] = sdr_pixels[i * 4] as f32 / 255.0;
                gamma[i * 3 + 1] = sdr_pixels[i * 4 + 1] as f32 / 255.0;
                gamma[i * 3 + 2] = sdr_pixels[i * 4 + 2] as f32 / 255.0;
                // Also build RGB for JPEG encoding
                sdr_rgb[i * 3] = sdr_pixels[i * 4];
                sdr_rgb[i * 3 + 1] = sdr_pixels[i * 4 + 1];
                sdr_rgb[i * 3 + 2] = sdr_pixels[i * 4 + 2];
            }
            // JPEG encode the raw SDR for the output base image
            let jpeg = crate::jpeg::encode::encode_rgb_to_jpeg(
                &sdr_rgb,
                self.hdr_width,
                self.hdr_height,
                self.quality,
            )?;
            (AutoSdr::Generated(jpeg), gamma)
        } else if let Some(sdr_jpeg) = &self.sdr_jpeg {
            let sdr_decoded = crate::jpeg::decode::decode_jpeg(sdr_jpeg)?;
            if sdr_decoded.width != self.hdr_width || sdr_decoded.height != self.hdr_height {
                return Err(Error::InvalidParam(format!(
                    "SDR dimensions {}x{} don't match HDR {}x{}",
                    sdr_decoded.width, sdr_decoded.height, self.hdr_width, self.hdr_height,
                )));
            }
            let gamma = {
                let mut gamma = vec![0.0f32; w * h * 3];
                for i in 0..(w * h) {
                    let base = i * 3;
                    gamma[base] = sdr_decoded.pixels[base] as f32 / 255.0;
                    gamma[base + 1] = sdr_decoded.pixels[base + 1] as f32 / 255.0;
                    gamma[base + 2] = sdr_decoded.pixels[base + 2] as f32 / 255.0;
                }
                gamma
            };
            let icc = sdr_decoded.icc_profile;
            (AutoSdr::Provided(sdr_jpeg.clone(), icc), gamma)
        } else {
            // Auto tone-map HDR → SDR
            // Linearize HDR for tone mapping (need linear values for global_tonemap)
            let hdr_linear = linearize_normalized(&hdr_normalized, self.hdr_transfer)?;
            let headroom = self.target_display_peak_nits / SDR_WHITE_NITS;
            let mut sdr_rgb = vec![0u8; w * h * 3];
            let mut sdr_gamma = vec![0.0f32; w * h * 3];
            for i in 0..(w * h) {
                let base = i * 3;
                let rgb = [hdr_linear[base], hdr_linear[base + 1], hdr_linear[base + 2]];
                let (tm_rgb, _, _) = global_tonemap(rgb, headroom, false);
                // Encode to sRGB gamma for JPEG and for gain map (gamma-space input)
                let r_gamma = srgb_oetf(tm_rgb[0].clamp(0.0, 1.0));
                let g_gamma = srgb_oetf(tm_rgb[1].clamp(0.0, 1.0));
                let b_gamma = srgb_oetf(tm_rgb[2].clamp(0.0, 1.0));
                sdr_rgb[base] = (r_gamma * 255.0 + 0.5) as u8;
                sdr_rgb[base + 1] = (g_gamma * 255.0 + 0.5) as u8;
                sdr_rgb[base + 2] = (b_gamma * 255.0 + 0.5) as u8;
                // Use the u8-quantized values for gain map to match C++ behavior
                sdr_gamma[base] = sdr_rgb[base] as f32 / 255.0;
                sdr_gamma[base + 1] = sdr_rgb[base + 1] as f32 / 255.0;
                sdr_gamma[base + 2] = sdr_rgb[base + 2] as f32 / 255.0;
            }
            let jpeg = crate::jpeg::encode::encode_rgb_to_jpeg(
                &sdr_rgb,
                self.hdr_width,
                self.hdr_height,
                self.quality,
            )?;
            (AutoSdr::Generated(jpeg), sdr_gamma)
        };

        // Generate gain map (inputs are in gamma space; generate_gainmap linearizes per-block)
        let (gainmap, metadata) = generate_gainmap(
            &sdr_gamma,
            &hdr_normalized,
            self.hdr_width,
            self.hdr_height,
            self.sdr_gamut,
            self.hdr_gamut,
            self.hdr_transfer,
            self.gainmap_scale,
            self.multichannel,
            self.target_display_peak_nits,
            false,
        )?;

        // Encode gain map as JPEG
        let map_w = (self.hdr_width as usize).div_ceil(self.gainmap_scale as usize);
        let map_h = (self.hdr_height as usize).div_ceil(self.gainmap_scale as usize);

        let gainmap_jpeg = if self.multichannel {
            crate::jpeg::encode::encode_rgb_to_jpeg(
                &gainmap,
                map_w as u32,
                map_h as u32,
                self.gainmap_quality,
            )?
        } else {
            crate::jpeg::encode::encode_grayscale_to_jpeg(
                &gainmap,
                map_w as u32,
                map_h as u32,
                self.gainmap_quality,
            )?
        };

        // Get ICC profile and SDR JPEG bytes
        let (sdr_jpeg_bytes, icc) = match &sdr_jpeg_data {
            AutoSdr::Provided(jpeg, icc_opt) => (jpeg.as_slice(), icc_opt.as_deref()),
            AutoSdr::Generated(jpeg) => (jpeg.as_slice(), None),
        };

        // Assemble the UltraHDR JPEG
        assemble_ultrahdr(sdr_jpeg_bytes, &gainmap_jpeg, &metadata, None, icc)
    }
}

/// Internal helper to track SDR JPEG source.
enum AutoSdr {
    Provided(Vec<u8>, Option<Vec<u8>>),
    Generated(Vec<u8>),
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_minimal_jpeg() -> Vec<u8> {
        crate::jpeg::encode::encode_rgb_to_jpeg(&[128u8; 2 * 2 * 3], 2, 2, 90)
            .expect("failed to create test JPEG")
    }

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

    // Task 24: Gain map generation
    #[test]
    fn generate_gainmap_uniform_images() {
        let width = 4;
        let height = 4;
        let sdr_linear: Vec<f32> = vec![0.5; width * height * 3];
        let hdr_linear: Vec<f32> = vec![0.5; width * height * 3];
        let (gainmap, _metadata) = generate_gainmap(
            &sdr_linear,
            &hdr_linear,
            width as u32,
            height as u32,
            ColorGamut::Bt709,
            ColorGamut::Bt709,
            ColorTransfer::Srgb,
            1,
            false,
            1.0,
            false,
        )
        .unwrap();
        assert_eq!(gainmap.len(), width * height);
    }

    #[test]
    fn generate_gainmap_hdr_brighter() {
        // Use varying pixel values so min/max gain differ
        let width = 4;
        let height = 4;
        let mut sdr_linear = vec![0.0f32; width * height * 3];
        let mut hdr_linear = vec![0.0f32; width * height * 3];
        for i in 0..(width * height) {
            let v = (i as f32 + 1.0) / (width * height) as f32;
            sdr_linear[i * 3] = v * 0.5;
            sdr_linear[i * 3 + 1] = v * 0.5;
            sdr_linear[i * 3 + 2] = v * 0.5;
            hdr_linear[i * 3] = v * 1.0;
            hdr_linear[i * 3 + 1] = v * 1.0;
            hdr_linear[i * 3 + 2] = v * 1.0;
        }
        let (gainmap, metadata) = generate_gainmap(
            &sdr_linear,
            &hdr_linear,
            width as u32,
            height as u32,
            ColorGamut::Bt709,
            ColorGamut::Bt709,
            ColorTransfer::Srgb,
            1,
            false,
            4.0 * SDR_WHITE_NITS,
            false,
        )
        .unwrap();
        // Metadata should reflect boost > 1
        assert!(
            metadata.max_content_boost[0] > 1.0,
            "max_content_boost should be > 1 when HDR is brighter"
        );
        assert_eq!(gainmap.len(), width * height);
    }

    #[test]
    fn generate_gainmap_with_scale() {
        let width = 8;
        let height = 8;
        let sdr_linear: Vec<f32> = vec![0.5; width * height * 3];
        let hdr_linear: Vec<f32> = vec![0.5; width * height * 3];
        let (gainmap, _) = generate_gainmap(
            &sdr_linear,
            &hdr_linear,
            width as u32,
            height as u32,
            ColorGamut::Bt709,
            ColorGamut::Bt709,
            ColorTransfer::Srgb,
            2,
            false,
            1.0,
            false,
        )
        .unwrap();
        // 8/2 = 4, so gain map should be 4x4 = 16 pixels
        assert_eq!(gainmap.len(), 4 * 4);
    }

    #[test]
    fn generate_gainmap_multichannel() {
        let width = 4;
        let height = 4;
        let sdr_linear: Vec<f32> = vec![0.5; width * height * 3];
        let hdr_linear: Vec<f32> = vec![0.5; width * height * 3];
        let (gainmap, _) = generate_gainmap(
            &sdr_linear,
            &hdr_linear,
            width as u32,
            height as u32,
            ColorGamut::Bt709,
            ColorGamut::Bt709,
            ColorTransfer::Srgb,
            1,
            true,
            1.0,
            false,
        )
        .unwrap();
        // Multichannel: 3 values per pixel
        assert_eq!(gainmap.len(), width * height * 3);
    }

    #[test]
    fn generate_gainmap_metadata_matches_cpp() {
        // C++ reference values (gainmapmath.h):
        //   kSdrOffset = kHdrOffset = 1e-7
        //   use_base_cg = false  (raw / API-0/API-1 path)
        let width = 8;
        let height = 8;
        let mut sdr_linear = vec![0.0f32; width * height * 3];
        let mut hdr_linear = vec![0.0f32; width * height * 3];
        for i in 0..(width * height) {
            let v = (i as f32 + 1.0) / (width * height) as f32;
            for c in 0..3 {
                sdr_linear[i * 3 + c] = v * 0.5;
                hdr_linear[i * 3 + c] = v * 2.0;
            }
        }
        let (_, metadata) = generate_gainmap(
            &sdr_linear,
            &hdr_linear,
            width as u32,
            height as u32,
            ColorGamut::Bt709,
            ColorGamut::Bt709,
            ColorTransfer::Srgb,
            1,
            false,
            1600.0,
            false,
        )
        .unwrap();

        // C++ sets offset_sdr = offset_hdr = kSdrOffset = kHdrOffset = 1e-7
        for ch in 0..3 {
            assert!(
                metadata.offset_sdr[ch] < 0.001,
                "offset_sdr[{ch}] should be ≈1e-7, got {}",
                metadata.offset_sdr[ch]
            );
            assert!(
                metadata.offset_hdr[ch] < 0.001,
                "offset_hdr[{ch}] should be ≈1e-7, got {}",
                metadata.offset_hdr[ch]
            );
        }

        // C++ API-0/API-1 (raw input) sets use_base_cg = false
        assert!(
            !metadata.use_base_cg,
            "use_base_cg should be false for raw input"
        );
    }

    #[test]
    fn generate_gainmap_allows_negative_gain() {
        // HDR dimmer than SDR everywhere → all gain < 1 → all log2 < 0
        // C++ clamps to (-14.3, 15.6), Rust incorrectly uses max(0).min(headroom)
        // which forces max_gain_log2 to 0 when all gains are negative.
        let width = 4;
        let height = 4;
        let mut sdr_linear = vec![0.0f32; width * height * 3];
        let mut hdr_linear = vec![0.0f32; width * height * 3];
        for i in 0..(width * height) {
            let v = (i as f32 + 1.0) / (width * height) as f32;
            for c in 0..3 {
                sdr_linear[i * 3 + c] = v * 0.8; // SDR brighter
                hdr_linear[i * 3 + c] = v * 0.3; // HDR dimmer
            }
        }

        let (_, metadata) = generate_gainmap(
            &sdr_linear,
            &hdr_linear,
            width as u32,
            height as u32,
            ColorGamut::Bt709,
            ColorGamut::Bt709,
            ColorTransfer::Srgb,
            1,
            false,
            1600.0,
            false,
        )
        .unwrap();

        // Both min and max content boost should be < 1.0
        // C++ preserves the negative max_gain_log2 via clamp(-14.3, 15.6)
        assert!(
            metadata.max_content_boost[0] < 1.0,
            "max_content_boost should be < 1.0 when all HDR dimmer than SDR, got {}",
            metadata.max_content_boost[0]
        );
        assert!(
            metadata.min_content_boost[0] < 1.0,
            "min_content_boost should be < 1.0 when HDR dimmer than SDR, got {}",
            metadata.min_content_boost[0]
        );
    }

    // Task 25: UltraHDR JPEG assembly
    #[test]
    fn assemble_ultrahdr_has_soi_eoi() {
        let sdr_jpeg = create_minimal_jpeg();
        let gainmap_jpeg = create_minimal_jpeg();
        let meta = default_test_metadata();
        let result = assemble_ultrahdr(&sdr_jpeg, &gainmap_jpeg, &meta, None, None);
        assert!(result.is_ok());
        let out = result.unwrap();
        assert_eq!(&out[..2], &[0xFF, 0xD8]); // SOI
        assert_eq!(&out[out.len() - 2..], &[0xFF, 0xD9]); // EOI
    }

    #[test]
    fn assemble_ultrahdr_contains_xmp() {
        let sdr_jpeg = create_minimal_jpeg();
        let gainmap_jpeg = create_minimal_jpeg();
        let meta = default_test_metadata();
        let out = assemble_ultrahdr(&sdr_jpeg, &gainmap_jpeg, &meta, None, None).unwrap();
        // Check that XMP signature is present
        let xmp_sig = b"http://ns.adobe.com/xap/1.0/\0";
        let contains_xmp = out.windows(xmp_sig.len()).any(|w| w == xmp_sig);
        assert!(contains_xmp, "output should contain XMP metadata");
    }

    #[test]
    fn assemble_ultrahdr_contains_mpf() {
        let sdr_jpeg = create_minimal_jpeg();
        let gainmap_jpeg = create_minimal_jpeg();
        let meta = default_test_metadata();
        let out = assemble_ultrahdr(&sdr_jpeg, &gainmap_jpeg, &meta, None, None).unwrap();
        // Check that MPF signature is present
        let contains_mpf = out.windows(4).any(|w| w == b"MPF\0");
        assert!(contains_mpf, "output should contain MPF segment");
    }

    #[test]
    fn assemble_ultrahdr_contains_iso_metadata() {
        let sdr_jpeg = create_minimal_jpeg();
        let gainmap_jpeg = create_minimal_jpeg();
        let meta = default_test_metadata();
        let out = assemble_ultrahdr(&sdr_jpeg, &gainmap_jpeg, &meta, None, None).unwrap();
        let contains_iso = out
            .windows(ISO_GAINMAP_TAG.len())
            .any(|w| w == ISO_GAINMAP_TAG);
        assert!(contains_iso, "output should contain ISO 21496-1 metadata");
    }

    #[test]
    fn assemble_ultrahdr_secondary_is_valid_jpeg() {
        let sdr_jpeg = create_minimal_jpeg();
        let gainmap_jpeg = create_minimal_jpeg();
        let meta = default_test_metadata();
        let out = assemble_ultrahdr(&sdr_jpeg, &gainmap_jpeg, &meta, None, None).unwrap();
        // The gain map JPEG should be appended at the end
        // Find the last SOI marker (the secondary image)
        let mut last_soi = 0;
        for i in 2..out.len() - 1 {
            if out[i] == 0xFF && out[i + 1] == 0xD8 {
                last_soi = i;
            }
        }
        assert!(last_soi > 0, "should have a secondary JPEG");
        assert_eq!(out[last_soi], 0xFF);
        assert_eq!(out[last_soi + 1], 0xD8);
    }

    #[test]
    fn metadata_to_frac_clamps_negative_headroom() {
        // hdr_capacity_min < 1.0 means log2 < 0; to_n_u should clamp to 0
        let sdr_jpeg = create_minimal_jpeg();
        let gainmap_jpeg = create_minimal_jpeg();
        let mut meta = default_test_metadata();
        meta.hdr_capacity_min = 0.5; // log2(0.5) = -1.0, would be negative
        let result = assemble_ultrahdr(&sdr_jpeg, &gainmap_jpeg, &meta, None, None);
        assert!(result.is_ok(), "should handle sub-1.0 hdr_capacity_min");
    }

    // Task 27: Encoder builder API
    #[test]
    fn encoder_builder_api() {
        let hdr_pixels = vec![0u8; 4 * 4 * 4]; // 4x4 RGBA8888
        let sdr_jpeg =
            crate::jpeg::encode::encode_rgb_to_jpeg(&[128u8; 4 * 4 * 3], 4, 4, 90).unwrap();
        let result = Encoder::new()
            .hdr_raw(
                &hdr_pixels,
                4,
                4,
                PixelFormat::Rgba8888,
                ColorGamut::Bt709,
                ColorTransfer::Pq,
            )
            .sdr_compressed(&sdr_jpeg, ColorGamut::Bt709)
            .quality(95)
            .gainmap_quality(85)
            .gainmap_scale(4)
            .multichannel_gainmap(false)
            .target_display_peak_nits(1600.0)
            .encode();
        // Should produce a valid UltraHDR JPEG
        assert!(
            result.is_ok(),
            "encode() should succeed: {:?}",
            result.err()
        );
        let out = result.unwrap();
        assert_eq!(&out[..2], &[0xFF, 0xD8]); // SOI
    }

    #[test]
    fn encoder_missing_hdr_input() {
        let sdr_jpeg = create_minimal_jpeg();
        let result = Encoder::new()
            .sdr_compressed(&sdr_jpeg, ColorGamut::Bt709)
            .encode();
        assert!(result.is_err());
    }

    #[test]
    fn encoder_auto_tonemap_when_no_sdr() {
        let hdr_pixels = vec![128u8; 4 * 4 * 4];
        let result = Encoder::new()
            .hdr_raw(
                &hdr_pixels,
                4,
                4,
                PixelFormat::Rgba8888,
                ColorGamut::Bt709,
                ColorTransfer::Srgb,
            )
            .encode();
        assert!(
            result.is_ok(),
            "auto tonemap should succeed: {:?}",
            result.err()
        );
        let out = result.unwrap();
        assert_eq!(&out[..2], &[0xFF, 0xD8]);
    }
}
