use criterion::{Criterion, black_box, criterion_group, criterion_main};
use ultrahdr::decoder::Decoder;
use ultrahdr::encoder::Encoder;
use ultrahdr::types::*;
use ultrahdr_sys::*;

const W: u32 = 512;
const H: u32 = 512;

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

fn rust_encode(sdr: &[u8], hdr: &[u8]) -> Vec<u8> {
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
        .expect("Rust encode failed")
}

unsafe fn cpp_encode(sdr: &[u8], hdr: &[u8]) -> Vec<u8> {
    unsafe {
        let enc = uhdr_create_encoder();
        assert!(!enc.is_null());

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
        assert_eq!(err.error_code, uhdr_codec_err::UHDR_CODEC_OK);

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

unsafe fn cpp_decode(jpeg: &[u8], fmt: uhdr_img_fmt, ct: uhdr_color_transfer) -> Vec<u8> {
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

        let err = uhdr_dec_set_out_img_format(dec, fmt);
        assert_eq!(err.error_code, uhdr_codec_err::UHDR_CODEC_OK);
        let err = uhdr_dec_set_out_color_transfer(dec, ct);
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
        let bytes_per_pixel = match fmt {
            uhdr_img_fmt::UHDR_IMG_FMT_64bppRGBAHalfFloat => 8,
            _ => 4,
        };
        let nbytes = (raw.w * raw.h) as usize * bytes_per_pixel;
        let data = std::slice::from_raw_parts(raw.planes[0] as *const u8, nbytes);
        let result = data.to_vec();

        uhdr_release_decoder(dec);
        result
    }
}

fn bench_encode(c: &mut Criterion) {
    let (sdr, hdr) = gen_gradient(W, H);
    let mut g = c.benchmark_group("encode_512x512");

    g.bench_function("rust", |b| {
        b.iter(|| rust_encode(black_box(&sdr), black_box(&hdr)))
    });

    g.bench_function("cpp", |b| {
        b.iter(|| unsafe { cpp_encode(black_box(&sdr), black_box(&hdr)) })
    });

    g.finish();
}

fn bench_decode(c: &mut Criterion) {
    let (sdr, hdr) = gen_gradient(W, H);
    let jpeg = rust_encode(&sdr, &hdr);

    let mut g = c.benchmark_group("decode_512x512");

    g.bench_function("rust_hlg_1010102", |b| {
        b.iter(|| {
            Decoder::new(black_box(&jpeg))
                .output_format(PixelFormat::Rgba1010102)
                .output_transfer(ColorTransfer::Hlg)
                .max_display_boost(4.0)
                .decode()
                .unwrap()
        })
    });

    g.bench_function("rust_pq_1010102", |b| {
        b.iter(|| {
            Decoder::new(black_box(&jpeg))
                .output_format(PixelFormat::Rgba1010102)
                .output_transfer(ColorTransfer::Pq)
                .max_display_boost(4.0)
                .decode()
                .unwrap()
        })
    });

    g.bench_function("rust_linear_f16", |b| {
        b.iter(|| {
            Decoder::new(black_box(&jpeg))
                .output_format(PixelFormat::RgbaF16)
                .output_transfer(ColorTransfer::Linear)
                .max_display_boost(4.0)
                .decode()
                .unwrap()
        })
    });

    g.bench_function("cpp_hlg_1010102", |b| {
        b.iter(|| unsafe {
            cpp_decode(
                black_box(&jpeg),
                uhdr_img_fmt::UHDR_IMG_FMT_32bppRGBA1010102,
                uhdr_color_transfer::UHDR_CT_HLG,
            )
        })
    });

    g.finish();
}

criterion_group!(benches, bench_encode, bench_decode);
criterion_main!(benches);
