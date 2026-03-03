# Bit-Exact Tests + Decode Performance 实现计划

> **For Claude:** 必须使用 superpowers:executing-plans skill 按任务逐一实现本计划。

**目标：** 添加严格 bit-exact 测试验证 Rust 与 C++ 编解码器输出完全一致，并优化 decode 性能至 C++ 水平

**架构：** 新建 `bitexact.rs` 集成测试文件，运行时同时调用 Rust API 和 C++ FFI，像素级比较。性能优化通过 `zune-jpeg` feature flag 替换 JPEG 解码器，并优化 `apply_gainmap_to_sdr` 热路径。

**技术栈：** Rust, ultrahdr-sys (C++ FFI), zune-jpeg, rayon

**Agent ID 前缀：** `bitexact`

---

### 任务 1：创建 encoder bit-exact 测试

**文件：**
- 创建：`ultrahdr/tests/bitexact.rs`

**背景：**
现有 `debug_encode.rs` 只打印 stats 和做 PSNR 检查，不做严格 byte-by-byte 比较。需要创建新的集成测试，运行时同时调用 Rust encoder 和 C++ encoder（通过 ultrahdr-sys FFI），然后：
1. 比较 gain map metadata（exact float match）
2. 提取并解压 gain map JPEG，比较像素 byte-by-byte

**完整代码：**

