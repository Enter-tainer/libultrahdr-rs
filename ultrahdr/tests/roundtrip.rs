use ultrahdr::decoder::Decoder;
use ultrahdr::encoder::Encoder;
use ultrahdr::types::*;

#[test]
fn encode_then_decode_rgba8888_sdr() {
    let width: usize = 64;
    let height: usize = 64;

    // Create synthetic HDR pixels (RGBA8888, will be treated as sRGB)
    let hdr_pixels: Vec<u8> = (0..width * height * 4)
        .map(|i| ((i * 7 + 13) % 256) as u8)
        .collect();

    // Create synthetic SDR pixels (RGB, 3 bytes per pixel) for JPEG encoding
    let sdr_rgb: Vec<u8> = (0..width * height * 3)
        .map(|i| ((i * 3 + 7) % 256) as u8)
        .collect();

    // Encode SDR to JPEG (API only supports RGB, not RGBA)
    let sdr_jpeg =
        ultrahdr::jpeg::encode::encode_rgb_to_jpeg(&sdr_rgb, width as u32, height as u32, 95)
            .expect("SDR JPEG encode failed");

    // Encode as UltraHDR JPEG
    let ultrahdr_jpeg = Encoder::new()
        .hdr_raw(
            &hdr_pixels,
            width as u32,
            height as u32,
            PixelFormat::Rgba8888,
            ColorGamut::Bt709,
            ColorTransfer::Srgb,
        )
        .sdr_compressed(&sdr_jpeg, ColorGamut::Bt709)
        .quality(95)
        .encode()
        .expect("UltraHDR encode failed");

    // Verify it's a valid JPEG
    assert_eq!(&ultrahdr_jpeg[..2], &[0xFF, 0xD8], "should start with SOI");
    assert_eq!(
        &ultrahdr_jpeg[ultrahdr_jpeg.len() - 2..],
        &[0xFF, 0xD9],
        "should end with EOI"
    );

    // Probe for gain map metadata
    let metadata = Decoder::new(&ultrahdr_jpeg)
        .probe()
        .expect("probe failed")
        .expect("should contain gain map metadata");
    assert!(
        metadata.max_content_boost[0] > 0.0,
        "max_content_boost should be positive"
    );

    // Full decode back to HDR
    let decoded = Decoder::new(&ultrahdr_jpeg)
        .output_format(PixelFormat::Rgba8888)
        .output_transfer(ColorTransfer::Srgb)
        .max_display_boost(4.0)
        .decode()
        .expect("UltraHDR decode failed");

    assert_eq!(decoded.width, width as u32);
    assert_eq!(decoded.height, height as u32);
    assert_eq!(decoded.format, PixelFormat::Rgba8888);
    assert_eq!(decoded.transfer, ColorTransfer::Srgb);
    assert_eq!(decoded.data.len(), width * height * 4);

    // Decoded pixels should all be valid (non-zero for our synthetic data)
    let nonzero_count = decoded.data.iter().filter(|&&b| b != 0).count();
    assert!(
        nonzero_count > 0,
        "decoded image should have non-zero pixels"
    );
}
