//! Data diagnostics: descriptive statistics, independence/trend tests, outliers and
//! actionable warnings shown before any fitting result.
//!
//! References: Wald & Wolfowitz (1940) runs test; Mann (1945) / Kendall (1975) trend test
//! with tie-corrected variance and continuity correction; Ljung & Box (1978) portmanteau
//! test; Hyndman & Fan (1996) type-7 quantiles (R's default); Joanes & Gill (1998) for the
//! adjusted skewness G1 and excess kurtosis G2.

use serde::Serialize;

#[derive(Clone, Debug, Default, Serialize)]
pub struct Descriptive {
    pub n: usize,
    pub mean: f64,
    pub median: f64,
    pub std_dev: f64,
    /// Coefficient of variation (std_dev / mean); ≈ 1 hints at an exponential.
    pub cv: f64,
    pub skewness: f64,
    pub excess_kurtosis: f64,
    pub min: f64,
    pub max: f64,
    pub q1: f64,
    pub q3: f64,
}

/// A hypothesis test on the data itself.
#[derive(Clone, Debug, Serialize)]
pub struct Check {
    pub name: String,
    pub statistic: f64,
    pub p_value: f64,
    /// Plain-language reading of the result, in Portuguese.
    pub verdict: String,
}

#[derive(Clone, Debug, Default, Serialize)]
pub struct Diagnostics {
    pub descriptive: Descriptive,
    /// Sample autocorrelation for lags 1..=min(10, n/4).
    pub autocorrelation: Vec<f64>,
    /// Runs test (statistic = z), Mann-Kendall (statistic = S) and Ljung-Box (statistic = Q).
    pub checks: Vec<Check>,
    /// Indices of values outside the 1.5·IQR fences.
    pub outliers: Vec<usize>,
    /// Fraction of repeated values (rounded/discretised data).
    pub ties_fraction: f64,
    /// Warnings the report must show before the fitting results.
    pub warnings: Vec<String>,
}

const ALPHA: f64 = 0.05;
/// Share of flagged points above which the outlier warning fires. Skewed but clean data
/// (e.g. exponential) already puts ~5-6 % beyond the 1.5·IQR fences, so 5 % would be noise.
const OUTLIER_FRACTION: f64 = 0.10;

/// Fraction as a percentage with one decimal and decimal comma.
fn pct(f: f64) -> String {
    format!("{:.1}", 100.0 * f).replace('.', ",")
}

fn sort_asc(v: &mut [f64]) {
    v.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
}

