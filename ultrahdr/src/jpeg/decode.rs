use jpeg_decoder::Decoder;

use crate::error::{Error, Result};

pub struct JpegDecoded {
    pub pixels: Vec<u8>,
    pub width: u32,
    pub height: u32,
    pub icc_profile: Option<Vec<u8>>,
}

pub fn decode_jpeg(data: &[u8]) -> Result<JpegDecoded> {
    let mut decoder = Decoder::new(data);
    let pixels = decoder
        .decode()
        .map_err(|e| Error::JpegError(e.to_string()))?;
    let info = decoder
        .info()
        .ok_or_else(|| Error::JpegError("no image info after decoding".into()))?;

    let icc_profile = decoder.icc_profile();

    Ok(JpegDecoded {
        pixels,
        width: info.width as u32,
        height: info.height as u32,
        icc_profile,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::jpeg::encode::encode_rgb_to_jpeg;

    #[test]
    fn decode_minimal_jpeg() {
        let width = 2;
        let height = 2;
        let pixels: Vec<u8> = vec![128; width * height * 3]; // grey RGB
        let jpeg_bytes =
            encode_rgb_to_jpeg(&pixels, width as u32, height as u32, 90).expect("encode failed");
        let info = decode_jpeg(&jpeg_bytes).expect("decode failed");
        assert_eq!(info.width, width as u32);
        assert_eq!(info.height, height as u32);
    }
}
