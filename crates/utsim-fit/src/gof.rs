//! Goodness-of-fit tests: Kolmogorov-Smirnov, Anderson-Darling and χ².
//!
//! KS and AD p-values come from a parametric bootstrap (simulate from the fitted model,
//! refit, recompute the statistic), which corrects the bias of estimating the parameters
//! from the very data being tested (Lilliefors-type correction; Stute, González-Manteiga
//! & Presedo-Quindimil, 1993). Without bootstrap the p-values are asymptotic.

use serde::Serialize;
use utsim_core::rng::stream;

use crate::family::{Family, Fitted, FittedModel};

#[derive(Clone, Debug, Serialize)]
pub struct GofConfig {
    /// Parametric bootstrap replicates for KS and AD p-values (0 = asymptotic only).
    pub bootstrap: usize,
    pub seed: u64,
    /// Number of equiprobable classes for χ² (`None` = automatic rule).
    pub chi2_bins: Option<usize>,
}

impl Default for GofConfig {
    fn default() -> Self {
        GofConfig {
            bootstrap: 300,
            seed: 1,
            chi2_bins: None,
        }
    }
}

/// One test: statistic, p-value and how the p-value was obtained.
#[derive(Clone, Debug, Serialize)]
pub struct TestStat {
    pub statistic: f64,
    pub p_value: f64,
    /// e.g. "bootstrap paramétrico (300)", "assintótico".
    pub method: String,
}

#[derive(Clone, Debug, Default, Serialize)]
pub struct GofResult {
    pub ks: Option<TestStat>,
    pub anderson_darling: Option<TestStat>,
    pub chi_square: Option<TestStat>,
}

/// Maximum number of redraws of a bootstrap sample whose refit fails.
const MAX_REFIT_RETRIES: u64 = 10;
/// Largest integer range scanned by the discrete χ² grouping.
const MAX_DISCRETE_RANGE: i64 = 200_000;

