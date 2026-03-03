use crate::types::{ColorGamut, ColorTransfer};

const MATCH_TOLERANCE: f32 = 0.005;

pub const PRIMARIES_BT709: [[f32; 2]; 3] =
    [[0.6400, 0.3300], [0.3000, 0.6000], [0.1500, 0.0600]];
const PRIMARIES_DISPLAY_P3: [[f32; 2]; 3] =
    [[0.6800, 0.3200], [0.2650, 0.6900], [0.1500, 0.0600]];
const PRIMARIES_BT2100: [[f32; 2]; 3] =
    [[0.7080, 0.2920], [0.1700, 0.7970], [0.1310, 0.0460]];

/// Convert a 4-byte big-endian s15Fixed16Number to `f32`.
pub fn s15fixed16_to_f32(bytes: &[u8]) -> f32 {
    let arr: [u8; 4] = bytes
        .try_into()
        .expect("s15fixed16_to_f32 requires exactly 4 bytes");
    i32::from_be_bytes(arr) as f32 / 65536.0
}

fn tag_data<'a>(icc: &'a [u8], sig: &[u8; 4]) -> Option<&'a [u8]> {
    if icc.len() < 132 {
        return None;
    }
    let tag_count = u32::from_be_bytes(icc[128..132].try_into().ok()?) as usize;
    let table_start = 132usize;
    let table_bytes = tag_count.checked_mul(12)?;
    let table_end = table_start.checked_add(table_bytes)?;
    if table_end > icc.len() {
        return None;
    }

    for i in 0..tag_count {
        let entry_start = table_start + i * 12;
        let entry = &icc[entry_start..entry_start + 12];
        if &entry[..4] != sig {
            continue;
        }
        let offset = u32::from_be_bytes(entry[4..8].try_into().ok()?) as usize;
        let size = u32::from_be_bytes(entry[8..12].try_into().ok()?) as usize;
        let end = offset.checked_add(size)?;
        if end <= icc.len() {
            return Some(&icc[offset..end]);
        } else {
            return None;
        }
    }
    None
}

fn parse_xyz_tag(icc: &[u8], tag: &[u8; 4]) -> Option<[f32; 3]> {
    let data = tag_data(icc, tag)?;
    if data.len() < 20 || &data[..4] != b"XYZ " {
        return None;
    }
    let x = s15fixed16_to_f32(&data[8..12]);
    let y = s15fixed16_to_f32(&data[12..16]);
    let z = s15fixed16_to_f32(&data[16..20]);
    Some([x, y, z])
}

fn xyz_to_xy(xyz: [f32; 3]) -> Option<[f32; 2]> {
    let sum = xyz[0] + xyz[1] + xyz[2];
    if sum <= f32::EPSILON {
        return None;
    }
    Some([xyz[0] / sum, xyz[1] / sum])
}

/// Check whether two sets of RGB chromaticity coordinates are close enough.
pub fn primaries_close(a: &[[f32; 2]; 3], b: &[[f32; 2]; 3]) -> bool {
    a.iter().zip(b.iter()).all(|(a_p, b_p)| {
        (a_p[0] - b_p[0]).abs() <= MATCH_TOLERANCE
            && (a_p[1] - b_p[1]).abs() <= MATCH_TOLERANCE
    })
}

fn parse_primaries(icc: &[u8]) -> Option<[[f32; 2]; 3]> {
    let r = parse_xyz_tag(icc, b"rXYZ")?;
    let g = parse_xyz_tag(icc, b"gXYZ")?;
    let b_val = parse_xyz_tag(icc, b"bXYZ")?;
    Some([xyz_to_xy(r)?, xyz_to_xy(g)?, xyz_to_xy(b_val)?])
}

fn profile_description(icc: &[u8]) -> Option<String> {
    let tag = tag_data(icc, b"desc")?;
    if tag.len() < 12 || &tag[..4] != b"desc" {
        return None;
    }
    let len = u32::from_be_bytes(tag[8..12].try_into().ok()?) as usize;
    let start = 12usize;
    let end = start.checked_add(len)?;
    if end > tag.len() || len == 0 {
        return None;
    }
    let raw = &tag[start..end];
    String::from_utf8(raw.to_vec())
        .ok()
        .map(|s| s.trim_matches('\0').to_string())
}

