const PQ_M1: f32 = 2610.0 / 16384.0;
const PQ_M2: f32 = 2523.0 / 4096.0 * 128.0;
const PQ_C1: f32 = 3424.0 / 4096.0;
const PQ_C2: f32 = 2413.0 / 4096.0 * 32.0;
const PQ_C3: f32 = 2392.0 / 4096.0 * 32.0;

/// PQ OETF (SMPTE ST 2084). Maps linear \[0,1\] to encoded \[0,1\].
pub fn pq_oetf(e: f32) -> f32 {
    if e <= 0.0 {
        return 0.0;
    }
    let e_m1 = e.powf(PQ_M1);
    ((PQ_C1 + PQ_C2 * e_m1) / (1.0 + PQ_C3 * e_m1)).powf(PQ_M2)
}

/// PQ inverse OETF (SMPTE ST 2084). Maps encoded \[0,1\] to linear \[0,1\].
pub fn pq_inv_oetf(e_gamma: f32) -> f32 {
    let val = e_gamma.powf(1.0 / PQ_M2);
    let num = (val - PQ_C1).max(0.0);
    let den = PQ_C2 - PQ_C3 * val;
    (num / den).powf(1.0 / PQ_M1)
}

const HLG_A: f32 = 0.17883277;
const HLG_B: f32 = 0.28466892;
const HLG_C: f32 = 0.559_910_7;

/// HLG OETF (ITU-R BT.2100). Maps linear \[0,1\] to encoded \[0,1\].
pub fn hlg_oetf(e: f32) -> f32 {
    if e <= 1.0 / 12.0 {
        (3.0 * e).sqrt()
    } else {
        HLG_A * (12.0 * e - HLG_B).ln() + HLG_C
    }
}

/// HLG inverse OETF (ITU-R BT.2100). Maps encoded \[0,1\] to linear \[0,1\].
pub fn hlg_inv_oetf(e_gamma: f32) -> f32 {
    if e_gamma <= 0.5 {
        e_gamma * e_gamma / 3.0
    } else {
        (((e_gamma - HLG_C) / HLG_A).exp() + HLG_B) / 12.0
    }
}

const HLG_OOTF_GAMMA: f32 = 1.2;

/// Approximate HLG OOTF per-channel.
pub fn hlg_ootf_approx(r: f32, g: f32, b: f32) -> [f32; 3] {
    [
        r.powf(HLG_OOTF_GAMMA),
        g.powf(HLG_OOTF_GAMMA),
        b.powf(HLG_OOTF_GAMMA),
    ]
}

/// Approximate HLG inverse OOTF per-channel.
pub fn hlg_inv_ootf_approx(r: f32, g: f32, b: f32) -> [f32; 3] {
    let inv_gamma = 1.0 / HLG_OOTF_GAMMA;
    [r.powf(inv_gamma), g.powf(inv_gamma), b.powf(inv_gamma)]
}

/// sRGB inverse OETF (IEC 61966-2-1). Maps encoded \[0,1\] to linear \[0,1\].
pub fn srgb_inv_oetf(e_gamma: f32) -> f32 {
    if e_gamma <= 0.04045 {
        e_gamma / 12.92
    } else {
        ((e_gamma + 0.055) / 1.055).powf(2.4)
    }
}

/// sRGB OETF (IEC 61966-2-1). Maps linear \[0,1\] to encoded \[0,1\].
pub fn srgb_oetf(e: f32) -> f32 {
    if e <= 0.0031308 {
        12.92 * e
    } else {
        1.055 * e.powf(1.0 / 2.4) - 0.055
    }
}

use std::sync::LazyLock;

use crate::types::{ColorTransfer, HLG_MAX_NITS, PQ_MAX_NITS, SDR_WHITE_NITS};

// ---------------------------------------------------------------------------
// LUT-based fast approximations
// ---------------------------------------------------------------------------

pub const LUT_SIZE: usize = 65536;

/// 256-entry LUT for sRGB inverse OETF: input u8 → linear f32.
static SRGB_INV_OETF_LUT: LazyLock<[f32; 256]> = LazyLock::new(|| {
    let mut lut = [0.0f32; 256];
    for (i, val) in lut.iter_mut().enumerate() {
        *val = srgb_inv_oetf(i as f32 / 255.0);
    }
    lut
});

/// Fast sRGB inverse OETF via 256-entry lookup (exact for u8 inputs).
#[inline(always)]
pub fn srgb_inv_oetf_lut(u8_val: u8) -> f32 {
    SRGB_INV_OETF_LUT[u8_val as usize]
}

