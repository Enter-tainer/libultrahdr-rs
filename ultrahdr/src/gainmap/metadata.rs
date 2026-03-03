use crate::error::{Error, Result};
use crate::types::GainMapMetadata;

/// Gain map metadata in rational fraction representation (ISO 21496-1).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GainMapMetadataFrac {
    pub gain_map_min_n: [i32; 3],
    pub gain_map_min_d: [u32; 3],
    pub gain_map_max_n: [i32; 3],
    pub gain_map_max_d: [u32; 3],
    pub gain_map_gamma_n: [u32; 3],
    pub gain_map_gamma_d: [u32; 3],
    pub base_offset_n: [i32; 3],
    pub base_offset_d: [u32; 3],
    pub alternate_offset_n: [i32; 3],
    pub alternate_offset_d: [u32; 3],
    pub base_hdr_headroom_n: u32,
    pub base_hdr_headroom_d: u32,
    pub alternate_hdr_headroom_n: u32,
    pub alternate_hdr_headroom_d: u32,
    pub backward_direction: bool,
    pub use_base_color_space: bool,
}

const IS_MULTI_CHANNEL_MASK: u8 = 0x80;
const USE_BASE_COLOR_SPACE_MASK: u8 = 0x40;
const BACKWARD_DIRECTION_MASK: u8 = 0x04;
const COMMON_DENOMINATOR_MASK: u8 = 0x08;

impl GainMapMetadataFrac {
    fn all_channels_identical(&self) -> bool {
        let eq3_i = |a: &[i32; 3]| a[0] == a[1] && a[0] == a[2];
        let eq3_u = |a: &[u32; 3]| a[0] == a[1] && a[0] == a[2];
        eq3_i(&self.gain_map_min_n)
            && eq3_u(&self.gain_map_min_d)
            && eq3_i(&self.gain_map_max_n)
            && eq3_u(&self.gain_map_max_d)
            && eq3_u(&self.gain_map_gamma_n)
            && eq3_u(&self.gain_map_gamma_d)
            && eq3_i(&self.base_offset_n)
            && eq3_u(&self.base_offset_d)
            && eq3_i(&self.alternate_offset_n)
            && eq3_u(&self.alternate_offset_d)
    }
}

// -- Stream write helpers --

fn write_u8(buf: &mut Vec<u8>, v: u8) {
    buf.push(v);
}

fn write_u16(buf: &mut Vec<u8>, v: u16) {
    buf.push((v >> 8) as u8);
    buf.push(v as u8);
}

fn write_u32(buf: &mut Vec<u8>, v: u32) {
    buf.extend_from_slice(&v.to_be_bytes());
}

fn write_s32(buf: &mut Vec<u8>, v: i32) {
    buf.extend_from_slice(&v.to_be_bytes());
}

// -- Stream read helpers --

struct Reader<'a> {
    data: &'a [u8],
    pos: usize,
}

impl<'a> Reader<'a> {
    fn new(data: &'a [u8]) -> Self {
        Self { data, pos: 0 }
    }

    fn read_u8(&mut self) -> Result<u8> {
        if self.pos >= self.data.len() {
            return Err(Error::MetadataError(
                "unexpected end of data reading u8".into(),
            ));
        }
        let v = self.data[self.pos];
        self.pos += 1;
        Ok(v)
    }

    fn read_u16(&mut self) -> Result<u16> {
        if self.pos + 1 >= self.data.len() {
            return Err(Error::MetadataError(
                "unexpected end of data reading u16".into(),
            ));
        }
        let v = u16::from_be_bytes([self.data[self.pos], self.data[self.pos + 1]]);
        self.pos += 2;
        Ok(v)
    }

    fn read_u32(&mut self) -> Result<u32> {
        if self.pos + 3 >= self.data.len() {
            return Err(Error::MetadataError(
                "unexpected end of data reading u32".into(),
            ));
        }
        let v = u32::from_be_bytes([
            self.data[self.pos],
            self.data[self.pos + 1],
            self.data[self.pos + 2],
            self.data[self.pos + 3],
        ]);
        self.pos += 4;
        Ok(v)
    }

