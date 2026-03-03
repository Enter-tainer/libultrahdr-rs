//! Debug test: compare Rust encoder vs C++ encoder output.
//!
//! Generates synthetic SDR+HDR images, encodes with both, then compares:
//! - File sizes
//! - Gain map metadata (max_content_boost, hdr_capacity_max, etc.)
//! - Gain map pixel statistics
//! - Cross-decode results (Rust output decoded by C++ and vice versa)
//!
//! Run: CARGO_HOME=/tmp/cargo-home cargo test -p ultrahdr --test debug_encode -- --nocapture --ignored

use std::io::Write as _;

use ultrahdr::decoder::{Decoder, extract_gainmap_jpeg};
use ultrahdr::encoder::Encoder;
use ultrahdr::types::*;
use ultrahdr_sys::*;

const W: u32 = 64;
const H: u32 = 64;

// ---------- synthetic image generators ----------

/// Create RGBA8888 SDR + RGBA1010102 HDR pair: solid white.
fn gen_white() -> (Vec<u8>, Vec<u8>) {
    let npx = (W * H) as usize;
    // SDR: white = (255,255,255,255) per pixel
    let sdr: Vec<u8> = vec![255u8; npx * 4];
    // HDR 1010102: R=1023,G=1023,B=1023,A=3 packed LE
    let mut hdr = vec![0u8; npx * 4];
    for i in 0..npx {
        let packed: u32 = 1023 | (1023 << 10) | (1023 << 20) | (3 << 30);
        hdr[i * 4..i * 4 + 4].copy_from_slice(&packed.to_le_bytes());
    }
    (sdr, hdr)
}

/// Create RGBA8888 SDR + RGBA1010102 HDR pair: horizontal gradient.
fn gen_gradient() -> (Vec<u8>, Vec<u8>) {
    let npx = (W * H) as usize;
    let mut sdr = vec![0u8; npx * 4];
    let mut hdr = vec![0u8; npx * 4];
    for y in 0..H {
        for x in 0..W {
            let i = (y * W + x) as usize;
            let t = x as f32 / (W - 1) as f32; // 0..1
            // SDR: linear gradient 0..255
            let sdr_val = (t * 255.0 + 0.5) as u8;
            sdr[i * 4] = sdr_val;
            sdr[i * 4 + 1] = sdr_val;
            sdr[i * 4 + 2] = sdr_val;
            sdr[i * 4 + 3] = 255;
            // HDR: same gradient mapped to 0..1023
            let hdr_val = (t * 1023.0 + 0.5) as u32;
            let packed: u32 = hdr_val | (hdr_val << 10) | (hdr_val << 20) | (3 << 30);
            hdr[i * 4..i * 4 + 4].copy_from_slice(&packed.to_le_bytes());
        }
    }
    (sdr, hdr)
}

// ---------- Rust encoder ----------

fn rust_encode(sdr: &[u8], hdr: &[u8], label: &str) -> Vec<u8> {
    Encoder::new()
        .hdr_raw(
            hdr,
            W,
            H,
            PixelFormat::Rgba1010102,
            ColorGamut::Bt2100,
            ColorTransfer::Hlg,
        )
        .sdr_raw(sdr, W, H, ColorGamut::Bt709)
        .quality(95)
        .gainmap_quality(85)
        .gainmap_scale(4)
        .multichannel_gainmap(false)
        .target_display_peak_nits(1600.0)
        .encode()
        .unwrap_or_else(|e| panic!("Rust encode failed for {label}: {e}"))
}

// ---------- C++ encoder via FFI ----------

