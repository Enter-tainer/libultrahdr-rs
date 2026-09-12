//! End-to-end coverage for the safe codec API.
//!
//! Every test synthesises its own tiny images, so the suite only depends on one fixture:
//! `data/plain.jpg`, a plain (non-UltraHDR) JPEG used for the negative cases.

use ultrahdr::{
    Codec, ColorAspects, ColorGamut, ColorRange, ColorTransfer, CompressedImage, Decoder, Encoder,
    Error, ImageLabel, Mirror, PixelFormat, RawImage, Rotation, is_uhdr_image,
};

/// Fully specified Display P3 / PQ / full-range aspects, used by the HDR inputs.
const HDR_ASPECTS: ColorAspects =
    ColorAspects::new(ColorGamut::DisplayP3, ColorTransfer::Pq, ColorRange::Full);

/// BT.709 / sRGB / full-range aspects, used by the SDR inputs.
const SDR_ASPECTS: ColorAspects =
    ColorAspects::new(ColorGamut::Bt709, ColorTransfer::Srgb, ColorRange::Full);

/// A plain JPEG without a gain map.
const PLAIN_JPEG: &[u8] = include_bytes!("data/plain.jpg");

// ---------------------------------------------------------------------------------------------
// Synthetic inputs
// ---------------------------------------------------------------------------------------------

/// RGBA1010102 (PQ) HDR pixels with a per-pixel gradient.
fn hdr_1010102_bytes(width: u32, height: u32) -> Vec<u8> {
    let mut buf = vec![0u8; (width * height * 4) as usize];
    for y in 0..height {
        for x in 0..width {
            let i = ((y * width + x) * 4) as usize;
            let r = (x * 23) % 1024;
            let g = (y * 37) % 1024;
            let b = ((x + y) * 11) % 1024;
            let px = r | (g << 10) | (b << 20) | (3u32 << 30);
            buf[i..i + 4].copy_from_slice(&px.to_le_bytes());
        }
    }
    buf
}

/// Fill an RGBA8888 buffer with a per-pixel gradient, honouring `stride` (in pixels).
fn sdr_8888_fill(buf: &mut [u8], width: u32, height: u32, stride: u32) {
    for y in 0..height {
        for x in 0..width {
            let i = ((y * stride + x) * 4) as usize;
            buf[i] = ((x * 16) % 256) as u8;
            buf[i + 1] = ((y * 16) % 256) as u8;
            buf[i + 2] = (((x + y) * 8) % 256) as u8;
            buf[i + 3] = 255;
        }
    }
}

fn sdr_8888_bytes(width: u32, height: u32) -> Vec<u8> {
    let mut buf = vec![0u8; (width * height * 4) as usize];
    sdr_8888_fill(&mut buf, width, height, width);
    buf
}

/// Y, U and V planes of an 8-bit 4:2:0 image (chroma planes at half resolution).
fn sdr_420_planes(width: u32, height: u32) -> (Vec<u8>, Vec<u8>, Vec<u8>) {
    let y = (0..width * height)
        .map(|i| ((i * 5) % 256) as u8)
        .collect::<Vec<u8>>();
    let chroma_len = ((width / 2) * (height / 2)) as usize;
    (y, vec![96u8; chroma_len], vec![160u8; chroma_len])
}

/// Y plane and interleaved UV plane of a 10-bit P010 image.
fn hdr_p010_planes(width: u32, height: u32) -> (Vec<u8>, Vec<u8>) {
    let mut y = vec![0u8; (width * height * 2) as usize];
    for (i, sample) in y.as_chunks_mut::<2>().0.iter_mut().enumerate() {
        let value = ((i as u32 * 7) % 1024) << 6;
        sample.copy_from_slice(&(value as u16).to_le_bytes());
    }
    let mut uv = vec![0u8; (width * (height / 2) * 2) as usize];
    for (i, sample) in uv.as_chunks_mut::<2>().0.iter_mut().enumerate() {
        let value = (if i % 2 == 0 { 512u32 } else { 480u32 }) << 6;
        sample.copy_from_slice(&(value as u16).to_le_bytes());
    }
    (y, uv)
}

