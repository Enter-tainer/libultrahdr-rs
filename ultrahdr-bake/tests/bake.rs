//! End-to-end coverage for the `ultrahdr-bake` CLI.
//!
//! The HDR fixture is synthesised with the library; the SDR base is the `plain.jpg` fixture of the
//! `ultrahdr` crate copied into the scratch directory. The scale factor used by the AVIF test makes
//! the gain map height odd, which is a shape upstream cannot write into a container (`SEE
//! Encoder::set_multi_channel_gainmap`), so the CLI has to fall back to a single-channel gain map.

use std::{
    fs,
    path::{Path, PathBuf},
    process::{Command, Output},
};

use ultrahdr::{
    Codec, ColorAspects, ColorGamut, ColorRange, ColorTransfer, Encoder, ImageLabel, PixelFormat,
    RawImage,
};

/// Matches `ultrahdr/tests/data/plain.jpg`, which is used as the SDR base.
const WIDTH: u32 = 320;
const HEIGHT: u32 = 240;

const HDR_ASPECTS: ColorAspects =
    ColorAspects::new(ColorGamut::DisplayP3, ColorTransfer::Pq, ColorRange::Full);

/// A gradient of RGBA1010102 (PQ) HDR pixels.
fn hdr_image() -> RawImage {
    let mut buf = vec![0u8; (WIDTH * HEIGHT * 4) as usize];
    for y in 0..HEIGHT {
        for x in 0..WIDTH {
            let i = ((y * WIDTH + x) * 4) as usize;
            let pixel = (x * 29 % 1024) | ((y * 31 % 1024) << 10) | (((x + y) * 17 % 1024) << 20);
            buf[i..i + 4].copy_from_slice(&(pixel | (3 << 30)).to_le_bytes());
        }
    }
    RawImage::from_packed(PixelFormat::Rgba1010102, WIDTH, HEIGHT, buf, HDR_ASPECTS)
        .expect("hdr image")
}

/// Encode one raw intent to a JPEG and return the bytes.
fn jpeg_of(label: ImageLabel, image: &RawImage) -> Vec<u8> {
    let mut enc = Encoder::new().expect("create encoder");
    enc.set_raw_image(label, image).expect("set raw image");
    enc.set_output_format(Codec::Jpeg).expect("jpeg output");
    enc.encode().expect("encode jpeg");
    enc.encoded_stream()
        .expect("encoded stream")
        .to_owned_image()
        .data
}

/// Write a gain map JPEG (HDR intent) and a plain JPEG (SDR base) into `dir`.
fn write_fixtures(dir: &Path) -> (PathBuf, PathBuf) {
    fs::create_dir_all(dir).expect("fixture directory");
    let hdr = dir.join("hdr.jpg");
    fs::write(&hdr, jpeg_of(ImageLabel::Hdr, &hdr_image())).expect("write hdr fixture");
    // Copied so the CLI's default output name stays inside the scratch directory.
    let sdr = dir.join("plain.jpg");
    fs::copy(
        concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../ultrahdr/tests/data/plain.jpg"
        ),
        &sdr,
    )
    .expect("copy the sdr fixture");
    (hdr, sdr)
}

fn bake(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_ultrahdr-bake"))
        .args(args)
        .output()
        .expect("run the CLI")
}

fn scratch(name: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join(name);
    let _ = fs::remove_dir_all(&dir);
    dir
}

#[test]
fn bakes_an_ultrahdr_jpeg_by_default() {
    let dir = scratch("jpeg");
    let (hdr, sdr) = write_fixtures(&dir);

    let out = dir.join("out.jpg");
    let result = bake(&[
        "--hdr",
        hdr.to_str().expect("hdr path"),
        "--sdr",
        sdr.to_str().expect("sdr path"),
        "--out",
        out.to_str().expect("out path"),
    ]);
    assert!(
        result.status.success(),
        "CLI failed: {}",
        String::from_utf8_lossy(&result.stderr)
    );

    let bytes = fs::read(&out).expect("output file");
    assert_eq!(&bytes[..2], &[0xFF, 0xD8], "not a JPEG");
    assert!(
        ultrahdr::is_uhdr_image(&bytes),
        "output carries no gain map"
    );
}

#[test]
fn default_output_name_follows_the_format() {
    let dir = scratch("naming");
    let (hdr, sdr) = write_fixtures(&dir);

    let result = bake(&[
        "--hdr",
        hdr.to_str().expect("hdr path"),
        "--sdr",
        sdr.to_str().expect("sdr path"),
    ]);
    assert!(result.status.success(), "CLI failed");
    assert!(
        dir.join("plain-merge.jpg").is_file(),
        "expected plain-merge.jpg in {}",
        dir.display()
    );
}
