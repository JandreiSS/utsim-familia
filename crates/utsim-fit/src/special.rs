//! Special functions and root finding needed by the maximum-likelihood fits.
//!
//! Self-contained (no external numerics crate). Accuracy is about 1e-13 relative,
//! which is far below the statistical noise of any fit.

use std::f64::consts::PI;

const TINY: f64 = 1e-300;
const EPS: f64 = 1e-16;

/// Natural log of the gamma function (Lanczos approximation, `g = 7`).
pub fn ln_gamma(x: f64) -> f64 {
    const C: [f64; 9] = [
        0.999_999_999_999_809_9,
        676.520_368_121_885_1,
        -1_259.139_216_722_402_8,
        771.323_428_777_653_1,
        -176.615_029_162_140_6,
        12.507_343_278_686_905,
        -0.138_571_095_265_720_12,
        9.984_369_578_019_572e-6,
        1.505_632_735_149_311_6e-7,
    ];
    if x < 0.5 {
        return (PI / (PI * x).sin().abs()).ln() - ln_gamma(1.0 - x);
    }
    let x = x - 1.0;
    let t = x + 7.5;
    let mut a = C[0];
    for (i, c) in C.iter().enumerate().skip(1) {
        a += c / (x + i as f64);
    }
    0.5 * (2.0 * PI).ln() + (x + 0.5) * t.ln() - t + a.ln()
}

/// `ln B(a, b)`.
pub fn ln_beta(a: f64, b: f64) -> f64 {
    ln_gamma(a) + ln_gamma(b) - ln_gamma(a + b)
}

/// `ln C(n, k)` for real arguments with `0 <= k <= n`.
pub fn ln_choose(n: f64, k: f64) -> f64 {
    ln_gamma(n + 1.0) - ln_gamma(k + 1.0) - ln_gamma(n - k + 1.0)
}

/// Digamma function ψ(x) = d/dx ln Γ(x).
pub fn digamma(mut x: f64) -> f64 {
    if x <= 0.0 {
        if x == x.floor() {
            return f64::NAN;
        }
        return digamma(1.0 - x) - PI / (PI * x).tan();
    }
    let mut r = 0.0;
    while x < 10.0 {
        r -= 1.0 / x;
        x += 1.0;
    }
    let f = 1.0 / (x * x);
    r + x.ln()
        - 0.5 / x
        - f * (1.0 / 12.0 - f * (1.0 / 120.0 - f * (1.0 / 252.0 - f * (1.0 / 240.0 - f / 132.0))))
}

/// Trigamma function ψ'(x), for `x > 0`.
pub fn trigamma(mut x: f64) -> f64 {
    if x <= 0.0 {
        return f64::NAN;
    }
    let mut r = 0.0;
    while x < 10.0 {
        r += 1.0 / (x * x);
        x += 1.0;
    }
    let f = 1.0 / (x * x);
    r + 1.0 / x + f / 2.0 + f / x * (1.0 / 6.0 - f * (1.0 / 30.0 - f * (1.0 / 42.0 - f / 30.0)))
}

/// Regularised lower incomplete gamma function `P(a, x)`.
pub fn gamma_p(a: f64, x: f64) -> f64 {
    if x <= 0.0 {
        0.0
    } else if x < a + 1.0 {
        gamma_series(a, x)
    } else {
        1.0 - gamma_cf(a, x)
    }
}

/// Regularised upper incomplete gamma function `Q(a, x) = 1 - P(a, x)`.
pub fn gamma_q(a: f64, x: f64) -> f64 {
    if x <= 0.0 {
        1.0
    } else if x < a + 1.0 {
        1.0 - gamma_series(a, x)
    } else {
        gamma_cf(a, x)
    }
}

fn gamma_series(a: f64, x: f64) -> f64 {
    let mut ap = a;
    let mut term = 1.0 / a;
    let mut sum = term;
    for _ in 0..100_000 {
        ap += 1.0;
        term *= x / ap;
        sum += term;
        if term.abs() < sum.abs() * EPS {
            break;
        }
    }
    sum * (-x + a * x.ln() - ln_gamma(a)).exp()
}

