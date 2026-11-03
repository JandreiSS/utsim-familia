//! Distribution of the studentized range, the basis of Tukey's HSD test.
//!
//! For `k` independent standard normal values and an independent estimate `s` of their
//! standard deviation with `df` degrees of freedom (`df·s²` is chi-square), the
//! studentized range is `Q = (max - min) / s`. Its CDF is a double integral:
//!
//! - with a known deviation (`df = ∞`), `P∞(w; k) = k ∫ φ(z) [Φ(z) - Φ(z - w)]^(k-1) dz`;
//! - with `df` finite, `P(q; k, df) = ∫₀^∞ g(s) P∞(q·s; k) ds`, where `g` is the density
//!   of `s = χ_df / √df`: `g(s) = df^(df/2) / (Γ(df/2) 2^(df/2 - 1)) s^(df-1) e^(-df s²/2)`.
//!
//! Both integrals are computed here by composite Gauss-Legendre quadrature (16 nodes per
//! panel), and the quantile by Brent's method on the CDF. The tests check the quantiles
//! against the published tables of the studentized range and the exact identity for two
//! groups, `q(p; 2, df) = √2 · t((1 + p) / 2; df)`.

use std::f64::consts::PI;
use std::sync::OnceLock;

use crate::special::{brent, ln_gamma, norm_cdf};

/// Nodes per Gauss-Legendre panel.
const NODES: usize = 16;
/// Panels of the inner integral over `z ∈ [-Z_MAX, Z_MAX]`.
const INNER_PANELS: usize = 12;
/// Panels of the outer integral over `s`.
const OUTER_PANELS: usize = 12;
/// Where the normal density is negligible (φ(8.5) ≈ 1e-16).
const Z_MAX: f64 = 8.5;
/// Ranges where `P∞(w; k)` changes fastest, used as extra cuts of the outer integral.
const RISE: [f64; 10] = [0.25, 0.5, 1.0, 1.5, 2.0, 3.0, 4.0, 5.0, 6.5, 8.0];
/// Above this many degrees of freedom the deviation is taken as known.
const DF_INFINITE: f64 = 1e6;

/// Gauss-Legendre nodes and weights on `[-1, 1]` (Newton iteration on `P_n`).
fn gauss_legendre() -> &'static [(f64, f64); NODES] {
    static GL: OnceLock<[(f64, f64); NODES]> = OnceLock::new();
    GL.get_or_init(|| {
        let n = NODES;
        let mut out = [(0.0, 0.0); NODES];
        for (i, slot) in out.iter_mut().enumerate() {
            // Initial guess (Tricomi), then Newton on the Legendre polynomial.
            let mut x = (PI * (i as f64 + 0.75) / (n as f64 + 0.5)).cos();
            let mut dp = 1.0;
            for _ in 0..100 {
                let (mut p0, mut p1) = (1.0, x);
                for j in 2..=n {
                    let j = j as f64;
                    let p2 = ((2.0 * j - 1.0) * x * p1 - (j - 1.0) * p0) / j;
                    p0 = p1;
                    p1 = p2;
                }
                dp = n as f64 * (x * p1 - p0) / (x * x - 1.0);
                let dx = p1 / dp;
                x -= dx;
                if dx.abs() < 1e-16 {
                    break;
                }
            }
            *slot = (x, 2.0 / ((1.0 - x * x) * dp * dp));
        }
        out
    })
}

/// `∫ f` over `[a, b]` with `panels` Gauss-Legendre panels.
fn integrate(f: impl Fn(f64) -> f64, a: f64, b: f64, panels: usize) -> f64 {
    let gl = gauss_legendre();
    let h = (b - a) / panels as f64;
    let mut total = 0.0;
    for p in 0..panels {
        let mid = a + h * (p as f64 + 0.5);
        let half = 0.5 * h;
        total += half * gl.iter().map(|(x, w)| w * f(mid + half * x)).sum::<f64>();
    }
    total
}

