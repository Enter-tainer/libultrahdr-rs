# GOAL: Byte-Exact Metadata Between Rust and C++ Encoder

## Background

The Rust libultrahdr encoder produces UltraHDR JPEGs that are functionally correct
(pixel PSNR > 30 dB vs C++, tests pass), but metadata segments are not byte-identical
to C++ libultrahdr output. This causes some HDR viewers to fail to "light up" the image,
as they may rely on exact metadata format/values.

Previous work (already completed):
- offset_sdr/hdr: fixed from 1/64 to 1e-7 (matching C++ kSdrOffset/kHdrOffset)
- use_base_cg: fixed from true to false (matching C++ API-0 raw input)
- gain clamp: fixed from max(0).min(headroom) to clamp(-14.3, 15.6)
- epsilon: fixed from 1e-6 to 0.1 (matching C++ FLT_EPSILON behavior)
- pixel sampling: fixed to block-average in gamma space (matching C++ samplePixels)

## Objective

Make Rust encoder's metadata segments byte-identical to C++ libultrahdr for the same
input. "Metadata segments" means:

1. **XMP APP1**: The gain map XMP in the primary image
2. **ISO APP2 (primary)**: Version stub in primary image
3. **ISO APP2 (secondary)**: Full ISO 21496-1 binary metadata in gain map image
4. **MPF APP2**: Multi-Picture Format linking primary and secondary images

## Known Remaining Differences

### 1. metadata_to_frac truncation vs rounding
Rust uses `as i32`/`as u32` (truncation toward zero), C++ uses `roundf()`.
Affects all fraction fields: gain_map_min/max_n, gamma_n, offset_n, headroom_n.
```
Example: log2(7.88) = 2.9779
Rust: (2.9779 * 10000) as u32 = 29779  (truncation)
C++:  roundf(2.9779 * 10000) = 29780   (rounding)
```

### 2. XMP format
Rust generates its own XMP XML format. C++ uses a specific template with potentially
different attribute ordering, namespace declarations, precision, and whitespace.

### 3. JPEG segment ordering
The order of APP0/APP1/APP2 segments in the assembled JPEG may differ from C++.

### 4. MPF byte layout
MPF data structure should match but needs byte-level verification.

## Constraints

- All existing tests (105 with simd, 101 without) must continue to pass
- Bitexact tests in `tests/bitexact.rs` must pass (encoder + decoder)
- `cargo clippy --all-targets -- -D warnings` clean
- `cargo fmt --check` clean
- Changes limited to metadata encoding/assembly; no pixel processing changes

## Success Criteria

1. **ISO binary metadata**: For uniform-RGB synthetic scenes (gradient, white, black,
   mixed), `encode_gainmap_metadata()` output is byte-identical between Rust and C++
2. **XMP metadata**: XMP bytes in primary image match C++ output byte-for-byte
3. **MPF segment**: MPF APP2 data matches C++ output byte-for-byte
4. **Bitexact test**: New test comparing full metadata segments (XMP + ISO + MPF)
   between Rust and C++ encoder outputs, asserting zero diff
5. **Cross-decode**: C++ decoder successfully decodes Rust-encoded UltraHDR JPEGs
   with correct HDR rendering (existing bitexact decoder tests pass)
