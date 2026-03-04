//! Multi-Picture Format (MPF) segment generation for embedding gain map JPEGs.
//!
//! Generates APP2/MPF marker data per the CIPA DC-007 specification,
//! used to link a primary JPEG image with a secondary gain map JPEG.

const NUM_PICTURES: usize = 2;
const MP_ENDIAN_SIZE: usize = 4;
const TAG_SERIALIZED_COUNT: u16 = 3;
const TAG_SIZE: usize = 12;

const TYPE_LONG: u16 = 0x4;
const TYPE_UNDEFINED: u16 = 0x7;

const MPF_SIG: &[u8; 4] = b"MPF\0";
const MP_BIG_ENDIAN: [u8; MP_ENDIAN_SIZE] = [0x4D, 0x4D, 0x00, 0x2A];

const VERSION_TAG: u16 = 0xB000;
const VERSION_TYPE: u16 = TYPE_UNDEFINED;
const VERSION_COUNT: u32 = 4;
const VERSION_EXPECTED: [u8; 4] = [b'0', b'1', b'0', b'0'];

const NUMBER_OF_IMAGES_TAG: u16 = 0xB001;
const NUMBER_OF_IMAGES_TYPE: u16 = TYPE_LONG;
const NUMBER_OF_IMAGES_COUNT: u32 = 1;

const MP_ENTRY_TAG: u16 = 0xB002;
const MP_ENTRY_TYPE: u16 = TYPE_UNDEFINED;
const MP_ENTRY_SIZE: usize = 16;

const MP_ENTRY_ATTRIBUTE_FORMAT_JPEG: u32 = 0x0000000;
const MP_ENTRY_ATTRIBUTE_TYPE_PRIMARY: u32 = 0x030000;

/// Returns the fixed size of the MPF segment data (excluding APP2 marker and length bytes).
pub fn calculate_mpf_size() -> usize {
    MPF_SIG.len()
        + MP_ENDIAN_SIZE
        + 4 // Index IFD Offset
        + 2 // Tag count
        + TAG_SERIALIZED_COUNT as usize * TAG_SIZE
        + 4 // Attribute IFD offset
        + NUM_PICTURES * MP_ENTRY_SIZE
}

/// Generates complete MPF APP2 data for a dual-image JPEG (primary + gain map).
///
/// All multi-byte values are written in big-endian byte order, matching the C++ reference.
pub fn generate_mpf(
    primary_size: u32,
    primary_offset: u32,
    secondary_size: u32,
    secondary_offset: u32,
) -> Vec<u8> {
    let mpf_size = calculate_mpf_size();
    let mut buf = Vec::with_capacity(mpf_size);

    // MPF signature
    buf.extend_from_slice(MPF_SIG);

    // Byte order: big-endian
    buf.extend_from_slice(&MP_BIG_ENDIAN);

    // Index IFD offset (from start of TIFF header = endian marker)
    let index_ifd_offset: u32 = (MP_ENDIAN_SIZE + MPF_SIG.len()) as u32;
    buf.extend_from_slice(&index_ifd_offset.to_be_bytes());

    // Tag count
    buf.extend_from_slice(&TAG_SERIALIZED_COUNT.to_be_bytes());

    // Tag 1: MPFVersion (0xB000)
    buf.extend_from_slice(&VERSION_TAG.to_be_bytes());
    buf.extend_from_slice(&VERSION_TYPE.to_be_bytes());
    buf.extend_from_slice(&VERSION_COUNT.to_be_bytes());
    buf.extend_from_slice(&VERSION_EXPECTED);

    // Tag 2: NumberOfImages (0xB001)
    buf.extend_from_slice(&NUMBER_OF_IMAGES_TAG.to_be_bytes());
    buf.extend_from_slice(&NUMBER_OF_IMAGES_TYPE.to_be_bytes());
    buf.extend_from_slice(&NUMBER_OF_IMAGES_COUNT.to_be_bytes());
    buf.extend_from_slice(&(NUM_PICTURES as u32).to_be_bytes());

    // Tag 3: MPEntry (0xB002)
    buf.extend_from_slice(&MP_ENTRY_TAG.to_be_bytes());
    buf.extend_from_slice(&MP_ENTRY_TYPE.to_be_bytes());
    buf.extend_from_slice(&((MP_ENTRY_SIZE * NUM_PICTURES) as u32).to_be_bytes());
    // Offset to MP entry data, relative to TIFF header (after signature).
    // Current position = buf.len(), then 4 bytes for this offset + 4 for attr IFD = entries start.
    let mp_entry_offset: u32 = (buf.len() - MPF_SIG.len() + 4 + 4) as u32;
    buf.extend_from_slice(&mp_entry_offset.to_be_bytes());

    // Attribute IFD offset: 0 (not used)
    buf.extend_from_slice(&0u32.to_be_bytes());

    // MP Entry: primary image
    buf.extend_from_slice(
        &(MP_ENTRY_ATTRIBUTE_FORMAT_JPEG | MP_ENTRY_ATTRIBUTE_TYPE_PRIMARY).to_be_bytes(),
    );
    buf.extend_from_slice(&primary_size.to_be_bytes());
    buf.extend_from_slice(&primary_offset.to_be_bytes());
    buf.extend_from_slice(&0u16.to_be_bytes());
    buf.extend_from_slice(&0u16.to_be_bytes());

    // MP Entry: secondary image (gain map)
    buf.extend_from_slice(&MP_ENTRY_ATTRIBUTE_FORMAT_JPEG.to_be_bytes());
    buf.extend_from_slice(&secondary_size.to_be_bytes());
    buf.extend_from_slice(&secondary_offset.to_be_bytes());
    buf.extend_from_slice(&0u16.to_be_bytes());
    buf.extend_from_slice(&0u16.to_be_bytes());

    debug_assert_eq!(buf.len(), mpf_size);
    buf
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mpf_size_is_correct() {
        let size = calculate_mpf_size();
        // Known fixed size: signature(4) + endian(4) + offset(4) + tagcount(2)
        // + 3*tags(36) + attrIFD(4) + 2*entries(32) = 86
        assert_eq!(size, 86);
    }

    #[test]
    fn generate_mpf_valid_structure() {
        let mpf = generate_mpf(10000, 0, 5000, 10000);
        // Check MPF signature at start
        assert_eq!(&mpf[..4], b"MPF\0");
    }
}