    fn read_s32(&mut self) -> Result<i32> {
        if self.pos + 3 >= self.data.len() {
            return Err(Error::MetadataError(
                "unexpected end of data reading s32".into(),
            ));
        }
        let v = i32::from_be_bytes([
            self.data[self.pos],
            self.data[self.pos + 1],
            self.data[self.pos + 2],
            self.data[self.pos + 3],
        ]);
        self.pos += 4;
        Ok(v)
    }
}

/// Encode gain map metadata to ISO 21496-1 binary format.
pub fn encode_gainmap_metadata(frac: &GainMapMetadataFrac) -> Result<Vec<u8>> {
    let mut buf = Vec::new();

    // Version fields
    let min_version: u16 = 0;
    let writer_version: u16 = 0;
    write_u16(&mut buf, min_version);
    write_u16(&mut buf, writer_version);

    // Flags
    let channel_count: u8 = if frac.all_channels_identical() { 1 } else { 3 };
    let mut flags: u8 = 0;
    if channel_count == 3 {
        flags |= IS_MULTI_CHANNEL_MASK;
    }
    if frac.use_base_color_space {
        flags |= USE_BASE_COLOR_SPACE_MASK;
    }
    if frac.backward_direction {
        flags |= BACKWARD_DIRECTION_MASK;
    }

    // Check common denominator optimization
    let denom = frac.base_hdr_headroom_d;
    let mut use_common = frac.alternate_hdr_headroom_d == denom;
    for c in 0..channel_count as usize {
        if frac.gain_map_min_d[c] != denom
            || frac.gain_map_max_d[c] != denom
            || frac.gain_map_gamma_d[c] != denom
            || frac.base_offset_d[c] != denom
            || frac.alternate_offset_d[c] != denom
        {
            use_common = false;
        }
    }
    if use_common {
        flags |= COMMON_DENOMINATOR_MASK;
    }
    write_u8(&mut buf, flags);

    if use_common {
        write_u32(&mut buf, denom);
        write_u32(&mut buf, frac.base_hdr_headroom_n);
        write_u32(&mut buf, frac.alternate_hdr_headroom_n);
        for c in 0..channel_count as usize {
            write_s32(&mut buf, frac.gain_map_min_n[c]);
            write_s32(&mut buf, frac.gain_map_max_n[c]);
            write_u32(&mut buf, frac.gain_map_gamma_n[c]);
            write_s32(&mut buf, frac.base_offset_n[c]);
            write_s32(&mut buf, frac.alternate_offset_n[c]);
        }
    } else {
        write_u32(&mut buf, frac.base_hdr_headroom_n);
        write_u32(&mut buf, frac.base_hdr_headroom_d);
        write_u32(&mut buf, frac.alternate_hdr_headroom_n);
        write_u32(&mut buf, frac.alternate_hdr_headroom_d);
        for c in 0..channel_count as usize {
            write_s32(&mut buf, frac.gain_map_min_n[c]);
            write_u32(&mut buf, frac.gain_map_min_d[c]);
            write_s32(&mut buf, frac.gain_map_max_n[c]);
            write_u32(&mut buf, frac.gain_map_max_d[c]);
            write_u32(&mut buf, frac.gain_map_gamma_n[c]);
            write_u32(&mut buf, frac.gain_map_gamma_d[c]);
            write_s32(&mut buf, frac.base_offset_n[c]);
            write_u32(&mut buf, frac.base_offset_d[c]);
            write_s32(&mut buf, frac.alternate_offset_n[c]);
            write_u32(&mut buf, frac.alternate_offset_d[c]);
        }
    }

    Ok(buf)
}

