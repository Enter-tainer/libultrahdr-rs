pub mod decode;
pub mod encode;

use crate::error::{Error, Result};

pub struct JpegSegment {
    pub marker: u8,
    pub offset: usize,
    pub data: Vec<u8>,
}

pub struct JpegSegments {
    pub soi_offset: usize,
    pub segments: Vec<JpegSegment>,
}

/// Parse JPEG marker segments from raw bytes.
///
/// Walks through JPEG markers (SOI, APPn, DQT, SOF, SOS, EOI, etc.)
/// and extracts each segment's marker byte, offset, and payload data.
/// Stops parsing after encountering SOS (start of scan) or EOI.
pub fn parse_jpeg_segments(data: &[u8]) -> Result<JpegSegments> {
    if data.len() < 2 || data[0] != 0xFF || data[1] != 0xD8 {
        return Err(Error::JpegError("not a JPEG: missing SOI marker".into()));
    }

    let soi_offset = 0;
    let mut segments = Vec::new();
    let mut pos = 2; // skip SOI

    while pos < data.len() {
        // Find next 0xFF
        if data[pos] != 0xFF {
            // In scan data or padding; stop parsing
            break;
        }

        // Skip any padding 0xFF bytes
        while pos < data.len() && data[pos] == 0xFF {
            pos += 1;
        }
        if pos >= data.len() {
            break;
        }

        let marker = data[pos];
        let marker_offset = pos - 1; // offset of the 0xFF byte
        pos += 1;

        // Standalone markers (no payload): RST0-RST7, SOI, EOI, TEM
        if marker == 0xD9 {
            // EOI — end of image
            break;
        }
        if marker == 0x00 || marker == 0x01 || (0xD0..=0xD7).contains(&marker) {
            // Stuffed byte, TEM, or RST markers — no length field
            continue;
        }

        // All other markers have a 2-byte length field
        if pos + 2 > data.len() {
            break;
        }
        let length = u16::from_be_bytes([data[pos], data[pos + 1]]) as usize;
        if length < 2 {
            return Err(Error::JpegError(format!(
                "invalid segment length {} at offset {}",
                length, marker_offset
            )));
        }

        let payload_start = pos + 2;
        let payload_len = length - 2;
        let payload_end = payload_start + payload_len;

        let segment_data = if payload_end <= data.len() {
            data[payload_start..payload_end].to_vec()
        } else {
            // Truncated segment — take what we can
            data[payload_start..data.len()].to_vec()
        };

        segments.push(JpegSegment {
            marker,
            offset: marker_offset,
            data: segment_data,
        });

        pos = payload_start + payload_len;

        // SOS marks start of entropy-coded data; stop parsing structured segments
        if marker == 0xDA {
            break;
        }
    }

    Ok(JpegSegments {
        soi_offset,
        segments,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extract_segments_from_minimal_jpeg() {
        // SOI + APP0 (JFIF) + minimal scan + EOI
        let jpeg = [0xFF, 0xD8, 0xFF, 0xE0, 0x00, 0x02, 0x00, 0xFF, 0xD9];
        let segments = parse_jpeg_segments(&jpeg);
        assert!(segments.is_ok());
    }

    #[test]
    fn find_soi_and_eoi() {
        let jpeg = [0xFF, 0xD8, 0xFF, 0xD9];
        let segments = parse_jpeg_segments(&jpeg).unwrap();
        assert!(segments.soi_offset == 0);
    }
}
