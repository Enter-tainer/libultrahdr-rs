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

// -- Float to fraction (continued fractions algorithm) --

/// Convert a float to an unsigned fraction using continued fractions algorithm.
/// Port of C++ `floatToUnsignedFractionImpl()` (gainmapmath.cpp:1626-1674).
fn float_to_unsigned_fraction_impl(v: f32, max_numerator: u32) -> Option<(u32, u32)> {
    if v.is_nan() || v < 0.0 || v > max_numerator as f32 {
        return None;
    }
    let max_d: u64 = if v <= 1.0 {
        u32::MAX as u64
    } else {
        (max_numerator as f64 / v as f64).floor() as u64
    };

    let mut denominator: u32 = 1;
    let mut previous_d: u32 = 0;
    let mut current_v: f64 = (v as f64) - (v as f64).floor();
    let max_iter = 39;

    for _ in 0..max_iter {
        let numerator_double: f64 = (denominator as f64) * (v as f64);
        if numerator_double > max_numerator as f64 {
            return None;
        }
        let numerator = numerator_double.round() as u32;
        if (numerator_double - numerator as f64).abs() == 0.0 {
            return Some((numerator, denominator));
        }
        current_v = 1.0 / current_v;
        let new_d: f64 = previous_d as f64 + current_v.floor() * denominator as f64;
        if new_d > max_d as f64 {
            return Some((numerator, denominator));
        }
        previous_d = denominator;
        if new_d > u32::MAX as f64 {
            return None;
        }
        denominator = new_d as u32;
        current_v -= current_v.floor();
    }
    let numerator = ((denominator as f64) * (v as f64)).round() as u32;
    Some((numerator, denominator))
}

/// Convert a float to a signed fraction using continued fractions.
/// Port of C++ `floatToSignedFraction()`.
pub fn float_to_signed_fraction(v: f32) -> Option<(i32, u32)> {
    let (num, den) = float_to_unsigned_fraction_impl(v.abs(), i32::MAX as u32)?;
    let signed_num = if v < 0.0 { -(num as i32) } else { num as i32 };
    Some((signed_num, den))
}

/// Convert a float to an unsigned fraction using continued fractions.
/// Port of C++ `floatToUnsignedFraction()`.
pub fn float_to_unsigned_fraction(v: f32) -> Option<(u32, u32)> {
    float_to_unsigned_fraction_impl(v, u32::MAX)
}

// -- XMP metadata --

const HDRGM_NS: &str = "http://ns.adobe.com/hdr-gain-map/1.0/";

/// Format a float value matching C++ `std::stringstream << float_value`
/// with default precision (6 significant digits, `defaultfloat` format).
///
/// C++ `defaultfloat` uses `%g`-style formatting:
/// - Scientific notation when exponent < -4 or >= precision (6)
/// - Fixed notation otherwise
/// - Trailing zeros are removed
fn cpp_float_to_string(v: f32) -> String {
    if v == 0.0 {
        return "0".to_string();
    }
    // Use %g-style formatting with 6 significant digits
    // This matches C++ defaultfloat precision(6)
    let s = format!("{:.6e}", v);
    // Parse out mantissa and exponent
    let parts: Vec<&str> = s.split('e').collect();
    if parts.len() != 2 {
        return format!("{v}");
    }
    let exp: i32 = parts[1].parse().unwrap_or(0);

    // C++ %g rules: use scientific if exp < -4 or exp >= 6
    if !(-4..6).contains(&exp) {
        // Scientific notation: format with 6 significant digits
        let formatted = format!("{:.5e}", v); // 5 decimal places = 6 sig digits
        // Parse to remove trailing zeros from mantissa
        let parts: Vec<&str> = formatted.split('e').collect();
        let mantissa = parts[0].trim_end_matches('0').trim_end_matches('.');
        let exp_val: i32 = parts[1].parse().unwrap_or(0);
        // C++ Linux format: e+06, e-07 (at least 2 digits, with sign)
        if exp_val >= 0 {
            format!("{mantissa}e+{:02}", exp_val)
        } else {
            format!("{mantissa}e-{:02}", -exp_val)
        }
    } else {
        // Fixed notation with 6 significant digits total
        // Number of decimal places = 6 - (exp + 1) = 5 - exp
        let decimal_places = (5 - exp).max(0) as usize;
        let formatted = format!("{:.prec$}", v, prec = decimal_places);
        // Remove trailing zeros after decimal point
        if formatted.contains('.') {
            formatted
                .trim_end_matches('0')
                .trim_end_matches('.')
                .to_string()
        } else {
            formatted
        }
    }
}

/// C++ XmlWriter-compatible XML builder.
///
/// Produces output matching the exact format of C++ libultrahdr's XmlWriter:
/// - `StartWritingElement(name)`: outputs `indent<name`, increases indent by 2 spaces
/// - `WriteXmlns(prefix, uri)`: outputs `\nindent xmlns:prefix="uri"`
/// - `WriteAttributeNameAndValue(name, value)`: outputs `\nindent name="value"`
/// - `FinishWriting()`: closes all open elements
struct XmlWriter {
    output: String,
    indent: String,
    /// Stack of element names (for closing tags).
    elements: Vec<String>,
    /// Whether the current element's opening `<` bracket has been closed with `>`.
    bracket_closed: Vec<bool>,
}

impl XmlWriter {
    fn new() -> Self {
        Self {
            output: String::new(),
            indent: String::new(),
            elements: Vec::new(),
            bracket_closed: Vec::new(),
        }
    }

    fn start_element(&mut self, name: &str) -> &mut Self {
        // If parent element's bracket is still open, close it first
        if let Some(last) = self.bracket_closed.last_mut()
            && !*last
        {
            self.output.push_str(">\n");
            *last = true;
        }
        self.output.push_str(&self.indent);
        self.output.push('<');
        self.output.push_str(name);
        self.elements.push(name.to_string());
        self.bracket_closed.push(false);
        self.indent.push_str("  ");
        self
    }

