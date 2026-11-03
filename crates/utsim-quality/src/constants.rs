//! Control chart constants, computed (not copied from a table).
//!
//! For a normal sample of size `n` with σ = 1, the range `W = max − min` has mean `d2` and
//! standard deviation `d3`, and the sample standard deviation has mean `c4`:
//!
//! - `d2(n) = ∫ [1 − Φ(x)ⁿ − (1 − Φ(x))ⁿ] dx` (expected range, a single integral);
//! - `E[W²] = ∫₀^∞ 2w [1 − F_W(w)] dw` with `F_W(w) = n ∫ φ(x) [Φ(x + w) − Φ(x)]ⁿ⁻¹ dx`,
//!   and `d3 = √(E[W²] − d2²)` (a double integral, Simpson on a shared grid);
//! - `c4(n) = √(2/(n−1)) · Γ(n/2) / Γ((n−1)/2)` (closed form).
//!
//! The chart factors follow from them: `A2 = 3/(d2√n)`, `A3 = 3/(c4√n)`,
//! `D3 = max(0, 1 − 3d3/d2)`, `D4 = 1 + 3d3/d2`, `B3 = max(0, 1 − 3√(1−c4²)/c4)`,
//! `B4 = 1 + 3√(1−c4²)/c4`, and, for a known σ, `A = 3/√n`, `D1 = max(0, d2 − 3d3)`,
//! `D2 = d2 + 3d3`, `B5 = max(0, c4 − 3√(1−c4²))`, `B6 = c4 + 3√(1−c4²)`.
//! The tests compare them with the published table.

use utsim_stats::special::{ln_gamma, norm_cdf};

use crate::prob::{simpson, simpson_weights};

/// Every factor of a subgroup size `n ≥ 2`, named as in the tables.
#[allow(non_snake_case)]
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Constants {
    pub n: usize,
    pub d2: f64,
    pub d3: f64,
    pub c4: f64,
    pub A: f64,
    pub A2: f64,
    pub A3: f64,
    pub B3: f64,
    pub B4: f64,
    pub B5: f64,
    pub B6: f64,
    pub D1: f64,
    pub D2: f64,
    pub D3: f64,
    pub D4: f64,
}

impl Constants {
    /// All factors for subgroup size `n` (at least 2).
    pub fn new(n: usize) -> Self {
        assert!(n >= 2, "constantes de carta exigem subgrupo com n ≥ 2");
        let (d2, d3, c4) = (d2(n), d3(n), c4(n));
        let rn = (n as f64).sqrt();
        let s = (1.0 - c4 * c4).sqrt();
        Constants {
            n,
            d2,
            d3,
            c4,
            A: 3.0 / rn,
            A2: 3.0 / (d2 * rn),
            A3: 3.0 / (c4 * rn),
            B3: (1.0 - 3.0 * s / c4).max(0.0),
            B4: 1.0 + 3.0 * s / c4,
            B5: (c4 - 3.0 * s).max(0.0),
            B6: c4 + 3.0 * s,
            D1: (d2 - 3.0 * d3).max(0.0),
            D2: d2 + 3.0 * d3,
            D3: (1.0 - 3.0 * d3 / d2).max(0.0),
            D4: 1.0 + 3.0 * d3 / d2,
        }
    }
}

/// Expected range of `n` standard normal values (`n ≥ 2`; 0 for `n < 2`).
pub fn d2(n: usize) -> f64 {
    if n < 2 {
        return 0.0;
    }
    let k = n as i32;
    // The integrand is symmetric around 0.
    2.0 * simpson(
        |x| {
            let p = norm_cdf(x);
            1.0 - p.powi(k) - (1.0 - p).powi(k)
        },
        0.0,
        12.0,
        4800,
    )
}

/// Second moment of the range of `n` standard normal values.
fn range_second_moment(n: usize) -> f64 {
    let h = 0.02;
    let (x0, nx) = (-9.0, 900); // x in [-9, 9]
    let nw = 700; // w in [0, 14]
    let grid: Vec<f64> = (0..=nx + nw).map(|i| norm_cdf(x0 + h * i as f64)).collect();
    let phi: Vec<f64> = (0..=nx)
        .map(|i| {
            let x = x0 + h * i as f64;
            (-0.5 * x * x).exp() / (2.0 * std::f64::consts::PI).sqrt()
        })
        .collect();
    let wx = simpson_weights(nx, h);
    let ww = simpson_weights(nw, h);
    let k = n as i32 - 1;
    let mut total = 0.0;
    for (j, &wwj) in ww.iter().enumerate() {
        let w = h * j as f64;
        let cdf: f64 = (0..=nx)
            .map(|i| wx[i] * phi[i] * (grid[i + j] - grid[i]).powi(k))
            .sum::<f64>()
            * n as f64;
        total += wwj * 2.0 * w * (1.0 - cdf.min(1.0));
    }
    total
}

/// Standard deviation of the range of `n` standard normal values (`n ≥ 2`).
pub fn d3(n: usize) -> f64 {
    if n < 2 {
        return 0.0;
    }
    let m = d2(n);
    (range_second_moment(n) - m * m).max(0.0).sqrt()
}

/// Mean of the sample standard deviation of `n` standard normal values (`n ≥ 2`).
pub fn c4(n: usize) -> f64 {
    if n < 2 {
        return 0.0;
    }
    let nf = n as f64;
    (2.0 / (nf - 1.0)).sqrt() * (ln_gamma(nf / 2.0) - ln_gamma((nf - 1.0) / 2.0)).exp()
}