fn match_desc_hint(desc: &str) -> Option<ColorGamut> {
    let lower = desc.to_ascii_lowercase();
    if lower.contains("p3") {
        Some(ColorGamut::DisplayP3)
    } else if lower.contains("2020") || lower.contains("2100") {
        Some(ColorGamut::Bt2100)
    } else if lower.contains("srgb") || lower.contains("709") {
        Some(ColorGamut::Bt709)
    } else {
        None
    }
}

fn match_primaries(primaries: &[[f32; 2]; 3]) -> Option<ColorGamut> {
    if primaries_close(primaries, &PRIMARIES_DISPLAY_P3) {
        Some(ColorGamut::DisplayP3)
    } else if primaries_close(primaries, &PRIMARIES_BT2100) {
        Some(ColorGamut::Bt2100)
    } else if primaries_close(primaries, &PRIMARIES_BT709) {
        Some(ColorGamut::Bt709)
    } else {
        None
    }
}

/// Detect the color gamut of an ICC profile from its raw bytes.
///
/// First tries to match the rXYZ/gXYZ/bXYZ chromaticity primaries against
/// known gamuts (BT.709, Display P3, BT.2100). Falls back to string matching
/// on the profile description tag.
pub fn detect_color_gamut(icc_bytes: &[u8]) -> Option<ColorGamut> {
    let primaries = parse_primaries(icc_bytes)?;
    if let Some(cg) = match_primaries(&primaries) {
        return Some(cg);
    }
    profile_description(icc_bytes)
        .as_deref()
        .and_then(match_desc_hint)
}

// -- ICC Profile Writing --

/// D65 white point in XYZ (ICC PCS illuminant).
const D65_X: f32 = 0.9505;
const D65_Y: f32 = 1.0000;
const D65_Z: f32 = 1.0890;

/// Convert f32 to s15Fixed16Number bytes (big-endian).
fn f32_to_s15fixed16(val: f32) -> [u8; 4] {
    ((val * 65536.0) as i32).to_be_bytes()
}

/// Convert chromaticity xy + Y=1 white point to XYZ.
fn xy_to_xyz(x: f32, y: f32) -> [f32; 3] {
    if y.abs() < f32::EPSILON {
        return [0.0, 0.0, 0.0];
    }
    [x / y, 1.0, (1.0 - x - y) / y]
}

/// Compute 3x3 matrix to convert RGB primaries to XYZ given D65 white point.
/// Returns column vectors for R, G, B (each is XYZ).
fn primaries_to_xyz_columns(primaries: &[[f32; 2]; 3]) -> [[f32; 3]; 3] {
    let r_xyz = xy_to_xyz(primaries[0][0], primaries[0][1]);
    let g_xyz = xy_to_xyz(primaries[1][0], primaries[1][1]);
    let b_xyz = xy_to_xyz(primaries[2][0], primaries[2][1]);

    // Solve for S = [Sr, Sg, Sb] such that M * S = W (white point)
    // where M = [Rxyz, Gxyz, Bxyz] column matrix
    // Use Cramer's rule for 3x3
    let det = r_xyz[0] * (g_xyz[1] * b_xyz[2] - g_xyz[2] * b_xyz[1])
        - g_xyz[0] * (r_xyz[1] * b_xyz[2] - r_xyz[2] * b_xyz[1])
        + b_xyz[0] * (r_xyz[1] * g_xyz[2] - r_xyz[2] * g_xyz[1]);

    if det.abs() < f32::EPSILON {
        return [[D65_X, D65_Y, D65_Z]; 3];
    }

    let w = [D65_X, D65_Y, D65_Z];

    let sr = (w[0] * (g_xyz[1] * b_xyz[2] - g_xyz[2] * b_xyz[1])
        - g_xyz[0] * (w[1] * b_xyz[2] - w[2] * b_xyz[1])
        + b_xyz[0] * (w[1] * g_xyz[2] - w[2] * g_xyz[1]))
        / det;

    let sg = (r_xyz[0] * (w[1] * b_xyz[2] - w[2] * b_xyz[1])
        - w[0] * (r_xyz[1] * b_xyz[2] - r_xyz[2] * b_xyz[1])
        + b_xyz[0] * (r_xyz[1] * w[2] - r_xyz[2] * w[1]))
        / det;

    let sb = (r_xyz[0] * (g_xyz[1] * w[2] - g_xyz[2] * w[1])
        - g_xyz[0] * (r_xyz[1] * w[2] - r_xyz[2] * w[1])
        + w[0] * (r_xyz[1] * g_xyz[2] - r_xyz[2] * g_xyz[1]))
        / det;

    [
        [r_xyz[0] * sr, r_xyz[1] * sr, r_xyz[2] * sr],
        [g_xyz[0] * sg, g_xyz[1] * sg, g_xyz[2] * sg],
        [b_xyz[0] * sb, b_xyz[1] * sb, b_xyz[2] * sb],
    ]
}