/// Decode gain map metadata from ISO 21496-1 binary format.
pub fn decode_gainmap_metadata(data: &[u8]) -> Result<GainMapMetadataFrac> {
    let mut r = Reader::new(data);

    let min_version = r.read_u16()?;
    if min_version != 0 {
        return Err(Error::MetadataError(format!(
            "unsupported minimum version {min_version}, expected 0"
        )));
    }
    let _writer_version = r.read_u16()?;

    let flags = r.read_u8()?;
    let channel_count = if flags & IS_MULTI_CHANNEL_MASK != 0 {
        3usize
    } else {
        1usize
    };
    let use_base_color_space = flags & USE_BASE_COLOR_SPACE_MASK != 0;
    let backward_direction = flags & BACKWARD_DIRECTION_MASK != 0;
    let use_common = flags & COMMON_DENOMINATOR_MASK != 0;

    let mut out = GainMapMetadataFrac {
        gain_map_min_n: [0; 3],
        gain_map_min_d: [1; 3],
        gain_map_max_n: [0; 3],
        gain_map_max_d: [1; 3],
        gain_map_gamma_n: [1; 3],
        gain_map_gamma_d: [1; 3],
        base_offset_n: [0; 3],
        base_offset_d: [1; 3],
        alternate_offset_n: [0; 3],
        alternate_offset_d: [1; 3],
        base_hdr_headroom_n: 0,
        base_hdr_headroom_d: 1,
        alternate_hdr_headroom_n: 0,
        alternate_hdr_headroom_d: 1,
        backward_direction,
        use_base_color_space,
    };

    if use_common {
        let common_denom = r.read_u32()?;
        out.base_hdr_headroom_n = r.read_u32()?;
        out.base_hdr_headroom_d = common_denom;
        out.alternate_hdr_headroom_n = r.read_u32()?;
        out.alternate_hdr_headroom_d = common_denom;
        for c in 0..channel_count {
            out.gain_map_min_n[c] = r.read_s32()?;
            out.gain_map_min_d[c] = common_denom;
            out.gain_map_max_n[c] = r.read_s32()?;
            out.gain_map_max_d[c] = common_denom;
            out.gain_map_gamma_n[c] = r.read_u32()?;
            out.gain_map_gamma_d[c] = common_denom;
            out.base_offset_n[c] = r.read_s32()?;
            out.base_offset_d[c] = common_denom;
            out.alternate_offset_n[c] = r.read_s32()?;
            out.alternate_offset_d[c] = common_denom;
        }
    } else {
        out.base_hdr_headroom_n = r.read_u32()?;
        out.base_hdr_headroom_d = r.read_u32()?;
        out.alternate_hdr_headroom_n = r.read_u32()?;
        out.alternate_hdr_headroom_d = r.read_u32()?;
        for c in 0..channel_count {
            out.gain_map_min_n[c] = r.read_s32()?;
            out.gain_map_min_d[c] = r.read_u32()?;
            out.gain_map_max_n[c] = r.read_s32()?;
            out.gain_map_max_d[c] = r.read_u32()?;
            out.gain_map_gamma_n[c] = r.read_u32()?;
            out.gain_map_gamma_d[c] = r.read_u32()?;
            out.base_offset_n[c] = r.read_s32()?;
            out.base_offset_d[c] = r.read_u32()?;
            out.alternate_offset_n[c] = r.read_s32()?;
            out.alternate_offset_d[c] = r.read_u32()?;
        }
    }

    // Fill remaining channels by copying channel 0.
    for c in channel_count..3 {
        out.gain_map_min_n[c] = out.gain_map_min_n[0];
        out.gain_map_min_d[c] = out.gain_map_min_d[0];
        out.gain_map_max_n[c] = out.gain_map_max_n[0];
        out.gain_map_max_d[c] = out.gain_map_max_d[0];
        out.gain_map_gamma_n[c] = out.gain_map_gamma_n[0];
        out.gain_map_gamma_d[c] = out.gain_map_gamma_d[0];
        out.base_offset_n[c] = out.base_offset_n[0];
        out.base_offset_d[c] = out.base_offset_d[0];
        out.alternate_offset_n[c] = out.alternate_offset_n[0];
        out.alternate_offset_d[c] = out.alternate_offset_d[0];
    }

    Ok(out)
}