/// Continued fraction for `Q(a, x)` (modified Lentz).
fn gamma_cf(a: f64, x: f64) -> f64 {
    let mut b = x + 1.0 - a;
    let mut c = 1.0 / TINY;
    let mut d = 1.0 / b;
    let mut h = d;
    for i in 1..100_000 {
        let an = -(i as f64) * (i as f64 - a);
        b += 2.0;
        d = an * d + b;
        if d.abs() < TINY {
            d = TINY;
        }
        c = b + an / c;
        if c.abs() < TINY {
            c = TINY;
        }
        d = 1.0 / d;
        let del = d * c;
        h *= del;
        if (del - 1.0).abs() < EPS {
            break;
        }
    }
    (-x + a * x.ln() - ln_gamma(a)).exp() * h
}

/// Regularised incomplete beta function `I_x(a, b)`.
pub fn beta_inc(a: f64, b: f64, x: f64) -> f64 {
    if x <= 0.0 {
        return 0.0;
    }
    if x >= 1.0 {
        return 1.0;
    }
    let ln_front = -ln_beta(a, b) + a * x.ln() + b * (-x).ln_1p();
    let front = ln_front.exp();
    if x < (a + 1.0) / (a + b + 2.0) {
        front * beta_cf(a, b, x) / a
    } else {
        1.0 - front * beta_cf(b, a, 1.0 - x) / b
    }
}

fn beta_cf(a: f64, b: f64, x: f64) -> f64 {
    let qab = a + b;
    let qap = a + 1.0;
    let qam = a - 1.0;
    let mut c = 1.0;
    let mut d = 1.0 - qab * x / qap;
    if d.abs() < TINY {
        d = TINY;
    }
    d = 1.0 / d;
    let mut h = d;
    for m in 1..100_000 {
        let m = m as f64;
        let m2 = 2.0 * m;
        let aa = m * (b - m) * x / ((qam + m2) * (a + m2));
        d = 1.0 + aa * d;
        if d.abs() < TINY {
            d = TINY;
        }
        c = 1.0 + aa / c;
        if c.abs() < TINY {
            c = TINY;
        }
        d = 1.0 / d;
        h *= d * c;
        let aa = -(a + m) * (qab + m) * x / ((a + m2) * (qap + m2));
        d = 1.0 + aa * d;
        if d.abs() < TINY {
            d = TINY;
        }
        c = 1.0 + aa / c;
        if c.abs() < TINY {
            c = TINY;
        }
        d = 1.0 / d;
        let del = d * c;
        h *= del;
        if (del - 1.0).abs() < EPS {
            break;
        }
    }
    h
}

/// Standard normal CDF.
pub fn norm_cdf(x: f64) -> f64 {
    // erfc(z) = Q(1/2, z²), so the tail beyond |x| is Q(1/2, x²/2) / 2.
    let tail = 0.5 * gamma_q(0.5, 0.5 * x * x);
    if x >= 0.0 { 1.0 - tail } else { tail }
}