pub fn diagnose(data: &[f64]) -> Diagnostics {
    let n = data.len();
    let mut sorted = data.to_vec();
    sort_asc(&mut sorted);
    let descriptive = describe(data, &sorted);

    let max_lag = (n / 4).min(10);
    let autocorrelation = autocorrelation(data, max_lag);

    let mut checks = Vec::new();
    let mut runs_p = None;
    let mut mk_p = None;
    let mut lb_p = None;
    let mut trend_up = true;
    if n >= 4 {
        let (z, p) = runs_test(data, descriptive.median);
        runs_p = Some(p);
        checks.push(Check {
            name: "Teste de sequências (Wald-Wolfowitz)".into(),
            statistic: z,
            p_value: p,
            verdict: if p < ALPHA {
                format!(
                    "Há evidência de dependência entre valores consecutivos (p = {}): os dados podem não ser independentes.",
                    fmt_p(p)
                )
            } else {
                format!(
                    "Não há evidência de dependência na ordem dos dados (p = {}).",
                    fmt_p(p)
                )
            },
        });
        let (s, _z, p) = mann_kendall(data);
        mk_p = Some(p);
        trend_up = s >= 0.0;
        checks.push(Check {
            name: "Tendência (Mann-Kendall)".into(),
            statistic: s,
            p_value: p,
            verdict: if p < ALPHA {
                format!(
                    "Há tendência {} ao longo da coleta (p = {}): os dados podem não ser estacionários.",
                    if trend_up { "crescente" } else { "decrescente" },
                    fmt_p(p)
                )
            } else {
                format!("Não há evidência de tendência (p = {}).", fmt_p(p))
            },
        });
        if max_lag >= 1 {
            let (q, p) = ljung_box(&autocorrelation, n);
            lb_p = Some(p);
            checks.push(Check {
                name: format!("Autocorrelação (Ljung-Box, {max_lag} lags)"),
                statistic: q,
                p_value: p,
                verdict: if p < ALPHA {
                    format!(
                        "Há autocorrelação significativa (p = {}): os valores não parecem independentes.",
                        fmt_p(p)
                    )
                } else {
                    format!("Não há evidência de autocorrelação (p = {}).", fmt_p(p))
                },
            });
        }
    }

    let outliers = find_outliers(data, descriptive.q1, descriptive.q3);
    let ties_fraction = if n == 0 {
        0.0
    } else {
        let distinct = 1 + sorted.windows(2).filter(|w| w[0] != w[1]).count();
        1.0 - distinct as f64 / n as f64
    };

    let mut warnings = Vec::new();
    let dependent = runs_p.is_some_and(|p| p < ALPHA) || lb_p.is_some_and(|p| p < ALPHA);
    if dependent {
        warnings.push(
            "Foi detectada dependência entre valores consecutivos: ajustar uma distribuição \
             supõe amostras independentes. Verifique a ordem de coleta, se os dados vêm em lotes \
             ou turnos, e considere separar os períodos antes de ajustar."
                .to_string(),
        );
    }
    if mk_p.is_some_and(|p| p < ALPHA) {
        warnings.push(format!(
            "Há tendência {} nos dados: o processo parece não estacionário. Considere dividir \
             os dados por período ou modelar chegadas não estacionárias.",
            if trend_up { "crescente" } else { "decrescente" }
        ));
    }
    if n > 0 && outliers.len() as f64 > OUTLIER_FRACTION * n as f64 {
        warnings.push(format!(
            "{} valores ({} % da amostra) estão fora das cercas de 1,5·IQR. Verifique erros de \
             digitação ou de medição antes de ajustar.",
            outliers.len(),
            pct(outliers.len() as f64 / n as f64)
        ));
    }
    if ties_fraction > 0.3 {
        warnings.push(format!(
            "{} % dos valores são repetidos: os dados parecem arredondados ou discretizados. Os \
             p-valores de KS e AD ficam menos confiáveis; considere famílias discretas.",
            pct(ties_fraction)
        ));
    }
    if n > 0 && n < 30 {
        warnings.push(format!(
            "Amostra pequena (n = {n}): os testes têm baixo poder e vários modelos podem parecer \
             igualmente adequados."
        ));
    }
    if data.iter().any(|&x| x < 0.0) {
        warnings.push(
            "Há valores negativos: famílias que exigem suporte positivo (exponencial, gama, \
             Weibull, log-normal etc.) serão excluídas."
                .to_string(),
        );
    }
    if n > 0 && data.iter().all(|x| x.is_finite() && x.fract() == 0.0) {
        warnings.push(
            "Todos os valores são inteiros: considere candidatas discretas (Poisson, binomial, \
             geométrica etc.)."
                .to_string(),
        );
    }

    Diagnostics {
        descriptive,
        autocorrelation,
        checks,
        outliers,
        ties_fraction,
        warnings,
    }
}

/// Plain-language p-value with decimal comma.
fn fmt_p(p: f64) -> String {
    if p < 0.001 {
        "< 0,001".into()
    } else if p < 0.01 {
        format!("{p:.3}").replace('.', ",")
    } else {
        format!("{p:.2}").replace('.', ",")
    }
}

/// Type-7 quantile (R default) of an ascending-sorted slice.
fn quantile7(sorted: &[f64], p: f64) -> f64 {
    let n = sorted.len();
    if n == 0 {
        return 0.0;
    }
    let h = (n - 1) as f64 * p;
    let lo = h.floor() as usize;
    let hi = (lo + 1).min(n - 1);
    sorted[lo] + (h - lo as f64) * (sorted[hi] - sorted[lo])
}