/// 65536-entry LUT for PQ OETF: input linear [0,1] → encoded [0,1].
static PQ_OETF_LUT: LazyLock<Vec<f32>> = LazyLock::new(|| {
    (0..LUT_SIZE)
        .map(|i| pq_oetf(i as f32 / (LUT_SIZE - 1) as f32))
        .collect()
});

/// Fast PQ OETF via 65536-entry lookup.
#[inline(always)]
pub fn pq_oetf_lut(e: f32) -> f32 {
    if e <= 0.0 {
        return 0.0;
    }
    let idx = (e * (LUT_SIZE - 1) as f32 + 0.5) as usize;
    PQ_OETF_LUT[idx.min(LUT_SIZE - 1)]
}

/// 65536-entry LUT for HLG OETF: input linear [0,1] → encoded [0,1].
static HLG_OETF_LUT: LazyLock<Vec<f32>> = LazyLock::new(|| {
    (0..LUT_SIZE)
        .map(|i| hlg_oetf(i as f32 / (LUT_SIZE - 1) as f32))
        .collect()
});

/// Fast HLG OETF via 65536-entry lookup.
#[inline(always)]
pub fn hlg_oetf_lut(e: f32) -> f32 {
    if e <= 0.0 {
        return 0.0;
    }
    let idx = (e * (LUT_SIZE - 1) as f32 + 0.5) as usize;
    HLG_OETF_LUT[idx.min(LUT_SIZE - 1)]
}

/// 65536-entry LUT for HLG inverse OOTF approx: pow(x, 1/1.2) for x in [0,1].
static HLG_INV_OOTF_LUT: LazyLock<Vec<f32>> = LazyLock::new(|| {
    let inv_gamma = 1.0f32 / 1.2;
    (0..LUT_SIZE)
        .map(|i| {
            let x = i as f32 / (LUT_SIZE - 1) as f32;
            x.powf(inv_gamma)
        })
        .collect()
});

/// Fast HLG inverse OOTF approx (pow(x, 1/1.2)) via 65536-entry lookup.
#[inline(always)]
pub fn hlg_inv_ootf_approx_lut(e: f32) -> f32 {
    if e <= 0.0 {
        return 0.0;
    }
    let idx = (e * (LUT_SIZE - 1) as f32 + 0.5) as usize;
    HLG_INV_OOTF_LUT[idx.min(LUT_SIZE - 1)]
}

/// Combined HLG inverse OOTF + OETF: hlg_oetf(pow(x, 1/1.2)).
/// Single LUT lookup replaces the two chained lookups.
static HLG_COMBINED_LUT: LazyLock<Vec<f32>> = LazyLock::new(|| {
    let inv_gamma = 1.0f32 / 1.2;
    (0..LUT_SIZE)
        .map(|i| {
            let x = i as f32 / (LUT_SIZE - 1) as f32;
            hlg_oetf(x.powf(inv_gamma))
        })
        .collect()
});

/// Fast combined HLG inverse OOTF + OETF via single 65536-entry lookup.
#[inline(always)]
pub fn hlg_combined_lut(e: f32) -> f32 {
    if e <= 0.0 {
        return 0.0;
    }
    let idx = (e * (LUT_SIZE - 1) as f32 + 0.5) as usize;
    HLG_COMBINED_LUT[idx.min(LUT_SIZE - 1)]
}

/// Get references to all LUTs used during decode. Call once outside the hot loop
/// to avoid per-pixel LazyLock atomic checks.
pub struct TransferLuts {
    pub pq_oetf: &'static [f32],
    pub hlg_combined: &'static [f32],
    /// 256-entry sRGB inverse OETF: input u8 index → linear f32 (exact for u8).
    pub srgb_inv_u8: &'static [f32; 256],
}

impl TransferLuts {
    /// Force-initialize all LUTs and return references.
    pub fn init() -> Self {
        Self {
            pq_oetf: &PQ_OETF_LUT,
            hlg_combined: &HLG_COMBINED_LUT,
            srgb_inv_u8: &SRGB_INV_OETF_LUT,
        }
    }

    #[inline(always)]
    pub fn pq_oetf_lookup(&self, e: f32) -> f32 {
        if e <= 0.0 {
            return 0.0;
        }
        let idx = (e * (LUT_SIZE - 1) as f32 + 0.5) as usize;
        self.pq_oetf[idx.min(LUT_SIZE - 1)]
    }

