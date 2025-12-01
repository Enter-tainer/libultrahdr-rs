use anyhow::{bail, Context, Result};
use std::env;
use std::fs;
use ultrahdr::{sys, CompressedImage, Decoder, Encoder, GainMapMetadata, ImgLabel};

#[derive(Debug)]
struct Args {
    hdr_uhdr_path: String,
    sdr_path: String,
    out_path: String,
    base_quality: i32,
    gainmap_quality: i32,
    gainmap_scale: i32,
    multichannel_gainmap: bool,
    target_peak_nits: Option<f32>,
}

impl Args {
    fn parse() -> Result<Self> {
        let mut hdr_uhdr_path = String::new();
        let mut sdr_path = String::new();
        let mut out_path = String::from("ultrahdr_oneclick_out.jpg");
        let mut base_quality = 95;
        let mut gainmap_quality = 95;
        let mut gainmap_scale = 1;
        let mut multichannel_gainmap = false;
        let mut target_peak_nits = None;

        let args: Vec<String> = env::args().collect();
        let mut i = 1;
        while i < args.len() {
            match args[i].as_str() {
                "--hdr" => {
                    i += 1;
                    hdr_uhdr_path = args.get(i).cloned().unwrap_or_default();
                }
                "--sdr" => {
                    i += 1;
                    sdr_path = args.get(i).cloned().unwrap_or_default();
                }
                "--out" => {
                    i += 1;
                    out_path = args.get(i).cloned().unwrap_or_default();
                }
                "--base-q" => {
                    i += 1;
                    base_quality = args
                        .get(i)
                        .and_then(|s| s.parse().ok())
                        .unwrap_or(base_quality);
                }
                "--gm-q" => {
                    i += 1;
                    gainmap_quality = args
                        .get(i)
                        .and_then(|s| s.parse().ok())
                        .unwrap_or(gainmap_quality);
                }
                "--scale" => {
                    i += 1;
                    gainmap_scale = args
                        .get(i)
                        .and_then(|s| s.parse().ok())
                        .unwrap_or(gainmap_scale);
                }
                "--mc" => {
                    multichannel_gainmap = true;
                }
                "--target-peak" => {
                    i += 1;
                    target_peak_nits = args.get(i).and_then(|s| s.parse().ok());
                }
                _ => {
                    bail!("Unknown arg: {}", args[i]);
                }
            }
            i += 1;
        }

        if hdr_uhdr_path.is_empty() || sdr_path.is_empty() {
            bail!("Usage: --hdr <uhdr_jpeg> --sdr <sdr_jpeg> [--out <file>] [--base-q 95] [--gm-q 95] [--scale 1] [--mc] [--target-peak <nits>]");
        }

        Ok(Self {
            hdr_uhdr_path,
            sdr_path,
            out_path,
            base_quality,
            gainmap_quality,
            gainmap_scale,
            multichannel_gainmap,
            target_peak_nits,
        })
    }
}

fn read_gainmap_metadata(buf: &mut [u8]) -> Result<Option<GainMapMetadata>> {
    let mut dec = Decoder::new()?;
    let mut comp = CompressedImage::from_bytes(
        buf,
        sys::uhdr_color_gamut::UHDR_CG_UNSPECIFIED,
        sys::uhdr_color_transfer::UHDR_CT_UNSPECIFIED,
        sys::uhdr_color_range::UHDR_CR_UNSPECIFIED,
    );
    dec.set_image(&mut comp)?;
    Ok(dec.gainmap_metadata()?)
}

fn main() -> Result<()> {
    let args = Args::parse()?;

    let mut hdr_bytes = fs::read(&args.hdr_uhdr_path)
        .with_context(|| format!("Failed to read HDR UltraHDR file {}", args.hdr_uhdr_path))?;
    let mut sdr_bytes = fs::read(&args.sdr_path)
        .with_context(|| format!("Failed to read SDR JPEG file {}", args.sdr_path))?;
    let gainmap_meta = read_gainmap_metadata(&mut hdr_bytes)?;

    // Decode HDR intent from UltraHDR JPEG.
    let mut dec = Decoder::new()?;
    let mut hdr_comp = CompressedImage::from_bytes(
        &mut hdr_bytes,
        sys::uhdr_color_gamut::UHDR_CG_UNSPECIFIED,
        sys::uhdr_color_transfer::UHDR_CT_UNSPECIFIED,
        sys::uhdr_color_range::UHDR_CR_UNSPECIFIED,
    );
    dec.set_image(&mut hdr_comp)?;
    let mut hdr_view = dec.decode_packed_view(
        sys::uhdr_img_fmt::UHDR_IMG_FMT_32bppRGBA1010102,
        sys::uhdr_color_transfer::UHDR_CT_PQ,
    )?;
    {
        if hdr_view.meta().0 == sys::uhdr_color_gamut::UHDR_CG_UNSPECIFIED {
            hdr_view.set_color_gamut(sys::uhdr_color_gamut::UHDR_CG_DISPLAY_P3);
        }
        if hdr_view.meta().1 == sys::uhdr_color_transfer::UHDR_CT_UNSPECIFIED {
            hdr_view.set_color_transfer(sys::uhdr_color_transfer::UHDR_CT_PQ);
        }
        hdr_view.set_color_range(sys::uhdr_color_range::UHDR_CR_FULL_RANGE);
    }

    // Encode with provided SDR base JPEG.
    let mut enc = Encoder::new()?;
    enc.set_raw_image_view(&mut hdr_view, ImgLabel::UHDR_HDR_IMG)?;

    let mut sdr_comp = CompressedImage::from_bytes(
        &mut sdr_bytes,
        sys::uhdr_color_gamut::UHDR_CG_DISPLAY_P3,
        sys::uhdr_color_transfer::UHDR_CT_SRGB,
        sys::uhdr_color_range::UHDR_CR_FULL_RANGE,
    );
    enc.set_compressed_image(&mut sdr_comp, ImgLabel::UHDR_SDR_IMG)?;

    enc.set_quality(args.base_quality, ImgLabel::UHDR_BASE_IMG)?;
    enc.set_quality(args.gainmap_quality, ImgLabel::UHDR_GAIN_MAP_IMG)?;
    enc.set_gainmap_scale_factor(args.gainmap_scale)?;
    enc.set_using_multi_channel_gainmap(args.multichannel_gainmap)?;
    enc.set_gainmap_gamma(1.0)?;
    let target_peak = args
        .target_peak_nits
        .or_else(|| gainmap_meta.as_ref().map(|m| m.target_display_peak_nits()))
        .unwrap_or(1600.0);
    if let Some(meta) = &gainmap_meta {
        println!(
            "Source gain map target peak: {:.1} nits (hdr_capacity_max={:.3})",
            meta.target_display_peak_nits(),
            meta.hdr_capacity_max
        );
    }
    println!("Using target peak brightness: {:.1} nits", target_peak);
    enc.set_target_display_peak_brightness(target_peak)?;
    enc.set_output_format(sys::uhdr_codec::UHDR_CODEC_JPG)?;
    enc.set_preset(sys::uhdr_enc_preset::UHDR_USAGE_BEST_QUALITY)?;
    enc.encode()?;

    let out_view = enc
        .encoded_stream()
        .context("Encode returned null output")?;
    let bytes = out_view.bytes()?;
    fs::write(&args.out_path, bytes)
        .with_context(|| format!("Failed to write output {}", args.out_path))?;

    println!("Wrote {}", args.out_path);
    Ok(())
}