unsafe fn cpp_encode(sdr: &[u8], hdr: &[u8], label: &str) -> Vec<u8> {
    unsafe {
        let enc = uhdr_create_encoder();
        assert!(!enc.is_null(), "uhdr_create_encoder returned null");

        // Set HDR raw image (RGBA1010102, HLG, BT.2100)
        let mut hdr_img = uhdr_raw_image {
            fmt: uhdr_img_fmt::UHDR_IMG_FMT_32bppRGBA1010102,
            cg: uhdr_color_gamut::UHDR_CG_BT_2100,
            ct: uhdr_color_transfer::UHDR_CT_HLG,
            range: uhdr_color_range::UHDR_CR_FULL_RANGE,
            w: W,
            h: H,
            planes: [
                hdr.as_ptr() as *mut _,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
            ],
            stride: [W, 0, 0],
        };
        let err = uhdr_enc_set_raw_image(enc, &mut hdr_img, uhdr_img_label::UHDR_HDR_IMG);
        assert_eq!(
            err.error_code,
            uhdr_codec_err::UHDR_CODEC_OK,
            "set HDR raw failed for {label}"
        );

        // Set SDR raw image (RGBA8888, sRGB, BT.709)
        let mut sdr_img = uhdr_raw_image {
            fmt: uhdr_img_fmt::UHDR_IMG_FMT_32bppRGBA8888,
            cg: uhdr_color_gamut::UHDR_CG_BT_709,
            ct: uhdr_color_transfer::UHDR_CT_SRGB,
            range: uhdr_color_range::UHDR_CR_FULL_RANGE,
            w: W,
            h: H,
            planes: [
                sdr.as_ptr() as *mut _,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
            ],
            stride: [W, 0, 0],
        };
        let err = uhdr_enc_set_raw_image(enc, &mut sdr_img, uhdr_img_label::UHDR_SDR_IMG);
        assert_eq!(
            err.error_code,
            uhdr_codec_err::UHDR_CODEC_OK,
            "set SDR raw failed for {label}"
        );

        // Quality
        let err = uhdr_enc_set_quality(enc, 95, uhdr_img_label::UHDR_BASE_IMG);
        assert_eq!(err.error_code, uhdr_codec_err::UHDR_CODEC_OK);
        let err = uhdr_enc_set_quality(enc, 85, uhdr_img_label::UHDR_GAIN_MAP_IMG);
        assert_eq!(err.error_code, uhdr_codec_err::UHDR_CODEC_OK);

        // Gainmap settings
        let err = uhdr_enc_set_using_multi_channel_gainmap(enc, 0); // single channel
        assert_eq!(err.error_code, uhdr_codec_err::UHDR_CODEC_OK);
        let err = uhdr_enc_set_gainmap_scale_factor(enc, 4);
        assert_eq!(err.error_code, uhdr_codec_err::UHDR_CODEC_OK);
        let err = uhdr_enc_set_target_display_peak_brightness(enc, 1600.0);
        assert_eq!(err.error_code, uhdr_codec_err::UHDR_CODEC_OK);

        // Encode
        let err = uhdr_encode(enc);
        if err.error_code != uhdr_codec_err::UHDR_CODEC_OK {
            let detail = if err.has_detail != 0 {
                std::ffi::CStr::from_ptr(err.detail.as_ptr())
                    .to_string_lossy()
                    .to_string()
            } else {
                "no detail".to_string()
            };
            panic!(
                "C++ encode failed for {label}: {:?} - {detail}",
                err.error_code
            );
        }

        // Get result
        let stream = uhdr_get_encoded_stream(enc);
        assert!(!stream.is_null(), "get_encoded_stream returned null");
        let data = std::slice::from_raw_parts((*stream).data as *const u8, (*stream).data_sz);
        let result = data.to_vec();

        uhdr_release_encoder(enc);
        result
    }
}

// ---------- C++ decoder: probe metadata ----------