/// Convert fraction-based metadata to float-based `GainMapMetadata`.
///
/// Conversion formulas (ISO 21496-1):
/// - `max_content_boost[i] = 2^(gain_map_max_n[i] / gain_map_max_d[i])`
/// - `min_content_boost[i] = 2^(gain_map_min_n[i] / gain_map_min_d[i])`
/// - `gamma[i] = gain_map_gamma_n[i] / gain_map_gamma_d[i]`
/// - `offset_sdr[i] = base_offset_n[i] / base_offset_d[i]`
/// - `offset_hdr[i] = alternate_offset_n[i] / alternate_offset_d[i]`
/// - `hdr_capacity_max = 2^(alternate_hdr_headroom_n / alternate_hdr_headroom_d)`
/// - `hdr_capacity_min = 2^(base_hdr_headroom_n / base_hdr_headroom_d)`
pub fn fraction_to_float(frac: &GainMapMetadataFrac) -> Result<GainMapMetadata> {
    // Validate denominators are non-zero
    if frac.base_hdr_headroom_d == 0 || frac.alternate_hdr_headroom_d == 0 {
        return Err(Error::MetadataError("headroom denominator is zero".into()));
    }
    for i in 0..3 {
        if frac.gain_map_max_d[i] == 0
            || frac.gain_map_min_d[i] == 0
            || frac.gain_map_gamma_d[i] == 0
            || frac.base_offset_d[i] == 0
            || frac.alternate_offset_d[i] == 0
        {
            return Err(Error::MetadataError("channel denominator is zero".into()));
        }
    }

    let mut max_content_boost = [0.0f32; 3];
    let mut min_content_boost = [0.0f32; 3];
    let mut gamma = [0.0f32; 3];
    let mut offset_sdr = [0.0f32; 3];
    let mut offset_hdr = [0.0f32; 3];

    for i in 0..3 {
        max_content_boost[i] =
            (2.0f32).powf(frac.gain_map_max_n[i] as f32 / frac.gain_map_max_d[i] as f32);
        min_content_boost[i] =
            (2.0f32).powf(frac.gain_map_min_n[i] as f32 / frac.gain_map_min_d[i] as f32);
        gamma[i] = frac.gain_map_gamma_n[i] as f32 / frac.gain_map_gamma_d[i] as f32;
        offset_sdr[i] = frac.base_offset_n[i] as f32 / frac.base_offset_d[i] as f32;
        offset_hdr[i] = frac.alternate_offset_n[i] as f32 / frac.alternate_offset_d[i] as f32;
    }

    let hdr_capacity_max =
        (2.0f32).powf(frac.alternate_hdr_headroom_n as f32 / frac.alternate_hdr_headroom_d as f32);
    let hdr_capacity_min =
        (2.0f32).powf(frac.base_hdr_headroom_n as f32 / frac.base_hdr_headroom_d as f32);

    Ok(GainMapMetadata {
        max_content_boost,
        min_content_boost,
        gamma,
        offset_sdr,
        offset_hdr,
        hdr_capacity_min,
        hdr_capacity_max,
        use_base_cg: frac.use_base_color_space,
    })
}

// -- XMP metadata --

const HDRGM_NS: &str = "http://ns.adobe.com/hdr-gain-map/1.0/";

