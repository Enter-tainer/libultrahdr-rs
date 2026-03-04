#![allow(dead_code)]

use pulp::{Arch, Simd, WithSimd};

struct ApplyGainChannel<'a> {
    lin: &'a [f32],
    factor: &'a [f32],
    offset_sdr: f32,
    offset_hdr: f32,
    out: &'a mut [f32],
}

impl WithSimd for ApplyGainChannel<'_> {
    type Output = ();

    #[inline(always)]
    fn with_simd<S: Simd>(self, simd: S) -> Self::Output {
        let Self {
            lin,
            factor,
            offset_sdr,
            offset_hdr,
            out,
        } = self;
        let n = lin.len();
        debug_assert_eq!(n, factor.len());
        debug_assert_eq!(n, out.len());

        let v_offset_sdr = simd.splat_f32s(offset_sdr);
        let v_neg_offset_hdr = simd.splat_f32s(-offset_hdr);

        let (lin_head, lin_tail) = S::as_simd_f32s(lin);
        let (fac_head, fac_tail) = S::as_simd_f32s(factor);
        let (out_head, out_tail) = S::as_mut_simd_f32s(out);

        for ((o, &l), &f) in out_head.iter_mut().zip(lin_head).zip(fac_head) {
            let shifted = simd.add_f32s(l, v_offset_sdr);
            // fma: factor * shifted + (-offset_hdr)
            *o = simd.mul_add_f32s(f, shifted, v_neg_offset_hdr);
        }

        for ((o, &l), &f) in out_tail.iter_mut().zip(lin_tail).zip(fac_tail) {
            *o = (l + offset_sdr) * f - offset_hdr;
        }
    }
}

struct ClampOp<'a> {
    data: &'a mut [f32],
    min: f32,
    max: f32,
}

impl WithSimd for ClampOp<'_> {
    type Output = ();

    #[inline(always)]
    fn with_simd<S: Simd>(self, simd: S) -> Self::Output {
        let Self { data, min, max } = self;
        let v_min = simd.splat_f32s(min);
        let v_max = simd.splat_f32s(max);

        let (head, tail) = S::as_mut_simd_f32s(data);

        for x in head.iter_mut() {
            *x = simd.min_f32s(simd.max_f32s(*x, v_min), v_max);
        }

        for x in tail.iter_mut() {
            *x = x.clamp(min, max);
        }
    }
}

/// Apply gain map using SIMD: `(lin + offset_sdr) * factor - offset_hdr` per channel.
#[allow(clippy::too_many_arguments)]
pub(crate) fn apply_gain_simd(
    r_lin: &[f32],
    g_lin: &[f32],
    b_lin: &[f32],
    factor_r: &[f32],
    factor_g: &[f32],
    factor_b: &[f32],
    offset_sdr: &[f32; 3],
    offset_hdr: &[f32; 3],
    hdr_r: &mut [f32],
    hdr_g: &mut [f32],
    hdr_b: &mut [f32],
) {
    let arch = Arch::new();
    arch.dispatch(ApplyGainChannel {
        lin: r_lin,
        factor: factor_r,
        offset_sdr: offset_sdr[0],
        offset_hdr: offset_hdr[0],
        out: hdr_r,
    });
    arch.dispatch(ApplyGainChannel {
        lin: g_lin,
        factor: factor_g,
        offset_sdr: offset_sdr[1],
        offset_hdr: offset_hdr[1],
        out: hdr_g,
    });
    arch.dispatch(ApplyGainChannel {
        lin: b_lin,
        factor: factor_b,
        offset_sdr: offset_sdr[2],
        offset_hdr: offset_hdr[2],
        out: hdr_b,
    });
}