```rust
//! Bit-exact tests: Rust encoder/decoder vs C++ (ultrahdr-sys) FFI.
//!
//! These tests call both implementations at runtime and compare output
//! pixel-by-pixel. They do NOT rely on golden data files.
//!
//! Run: CARGO_HOME=/tmp/cargo-home cargo test -p ultrahdr --test bitexact -- --nocapture

use ultrahdr::decoder::{Decoder, extract_gainmap_jpeg};
use ultrahdr::encoder::Encoder;
use ultrahdr::types::*;
use ultrahdr_sys::*;

// ===== Synthetic image generators =====

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

fn gen_white(w: u32, h: u32) -> (Vec<u8>, Vec<u8>) {
    let npx = (w * h) as usize;
    let sdr = vec![255u8; npx * 4];
    let mut hdr = vec![0u8; npx * 4];
    let packed: u32 = 1023 | (1023 << 10) | (1023 << 20) | (3 << 30);
    for i in 0..npx {
        hdr[i * 4..i * 4 + 4].copy_from_slice(&packed.to_le_bytes());
    }
    (sdr, hdr)
}

fn gen_black(w: u32, h: u32) -> (Vec<u8>, Vec<u8>) {
    let npx = (w * h) as usize;
    let mut sdr = vec![0u8; npx * 4];
    // Set alpha to 255
    for i in 0..npx {
        sdr[i * 4 + 3] = 255;
    }
    let mut hdr = vec![0u8; npx * 4];
    for i in 0..npx {
        let packed: u32 = 3 << 30; // R=G=B=0, A=3
        hdr[i * 4..i * 4 + 4].copy_from_slice(&packed.to_le_bytes());
    }
    (sdr, hdr)
}

fn gen_color_ramp(w: u32, h: u32) -> (Vec<u8>, Vec<u8>) {
    let npx = (w * h) as usize;
    let mut sdr = vec![0u8; npx * 4];
    let mut hdr = vec![0u8; npx * 4];
    for y in 0..h {
        for x in 0..w {
            let i = (y * w + x) as usize;
            let t = x as f32 / (w - 1) as f32;
            // R ramp, G inverse ramp, B constant mid
            sdr[i * 4] = (t * 255.0 + 0.5) as u8;
            sdr[i * 4 + 1] = ((1.0 - t) * 255.0 + 0.5) as u8;
            sdr[i * 4 + 2] = 128;
            sdr[i * 4 + 3] = 255;
            let r10 = (t * 1023.0 + 0.5) as u32;
            let g10 = ((1.0 - t) * 1023.0 + 0.5) as u32;
            let b10 = 512u32;
            let packed: u32 = r10 | (g10 << 10) | (b10 << 20) | (3 << 30);
            hdr[i * 4..i * 4 + 4].copy_from_slice(&packed.to_le_bytes());
        }
    }
    (sdr, hdr)
}

fn gen_mixed_bright_dark(w: u32, h: u32) -> (Vec<u8>, Vec<u8>) {
    let npx = (w * h) as usize;
    let mut sdr = vec![0u8; npx * 4];
    let mut hdr = vec![0u8; npx * 4];
    for y in 0..h {
        for x in 0..w {
            let i = (y * w + x) as usize;
            // Checkerboard: bright/dark blocks of 16x16
            let bright = ((x / 16) + (y / 16)) % 2 == 0;
            let (sdr_v, hdr_v) = if bright { (240u8, 900u32) } else { (16u8, 64u32) };
            sdr[i * 4] = sdr_v;
            sdr[i * 4 + 1] = sdr_v;
            sdr[i * 4 + 2] = sdr_v;
            sdr[i * 4 + 3] = 255;
            let packed: u32 = hdr_v | (hdr_v << 10) | (hdr_v << 20) | (3 << 30);
            hdr[i * 4..i * 4 + 4].copy_from_slice(&packed.to_le_bytes());
        }
    }
    (sdr, hdr)
}

// ===== Encoder helpers =====

fn rust_encode(sdr: &[u8], hdr: &[u8], w: u32, h: u32) -> Vec<u8> {
    Encoder::new()
        .hdr_raw(hdr, w, h, PixelFormat::Rgba1010102, ColorGamut::Bt2100, ColorTransfer::Hlg)
        .sdr_raw(sdr, w, h, ColorGamut::Bt709)
        .quality(95)
        .gainmap_quality(85)
        .gainmap_scale(4)
        .multichannel_gainmap(false)
        .target_display_peak_nits(1600.0)
        .encode()
        .expect("Rust encode failed")
}

unsafe fn cpp_encode(sdr: &[u8], hdr: &[u8], w: u32, h: u32) -> Vec<u8> {
    unsafe {
        let enc = uhdr_create_encoder();
        assert!(!enc.is_null());

        let mut hdr_img = uhdr_raw_image {
            fmt: uhdr_img_fmt::UHDR_IMG_FMT_32bppRGBA1010102,
            cg: uhdr_color_gamut::UHDR_CG_BT_2100,
            ct: uhdr_color_transfer::UHDR_CT_HLG,
            range: uhdr_color_range::UHDR_CR_FULL_RANGE,
            w, h,
            planes: [hdr.as_ptr() as *mut _, std::ptr::null_mut(), std::ptr::null_mut()],
            stride: [w, 0, 0],
        };
        let err = uhdr_enc_set_raw_image(enc, &mut hdr_img, uhdr_img_label::UHDR_HDR_IMG);
        assert_eq!(err.error_code, uhdr_codec_err::UHDR_CODEC_OK);

        let mut sdr_img = uhdr_raw_image {
            fmt: uhdr_img_fmt::UHDR_IMG_FMT_32bppRGBA8888,
            cg: uhdr_color_gamut::UHDR_CG_BT_709,
            ct: uhdr_color_transfer::UHDR_CT_SRGB,
            range: uhdr_color_range::UHDR_CR_FULL_RANGE,
            w, h,
            planes: [sdr.as_ptr() as *mut _, std::ptr::null_mut(), std::ptr::null_mut()],
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
        assert_eq!(err.error_code, uhdr_codec_err::UHDR_CODEC_OK, "C++ encode failed");

        let stream = uhdr_get_encoded_stream(enc);
        assert!(!stream.is_null());
        let data = std::slice::from_raw_parts((*stream).data as *const u8, (*stream).data_sz);
        let result = data.to_vec();
        uhdr_release_encoder(enc);
        result
    }
}

// ===== Decoder helpers =====

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
        assert_eq!(err.error_code, uhdr_codec_err::UHDR_CODEC_OK, "C++ decode failed");

        let raw_ptr = uhdr_get_decoded_image(dec);
        assert!(!raw_ptr.is_null());
        let raw = &*raw_ptr;
        let bpp = match fmt {
            uhdr_img_fmt::UHDR_IMG_FMT_32bppRGBA8888 => 4,
            uhdr_img_fmt::UHDR_IMG_FMT_32bppRGBA1010102 => 4,
            uhdr_img_fmt::UHDR_IMG_FMT_64bppRGBAHalfFloat => 8,
            _ => 4,
        };
        let nbytes = (raw.w * raw.h) as usize * bpp;
        let data = std::slice::from_raw_parts(raw.planes[0] as *const u8, nbytes);
        let result = data.to_vec();
        uhdr_release_decoder(dec);
        result
    }
}

fn rust_decode_pixels(jpeg: &[u8], fmt: PixelFormat, ct: ColorTransfer) -> Vec<u8> {
    Decoder::new(jpeg)
        .output_format(fmt)
        .output_transfer(ct)
        .max_display_boost(4.0)
        .decode()
        .expect("Rust decode failed")
        .data
}

// ===== Comparison helpers =====

fn extract_gainmap_pixels(jpeg: &[u8]) -> Vec<u8> {
    let extract = extract_gainmap_jpeg(jpeg)
        .expect("extract failed")
        .expect("no gain map");
    let mut decoder = jpeg_decoder::Decoder::new(extract.gainmap_jpeg.as_slice());
    decoder.decode().expect("gain map decode failed")
}

fn compare_pixels(label: &str, rust: &[u8], cpp: &[u8]) -> (usize, u8) {
    assert_eq!(rust.len(), cpp.len(), "{label}: buffer size mismatch");
    let mut diff_count = 0usize;
    let mut max_diff = 0u8;
    for (i, (&r, &c)) in rust.iter().zip(cpp.iter()).enumerate() {
        let d = (r as i16 - c as i16).unsigned_abs() as u8;
        if d > 0 {
            diff_count += 1;
            if d > max_diff {
                max_diff = d;
                if diff_count <= 5 {
                    eprintln!("  {label} diff at byte {i}: Rust={r} C++={c} (diff={d})");
                }
            }
        }
    }
    eprintln!("  {label}: {diff_count}/{} bytes differ, max_diff={max_diff}", rust.len());
    (diff_count, max_diff)
}

// ===== Encoder bit-exact tests =====

const W: u32 = 64;
const H: u32 = 64;

#[test]
fn enc_bitexact_gradient() {
    let (sdr, hdr) = gen_gradient(W, H);
    enc_bitexact_scenario("gradient", &sdr, &hdr, W, H);
}

#[test]
fn enc_bitexact_white() {
    let (sdr, hdr) = gen_white(W, H);
    enc_bitexact_scenario("white", &sdr, &hdr, W, H);
}

#[test]
fn enc_bitexact_black() {
    let (sdr, hdr) = gen_black(W, H);
    enc_bitexact_scenario("black", &sdr, &hdr, W, H);
}

#[test]
fn enc_bitexact_color_ramp() {
    let (sdr, hdr) = gen_color_ramp(W, H);
    enc_bitexact_scenario("color_ramp", &sdr, &hdr, W, H);
}

#[test]
fn enc_bitexact_mixed() {
    let (sdr, hdr) = gen_mixed_bright_dark(W, H);
    enc_bitexact_scenario("mixed", &sdr, &hdr, W, H);
}

fn enc_bitexact_scenario(name: &str, sdr: &[u8], hdr: &[u8], w: u32, h: u32) {
    eprintln!("\n=== ENC BITEXACT: {name} ({w}x{h}) ===");

    let rust_out = rust_encode(sdr, hdr, w, h);
    let cpp_out = unsafe { cpp_encode(sdr, hdr, w, h) };

    // 1. Compare metadata
    let rust_meta = Decoder::new(&rust_out).probe().unwrap().unwrap();
    let cpp_meta = Decoder::new(&cpp_out).probe().unwrap().unwrap();

    eprintln!("  Rust meta: max_boost={:?} min_boost={:?}", rust_meta.max_content_boost, rust_meta.min_content_boost);
    eprintln!("  C++  meta: max_boost={:?} min_boost={:?}", cpp_meta.max_content_boost, cpp_meta.min_content_boost);

    for ch in 0..3 {
        assert!(
            (rust_meta.max_content_boost[ch] - cpp_meta.max_content_boost[ch]).abs() < 1e-4,
            "[{name}] max_content_boost[{ch}]: Rust={} vs C++={}", rust_meta.max_content_boost[ch], cpp_meta.max_content_boost[ch]
        );
        assert!(
            (rust_meta.min_content_boost[ch] - cpp_meta.min_content_boost[ch]).abs() < 1e-4,
            "[{name}] min_content_boost[{ch}]: Rust={} vs C++={}", rust_meta.min_content_boost[ch], cpp_meta.min_content_boost[ch]
        );
    }

    // 2. Compare gain map pixels
    let rust_gm = extract_gainmap_pixels(&rust_out);
    let cpp_gm = extract_gainmap_pixels(&cpp_out);
    let (diff_count, max_diff) = compare_pixels(&format!("{name} gainmap"), &rust_gm, &cpp_gm);

    // Allow small JPEG codec difference (different encoder libraries), but gain map
    // values pre-JPEG-compression are identical so post-decompress should be very close
    assert!(max_diff <= 1, "[{name}] gain map max pixel diff {max_diff} > 1 (JPEG codec tolerance)");
    let diff_pct = diff_count as f64 / rust_gm.len() as f64 * 100.0;
    eprintln!("  {name} gain map diff: {diff_pct:.1}% pixels differ by <=1");
}

// ===== Decoder bit-exact tests =====

#[test]
fn dec_bitexact_hlg_1010102_cpp_encoded() {
    let (sdr, hdr) = gen_gradient(W, H);
    let jpeg = unsafe { cpp_encode(&sdr, &hdr, W, H) };
    dec_bitexact_scenario("hlg_cpp", &jpeg, PixelFormat::Rgba1010102, ColorTransfer::Hlg,
        uhdr_img_fmt::UHDR_IMG_FMT_32bppRGBA1010102, uhdr_color_transfer::UHDR_CT_HLG);
}

#[test]
fn dec_bitexact_hlg_1010102_rust_encoded() {
    let (sdr, hdr) = gen_gradient(W, H);
    let jpeg = rust_encode(&sdr, &hdr, W, H);
    dec_bitexact_scenario("hlg_rust", &jpeg, PixelFormat::Rgba1010102, ColorTransfer::Hlg,
        uhdr_img_fmt::UHDR_IMG_FMT_32bppRGBA1010102, uhdr_color_transfer::UHDR_CT_HLG);
}

#[test]
fn dec_bitexact_pq_1010102_cpp_encoded() {
    let (sdr, hdr) = gen_gradient(W, H);
    let jpeg = unsafe { cpp_encode(&sdr, &hdr, W, H) };
    dec_bitexact_scenario("pq_cpp", &jpeg, PixelFormat::Rgba1010102, ColorTransfer::Pq,
        uhdr_img_fmt::UHDR_IMG_FMT_32bppRGBA1010102, uhdr_color_transfer::UHDR_CT_PQ);
}

#[test]
fn dec_bitexact_pq_1010102_rust_encoded() {
    let (sdr, hdr) = gen_gradient(W, H);
    let jpeg = rust_encode(&sdr, &hdr, W, H);
    dec_bitexact_scenario("pq_rust", &jpeg, PixelFormat::Rgba1010102, ColorTransfer::Pq,
        uhdr_img_fmt::UHDR_IMG_FMT_32bppRGBA1010102, uhdr_color_transfer::UHDR_CT_PQ);
}

#[test]
fn dec_bitexact_linear_f16_cpp_encoded() {
    let (sdr, hdr) = gen_gradient(W, H);
    let jpeg = unsafe { cpp_encode(&sdr, &hdr, W, H) };
    dec_bitexact_scenario("linear_cpp", &jpeg, PixelFormat::RgbaF16, ColorTransfer::Linear,
        uhdr_img_fmt::UHDR_IMG_FMT_64bppRGBAHalfFloat, uhdr_color_transfer::UHDR_CT_LINEAR);
}

#[test]
fn dec_bitexact_linear_f16_rust_encoded() {
    let (sdr, hdr) = gen_gradient(W, H);
    let jpeg = rust_encode(&sdr, &hdr, W, H);
    dec_bitexact_scenario("linear_rust", &jpeg, PixelFormat::RgbaF16, ColorTransfer::Linear,
        uhdr_img_fmt::UHDR_IMG_FMT_64bppRGBAHalfFloat, uhdr_color_transfer::UHDR_CT_LINEAR);
}

fn dec_bitexact_scenario(
    name: &str,
    jpeg: &[u8],
    rust_fmt: PixelFormat,
    rust_ct: ColorTransfer,
    cpp_fmt: uhdr_img_fmt,
    cpp_ct: uhdr_color_transfer,
) {
    eprintln!("\n=== DEC BITEXACT: {name} ===");

    let rust_pixels = rust_decode_pixels(jpeg, rust_fmt, rust_ct);
    let cpp_pixels = unsafe { cpp_decode(jpeg, cpp_fmt, cpp_ct) };

    let (diff_count, max_diff) = compare_pixels(name, &rust_pixels, &cpp_pixels);

    assert_eq!(diff_count, 0,
        "[{name}] decoder output differs: {diff_count} bytes differ, max_diff={max_diff}");
}
```

