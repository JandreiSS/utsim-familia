//! Process capability: how the process spread compares with the specification.
//!
//! - **σ dentro** (short term, from the subgroups: `MR̄/d2`, `R̄/d2` or `S̄/c4`, declared)
//!   gives `Cp = (LSE − LIE)/6σ`, `Cpu = (LSE − μ)/3σ`, `Cpl = (μ − LIE)/3σ` and
//!   `Cpk = min(Cpu, Cpl)`; **σ global** (the sample standard deviation of all values)
//!   gives `Pp`, `Ppu`, `Ppl`, `Ppk`; `Cpm = (LSE − LIE)/(6√(s² + (μ − T)²))`.
//! - Intervals: `Cpk ± z√(1/(9n) + Cpk²/(2(n − 1)))` (Bissell, 1990) and the χ² interval of
//!   `Cp`/`Pp`, `Cp·√(χ²_{α/2; n−1}/(n − 1))` to `Cp·√(χ²_{1−α/2; n−1}/(n − 1))`.
//! - PPM out of specification: observed (counted) and expected (normal with σ dentro and
//!   with σ global); `Z_bench = Φ⁻¹(1 − p_total)`.
//! - **Nível sigma** = `Z_bench` (global) + the shift, **1.5 by default and explicit**
//!   (the long-term shift convention of Six Sigma); `shift = 0` gives the short-term `Z`.
//! - Non-normal data: [`box_cox_capability`] (indices on the transformed data and limits)
//!   and [`percentile_capability`] (`(LSE − P50)/(P99.865 − P50)` and the lower equivalent,
//!   with any quantile function: a distribution fitted by `utsim-fit` or the empirical
//!   one).

use serde::{Deserialize, Serialize};
use utsim_stats::special::{norm_cdf, norm_ppf};

use crate::constants::Constants;
use crate::normality::{self, AndersonDarling, BoxCoxFit};
use crate::prob::chi2_ppf;

/// Specification limits and target.
#[derive(Debug, Clone, Copy, PartialEq, Default, Serialize, Deserialize)]
pub struct Spec {
    pub lsl: Option<f64>,
    pub usl: Option<f64>,
    /// Target; the middle of the limits when both exist and no target is given.
    pub target: Option<f64>,
}

impl Spec {
    pub fn validate(&self) -> Result<(), String> {
        if self.lsl.is_none() && self.usl.is_none() {
            return Err("informe ao menos um limite de especificação (LIE ou LSE)".into());
        }
        if let (Some(l), Some(u)) = (self.lsl, self.usl)
            && !(l < u)
        {
            return Err(format!("LIE ({l}) deve ser menor que LSE ({u})"));
        }
        Ok(())
    }

    /// Target used by `Cpm`.
    pub fn target_or_mid(&self) -> Option<f64> {
        self.target.or(match (self.lsl, self.usl) {
            (Some(l), Some(u)) => Some(0.5 * (l + u)),
            _ => None,
        })
    }
}

/// How σ within is estimated.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum WithinMethod {
    /// `MR̄/d2(2)` over consecutive individual values.
    MovingRange,
    /// `R̄/d2(n)` over the subgroups.
    RBar,
    /// `S̄/c4(n)` over the subgroups.
    SBar,
}

impl WithinMethod {
    pub fn label(&self) -> &'static str {
        match self {
            WithinMethod::MovingRange => "MR̄/d2 (amplitude móvel)",
            WithinMethod::RBar => "R̄/d2 (amplitude média dos subgrupos)",
            WithinMethod::SBar => "S̄/c4 (desvio médio dos subgrupos)",
        }
    }
}

/// Options of the capability study.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Options {
    /// `None`: moving range for individual values, `R̄/d2` for subgroups.
    pub within: Option<WithinMethod>,
    /// Confidence level of the intervals.
    pub level: f64,
    /// Shift added to `Z` in the sigma level (1.5 by convention; 0 for none).
    pub sigma_shift: f64,
}

