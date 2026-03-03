use jpeg_encoder::{ColorType, Encoder};

use crate::error::{Error, Result};

pub fn encode_rgb_to_jpeg(pixels: &[u8], width: u32, height: u32, quality: u8) -> Result<Vec<u8>> {
    let w: u16 = width
        .try_into()
        .map_err(|_| Error::InvalidParam(format!("width {width} exceeds u16::MAX")))?;
    let h: u16 = height
        .try_into()
        .map_err(|_| Error::InvalidParam(format!("height {height} exceeds u16::MAX")))?;
    let mut buf = Vec::new();
    let encoder = Encoder::new(&mut buf, quality);
    encoder
        .encode(pixels, w, h, ColorType::Rgb)
        .map_err(|e| Error::JpegError(e.to_string()))?;
    Ok(buf)
}

pub fn encode_grayscale_to_jpeg(
    pixels: &[u8],
    width: u32,
    height: u32,
    quality: u8,
) -> Result<Vec<u8>> {
    let w: u16 = width
        .try_into()
        .map_err(|_| Error::InvalidParam(format!("width {width} exceeds u16::MAX")))?;
    let h: u16 = height
        .try_into()
        .map_err(|_| Error::InvalidParam(format!("height {height} exceeds u16::MAX")))?;
    let mut buf = Vec::new();
    let encoder = Encoder::new(&mut buf, quality);
    encoder
        .encode(pixels, w, h, ColorType::Luma)
        .map_err(|e| Error::JpegError(e.to_string()))?;
    Ok(buf)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encode_roundtrip() {
        let width = 4;
        let height = 4;
        let pixels: Vec<u8> = (0..width * height * 3).map(|i| (i % 256) as u8).collect();
        let jpeg = encode_rgb_to_jpeg(&pixels, width as u32, height as u32, 95)
            .expect("encode failed");
        // Verify JPEG starts with SOI marker
        assert_eq!(&jpeg[..2], &[0xFF, 0xD8]);
        // Verify it ends with EOI marker
        assert_eq!(&jpeg[jpeg.len()-2..], &[0xFF, 0xD9]);
    }

    #[test]
    fn encode_grayscale() {
        let width = 2;
        let height = 2;
        let pixels: Vec<u8> = vec![128; width * height];
        let jpeg = encode_grayscale_to_jpeg(&pixels, width as u32, height as u32, 90)
            .expect("encode failed");
        assert_eq!(&jpeg[..2], &[0xFF, 0xD8]);
    }

    #[test]
    fn encode_rgb_rejects_oversized_width() {
        let width = u32::from(u16::MAX) + 1;
        let height = 1;
        let pixels = vec![0u8; width as usize * height as usize * 3];
        let result = encode_rgb_to_jpeg(&pixels, width, height, 90);
        assert!(result.is_err());
    }

    #[test]
    fn encode_grayscale_rejects_oversized_height() {
        let width = 1;
        let height = u32::from(u16::MAX) + 1;
        let pixels = vec![0u8; width as usize * height as usize];
        let result = encode_grayscale_to_jpeg(&pixels, width, height, 90);
        assert!(result.is_err());
    }
}