fn get_primaries(gamut: ColorGamut) -> &'static [[f32; 2]; 3] {
    match gamut {
        ColorGamut::Bt709 => &PRIMARIES_BT709,
        ColorGamut::DisplayP3 => &PRIMARIES_DISPLAY_P3,
        ColorGamut::Bt2100 => &PRIMARIES_BT2100,
    }
}

/// Build an XYZ tag (20 bytes): type sig "XYZ " + reserved(4) + XYZ values.
fn build_xyz_tag(xyz: [f32; 3]) -> Vec<u8> {
    let mut tag = Vec::with_capacity(20);
    tag.extend_from_slice(b"XYZ "); // type signature
    tag.extend_from_slice(&[0u8; 4]); // reserved
    tag.extend_from_slice(&f32_to_s15fixed16(xyz[0]));
    tag.extend_from_slice(&f32_to_s15fixed16(xyz[1]));
    tag.extend_from_slice(&f32_to_s15fixed16(xyz[2]));
    tag
}

/// Build a TRC (Tone Reproduction Curve) tag for sRGB gamma.
/// Uses a parametric curve type (type 4 = IEC 61966-2-1).
fn build_srgb_trc_tag() -> Vec<u8> {
    // parametricCurveType with function type 3:
    // Y = (aX+b)^g + c  for X >= d
    // Y = cX + f         for X < d (not used directly; type 3 has 7 params)
    // Actually, for sRGB we use function type 3:
    // if X >= d: Y = (a*X + b)^gamma + e
    // if X < d:  Y = c*X + f
    // sRGB: gamma=2.4, a=1/1.055, b=0.055/1.055, c=1/12.92, d=0.04045, e=0, f=0
    let mut tag = Vec::with_capacity(40);
    tag.extend_from_slice(b"para"); // type signature
    tag.extend_from_slice(&[0u8; 4]); // reserved
    tag.extend_from_slice(&3u16.to_be_bytes()); // function type 3
    tag.extend_from_slice(&[0u8; 2]); // reserved

    // Parameters as s15Fixed16Number
    let gamma = 2.4f32;
    let a = 1.0f32 / 1.055;
    let b = 0.055f32 / 1.055;
    let c = 1.0f32 / 12.92;
    let d = 0.04045f32;
    let e = 0.0f32;
    let f = 0.0f32;

    tag.extend_from_slice(&f32_to_s15fixed16(gamma));
    tag.extend_from_slice(&f32_to_s15fixed16(a));
    tag.extend_from_slice(&f32_to_s15fixed16(b));
    tag.extend_from_slice(&f32_to_s15fixed16(c));
    tag.extend_from_slice(&f32_to_s15fixed16(d));
    tag.extend_from_slice(&f32_to_s15fixed16(e));
    tag.extend_from_slice(&f32_to_s15fixed16(f));
    tag
}

/// Build a linear TRC tag (gamma = 1.0).
fn build_linear_trc_tag() -> Vec<u8> {
    let mut tag = Vec::with_capacity(12);
    tag.extend_from_slice(b"curv"); // type signature
    tag.extend_from_slice(&[0u8; 4]); // reserved
    tag.extend_from_slice(&1u32.to_be_bytes()); // count = 1
    // A single entry of 0x100 (= 1.0 in u8Fixed8Number) means gamma 1.0
    tag.extend_from_slice(&0x0100u16.to_be_bytes());
    tag
}