/// Runs all applicable tests for one fitted candidate. `family` is needed to refit
/// each bootstrap sample, since parameters estimated from the data bias plain KS/AD.
///
/// Tests that do not apply return `None`: Anderson-Darling for discrete families, and
/// χ² when n < 30 or the degrees of freedom would be below 1.
pub fn evaluate(
    family: &dyn Family,
    fitted: &Fitted,
    data: &[f64],
    config: &GofConfig,
) -> GofResult {
    let mut sorted: Vec<f64> = data.iter().copied().filter(|x| x.is_finite()).collect();
    if sorted.is_empty() {
        return GofResult::default();
    }
    sorted.sort_by(|a, b| a.total_cmp(b));
    let discrete = family.is_discrete();
    let n = sorted.len();
    let model = fitted.model.as_ref();

    let d_obs = ks_statistic(model, &sorted, discrete);
    let a_obs = (!discrete).then(|| ad_statistic(model, &sorted));
    let chi_obs = chi_square_statistic(fitted, &sorted, discrete, config.chi2_bins);

    // Parametric bootstrap: simulate from the fitted model, refit, recompute.
    let mut boot = None;
    let mut chi_boot = None;
    if config.bootstrap > 0 {
        let (mut ge_d, mut ge_a, mut valid) = (0usize, 0usize, 0usize);
        let (mut ge_chi, mut valid_chi) = (0usize, 0usize);
        let mut sample = vec![0.0; n];
        for b in 1..=config.bootstrap {
            let mut refit = None;
            for attempt in 0..MAX_REFIT_RETRIES {
                let mut rng = stream(config.seed, b as u32, attempt);
                for v in sample.iter_mut() {
                    *v = model.sample(&mut rng);
                }
                if sample.iter().any(|x| !x.is_finite()) {
                    continue;
                }
                if let Ok(f) = family.fit(&sample) {
                    refit = Some(f);
                    break;
                }
            }
            let Some(f) = refit else { continue };
            sample.sort_by(|a, b| a.total_cmp(b));
            valid += 1;
            if ks_statistic(f.model.as_ref(), &sample, discrete) >= d_obs {
                ge_d += 1;
            }
            if let Some(a) = a_obs
                && ad_statistic(f.model.as_ref(), &sample) >= a
            {
                ge_a += 1;
            }
            // χ² is bootstrapped too: with estimated bounds (Uniform, Triangular, Beta,
            // shifted families) the asymptotic χ²(k − 1 − p) reference is not valid.
            if let Some(obs) = &chi_obs
                && let Some(b) = chi_square_statistic(&f, &sample, discrete, config.chi2_bins)
            {
                valid_chi += 1;
                if b.statistic >= obs.statistic {
                    ge_chi += 1;
                }
            }
        }
        if valid > 0 {
            let p = |ge: usize| (1 + ge) as f64 / (valid + 1) as f64;
            boot = Some((p(ge_d), p(ge_a), valid));
        }
        if valid_chi > 0 {
            chi_boot = Some(((1 + ge_chi) as f64 / (valid_chi + 1) as f64, valid_chi));
        }
    }

    let ks = Some(match boot {
        Some((p, _, valid)) => TestStat {
            statistic: d_obs,
            p_value: p,
            method: format!(
                "bootstrap paramétrico ({valid} réplicas){}",
                if discrete {
                    "; KS discreto sobre a CDF em degraus"
                } else {
                    ""
                }
            ),
        },
        None => TestStat {
            statistic: d_obs,
            p_value: ks_asymptotic_p(d_obs, n),
            method: format!(
                "assintótico (conservador: parâmetros estimados){}",
                if discrete {
                    "; KS discreto, p também conservador"
                } else {
                    ""
                }
            ),
        },
    });

    let anderson_darling = a_obs.map(|a| match boot {
        Some((_, p, valid)) => TestStat {
            statistic: a,
            p_value: p,
            method: format!("bootstrap paramétrico ({valid} réplicas)"),
        },
        None => TestStat {
            statistic: a,
            p_value: ad_asymptotic_p(a),
            method: "assintótico (Marsaglia e Marsaglia, 2004, parâmetros totalmente \
                     especificados: não corrige a estimação)"
                .into(),
        },
    });

    let chi_square = chi_obs.map(|obs| match chi_boot {
        Some((p, valid)) => TestStat {
            statistic: obs.statistic,
            p_value: p,
            method: format!(
                "{}; p por bootstrap paramétrico ({valid} réplicas)",
                obs.method
            ),
        },
        None => obs,
    });

    GofResult {
        ks,
        anderson_darling,
        chi_square,
    }
}

/// Kolmogorov-Smirnov distance between the empirical CDF of `sorted` and the model.
/// Discrete families compare both sides of every jump of the step CDF (integer support).
fn ks_statistic(model: &dyn FittedModel, sorted: &[f64], discrete: bool) -> f64 {
    let n = sorted.len() as f64;
    let mut d = 0.0f64;
    if !discrete {
        for (i, &x) in sorted.iter().enumerate() {
            let f = model.cdf(x);
            d = d.max((i + 1) as f64 / n - f).max(f - i as f64 / n);
        }
        return d;
    }
    let mut i = 0;
    while i < sorted.len() {
        let v = sorted[i];
        let mut j = i;
        while j < sorted.len() && sorted[j] == v {
            j += 1;
        }
        // Empirical CDF at v and just before v, against the model CDF at v and v-1.
        d = d
            .max(((j as f64 / n) - model.cdf(v)).abs())
            .max(((i as f64 / n) - model.cdf(v - 1.0)).abs());
        i = j;
    }
    d
}

/// Anderson-Darling A² with F clamped to [1e-12, 1-1e-12].
fn ad_statistic(model: &dyn FittedModel, sorted: &[f64]) -> f64 {
    let n = sorted.len();
    let nf = n as f64;
    let f: Vec<f64> = sorted
        .iter()
        .map(|&x| model.cdf(x).clamp(1e-12, 1.0 - 1e-12))
        .collect();
    let s: f64 = (0..n)
        .map(|i| (2 * i + 1) as f64 * (f[i].ln() + (1.0 - f[n - 1 - i]).ln()))
        .sum();
    -nf - s / nf
}