fn describe(data: &[f64], sorted: &[f64]) -> Descriptive {
    let n = data.len();
    if n == 0 {
        return Descriptive::default();
    }
    let nf = n as f64;
    let mean = data.iter().sum::<f64>() / nf;
    let (std_dev, skewness, excess_kurtosis) = if n >= 2 {
        let s2 = data.iter().map(|x| (x - mean).powi(2)).sum::<f64>() / (nf - 1.0);
        let s = s2.sqrt();
        if s > 0.0 && s.is_finite() {
            let z3: f64 = data.iter().map(|x| ((x - mean) / s).powi(3)).sum();
            let z4: f64 = data.iter().map(|x| ((x - mean) / s).powi(4)).sum();
            let g1 = if n >= 3 {
                nf / ((nf - 1.0) * (nf - 2.0)) * z3
            } else {
                0.0
            };
            let g2 = if n >= 4 {
                nf * (nf + 1.0) / ((nf - 1.0) * (nf - 2.0) * (nf - 3.0)) * z4
                    - 3.0 * (nf - 1.0).powi(2) / ((nf - 2.0) * (nf - 3.0))
            } else {
                0.0
            };
            (s, g1, g2)
        } else {
            (0.0, 0.0, 0.0)
        }
    } else {
        (0.0, 0.0, 0.0)
    };
    Descriptive {
        n,
        mean,
        median: quantile7(sorted, 0.5),
        std_dev,
        cv: if mean != 0.0 { std_dev / mean } else { 0.0 },
        skewness,
        excess_kurtosis,
        min: sorted[0],
        max: sorted[n - 1],
        q1: quantile7(sorted, 0.25),
        q3: quantile7(sorted, 0.75),
    }
}

fn autocorrelation(data: &[f64], max_lag: usize) -> Vec<f64> {
    let n = data.len();
    if n == 0 || max_lag == 0 {
        return Vec::new();
    }
    let mean = data.iter().sum::<f64>() / n as f64;
    let denom: f64 = data.iter().map(|x| (x - mean).powi(2)).sum();
    (1..=max_lag)
        .map(|k| {
            if denom <= 0.0 {
                return 0.0;
            }
            let cov: f64 = (0..n - k)
                .map(|t| (data[t] - mean) * (data[t + k] - mean))
                .sum();
            cov / denom
        })
        .collect()
}

fn find_outliers(data: &[f64], q1: f64, q3: f64) -> Vec<usize> {
    if data.len() < 4 {
        return Vec::new();
    }
    let iqr = q3 - q1;
    let (lo, hi) = (q1 - 1.5 * iqr, q3 + 1.5 * iqr);
    data.iter()
        .enumerate()
        .filter(|&(_, &x)| x < lo || x > hi)
        .map(|(i, _)| i)
        .collect()
}

/// Wald-Wolfowitz runs test above/below the median (values equal to it are dropped).
/// Returns (z, two-sided p) under the normal approximation.
fn runs_test(data: &[f64], median: f64) -> (f64, f64) {
    let signs: Vec<bool> = data
        .iter()
        .filter(|&&x| x != median)
        .map(|&x| x > median)
        .collect();
    let n1 = signs.iter().filter(|&&b| b).count() as f64;
    let n2 = signs.len() as f64 - n1;
    if n1 == 0.0 || n2 == 0.0 {
        return (0.0, 1.0);
    }
    let runs = 1.0 + signs.windows(2).filter(|w| w[0] != w[1]).count() as f64;
    let m = n1 + n2;
    let mean = 2.0 * n1 * n2 / m + 1.0;
    let var = 2.0 * n1 * n2 * (2.0 * n1 * n2 - m) / (m * m * (m - 1.0));
    if var <= 0.0 {
        return (0.0, 1.0);
    }
    let z = (runs - mean) / var.sqrt();
    (z, normal_two_sided(z))
}

