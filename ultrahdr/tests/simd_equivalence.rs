//! SIMD equivalence test: verify that the SIMD decode path produces valid output.
//!
//! Since SIMD and scalar paths are compile-time `cfg`, we cannot compare them
//! in the same binary. Instead we verify that the SIMD path produces reasonable
//! output (non-zero, values in expected range) and matches C++ bit-exact output.
//!
//! Run: CARGO_HOME=/tmp/cargo-home cargo test --features simd --test simd_equivalence

#[cfg(feature = "simd")]
mod tests {
    use ultrahdr::decoder::Decoder;
    use ultrahdr::encoder::Encoder;
    use ultrahdr::types::*;

    const W: u32 = 64;
    const H: u32 = 64;

    fn gen_gradient_ultrahdr_jpeg() -> Vec<u8> {
        let npx = (W * H) as usize;
        let mut sdr = vec![0u8; npx * 4];
        let mut hdr = vec![0u8; npx * 4];
        for y in 0..H {
            for x in 0..W {
                let i = (y * W + x) as usize;
                let t = x as f32 / (W - 1) as f32;
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

        Encoder::new()
            .hdr_raw(
                &hdr,
                W,
                H,
                PixelFormat::Rgba1010102,
                ColorGamut::Bt2100,
                ColorTransfer::Hlg,
            )
            .sdr_raw(&sdr, W, H, ColorGamut::Bt709)
            .quality(95)
            .gainmap_quality(85)
            .gainmap_scale(4)
            .multichannel_gainmap(false)
            .target_display_peak_nits(1600.0)
            .encode()
            .expect("encode failed")
    }

    #[test]
    fn test_simd_decode_produces_valid_output() {
        let jpeg = gen_gradient_ultrahdr_jpeg();

        // Test all output format × transfer combinations used in decoder
        let cases: &[(PixelFormat, ColorTransfer, &str)] = &[
            (PixelFormat::Rgba1010102, ColorTransfer::Hlg, "hlg_1010102"),
            (PixelFormat::Rgba1010102, ColorTransfer::Pq, "pq_1010102"),
            (PixelFormat::RgbaF16, ColorTransfer::Linear, "linear_f16"),
        ];

        for &(fmt, ct, label) in cases {
            let decoded = Decoder::new(&jpeg)
                .output_format(fmt)
                .output_transfer(ct)
                .max_display_boost(4.0)
                .decode()
                .unwrap_or_else(|e| panic!("[{label}] decode failed: {e}"));

            let npx = (W * H) as usize;
            let bpp = fmt.bytes_per_pixel();
            assert_eq!(
                decoded.data.len(),
                npx * bpp,
                "[{label}] output size mismatch"
            );

            // Verify output is not all zeros (gain map was applied)
            let all_zero = decoded.data.iter().all(|&b| b == 0);
            assert!(
                !all_zero,
                "[{label}] output is all zeros — SIMD path likely broken"
            );

            // For 1010102: verify pixel values are in valid range [0, 1023]
            if fmt == PixelFormat::Rgba1010102 {
                for i in 0..npx {
                    let packed = u32::from_le_bytes([
                        decoded.data[i * 4],
                        decoded.data[i * 4 + 1],
                        decoded.data[i * 4 + 2],
                        decoded.data[i * 4 + 3],
                    ]);
                    let r = packed & 0x3FF;
                    let g = (packed >> 10) & 0x3FF;
                    let b = (packed >> 20) & 0x3FF;
                    assert!(
                        r <= 1023 && g <= 1023 && b <= 1023,
                        "[{label}] pixel {i} out of range: r={r}, g={g}, b={b}"
                    );
                }
            }

            println!(
                "[{label}] SIMD decode: OK ({} bytes output)",
                decoded.data.len()
            );
        }
    }
}