/// Asymptotic Kolmogorov survival function with Stephens' small-sample adjustment.
fn ks_asymptotic_p(d: f64, n: usize) -> f64 {
    let sn = (n as f64).sqrt();
    let lambda = (sn + 0.12 + 0.11 / sn) * d;
    if lambda < 0.2 {
        return 1.0;
    }
    let mut sum = 0.0;
    for k in 1..=100 {
        let kf = k as f64;
        let term = (-2.0 * kf * kf * lambda * lambda).exp();
        sum += if k % 2 == 1 { term } else { -term };
        if term < 1e-16 {
            break;
        }
    }
    (2.0 * sum).clamp(0.0, 1.0)
}

/// P(A² ≥ a) for fully specified parameters, n → ∞ (Marsaglia & Marsaglia, 2004).
fn ad_asymptotic_p(a: f64) -> f64 {
    if a <= 0.0 {
        return 1.0;
    }
    let cdf = if a < 2.0 {
        a.powf(-0.5)
            * (-1.2337141 / a).exp()
            * (2.00012
                + (0.247105 - (0.0649821 - (0.0347962 - (0.011672 - 0.00168691 * a) * a) * a) * a)
                    * a)
    } else {
        (-(1.0776
            - (2.30695 - (0.43424 - (0.082433 - (0.008056 - 0.0003146 * a) * a) * a) * a) * a)
            .exp())
        .exp()
    };
    (1.0 - cdf).clamp(0.0, 1.0)
}

/// χ² statistic with equiprobable classes (continuous) or classes of expected count ≥ 5
/// (discrete), with its asymptotic p-value (d.o.f. = classes − 1 − estimated parameters).
fn chi_square_statistic(
    fitted: &Fitted,
    sorted: &[f64],
    discrete: bool,
    bins: Option<usize>,
) -> Option<TestStat> {
    let n = sorted.len();
    if n < 30 {
        return None;
    }
    let model = fitted.model.as_ref();
    let (stat, k, desc) = if discrete {
        let (stat, k) = chi2_discrete(model, sorted)?;
        (stat, k, "classes com esperado ≥ 5")
    } else {
        let k = match bins {
            Some(k) => k.max(2),
            None => ((2.0 * (n as f64).powf(0.4)).ceil() as usize).clamp(5, 50),
        };
        let bounds: Vec<f64> = (1..k)
            .map(|i| model.quantile(i as f64 / k as f64))
            .collect();
        if bounds.iter().any(|b| !b.is_finite()) || bounds.windows(2).any(|w| w[1] <= w[0]) {
            return None;
        }
        let mut counts = vec![0usize; k];
        for &x in sorted {
            counts[bounds.partition_point(|b| *b < x)] += 1;
        }
        let e = n as f64 / k as f64;
        let stat = counts.iter().map(|&o| (o as f64 - e).powi(2) / e).sum();
        (stat, k, "classes equiprováveis")
    };
    let dof = k as i64 - 1 - fitted.n_params as i64;
    if dof < 1 {
        return None;
    }
    Some(TestStat {
        statistic: stat,
        p_value: chi2_sf(stat, dof as f64),
        method: format!("qui-quadrado com {k} {desc} ({dof} graus de liberdade)"),
    })
}

/// Discrete χ²: statistic and number of classes after merging to expected ≥ 5.
fn chi2_discrete(model: &dyn FittedModel, sorted: &[f64]) -> Option<(f64, usize)> {
    let n = sorted.len() as f64;
    let lo = sorted[0].round() as i64;
    let hi = sorted[sorted.len() - 1].round() as i64;
    if hi - lo > MAX_DISCRETE_RANGE {
        return None;
    }
    let mut observed = vec![0.0f64; (hi - lo + 1) as usize];
    for &x in sorted {
        observed[(x.round() as i64 - lo) as usize] += 1.0;
    }
    // (expected, observed) per class; the first class also takes the lower tail and the
    // last one the upper tail, so expected counts add up to n.
    let mut classes: Vec<(f64, f64)> = Vec::new();
    let (mut e_acc, mut o_acc) = (0.0, 0.0);
    let mut prev_cdf = 0.0;
    for (idx, &o) in observed.iter().enumerate() {
        let v = (lo + idx as i64) as f64;
        let c = if idx + 1 == observed.len() {
            1.0
        } else {
            model.cdf(v)
        };
        e_acc += n * (c - prev_cdf);
        o_acc += o;
        prev_cdf = c;
        if e_acc >= 5.0 {
            classes.push((e_acc, o_acc));
            e_acc = 0.0;
            o_acc = 0.0;
        }
    }
    if e_acc > 0.0 || o_acc > 0.0 {
        match classes.last_mut() {
            Some(last) => {
                last.0 += e_acc;
                last.1 += o_acc;
            }
            None => classes.push((e_acc, o_acc)),
        }
    }
    let stat = classes.iter().map(|&(e, o)| (o - e).powi(2) / e).sum();
    Some((stat, classes.len()))
}