/// Mann-Kendall trend test. Returns (S, z, two-sided p). S is computed in O(n log n)
/// with a Fenwick tree over value ranks; the variance includes the tie correction.
fn mann_kendall(data: &[f64]) -> (f64, f64, f64) {
    let n = data.len();
    let mut sorted = data.to_vec();
    sort_asc(&mut sorted);
    let mut vals = sorted.clone();
    vals.dedup();
    let m = vals.len();
    let mut tree = vec![0i64; m + 1];
    let mut s = 0i64;
    for (j, &x) in data.iter().enumerate() {
        let r = vals.partition_point(|&v| v < x) + 1; // 1-based rank
        let mut less = 0i64;
        let mut i = r - 1;
        while i > 0 {
            less += tree[i];
            i -= i & i.wrapping_neg();
        }
        let mut leq = 0i64;
        let mut i = r;
        while i > 0 {
            leq += tree[i];
            i -= i & i.wrapping_neg();
        }
        let greater = j as i64 - leq;
        s += less - greater;
        let mut i = r;
        while i <= m {
            tree[i] += 1;
            i += i & i.wrapping_neg();
        }
    }
    let nf = n as f64;
    let mut var = nf * (nf - 1.0) * (2.0 * nf + 5.0);
    let mut i = 0;
    while i < n {
        let mut j = i;
        while j + 1 < n && sorted[j + 1] == sorted[i] {
            j += 1;
        }
        let t = (j - i + 1) as f64;
        if t > 1.0 {
            var -= t * (t - 1.0) * (2.0 * t + 5.0);
        }
        i = j + 1;
    }
    var /= 18.0;
    let sf = s as f64;
    if var <= 0.0 {
        return (sf, 0.0, 1.0);
    }
    let z = if s > 0 {
        (sf - 1.0) / var.sqrt()
    } else if s < 0 {
        (sf + 1.0) / var.sqrt()
    } else {
        0.0
    };
    (sf, z, normal_two_sided(z))
}

/// Ljung-Box Q over the given autocorrelations; p from chi-square with `lags` d.o.f.
fn ljung_box(acf: &[f64], n: usize) -> (f64, f64) {
    let nf = n as f64;
    let q = nf
        * (nf + 2.0)
        * acf
            .iter()
            .enumerate()
            .map(|(i, r)| r * r / (nf - (i + 1) as f64))
            .sum::<f64>();
    (q, chi2_sf(q, acf.len() as f64))
}

fn normal_two_sided(z: f64) -> f64 {
    // 2·(1 − Φ(|z|)) = Q(1/2, z²/2)
    gamma_q(0.5, z * z / 2.0).clamp(0.0, 1.0)
}

fn chi2_sf(x: f64, dof: f64) -> f64 {
    if x <= 0.0 {
        1.0
    } else {
        gamma_q(dof / 2.0, x / 2.0)
    }
}

/// Lanczos approximation of ln Γ(x), x > 0.
fn ln_gamma(x: f64) -> f64 {
    const G: [f64; 6] = [
        76.180_091_729_471_46,
        -86.505_320_329_416_77,
        24.014_098_240_830_91,
        -1.231_739_572_450_155,
        0.001_208_650_973_866_179,
        -0.000_005_395_239_384_953,
    ];
    let mut y = x;
    let tmp = x + 5.5;
    let tmp = tmp - (x + 0.5) * tmp.ln();
    let mut ser = 1.000_000_000_190_015;
    for g in G {
        y += 1.0;
        ser += g / y;
    }
    -tmp + (2.506_628_274_631_000_5 * ser / x).ln()
}