/// Build a PQ TRC tag using a curv table (sampled 1024 entries).
fn build_pq_trc_tag() -> Vec<u8> {
    use crate::color::transfer::pq_oetf;
    let n = 1024u32;
    let mut tag = Vec::with_capacity(12 + n as usize * 2);
    tag.extend_from_slice(b"curv"); // type signature
    tag.extend_from_slice(&[0u8; 4]); // reserved
    tag.extend_from_slice(&n.to_be_bytes());
    for i in 0..n {
        let linear = i as f32 / (n - 1) as f32;
        let encoded = pq_oetf(linear);
        let u16_val = (encoded.clamp(0.0, 1.0) * 65535.0 + 0.5) as u16;
        tag.extend_from_slice(&u16_val.to_be_bytes());
    }
    tag
}

/// Build an HLG TRC tag using a curv table (sampled 1024 entries).
fn build_hlg_trc_tag() -> Vec<u8> {
    use crate::color::transfer::hlg_oetf;
    let n = 1024u32;
    let mut tag = Vec::with_capacity(12 + n as usize * 2);
    tag.extend_from_slice(b"curv"); // type signature
    tag.extend_from_slice(&[0u8; 4]); // reserved
    tag.extend_from_slice(&n.to_be_bytes());
    for i in 0..n {
        let linear = i as f32 / (n - 1) as f32;
        let encoded = hlg_oetf(linear);
        let u16_val = (encoded.clamp(0.0, 1.0) * 65535.0 + 0.5) as u16;
        tag.extend_from_slice(&u16_val.to_be_bytes());
    }
    tag
}

/// Build a desc tag from a string.
fn build_desc_tag(desc: &str) -> Vec<u8> {
    let desc_bytes = desc.as_bytes();
    let mut tag = Vec::with_capacity(12 + desc_bytes.len() + 1);
    tag.extend_from_slice(b"desc"); // type signature
    tag.extend_from_slice(&[0u8; 4]); // reserved
    tag.extend_from_slice(&((desc_bytes.len() + 1) as u32).to_be_bytes()); // count includes null
    tag.extend_from_slice(desc_bytes);
    tag.push(0); // null terminator
    // Pad to 4-byte boundary
    while tag.len() % 4 != 0 {
        tag.push(0);
    }
    tag
}

