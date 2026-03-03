# GOAL: Bit-Exact Tests + Decode Performance Optimization

## Background

The Rust UltraHDR encoder is now metadata-aligned with C++ libultrahdr (LUT-based inverse
OETF, correct offsets, gain clamping, etc.). Current status:
- Encoder metadata: bit-exact match with C++ on gradient/white scenarios
- Cross-decode: works (Rust→C++ and C++→Rust)
- Encode performance: on par with C++ (0.98x at 512x512)
- Decode performance: 2.4x slower than C++ (24.3ms vs 10.0ms at 512x512)

Existing tests use PSNR thresholds (20-55 dB) against golden data files, but do NOT
verify strict bit-exact match by calling both Rust and C++ at runtime.

## Objective

### Part 1: Strict Bit-Exact Test Suite

Add integration tests that call both Rust and C++ (via ultrahdr-sys FFI) at runtime
and compare output pixel-by-pixel:

**Encoder bit-exact tests:**
- Same synthetic input → encode with Rust and C++ → compare:
  - Gain map metadata values (exact float match)
  - Gain map pixel bytes (extract + decompress, byte-by-byte)
- Scenarios: gradient, solid white, solid black, color ramps, mixed bright/dark

**Decoder bit-exact tests:**
- Same UltraHDR JPEG → decode with Rust and C++ → compare decoded pixels byte-by-byte
- Test with both Rust-encoded and C++-encoded JPEGs
- Output formats: RGBA1010102 (HLG), RGBA1010102 (PQ), RgbaF16 (Linear)
- Report max pixel diff and count of differing pixels

### Part 2: Decode Performance Optimization

Optimize Rust decode to match C++ speed (~10ms at 512x512) while maintaining bit-exact
output. Primary bottleneck is likely JPEG decode (`jpeg-decoder` vs `libjpeg-turbo`).

Candidate optimizations (in priority order):
1. Replace `jpeg-decoder` with faster alternative (e.g. `zune-jpeg` feature flag)
2. Eliminate `rgb_to_rgba` allocation (decode directly to RGBA or use in-place)
3. Optimize `apply_gainmap_to_sdr` hot loops (SIMD-friendly patterns, reduce branching)
4. Enable `rayon` parallelism for gain map application

## Constraints

- All existing 78+ tests must continue to pass
- No new unsafe code in the main library
- Performance optimizations must not change output (bit-exact preservation)
- New faster JPEG decoder should be behind an optional feature flag
- `cargo clippy --all-targets -- -D warnings` clean
- `cargo fmt --check` clean

## Success Criteria

1. **Encoder bit-exact**: At least 5 scenarios where Rust and C++ gain map metadata
   match exactly (float equality) and gain map pixels match byte-for-byte
2. **Decoder bit-exact**: For each tested JPEG, Rust and C++ decoded pixels are
   identical (0 differing pixels) across HLG, PQ, and Linear output modes
3. **Decode performance**: Rust decode within 1.5x of C++ speed (target: <15ms at 512x512)
4. **All tests pass**: `cargo test` green (including new bit-exact tests)
5. **CI clean**: clippy + fmt pass
