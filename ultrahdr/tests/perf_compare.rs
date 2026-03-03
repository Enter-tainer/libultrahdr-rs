//! Performance comparison: Rust encoder/decoder vs C++ (ultrahdr-sys) FFI.
//!
//! Run: CARGO_HOME=/tmp/cargo-home cargo test -p ultrahdr --test perf_compare -- --nocapture --ignored

use std::time::Instant;

use ultrahdr::decoder::Decoder;
use ultrahdr::encoder::Encoder;
use ultrahdr::types::*;
use ultrahdr_sys::*;

const W: u32 = 512;
const H: u32 = 512;
const ENCODE_ITERS: usize = 10;
const DECODE_ITERS: usize = 10;

// ---------- synthetic image generators ----------

/// Create RGBA8888 SDR + RGBA1010102 HDR pair: horizontal gradient.
fn gen_gradient(w: u32, h: u32) -> (Vec<u8>, Vec<u8>) {
    let npx = (w * h) as usize;
    let mut sdr = vec![0u8; npx * 4];
    let mut hdr = vec![0u8; npx * 4];
    for y in 0..h {
        for x in 0..w {
            let i = (y * w + x) as usize;
            let t = x as f32 / (w - 1) as f32;
            let sdr_val = (t * 255.0 + 0.5) as u8;
            sdr[i * 4] = sdr_val;
            sdr[i * 4 + 1] = sdr_val;
            sdr[i * 4 + 2] = sdr_val;
            sdr[i * 4 + 3] = 255;
            let hdr_val = (t * 1023.0 + 0.5) as u32;
            let packed: u32 = hdr_val | (hdr_val << 10) | (hdr_val << 20) | (3 << 30);
            hdr[i * 4..i * 4 + 4].copy_from_slice(&packed.to_le_bytes());
        }
    }
    (sdr, hdr)
}

// ---------- Rust encoder ----------

fn rust_encode(sdr: &[u8], hdr: &[u8], w: u32, h: u32) -> Vec<u8> {
    Encoder::new()
        .hdr_raw(
            hdr,
            w,
            h,
            PixelFormat::Rgba1010102,
            ColorGamut::Bt2100,
            ColorTransfer::Hlg,
        )
        .sdr_raw(sdr, w, h, ColorGamut::Bt709)
        .quality(95)
        .gainmap_quality(85)
        .gainmap_scale(4)
        .multichannel_gainmap(false)
        .target_display_peak_nits(1600.0)
        .encode()
        .expect("Rust encode failed")
}

// ---------- C++ encoder via FFI ----------