/// `d2*` of the average range of `g` ranges of size `m` (Duncan's approximation used in
/// gauge studies): `d2* = √(d2² + d3²/g)`, which tends to `d2` as `g` grows. For `m = 2`,
/// `g = 1` it is exactly `√2` (`E[W²] = 2`).
pub fn d2_star(m: usize, g: usize) -> f64 {
    let (a, b) = (d2(m), d3(m));
    (a * a + b * b / g.max(1) as f64).sqrt()
}

#[cfg(test)]
mod tests {
    use super::*;

    // Factors for variables control charts, n = 2..10, 15, 20, 25, as printed (three or
    // four decimals) in the NIST/SEMATECH e-Handbook of Statistical Methods, section
    // 6.3.2 ("What are variables control charts?", tables of A2, D3, D4, c4, B3, B4) and in
    // ASTM's Manual on Presentation of Data and Control Chart Analysis; the same values are
    // in the appendix of every statistical quality control textbook. [conferir a tabela
    // impressa da seção 6.3.2 antes de cada versão]
    // columns: n, d2, d3, c4, A2, A3, B3, B4, D3, D4
    type Row = (usize, f64, f64, f64, f64, f64, f64, f64, f64, f64);
    #[rustfmt::skip]
    const TABLE: [Row; 12] = [
        ( 2, 1.128, 0.853, 0.7979, 1.880, 2.659, 0.0,   3.267, 0.0,   3.267),
        ( 3, 1.693, 0.888, 0.8862, 1.023, 1.954, 0.0,   2.568, 0.0,   2.574),
        ( 4, 2.059, 0.880, 0.9213, 0.729, 1.628, 0.0,   2.266, 0.0,   2.282),
        ( 5, 2.326, 0.864, 0.9400, 0.577, 1.427, 0.0,   2.089, 0.0,   2.114),
        ( 6, 2.534, 0.848, 0.9515, 0.483, 1.287, 0.030, 1.970, 0.0,   2.004),
        ( 7, 2.704, 0.833, 0.9594, 0.419, 1.182, 0.118, 1.882, 0.076, 1.924),
        ( 8, 2.847, 0.820, 0.9650, 0.373, 1.099, 0.185, 1.815, 0.136, 1.864),
        ( 9, 2.970, 0.808, 0.9693, 0.337, 1.032, 0.239, 1.761, 0.184, 1.816),
        (10, 3.078, 0.797, 0.9727, 0.308, 0.975, 0.284, 1.716, 0.223, 1.777),
        (15, 3.472, 0.756, 0.9823, 0.223, 0.789, 0.428, 1.572, 0.347, 1.653),
        (20, 3.735, 0.729, 0.9869, 0.180, 0.680, 0.510, 1.490, 0.415, 1.585),
        (25, 3.931, 0.708, 0.9896, 0.153, 0.606, 0.565, 1.435, 0.459, 1.541),
    ];

    /// Within 1.5 units of the last printed digit (the table was computed with rounded
    /// intermediate values, so the last digit can differ by one).
    fn close(got: f64, want: f64, decimals: i32) -> bool {
        (got - want).abs() <= 1.5 * 10f64.powi(-decimals)
    }

    #[test]
    fn computed_constants_match_the_table() {
        for &(n, d2v, d3v, c4v, a2, a3, b3, b4, dd3, dd4) in &TABLE {
            let c = Constants::new(n);
            let checks = [
                ("d2", c.d2, d2v, 3),
                ("d3", c.d3, d3v, 3),
                ("c4", c.c4, c4v, 4),
                ("A2", c.A2, a2, 3),
                ("A3", c.A3, a3, 3),
                ("B3", c.B3, b3, 3),
                ("B4", c.B4, b4, 3),
                ("D3", c.D3, dd3, 3),
                ("D4", c.D4, dd4, 3),
            ];
            for (name, got, want, dec) in checks {
                assert!(
                    close(got, want, dec),
                    "n = {n}: {name} = {got}, tabela {want}"
                );
            }
        }
    }

    #[test]
    fn exact_values() {
        // d2(2) = 2/√π, c4(2) = √(2/π), E[W²] = 2 for n = 2 (W = |X1 − X2| ~ σ√2 |Z|).
        assert!((d2(2) - 2.0 / std::f64::consts::PI.sqrt()).abs() < 1e-10);
        assert!((c4(2) - (2.0 / std::f64::consts::PI).sqrt()).abs() < 1e-12);
        assert!((range_second_moment(2) - 2.0).abs() < 1e-8);
        assert!((d2_star(2, 1) - 2f64.sqrt()).abs() < 1e-8);
        // d2(3) = 3/√π.
        assert!((d2(3) - 3.0 / std::f64::consts::PI.sqrt()).abs() < 1e-10);
        // d2* decreases to d2.
        assert!(d2_star(3, 1) > d2_star(3, 5) && d2_star(3, 5) > d2(3));
        assert!((d2_star(3, 100_000) - d2(3)).abs() < 1e-5);
    }

    #[test]
    fn known_sigma_factors_agree_with_the_estimated_ones() {
        // D3·R̄ = D1·σ̂ and D4·R̄ = D2·σ̂ when σ̂ = R̄/d2; same for B3, B4 and B5, B6.
        for n in [2, 5, 9] {
            let c = Constants::new(n);
            assert!((c.D3 * c.d2 - c.D1).abs() < 1e-12);
            assert!((c.D4 * c.d2 - c.D2).abs() < 1e-12);
            assert!((c.B3 * c.c4 - c.B5).abs() < 1e-12);
            assert!((c.B4 * c.c4 - c.B6).abs() < 1e-12);
            assert!((c.A2 * c.d2 - c.A).abs() < 1e-12);
        }
    }
}
