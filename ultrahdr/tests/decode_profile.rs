// Quick decode profiling
// Run: CARGO_HOME=/tmp/cargo-home cargo test -p ultrahdr --release --test decode_profile -- --nocapture --ignored

use std::time::Instant;

use ultrahdr::decoder::{apply_gainmap_to_sdr_rgb, extract_gainmap_jpeg};
use ultrahdr::encoder::Encoder;
use ultrahdr::types::*;

const W: u32 = 512;
const H: u32 = 512;
const ITERS: usize = 10;

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

#[test]
#[ignore]
fn decode_profile() {
    let (sdr, hdr) = gen_gradient(W, H);
    let jpeg = Encoder::new()
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
        .unwrap();

    // Warmup
    let _ = ultrahdr::decoder::Decoder::new(&jpeg)
        .output_format(PixelFormat::Rgba1010102)
        .output_transfer(ColorTransfer::Hlg)
        .max_display_boost(4.0)
        .decode()
        .unwrap();

    // Step-by-step profiling
    let mut t_extract = 0.0;
    let mut t_primary = 0.0;
    let mut t_gainmap_decode = 0.0;
    let mut t_apply = 0.0;

    for _ in 0..ITERS {
        let t0 = Instant::now();
        let extract = extract_gainmap_jpeg(&jpeg).unwrap().unwrap();
        t_extract += t0.elapsed().as_secs_f64();

        let t1 = Instant::now();
        let primary = ultrahdr::jpeg::decode::decode_jpeg(&jpeg).unwrap();
        t_primary += t1.elapsed().as_secs_f64();

        let t2 = Instant::now();
        let gm = ultrahdr::jpeg::decode::decode_jpeg(&extract.gainmap_jpeg).unwrap();
        t_gainmap_decode += t2.elapsed().as_secs_f64();

        let t3 = Instant::now();
        let _ = apply_gainmap_to_sdr_rgb(
            &primary.pixels,
            primary.width as usize,
            primary.height as usize,
            &gm.pixels,
            gm.width as usize,
            gm.height as usize,
            &extract.metadata,
            4.0,
            ColorTransfer::Hlg,
            PixelFormat::Rgba1010102,
        )
        .unwrap();
        t_apply += t3.elapsed().as_secs_f64();
    }

    let n = ITERS as f64;
    println!(
        "\n=== Decode Step Profiling ({}x{}, avg of {} runs) ===",
        W, H, ITERS
    );
    println!("  Extract gain map:  {:.1} ms", t_extract / n * 1000.0);
    println!("  Decode primary:    {:.1} ms", t_primary / n * 1000.0);
    println!(
        "  Decode gain map:   {:.1} ms",
        t_gainmap_decode / n * 1000.0
    );
    println!("  Apply gain map:    {:.1} ms", t_apply / n * 1000.0);
    println!(
        "  Total:             {:.1} ms",
        (t_extract + t_primary + t_gainmap_decode + t_apply) / n * 1000.0
    );

    // Compare HLG vs PQ apply time to measure powf overhead
    let extract = extract_gainmap_jpeg(&jpeg).unwrap().unwrap();
    let primary = ultrahdr::jpeg::decode::decode_jpeg(&jpeg).unwrap();
    let gm = ultrahdr::jpeg::decode::decode_jpeg(&extract.gainmap_jpeg).unwrap();

    // Warmup PQ path
    let _ = apply_gainmap_to_sdr_rgb(
        &primary.pixels,
        primary.width as usize,
        primary.height as usize,
        &gm.pixels,
        gm.width as usize,
        gm.height as usize,
        &extract.metadata,
        4.0,
        ColorTransfer::Pq,
        PixelFormat::Rgba1010102,
    )
    .unwrap();

    let mut t_pq = 0.0;
    let mut t_hlg = 0.0;
    let mut t_linear = 0.0;
    for _ in 0..ITERS {
        let t = Instant::now();
        let _ = apply_gainmap_to_sdr_rgb(
            &primary.pixels,
            primary.width as usize,
            primary.height as usize,
            &gm.pixels,
            gm.width as usize,
            gm.height as usize,
            &extract.metadata,
            4.0,
            ColorTransfer::Pq,
            PixelFormat::Rgba1010102,
        )
        .unwrap();
        t_pq += t.elapsed().as_secs_f64();

        let t = Instant::now();
        let _ = apply_gainmap_to_sdr_rgb(
            &primary.pixels,
            primary.width as usize,
            primary.height as usize,
            &gm.pixels,
            gm.width as usize,
            gm.height as usize,
            &extract.metadata,
            4.0,
            ColorTransfer::Hlg,
            PixelFormat::Rgba1010102,
        )
        .unwrap();
        t_hlg += t.elapsed().as_secs_f64();

        let t = Instant::now();
        let _ = apply_gainmap_to_sdr_rgb(
            &primary.pixels,
            primary.width as usize,
            primary.height as usize,
            &gm.pixels,
            gm.width as usize,
            gm.height as usize,
            &extract.metadata,
            4.0,
            ColorTransfer::Linear,
            PixelFormat::Rgba1010102,
        )
        .unwrap();
        t_linear += t.elapsed().as_secs_f64();
    }

    println!("\n=== Transfer Function Comparison ===");
    println!("  Linear:  {:.1} ms", t_linear / n * 1000.0);
    println!("  PQ:      {:.1} ms", t_pq / n * 1000.0);
    println!("  HLG:     {:.1} ms", t_hlg / n * 1000.0);
}
