use criterion::{Criterion, black_box, criterion_group, criterion_main};
use rand::rngs::SmallRng;
use rand::{Rng, SeedableRng};

use ultrahdr::color::transfer::{
    hlg_inv_ootf_approx_lut, hlg_oetf, hlg_oetf_lut, pq_oetf, pq_oetf_lut, srgb_inv_oetf,
    srgb_inv_oetf_lut,
};

// ---------------------------------------------------------------------------
// Polynomial approximations (Chebyshev-fitted, Horner evaluation)
// ---------------------------------------------------------------------------

/// Horner evaluation: coeffs[0] + x*(coeffs[1] + x*(coeffs[2] + ...))
#[inline(always)]
fn horner(x: f32, coeffs: &[f32]) -> f32 {
    let mut acc = coeffs[coeffs.len() - 1];
    for &c in coeffs[..coeffs.len() - 1].iter().rev() {
        acc = acc.mul_add(x, c);
    }
    acc
}

// -- PQ OETF polynomial: result = poly(e.powf(0.25)), degree 12 --
// max_err=7.92e-04 over [0,1]
const PQ_POLY_COEFFS: [f32; 13] = [
    7.9182636634e-04,
    -3.1570100688e-01,
    4.0564559477e+01,
    -3.7115377247e+02,
    2.0786646686e+03,
    -7.9149884384e+03,
    2.1042845643e+04,
    -3.9340736668e+04,
    5.1400947469e+04,
    -4.5886917414e+04,
    2.6647034801e+04,
    -9.0651249529e+03,
    1.3701790501e+03,
];

#[inline(always)]
fn pq_oetf_poly(e: f32) -> f32 {
    if e <= 0.0 {
        return 0.0;
    }
    let x = e.sqrt().sqrt(); // e^0.25
    horner(x, &PQ_POLY_COEFFS).clamp(0.0, 1.0)
}

// -- HLG OETF polynomial: result = poly(e.sqrt()), degree 10 --
// max_err=2.44e-03 over [0,1]
const HLG_POLY_COEFFS: [f32; 11] = [
    1.0176642561e-03,
    1.4877581905e+00,
    9.3736433566e+00,
    -1.3505532514e+02,
    9.5337155812e+02,
    -3.7169950285e+03,
    8.5111345499e+03,
    -1.1801944865e+04,
    9.7742297606e+03,
    -4.4559201898e+03,
    8.6131770399e+02,
];

#[inline(always)]
fn hlg_oetf_poly(e: f32) -> f32 {
    if e <= 0.0 {
        return 0.0;
    }
    let x = e.sqrt();
    horner(x, &HLG_POLY_COEFFS).clamp(0.0, 1.0)
}

// -- HLG inv OOTF polynomial: result = poly(x.sqrt()), degree 8 --
// Approximates x^(1/1.2), max_err=2.04e-04 over [0,1]
const INV_OOTF_POLY_COEFFS: [f32; 9] = [
    -2.0421011598e-04,
    5.0760609641e-02,
    1.9724522293e+00,
    -3.8423949663e+00,
    8.6082146026e+00,
    -1.3000290598e+01,
    1.2045463472e+01,
    -6.1674075432e+00,
    1.3334155372e+00,
];

#[inline(always)]
fn hlg_inv_ootf_poly(x: f32) -> f32 {
    if x <= 0.0 {
        return 0.0;
    }
    let t = x.sqrt();
    horner(t, &INV_OOTF_POLY_COEFFS).clamp(0.0, 1.0)
}

// -- sRGB inv OETF polynomial: result = poly(x), degree 8 --
// max_err=1.91e-04 over [0,1]
const SRGB_POLY_COEFFS: [f32; 9] = [
    1.3895814630e-04,
    5.7069391634e-02,
    2.5255557709e-01,
    2.0825589198e+00,
    -4.4278938419e+00,
    7.0321846013e+00,
    -6.7782359174e+00,
    3.5687860787e+00,
    -7.8716964705e-01,
];