// ---------------------------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------------------------

/// An owning RGBA1010102 HDR image of the requested size.
fn hdr_image(width: u32, height: u32) -> RawImage {
    RawImage::from_packed(
        PixelFormat::Rgba1010102,
        width,
        height,
        hdr_1010102_bytes(width, height),
        HDR_ASPECTS,
    )
    .expect("hdr image")
}

/// An owning RGBA8888 SDR image of the requested size.
fn sdr_image(width: u32, height: u32) -> RawImage {
    RawImage::from_packed(
        PixelFormat::Rgba8888,
        width,
        height,
        sdr_8888_bytes(width, height),
        SDR_ASPECTS,
    )
    .expect("sdr image")
}

fn set_stream(dec: &mut Decoder, bytes: &[u8]) {
    dec.set_image(&CompressedImage::new(bytes))
        .expect("set decoder input");
}

fn encode_configured<F>(hdr: &RawImage, sdr: Option<&RawImage>, configure: F) -> Vec<u8>
where
    F: FnOnce(&mut Encoder) -> Result<(), Error>,
{
    let mut enc = Encoder::new().expect("create encoder");
    configure(&mut enc).expect("configure encoder");
    enc.set_raw_image(ImageLabel::Hdr, hdr)
        .expect("set hdr input");
    if let Some(sdr) = sdr {
        enc.set_raw_image(ImageLabel::Sdr, sdr)
            .expect("set sdr input");
    }
    enc.encode().expect("encode");
    enc.encoded_stream()
        .expect("encoded stream")
        .to_owned_image()
        .data
}

fn encode_hdr_only(width: u32, height: u32) -> Vec<u8> {
    encode_configured(&hdr_image(width, height), None, |_| Ok(()))
}

fn decode_dimensions(stream: &[u8]) -> (u32, u32) {
    let mut dec = Decoder::new().expect("create decoder");
    set_stream(&mut dec, stream);
    (
        dec.image_width().expect("base width"),
        dec.image_height().expect("base height"),
    )
}

fn assert_close(actual: f32, expected: f32) {
    let tolerance = 1e-3 * expected.abs().max(1.0);
    assert!(
        (actual - expected).abs() <= tolerance,
        "expected {expected}, got {actual}"
    );
}

// ---------------------------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------------------------

