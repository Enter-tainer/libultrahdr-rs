use bytes::Bytes;
use img_parts::{ImageICC, jpeg::Jpeg};
use ultrahdr::ColorGamut;

/// Best-effort ICC-based color gamut detection for a JPEG.
pub fn detect_icc_color_gamut(bytes: &[u8]) -> Option<ColorGamut> {
    let icc_bytes = Jpeg::from_bytes(Bytes::copy_from_slice(bytes))
        .ok()?
        .icc_profile()?
        .to_vec();
    ultrahdr::color::icc::detect_color_gamut(&icc_bytes)
}

pub fn gamut_label(cg: ColorGamut) -> &'static str {
    match cg {
        ColorGamut::Bt709 => "BT.709 / sRGB",
        ColorGamut::Bt2100 => "BT.2100 / Rec.2020",
        ColorGamut::DisplayP3 => "Display P3",
    }
}