/// χ² survival function: P(X ≥ x) for X ~ χ²(dof) = Q(dof/2, x/2).
fn chi2_sf(x: f64, dof: f64) -> f64 {
    if x <= 0.0 {
        return 1.0;
    }
    gamma_q(dof / 2.0, x / 2.0)
}

/// Regularized upper incomplete gamma Q(a, x) (series for x < a+1, continued fraction
/// otherwise; Numerical Recipes 6.2).
fn gamma_q(a: f64, x: f64) -> f64 {
    if x <= 0.0 {
        return 1.0;
    }
    let gln = ln_gamma(a);
    if x < a + 1.0 {
        let (mut ap, mut del) = (a, 1.0 / a);
        let mut sum = del;
        for _ in 0..1000 {
            ap += 1.0;
            del *= x / ap;
            sum += del;
            if del.abs() < sum.abs() * 1e-16 {
                break;
            }
        }
        (1.0 - sum * (-x + a * x.ln() - gln).exp()).clamp(0.0, 1.0)
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
            if (del - 1.0).abs() < 1e-16 {
                break;
            }
        }
        ((-x + a * x.ln() - gln).exp() * h).clamp(0.0, 1.0)
    }
}

/// ln Γ(x) for x > 0 (Lanczos, g = 7, n = 9).
fn ln_gamma(x: f64) -> f64 {
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
    use std::f64::consts::PI;
    if x < 0.5 {
        return (PI / (PI * x).sin()).ln() - ln_gamma(1.0 - x);
    }
    let x = x - 1.0;
    let mut a = C[0];
    let t = x + 7.5;
    for (i, c) in C.iter().enumerate().skip(1) {
        a += c / (x + i as f64);
    }
    0.5 * (2.0 * PI).ln() + (x + 0.5) * t.ln() - t + a.ln()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::families::Exponential;
    use rand::Rng;
    use std::time::Instant;
    use utsim_core::rng::SimRng;

    /// Fixed Uniform(0,1) model for hand-checked statistics.
    struct U01;
    impl FittedModel for U01 {
        fn cdf(&self, x: f64) -> f64 {
            x.clamp(0.0, 1.0)
        }
        fn pdf(&self, x: f64) -> f64 {
            if (0.0..=1.0).contains(&x) { 1.0 } else { 0.0 }
        }
        fn sample(&self, rng: &mut SimRng) -> f64 {
            rng.random()
        }
        fn quantile(&self, p: f64) -> f64 {
            p
        }
    }

    // Private test-only families with closed-form MLE.
    struct NormalModel(f64, f64);
    impl FittedModel for NormalModel {
        fn cdf(&self, x: f64) -> f64 {
            0.5 * (1.0 + erf((x - self.0) / (self.1 * std::f64::consts::SQRT_2)))
        }
        fn pdf(&self, x: f64) -> f64 {
            let z = (x - self.0) / self.1;
            (-0.5 * z * z).exp() / (self.1 * (2.0 * std::f64::consts::PI).sqrt())
        }
        fn sample(&self, rng: &mut SimRng) -> f64 {
            self.0 + self.1 * std_normal(rng)
        }
    }
    struct Normal;
    impl Family for Normal {
        fn name(&self) -> &'static str {
            "Normal"
        }
        fn n_params(&self) -> usize {
            2
        }
        fn fit(&self, d: &[f64]) -> Result<Fitted, String> {
            let n = d.len() as f64;
            let m = d.iter().sum::<f64>() / n;
            let v = d.iter().map(|x| (x - m).powi(2)).sum::<f64>() / n;
            if v <= 0.0 {
                return Err("variância nula".into());
            }
            Ok(Fitted {
                family: "Normal",
                params: vec![("média", m), ("desvio", v.sqrt())],
                n_params: 2,
                model: Box::new(NormalModel(m, v.sqrt())),
                dist: None,
            })
        }
    }

    struct UniformModel(f64, f64);
    impl FittedModel for UniformModel {
        fn cdf(&self, x: f64) -> f64 {
            ((x - self.0) / (self.1 - self.0)).clamp(0.0, 1.0)
        }
        fn pdf(&self, x: f64) -> f64 {
            if (self.0..=self.1).contains(&x) {
                1.0 / (self.1 - self.0)
            } else {
                0.0
            }
        }
        fn sample(&self, rng: &mut SimRng) -> f64 {
            self.0 + (self.1 - self.0) * rng.random::<f64>()
        }
        fn quantile(&self, p: f64) -> f64 {
            self.0 + p * (self.1 - self.0)
        }
    }
    struct Uniform;
    impl Family for Uniform {
        fn name(&self) -> &'static str {
            "Uniforme"
        }
        fn n_params(&self) -> usize {
            2
        }
        fn fit(&self, d: &[f64]) -> Result<Fitted, String> {
            let lo = d.iter().cloned().fold(f64::INFINITY, f64::min);
            let hi = d.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
            if hi <= lo {
                return Err("sem amplitude".into());
            }
            Ok(Fitted {
                family: "Uniforme",
                params: vec![("min", lo), ("max", hi)],
                n_params: 2,
                model: Box::new(UniformModel(lo, hi)),
                dist: None,
            })
        }
    }

    /// Discrete uniform on {0,1,2,3}, no estimated parameters.
    struct DiscModel;
    impl FittedModel for DiscModel {
        fn cdf(&self, x: f64) -> f64 {
            ((x.floor() + 1.0) / 4.0).clamp(0.0, 1.0)
        }
        fn pdf(&self, x: f64) -> f64 {
            if x.fract() == 0.0 && (0.0..=3.0).contains(&x) {
                0.25
            } else {
                0.0
            }
        }
        fn sample(&self, rng: &mut SimRng) -> f64 {
            rng.random_range(0..4) as f64
        }
    }
    struct DiscUniform;
    impl Family for DiscUniform {
        fn name(&self) -> &'static str {
            "Uniforme discreta"
        }
        fn n_params(&self) -> usize {
            0
        }
        fn is_discrete(&self) -> bool {
            true
        }
        fn fit(&self, _: &[f64]) -> Result<Fitted, String> {
            Ok(Fitted {
                family: "Uniforme discreta",
                params: vec![],
                n_params: 0,
                model: Box::new(DiscModel),
                dist: None,
            })
        }
    }

    fn std_normal(rng: &mut SimRng) -> f64 {
        let u1 = 1.0 - rng.random::<f64>();
        let u2: f64 = rng.random();
        (-2.0 * u1.ln()).sqrt() * (2.0 * std::f64::consts::PI * u2).cos()
    }

    /// erf through the incomplete gamma: erf(x) = 1 - Q(1/2, x²) for x ≥ 0.
    fn erf(x: f64) -> f64 {
        let p = 1.0 - gamma_q(0.5, x * x);
        if x < 0.0 { -p } else { p }
    }

    fn exp_sample(n: usize, mean: f64, seed: u64) -> Vec<f64> {
        let mut rng = stream(seed, 0, 99);
        (0..n)
            .map(|_| -mean * (1.0 - rng.random::<f64>()).ln())
            .collect()
    }

    fn lognormal_sample(n: usize, seed: u64) -> Vec<f64> {
        let mut rng = stream(seed, 0, 98);
        (0..n).map(|_| (0.8 * std_normal(&mut rng)).exp()).collect()
    }

    fn run(family: &dyn Family, data: &[f64], cfg: &GofConfig) -> GofResult {
        let f = family.fit(data).unwrap();
        evaluate(family, &f, data, cfg)
    }

    #[test]
    fn ks_and_ad_match_hand_computation() {
        // x = (0.2, 0.5, 0.9) against U(0,1), n = 3.
        // D+ = max(1/3-.2, 2/3-.5, 1-.9) = .1667; D- = max(.2, .5-1/3, .9-2/3) = .2333.
        let x = [0.2, 0.5, 0.9];
        assert!((ks_statistic(&U01, &x, false) - (0.9 - 2.0 / 3.0)).abs() < 1e-12);
        // A² = -3 - (1/3)[1(ln.2+ln.1) + 3(2 ln.5) + 5(ln.9+ln.8)] = 0.23780
        let a = -3.0
            - ((0.2f64.ln() + 0.1f64.ln())
                + 3.0 * 2.0 * 0.5f64.ln()
                + 5.0 * (0.9f64.ln() + 0.8f64.ln()))
                / 3.0;
        assert!((ad_statistic(&U01, &x) - a).abs() < 1e-12);
        assert!((a - 0.2378).abs() < 1e-3);
        // n = 1, F = .5: A² = -1 - (2 ln .5) = 0.38629.
        assert!((ad_statistic(&U01, &[0.5]) - (2.0 * 2f64.ln() - 1.0)).abs() < 1e-12);
        // n = 2, F = (.25, .75): A² = -2 - (1/2)[2 ln .25 + 6 ln .75] = 0.24935.
        let a2 = ad_statistic(&U01, &[0.25, 0.75]);
        assert!((a2 - 0.249_35).abs() < 1e-4);
    }

    #[test]
    fn special_functions_match_tables() {
        // χ² 0.95 quantiles.
        for (x, k) in [
            (3.841_459, 1.0),
            (5.991_465, 2.0),
            (18.307_038, 10.0),
            (124.342, 100.0),
        ] {
            assert!((chi2_sf(x, k) - 0.05).abs() < 1e-5, "k = {k}");
        }
        assert_eq!(chi2_sf(0.0, 3.0), 1.0);
        assert!((ln_gamma(5.0) - 24f64.ln()).abs() < 1e-12);
        // Kolmogorov: Q(1.358) ≈ 0.05; A²: P(A² ≥ 2.492) ≈ 0.05, P(≥ 1.933) ≈ 0.10.
        assert!((ks_asymptotic_p(1.358 / (1e6f64).sqrt(), 1_000_000) - 0.05).abs() < 2e-3);
        assert!((ad_asymptotic_p(2.492) - 0.05).abs() < 2e-3);
        assert!((ad_asymptotic_p(1.933) - 0.10).abs() < 3e-3);
    }

    #[test]
    fn exponential_data_not_rejected_lognormal_rejected() {
        let cfg = GofConfig::default();
        let ok = run(&Exponential, &exp_sample(500, 3.0, 7), &cfg);
        for t in [&ok.ks, &ok.anderson_darling, &ok.chi_square] {
            let t = t.as_ref().unwrap();
            assert!(t.p_value > 0.05, "{t:?}");
        }
        assert_eq!(
            ok.ks.unwrap().method,
            "bootstrap paramétrico (300 réplicas)"
        );

        let bad = run(&Exponential, &lognormal_sample(500, 4), &cfg);
        assert!(bad.ks.as_ref().unwrap().p_value < 0.05);
        assert!(bad.anderson_darling.as_ref().unwrap().p_value < 0.05);
        assert!(bad.chi_square.as_ref().unwrap().p_value < 0.05);
        // The right family (Normal on the log of lognormal data) is accepted.
        let logs: Vec<f64> = lognormal_sample(500, 4).iter().map(|x| x.ln()).collect();
        let good = run(&Normal, &logs, &cfg);
        assert!(good.ks.unwrap().p_value > 0.05);
        assert!(good.anderson_darling.unwrap().p_value > 0.05);
        // Uniform is rejected for exponential data.
        let u = run(&Uniform, &exp_sample(500, 3.0, 5), &cfg);
        assert!(u.anderson_darling.unwrap().p_value < 0.05);
    }

    #[test]
    fn asymptotic_mode_labels_and_does_not_bootstrap() {
        let cfg = GofConfig {
            bootstrap: 0,
            ..GofConfig::default()
        };
        let r = run(&Exponential, &exp_sample(200, 2.0, 1), &cfg);
        let ks = r.ks.unwrap();
        assert!(ks.method.starts_with("assintótico (conservador"));
        assert!(ks.p_value > 0.05);
        assert!(r.anderson_darling.unwrap().method.contains("Marsaglia"));
    }

    #[test]
    fn bootstrap_is_deterministic_and_fast() {
        let data = exp_sample(1000, 2.0, 11);
        let cfg = GofConfig::default();
        let t = Instant::now();
        let a = run(&Exponential, &data, &cfg);
        eprintln!("bootstrap 300 x n=1000: {:?}", t.elapsed());
        let b = run(&Exponential, &data, &cfg);
        assert_eq!(a.ks.unwrap().p_value, b.ks.unwrap().p_value);
    }

    #[test]
    fn chi_square_perfect_fit() {
        // Exact exponential quantiles at (i-.5)/n: the equiprobable classes are almost
        // exactly balanced, so χ² ≈ 0 and p ≈ 1.
        let n = 100;
        let data: Vec<f64> = (0..n)
            .map(|i| -(1.0 - (i as f64 + 0.5) / n as f64).ln())
            .collect();
        let r = run(&Exponential, &data, &GofConfig::default());
        let c = r.chi_square.unwrap();
        // k = ceil(2 * 100^0.4) = 13, dof = 13 - 1 - 1 = 11.
        assert!(
            c.method.contains("13 classes") && c.method.contains("11 graus"),
            "{}",
            c.method
        );
        assert!(c.statistic < 3.0 && c.p_value > 0.95, "{c:?}");
        // chi2_bins is respected: 10 classes -> 8 d.o.f.
        let cfg = GofConfig {
            chi2_bins: Some(10),
            ..GofConfig::default()
        };
        let c = run(&Exponential, &data, &cfg).chi_square.unwrap();
        assert!(c.method.contains("8 graus"));
    }

    #[test]
    fn chi_square_skipped_for_small_n_or_no_dof() {
        let small = exp_sample(29, 1.0, 2);
        assert!(
            run(&Exponential, &small, &GofConfig::default())
                .chi_square
                .is_none()
        );
        let data = exp_sample(100, 1.0, 2);
        let cfg = GofConfig {
            chi2_bins: Some(2),
            ..GofConfig::default()
        };
        assert!(run(&Exponential, &data, &cfg).chi_square.is_none());
    }

    #[test]
    fn discrete_family_perfect_fit() {
        let data: Vec<f64> = (0..40).map(|i| (i % 4) as f64).collect();
        let r = run(&DiscUniform, &data, &GofConfig::default());
        assert!(r.anderson_darling.is_none());
        let ks = r.ks.unwrap();
        assert!(ks.statistic < 1e-12 && ks.method.contains("discreto"));
        let c = r.chi_square.unwrap();
        // 4 classes of expected 10, no parameters: dof = 3, χ² = 0.
        assert!(c.method.contains("3 graus") && c.statistic == 0.0 && c.p_value == 1.0);
        // Skewed counts are rejected.
        let skew: Vec<f64> = (0..60)
            .map(|i| if i < 40 { 0.0 } else { (i % 4) as f64 })
            .collect();
        let r = run(&DiscUniform, &skew, &GofConfig::default());
        assert!(r.chi_square.unwrap().p_value < 0.01);
        assert!(r.ks.unwrap().p_value < 0.01);
    }

    #[test]
    fn type_one_error_is_close_to_alpha() {
        let cfg = GofConfig {
            bootstrap: 99,
            ..GofConfig::default()
        };
        let t = Instant::now();
        let (mut ks_rej, mut ad_rej, reps) = (0, 0, 200);
        for r in 0..reps {
            let data = exp_sample(50, 2.0, 1000 + r as u64);
            let cfg = GofConfig {
                seed: r as u64,
                ..cfg.clone()
            };
            let res = run(&Exponential, &data, &cfg);
            ks_rej += (res.ks.unwrap().p_value < 0.05) as usize;
            ad_rej += (res.anderson_darling.unwrap().p_value < 0.05) as usize;
        }
        let (k, a) = (ks_rej as f64 / reps as f64, ad_rej as f64 / reps as f64);
        eprintln!("type I error: KS {k}, AD {a}, took {:?}", t.elapsed());
        assert!((0.02..=0.09).contains(&k), "KS {k}");
        assert!((0.02..=0.09).contains(&a), "AD {a}");
    }
}