#[test]
fn encode_decode_exposes_metadata_and_compressed_parts() {
    let stream = encode_hdr_only(16, 16);
    assert!(
        is_uhdr_image(&stream),
        "baked stream must probe as UltraHDR"
    );
    assert!(Decoder::is_uhdr_image(&stream));

    let mut dec = Decoder::new().expect("create decoder");
    dec.enable_gpu_acceleration(false).expect("toggle gpu");
    set_stream(&mut dec, &stream);
    // The C context freezes once probed, so configure the output before probing.
    dec.set_output_format(PixelFormat::Rgba1010102)
        .expect("output format");
    dec.set_output_transfer(ColorTransfer::Pq)
        .expect("output transfer");
    dec.probe().expect("probe");

    let frozen = dec.set_output_format(PixelFormat::Rgba8888).unwrap_err();
    assert!(
        matches!(frozen, Error::InvalidOperation(_)),
        "configuration must be rejected after probing, got {frozen:?}"
    );
    assert_eq!(
        frozen.code(),
        ultrahdr::sys::uhdr_codec_err_t::UHDR_CODEC_INVALID_OPERATION
    );

    let info = dec.info().expect("stream info");
    assert_eq!((info.width, info.height), (16, 16));
    let (gainmap_w, gainmap_h) = info.gainmap_size.expect("gain map size");
    assert!(gainmap_w > 0 && gainmap_h > 0);
    assert!(gainmap_w <= 16 && gainmap_h <= 16);
    assert_eq!(dec.image_width().expect("width"), 16);
    assert_eq!(dec.image_height().expect("height"), 16);
    assert_eq!(dec.gainmap_width().expect("gain map width"), gainmap_w);
    assert_eq!(dec.gainmap_height().expect("gain map height"), gainmap_h);

    let meta = info.metadata.expect("gain map metadata");
    assert!(meta.max_content_boost[0] >= 1.0);
    assert!(meta.hdr_capacity_max >= 1.0);
    assert_eq!(dec.gainmap_metadata().expect("metadata query"), Some(meta));

    assert!(
        dec.exif().expect("exif query").is_none(),
        "no exif was attached"
    );

    let base = dec
        .base_image()
        .expect("base image query")
        .expect("base image");
    assert!(!base.is_empty());
    assert_eq!(&base.bytes()[..2], &[0xFF, 0xD8]);

    let gainmap = dec
        .gainmap_image()
        .expect("gain map image query")
        .expect("gain map image");
    assert!(!gainmap.is_empty());
    assert_eq!(&gainmap.bytes()[..2], &[0xFF, 0xD8]);

    if let Some(icc) = dec.icc().expect("icc query") {
        assert!(!icc.is_empty());
        assert!(icc.len() <= icc.capacity());
    }

    {
        let view = dec
            .decode_as(PixelFormat::Rgba1010102, ColorTransfer::Pq)
            .expect("decode");
        assert_eq!((view.width(), view.height()), (16, 16));
        assert_eq!(view.format(), PixelFormat::Rgba1010102);
        assert_eq!(view.row(0).expect("row 0").len(), 16 * 4);
        assert!(view.row(15).is_ok());
        assert!(view.row(16).is_err(), "out of range row must be rejected");

        let owned = view.to_owned_image();
        assert_eq!(owned.data.len(), 16 * 16 * 4);
        assert_eq!(
            view.row(3).expect("row 3"),
            &owned.data[3 * 16 * 4..4 * 16 * 4]
        );
    }

    let decoded_gainmap = dec
        .decoded_gainmap()
        .expect("decoded gain map query")
        .expect("decoded gain map");
    assert_eq!(
        (decoded_gainmap.width(), decoded_gainmap.height()),
        (gainmap_w, gainmap_h)
    );
    let bpp = decoded_gainmap
        .format()
        .bytes_per_pixel()
        .expect("packed gain map format");
    assert_eq!(
        decoded_gainmap.row(0).expect("gain map row").len(),
        gainmap_w as usize * bpp
    );
}

#[test]
fn plain_jpeg_is_not_reported_as_uhdr() {
    assert!(!is_uhdr_image(PLAIN_JPEG));
    assert!(!is_uhdr_image(b"not a jpeg at all"));
    assert!(!is_uhdr_image(&[]));

    let mut dec = Decoder::new().expect("create decoder");
    set_stream(&mut dec, PLAIN_JPEG);
    let err = dec
        .probe()
        .expect_err("plain JPEG has no gain map metadata");
    assert!(
        matches!(err, Error::InvalidParameter(_)),
        "unexpected error: {err:?}"
    );
    assert!(dec.image_width().is_err());
    assert!(dec.gainmap_image().is_err());
    assert!(dec.gainmap_metadata().is_err());
}

#[test]
fn exif_data_round_trips_through_encode_and_decode() {
    // Minimal but structurally valid EXIF: "Exif\0\0" + empty big-endian TIFF directory.
    const EXIF: &[u8] = b"Exif\0\0MM\0*\0\0\0\x08\0\0\0\0\0\0";

    let mut enc = Encoder::new().expect("create encoder");
    assert!(
        enc.set_exif_data(Vec::new()).is_err(),
        "empty exif must be rejected"
    );

    let stream = encode_configured(&hdr_image(16, 16), None, |enc| enc.set_exif_data(EXIF));
    assert!(is_uhdr_image(&stream));

    let mut dec = Decoder::new().expect("create decoder");
    set_stream(&mut dec, &stream);
    let exif = dec
        .exif()
        .expect("exif query")
        .expect("exif must survive the round trip");
    assert_eq!(exif.bytes(), EXIF);
    assert_eq!(exif.to_vec(), EXIF);
}