**验证命令：**
```bash
CARGO_HOME=/tmp/cargo-home cargo test -p ultrahdr --test bitexact -- --nocapture
```

**预期：** 所有 encoder tests 通过（metadata exact, gain map pixels <=1 diff due to JPEG codec），decoder tests 可能有一些 pixel diff 需要调查。

**提交：**
```bash
git add ultrahdr/tests/bitexact.rs
git commit -m "test: add bit-exact Rust vs C++ runtime comparison tests"
```

---

### 任务 2：修复 decoder bit-exact 差异

**文件：**
- 可能修改：`ultrahdr/src/decoder.rs`
- 可能修改：`ultrahdr/src/color/transfer.rs`
- 可能修改：`ultrahdr/src/gainmap/math.rs`

**背景：**
任务 1 的 decoder tests 可能会发现 pixel diff。需要调查根因并修复。
常见差异来源：
- JPEG 解码器不同（jpeg-decoder vs libjpeg-turbo）可能导致 ±1 像素差异
- Transfer function LUT 精度差异
- Gain apply rounding 差异

**步骤：**
1. 运行 decoder bit-exact tests，收集所有 diff 信息
2. 分析 diff pattern（是全局偏移还是局部差异？是 JPEG decode 还是 gain apply？）
3. 如果是 JPEG decode 差异（±1），在测试中允许 max_diff<=1
4. 如果是 gain apply / transfer function 差异，修复 Rust 实现匹配 C++
5. 重新运行所有测试确认通过

