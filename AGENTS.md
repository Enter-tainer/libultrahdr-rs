# Agent notes

What this repo is: Rust bindings for Google's `libultrahdr` gain-map JPEG library plus a small CLI (`ultrahdr-bake`). The upstream C/C++ sources live in the `ultrahdr-sys/libultrahdr` submodule.

## Layout
- `ultrahdr-sys/`: build.rs drives CMake; bindgen output is included via `OUT_DIR`. Features: `vendored` (default, builds libjpeg-turbo etc.), `shared`, `gles`, `iso21496` (default).
- `ultrahdr/`: safer wrapper types (`Encoder`, `Decoder`, `RawImage`, `CompressedImage`, `GainMapMetadata`, etc.) and `examples/ultrahdr_app.rs` showcasing encode/decode.
- `ultrahdr-bake/`: end-user CLI that fuses an HDR gain-map JPEG + SDR JPEG into an UltraHDR JPEG. Entrypoints in `src/main.rs`, CLI args in `src/cli.rs`, detection logic in `src/detect.rs`, encoding in `src/encode.rs`.
- `ultrahdr-sys/libultrahdr/`: git submodule for upstream sources; can be overridden with `ULTRAHDR_SRC_DIR`.

## Build & test
- Ensure submodules are present: `git submodule update --init --recursive`.
- Tooling prerequisites: `cmake`, `ninja` (optional), `nasm`, `pkg-config`; add EGL/GLES headers if building with `--features gles`.
- Default build (vendored deps): `cargo build -p ultrahdr-bake --release`.
- WASM/WASI: target `wasm32-wasip1` with wasi-sdk (toolchain file at `/opt/wasi-sdk/share/cmake/wasi-sdk-p1.cmake`). `cargo build --target wasm32-wasip1 -p ultrahdr-bake --release` works with vendored deps after cloning `third_party/turbojpeg`. Run via wasmtime with import stubbing, e.g. `XDG_CACHE_HOME=$(pwd)/.cache wasmtime -W unknown-imports-default=yes --dir=. target/wasm32-wasip1/release/ultrahdr-bake.wasm --help`.
- CI parity checks:  
  `cargo fmt --all -- --check`  
  `cargo clippy --workspace --all-targets --all-features --locked`  
  `cargo test --workspace --all-features --locked`

## Common tasks
- Link against a system-provided libjpeg/libuhdr: disable `vendored`, optionally enable `shared`.
- Point at an external libultrahdr checkout: set `ULTRAHDR_SRC_DIR=/path/to/libultrahdr` before building.
- Try the wrapper example: `cargo run -p ultrahdr --example ultrahdr_app -- --help`.
- Bake an UltraHDR JPEG with auto-detection: `cargo run -p ultrahdr-bake -- photo1.jpg photo2.jpg`.

## Pitfalls
- Missing submodule or headers will surface as CMake failures in `ultrahdr-sys/build.rs`; check `ULTRAHDR_SRC_DIR` and prerequisites.
- Vendored libjpeg-turbo needs `nasm`; without it the build will fail.
- `shared` on MSVC links against `uhdr` (not `uhdr-static`); keep the DLL in PATH when running binaries.