    #[inline(always)]
    pub fn hlg_combined_lookup(&self, e: f32) -> f32 {
        if e <= 0.0 {
            return 0.0;
        }
        let idx = (e * (LUT_SIZE - 1) as f32 + 0.5) as usize;
        self.hlg_combined[idx.min(LUT_SIZE - 1)]
    }
}

// ---------------------------------------------------------------------------
// C++-matching inverse OETF LUTs (for gain map generation)
// These match the exact LUT sizes and lookup logic used in C++ libultrahdr's
// srgbInvOetfLUT, hlgInvOetfLUT, and pqInvOetfLUT functions.
// ---------------------------------------------------------------------------

const SRGB_INV_OETF_LUT_SIZE: usize = 1024; // 1 << 10, matches C++ kSrgbInvOETFNumEntries

static SRGB_INV_OETF_LUT_1024: LazyLock<[f32; 1024]> = LazyLock::new(|| {
    let mut lut = [0.0f32; 1024];
    for (i, val) in lut.iter_mut().enumerate() {
        *val = srgb_inv_oetf(i as f32 / (SRGB_INV_OETF_LUT_SIZE - 1) as f32);
    }
    lut
});

/// sRGB inverse OETF via 1024-entry LUT matching C++ libultrahdr.
#[inline(always)]
pub fn srgb_inv_oetf_lut_1024(e_gamma: f32) -> f32 {
    let idx = (e_gamma * (SRGB_INV_OETF_LUT_SIZE - 1) as f32 + 0.5) as i32;
    let idx = idx.clamp(0, (SRGB_INV_OETF_LUT_SIZE - 1) as i32) as usize;
    SRGB_INV_OETF_LUT_1024[idx]
}

const HLG_INV_OETF_LUT_SIZE: usize = 4096; // 1 << 12, matches C++ kHlgInvOETFNumEntries

static HLG_INV_OETF_LUT_4096: LazyLock<[f32; 4096]> = LazyLock::new(|| {
    let mut lut = [0.0f32; 4096];
    for (i, val) in lut.iter_mut().enumerate() {
        *val = hlg_inv_oetf(i as f32 / (HLG_INV_OETF_LUT_SIZE - 1) as f32);
    }
    lut
});

/// HLG inverse OETF via 4096-entry LUT matching C++ libultrahdr.
#[inline(always)]
pub fn hlg_inv_oetf_lut_4096(e_gamma: f32) -> f32 {
    let idx = (e_gamma * (HLG_INV_OETF_LUT_SIZE - 1) as f32 + 0.5) as i32;
    let idx = idx.clamp(0, (HLG_INV_OETF_LUT_SIZE - 1) as i32) as usize;
    HLG_INV_OETF_LUT_4096[idx]
}

const PQ_INV_OETF_LUT_SIZE: usize = 4096; // 1 << 12, matches C++ kPqInvOETFNumEntries

static PQ_INV_OETF_LUT_4096: LazyLock<[f32; 4096]> = LazyLock::new(|| {
    let mut lut = [0.0f32; 4096];
    for (i, val) in lut.iter_mut().enumerate() {
        *val = pq_inv_oetf(i as f32 / (PQ_INV_OETF_LUT_SIZE - 1) as f32);
    }
    lut
});

/// PQ inverse OETF via 4096-entry LUT matching C++ libultrahdr.
#[inline(always)]
pub fn pq_inv_oetf_lut_4096(e_gamma: f32) -> f32 {
    let idx = (e_gamma * (PQ_INV_OETF_LUT_SIZE - 1) as f32 + 0.5) as i32;
    let idx = idx.clamp(0, (PQ_INV_OETF_LUT_SIZE - 1) as i32) as usize;
    PQ_INV_OETF_LUT_4096[idx]
}

