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
- WASM/WASI: target `wasm32-wasip1` with wasi-sdk (toolchain file at `/opt/wasi-sdk/share/cmake/wasi-sdk-p1.cmake`). `cargo build --target wasm32-wasip1 -p ultrahdr-bake --release` works with vendored deps after cloning `third_party/turbojpeg`.
- WASM/WASI with container codecs: `--features heif` is the only switch (libultrahdr's `UHDR_ENABLE_HEIF` covers HEIF and AVIF together). On wasm it clones and cross-compiles libaom (pin in `WASM_AOM_VERSION`, override the checkout with an **absolute** `ULTRAHDR_AOM_SRC`) into `OUT_DIR/aom-wasm-prefix` and points libheif's `find_package(AOM)` at it, so AVIF has a real codec. Expect ~+4.3 MB of wasm and several minutes on a cold build; HEVC has no WASI port, so a wasm `heif` build warns and provides AVIF only.
- Run either wasm build via wasmtime with import stubbing, e.g. `XDG_CACHE_HOME=$(pwd)/.cache wasmtime -W unknown-imports-default=yes --dir=. target/wasm32-wasip1/release/ultrahdr-bake.wasm --help`.
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
- wasi-sdk ≥ 22 ships libc++ in `eh`/`noeh` subdirectories and defaults C++ to wasm exception handling; `build.rs` therefore compiles the wasm C++ side with `-fno-exceptions` and links the `noeh` variant, because the `exnref`/`try_table` instructions it emits otherwise fail to instantiate in browsers (`browser_wasi_shim`-based `ultrahdr-browser` demo).
- `--features heif` on `wasm32-wasip1`: libheif's CMake would otherwise find the **build host** codecs (x265/aom/dav1d/…) and leak `/usr/include` into the cross compile (`gnu/stubs-32.h` not found). `build.rs` therefore appends a WASI-scoped hunk to the vendored libheif patch that forces `WITH_<codec>=OFF` while keeping AOM, which is the cross-compiled libaom (see below); a heif-enabled wasm also imports `dlopen`/`dlsym`/`dlclose`/`dlerror`, which `ultrahdr-browser`'s worker stubs out. HEIC is unavailable in that configuration, and `ultrahdr-bake --format heif` says so instead of surfacing libheif's `Unsupported file-type`.
- `--features heif` on native targets: libheif probes the host, so `build.rs` probes the same way first (`pkg-config --exists`, honoring `PKG_CONFIG`, `PKG_CONFIG_PATH`, `PKG_CONFIG_LIBDIR` and `<PKG>_NO_PKG_CONFIG`) and fails with `cargo::error` if it finds no codec at all — a libheif with zero codecs can only parse containers, which used to happen silently. Otherwise it warns with the codec matrix it found. Cross-target builds and `DOCS_RS` builds skip that check and stay with libheif's own probe. CI installs `libaom-dev libx265-dev libde265-dev` for the all-features job.
- Cross-compiling libaom for wasm needs three things to be reproducible: `AOM_TARGET_CPU=generic` (no wasm SIMD port), `CONFIG_MULTITHREAD=0`, and `-DCMAKE_C_FLAGS=-mllvm -wasm-enable-sjlj` — wasi-sdk ≥ 22's `<setjmp.h>` refuses to compile without SJLJ emulation, and libaom uses `setjmp`/`longjmp` for error recovery. SJLJ lowers to the `env.setjmp`/`env.longjmp` imports that wasi-libc already produced, which the browser worker provides.
- HEIF/AVIF output requires **raw intents**: upstream rejects a compressed base (`heif/avif encoding is supported only with raw intents`), and libultrahdr's decoder only accepts JPEGs that carry a gain map, so `ultrahdr-bake` decodes the SDR base with `zune-jpeg` (optional dependency behind `heif`).
- Upstream corrupts the heap when a HEIF/AVIF stream carries a **multi-channel gain map with an odd height** (`map_height = image_height / scale`): it overruns the YCbCr plane (`malloc(): invalid size`, SIGABRT; odd widths and even heights are fine, and JPEG output is unaffected). `Encoder::encode` rejects the combination and `ultrahdr-bake` falls back to a single-channel gain map with a warning.