impl Default for Options {
    fn default() -> Self {
        Options {
            within: None,
            level: 0.95,
            sigma_shift: 1.5,
        }
    }
}

/// Parts per million below, above and in total.
#[derive(Debug, Clone, Copy, PartialEq, Default, Serialize, Deserialize)]
pub struct Ppm {
    pub below: f64,
    pub above: f64,
    pub total: f64,
}

/// Index with its interval.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Index {
    pub value: f64,
    pub lo: f64,
    pub hi: f64,
}

/// Result of a capability study.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Capability {
    pub n: usize,
    pub subgroup_size: usize,
    pub mean: f64,
    pub sigma_within: f64,
    pub within_method: WithinMethod,
    pub sigma_overall: f64,
    pub spec: Spec,
    pub cp: Option<Index>,
    pub cpu: Option<f64>,
    pub cpl: Option<f64>,
    pub cpk: Index,
    pub pp: Option<Index>,
    pub ppu: Option<f64>,
    pub ppl: Option<f64>,
    pub ppk: Index,
    pub cpm: Option<f64>,
    pub level: f64,
    pub ppm_observed: Ppm,
    pub ppm_within: Ppm,
    pub ppm_overall: Ppm,
    pub z_bench_within: f64,
    pub z_bench_overall: f64,
    pub sigma_shift: f64,
    /// `z_bench_overall + sigma_shift`.
    pub sigma_level: f64,
    pub normality: Option<AndersonDarling>,
    pub notes: Vec<String>,
}

fn mean(xs: &[f64]) -> f64 {
    xs.iter().sum::<f64>() / xs.len() as f64
}

fn sample_sd(xs: &[f64]) -> f64 {
    let m = mean(xs);
    (xs.iter().map(|x| (x - m).powi(2)).sum::<f64>() / (xs.len() as f64 - 1.0)).sqrt()
}

/// σ within the subgroups by `method`.
pub fn sigma_within(groups: &[Vec<f64>], method: WithinMethod) -> Result<f64, String> {
    match method {
        WithinMethod::MovingRange => {
            let x: Vec<f64> = groups.iter().flatten().copied().collect();
            if x.len() < 2 {
                return Err("amplitude móvel exige ao menos 2 valores".into());
            }
            let mr = x.windows(2).map(|w| (w[1] - w[0]).abs()).sum::<f64>() / (x.len() - 1) as f64;
            Ok(mr / Constants::new(2).d2)
        }
        WithinMethod::RBar | WithinMethod::SBar => {
            let n = groups.first().map_or(0, Vec::len);
            if n < 2 || groups.iter().any(|g| g.len() != n) {
                return Err(
                    "R̄/d2 e S̄/c4 exigem subgrupos do mesmo tamanho, com ao menos 2 valores".into(),
                );
            }
            let c = Constants::new(n);
            let k = groups.len() as f64;
            Ok(if method == WithinMethod::RBar {
                groups
                    .iter()
                    .map(|g| {
                        let (lo, hi) =
                            g.iter().fold((f64::INFINITY, f64::NEG_INFINITY), |a, &x| {
                                (a.0.min(x), a.1.max(x))
                            });
                        hi - lo
                    })
                    .sum::<f64>()
                    / k
                    / c.d2
            } else {
                groups.iter().map(|g| sample_sd(g)).sum::<f64>() / k / c.c4
            })
        }
    }
}

/// Expected fraction below `lsl` and above `usl` for a normal `(μ, σ)`.
fn expected(spec: &Spec, mu: f64, sigma: f64) -> Ppm {
    let below = spec.lsl.map_or(0.0, |l| norm_cdf((l - mu) / sigma));
    let above = spec.usl.map_or(0.0, |u| norm_cdf((mu - u) / sigma));
    Ppm {
        below: below * 1e6,
        above: above * 1e6,
        total: (below + above) * 1e6,
    }
}