    fn write_xmlns(&mut self, prefix: &str, uri: &str) -> &mut Self {
        self.output.push('\n');
        self.output.push_str(&self.indent);
        self.output.push_str("xmlns:");
        self.output.push_str(prefix);
        self.output.push_str("=\"");
        self.output.push_str(uri);
        self.output.push('"');
        self
    }

    fn write_attr_str(&mut self, name: &str, value: &str) -> &mut Self {
        self.output.push('\n');
        self.output.push_str(&self.indent);
        self.output.push_str(name);
        self.output.push_str("=\"");
        self.output.push_str(value);
        self.output.push('"');
        self
    }

    fn write_attr_float(&mut self, name: &str, value: f32) -> &mut Self {
        self.write_attr_str(name, &cpp_float_to_string(value))
    }

    fn write_attr_usize(&mut self, name: &str, value: usize) -> &mut Self {
        self.write_attr_str(name, &value.to_string())
    }

    /// Close elements until depth matches `target_depth`.
    /// `target_depth` is the number of open elements to keep.
    fn finish_to_depth(&mut self, target_depth: usize) -> &mut Self {
        while self.elements.len() > target_depth {
            self.finish_element();
        }
        self
    }

    fn finish_element(&mut self) -> &mut Self {
        if self.elements.is_empty() {
            return self;
        }
        self.indent.truncate(self.indent.len().saturating_sub(2));
        let name = self.elements.pop().unwrap();
        let bracket_was_closed = self.bracket_closed.pop().unwrap_or(false);
        if bracket_was_closed {
            // Has children/content, use closing tag
            self.output.push_str(&self.indent);
            self.output.push_str("</");
            self.output.push_str(&name);
            self.output.push_str(">\n");
        } else {
            // Self-closing (no children)
            self.output.push_str("/>\n");
        }
        self
    }

    fn finish_all(&mut self) -> &mut Self {
        while !self.elements.is_empty() {
            self.finish_element();
        }
        self
    }

    fn into_string(self) -> String {
        self.output
    }
}

/// Write primary image XMP (container directory) matching C++ `generateXmpForPrimaryImage()`.
///
/// This XMP describes the container structure: primary JPEG + secondary gain map JPEG.
pub fn write_xmp_primary_container(secondary_image_length: usize) -> Vec<u8> {
    let mut w = XmlWriter::new();
    w.start_element("x:xmpmeta")
        .write_xmlns("x", "adobe:ns:meta/")
        .write_attr_str("x:xmptk", "Adobe XMP Core 5.1.2");
    w.start_element("rdf:RDF")
        .write_xmlns("rdf", "http://www.w3.org/1999/02/22-rdf-syntax-ns#");
    w.start_element("rdf:Description")
        .write_xmlns("Container", "http://ns.google.com/photos/1.0/container/")
        .write_xmlns("Item", "http://ns.google.com/photos/1.0/container/item/")
        .write_xmlns("hdrgm", HDRGM_NS)
        .write_attr_str("hdrgm:Version", "1.0");

    // Container:Directory > rdf:Seq
    w.start_element("Container:Directory");
    w.start_element("rdf:Seq");

    // First item: Primary
    let item_depth = w.elements.len(); // depth to return to after each rdf:li
    w.start_element("rdf:li")
        .write_attr_str("rdf:parseType", "Resource");
    w.start_element("Container:Item")
        .write_attr_str("Item:Semantic", "Primary")
        .write_attr_str("Item:Mime", "image/jpeg");
    w.finish_to_depth(item_depth); // close Container:Item + rdf:li

    // Second item: GainMap
    w.start_element("rdf:li")
        .write_attr_str("rdf:parseType", "Resource");
    w.start_element("Container:Item")
        .write_attr_str("Item:Semantic", "GainMap")
        .write_attr_str("Item:Mime", "image/jpeg")
        .write_attr_usize("Item:Length", secondary_image_length);

    w.finish_all();
    w.into_string().into_bytes()
}

/// Write gain map metadata as XMP bytes (secondary image XMP).
///
/// Matches C++ `generateXmpForSecondaryImage()` XmlWriter output format.
pub fn write_xmp_gainmap_metadata(meta: &GainMapMetadata) -> Result<Vec<u8>> {
    let mut w = XmlWriter::new();
    w.start_element("x:xmpmeta")
        .write_xmlns("x", "adobe:ns:meta/")
        .write_attr_str("x:xmptk", "Adobe XMP Core 5.1.2");
    w.start_element("rdf:RDF")
        .write_xmlns("rdf", "http://www.w3.org/1999/02/22-rdf-syntax-ns#");
    w.start_element("rdf:Description")
        .write_xmlns("hdrgm", HDRGM_NS)
        .write_attr_str("hdrgm:Version", "1.0")
        .write_attr_float("hdrgm:GainMapMin", meta.min_content_boost[0].log2())
        .write_attr_float("hdrgm:GainMapMax", meta.max_content_boost[0].log2())
        .write_attr_float("hdrgm:Gamma", meta.gamma[0])
        .write_attr_float("hdrgm:OffsetSDR", meta.offset_sdr[0])
        .write_attr_float("hdrgm:OffsetHDR", meta.offset_hdr[0])
        .write_attr_float("hdrgm:HDRCapacityMin", meta.hdr_capacity_min.log2())
        .write_attr_float("hdrgm:HDRCapacityMax", meta.hdr_capacity_max.log2())
        .write_attr_str("hdrgm:BaseRenditionIsHDR", "False");
    w.finish_all();
    Ok(w.into_string().into_bytes())
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