unsafe fn cpp_probe_metadata(jpeg: &[u8]) -> uhdr_gainmap_metadata {
    unsafe {
        let dec = uhdr_create_decoder();
        assert!(!dec.is_null());

        let mut img = uhdr_compressed_image {
            data: jpeg.as_ptr() as *mut _,
            data_sz: jpeg.len(),
            capacity: jpeg.len(),
            cg: uhdr_color_gamut::UHDR_CG_UNSPECIFIED,
            ct: uhdr_color_transfer::UHDR_CT_UNSPECIFIED,
            range: uhdr_color_range::UHDR_CR_UNSPECIFIED,
        };
        let err = uhdr_dec_set_image(dec, &mut img);
        assert_eq!(err.error_code, uhdr_codec_err::UHDR_CODEC_OK);

        let err = uhdr_dec_probe(dec);
        assert_eq!(
            err.error_code,
            uhdr_codec_err::UHDR_CODEC_OK,
            "probe failed"
        );

        let meta_ptr = uhdr_dec_get_gainmap_metadata(dec);
        assert!(!meta_ptr.is_null(), "get_gainmap_metadata returned null");
        let meta = *meta_ptr;

        uhdr_release_decoder(dec);
        meta
    }
}

// ---------- C++ decoder: decode to 1010102 HLG ----------

unsafe fn cpp_decode_to_1010102(jpeg: &[u8]) -> Vec<u8> {
    unsafe {
        let dec = uhdr_create_decoder();
        assert!(!dec.is_null());

        let mut img = uhdr_compressed_image {
            data: jpeg.as_ptr() as *mut _,
            data_sz: jpeg.len(),
            capacity: jpeg.len(),
            cg: uhdr_color_gamut::UHDR_CG_UNSPECIFIED,
            ct: uhdr_color_transfer::UHDR_CT_UNSPECIFIED,
            range: uhdr_color_range::UHDR_CR_UNSPECIFIED,
        };
        let err = uhdr_dec_set_image(dec, &mut img);
        assert_eq!(err.error_code, uhdr_codec_err::UHDR_CODEC_OK);

        let err = uhdr_dec_set_out_img_format(dec, uhdr_img_fmt::UHDR_IMG_FMT_32bppRGBA1010102);
        assert_eq!(err.error_code, uhdr_codec_err::UHDR_CODEC_OK);
        let err = uhdr_dec_set_out_color_transfer(dec, uhdr_color_transfer::UHDR_CT_HLG);
        assert_eq!(err.error_code, uhdr_codec_err::UHDR_CODEC_OK);
        let err = uhdr_dec_set_out_max_display_boost(dec, 4.0);
        assert_eq!(err.error_code, uhdr_codec_err::UHDR_CODEC_OK);

        let err = uhdr_decode(dec);
        assert_eq!(
            err.error_code,
            uhdr_codec_err::UHDR_CODEC_OK,
            "decode failed"
        );

        let raw_ptr = uhdr_get_decoded_image(dec);
        assert!(!raw_ptr.is_null());
        let raw = &*raw_ptr;
        let nbytes = (raw.w * raw.h * 4) as usize;
        let data = std::slice::from_raw_parts(raw.planes[0] as *const u8, nbytes);
        let result = data.to_vec();

        uhdr_release_decoder(dec);
        result
    }
}

// ---------- stats helpers ----------

fn gainmap_pixel_stats(jpeg: &[u8]) -> (u8, u8, f64) {
    // Decode gain map JPEG to get raw pixels
    let extract = extract_gainmap_jpeg(jpeg).expect("extract failed");
    let extract = extract.expect("no gain map found");
    let gm_jpeg = &extract.gainmap_jpeg;
    // Use jpeg-decoder to get raw pixels
    let mut decoder = jpeg_decoder::Decoder::new(gm_jpeg.as_slice());
    let pixels = decoder.decode().expect("gain map JPEG decode failed");
    let min = pixels.iter().copied().min().unwrap_or(0);
    let max = pixels.iter().copied().max().unwrap_or(0);
    let mean = pixels.iter().map(|&v| v as f64).sum::<f64>() / pixels.len() as f64;
    (min, max, mean)
}