/// Inverse of the standard normal CDF (Acklam's rational approximation plus one
/// Halley refinement step, accurate to about 1e-15).
pub fn norm_ppf(p: f64) -> f64 {
    if p <= 0.0 {
        return f64::NEG_INFINITY;
    }
    if p >= 1.0 {
        return f64::INFINITY;
    }
    const A: [f64; 6] = [
        -3.969_683_028_665_376e1,
        2.209_460_984_245_205e2,
        -2.759_285_104_469_687e2,
        1.383_577_518_672_69e2,
        -3.066_479_806_614_716e1,
        2.506_628_277_459_239,
    ];
    const B: [f64; 5] = [
        -5.447_609_879_822_406e1,
        1.615_858_368_580_409e2,
        -1.556_989_798_598_866e2,
        6.680_131_188_771_972e1,
        -1.328_068_155_288_572e1,
    ];
    const C: [f64; 6] = [
        -7.784_894_002_430_293e-3,
        -3.223_964_580_411_365e-1,
        -2.400_758_277_161_838,
        -2.549_732_539_343_734,
        4.374_664_141_464_968,
        2.938_163_982_698_783,
    ];
    const D: [f64; 4] = [
        7.784_695_709_041_462e-3,
        3.224_671_290_700_398e-1,
        2.445_134_137_142_996,
        3.754_408_661_907_416,
    ];
    let tail = |q: f64| {
        (((((C[0] * q + C[1]) * q + C[2]) * q + C[3]) * q + C[4]) * q + C[5])
            / ((((D[0] * q + D[1]) * q + D[2]) * q + D[3]) * q + 1.0)
    };
    let x = if p < 0.02425 {
        tail((-2.0 * p.ln()).sqrt())
    } else if p > 1.0 - 0.02425 {
        -tail((-2.0 * (1.0 - p).ln()).sqrt())
    } else {
        let q = p - 0.5;
        let r = q * q;
        (((((A[0] * r + A[1]) * r + A[2]) * r + A[3]) * r + A[4]) * r + A[5]) * q
            / (((((B[0] * r + B[1]) * r + B[2]) * r + B[3]) * r + B[4]) * r + 1.0)
    };
    let e = norm_cdf(x) - p;
    let u = e * (2.0 * PI).sqrt() * (x * x / 2.0).exp();
    x - u / (1.0 + x * u / 2.0)
}

/// Brent's root finder. `f(a)` and `f(b)` must have opposite signs (or one be zero).
pub fn brent(f: impl Fn(f64) -> f64, mut a: f64, mut b: f64, tol: f64) -> Option<f64> {
    let (mut fa, mut fb) = (f(a), f(b));
    if !fa.is_finite() || !fb.is_finite() {
        return None;
    }
    if fa == 0.0 {
        return Some(a);
    }
    if fb == 0.0 {
        return Some(b);
    }
    if fa * fb > 0.0 {
        return None;
    }
    let (mut c, mut fc) = (a, fa);
    let (mut d, mut e) = (b - a, b - a);
    for _ in 0..200 {
        if fb * fc > 0.0 {
            c = a;
            fc = fa;
            d = b - a;
            e = d;
        }
        if fc.abs() < fb.abs() {
            a = b;
            b = c;
            c = a;
            fa = fb;
            fb = fc;
            fc = fa;
        }
        let tol1 = 2.0 * EPS * b.abs() + 0.5 * tol;
        let xm = 0.5 * (c - b);
        if xm.abs() <= tol1 || fb == 0.0 {
            return Some(b);
        }
        if e.abs() >= tol1 && fa.abs() > fb.abs() {
            let s = fb / fa;
            let (mut p, mut q);
            if a == c {
                p = 2.0 * xm * s;
                q = 1.0 - s;
            } else {
                let qq = fa / fc;
                let r = fb / fc;
                p = s * (2.0 * xm * qq * (qq - r) - (b - a) * (r - 1.0));
                q = (qq - 1.0) * (r - 1.0) * (s - 1.0);
            }
            if p > 0.0 {
                q = -q;
            }
            p = p.abs();
            if 2.0 * p < (3.0 * xm * q - (tol1 * q).abs()).min((e * q).abs()) {
                e = d;
                d = p / q;
            } else {
                d = xm;
                e = d;
            }
        } else {
            d = xm;
            e = d;
        }
        a = b;
        fa = fb;
        b += if d.abs() > tol1 { d } else { tol1.copysign(xm) };
        fb = f(b);
        if !fb.is_finite() {
            return None;
        }
    }
    Some(b)
}

/// Root of an increasing function near `x0 > 0`: expands the bracket
/// geometrically (`x0 / 2^i`, `x0 * 2^i`) until the sign changes, then runs Brent.
pub fn root_increasing_positive(
    f: impl Fn(f64) -> f64,
    x0: f64,
    lo_limit: f64,
    hi_limit: f64,
) -> Option<f64> {
    let mut lo = x0;
    let mut hi = x0;
    let f0 = f(x0);
    if !f0.is_finite() {
        return None;
    }
    if f0 > 0.0 {
        while f(lo) > 0.0 {
            lo /= 2.0;
            if lo < lo_limit {
                return None;
            }
        }
    } else {
        while f(hi) < 0.0 {
            hi *= 2.0;
            if hi > hi_limit {
                return None;
            }
        }
    }
    brent(&f, lo, hi, 1e-12 * x0)
}