/// `Φ⁻¹(1 − p)` for a fraction `p` (∞ at 0).
fn z_of(p: f64) -> f64 {
    if p <= 0.0 {
        f64::INFINITY
    } else if p >= 1.0 {
        f64::NEG_INFINITY
    } else {
        -norm_ppf(p)
    }
}

/// Bissell's interval of `Cpk` (or of a one-sided index) with `n` values.
pub fn bissell(cpk: f64, n: usize, level: f64) -> (f64, f64) {
    let nf = n as f64;
    let z = norm_ppf(1.0 - (1.0 - level) / 2.0);
    let se = (1.0 / (9.0 * nf) + cpk * cpk / (2.0 * (nf - 1.0))).sqrt();
    (cpk - z * se, cpk + z * se)
}

/// χ² interval of `Cp` (or `Pp`) with `df` degrees of freedom.
pub fn cp_interval(cp: f64, df: f64, level: f64) -> (f64, f64) {
    let a = (1.0 - level) / 2.0;
    (
        cp * (chi2_ppf(a, df) / df).sqrt(),
        cp * (chi2_ppf(1.0 - a, df) / df).sqrt(),
    )
}

/// Capability of normal data. `groups` are the subgroups in time order (size 1 for
/// individual values).
pub fn capability(
    groups: &[Vec<f64>],
    spec: &Spec,
    options: &Options,
) -> Result<Capability, String> {
    spec.validate()?;
    let all: Vec<f64> = groups.iter().flatten().copied().collect();
    let n = all.len();
    if n < 2 {
        return Err("capacidade exige ao menos 2 valores".into());
    }
    if all.iter().any(|x| !x.is_finite()) {
        return Err("há valores não numéricos ou infinitos".into());
    }
    let size = groups.first().map_or(1, Vec::len);
    let equal = groups.iter().all(|g| g.len() == size);
    let method = options.within.unwrap_or(if size >= 2 && equal {
        WithinMethod::RBar
    } else {
        WithinMethod::MovingRange
    });
    let sw = sigma_within(groups, method)?;
    let mu = mean(&all);
    let so = sample_sd(&all);
    if !(sw > 0.0 && so > 0.0) {
        return Err("dados sem variação: os índices não existem".into());
    }
    let level = options.level;
    let df = (n - 1) as f64;
    let mut notes = Vec::new();

    let idx = |s: f64| {
        let cp = match (spec.lsl, spec.usl) {
            (Some(l), Some(u)) => Some((u - l) / (6.0 * s)),
            _ => None,
        };
        let cu = spec.usl.map(|u| (u - mu) / (3.0 * s));
        let cl = spec.lsl.map(|l| (mu - l) / (3.0 * s));
        let k = match (cu, cl) {
            (Some(a), Some(b)) => a.min(b),
            (Some(a), None) | (None, Some(a)) => a,
            _ => unreachable!(),
        };
        (cp, cu, cl, k)
    };
    let (cp, cpu, cpl, cpk) = idx(sw);
    let (pp, ppu, ppl, ppk) = idx(so);
    let with_ci = |v: f64| {
        let (lo, hi) = cp_interval(v, df, level);
        Index { value: v, lo, hi }
    };
    let with_bissell = |v: f64| {
        let (lo, hi) = bissell(v, n, level);
        Index { value: v, lo, hi }
    };
    let cpm = match (spec.lsl, spec.usl, spec.target_or_mid()) {
        (Some(l), Some(u), Some(t)) => Some((u - l) / (6.0 * (so * so + (mu - t).powi(2)).sqrt())),
        _ => None,
    };

    let below = spec
        .lsl
        .map_or(0, |l| all.iter().filter(|&&x| x < l).count());
    let above = spec
        .usl
        .map_or(0, |u| all.iter().filter(|&&x| x > u).count());
    let ppm_observed = Ppm {
        below: below as f64 / n as f64 * 1e6,
        above: above as f64 / n as f64 * 1e6,
        total: (below + above) as f64 / n as f64 * 1e6,
    };
    let ppm_within = expected(spec, mu, sw);
    let ppm_overall = expected(spec, mu, so);
    let z_bench_within = z_of(ppm_within.total / 1e6);
    let z_bench_overall = z_of(ppm_overall.total / 1e6);

    let normality = normality::anderson_darling(&all);
    if let Some(ad) = normality
        && ad.p_value < 0.05
    {
        notes.push(format!(
            "o teste de Anderson-Darling rejeita a normalidade (p = {:.4}): use Box-Cox ou o \
             método dos percentis",
            ad.p_value
        ));
    }
    if n < 30 {
        notes.push(format!(
            "só {n} valores: os intervalos dos índices são largos"
        ));
    }
    Ok(Capability {
        n,
        subgroup_size: if equal { size } else { 0 },
        mean: mu,
        sigma_within: sw,
        within_method: method,
        sigma_overall: so,
        spec: *spec,
        cp: cp.map(with_ci),
        cpu,
        cpl,
        cpk: with_bissell(cpk),
        pp: pp.map(with_ci),
        ppu,
        ppl,
        ppk: with_bissell(ppk),
        cpm,
        level,
        ppm_observed,
        ppm_within,
        ppm_overall,
        z_bench_within,
        z_bench_overall,
        sigma_shift: options.sigma_shift,
        sigma_level: z_bench_overall + options.sigma_shift,
        normality,
        notes,
    })
}