#[test]
fn precomputed_gainmap_can_be_reencoded() {
    let stream = encode_hdr_only(16, 16);

    let (base, gainmap, meta) = {
        let mut dec = Decoder::new().expect("create decoder");
        set_stream(&mut dec, &stream);
        let meta = dec
            .gainmap_metadata()
            .expect("metadata query")
            .expect("gain map metadata");
        let base = dec
            .base_image()
            .expect("base image query")
            .expect("base image")
            .to_vec();
        let gainmap = dec
            .gainmap_image()
            .expect("gain map image query")
            .expect("gain map image")
            .to_vec();
        (base, gainmap, meta)
    };

    let mut enc = Encoder::new().expect("create encoder");
    enc.set_compressed_image(ImageLabel::Base, &CompressedImage::new(base))
        .expect("set base image");
    enc.set_gainmap_image(&CompressedImage::new(gainmap), &meta)
        .expect("set gain map image");
    enc.encode()
        .expect("encode without recomputing the gain map");
    let out = enc
        .encoded_stream()
        .expect("encoded stream")
        .to_owned_image()
        .data;

    assert!(is_uhdr_image(&out));
    let mut dec = Decoder::new().expect("create decoder");
    set_stream(&mut dec, &out);
    assert_eq!(decode_dimensions(&out), (16, 16));
    let roundtripped = dec
        .gainmap_metadata()
        .expect("metadata query")
        .expect("gain map metadata");
    assert_close(roundtripped.max_content_boost[0], meta.max_content_boost[0]);
    assert_close(roundtripped.min_content_boost[0], meta.min_content_boost[0]);
    assert_close(roundtripped.gamma[0], meta.gamma[0]);
    assert_close(roundtripped.hdr_capacity_max, meta.hdr_capacity_max);
    assert_eq!(roundtripped.use_base_cg, meta.use_base_cg);
}

#[test]
fn encoder_effects_reshape_the_output() {
    let resized = encode_configured(&hdr_image(16, 16), None, |enc| enc.resize(8, 8));
    assert_eq!(decode_dimensions(&resized), (8, 8));

    // crop takes exclusive right/bottom coordinates.
    let cropped = encode_configured(&hdr_image(16, 16), None, |enc| enc.crop(2, 10, 3, 11));
    assert_eq!(decode_dimensions(&cropped), (8, 8));

    // A 90 degree rotation swaps the stored dimensions.
    let rotated = encode_configured(&hdr_image(16, 8), None, |enc| enc.rotate(Rotation::Deg90));
    assert_eq!(decode_dimensions(&rotated), (8, 16));
}