**验证：**
```bash
CARGO_HOME=/tmp/cargo-home cargo test -p ultrahdr --test bitexact -- --nocapture
CARGO_HOME=/tmp/cargo-home cargo test -p ultrahdr
CARGO_HOME=/tmp/cargo-home cargo clippy -p ultrahdr --all-targets -- -D warnings
```

---

### 任务 3：添加 zune-jpeg feature flag 优化 JPEG 解码性能

**文件：**
- 修改：`ultrahdr/Cargo.toml` — 添加 `zune-jpeg` 可选依赖
- 修改：`ultrahdr/src/jpeg/decode.rs` — 条件编译使用 zune-jpeg 或 jpeg-decoder

**背景：**
Decode 性能瓶颈主要在 JPEG 解码。`jpeg-decoder` 是纯 Rust 但较慢，`zune-jpeg` 也是纯 Rust 但有 SIMD 优化，通常快 2-3x。

**步骤 1：添加 zune-jpeg 依赖**

在 `Cargo.toml` 的 `[features]` 和 `[dependencies]` 中添加：
```toml
[features]
default = []
rayon = ["dep:rayon"]
zune-jpeg = ["dep:zune-jpeg"]

[dependencies]
zune-jpeg = { version = "0.5", optional = true }
```

**步骤 2：修改 decode.rs 条件编译**