#[inline(always)]
fn srgb_inv_oetf_poly(x: f32) -> f32 {
    horner(x, &SRGB_POLY_COEFFS).clamp(0.0, 1.0)
}

// ---------------------------------------------------------------------------
// Batch polynomial helpers (SOA layout for autovectorization)
// ---------------------------------------------------------------------------

fn pq_oetf_poly_batch(input: &[f32], output: &mut [f32]) {
    for (o, &e) in output.iter_mut().zip(input.iter()) {
        *o = pq_oetf_poly(e);
    }
}

fn hlg_oetf_poly_batch(input: &[f32], output: &mut [f32]) {
    for (o, &e) in output.iter_mut().zip(input.iter()) {
        *o = hlg_oetf_poly(e);
    }
}

fn hlg_inv_ootf_poly_batch(input: &[f32], output: &mut [f32]) {
    for (o, &x) in output.iter_mut().zip(input.iter()) {
        *o = hlg_inv_ootf_poly(x);
    }
}

fn srgb_inv_oetf_poly_batch(input: &[f32], output: &mut [f32]) {
    for (o, &x) in output.iter_mut().zip(input.iter()) {
        *o = srgb_inv_oetf_poly(x);
    }
}

// ---------------------------------------------------------------------------
// Test data generation
// ---------------------------------------------------------------------------