#[test]
fn decoder_effects_transform_the_decoded_pixels() {
    let stream = encode_hdr_only(8, 8);

    let reference = {
        let mut dec = Decoder::new().expect("create decoder");
        set_stream(&mut dec, &stream);
        dec.decode_as(PixelFormat::Rgba1010102, ColorTransfer::Pq)
            .expect("decode")
            .to_owned_image()
    };
    assert_eq!((reference.width, reference.height), (8, 8));
    let row = reference.width as usize * 4;

    // Vertical mirror reverses the row order.
    let mirrored = {
        let mut dec = Decoder::new().expect("create decoder");
        set_stream(&mut dec, &stream);
        dec.mirror(Mirror::Vertical).expect("add mirror effect");
        dec.decode_as(PixelFormat::Rgba1010102, ColorTransfer::Pq)
            .expect("decode")
            .to_owned_image()
    };
    assert_eq!((mirrored.width, mirrored.height), (8, 8));
    for y in 0..8usize {
        let src = (7 - y) * row;
        assert_eq!(
            &mirrored.data[y * row..(y + 1) * row],
            &reference.data[src..src + row],
            "mirrored row {y}"
        );
    }

    // Crop keeps the requested window.
    let cropped = {
        let mut dec = Decoder::new().expect("create decoder");
        set_stream(&mut dec, &stream);
        dec.crop(2, 6, 1, 5).expect("add crop effect");
        dec.decode_as(PixelFormat::Rgba1010102, ColorTransfer::Pq)
            .expect("decode")
            .to_owned_image()
    };
    assert_eq!((cropped.width, cropped.height), (4, 4));
    let cropped_row = cropped.width as usize * 4;
    for y in 0..4usize {
        let src = (1 + y) * row + 2 * 4;
        assert_eq!(
            &cropped.data[y * cropped_row..(y + 1) * cropped_row],
            &reference.data[src..src + cropped_row],
            "cropped row {y}"
        );
    }

    // A 90 degree rotation swaps the stored dimensions.
    let wide = encode_hdr_only(16, 8);
    let rotated = {
        let mut dec = Decoder::new().expect("create decoder");
        set_stream(&mut dec, &wide);
        dec.rotate(Rotation::Deg90).expect("add rotate effect");
        dec.decode_as(PixelFormat::Rgba1010102, ColorTransfer::Pq)
            .expect("decode")
            .to_owned_image()
    };
    assert_eq!((rotated.width, rotated.height), (8, 16));
}

#[test]
fn decoder_reset_allows_reuse() {
    let first = encode_hdr_only(16, 16);
    let second = encode_hdr_only(16, 16);

    let mut dec = Decoder::new().expect("create decoder");
    set_stream(&mut dec, &first);
    assert_eq!(decode_dimensions(&first), (16, 16));

    dec.reset();
    assert!(
        dec.image_width().is_err(),
        "reset must clear the previously probed image"
    );
    assert!(
        matches!(dec.decode().unwrap_err(), Error::InvalidOperation(_)),
        "a reset decoder has no image to decode"
    );

    set_stream(&mut dec, &second);
    let view = dec
        .decode_as(PixelFormat::Rgba1010102, ColorTransfer::Pq)
        .expect("decode after reset");
    assert_eq!((view.width(), view.height()), (16, 16));
}

#[test]
fn decoded_pixels_can_be_reencoded() {
    let stream = encode_hdr_only(16, 16);

    let mut dec = Decoder::new().expect("create decoder");
    set_stream(&mut dec, &stream);
    let mut decoded = dec
        .decode_as(PixelFormat::Rgba1010102, ColorTransfer::Pq)
        .expect("decode")
        .to_owned_image();
    // The stream does not signal every aspect; the encoder needs them for raw input.
    decoded.aspects = HDR_ASPECTS;

    // The owned pixels move into a `RawImage` without copying, then feed a second encode.
    let raw = decoded.into_raw_image().expect("re-wrap decoded pixels");
    assert_eq!(raw.format(), PixelFormat::Rgba1010102);
    assert_eq!(raw.data().expect("packed").len(), 16 * 16 * 4);

    let reencoded = encode_configured(&raw, None, |enc| enc.set_output_format(Codec::Jpeg));
    assert!(is_uhdr_image(&reencoded));
    assert_eq!(decode_dimensions(&reencoded), (16, 16));
}

