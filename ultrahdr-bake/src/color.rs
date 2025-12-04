use bytes::Bytes;
use img_parts::{ImageICC, jpeg::Jpeg};
use ultrahdr::sys;

const MATCH_TOLERANCE: f32 = 0.005;

const PRIMARIES_BT709: [[f32; 2]; 3] = [[0.6400, 0.3300], [0.3000, 0.6000], [0.1500, 0.0600]];
const PRIMARIES_DISPLAY_P3: [[f32; 2]; 3] = [[0.6800, 0.3200], [0.2650, 0.6900], [0.1500, 0.0600]];
const PRIMARIES_BT2100: [[f32; 2]; 3] = [[0.7080, 0.2920], [0.1700, 0.7970], [0.1310, 0.0460]];

/// Best-effort ICC-based color gamut detection for a JPEG.
pub fn detect_icc_color_gamut(bytes: &[u8]) -> Option<sys::uhdr_color_gamut> {
    let icc_bytes = Jpeg::from_bytes(Bytes::copy_from_slice(bytes))
        .ok()?
        .icc_profile()?
        .to_vec();
    let primaries = parse_primaries(&icc_bytes)?;

    if let Some(cg) = match_primaries(&primaries) {
        return Some(cg);
    }

    profile_description(&icc_bytes)
        .as_deref()
        .and_then(match_desc_hint)
}

pub fn gamut_label(cg: sys::uhdr_color_gamut) -> &'static str {
    match cg {
        sys::uhdr_color_gamut::UHDR_CG_BT_709 => "BT.709 / sRGB",
        sys::uhdr_color_gamut::UHDR_CG_BT_2100 => "BT.2100 / Rec.2020",
        sys::uhdr_color_gamut::UHDR_CG_DISPLAY_P3 => "Display P3",
        sys::uhdr_color_gamut::UHDR_CG_UNSPECIFIED => "unspecified",
    }
}

fn parse_primaries(icc: &[u8]) -> Option<[[f32; 2]; 3]> {
    let r = parse_xyz_tag(icc, b"rXYZ")?;
    let g = parse_xyz_tag(icc, b"gXYZ")?;
    let b = parse_xyz_tag(icc, b"bXYZ")?;
    Some([xyz_to_xy(r)?, xyz_to_xy(g)?, xyz_to_xy(b)?])
}

fn match_primaries(primaries: &[[f32; 2]; 3]) -> Option<sys::uhdr_color_gamut> {
    if primaries_close(primaries, &PRIMARIES_DISPLAY_P3) {
        Some(sys::uhdr_color_gamut::UHDR_CG_DISPLAY_P3)
    } else if primaries_close(primaries, &PRIMARIES_BT2100) {
        Some(sys::uhdr_color_gamut::UHDR_CG_BT_2100)
    } else if primaries_close(primaries, &PRIMARIES_BT709) {
        Some(sys::uhdr_color_gamut::UHDR_CG_BT_709)
    } else {
        None
    }
}

fn primaries_close(a: &[[f32; 2]; 3], b: &[[f32; 2]; 3]) -> bool {
    a.iter().zip(b.iter()).all(|(a_p, b_p)| {
        (a_p[0] - b_p[0]).abs() <= MATCH_TOLERANCE && (a_p[1] - b_p[1]).abs() <= MATCH_TOLERANCE
    })
}

fn parse_xyz_tag(icc: &[u8], tag: &[u8; 4]) -> Option<[f32; 3]> {
    let data = tag_data(icc, tag)?;
    if data.len() < 20 || &data[..4] != b"XYZ " {
        return None;
    }
    let x = s15fixed16(&data[8..12])?;
    let y = s15fixed16(&data[12..16])?;
    let z = s15fixed16(&data[16..20])?;
    Some([x, y, z])
}

fn xyz_to_xy(xyz: [f32; 3]) -> Option<[f32; 2]> {
    let sum = xyz[0] + xyz[1] + xyz[2];
    if sum <= f32::EPSILON {
        return None;
    }
    Some([xyz[0] / sum, xyz[1] / sum])
}

fn s15fixed16(bytes: &[u8]) -> Option<f32> {
    let arr: [u8; 4] = bytes.try_into().ok()?;
    Some(i32::from_be_bytes(arr) as f32 / 65536.0)
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

fn match_desc_hint(desc: &str) -> Option<sys::uhdr_color_gamut> {
    let lower = desc.to_ascii_lowercase();
    if lower.contains("p3") {
        Some(sys::uhdr_color_gamut::UHDR_CG_DISPLAY_P3)
    } else if lower.contains("2020") || lower.contains("2100") {
        Some(sys::uhdr_color_gamut::UHDR_CG_BT_2100)
    } else if lower.contains("srgb") || lower.contains("709") {
        Some(sys::uhdr_color_gamut::UHDR_CG_BT_709)
    } else {
        None
    }
}