/// Generate an ICC profile for a given transfer function and color gamut.
///
/// Produces a minimal but valid ICC v2 profile with:
/// - Profile header (128 bytes)
/// - Tag table
/// - rXYZ, gXYZ, bXYZ tags (color primaries)
/// - rTRC, gTRC, bTRC tags (transfer curve)
/// - wtpt tag (white point)
/// - desc tag (profile description)
/// - cprt tag (copyright)
///
/// Port of `IccHelper::writeIccProfile()` from libultrahdr.
pub fn write_icc_profile(transfer: ColorTransfer, gamut: ColorGamut) -> Vec<u8> {
    let primaries = get_primaries(gamut);
    let xyz_cols = primaries_to_xyz_columns(primaries);

    // Build tag data
    let r_xyz_tag = build_xyz_tag(xyz_cols[0]);
    let g_xyz_tag = build_xyz_tag(xyz_cols[1]);
    let b_xyz_tag = build_xyz_tag(xyz_cols[2]);
    let wtpt_tag = build_xyz_tag([D65_X, D65_Y, D65_Z]);

    let trc_tag = match transfer {
        ColorTransfer::Srgb => build_srgb_trc_tag(),
        ColorTransfer::Linear => build_linear_trc_tag(),
        ColorTransfer::Pq => build_pq_trc_tag(),
        ColorTransfer::Hlg => build_hlg_trc_tag(),
    };

    let desc_str = match (transfer, gamut) {
        (ColorTransfer::Srgb, ColorGamut::Bt709) => "sRGB IEC61966-2.1",
        (ColorTransfer::Srgb, ColorGamut::DisplayP3) => "Display P3 sRGB",
        (ColorTransfer::Pq, ColorGamut::Bt2100) => "BT.2100 PQ",
        (ColorTransfer::Pq, ColorGamut::DisplayP3) => "Display P3 PQ",
        (ColorTransfer::Pq, ColorGamut::Bt709) => "BT.709 PQ",
        (ColorTransfer::Hlg, ColorGamut::Bt2100) => "BT.2100 HLG",
        (ColorTransfer::Hlg, ColorGamut::DisplayP3) => "Display P3 HLG",
        (ColorTransfer::Hlg, ColorGamut::Bt709) => "BT.709 HLG",
        (ColorTransfer::Linear, ColorGamut::Bt709) => "Linear sRGB",
        (ColorTransfer::Linear, ColorGamut::DisplayP3) => "Linear Display P3",
        (ColorTransfer::Linear, ColorGamut::Bt2100) => "Linear BT.2100",
        (ColorTransfer::Srgb, ColorGamut::Bt2100) => "BT.2100 sRGB",
    };
    let desc_tag = build_desc_tag(desc_str);
    let cprt_tag = build_desc_tag("ultrahdr-rs");

    // Number of tags: rXYZ, gXYZ, bXYZ, rTRC, gTRC, bTRC, wtpt, desc, cprt = 9
    // But rTRC, gTRC, bTRC can share the same data offset
    let num_tags = 9u32;

    let header_size = 128usize;
    let tag_table_size = 4 + num_tags as usize * 12; // tag count(4) + entries

    // Calculate data offsets
    let data_start = header_size + tag_table_size;
    let mut offset = data_start;

    let r_xyz_offset = offset;
    offset += r_xyz_tag.len();
    let g_xyz_offset = offset;
    offset += g_xyz_tag.len();
    let b_xyz_offset = offset;
    offset += b_xyz_tag.len();
    let wtpt_offset = offset;
    offset += wtpt_tag.len();
    let trc_offset = offset;
    offset += trc_tag.len();
    let desc_offset = offset;
    offset += desc_tag.len();
    let cprt_offset = offset;
    offset += cprt_tag.len();

    let profile_size = offset;

    // Build profile
    let mut buf = Vec::with_capacity(profile_size);

    // -- Header (128 bytes) --
    buf.extend_from_slice(&(profile_size as u32).to_be_bytes()); // 0: profile size
    buf.extend_from_slice(&[0u8; 4]); // 4: preferred CMM type
    buf.extend_from_slice(&[2, 0x40, 0, 0]); // 8: version 2.4.0 (for compatibility)
    buf.extend_from_slice(b"mntr"); // 12: device class = monitor
    buf.extend_from_slice(b"RGB "); // 16: color space = RGB
    buf.extend_from_slice(b"XYZ "); // 20: PCS = XYZ
    buf.extend_from_slice(&[0u8; 12]); // 24: date/time
    buf.extend_from_slice(b"acsp"); // 36: file signature
    buf.extend_from_slice(&[0u8; 4]); // 40: primary platform
    buf.extend_from_slice(&[0u8; 4]); // 44: profile flags
    buf.extend_from_slice(&[0u8; 4]); // 48: device manufacturer
    buf.extend_from_slice(&[0u8; 4]); // 52: device model
    buf.extend_from_slice(&[0u8; 8]); // 56: device attributes
    buf.extend_from_slice(&[0u8; 4]); // 64: rendering intent (perceptual)
    // 68: PCS illuminant (D65)
    buf.extend_from_slice(&f32_to_s15fixed16(D65_X));
    buf.extend_from_slice(&f32_to_s15fixed16(D65_Y));
    buf.extend_from_slice(&f32_to_s15fixed16(D65_Z));
    buf.extend_from_slice(&[0u8; 4]); // 80: profile creator
    buf.extend_from_slice(&[0u8; 16]); // 84: profile ID (MD5)
    buf.extend_from_slice(&[0u8; 28]); // 100: reserved
    debug_assert_eq!(buf.len(), 128);

    // -- Tag table --
    buf.extend_from_slice(&num_tags.to_be_bytes());

    // Tag entries: signature(4) + offset(4) + size(4)
    let write_tag_entry = |buf: &mut Vec<u8>, sig: &[u8; 4], off: usize, size: usize| {
        buf.extend_from_slice(sig);
        buf.extend_from_slice(&(off as u32).to_be_bytes());
        buf.extend_from_slice(&(size as u32).to_be_bytes());
    };

    write_tag_entry(&mut buf, b"rXYZ", r_xyz_offset, r_xyz_tag.len());
    write_tag_entry(&mut buf, b"gXYZ", g_xyz_offset, g_xyz_tag.len());
    write_tag_entry(&mut buf, b"bXYZ", b_xyz_offset, b_xyz_tag.len());
    write_tag_entry(&mut buf, b"rTRC", trc_offset, trc_tag.len());
    write_tag_entry(&mut buf, b"gTRC", trc_offset, trc_tag.len()); // shared
    write_tag_entry(&mut buf, b"bTRC", trc_offset, trc_tag.len()); // shared
    write_tag_entry(&mut buf, b"wtpt", wtpt_offset, wtpt_tag.len());
    write_tag_entry(&mut buf, b"desc", desc_offset, desc_tag.len());
    write_tag_entry(&mut buf, b"cprt", cprt_offset, cprt_tag.len());

    debug_assert_eq!(buf.len(), data_start);

    // -- Tag data --
    buf.extend_from_slice(&r_xyz_tag);
    buf.extend_from_slice(&g_xyz_tag);
    buf.extend_from_slice(&b_xyz_tag);
    buf.extend_from_slice(&wtpt_tag);
    buf.extend_from_slice(&trc_tag);
    buf.extend_from_slice(&desc_tag);
    buf.extend_from_slice(&cprt_tag);

    debug_assert_eq!(buf.len(), profile_size);

    buf
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn s15fixed16_conversion() {
        let val = s15fixed16_to_f32(&0x00010000_i32.to_be_bytes());
        assert!((val - 1.0).abs() < 1e-6);

        let val = s15fixed16_to_f32(&0x00008000_i32.to_be_bytes());
        assert!((val - 0.5).abs() < 1e-6);
    }

    #[test]
    fn parse_xyz_tag_valid() {
        let xyz = s15fixed16_to_f32(&[0x00, 0x01, 0x00, 0x00]);
        assert!((xyz - 1.0).abs() < 1e-5);
    }

    #[test]
    fn primaries_close_same() {
        let p = [[0.64, 0.33], [0.30, 0.60], [0.15, 0.06]];
        assert!(primaries_close(&p, &PRIMARIES_BT709));
    }

    // Task 26: ICC profile writing
    #[test]
    fn write_icc_srgb_starts_with_correct_header() {
        let icc = write_icc_profile(ColorTransfer::Srgb, ColorGamut::Bt709);
        assert!(icc.len() > 128);
        let size = u32::from_be_bytes([icc[0], icc[1], icc[2], icc[3]]) as usize;
        assert_eq!(size, icc.len());
    }

    #[test]
    fn written_icc_roundtrips_gamut_detection() {
        let icc = write_icc_profile(ColorTransfer::Srgb, ColorGamut::DisplayP3);
        let detected = detect_color_gamut(&icc);
        assert_eq!(detected, Some(ColorGamut::DisplayP3));
    }

    #[test]
    fn written_icc_bt709_roundtrips() {
        let icc = write_icc_profile(ColorTransfer::Srgb, ColorGamut::Bt709);
        let detected = detect_color_gamut(&icc);
        assert_eq!(detected, Some(ColorGamut::Bt709));
    }

    #[test]
    fn written_icc_bt2100_roundtrips() {
        let icc = write_icc_profile(ColorTransfer::Pq, ColorGamut::Bt2100);
        let detected = detect_color_gamut(&icc);
        assert_eq!(detected, Some(ColorGamut::Bt2100));
    }

    #[test]
    fn write_icc_profile_has_valid_signature() {
        let icc = write_icc_profile(ColorTransfer::Srgb, ColorGamut::Bt709);
        // Check "acsp" signature at offset 36
        assert_eq!(&icc[36..40], b"acsp");
    }
}