/// Clamp all values in `data` to `[min, max]` using SIMD.
pub(crate) fn clamp_simd(data: &mut [f32], min: f32, max: f32) {
    let arch = Arch::new();
    arch.dispatch(ClampOp { data, min, max });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_apply_gain_simd() {
        let n = 37; // non-aligned length
        let r_lin: Vec<f32> = (0..n).map(|i| i as f32 * 0.01).collect();
        let g_lin: Vec<f32> = (0..n).map(|i| i as f32 * 0.02).collect();
        let b_lin: Vec<f32> = (0..n).map(|i| i as f32 * 0.03).collect();
        let factor_r: Vec<f32> = (0..n).map(|i| 1.0 + i as f32 * 0.1).collect();
        let factor_g: Vec<f32> = (0..n).map(|i| 1.5 + i as f32 * 0.05).collect();
        let factor_b: Vec<f32> = (0..n).map(|i| 0.8 + i as f32 * 0.15).collect();
        let offset_sdr = [0.1_f32, 0.2, 0.05];
        let offset_hdr = [0.01_f32, 0.02, 0.03];

        let mut hdr_r = vec![0.0_f32; n];
        let mut hdr_g = vec![0.0_f32; n];
        let mut hdr_b = vec![0.0_f32; n];

        apply_gain_simd(
            &r_lin,
            &g_lin,
            &b_lin,
            &factor_r,
            &factor_g,
            &factor_b,
            &offset_sdr,
            &offset_hdr,
            &mut hdr_r,
            &mut hdr_g,
            &mut hdr_b,
        );

        // Verify against scalar computation
        for i in 0..n {
            let expected_r = (r_lin[i] + offset_sdr[0]) * factor_r[i] - offset_hdr[0];
            let expected_g = (g_lin[i] + offset_sdr[1]) * factor_g[i] - offset_hdr[1];
            let expected_b = (b_lin[i] + offset_sdr[2]) * factor_b[i] - offset_hdr[2];
            assert!(
                (hdr_r[i] - expected_r).abs() < 1e-5,
                "r mismatch at {i}: got {} expected {expected_r}",
                hdr_r[i]
            );
            assert!(
                (hdr_g[i] - expected_g).abs() < 1e-5,
                "g mismatch at {i}: got {} expected {expected_g}",
                hdr_g[i]
            );
            assert!(
                (hdr_b[i] - expected_b).abs() < 1e-5,
                "b mismatch at {i}: got {} expected {expected_b}",
                hdr_b[i]
            );
        }
    }

    #[test]
    fn test_clamp_simd() {
        let mut data: Vec<f32> = vec![-1.0, 0.0, 0.5, 1.0, 1.5, 2.0, -0.5, 0.3, 0.7, 1.2, 3.0];
        let expected: Vec<f32> = data.iter().map(|&x| x.clamp(0.0, 1.0)).collect();

        clamp_simd(&mut data, 0.0, 1.0);

        for (i, (&got, &exp)) in data.iter().zip(expected.iter()).enumerate() {
            assert!(
                (got - exp).abs() < 1e-7,
                "clamp mismatch at {i}: got {got} expected {exp}"
            );
        }
    }

    #[test]
    fn test_apply_gain_simd_edge_cases() {
        let offset_sdr = [0.1_f32, 0.2, 0.3];
        let offset_hdr = [0.01_f32, 0.02, 0.03];

        // Empty slices (n=0)
        {
            let mut hdr_r = vec![];
            let mut hdr_g = vec![];
            let mut hdr_b = vec![];
            apply_gain_simd(
                &[],
                &[],
                &[],
                &[],
                &[],
                &[],
                &offset_sdr,
                &offset_hdr,
                &mut hdr_r,
                &mut hdr_g,
                &mut hdr_b,
            );
        }

        // Single element (n=1)
        {
            let r = [0.5_f32];
            let g = [0.3_f32];
            let b = [0.7_f32];
            let fr = [2.0_f32];
            let fg = [1.5_f32];
            let fb = [0.8_f32];
            let mut hdr_r = vec![0.0_f32; 1];
            let mut hdr_g = vec![0.0_f32; 1];
            let mut hdr_b = vec![0.0_f32; 1];

            apply_gain_simd(
                &r,
                &g,
                &b,
                &fr,
                &fg,
                &fb,
                &offset_sdr,
                &offset_hdr,
                &mut hdr_r,
                &mut hdr_g,
                &mut hdr_b,
            );

            let expected_r = (0.5 + 0.1) * 2.0 - 0.01;
            let expected_g = (0.3 + 0.2) * 1.5 - 0.02;
            let expected_b = (0.7 + 0.3) * 0.8 - 0.03;
            assert!((hdr_r[0] - expected_r).abs() < 1e-5);
            assert!((hdr_g[0] - expected_g).abs() < 1e-5);
            assert!((hdr_b[0] - expected_b).abs() < 1e-5);
        }

        // Aligned length (n=16, typical SIMD register width for f32)
        {
            let n = 16;
            let r: Vec<f32> = (0..n).map(|i| i as f32 * 0.1).collect();
            let g: Vec<f32> = (0..n).map(|i| i as f32 * 0.2).collect();
            let b: Vec<f32> = (0..n).map(|i| i as f32 * 0.05).collect();
            let fr: Vec<f32> = vec![1.5; n];
            let fg: Vec<f32> = vec![2.0; n];
            let fb: Vec<f32> = vec![0.5; n];
            let mut hdr_r = vec![0.0_f32; n];
            let mut hdr_g = vec![0.0_f32; n];
            let mut hdr_b = vec![0.0_f32; n];

            apply_gain_simd(
                &r,
                &g,
                &b,
                &fr,
                &fg,
                &fb,
                &offset_sdr,
                &offset_hdr,
                &mut hdr_r,
                &mut hdr_g,
                &mut hdr_b,
            );

            for i in 0..n {
                let expected_r = (r[i] + offset_sdr[0]) * fr[i] - offset_hdr[0];
                let expected_g = (g[i] + offset_sdr[1]) * fg[i] - offset_hdr[1];
                let expected_b = (b[i] + offset_sdr[2]) * fb[i] - offset_hdr[2];
                assert!((hdr_r[i] - expected_r).abs() < 1e-5, "aligned r[{i}]");
                assert!((hdr_g[i] - expected_g).abs() < 1e-5, "aligned g[{i}]");
                assert!((hdr_b[i] - expected_b).abs() < 1e-5, "aligned b[{i}]");
            }
        }
    }
}