#[test]
fn padded_input_stride_matches_tight_input() {
    const WIDTH: u32 = 16;
    const HEIGHT: u32 = 16;
    const STRIDE: u32 = WIDTH + 4;

    let tight_stream = encode_configured(
        &hdr_image(WIDTH, HEIGHT),
        Some(&sdr_image(WIDTH, HEIGHT)),
        |_| Ok(()),
    );

    let padded_stream = {
        let mut padded = vec![0u8; (STRIDE * HEIGHT * 4) as usize];
        sdr_8888_fill(&mut padded, WIDTH, HEIGHT, STRIDE);
        let sdr = RawImage::from_planes(
            PixelFormat::Rgba8888,
            WIDTH,
            HEIGHT,
            [Some(padded), None, None],
            [STRIDE, 0, 0],
            SDR_ASPECTS,
        )
        .expect("padded sdr image");
        encode_configured(&hdr_image(WIDTH, HEIGHT), Some(&sdr), |_| Ok(()))
    };

    assert_eq!(
        tight_stream, padded_stream,
        "the encoder must honour the SDR row stride"
    );
}

#[test]
fn planar_ycbcr_inputs_are_accepted() {
    const WIDTH: u32 = 16;
    const HEIGHT: u32 = 16;

    // 8-bit planar 4:2:0 SDR rendition next to an RGBA1010102 HDR rendition.
    let planar_sdr = {
        let (y, u, v) = sdr_420_planes(WIDTH, HEIGHT);
        let sdr = RawImage::yuv420(WIDTH, HEIGHT, y, u, v, SDR_ASPECTS).expect("420 image");
        assert_eq!(sdr.plane_count(), 3);
        assert_eq!(sdr.stride(ultrahdr::Plane::U), Some(WIDTH / 2));
        encode_configured(&hdr_image(WIDTH, HEIGHT), Some(&sdr), |_| Ok(()))
    };
    assert!(is_uhdr_image(&planar_sdr));
    assert_eq!(decode_dimensions(&planar_sdr), (WIDTH, HEIGHT));

    // P010 HDR rendition next to an RGBA8888 SDR rendition.
    let p010_hdr = {
        let (y, uv) = hdr_p010_planes(WIDTH, HEIGHT);
        let hdr = RawImage::p010(
            WIDTH,
            HEIGHT,
            y,
            uv,
            ColorAspects::new(ColorGamut::Bt2100, ColorTransfer::Pq, ColorRange::Full),
        )
        .expect("p010 image");
        encode_configured(&hdr, Some(&sdr_image(WIDTH, HEIGHT)), |_| Ok(()))
    };
    assert!(is_uhdr_image(&p010_hdr));
    assert_eq!(decode_dimensions(&p010_hdr), (WIDTH, HEIGHT));
}

#[test]
fn min_max_content_boost_is_validated_and_applied() {
    let mut enc = Encoder::new().expect("create encoder");
    let reversed = enc.set_min_max_content_boost(4.0, 1.0).unwrap_err();
    assert!(
        matches!(reversed, Error::InvalidParameter(_)),
        "max below min must be rejected: {reversed:?}"
    );
    let negative = enc.set_min_max_content_boost(-1.0, 2.0).unwrap_err();
    assert!(
        matches!(negative, Error::InvalidParameter(_)),
        "non-positive min must be rejected: {negative:?}"
    );
    enc.set_min_max_content_boost(1.0, 4.0)
        .expect("valid recommendation");

    let stream = encode_configured(&hdr_image(16, 16), None, |enc| {
        enc.enable_gpu_acceleration(false)?;
        enc.set_min_max_content_boost(1.0, 4.0)
    });

    let mut dec = Decoder::new().expect("create decoder");
    set_stream(&mut dec, &stream);
    let meta = dec
        .gainmap_metadata()
        .expect("metadata query")
        .expect("gain map metadata");
    assert!(meta.max_content_boost[0] >= 1.0);
    assert!(meta.hdr_capacity_max >= 1.0);
}

#[test]
fn quality_out_of_range_is_rejected() {
    let mut enc = Encoder::new().expect("create encoder");
    let err = enc.set_quality(ImageLabel::Hdr, 101).unwrap_err();
    assert!(matches!(err, Error::InvalidParameter(_)), "{err:?}");
    enc.set_quality(ImageLabel::Hdr, 100).expect("quality 100");
}

// ---------------------------------------------------------------------------------------------
