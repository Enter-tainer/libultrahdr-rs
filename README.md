# libultrahdr-rs

[![ultrahdr crates.io](https://img.shields.io/crates/v/ultrahdr.svg?label=ultrahdr)](https://crates.io/crates/ultrahdr)
[![ultrahdr-sys crates.io](https://img.shields.io/crates/v/ultrahdr-sys.svg?label=ultrahdr-sys)](https://crates.io/crates/ultrahdr-sys)
[![ultrahdr docs.rs](https://docs.rs/ultrahdr/badge.svg)](https://docs.rs/ultrahdr)
[![ultrahdr-sys docs.rs](https://docs.rs/ultrahdr-sys/badge.svg)](https://docs.rs/ultrahdr-sys)

Rust bindings for Google's UltraHDR gain-map JPEG library with a small CLI. / 基于 Google libultrahdr 的 Rust 绑定与命令行工具。

## Overview / 概览
- `ultrahdr-sys`: raw FFI bindings to `libultrahdr` built via CMake. / `ultrahdr-sys`：通过 CMake 构建的 `libultrahdr` 原始 FFI 绑定。
- `ultrahdr`: safe-ish wrapper around the FFI types plus helpers for gain map metadata, packed buffers, and error handling. / `ultrahdr`：封装 FFI，提供增益图元数据、打包缓冲区和错误处理辅助。
- `ultrahdr-bake`: CLI that bakes an UltraHDR JPEG from an HDR (gain map) JPEG + SDR base JPEG, and can assemble Motion Photos (JPEG + MP4). / `ultrahdr-bake`：将 HDR（增益图）JPEG 与 SDR 基础 JPEG 合成为 UltraHDR JPEG，并可组装 Motion Photo（JPEG + MP4）。
- Upstream sources live in the `ultrahdr-sys/libultrahdr` submodule (Apache-2.0). / 上游源码存放在 `ultrahdr-sys/libultrahdr` 子模块（Apache-2.0）。
- `ultrahdr-browser`: Vite/React demo that runs `ultrahdr-bake` via WASI in the browser; / `ultrahdr-browser`：基于 Vite/React 的浏览器演示，通过 WASI 运行 `ultrahdr-bake`

## Repository layout / 目录结构
- `ultrahdr-sys/`: build script, bindgen output, and generated `sys` APIs. / `ultrahdr-sys/`：构建脚本、bindgen 输出与底层 `sys` 接口。
- `ultrahdr/`: ergonomic wrapper plus `examples/ultrahdr_app.rs` sample. / `ultrahdr/`：易用封装与示例 `examples/ultrahdr_app.rs`。
- `ultrahdr-bake/`: end-user CLI for baking UltraHDR JPEGs and assembling Motion Photos. / `ultrahdr-bake/`：面向用户的 UltraHDR 生成命令行，并可组装 Motion Photo。
- `ultrahdr-sys/libultrahdr/`: upstream C/C++ sources pulled as a git submodule. / `ultrahdr-sys/libultrahdr/`：上游 C/C++ 源码子模块。

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
  --base-q 95 --gm-q 100

# Or let the tool auto-detect which JPEG is HDR vs SDR
target/release/ultrahdr-bake photo1.jpg photo2.jpg

# Write AVIF/HEIF instead of JPEG (needs the `heif` feature)
cargo build -p ultrahdr-bake --release --features heif
target/release/ultrahdr-bake --hdr hdr_gainmap.jpg --sdr base_sdr.jpg --format avif --out out.avif

# Build a Motion Photo (v2 metadata) from a still + MP4
target/release/ultrahdr-bake motion \
  --photo ultrahdr_out.jpg \
  --video clip.mp4 \
  --timestamp-us 0 \
  --out motionphoto.jpg

# Build the browser demo (wasm + Vite/React)
pnpm --dir ultrahdr-browser install --frozen-lockfile
pnpm --dir ultrahdr-browser build
```
使用默认特性构建 CLI 并编码 UltraHDR 的示例如上；`--format jpeg|avif|heif` 用于选择输出容器（AVIF/HEIF 需 `heif` 特性）。

Baking defaults to RGB multi-channel gain maps, gain map JPEG quality 100, and scale 1 to preserve HDR colors. Use `--multichannel=false` for a single-channel gain map or `--gm-q` to reduce quality and file size.

合并默认使用 RGB 三通道增益图、增益图 JPEG 质量 100、缩放因子 1，以保留 HDR 颜色。可通过 `--multichannel=false` 切换为单通道，或用 `--gm-q` 降低质量以减小文件体积。

HEIF/AVIF output keeps the base image inside the container, so libultrahdr encodes those formats from
raw intents: the CLI decodes the SDR JPEG itself and submits raw pixels. Upstream also corrupts the
heap when a container carries a multi-channel gain map whose height is odd, so `--format avif|heif`
falls back to a single-channel gain map and warns instead of aborting. /
HEIF/AVIF 会把基础图放进容器，libultrahdr 对这两种格式要求原始（raw）输入，因此 CLI 自行解码 SDR JPEG
再提交 raw 像素。另外，容器内多通道增益图高度为奇数时上游会破坏堆内存，所以 `--format avif|heif`
会自动回退成单通道增益图并打印警告，而不是直接崩溃。

Browser demo: deploys under root by default; GitHub Pages build sets `VITE_BASE_PATH=/libultrahdr-rs/`. The wasm (`ultrahdr-bake.wasm`) is fetched relative to `import.meta.env.BASE_URL`. /
浏览器演示：默认以根路径部署；在 GitHub Pages 上构建时使用 `VITE_BASE_PATH=/libultrahdr-rs/`，WASM（`ultrahdr-bake.wasm`）从 `import.meta.env.BASE_URL` 相对路径加载。

## Library usage / 库用法示例
```rust
use ultrahdr::{
    Codec, ColorAspects, ColorGamut, ColorRange, ColorTransfer, CompressedImage, Decoder, Encoder,
    ImageLabel, PixelFormat,
};

fn round_trip(jpeg: &[u8]) -> ultrahdr::Result<()> {
    // Decode an UltraHDR JPEG into PQ RGBA1010102 pixels.
    let mut dec = Decoder::new()?;
    dec.set_image(&CompressedImage::new(jpeg))?;
    dec.set_output_format(PixelFormat::Rgba1010102)?;
    dec.set_output_transfer(ColorTransfer::Pq)?;

    let info = dec.info()?;                       // dimensions + gain map metadata
    let decoded = dec.decode()?.to_owned_image();
    println!("{}x{} -> {} bytes", info.width, info.height, decoded.data.len());

    // Re-encode. Aspects the stream did not signal must be filled in for raw input.
    let mut raw = decoded.into_raw_image()?;
    raw.set_aspects(ColorAspects::new(
        ColorGamut::DisplayP3,
        ColorTransfer::Pq,
        ColorRange::Full,
    ));

    let mut enc = Encoder::new()?;
    enc.set_raw_image(ImageLabel::Hdr, &raw)?;
    enc.set_output_format(Codec::Jpeg)?;
    enc.encode()?;
    println!("Encoded {} bytes", enc.encoded_stream().expect("no output").bytes().len());
    Ok(())
}
```
解码 UltraHDR JPEG 并再次编码的简要示例。

### Codec surface / 接口覆盖

The safe wrapper exposes the full `ultrahdr_api.h` surface, with owning image types and Rust enums
instead of the C constants; `ultrahdr::sys` re-exports the raw bindings for anything not covered. /
安全封装覆盖 `ultrahdr_api.h` 的全部接口：图像类型拥有自己的数据（无生命周期），C 常量换成 Rust 枚举，`ultrahdr::sys` 保留原始绑定。

- Detect: `is_uhdr_image`, `Decoder::{probe, is_uhdr_image}`.
- Stream info: `Decoder::info` (`ImageInfo`) plus `Decoder::{image_width, image_height, gainmap_width, gainmap_height, gainmap_metadata}`.
- Embedded data: `Decoder::{exif, icc, base_image, gainmap_image}` returning `MemBlockView`.
- Encode inputs: `Encoder::{set_raw_image, set_decoded_image, set_compressed_image, set_gainmap_image, set_exif_data}`.
- Encode tuning: `Encoder::{set_quality, set_gainmap_scale_factor, set_multi_channel_gainmap, set_gainmap_gamma, set_min_max_content_boost, set_target_display_peak_brightness, set_preset, set_output_format}`.
- Decode output: `Decoder::{set_output_format, set_output_transfer, set_max_display_boost, decode, decode_as, decoded_gainmap}`.
- Shared: `enable_gpu_acceleration`, `mirror`/`rotate`/`crop`/`resize`, `reset`, `LIB_VERSION` / `version_string()`.
- Pixel buffers: `RawImage::{new, from_packed, from_planes, yuv420, p010}` (owning, packed or planar) and `DecodedImage::into_raw_image` for zero-copy re-encode.

**Ordering caveat.** libultrahdr freezes a decoder as soon as it is probed: output settings and the input image can no longer change until `reset()`. Configure `set_output_format` / `set_output_transfer` before calling an info getter (they probe on demand); `decode_as` skips redundant setters, so the ordering above works. /
**顺序注意。** 解码器一旦 `probe` 就会锁定配置，必须先用 `set_output_*` 设定输出，再调用信息 getter 或解码；`reset()` 后才能重新配置。

## Features / 可选特性
- `vendored` (default): build libjpeg-turbo and other deps from source. / `vendored`（默认）：从源码构建 libjpeg-turbo 等依赖。
- `shared`: link dynamically against `libuhdr`. / `shared`：动态链接 `libuhdr`。
- `gles`: enable EGL/GLES support in upstream CMake. / `gles`：在上游启用 EGL/GLES 支持。
- `heif`: HEIF/HEIC and AVIF containers via libheif (one upstream switch covers both; needs network at build time, or a system libheif with the ISO 21496-1 API). libheif only supplies the container plumbing, so the codec set is provisioned per target and reported as a build warning: native builds use the host's codec libraries and fail to build if none of them is present, while `wasm32-wasip1` cross-compiles libaom (AVIF works, HEVC has no WASI port). / `heif`：通过 libheif 支持 HEIF/HEIC 与 AVIF 容器（上游只有一个开关同时覆盖两者）。libheif 只提供容器能力，codec 按目标平台供给并以构建警告列出：宿主构建使用系统的 codec 库、一个都没有时直接构建失败；`wasm32-wasip1` 则交叉编译 libaom（AVIF 可用，HEVC 没有 WASI 移植）。
- `iso21496` (default): emit ISO/TS 21496-1 gain map metadata. / `iso21496`（默认）：写入 ISO/TS 21496-1 增益图元数据。
- `xmp` (default): also write XMP (`GContainer` + `hdrgm`) gain map metadata for older readers. / `xmp`（默认）：同时写入 XMP 元数据，兼容旧版读取器。
- `smpte2094-50`: SMPTE ST 2094-50 dynamic metadata (AGTM). / `smpte2094-50`：SMPTE ST 2094-50 动态元数据（AGTM）。
- `no-threads`: build upstream without `std::thread`. / `no-threads`：上游构建禁用 `std::thread`。
- `jpeg-max-dimension`: raise the JPEG dimension limit. / `jpeg-max-dimension`：提高 JPEG 尺寸上限。

## Tests / 测试
Run with all features enabled to mirror CI. / 建议启用全部特性以对齐 CI。
```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features --locked
cargo test --workspace --all-features --locked
```

## License / 许可证
Apache-2.0, matching upstream `libultrahdr`. / 与上游 `libultrahdr` 相同的 Apache-2.0 许可。
