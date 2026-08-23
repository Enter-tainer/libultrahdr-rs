use std::env;

/// Emit `cargo:rustc-link-arg=--allow-undefined` so wasm32-wasip1 binaries link.
///
/// libultrahdr's jpeg helpers (`jpegdecoderhelper.cpp`, `jpegencoderhelper.cpp`)
/// are C++ and use `setjmp`/`longjmp` for JPEG error handling. On wasm32-wasip1
/// `<setjmp.h>` declares these as plain functions, but no wasi-sysroot library
/// defines the bare `setjmp`/`longjmp` symbols (only `__wasm_setjmp` /
/// `__wasm_longjmp` exist in `libsetjmp.a`, which cannot satisfy their calls).
/// They therefore must remain *imports* resolved by the embedding host: the
/// browser demo provides stubs in `ultrahdr-browser/src/worker.ts` and the
/// wasmtime runner stubs them via `-W unknown-imports-default=yes`.
///
/// Older rustc passed `--allow-undefined` for wasm32-wasip1 by default, so the
/// project built with those symbols left as imports. rustc 1.98.0 stopped doing
/// so, turning them into hard `undefined symbol` link errors. This build script
/// (on the final `ultrahdr-bake` binary, where `rustc-link-arg` reaches the wasm
/// link line — unlike `ultrahdr-sys`'s rlib-only directives) restores the
/// intended behaviour.
fn main() {
    let target = env::var("TARGET").unwrap_or_default();
    if !target.starts_with("wasm32-wasi") {
        return;
    }
    println!("cargo:rustc-link-arg=--allow-undefined");
}