/// Reference display peak brightness in nits for a given transfer function.
pub fn reference_display_peak_nits(transfer: ColorTransfer) -> f32 {
    match transfer {
        ColorTransfer::Linear => PQ_MAX_NITS,
        ColorTransfer::Hlg => HLG_MAX_NITS,
        ColorTransfer::Pq => PQ_MAX_NITS,
        ColorTransfer::Srgb => SDR_WHITE_NITS,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn srgb_inv_oetf_linear_region() {
        let result = srgb_inv_oetf(0.04045);
        assert!((result - 0.04045 / 12.92).abs() < 1e-6);
    }

    #[test]
    fn srgb_inv_oetf_gamma_region() {
        let result = srgb_inv_oetf(1.0);
        assert!((result - 1.0).abs() < 1e-5);
    }

    #[test]
    fn srgb_oetf_roundtrip() {
        for i in 0..=10 {
            let linear = i as f32 / 10.0;
            let encoded = srgb_oetf(linear);
            let decoded = srgb_inv_oetf(encoded);
            assert!((linear - decoded).abs() < 1e-5, "failed at {linear}");
        }
    }

    #[test]
    fn srgb_oetf_zero_and_one() {
        assert!((srgb_oetf(0.0)).abs() < 1e-7);
        assert!((srgb_oetf(1.0) - 1.0).abs() < 1e-5);
    }

    #[test]
    fn pq_oetf_zero() {
        assert!(pq_oetf(0.0).abs() < 1e-7);
    }

    #[test]
    fn pq_roundtrip() {
        for i in 1..=10 {
            let linear = i as f32 / 10.0;
            let encoded = pq_oetf(linear);
            let decoded = pq_inv_oetf(encoded);
            assert!(
                (linear - decoded).abs() < 1e-4,
                "failed at {linear}: got {decoded}"
            );
        }
    }

    #[test]
    fn pq_oetf_negative_clamps() {
        assert!(pq_oetf(-1.0).abs() < 1e-7);
    }

    #[test]
    fn hlg_oetf_low_range() {
        let e = 1.0 / 12.0;
        let result = hlg_oetf(e);
        assert!((result - (3.0 * e).sqrt()).abs() < 1e-6);
    }

    #[test]
    fn hlg_roundtrip() {
        for i in 1..=10 {
            let linear = i as f32 / 10.0;
            let encoded = hlg_oetf(linear);
            let decoded = hlg_inv_oetf(encoded);
            assert!((linear - decoded).abs() < 1e-4, "failed at {linear}");
        }
    }

    #[test]
    fn hlg_inv_oetf_low_range() {
        let result = hlg_inv_oetf(0.5);
        assert!((result - 0.25 / 3.0).abs() < 1e-6);
    }

    #[test]
    fn reference_display_peak_pq() {
        use crate::types::ColorTransfer;
        assert!((reference_display_peak_nits(ColorTransfer::Pq) - 10000.0).abs() < 0.1);
    }

    #[test]
    fn reference_display_peak_srgb() {
        use crate::types::ColorTransfer;
        assert!((reference_display_peak_nits(ColorTransfer::Srgb) - 203.0).abs() < 0.1);
    }

    #[test]
    fn srgb_inv_oetf_lut_1024_differs_from_direct() {
        // LUT quantizes input to 1024 entries, so result differs from direct computation
        let val = 0.5;
        let direct = srgb_inv_oetf(val);
        let lut = srgb_inv_oetf_lut_1024(val);
        // Both should be close but not identical due to quantization
        assert!(
            (direct - lut).abs() < 0.01,
            "LUT and direct should be close: direct={direct}, lut={lut}"
        );
    }

    #[test]
    fn hlg_inv_oetf_lut_4096_differs_from_direct() {
        let val = 0.7;
        let direct = hlg_inv_oetf(val);
        let lut = hlg_inv_oetf_lut_4096(val);
        assert!(
            (direct - lut).abs() < 0.001,
            "HLG LUT and direct should be close: direct={direct}, lut={lut}"
        );
    }

    #[test]
    fn pq_inv_oetf_lut_4096_differs_from_direct() {
        let val = 0.5;
        let direct = pq_inv_oetf(val);
        let lut = pq_inv_oetf_lut_4096(val);
        assert!(
            (direct - lut).abs() < 0.001,
            "PQ LUT and direct should be close: direct={direct}, lut={lut}"
        );
    }

    #[test]
    fn srgb_inv_oetf_lut_1024_boundary_values() {
        // Test boundary: 0.0 and 1.0 should match exactly
        assert_eq!(srgb_inv_oetf_lut_1024(0.0), srgb_inv_oetf(0.0));
        assert_eq!(srgb_inv_oetf_lut_1024(1.0), srgb_inv_oetf(1.0));
    }

    #[test]
    fn lut_clamps_out_of_range() {
        // Negative and >1.0 inputs should be clamped
        let _ = srgb_inv_oetf_lut_1024(-0.1); // should not panic
        let _ = srgb_inv_oetf_lut_1024(1.5); // should not panic
        let _ = hlg_inv_oetf_lut_4096(-0.1);
        let _ = hlg_inv_oetf_lut_4096(1.5);
        let _ = pq_inv_oetf_lut_4096(-0.1);
        let _ = pq_inv_oetf_lut_4096(1.5);
    }
}