/// Write gain map metadata as XMP bytes (secondary image XMP).
pub fn write_xmp_gainmap_metadata(meta: &GainMapMetadata) -> Result<Vec<u8>> {
    use std::fmt::Write;
    let mut s = String::new();
    writeln!(s, "<x:xmpmeta xmlns:x=\"adobe:ns:meta/\">").unwrap();
    writeln!(
        s,
        " <rdf:RDF xmlns:rdf=\"http://www.w3.org/1999/02/22-rdf-syntax-ns#\">"
    )
    .unwrap();
    writeln!(s, "  <rdf:Description xmlns:hdrgm=\"{HDRGM_NS}\"").unwrap();
    writeln!(s, "   hdrgm:Version=\"1.0\"").unwrap();
    writeln!(
        s,
        "   hdrgm:GainMapMin=\"{}\"",
        meta.min_content_boost[0].log2()
    )
    .unwrap();
    writeln!(
        s,
        "   hdrgm:GainMapMax=\"{}\"",
        meta.max_content_boost[0].log2()
    )
    .unwrap();
    writeln!(s, "   hdrgm:Gamma=\"{}\"", meta.gamma[0]).unwrap();
    writeln!(s, "   hdrgm:OffsetSDR=\"{}\"", meta.offset_sdr[0]).unwrap();
    writeln!(s, "   hdrgm:OffsetHDR=\"{}\"", meta.offset_hdr[0]).unwrap();
    writeln!(
        s,
        "   hdrgm:HDRCapacityMin=\"{}\"",
        meta.hdr_capacity_min.log2()
    )
    .unwrap();
    writeln!(
        s,
        "   hdrgm:HDRCapacityMax=\"{}\"",
        meta.hdr_capacity_max.log2()
    )
    .unwrap();
    if !meta.use_base_cg {
        writeln!(s, "   hdrgm:BaseColorSpace=\"0\"").unwrap();
    }
    writeln!(s, "   hdrgm:BaseRenditionIsHDR=\"False\"/>").unwrap();
    writeln!(s, " </rdf:RDF>").unwrap();
    write!(s, "</x:xmpmeta>").unwrap();
    Ok(s.into_bytes())
}

