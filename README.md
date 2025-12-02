# libultrahdr-rs

Rust bindings for Google's UltraHDR gain-map JPEG library with a small CLI. / 基于 Google libultrahdr 的 Rust 绑定与命令行工具。

## Overview / 概览
- `ultrahdr-sys`: raw FFI bindings to `libultrahdr` built via CMake. / `ultrahdr-sys`：通过 CMake 构建的 `libultrahdr` 原始 FFI 绑定。
- `ultrahdr`: safe-ish wrapper around the FFI types plus helpers for gain map metadata, packed buffers, and error handling. / `ultrahdr`：封装 FFI，提供增益图元数据、打包缓冲区和错误处理辅助。
- `ultrahdr-bake`: CLI that bakes an UltraHDR JPEG from an HDR (gain map) JPEG + SDR base JPEG, and can assemble Motion Photos (JPEG + MP4). / `ultrahdr-bake`：将 HDR（增益图）JPEG 与 SDR 基础 JPEG 合成为 UltraHDR JPEG，并可组装 Motion Photo（JPEG + MP4）。
- Upstream sources live in the `libultrahdr` submodule (Apache-2.0). / 上游源码存放在 `libultrahdr` 子模块（Apache-2.0）。

## Repository layout / 目录结构
- `ultrahdr-sys/`: build script, bindgen output, and generated `sys` APIs. / `ultrahdr-sys/`：构建脚本、bindgen 输出与底层 `sys` 接口。
- `ultrahdr/`: ergonomic wrapper plus `examples/ultrahdr_app.rs` sample. / `ultrahdr/`：易用封装与示例 `examples/ultrahdr_app.rs`。
- `ultrahdr-bake/`: end-user CLI for baking UltraHDR JPEGs and assembling Motion Photos. / `ultrahdr-bake/`：面向用户的 UltraHDR 生成命令行，并可组装 Motion Photo。
- `libultrahdr/`: upstream C/C++ sources pulled as a git submodule. / `libultrahdr/`：上游 C/C++ 源码子模块。

## Prerequisites / 前置依赖
- Initialize submodules: `git submodule update --init --recursive`. / 初始化子模块：`git submodule update --init --recursive`。
- Build tools: `cmake`, `ninja` (optional but faster), `nasm`, `pkg-config`; install EGL/GLES dev packages when enabling the `gles` feature. / 构建工具：`cmake`、`ninja`（可选）、`nasm`、`pkg-config`；启用 `gles` 特性时需安装 EGL/GLES 开发包。
- By default the `vendored` feature builds libjpeg-turbo and friends; disable it to link against system libs. / 默认启用 `vendored` 从源码构建 libjpeg-turbo 等依赖；若要链接系统库可关闭该特性。
- To point at an existing `libultrahdr` checkout, set `ULTRAHDR_SRC_DIR=/path/to/libultrahdr`. / 若已有 `libultrahdr` 源码，可设置 `ULTRAHDR_SRC_DIR=/path/to/libultrahdr`。

## Quick start / 快速开始
```bash
# Build the CLI with default features (vendored, iso21496)
cargo build -p ultrahdr-bake --release

# Encode using explicit HDR/SDR inputs
target/release/ultrahdr-bake \
  --hdr hdr_gainmap.jpg \
  --sdr base_sdr.jpg \
  --out ultrahdr_out.jpg \
  --base-q 95 --gm-q 95

# Or let the tool auto-detect which JPEG is HDR vs SDR
target/release/ultrahdr-bake photo1.jpg photo2.jpg

# Build a Motion Photo (v2 metadata) from a still + MP4
target/release/ultrahdr-bake motion \
  --photo ultrahdr_out.jpg \
  --video clip.mp4 \
  --timestamp-us 0 \
  --out motionphoto.jpg
```
使用默认特性构建 CLI 并编码 UltraHDR 的示例如上。

## Library usage / 库用法示例
```rust
use ultrahdr::{sys, CompressedImage, Decoder, Encoder, ImgLabel, RawImage};

fn round_trip(buf: &mut [u8]) -> ultrahdr::Result<()> {
    // Decode UltraHDR JPEG to packed PQ RGBA1010102
    let mut dec = Decoder::new()?;
    let mut comp = CompressedImage::from_bytes(
        buf,
        sys::uhdr_color_gamut::UHDR_CG_UNSPECIFIED,
        sys::uhdr_color_transfer::UHDR_CT_UNSPECIFIED,
        sys::uhdr_color_range::UHDR_CR_UNSPECIFIED,
    );
    dec.set_image(&mut comp)?;
    let mut hdr = dec.decode_packed_view(
        sys::uhdr_img_fmt::UHDR_IMG_FMT_32bppRGBA1010102,
        sys::uhdr_color_transfer::UHDR_CT_PQ,
    )?;

    // Re-encode with an SDR base (omitted here) and gain map metadata preserved.
    let mut enc = Encoder::new()?;
    enc.set_raw_image_view(&mut hdr, ImgLabel::UHDR_HDR_IMG)?;
    enc.set_output_format(sys::uhdr_codec::UHDR_CODEC_JPG)?;
    enc.encode()?;
    let bytes = enc.encoded_stream().expect("no output").bytes()?;
    println!("Encoded {} bytes", bytes.len());
    Ok(())
}
```
解码 UltraHDR JPEG 并再次编码的简要示例。

## Features / 可选特性
- `vendored` (default): build libjpeg-turbo and other deps from source. / `vendored`（默认）：从源码构建 libjpeg-turbo 等依赖。
- `shared`: link dynamically against `libuhdr`. / `shared`：动态链接 `libuhdr`。
- `gles`: enable EGL/GLES support in upstream CMake. / `gles`：在上游启用 EGL/GLES 支持。
- `iso21496` (default): emit ISO/TS 21496-1 gain map metadata. / `iso21496`（默认）：写入 ISO/TS 21496-1 增益图元数据。

## Tests / 测试
Run with all features enabled to mirror CI. / 建议启用全部特性以对齐 CI。
```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features --locked
cargo test --workspace --all-features --locked
```

## License / 许可证
Apache-2.0, matching upstream `libultrahdr`. / 与上游 `libultrahdr` 相同的 Apache-2.0 许可。