/// Defects per million opportunities.
pub fn dpmo(defects: f64, units: f64, opportunities: f64) -> f64 {
    defects / (units * opportunities) * 1e6
}

/// Sigma level of a DPMO: `Φ⁻¹(1 − DPMO/10⁶) + shift` (shift 1.5 by convention).
pub fn sigma_level(dpmo: f64, shift: f64) -> f64 {
    z_of(dpmo / 1e6) + shift
}

/// DPMO of a sigma level (inverse of [`sigma_level`]).
pub fn dpmo_of_sigma(level: f64, shift: f64) -> f64 {
    norm_cdf(-(level - shift)) * 1e6
}

/// Rolled throughput yield: product of the first-pass yields of the steps.
pub fn rolled_throughput_yield(yields: &[f64]) -> f64 {
    yields.iter().product()
}

/// Quantile of sorted data with linear interpolation (`(n − 1)p`, the usual "type 7").
pub fn quantile_sorted(sorted: &[f64], p: f64) -> f64 {
    let n = sorted.len();
    if n == 0 {
        return f64::NAN;
    }
    let h = (n - 1) as f64 * p.clamp(0.0, 1.0);
    let lo = h.floor() as usize;
    let hi = (lo + 1).min(n - 1);
    sorted[lo] + (h - lo as f64) * (sorted[hi] - sorted[lo])
}

/// Indices by the percentile method.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct PercentileIndices {
    pub p_low: f64,
    pub median: f64,
    pub p_high: f64,
    pub pp: Option<f64>,
    pub ppu: Option<f64>,
    pub ppl: Option<f64>,
    pub ppk: f64,
}

/// Percentile method with the quantile function `q` of the process distribution:
/// `Pp = (LSE − LIE)/(q(0.99865) − q(0.00135))`, `Ppu = (LSE − q(0.5))/(q(0.99865) −
/// q(0.5))`, `Ppl = (q(0.5) − LIE)/(q(0.5) − q(0.00135))`. For a normal `q` they are the
/// classic `Pp`, `Ppu`, `Ppl`.
pub fn percentile_capability(
    q: &dyn Fn(f64) -> f64,
    spec: &Spec,
) -> Result<PercentileIndices, String> {
    spec.validate()?;
    let (lo, med, hi) = (q(0.00135), q(0.5), q(0.99865));
    if !(lo < med && med < hi) {
        return Err("quantis da distribuição fora de ordem".into());
    }
    let pp = match (spec.lsl, spec.usl) {
        (Some(l), Some(u)) => Some((u - l) / (hi - lo)),
        _ => None,
    };
    let ppu = spec.usl.map(|u| (u - med) / (hi - med));
    let ppl = spec.lsl.map(|l| (med - l) / (med - lo));
    let ppk = match (ppu, ppl) {
        (Some(a), Some(b)) => a.min(b),
        (Some(a), None) | (None, Some(a)) => a,
        _ => unreachable!(),
    };
    Ok(PercentileIndices {
        p_low: lo,
        median: med,
        p_high: hi,
        pp,
        ppu,
        ppl,
        ppk,
    })
}