/// Regularized upper incomplete gamma Q(a, x): series for x < a+1, Lentz continued
/// fraction otherwise (Numerical Recipes §6.2).
fn gamma_q(a: f64, x: f64) -> f64 {
    if x <= 0.0 {
        return 1.0;
    }
    let ln_pref = -x + a * x.ln() - ln_gamma(a);
    if x < a + 1.0 {
        let mut ap = a;
        let mut del = 1.0 / a;
        let mut sum = del;
        for _ in 0..1000 {
            ap += 1.0;
            del *= x / ap;
            sum += del;
            if del.abs() < sum.abs() * 1e-15 {
                break;
            }
        }
        1.0 - sum * ln_pref.exp()
    } else {
        const TINY: f64 = 1e-300;
        let mut b = x + 1.0 - a;
        let mut c = 1.0 / TINY;
        let mut d = 1.0 / b;
        let mut h = d;
        for i in 1..1000 {
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
            if (del - 1.0).abs() < 1e-15 {
                break;
            }
        }
        ln_pref.exp() * h
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rand::Rng;
    use utsim_core::rng::stream;

    fn close(a: f64, b: f64, tol: f64) {
        assert!((a - b).abs() < tol, "{a} vs {b}");
    }

    fn has(d: &Diagnostics, needle: &str) -> bool {
        d.warnings.iter().any(|w| w.contains(needle))
    }

    fn exp_sample(n: usize, seed: u64) -> Vec<f64> {
        let mut rng = stream(seed, 0, 1);
        (0..n).map(|_| -(1.0 - rng.random::<f64>()).ln()).collect()
    }

    fn std_normal(rng: &mut impl Rng) -> f64 {
        let (u1, u2) = (1.0 - rng.random::<f64>(), rng.random::<f64>());
        (-2.0 * u1.ln()).sqrt() * (2.0 * std::f64::consts::PI * u2).cos()
    }

    #[test]
    fn descriptive_matches_reference() {
        // x <- c(2,4,4,4,5,5,7,9)
        // mean 5; median 4.5; sd(x) = 2.138090; quantile(x, c(.25,.75)) = 4, 5.5 (type 7)
        // e1071::skewness(x, type = 2) = 0.8184876; kurtosis(x, type = 2) = 0.940625
        let x = [2.0, 4.0, 4.0, 4.0, 5.0, 5.0, 7.0, 9.0];
        let d = diagnose(&x).descriptive;
        assert_eq!(d.n, 8);
        close(d.mean, 5.0, 1e-12);
        close(d.median, 4.5, 1e-12);
        close(d.std_dev, 2.138_089_935, 1e-8);
        close(d.cv, 2.138_089_935 / 5.0, 1e-8);
        close(d.skewness, 0.818_487_553, 1e-8);
        close(d.excess_kurtosis, 0.940_625, 1e-8);
        close(d.q1, 4.0, 1e-12);
        close(d.q3, 5.5, 1e-12);
        assert_eq!((d.min, d.max), (2.0, 9.0));
    }

    #[test]
    fn special_functions() {
        // pchisq(3.2, 2, lower.tail = FALSE) = exp(-1.6); pchisq(10, 4, lower.tail = FALSE)
        close(chi2_sf(3.2, 2.0), (-1.6f64).exp(), 1e-12);
        close(chi2_sf(10.0, 4.0), 6.0 * (-5.0f64).exp(), 1e-12);
        // 2 * pnorm(-1.96) = 0.0499958
        close(normal_two_sided(1.96), 0.049_995_8, 1e-6);
        close(normal_two_sided(0.0), 1.0, 1e-12);
    }

    #[test]
    fn runs_test_hand_checked() {
        // 1..6, median 3.5: n1 = n2 = 3, R = 2, E = 4, Var = 1.2 -> z = -1.8257
        let (z, p) = runs_test(&[1.0, 2.0, 3.0, 4.0, 5.0, 6.0], 3.5);
        close(z, -2.0 / 1.2f64.sqrt(), 1e-12);
        close(p, 0.0679, 1e-3);
        // Alternating: R = 6 -> z = +1.0954
        let (z, _) = runs_test(&[1.0, 9.0, 1.0, 9.0, 1.0, 9.0], 5.0);
        close(z, 2.0 / 1.2f64.sqrt(), 1e-12);
    }

    #[test]
    fn mann_kendall_hand_checked() {
        // 1..6 increasing: S = 15, Var = 6*5*17/18 -> z = 14 / sqrt(28.333)
        let (s, z, p) = mann_kendall(&[1.0, 2.0, 3.0, 4.0, 5.0, 6.0]);
        assert_eq!(s, 15.0);
        close(z, 14.0 / (510.0f64 / 18.0).sqrt(), 1e-12);
        assert!(p < 0.01);
        let (s, z2, _) = mann_kendall(&[6.0, 5.0, 4.0, 3.0, 2.0, 1.0]);
        assert_eq!(s, -15.0);
        close(z2, -z, 1e-12);
        // With ties: 1,1,2 -> S = 2; Var = (3*2*11 - 2*1*9)/18 = 48/18
        let (s, z, _) = mann_kendall(&[1.0, 1.0, 2.0]);
        assert_eq!(s, 2.0);
        close(z, 1.0 / (48.0f64 / 18.0).sqrt(), 1e-12);
    }

    #[test]
    fn iid_exponential_has_no_warnings() {
        let d = diagnose(&exp_sample(500, 11));
        assert_eq!(d.checks.len(), 3);
        assert!(d.warnings.is_empty(), "{:?}", d.warnings);
        assert!(d.checks.iter().all(|c| c.p_value >= ALPHA));
        close(d.descriptive.cv, 1.0, 0.2);
    }

    #[test]
    fn ar1_triggers_dependence() {
        let mut rng = stream(5, 0, 2);
        let mut x = 0.0;
        let data: Vec<f64> = (0..500)
            .map(|_| {
                x = 0.8 * x + std_normal(&mut rng);
                x
            })
            .collect();
        let d = diagnose(&data);
        assert!(has(&d, "dependência"), "{:?}", d.warnings);
        assert!(d.autocorrelation[0] > 0.6);
    }

    #[test]
    fn trend_triggers_warning() {
        let mut rng = stream(6, 0, 3);
        let data: Vec<f64> = (0..200)
            .map(|i| 0.05 * i as f64 + rng.random::<f64>())
            .collect();
        let d = diagnose(&data);
        assert!(has(&d, "tendência crescente"), "{:?}", d.warnings);
        assert!(d.checks[1].verdict.contains("crescente"));
    }

    #[test]
    fn rounded_data_triggers_ties_and_integers() {
        let data: Vec<f64> = exp_sample(200, 3)
            .iter()
            .map(|x| (x * 2.0).round())
            .collect();
        let d = diagnose(&data);
        assert!(d.ties_fraction > 0.3);
        assert!(has(&d, "arredondados"));
        assert!(has(&d, "inteiros"));
    }

    #[test]
    fn small_negative_and_outliers() {
        let data = [
            1.5, 2.5, 2.0, 1.8, 2.2, 2.1, 1.9, 2.4, 1.7, 2.3, 100.0, -50.0, 2.0, 2.1,
        ];
        let d = diagnose(&data);
        assert!(d.outliers.contains(&10) && d.outliers.contains(&11));
        assert!(has(&d, "Amostra pequena"));
        assert!(has(&d, "negativos"));
        assert!(has(&d, "1,5·IQR"));
    }

    #[test]
    fn degenerate_inputs_do_not_panic() {
        for n in 0..6 {
            for v in [0.0, 3.0] {
                let data = vec![v; n];
                let d = diagnose(&data);
                assert!(d.descriptive.mean.is_finite() && d.descriptive.skewness.is_finite());
                assert!(d.descriptive.excess_kurtosis.is_finite() && d.descriptive.cv.is_finite());
                assert!(d.checks.iter().all(|c| c.p_value.is_finite()));
                assert!(d.autocorrelation.iter().all(|r| r.is_finite()));
            }
        }
        assert_eq!(diagnose(&[]).descriptive.n, 0);
        assert_eq!(diagnose(&[4.0]).descriptive.median, 4.0);
    }
}