/// Minimises a unimodal function on `[a, b]` by golden-section search.
pub fn golden_min(f: impl Fn(f64) -> f64, mut a: f64, mut b: f64, iterations: usize) -> f64 {
    let g = (5.0_f64.sqrt() - 1.0) / 2.0;
    let mut c = b - g * (b - a);
    let mut d = a + g * (b - a);
    let (mut fc, mut fd) = (f(c), f(d));
    for _ in 0..iterations {
        if fc < fd {
            b = d;
            d = c;
            fd = fc;
            c = b - g * (b - a);
            fc = f(c);
        } else {
            a = c;
            c = d;
            fc = fd;
            d = a + g * (b - a);
            fd = f(d);
        }
    }
    0.5 * (a + b)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(a: f64, b: f64, tol: f64) {
        assert!((a - b).abs() <= tol * (1.0 + b.abs()), "{a} vs {b}");
    }

    #[test]
    fn gamma_family_known_values() {
        close(ln_gamma(1.0), 0.0, 1e-13);
        close(ln_gamma(0.5), 0.5 * PI.ln(), 1e-13);
        close(ln_gamma(10.0), 362_880.0_f64.ln(), 1e-13);
        close(digamma(1.0), -0.577_215_664_901_532_9, 1e-12);
        close(digamma(0.5), -1.963_510_026_021_423_5, 1e-12);
        close(trigamma(1.0), PI * PI / 6.0, 1e-12);
        close(trigamma(0.5), PI * PI / 2.0, 1e-12);
    }

    #[test]
    fn incomplete_gamma_and_beta() {
        // P(1, x) = 1 - e^-x
        close(gamma_p(1.0, 2.0), 1.0 - (-2.0_f64).exp(), 1e-13);
        // chi-square(2 dof) at 5.991 is 0.95: P(1, 5.991/2)
        close(gamma_p(1.0, 5.991_464_547 / 2.0), 0.95, 1e-8);
        close(gamma_p(3.5, 2.0) + gamma_q(3.5, 2.0), 1.0, 1e-14);
        close(gamma_p(30.0, 40.0) + gamma_q(30.0, 40.0), 1.0, 1e-14);
        // I_x(1, 1) = x; I_x(2, 1) = x²; symmetry I_x(a,b) = 1 - I_{1-x}(b,a)
        close(beta_inc(1.0, 1.0, 0.3), 0.3, 1e-13);
        close(beta_inc(2.0, 1.0, 0.3), 0.09, 1e-13);
        close(
            beta_inc(2.5, 4.0, 0.7),
            1.0 - beta_inc(4.0, 2.5, 0.3),
            1e-12,
        );
    }

    #[test]
    fn normal_cdf_and_inverse() {
        close(norm_cdf(0.0), 0.5, 1e-14);
        close(norm_cdf(1.959_963_984_540_054), 0.975, 1e-12);
        close(norm_cdf(-3.0), 0.001_349_898_031_630_095, 1e-12);
        close(norm_ppf(0.975), 1.959_963_984_540_054, 1e-12);
        for p in [1e-9, 0.01, 0.2, 0.5, 0.9, 0.999] {
            close(norm_cdf(norm_ppf(p)), p, 1e-12);
        }
    }

    #[test]
    fn root_finding() {
        let r = brent(|x| x * x - 2.0, 0.0, 2.0, 1e-14).unwrap();
        close(r, 2.0_f64.sqrt(), 1e-12);
        assert!(brent(|x| x * x + 1.0, -1.0, 1.0, 1e-12).is_none());
        let r = root_increasing_positive(|x| x.ln() - 3.0, 1.0, 1e-9, 1e9).unwrap();
        close(r, 3.0_f64.exp(), 1e-9);
        let m = golden_min(|x| (x - 1.5) * (x - 1.5), 0.0, 4.0, 80);
        close(m, 1.5, 1e-7);
    }
}