unsafe fn cpp_encode(sdr: &[u8], hdr: &[u8], w: u32, h: u32) -> Vec<u8> {
    unsafe {
        let enc = uhdr_create_encoder();
        assert!(!enc.is_null());

        let mut hdr_img = uhdr_raw_image {
            fmt: uhdr_img_fmt::UHDR_IMG_FMT_32bppRGBA1010102,
            cg: uhdr_color_gamut::UHDR_CG_BT_2100,
            ct: uhdr_color_transfer::UHDR_CT_HLG,
            range: uhdr_color_range::UHDR_CR_FULL_RANGE,
            w,
            h,
            planes: [
                hdr.as_ptr() as *mut _,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
            ],
            stride: [w, 0, 0],
        };
        let err = uhdr_enc_set_raw_image(enc, &mut hdr_img, uhdr_img_label::UHDR_HDR_IMG);
        assert_eq!(err.error_code, uhdr_codec_err::UHDR_CODEC_OK);

        let mut sdr_img = uhdr_raw_image {
            fmt: uhdr_img_fmt::UHDR_IMG_FMT_32bppRGBA8888,
            cg: uhdr_color_gamut::UHDR_CG_BT_709,
            ct: uhdr_color_transfer::UHDR_CT_SRGB,
            range: uhdr_color_range::UHDR_CR_FULL_RANGE,
            w,
            h,
            planes: [
                sdr.as_ptr() as *mut _,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
            ],
            stride: [w, 0, 0],
        };
        let err = uhdr_enc_set_raw_image(enc, &mut sdr_img, uhdr_img_label::UHDR_SDR_IMG);
        assert_eq!(err.error_code, uhdr_codec_err::UHDR_CODEC_OK);

        let err = uhdr_enc_set_quality(enc, 95, uhdr_img_label::UHDR_BASE_IMG);
        assert_eq!(err.error_code, uhdr_codec_err::UHDR_CODEC_OK);
        let err = uhdr_enc_set_quality(enc, 85, uhdr_img_label::UHDR_GAIN_MAP_IMG);
        assert_eq!(err.error_code, uhdr_codec_err::UHDR_CODEC_OK);

        let err = uhdr_enc_set_using_multi_channel_gainmap(enc, 0);
        assert_eq!(err.error_code, uhdr_codec_err::UHDR_CODEC_OK);
        let err = uhdr_enc_set_gainmap_scale_factor(enc, 4);
        assert_eq!(err.error_code, uhdr_codec_err::UHDR_CODEC_OK);
        let err = uhdr_enc_set_target_display_peak_brightness(enc, 1600.0);
        assert_eq!(err.error_code, uhdr_codec_err::UHDR_CODEC_OK);

        let err = uhdr_encode(enc);
        assert_eq!(
            err.error_code,
            uhdr_codec_err::UHDR_CODEC_OK,
            "C++ encode failed"
        );

        let stream = uhdr_get_encoded_stream(enc);
        assert!(!stream.is_null());
        let data = std::slice::from_raw_parts((*stream).data as *const u8, (*stream).data_sz);
        let result = data.to_vec();

        uhdr_release_encoder(enc);
        result
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
            "C++ decode failed"
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

fn rust_decode(jpeg: &[u8]) -> Vec<u8> {
    Decoder::new(jpeg)
        .output_format(PixelFormat::Rgba1010102)
        .output_transfer(ColorTransfer::Hlg)
        .max_display_boost(4.0)
        .decode()
        .expect("Rust decode failed")
        .data
}

// ---------- benchmark helpers ----------

fn bench<F: FnMut()>(label: &str, iters: usize, mut f: F) -> f64 {
    // Warmup
    f();

    let start = Instant::now();
    for _ in 0..iters {
        f();
    }
    let total_ms = start.elapsed().as_secs_f64() * 1000.0;
    let avg_ms = total_ms / iters as f64;
    println!("  {label}: {avg_ms:.1} ms (avg of {iters} runs)");
    avg_ms
}

// ---------- main test ----------

#[test]
#[ignore]
fn perf_compare_rust_vs_cpp() {
    println!("\n=== Rust vs C++ Performance ({W}x{H}) ===\n");

    let (sdr, hdr) = gen_gradient(W, H);

    // --- Encode benchmark ---
    println!("Encode:");

    let rust_encode_ms = bench("Rust", ENCODE_ITERS, || {
        let _ = rust_encode(&sdr, &hdr, W, H);
    });

    let cpp_encode_ms = bench("C++ ", ENCODE_ITERS, || {
        let _ = unsafe { cpp_encode(&sdr, &hdr, W, H) };
    });

    let encode_ratio = rust_encode_ms / cpp_encode_ms;
    println!("  Ratio: Rust/C++ = {encode_ratio:.2}x\n");

    // Produce JPEG data for decode benchmarks
    let rust_jpeg = rust_encode(&sdr, &hdr, W, H);
    let cpp_jpeg = unsafe { cpp_encode(&sdr, &hdr, W, H) };

    // --- Decode benchmark (using C++ encoded JPEG as input for both) ---
    println!(
        "Decode (input: C++ encoded JPEG, {} bytes):",
        cpp_jpeg.len()
    );

    let rust_decode_ms = bench("Rust", DECODE_ITERS, || {
        let _ = rust_decode(&cpp_jpeg);
    });

    let cpp_decode_ms = bench("C++ ", DECODE_ITERS, || {
        let _ = unsafe { cpp_decode_to_1010102(&cpp_jpeg) };
    });

    let decode_ratio = rust_decode_ms / cpp_decode_ms;
    println!("  Ratio: Rust/C++ = {decode_ratio:.2}x\n");

    // --- Also test decode with Rust-encoded JPEG ---
    println!(
        "Decode (input: Rust encoded JPEG, {} bytes):",
        rust_jpeg.len()
    );

    let rust_decode_ms2 = bench("Rust", DECODE_ITERS, || {
        let _ = rust_decode(&rust_jpeg);
    });

    let cpp_decode_ms2 = bench("C++ ", DECODE_ITERS, || {
        let _ = unsafe { cpp_decode_to_1010102(&rust_jpeg) };
    });

    let decode_ratio2 = rust_decode_ms2 / cpp_decode_ms2;
    println!("  Ratio: Rust/C++ = {decode_ratio2:.2}x\n");

    // --- Summary ---
    println!("=== Summary ===");
    println!(
        "  Encode: Rust {rust_encode_ms:.1}ms vs C++ {cpp_encode_ms:.1}ms (ratio {encode_ratio:.2}x)"
    );
    println!(
        "  Decode (C++ JPEG): Rust {rust_decode_ms:.1}ms vs C++ {cpp_decode_ms:.1}ms (ratio {decode_ratio:.2}x)"
    );
    println!(
        "  Decode (Rust JPEG): Rust {rust_decode_ms2:.1}ms vs C++ {cpp_decode_ms2:.1}ms (ratio {decode_ratio2:.2}x)"
    );
}
