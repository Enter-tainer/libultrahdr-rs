# GOAL: pulp SIMD Optimization for apply_gainmap

## Background

libultrahdr-rs decode performance has been optimized from 24.3ms to 4.3ms (with rayon)
at 512x512. Current breakdown (Intel Xeon 8375C, AVX-512):

| Step | Single-threaded | With rayon |
|------|----------------|------------|
| JPEG decode (primary) | 1.3ms | 1.7ms |
| JPEG decode (gainmap) | 0.0ms | 0.1ms |
| apply_gainmap (HLG/PQ) | 13.1ms | 3.2ms |
| apply_gainmap (Linear) | 8.3ms | 2.3ms |
| **Total** | **14.5ms** | **5.0ms** |

apply_gainmap remains the bottleneck (64% of total). The inner loop processes each
pixel through: sRGB inverse LUT (256 entries) -> gain map sampling (bilinear/IDW) ->
gain factor LUT (1024 entries) -> arithmetic (add/mul/sub) -> transfer function LUT
(65536 entries, PQ/HLG only) -> output format conversion (clamp/scale/cast).

## Objective

Use the `pulp` crate to add SIMD vectorization to `apply_gainmap_inner`, processing
multiple pixels in parallel within each row. This complements rayon's row-level
parallelism with pixel-level SIMD.

### Vectorization Strategy

**Batch N pixels per iteration** (N = SIMD width: 8 for AVX2, 16 for AVX-512):

1. **Scalar**: Read N pixels' u8 RGB values, do N*3 sRGB inverse LUT lookups
2. **Scalar**: Do N gain map samples (bilinear/IDW - data-dependent access)
3. **Scalar**: Do N*3 gain factor LUT lookups (1024-entry)
4. **SIMD**: N parallel gain applications: `(color + offset_sdr) * factor - offset_hdr`
5. **Scalar/SIMD**: N*3 transfer function (scalar LUT for PQ/HLG, SIMD clamp for Linear)
6. **SIMD**: N parallel output format conversions (clamp + scale + cast)

LUT lookups remain scalar because pulp does not expose gather instructions.
Arithmetic and format conversion are fully vectorizable.

### Expected Performance

- SIMD-able operations (arithmetic + conversion) are ~30% of per-pixel work
- With 8-16x SIMD speedup on those portions: ~1.3-1.5x overall per-pixel speedup
- Single-threaded: 13.1ms -> ~9-10ms
- With rayon: 3.2ms -> ~2.2-2.5ms
- Conservative estimate; actual gain depends on memory bandwidth saturation

## Constraints

- **Bit-exact output**: SIMD must produce identical results to scalar path
- **Optional feature flag**: `simd` feature in Cargo.toml, pulp as optional dependency
- **No unsafe code**: pulp provides safe SIMD abstractions
- **Scalar fallback**: Non-SIMD path unchanged, guarded by `#[cfg(feature = "simd")]`
- All 101 existing tests must continue to pass
- `cargo clippy --all-targets -- -D warnings` clean
- `cargo fmt --check` clean

## Success Criteria

1. **Feature flag**: `cargo test -p ultrahdr --features simd` compiles and passes all tests
2. **Bit-exact**: SIMD path produces byte-identical output to scalar path for all
   transfer functions (Linear/Srgb/PQ/HLG) and formats (Rgba8888/Rgba1010102/RgbaF16)
3. **Performance**: apply_gainmap at least 1.2x faster single-threaded (measurable via
   decode_profile test), no regression with rayon
4. **CI clean**: clippy + fmt pass with and without `simd` feature