/// Percentile method with the empirical quantiles of the data (needs many values: the
/// 0.135% and 99.865% quantiles rest on the extremes; at least 1000 recommended).
pub fn percentile_capability_empirical(
    xs: &[f64],
    spec: &Spec,
) -> Result<PercentileIndices, String> {
    if xs.len() < 100 {
        return Err(format!(
            "o método dos percentis empírico exige ao menos 100 valores (há {}); use uma \
             distribuição ajustada",
            xs.len()
        ));
    }
    let mut v = xs.to_vec();
    v.sort_by(|a, b| a.total_cmp(b));
    percentile_capability(&|p| quantile_sorted(&v, p), spec)
}

/// Capability after a Box-Cox transformation of the data and of the limits.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BoxCoxCapability {
    pub fit: BoxCoxFit,
    /// Normality of the original data.
    pub before: Option<AndersonDarling>,
    /// Indices on the transformed scale (its `normality` is the test after).
    pub transformed: Capability,
}

/// Box-Cox capability: fits `λ` (rounded to a usual value inside the interval when
/// `round`), transforms data, limits and target, and computes the indices.
pub fn box_cox_capability(
    groups: &[Vec<f64>],
    spec: &Spec,
    options: &Options,
    round: bool,
) -> Result<BoxCoxCapability, String> {
    spec.validate()?;
    let all: Vec<f64> = groups.iter().flatten().copied().collect();
    let fit = normality::box_cox_fit(&all, options.level, round)?;
    let l = fit.lambda;
    let t = |x: f64| -> Result<f64, String> {
        if x > 0.0 {
            Ok(normality::box_cox(x, l))
        } else {
            Err(format!("limite {x} não positivo: Box-Cox não se aplica"))
        }
    };
    let tspec = Spec {
        lsl: spec.lsl.map(t).transpose()?,
        usl: spec.usl.map(t).transpose()?,
        target: spec.target_or_mid().map(t).transpose()?,
    };
    let tgroups: Vec<Vec<f64>> = groups
        .iter()
        .map(|g| g.iter().map(|&x| normality::box_cox(x, l)).collect())
        .collect();
    let mut transformed = capability(&tgroups, &tspec, options)?;
    // Observed PPM must be counted on the original scale (identical, the map is monotone).
    transformed.notes.insert(
        0,
        format!("índices sobre os dados transformados por Box-Cox com λ = {l:.3}"),
    );
    Ok(BoxCoxCapability {
        fit,
        before: normality::anderson_darling(&all),
        transformed,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn singles(x: &[f64]) -> Vec<Vec<f64>> {
        x.iter().map(|&v| vec![v]).collect()
    }

    #[test]
    fn nist_capability_example() {
        // NIST/SEMATECH e-Handbook, section 6.1.6 ("What is process capability?"):
        // USL = 20, LSL = 8, μ = 16, σ = 2 give Cp = 1.0 and Cpk = min(4/6, 8/6) = 0.667.
        // [conferir os números da seção 6.1.6]
        // Two subgroups (14, 18): mean 16 and σ dentro = R̄/d2 = 4/d2(2); the limits keep
        // the ratios of the example: USL − μ = 2σ and μ − LSL = 4σ.
        let groups = vec![vec![14.0, 18.0], vec![14.0, 18.0]];
        let sw = 4.0 / Constants::new(2).d2;
        let spec = Spec {
            lsl: Some(16.0 - 4.0 * sw),
            usl: Some(16.0 + 2.0 * sw),
            target: None,
        };
        let c = capability(&groups, &spec, &Options::default()).unwrap();
        assert!((c.cp.unwrap().value - 1.0).abs() < 1e-12);
        assert!((c.cpk.value - 4.0 / 6.0).abs() < 1e-12);
        assert!((c.cpl.unwrap() - 8.0 / 6.0).abs() < 1e-12);
    }

    #[test]
    fn hard_bake_cp() {
        // Montgomery, "Introduction to Statistical Quality Control", capability chapter,
        // hard-bake process: LSL = 1.00, USL = 2.00 microns and σ̂ = R̄/d2 = 0.1398 give
        // Cp = 1/(6·0.1398) = 1.192. [conferir edição e exemplo]
        let cp: f64 = (2.00 - 1.00) / (6.0 * 0.1398);
        assert!((cp - 1.192).abs() < 5e-4);
    }

    #[test]
    fn index_properties() {
        let x: Vec<f64> = (0..50)
            .map(|i| 10.0 + ((i * 37) % 17) as f64 / 10.0)
            .collect();
        let spec = Spec {
            lsl: Some(9.0),
            usl: Some(12.5),
            target: None,
        };
        let c = capability(&singles(&x), &spec, &Options::default()).unwrap();
        assert_eq!(c.cpk.value, c.cpu.unwrap().min(c.cpl.unwrap()));
        assert_eq!(c.ppk.value, c.ppu.unwrap().min(c.ppl.unwrap()));
        assert!(c.cpk.value <= c.cp.unwrap().value);
        // (Cpu + Cpl)/2 = Cp.
        assert!(((c.cpu.unwrap() + c.cpl.unwrap()) / 2.0 - c.cp.unwrap().value).abs() < 1e-12);
        assert!(c.cpk.lo < c.cpk.value && c.cpk.value < c.cpk.hi);
        // Centred mean: Cpk = Cp.
        let m = c.mean;
        let centred = Spec {
            lsl: Some(m - 2.0),
            usl: Some(m + 2.0),
            target: None,
        };
        let c2 = capability(&singles(&x), &centred, &Options::default()).unwrap();
        assert!((c2.cpk.value - c2.cp.unwrap().value).abs() < 1e-12);
        // Cpm = Pp when the mean is on target.
        assert!((c2.cpm.unwrap() - c2.pp.unwrap().value).abs() < 1e-12);
    }

    #[test]
    fn bissell_interval_formula() {
        // Cpk = 1.33, n = 50, 95%: 1.33 ± 1.96·√(1/450 + 1.33²/98) = [1.051, 1.609].
        let (lo, hi) = bissell(1.33, 50, 0.95);
        let se = (1.0 / 450.0 + 1.33f64.powi(2) / 98.0).sqrt();
        assert!((lo - (1.33 - 1.959964 * se)).abs() < 1e-6);
        assert!((hi - 1.609).abs() < 1e-3);
    }

    #[test]
    fn classic_sigma_levels() {
        // Six Sigma convention with the 1.5σ shift: 3.4 DPMO ↔ 6σ, 233 ↔ 5σ, 6210 ↔ 4σ,
        // 66807 ↔ 3σ (the usual conversion table, e.g. Harry and Schroeder, "Six Sigma",
        // 2000). [conferir]
        for (d, s) in [(3.4, 6.0), (233.0, 5.0), (6210.0, 4.0), (66807.0, 3.0)] {
            assert!((sigma_level(d, 1.5) - s).abs() < 0.01, "{d}");
            assert!((dpmo_of_sigma(s, 1.5) / d - 1.0).abs() < 0.02);
        }
        // Without the shift, 66807 DPMO is Z = 1.5.
        assert!((sigma_level(66807.0, 0.0) - 1.5).abs() < 1e-3);
        assert_eq!(dpmo(5.0, 1000.0, 10.0), 500.0);
        assert!((rolled_throughput_yield(&[0.9, 0.95, 0.99]) - 0.84645).abs() < 1e-12);
    }

    #[test]
    fn expected_ppm_and_sigma_level() {
        // Symmetric data with σ global = 1 at ±3: 2700 PPM expected, Z_bench = 2.782.
        let mut x = Vec::new();
        for i in 0..1000 {
            x.push(norm_ppf((i as f64 + 0.5) / 1000.0));
        }
        let sd = sample_sd(&x);
        let spec = Spec {
            lsl: Some(-3.0 * sd),
            usl: Some(3.0 * sd),
            target: None,
        };
        let c = capability(&singles(&x), &spec, &Options::default()).unwrap();
        assert!((c.ppm_overall.total - 2699.8).abs() < 0.5);
        assert!((c.z_bench_overall - 2.782).abs() < 1e-3);
        assert!((c.sigma_level - (c.z_bench_overall + 1.5)).abs() < 1e-12);
        assert!((c.pp.unwrap().value - 1.0).abs() < 1e-12);
    }

    #[test]
    fn within_methods() {
        let g = vec![vec![1.0, 3.0, 2.0], vec![2.0, 2.0, 5.0]];
        let r = sigma_within(&g, WithinMethod::RBar).unwrap();
        assert!((r - 2.5 / Constants::new(3).d2).abs() < 1e-12);
        let s = sigma_within(&g, WithinMethod::SBar).unwrap();
        assert!((s - (1.0 + 3f64.sqrt()) / 2.0 / Constants::new(3).c4).abs() < 1e-12);
        let mr = sigma_within(&g, WithinMethod::MovingRange).unwrap();
        // 1, 3, 2, 2, 2, 5: MR = 2, 1, 0, 0, 3 → 1.2.
        assert!((mr - 1.2 / Constants::new(2).d2).abs() < 1e-12);
    }

    #[test]
    fn percentile_method_equals_classic_on_normal_quantiles() {
        let (mu, s) = (10.0, 0.5);
        let q = |p: f64| mu + s * norm_ppf(p);
        let spec = Spec {
            lsl: Some(8.8),
            usl: Some(11.9),
            target: None,
        };
        let r = percentile_capability(&q, &spec).unwrap();
        assert!((r.pp.unwrap() - 3.1 / (6.0 * s)).abs() < 1e-4);
        assert!((r.ppu.unwrap() - 1.9 / (3.0 * s)).abs() < 1e-4);
        assert!((r.ppk - 1.2 / (3.0 * s)).abs() < 1e-4);
    }

    #[test]
    fn one_sided_specs() {
        let x: Vec<f64> = (0..40).map(|i| 5.0 + (i % 7) as f64 * 0.3).collect();
        let spec = Spec {
            lsl: None,
            usl: Some(9.0),
            target: None,
        };
        let c = capability(&singles(&x), &spec, &Options::default()).unwrap();
        assert!(c.cp.is_none() && c.cpl.is_none() && c.cpm.is_none());
        assert_eq!(c.cpk.value, c.cpu.unwrap());
        assert_eq!(c.ppm_overall.below, 0.0);
        assert!(capability(&singles(&x), &Spec::default(), &Options::default()).is_err());
    }

    #[test]
    fn box_cox_capability_runs_on_skewed_data() {
        use rand::SeedableRng;
        use rand_distr::{Distribution, StandardNormal};
        let mut rng = rand_pcg::Pcg64::seed_from_u64(8);
        let x: Vec<f64> = (0..300)
            .map(|_| (Distribution::<f64>::sample(&StandardNormal, &mut rng) * 0.4).exp())
            .collect();
        let spec = Spec {
            lsl: None,
            usl: Some(4.0),
            target: None,
        };
        let r = box_cox_capability(&singles(&x), &spec, &Options::default(), true).unwrap();
        assert_eq!(r.fit.lambda, 0.0);
        // ln 4 = 1.386 against σ = 0.4: Ppk ≈ 1.386/(3·0.4) = 1.155.
        assert!((r.transformed.ppk.value - 1.155).abs() < 0.12);
        assert!(r.before.unwrap().p_value < r.transformed.normality.unwrap().p_value);
    }
}