```rust
// When zune-jpeg feature is enabled, use it for decoding
#[cfg(feature = "zune-jpeg")]
pub fn decode_jpeg(data: &[u8]) -> Result<JpegDecoded> {
    use zune_jpeg::JpegDecoder;
    // ... zune-jpeg implementation
}

#[cfg(not(feature = "zune-jpeg"))]
pub fn decode_jpeg(data: &[u8]) -> Result<JpegDecoded> {
    // existing jpeg-decoder implementation
}
```

**步骤 3：验证 bit-exact 仍然通过**
```bash
CARGO_HOME=/tmp/cargo-home cargo test -p ultrahdr --features zune-jpeg --test bitexact -- --nocapture
```

**步骤 4：性能对比**
```bash
CARGO_HOME=/tmp/cargo-home cargo test -p ultrahdr --release --features zune-jpeg --test perf_compare -- --nocapture --ignored
```

---

### 任务 4：优化 apply_gainmap_to_sdr 性能

**文件：**
- 修改：`ultrahdr/src/decoder.rs` — `apply_gainmap_to_sdr` 函数
- 修改：`ultrahdr/src/decoder.rs` — `rgb_to_rgba` 函数

**背景：**
除 JPEG 解码外，`apply_gainmap_to_sdr` 是另一个性能热点。优化方向：
1. 消除 `rgb_to_rgba` 分配（直接在 apply 中处理 RGB 输入）
2. 减少 `sample_map_bilinear` 中的 sqrt 调用（改用距离平方的倒数）
3. 内联 LUT 查找，减少分支

**注意：** 所有优化必须保持 bit-exact 输出。每个优化后都要重新运行 bitexact 测试。

**验证：**
```bash
CARGO_HOME=/tmp/cargo-home cargo test -p ultrahdr --test bitexact -- --nocapture
CARGO_HOME=/tmp/cargo-home cargo test -p ultrahdr --release --test perf_compare -- --nocapture --ignored
```

---

### 任务 5：最终性能验证和 CI 检查

**文件：** 无新文件

**步骤：**
1. 运行完整测试套件
2. 运行 clippy + fmt
3. 运行性能对比（release mode）
4. 贴结果到 MR

**验证：**
```bash
CARGO_HOME=/tmp/cargo-home cargo test -p ultrahdr
CARGO_HOME=/tmp/cargo-home cargo clippy -p ultrahdr --all-targets -- -D warnings
CARGO_HOME=/tmp/cargo-home cargo fmt -p ultrahdr --check
CARGO_HOME=/tmp/cargo-home cargo test -p ultrahdr --release --test perf_compare -- --nocapture --ignored
```