/// The fixed `z` grid of the inner integral: `(z, weight · φ(z), Φ(z))`.
fn inner_grid() -> &'static Vec<(f64, f64, f64)> {
    static GRID: OnceLock<Vec<(f64, f64, f64)>> = OnceLock::new();
    GRID.get_or_init(|| {
        let gl = gauss_legendre();
        let h = 2.0 * Z_MAX / INNER_PANELS as f64;
        let mut v = Vec::with_capacity(INNER_PANELS * NODES);
        for p in 0..INNER_PANELS {
            let mid = -Z_MAX + h * (p as f64 + 0.5);
            for (x, w) in gl {
                let z = mid + 0.5 * h * x;
                let phi = (-0.5 * z * z).exp() / (2.0 * PI).sqrt();
                v.push((z, 0.5 * h * w * phi, norm_cdf(z)));
            }
        }
        v
    })
}

/// CDF of the range of `k` standard normal values, `P(max - min ≤ w)`.
pub fn range_cdf(w: f64, k: usize) -> f64 {
    if w.is_nan() || k < 2 {
        return f64::NAN;
    }
    if w <= 0.0 {
        return 0.0;
    }
    if w.is_infinite() {
        return 1.0;
    }
    let e = (k - 1) as i32;
    let total: f64 = inner_grid()
        .iter()
        .map(|&(z, wphi, cdf)| {
            let d = cdf - norm_cdf(z - w);
            if d > 0.0 { wphi * d.powi(e) } else { 0.0 }
        })
        .sum();
    (k as f64 * total).clamp(0.0, 1.0)
}

/// CDF of the studentized range: `P(Q ≤ q)` for `k ≥ 2` groups and `df > 0` degrees
/// of freedom of the deviation (`f64::INFINITY` for a known deviation).
pub fn ptukey(q: f64, k: usize, df: f64) -> f64 {
    if q.is_nan() || df.is_nan() || df <= 0.0 || k < 2 {
        return f64::NAN;
    }
    if q <= 0.0 {
        return 0.0;
    }
    if q.is_infinite() {
        return 1.0;
    }
    if df >= DF_INFINITE {
        return range_cdf(q, k);
    }
    // s = χ_df / √df has mean ≈ 1 and deviation ≈ 1/√(2 df); the window below holds
    // all but a negligible tail for every df (also the half-normal of df = 1).
    let spread = 9.0 / df.sqrt();
    let (lo, hi) = ((1.0 - spread / 2f64.sqrt()).max(0.0), 1.0 + spread);
    let ln_c = 0.5 * df * df.ln() - ln_gamma(0.5 * df) - (0.5 * df - 1.0) * 2f64.ln();
    let density = |s: f64| {
        if s <= 0.0 {
            return if df == 1.0 { ln_c.exp() } else { 0.0 };
        }
        (ln_c + (df - 1.0) * s.ln() - 0.5 * df * s * s).exp()
    };
    // Panels: an even grid, refined where the range `q·s` crosses the values over which
    // `P∞` rises (for a large `q` and few degrees of freedom that is very close to 0).
    let mut cuts: Vec<f64> = (0..=OUTER_PANELS)
        .map(|i| lo + (hi - lo) * i as f64 / OUTER_PANELS as f64)
        .chain(RISE.iter().map(|w| w / q).filter(|s| *s > lo && *s < hi))
        .collect();
    cuts.sort_by(f64::total_cmp);
    cuts.dedup();
    let f = |s: f64| density(s) * range_cdf(q * s, k);
    cuts.windows(2)
        .map(|w| integrate(f, w[0], w[1], 1))
        .sum::<f64>()
        .clamp(0.0, 1.0)
}