/// Parse gain map metadata from XMP bytes.
pub fn parse_xmp_gainmap_metadata(data: &[u8]) -> Result<GainMapMetadata> {
    let text = std::str::from_utf8(data)
        .map_err(|e| Error::MetadataError(format!("XMP is not valid UTF-8: {e}")))?;
    let doc = roxmltree::Document::parse(text)
        .map_err(|e| Error::MetadataError(format!("XMP parse error: {e}")))?;

    // Find the rdf:Description element that has hdrgm attributes.
    let desc = doc
        .descendants()
        .find(|n| {
            n.tag_name().name() == "Description"
                && (n.has_attribute(("http://ns.adobe.com/hdr-gain-map/1.0/", "Version"))
                    || n.has_attribute("hdrgm:Version"))
        })
        .ok_or_else(|| Error::MetadataError("no hdrgm rdf:Description found".into()))?;

    let get_attr = |local_name: &str| -> Option<&str> { desc.attribute((HDRGM_NS, local_name)) };

    let parse_f32 = |local_name: &str| -> Result<f32> {
        let val = get_attr(local_name)
            .ok_or_else(|| Error::MetadataError(format!("missing attribute hdrgm:{local_name}")))?;
        val.parse::<f32>()
            .map_err(|e| Error::MetadataError(format!("invalid float for hdrgm:{local_name}: {e}")))
    };

    let parse_f32_or = |local_name: &str, default: f32| -> Result<f32> {
        match get_attr(local_name) {
            Some(val) => val.parse::<f32>().map_err(|e| {
                Error::MetadataError(format!("invalid float for hdrgm:{local_name}: {e}"))
            }),
            None => Ok(default),
        }
    };

    // Required fields
    let gain_map_max_log2 = parse_f32("GainMapMax")?;
    let hdr_capacity_max_log2 = parse_f32("HDRCapacityMax")?;

    // Optional fields with defaults
    let gain_map_min_log2 = parse_f32_or("GainMapMin", 0.0)?;
    let gamma = parse_f32_or("Gamma", 1.0)?;
    let offset_sdr = parse_f32_or("OffsetSDR", 1.0 / 64.0)?;
    let offset_hdr = parse_f32_or("OffsetHDR", 1.0 / 64.0)?;
    let hdr_capacity_min_log2 = parse_f32_or("HDRCapacityMin", 0.0)?;

    let max_content_boost = (2.0f32).powf(gain_map_max_log2);
    let min_content_boost = (2.0f32).powf(gain_map_min_log2);
    let hdr_capacity_max = (2.0f32).powf(hdr_capacity_max_log2);
    let hdr_capacity_min = (2.0f32).powf(hdr_capacity_min_log2);

    // If XMP contains BaseColorSpace="0", use_base_cg is false.
    // If absent, default to true (base color space is used).
    let use_base_cg = match get_attr("BaseColorSpace") {
        Some(val) => val != "0",
        None => true,
    };

    Ok(GainMapMetadata {
        max_content_boost: [max_content_boost; 3],
        min_content_boost: [min_content_boost; 3],
        gamma: [gamma; 3],
        offset_sdr: [offset_sdr; 3],
        offset_hdr: [offset_hdr; 3],
        hdr_capacity_min,
        hdr_capacity_max,
        use_base_cg,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encode_decode_roundtrip_single_channel() {
        let frac = GainMapMetadataFrac {
            gain_map_min_n: [0; 3],
            gain_map_min_d: [1; 3],
            gain_map_max_n: [100; 3],
            gain_map_max_d: [1; 3],
            gain_map_gamma_n: [1; 3],
            gain_map_gamma_d: [1; 3],
            base_offset_n: [0; 3],
            base_offset_d: [1; 3],
            alternate_offset_n: [0; 3],
            alternate_offset_d: [1; 3],
            base_hdr_headroom_n: 0,
            base_hdr_headroom_d: 1,
            alternate_hdr_headroom_n: 2,
            alternate_hdr_headroom_d: 1,
            backward_direction: false,
            use_base_color_space: true,
        };
        let encoded = encode_gainmap_metadata(&frac).unwrap();
        let decoded = decode_gainmap_metadata(&encoded).unwrap();
        assert_eq!(frac.gain_map_min_n, decoded.gain_map_min_n);
        assert_eq!(
            frac.alternate_hdr_headroom_n,
            decoded.alternate_hdr_headroom_n
        );
        assert_eq!(frac.use_base_color_space, decoded.use_base_color_space);
    }

    #[test]
    fn fraction_to_float_conversion() {
        let frac = GainMapMetadataFrac {
            gain_map_min_n: [0; 3],
            gain_map_min_d: [1; 3],
            gain_map_max_n: [2; 3],
            gain_map_max_d: [1; 3],
            gain_map_gamma_n: [1; 3],
            gain_map_gamma_d: [1; 3],
            base_offset_n: [0; 3],
            base_offset_d: [1; 3],
            alternate_offset_n: [0; 3],
            alternate_offset_d: [1; 3],
            base_hdr_headroom_n: 0,
            base_hdr_headroom_d: 1,
            alternate_hdr_headroom_n: 2,
            alternate_hdr_headroom_d: 1,
            backward_direction: false,
            use_base_color_space: false,
        };
        let float_meta = fraction_to_float(&frac).unwrap();
        assert!((float_meta.max_content_boost[0] - 4.0).abs() < 0.001);
        assert!((float_meta.hdr_capacity_max - 4.0).abs() < 0.001);
    }

    #[test]
    fn xmp_write_read_roundtrip() {
        let meta = GainMapMetadata {
            max_content_boost: [4.0; 3],
            min_content_boost: [1.0; 3],
            gamma: [1.0; 3],
            offset_sdr: [0.015625; 3],
            offset_hdr: [0.015625; 3],
            hdr_capacity_min: 1.0,
            hdr_capacity_max: 4.0,
            use_base_cg: false,
        };
        let xmp_bytes = write_xmp_gainmap_metadata(&meta).unwrap();
        let xmp_str = std::str::from_utf8(&xmp_bytes).unwrap();
        assert!(xmp_str.contains("hdrgm:Version"));
        let parsed = parse_xmp_gainmap_metadata(&xmp_bytes).unwrap();
        assert!((parsed.max_content_boost[0] - 4.0).abs() < 0.01);
    }
}
