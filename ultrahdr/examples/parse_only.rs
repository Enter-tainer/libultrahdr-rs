use std::fs;
use std::path::PathBuf;

use ultrahdr::parse_ultra_hdr;

/// Demonstration of the parse-only API: probe an UltraHDR image (JPEG / HEIF /
/// AVIF container) and print base / gain-map sizes, dimensions and gain-map
/// metadata WITHOUT decoding pixels. The returned owned bytes are what a JS +
/// WebGPU renderer would consume (decode with WebCodecs, upload to WebGPU).
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let path = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "image.jpg".into());
    let mut bytes = fs::read(PathBuf::from(&path))?;

    let layout = parse_ultra_hdr(&mut bytes)?;
    println!("container: {:?}", layout.container);
    println!("gainmap bytes: {}", layout.gainmap_image.len());
    println!("image dims: {}x{}", layout.width, layout.height);
    println!(
        "gainmap dims: {}x{}",
        layout.gainmap_width, layout.gainmap_height
    );
    println!("exif: {}", layout.exif.map_or(0, |b| b.len()));
    println!("icc: {}", layout.icc.map_or(0, |b| b.len()));
    if let Some(m) = &layout.gainmap_metadata {
        println!(
            "metadata: boost_min={:?} boost_max={:?} gamma={:?} off_sdr={:?} off_hdr={:?}",
            m.min_content_boost, m.max_content_boost, m.gamma, m.offset_sdr, m.offset_hdr
        );
        println!(
            "capacity: {}..{}  use_base_cg={}  target_peak_nits={}",
            m.hdr_capacity_min,
            m.hdr_capacity_max,
            m.use_base_cg,
            m.target_display_peak_nits()
        );
    } else {
        println!("metadata: <none>");
    }
    Ok(())
}