fn random_f32_01(n: usize) -> Vec<f32> {
    let mut rng = SmallRng::seed_from_u64(42);
    (0..n).map(|_| rng.r#gen::<f32>()).collect()
}

fn random_u8(n: usize) -> Vec<u8> {
    let mut rng = SmallRng::seed_from_u64(42);
    (0..n).map(|_| rng.r#gen::<u8>()).collect()
}

const BATCH: usize = 1000;

// ---------------------------------------------------------------------------
// Accuracy measurement (printed at start)
// ---------------------------------------------------------------------------

fn measure_accuracy() {
    let n = 65536;
    let vals: Vec<f32> = (0..n).map(|i| i as f32 / (n - 1) as f32).collect();

    // PQ OETF
    let (mut max_e, mut sum_e) = (0.0f32, 0.0f64);
    for &v in &vals {
        let err = (pq_oetf_poly(v) - pq_oetf(v)).abs();
        max_e = max_e.max(err);
        sum_e += err as f64;
    }
    eprintln!(
        "PQ poly   accuracy: max_err={max_e:.6e}, mean_err={:.6e}",
        sum_e / n as f64
    );

    let (mut max_e, mut sum_e) = (0.0f32, 0.0f64);
    for &v in &vals {
        let err = (pq_oetf_lut(v) - pq_oetf(v)).abs();
        max_e = max_e.max(err);
        sum_e += err as f64;
    }
    eprintln!(
        "PQ LUT    accuracy: max_err={max_e:.6e}, mean_err={:.6e}",
        sum_e / n as f64
    );

    // HLG OETF
    let (mut max_e, mut sum_e) = (0.0f32, 0.0f64);
    for &v in &vals {
        let err = (hlg_oetf_poly(v) - hlg_oetf(v)).abs();
        max_e = max_e.max(err);
        sum_e += err as f64;
    }
    eprintln!(
        "HLG poly  accuracy: max_err={max_e:.6e}, mean_err={:.6e}",
        sum_e / n as f64
    );

    let (mut max_e, mut sum_e) = (0.0f32, 0.0f64);
    for &v in &vals {
        let err = (hlg_oetf_lut(v) - hlg_oetf(v)).abs();
        max_e = max_e.max(err);
        sum_e += err as f64;
    }
    eprintln!(
        "HLG LUT   accuracy: max_err={max_e:.6e}, mean_err={:.6e}",
        sum_e / n as f64
    );

    // HLG inv OOTF
    let inv_gamma = 1.0f32 / 1.2;
    let (mut max_e, mut sum_e) = (0.0f32, 0.0f64);
    for &v in &vals {
        if v <= 0.0 {
            continue;
        }
        let err = (hlg_inv_ootf_poly(v) - v.powf(inv_gamma)).abs();
        max_e = max_e.max(err);
        sum_e += err as f64;
    }
    eprintln!(
        "OOTF poly accuracy: max_err={max_e:.6e}, mean_err={:.6e}",
        sum_e / n as f64
    );

    let (mut max_e, mut sum_e) = (0.0f32, 0.0f64);
    for &v in &vals {
        if v <= 0.0 {
            continue;
        }
        let err = (hlg_inv_ootf_approx_lut(v) - v.powf(inv_gamma)).abs();
        max_e = max_e.max(err);
        sum_e += err as f64;
    }
    eprintln!(
        "OOTF LUT  accuracy: max_err={max_e:.6e}, mean_err={:.6e}",
        sum_e / n as f64
    );

    // sRGB inv OETF
    let (mut max_e, mut sum_e) = (0.0f32, 0.0f64);
    for &v in &vals {
        let err = (srgb_inv_oetf_poly(v) - srgb_inv_oetf(v)).abs();
        max_e = max_e.max(err);
        sum_e += err as f64;
    }
    eprintln!(
        "sRGB poly accuracy: max_err={max_e:.6e}, mean_err={:.6e}",
        sum_e / n as f64
    );

    eprintln!();
}

// ---------------------------------------------------------------------------
// Benchmarks
// ---------------------------------------------------------------------------

fn bench_pq_oetf(c: &mut Criterion) {
    measure_accuracy();

    let data = random_f32_01(BATCH);
    let mut out = vec![0.0f32; BATCH];

    let mut g = c.benchmark_group("pq_oetf");

    // Single value
    g.bench_function("scalar_single", |b| {
        b.iter(|| black_box(pq_oetf(black_box(0.5))))
    });
    g.bench_function("lut_single", |b| {
        // warm up LUT
        let _ = pq_oetf_lut(0.0);
        b.iter(|| black_box(pq_oetf_lut(black_box(0.5))))
    });
    g.bench_function("poly_single", |b| {
        b.iter(|| black_box(pq_oetf_poly(black_box(0.5))))
    });

    // Batch
    g.bench_function("scalar_batch_1000", |b| {
        b.iter(|| {
            for (o, &v) in out.iter_mut().zip(data.iter()) {
                *o = pq_oetf(v);
            }
            black_box(&out);
        })
    });
    g.bench_function("lut_batch_1000", |b| {
        let _ = pq_oetf_lut(0.0);
        b.iter(|| {
            for (o, &v) in out.iter_mut().zip(data.iter()) {
                *o = pq_oetf_lut(v);
            }
            black_box(&out);
        })
    });
    g.bench_function("poly_batch_1000", |b| {
        b.iter(|| {
            pq_oetf_poly_batch(&data, &mut out);
            black_box(&out);
        })
    });

    g.finish();
}

fn bench_hlg_oetf(c: &mut Criterion) {
    let data = random_f32_01(BATCH);
    let mut out = vec![0.0f32; BATCH];

    let mut g = c.benchmark_group("hlg_oetf");

    g.bench_function("scalar_single", |b| {
        b.iter(|| black_box(hlg_oetf(black_box(0.5))))
    });
    g.bench_function("lut_single", |b| {
        let _ = hlg_oetf_lut(0.0);
        b.iter(|| black_box(hlg_oetf_lut(black_box(0.5))))
    });
    g.bench_function("poly_single", |b| {
        b.iter(|| black_box(hlg_oetf_poly(black_box(0.5))))
    });

    g.bench_function("scalar_batch_1000", |b| {
        b.iter(|| {
            for (o, &v) in out.iter_mut().zip(data.iter()) {
                *o = hlg_oetf(v);
            }
            black_box(&out);
        })
    });
    g.bench_function("lut_batch_1000", |b| {
        let _ = hlg_oetf_lut(0.0);
        b.iter(|| {
            for (o, &v) in out.iter_mut().zip(data.iter()) {
                *o = hlg_oetf_lut(v);
            }
            black_box(&out);
        })
    });
    g.bench_function("poly_batch_1000", |b| {
        b.iter(|| {
            hlg_oetf_poly_batch(&data, &mut out);
            black_box(&out);
        })
    });

    g.finish();
}

fn bench_hlg_inv_ootf(c: &mut Criterion) {
    let data = random_f32_01(BATCH);
    let mut out = vec![0.0f32; BATCH];
    let inv_gamma = 1.0f32 / 1.2;

    let mut g = c.benchmark_group("hlg_inv_ootf");

    g.bench_function("scalar_single", |b| {
        b.iter(|| black_box(black_box(0.5f32).powf(inv_gamma)))
    });
    g.bench_function("lut_single", |b| {
        let _ = hlg_inv_ootf_approx_lut(0.0);
        b.iter(|| black_box(hlg_inv_ootf_approx_lut(black_box(0.5))))
    });
    g.bench_function("poly_single", |b| {
        b.iter(|| black_box(hlg_inv_ootf_poly(black_box(0.5))))
    });

    g.bench_function("scalar_batch_1000", |b| {
        b.iter(|| {
            for (o, &v) in out.iter_mut().zip(data.iter()) {
                *o = v.powf(inv_gamma);
            }
            black_box(&out);
        })
    });
    g.bench_function("lut_batch_1000", |b| {
        let _ = hlg_inv_ootf_approx_lut(0.0);
        b.iter(|| {
            for (o, &v) in out.iter_mut().zip(data.iter()) {
                *o = hlg_inv_ootf_approx_lut(v);
            }
            black_box(&out);
        })
    });
    g.bench_function("poly_batch_1000", |b| {
        b.iter(|| {
            hlg_inv_ootf_poly_batch(&data, &mut out);
            black_box(&out);
        })
    });

    g.finish();
}

fn bench_srgb_inv_oetf(c: &mut Criterion) {
    let data_f32 = random_f32_01(BATCH);
    let data_u8 = random_u8(BATCH);
    let mut out = vec![0.0f32; BATCH];

    let mut g = c.benchmark_group("srgb_inv_oetf");

    g.bench_function("scalar_single", |b| {
        b.iter(|| black_box(srgb_inv_oetf(black_box(0.5))))
    });
    g.bench_function("lut_single", |b| {
        let _ = srgb_inv_oetf_lut(0);
        b.iter(|| black_box(srgb_inv_oetf_lut(black_box(128))))
    });
    g.bench_function("poly_single", |b| {
        b.iter(|| black_box(srgb_inv_oetf_poly(black_box(0.5))))
    });

    g.bench_function("scalar_batch_1000", |b| {
        b.iter(|| {
            for (o, &v) in out.iter_mut().zip(data_f32.iter()) {
                *o = srgb_inv_oetf(v);
            }
            black_box(&out);
        })
    });
    g.bench_function("lut_batch_1000", |b| {
        let _ = srgb_inv_oetf_lut(0);
        b.iter(|| {
            for (o, &v) in out.iter_mut().zip(data_u8.iter()) {
                *o = srgb_inv_oetf_lut(v);
            }
            black_box(&out);
        })
    });
    g.bench_function("poly_batch_1000", |b| {
        b.iter(|| {
            srgb_inv_oetf_poly_batch(&data_f32, &mut out);
            black_box(&out);
        })
    });

    g.finish();
}

criterion_group!(
    benches,
    bench_pq_oetf,
    bench_hlg_oetf,
    bench_hlg_inv_ootf,
    bench_srgb_inv_oetf,
);
criterion_main!(benches);
