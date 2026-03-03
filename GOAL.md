# GOAL: Rust Encoder Bit-Exact with C++ libultrahdr

## Background

The Rust UltraHDR encoder (`ultrahdr/src/encoder.rs`) produces gain map metadata and
JPEG output that differs from the C++ libultrahdr reference implementation. As a result:

1. HDR effect doesn't trigger properly ("亮不起来")
2. C++ decoder cannot decode Rust encoder output (cross-decode fails)
3. Gain map pixel distribution is significantly different

The pure Rust rewrite (PR #6) is functionally complete for decode, but the encoder has
critical metadata and computation differences that must be fixed.

## Objective

Make the Rust encoder produce output that is **functionally equivalent** to the C++
`generateGainMapTwoPass()` code path (BEST_QUALITY preset), so that:

- C++ can successfully decode Rust-encoded UltraHDR JPEGs
- Metadata values (offset, content_boost, use_base_cg) match C++
- Gain map pixel distribution matches C++

## Identified Differences

### P1 — Blocks cross-decode

| Issue | C++ (two-pass) | Rust | Impact |
|-------|----------------|------|--------|
| offset_sdr/hdr | `1e-7` (kSdrOffset/kHdrOffset) | `1/64 = 0.015625` | 156K× wrong offset in decoder formula |
| use_base_cg | `false` (for raw input API-0/API-1) | `true` (hardcoded) | Gamut conversion mismatch |

### P2 — Affects gain map accuracy

| Issue | C++ (two-pass) | Rust | Impact |
|-------|----------------|------|--------|
| min/max gain clamp | `clamp(-14.3, 15.6)` | `max(0).min(headroom.log2())` | Rust min_boost=2.52 vs C++=0.091 |
| Zero-range epsilon | `max += 0.1` | `max += 1e-6` | Minor quantization diff |

### P3 — May affect JPEG structure compatibility

| Issue | C++ | Rust | Impact |
|-------|-----|------|--------|
| Secondary image metadata | Has XMP + ISO in gain map JPEG | May be incomplete | C++ decoder may reject |

## Constraints

- Must match C++ `generateGainMapTwoPass()` path (BEST_QUALITY preset)
- `compute_gain()` formula already correct (log2 with 1e-7 epsilon) — no change needed
- Decoder (`apply_gain_single/multi`) already correct — no change needed
- No new public API changes — only internal encoder behavior fixes
- All existing tests must continue to pass

## Success Criteria

1. **Cross-decode works**: `debug_encode` test shows Rust→C++ decode succeeds (not FAILED)
2. **Metadata matches**: offset_sdr ≈ 1e-7, offset_hdr ≈ 1e-7, use_base_cg = false — all match C++
3. **Gain map distribution**: gradient scenario mean within ±10 of C++ value (167.38)
4. **min_content_boost**: gradient scenario value < 1.0 (matching C++ allowing gain < 1)
5. **All tests pass**: `cargo test` green, `cargo clippy --all-targets -- -D warnings` clean, `cargo fmt --check` clean
