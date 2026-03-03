use std::time::Instant;

use ultrahdr::decoder::Decoder;
use ultrahdr::types::*;

fn test_data_dir() -> std::path::PathBuf {
    std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/data")
}

#[test]
#[ignore]
fn bench_decode_performance() {
    let jpeg = std::fs::read(test_data_dir().join("p010_ultrahdr_sys.jpg"))
        .expect("failed to read test JPEG");

    let iters = 20;

    let configs: [(ColorTransfer, PixelFormat, &str); 4] = [
        (ColorTransfer::Srgb, PixelFormat::Rgba8888, "SRGB"),
        (ColorTransfer::Linear, PixelFormat::RgbaF16, "LINEAR"),
        (ColorTransfer::Hlg, PixelFormat::Rgba1010102, "HLG"),
        (ColorTransfer::Pq, PixelFormat::Rgba1010102, "PQ"),
    ];

    println!("\n--- Decode Performance ({iters} iterations) ---");
    for (transfer, format, name) in configs {
        // Warmup
        let _ = Decoder::new(&jpeg)
            .output_format(format)
            .output_transfer(transfer)
            .max_display_boost(4.0)
            .decode()
            .unwrap();

        let start = Instant::now();
        for _ in 0..iters {
            let _ = Decoder::new(&jpeg)
                .output_format(format)
                .output_transfer(transfer)
                .max_display_boost(4.0)
                .decode()
                .unwrap();
        }
        let ms = start.elapsed().as_secs_f64() * 1000.0 / iters as f64;
        println!("{name:>8}: {ms:.1} ms");
    }
}