fn max_1010102_value(data: &[u8]) -> u32 {
    let mut max_val = 0u32;
    for chunk in data.chunks_exact(4) {
        let packed = u32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]);
        let r = packed & 0x3FF;
        let g = (packed >> 10) & 0x3FF;
        let b = (packed >> 20) & 0x3FF;
        max_val = max_val.max(r).max(g).max(b);
    }
    max_val
}

// ---------- main test ----------

#[test]
#[ignore]
fn debug_encode_compare() {
    let mut output = Vec::new();

    macro_rules! log {
        ($($arg:tt)*) => {{
            let line = format!($($arg)*);
            println!("{}", line);
            let _ = writeln!(output, "{}", line);
        }};
    }

    #[allow(clippy::type_complexity)]
    let scenarios: &[(&str, fn() -> (Vec<u8>, Vec<u8>))] = &[
        ("white", gen_white as fn() -> _),
        ("gradient", gen_gradient as fn() -> _),
    ];

    for &(name, gen_fn) in scenarios {
        log!("\n{}", "=".repeat(60));
        log!("  Scenario: {name}");
        log!("{}", "=".repeat(60));

        let (sdr, hdr) = gen_fn();

        // Encode
        let rust_out = rust_encode(&sdr, &hdr, name);
        let cpp_out = unsafe { cpp_encode(&sdr, &hdr, name) };

        log!("[{name}] Rust output: {} bytes", rust_out.len());
        log!("[{name}] C++  output: {} bytes", cpp_out.len());

        // Save to /tmp
        std::fs::write(format!("/tmp/debug_rust_{name}.jpg"), &rust_out).ok();
        std::fs::write(format!("/tmp/debug_cpp_{name}.jpg"), &cpp_out).ok();

        // --- Metadata comparison ---
        let rust_meta = Decoder::new(&rust_out)
            .probe()
            .expect("Rust probe failed")
            .expect("Rust output has no gain map");
        let cpp_meta_raw = unsafe { cpp_probe_metadata(&cpp_out) };

        log!("\n[{name}] === METADATA (Rust encoder) ===");
        log!("  max_content_boost: {:?}", rust_meta.max_content_boost);
        log!("  min_content_boost: {:?}", rust_meta.min_content_boost);
        log!("  gamma:             {:?}", rust_meta.gamma);
        log!("  offset_sdr:        {:?}", rust_meta.offset_sdr);
        log!("  offset_hdr:        {:?}", rust_meta.offset_hdr);
        log!("  hdr_capacity_min:  {}", rust_meta.hdr_capacity_min);
        log!("  hdr_capacity_max:  {}", rust_meta.hdr_capacity_max);
        log!("  use_base_cg:       {}", rust_meta.use_base_cg);

        log!("\n[{name}] === METADATA (C++ encoder, via C++ probe) ===");
        log!("  max_content_boost: {:?}", cpp_meta_raw.max_content_boost);
        log!("  min_content_boost: {:?}", cpp_meta_raw.min_content_boost);
        log!("  gamma:             {:?}", cpp_meta_raw.gamma);
        log!("  offset_sdr:        {:?}", cpp_meta_raw.offset_sdr);
        log!("  offset_hdr:        {:?}", cpp_meta_raw.offset_hdr);
        log!("  hdr_capacity_min:  {}", cpp_meta_raw.hdr_capacity_min);
        log!("  hdr_capacity_max:  {}", cpp_meta_raw.hdr_capacity_max);
        log!("  use_base_cg:       {}", cpp_meta_raw.use_base_cg);

        // Cross-probe: C++ output parsed by Rust
        let cpp_meta_by_rust = Decoder::new(&cpp_out)
            .probe()
            .expect("Rust probe of C++ output failed")
            .expect("C++ output has no gain map (per Rust)");
        log!("\n[{name}] === METADATA (C++ encoder, via Rust probe) ===");
        log!(
            "  max_content_boost: {:?}",
            cpp_meta_by_rust.max_content_boost
        );
        log!(
            "  min_content_boost: {:?}",
            cpp_meta_by_rust.min_content_boost
        );
        log!("  gamma:             {:?}", cpp_meta_by_rust.gamma);
        log!("  hdr_capacity_min:  {}", cpp_meta_by_rust.hdr_capacity_min);
        log!("  hdr_capacity_max:  {}", cpp_meta_by_rust.hdr_capacity_max);

        // --- Gain map pixel stats ---
        let (r_min, r_max, r_mean) = gainmap_pixel_stats(&rust_out);
        let (c_min, c_max, c_mean) = gainmap_pixel_stats(&cpp_out);
        log!("\n[{name}] === GAIN MAP PIXEL STATS ===");
        log!("  Rust: min={r_min}, max={r_max}, mean={r_mean:.2}");
        log!("  C++:  min={c_min}, max={c_max}, mean={c_mean:.2}");

        // --- Cross-decode: each output decoded by both decoders ---
        log!("\n[{name}] === CROSS-DECODE MAX 10-bit VALUES ===");

        // Rust decoded by C++
        match std::panic::catch_unwind(|| unsafe { cpp_decode_to_1010102(&rust_out) }) {
            Ok(data) => log!("  Rust→C++:  {}/1023", max_1010102_value(&data)),
            Err(_) => log!("  Rust→C++:  FAILED (C++ couldn't decode Rust output)"),
        }

        // C++ decoded by C++
        match std::panic::catch_unwind(|| unsafe { cpp_decode_to_1010102(&cpp_out) }) {
            Ok(data) => log!("  C++→C++:   {}/1023", max_1010102_value(&data)),
            Err(_) => log!("  C++→C++:   FAILED"),
        }

        // Rust decoded by Rust
        match Decoder::new(&rust_out)
            .output_format(PixelFormat::Rgba1010102)
            .output_transfer(ColorTransfer::Hlg)
            .max_display_boost(4.0)
            .decode()
        {
            Ok(img) => log!("  Rust→Rust: {}/1023", max_1010102_value(&img.data)),
            Err(e) => log!("  Rust→Rust: FAILED ({e})"),
        }

        // C++ decoded by Rust
        match Decoder::new(&cpp_out)
            .output_format(PixelFormat::Rgba1010102)
            .output_transfer(ColorTransfer::Hlg)
            .max_display_boost(4.0)
            .decode()
        {
            Ok(img) => log!("  C++→Rust:  {}/1023", max_1010102_value(&img.data)),
            Err(e) => log!("  C++→Rust:  FAILED ({e})"),
        }

        // Metadata diagnosis
        if name == "white" {
            log!("\n[{name}] === DIAGNOSIS ===");
            if rust_meta.max_content_boost[0] <= 1.01 {
                log!(
                    "  WARNING: Rust max_content_boost is ~1.0 ({}) - no HDR boost!",
                    rust_meta.max_content_boost[0]
                );
            }
            if rust_meta.hdr_capacity_max <= 1.01 {
                log!(
                    "  WARNING: Rust hdr_capacity_max is ~1.0 ({}) - display won't boost!",
                    rust_meta.hdr_capacity_max
                );
            }
            // Compare key metadata diffs
            log!(
                "  offset_sdr: Rust={:?} vs C++={:?}",
                rust_meta.offset_sdr,
                cpp_meta_raw.offset_sdr
            );
            log!(
                "  offset_hdr: Rust={:?} vs C++={:?}",
                rust_meta.offset_hdr,
                cpp_meta_raw.offset_hdr
            );
            log!(
                "  use_base_cg: Rust={} vs C++={}",
                rust_meta.use_base_cg,
                cpp_meta_raw.use_base_cg
            );
        }
    }

    // Write output file
    let out_path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .join(".freeman/enc_debug.txt");
    if let Some(parent) = out_path.parent() {
        std::fs::create_dir_all(parent).ok();
    }
    std::fs::write(&out_path, &output).expect("failed to write enc_debug.txt");
    println!("\nResults written to {}", out_path.display());
}
