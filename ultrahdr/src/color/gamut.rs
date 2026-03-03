use crate::color::Color;
use crate::types::ColorGamut;

const SRGB_R: f32 = 0.212639;
const SRGB_G: f32 = 0.715169;
const SRGB_B: f32 = 0.072192;

pub fn srgb_luminance(c: Color) -> f32 {
    SRGB_R * c.r + SRGB_G * c.g + SRGB_B * c.b
}

const P3_R: f32 = 0.2289746;
const P3_G: f32 = 0.6917385;
const P3_B: f32 = 0.0792869;

pub fn p3_luminance(c: Color) -> f32 {
    P3_R * c.r + P3_G * c.g + P3_B * c.b
}

const BT2100_R: f32 = 0.2627;
const BT2100_G: f32 = 0.677998;
const BT2100_B: f32 = 0.059302;

pub fn bt2100_luminance(c: Color) -> f32 {
    BT2100_R * c.r + BT2100_G * c.g + BT2100_B * c.b
}

pub fn luminance(c: Color, gamut: ColorGamut) -> f32 {
    match gamut {
        ColorGamut::Bt709 => srgb_luminance(c),
        ColorGamut::DisplayP3 => p3_luminance(c),
        ColorGamut::Bt2100 => bt2100_luminance(c),
    }
}

#[rustfmt::skip]
const BT709_TO_P3: [f32; 9] = [
    0.822462,  0.177537,  0.000001,
    0.033194,  0.966807, -0.000001,
    0.017083,  0.072398,  0.91052,
];
#[rustfmt::skip]
const BT709_TO_BT2100: [f32; 9] = [
    0.627404, 0.329282, 0.043314,
    0.069097, 0.919541, 0.011362,
    0.016392, 0.088013, 0.895595,
];
#[rustfmt::skip]
const P3_TO_BT709: [f32; 9] = [
    1.22494, -0.22494,  0.0,
   -0.042057, 1.042057, 0.0,
   -0.019638,-0.078636, 1.098274,
];
#[rustfmt::skip]
const P3_TO_BT2100: [f32; 9] = [
    0.753833, 0.198597, 0.04757,
    0.045744, 0.941777, 0.012479,
   -0.00121,  0.017601, 0.983608,
];
#[rustfmt::skip]
const BT2100_TO_BT709: [f32; 9] = [
    1.660491, -0.587641, -0.07285,
   -0.124551,  1.1329,   -0.008349,
   -0.018151, -0.100579,  1.11873,
];
#[rustfmt::skip]
const BT2100_TO_P3: [f32; 9] = [
    1.343578, -0.282179, -0.061399,
   -0.065298,  1.075788, -0.01049,
    0.002822, -0.019598,  1.016777,
];

fn apply_matrix(c: Color, m: &[f32; 9]) -> Color {
    Color {
        r: m[0] * c.r + m[1] * c.g + m[2] * c.b,
        g: m[3] * c.r + m[4] * c.g + m[5] * c.b,
        b: m[6] * c.r + m[7] * c.g + m[8] * c.b,
    }
}

pub fn gamut_convert(c: Color, from: ColorGamut, to: ColorGamut) -> Color {
    if from == to {
        return c;
    }
    match (from, to) {
        (ColorGamut::Bt709, ColorGamut::DisplayP3) => apply_matrix(c, &BT709_TO_P3),
        (ColorGamut::Bt709, ColorGamut::Bt2100) => apply_matrix(c, &BT709_TO_BT2100),
        (ColorGamut::DisplayP3, ColorGamut::Bt709) => apply_matrix(c, &P3_TO_BT709),
        (ColorGamut::DisplayP3, ColorGamut::Bt2100) => apply_matrix(c, &P3_TO_BT2100),
        (ColorGamut::Bt2100, ColorGamut::Bt709) => apply_matrix(c, &BT2100_TO_BT709),
        (ColorGamut::Bt2100, ColorGamut::DisplayP3) => apply_matrix(c, &BT2100_TO_P3),
        _ => c,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::color::Color;

    #[test]
    fn srgb_luminance_white() {
        let white = Color::new(1.0, 1.0, 1.0);
        let lum = srgb_luminance(white);
        assert!((lum - 1.0).abs() < 0.001);
    }

    #[test]
    fn bt2100_luminance_white() {
        let white = Color::new(1.0, 1.0, 1.0);
        let lum = bt2100_luminance(white);
        assert!((lum - 1.0).abs() < 0.001);
    }

    #[test]
    fn bt709_to_p3_identity_white() {
        let white = Color::new(1.0, 1.0, 1.0);
        let converted = gamut_convert(white, ColorGamut::Bt709, ColorGamut::DisplayP3);
        assert!((converted.r - 1.0).abs() < 0.01);
        assert!((converted.g - 1.0).abs() < 0.01);
        assert!((converted.b - 1.0).abs() < 0.01);
    }
}