/// Quantile of the studentized range: the `q` with `ptukey(q, k, df) = p`, for
/// `0 < p < 1` (NaN outside). With `p = 0.95` it is the `q(0.05; k, df)` of the
/// published tables used by Tukey's test.
pub fn qtukey(p: f64, k: usize, df: f64) -> f64 {
    if !(p > 0.0 && p < 1.0) || k < 2 || df.is_nan() || df <= 0.0 {
        return f64::NAN;
    }
    let mut hi = 4.0;
    while ptukey(hi, k, df) < p {
        hi *= 2.0;
        if hi > 1e6 {
            return f64::INFINITY;
        }
    }
    brent(|q| ptukey(q, k, df) - p, 0.0, hi, 1e-10).unwrap_or(f64::NAN)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::special::{norm_ppf, t_ppf};

    #[test]
    fn quadrature_is_exact_for_polynomials() {
        // 16 nodes integrate polynomials up to degree 31 exactly.
        let v = integrate(|x| x.powi(6) - 2.0 * x, 0.0, 2.0, 1);
        assert!((v - (128.0 / 7.0 - 4.0)).abs() < 1e-12, "{v}");
        let w: f64 = gauss_legendre().iter().map(|(_, w)| w).sum();
        assert!((w - 2.0).abs() < 1e-14);
    }

    #[test]
    fn two_groups_are_a_scaled_t() {
        // Exact identity: the range of two values is |X1 - X2|, so
        // q(p; 2, df) = √2 · t((1 + p) / 2; df), and √2 · z for a known deviation.
        for df in [1.0, 2.0, 5.0, 10.0, 30.0, 120.0] {
            for p in [0.90, 0.95, 0.99] {
                let want = 2f64.sqrt() * t_ppf((1.0 + p) / 2.0, df);
                let got = qtukey(p, 2, df);
                assert!(
                    (got - want).abs() < 1e-6 * want,
                    "df {df} p {p}: {got} vs {want}"
                );
            }
        }
        let z = 2f64.sqrt() * norm_ppf(0.975);
        assert!((qtukey(0.95, 2, f64::INFINITY) - z).abs() < 1e-7);
    }

    /// Upper 5% points `q(0.05; k, df)` of the studentized range from the published
    /// tables (Harter, 1960, three decimals; reproduced in Montgomery, *Design and
    /// Analysis of Experiments*, appendix, and in the NIST/SEMATECH e-Handbook,
    /// section 7.4.7.1), as `(k, df, q, decimals)`. [conferir]
    const TABLE_05: [(usize, f64, f64, i32); 16] = [
        (2, 1.0, 17.97, 2),
        (3, 1.0, 26.98, 2),
        (3, 5.0, 4.602, 3),
        (4, 5.0, 5.218, 3),
        (5, 5.0, 5.673, 3),
        (3, 10.0, 3.877, 3),
        (4, 10.0, 4.327, 3),
        (5, 10.0, 4.654, 3),
        (3, 12.0, 3.773, 3),
        (4, 16.0, 4.046, 3),
        (3, 20.0, 3.578, 3),
        (4, 20.0, 3.958, 3),
        (5, 30.0, 4.102, 3),
        (10, 60.0, 4.646, 3),
        (3, f64::INFINITY, 3.314, 3),
        (10, f64::INFINITY, 4.474, 3),
    ];

    #[test]
    fn quantiles_match_the_published_table() {
        for (k, df, want, decimals) in TABLE_05 {
            let got = qtukey(0.95, k, df);
            // Rounded to the decimals of the table, the value is the table's.
            let tol = 0.5 * 10f64.powi(-decimals) + 1e-9;
            assert!((got - want).abs() <= tol, "k {k} df {df}: {got} vs {want}");
        }
        // Upper 1% points (same sources). [conferir]
        for (k, df, want) in [
            (3, 10.0, 5.270),
            (4, 20.0, 5.018),
            (3, f64::INFINITY, 4.120),
        ] {
            let got = qtukey(0.99, k, df);
            assert!(
                (got - want).abs() <= 5e-4 + 1e-9,
                "k {k} df {df}: {got} vs {want}"
            );
        }
    }

    #[test]
    fn cdf_is_a_distribution() {
        assert_eq!(ptukey(0.0, 3, 10.0), 0.0);
        assert_eq!(ptukey(f64::INFINITY, 3, 10.0), 1.0);
        let mut last = 0.0;
        for q in [0.5, 1.0, 2.0, 3.0, 4.0, 6.0, 10.0] {
            let p = ptukey(q, 4, 12.0);
            assert!(p > last && p < 1.0, "q {q}: {p}");
            last = p;
        }
        // Round trip and more groups need a larger range.
        let q = qtukey(0.9, 6, 25.0);
        assert!((ptukey(q, 6, 25.0) - 0.9).abs() < 1e-9);
        assert!(qtukey(0.95, 6, 25.0) > qtukey(0.95, 5, 25.0));
        assert!(ptukey(1.0, 1, 5.0).is_nan() && qtukey(1.0, 3, 5.0).is_nan());
    }
}
