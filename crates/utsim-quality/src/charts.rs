//! Shewhart control charts: I-MR, X̄-R, X̄-S, p, np, c and u.
//!
//! **Phase I** ([`phase1`]) estimates the limits from the data, leaving out the subgroups
//! with an assignable cause the user registered (`excluded`); the excluded points are
//! still drawn, but neither the limits nor the rules use them. **Phase II** ([`apply`])
//! uses frozen [`Limits`] (from a phase I study, from standards or from the model).
//!
//! Variables limits are kept as `(μ, σ, n)`: `X̄: μ ± 3σ/√n`, `R: [D1σ, d2σ, D2σ]`,
//! `S: [B5σ, c4σ, B6σ]`, `MR (n = 2): [0, d2σ, D2σ]`. With `σ̂ = R̄/d2` they are exactly the
//! textbook `x̿ ± A2·R̄`, `[D3·R̄, D4·R̄]` (and `σ̂ = S̄/c4` gives `x̿ ± A3·S̄`, `[B3·S̄, B4·S̄]`).
//! Attribute charts use `p̄ ± 3√(p̄(1−p̄)/nᵢ)`, `np̄ ± 3√(np̄(1−p̄))`, `c̄ ± 3√c̄` and
//! `ū ± 3√(ū/nᵢ)`, with the lower limit truncated at 0 and limits that vary with `nᵢ`.

use serde::{Deserialize, Serialize};

use crate::constants::Constants;
use crate::rules::{Rule, RuleSet, Violation};

/// The chart types.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ChartKind {
    /// Individuals and moving range.
    IMr,
    /// Subgroup means and ranges.
    XbarR,
    /// Subgroup means and standard deviations.
    XbarS,
    /// Proportion nonconforming (variable n allowed).
    P,
    /// Number nonconforming (constant n).
    Np,
    /// Number of nonconformities (constant inspection unit).
    C,
    /// Nonconformities per unit (variable n allowed).
    U,
}

impl ChartKind {
    /// Parses `imr`, `xbarr`, `xbars`, `p`, `np`, `c` or `u` (also `i-mr`, `xbar-r`...).
    pub fn parse(text: &str) -> Option<Self> {
        let t: String = text
            .to_lowercase()
            .chars()
            .filter(|c| c.is_alphanumeric())
            .collect();
        Some(match t.as_str() {
            "imr" | "i" | "individuais" => ChartKind::IMr,
            "xbarr" | "xr" | "mediaamplitude" => ChartKind::XbarR,
            "xbars" | "xs" | "mediadesvio" => ChartKind::XbarS,
            "p" => ChartKind::P,
            "np" => ChartKind::Np,
            "c" => ChartKind::C,
            "u" => ChartKind::U,
            _ => return None,
        })
    }

    /// Name in Portuguese.
    pub fn label(&self) -> &'static str {
        match self {
            ChartKind::IMr => "I-MR (valores individuais e amplitude móvel)",
            ChartKind::XbarR => "X̄-R (médias e amplitudes)",
            ChartKind::XbarS => "X̄-S (médias e desvios)",
            ChartKind::P => "p (proporção de defeituosos)",
            ChartKind::Np => "np (número de defeituosos)",
            ChartKind::C => "c (número de defeitos)",
            ChartKind::U => "u (defeitos por unidade)",
        }
    }

    pub fn is_attribute(&self) -> bool {
        matches!(
            self,
            ChartKind::P | ChartKind::Np | ChartKind::C | ChartKind::U
        )
    }
}

/// The data of a chart.
#[derive(Debug, Clone, PartialEq)]
pub enum ChartData {
    /// One vector per subgroup (size 1 for I-MR).
    Variables(Vec<Vec<f64>>),
    /// Counts (defectives or defects) and sizes (units inspected; 1 for a c chart).
    Attributes { counts: Vec<f64>, sizes: Vec<f64> },
}

impl ChartData {
    pub fn len(&self) -> usize {
        match self {
            ChartData::Variables(g) => g.len(),
            ChartData::Attributes { counts, .. } => counts.len(),
        }
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

/// Frozen limits (what phase II needs). Serialisable, so a phase I study can be saved.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Limits {
    /// I-MR, X̄-R or X̄-S: process mean, σ and subgroup size.
    Variables {
        kind: ChartKind,
        n: usize,
        mean: f64,
        sigma: f64,
    },
    /// p and np charts: proportion nonconforming (np also keeps `n`).
    Proportion { kind: ChartKind, pbar: f64, n: f64 },
    /// c and u charts: mean count per unit.
    Count { kind: ChartKind, mean: f64 },
    /// Single chart with constant limits given directly (e.g. quantiles from the model).
    Fixed { center: f64, lcl: f64, ucl: f64 },
}

impl Limits {
    pub fn kind(&self) -> Option<ChartKind> {
        match self {
            Limits::Variables { kind, .. }
            | Limits::Proportion { kind, .. }
            | Limits::Count { kind, .. } => Some(*kind),
            Limits::Fixed { .. } => None,
        }
    }
}

/// One plotted statistic with its limits, point by point.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Panel {
    pub name: String,
    /// `None` where the statistic does not exist (the first moving range).
    pub values: Vec<Option<f64>>,
    pub center: Vec<f64>,
    pub lcl: Vec<f64>,
    pub ucl: Vec<f64>,
    pub violations: Vec<Violation>,
}

impl Panel {
    /// Standardised distance to the centre, `(x − c)/((LSC − c)/3)`.
    pub fn z(&self) -> Vec<Option<f64>> {
        self.values
            .iter()
            .enumerate()
            .map(|(i, v)| {
                v.map(|x| {
                    let s = (self.ucl[i] - self.center[i]) / 3.0;
                    if s > 0.0 {
                        (x - self.center[i]) / s
                    } else {
                        0.0
                    }
                })
            })
            .collect()
    }

    /// Indices of the points beyond the limits.
    pub fn out_of_limits(&self) -> Vec<usize> {
        self.values
            .iter()
            .enumerate()
            .filter_map(|(i, v)| v.filter(|&x| x > self.ucl[i] || x < self.lcl[i]).map(|_| i))
            .collect()
    }
}

/// Phase of the study.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Phase {
    /// Limits estimated from these data.
    I,
    /// Limits frozen (given).
    II,
}

/// A computed control chart.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Chart {
    pub kind: Option<ChartKind>,
    pub phase: Phase,
    pub limits: Limits,
    /// Location panel (or the only one), then the dispersion panel.
    pub panels: Vec<Panel>,
    /// Subgroups left out of the estimation (0-based).
    pub excluded: Vec<usize>,
    pub rules: RuleSet,
    pub notes: Vec<String>,
}

impl Chart {
    /// Whether no rule fired on any panel.
    pub fn in_control(&self) -> bool {
        self.panels.iter().all(|p| p.violations.is_empty())
    }
}

fn mean(xs: &[f64]) -> f64 {
    xs.iter().sum::<f64>() / xs.len() as f64
}

fn range(xs: &[f64]) -> f64 {
    let (lo, hi) = xs
        .iter()
        .fold((f64::INFINITY, f64::NEG_INFINITY), |(a, b), &x| {
            (a.min(x), b.max(x))
        });
    hi - lo
}

fn sd(xs: &[f64]) -> f64 {
    let m = mean(xs);
    (xs.iter().map(|x| (x - m).powi(2)).sum::<f64>() / (xs.len() as f64 - 1.0)).sqrt()
}

/// Common subgroup size, or an error.
fn common_size(groups: &[Vec<f64>], kind: ChartKind) -> Result<usize, String> {
    let n = groups.first().map_or(0, Vec::len);
    if groups.iter().any(|g| g.len() != n) {
        return Err(format!(
            "a carta {} exige subgrupos do mesmo tamanho (há subgrupos com {} e {} valores)",
            kind.label(),
            n,
            groups.iter().map(Vec::len).find(|&m| m != n).unwrap_or(n)
        ));
    }
    Ok(n)
}

fn check(kind: ChartKind, data: &ChartData) -> Result<(), String> {
    match (kind.is_attribute(), data) {
        (false, ChartData::Variables(g)) => {
            if g.iter().flatten().any(|x| !x.is_finite()) {
                return Err("há valores não numéricos ou infinitos".into());
            }
            Ok(())
        }
        (true, ChartData::Attributes { counts, sizes }) => {
            if counts.len() != sizes.len() {
                return Err("contagens e tamanhos com comprimentos diferentes".into());
            }
            for (i, (&c, &n)) in counts.iter().zip(sizes).enumerate() {
                if !(c >= 0.0 && n > 0.0) || !c.is_finite() || !n.is_finite() {
                    return Err(format!(
                        "subgrupo {}: contagem {c} e tamanho {n} inválidos (contagem ≥ 0, tamanho > 0)",
                        i + 1
                    ));
                }
                if matches!(kind, ChartKind::P | ChartKind::Np) && c > n {
                    return Err(format!(
                        "subgrupo {}: {c} defeituosos em {n} unidades",
                        i + 1
                    ));
                }
            }
            Ok(())
        }
        _ => Err(format!(
            "a carta {} recebeu dados do tipo errado ({})",
            kind.label(),
            if kind.is_attribute() {
                "esperava contagens e tamanhos"
            } else {
                "esperava medidas"
            }
        )),
    }
}

/// Phase I: estimates the limits without the `excluded` subgroups (0-based indices).
pub fn estimate_limits(
    kind: ChartKind,
    data: &ChartData,
    excluded: &[usize],
) -> Result<Limits, String> {
    check(kind, data)?;
    let keep = |i: usize| !excluded.contains(&i);
    let used = (0..data.len()).filter(|&i| keep(i)).count();
    let min = if kind == ChartKind::IMr { 2 } else { 1 };
    if used < min {
        return Err(format!(
            "dados insuficientes para estimar os limites ({used} subgrupos usados)"
        ));
    }
    match (kind, data) {
        (ChartKind::IMr, ChartData::Variables(groups)) => {
            if groups.iter().any(|g| g.len() != 1) {
                return Err(
                    "a carta I-MR recebe um valor por linha (subgrupos de tamanho 1); \
                     use X̄-R ou X̄-S para subgrupos"
                        .into(),
                );
            }
            let x: Vec<f64> = groups.iter().map(|g| g[0]).collect();
            let kept: Vec<f64> = (0..x.len()).filter(|&i| keep(i)).map(|i| x[i]).collect();
            // Moving ranges between consecutive points, neither excluded.
            let mr: Vec<f64> = (1..x.len())
                .filter(|&i| keep(i) && keep(i - 1))
                .map(|i| (x[i] - x[i - 1]).abs())
                .collect();
            if mr.is_empty() {
                return Err("nenhuma amplitude móvel entre pontos consecutivos usados".into());
            }
            Ok(Limits::Variables {
                kind,
                n: 1,
                mean: mean(&kept),
                sigma: mean(&mr) / Constants::new(2).d2,
            })
        }
        (ChartKind::XbarR | ChartKind::XbarS, ChartData::Variables(groups)) => {
            let n = common_size(groups, kind)?;
            if n < 2 {
                return Err(format!(
                    "a carta {} exige subgrupos com pelo menos 2 valores; use I-MR",
                    kind.label()
                ));
            }
            let c = Constants::new(n);
            let kept: Vec<&Vec<f64>> = (0..groups.len())
                .filter(|&i| keep(i))
                .map(|i| &groups[i])
                .collect();
            let grand = kept.iter().map(|g| mean(g)).sum::<f64>() / kept.len() as f64;
            let sigma = if kind == ChartKind::XbarR {
                kept.iter().map(|g| range(g)).sum::<f64>() / kept.len() as f64 / c.d2
            } else {
                kept.iter().map(|g| sd(g)).sum::<f64>() / kept.len() as f64 / c.c4
            };
            Ok(Limits::Variables {
                kind,
                n,
                mean: grand,
                sigma,
            })
        }
        (_, ChartData::Attributes { counts, sizes }) => {
            let (mut c, mut n) = (0.0, 0.0);
            for i in (0..counts.len()).filter(|&i| keep(i)) {
                c += counts[i];
                n += sizes[i];
            }
            match kind {
                ChartKind::P => Ok(Limits::Proportion {
                    kind,
                    pbar: c / n,
                    n: f64::NAN,
                }),
                ChartKind::Np => {
                    let n0 = sizes[0];
                    if sizes.iter().any(|&s| s != n0) {
                        return Err("a carta np exige o mesmo tamanho de amostra em todos os \
                             subgrupos; use a carta p"
                            .into());
                    }
                    Ok(Limits::Proportion {
                        kind,
                        pbar: c / n,
                        n: n0,
                    })
                }
                ChartKind::C => {
                    if sizes.iter().any(|&s| s != sizes[0]) {
                        return Err("a carta c exige a mesma unidade de inspeção em todos os \
                             subgrupos; use a carta u"
                            .into());
                    }
                    let k = (0..counts.len()).filter(|&i| keep(i)).count() as f64;
                    Ok(Limits::Count { kind, mean: c / k })
                }
                ChartKind::U => Ok(Limits::Count { kind, mean: c / n }),
                _ => unreachable!(),
            }
        }
        _ => unreachable!("checked above"),
    }
}

/// Applies frozen limits to the data (phase II), evaluating the rules. `excluded` points
/// are drawn but skipped by the rules.
pub fn apply(
    limits: &Limits,
    data: &ChartData,
    rules: &RuleSet,
    excluded: &[usize],
) -> Result<Chart, String> {
    let k = data.len();
    if k == 0 {
        return Err("nenhum subgrupo".into());
    }
    let mut notes = Vec::new();
    let panels = match limits {
        Limits::Variables {
            kind,
            n,
            mean: mu,
            sigma,
        } => {
            check(*kind, data)?;
            let ChartData::Variables(groups) = data else {
                unreachable!()
            };
            let (mu, sigma) = (*mu, *sigma);
            if !(sigma > 0.0) {
                return Err(format!("σ dos limites inválido ({sigma})"));
            }
            match kind {
                ChartKind::IMr => {
                    if groups.iter().any(|g| g.len() != 1) {
                        return Err("a carta I-MR recebe um valor por linha".into());
                    }
                    let x: Vec<f64> = groups.iter().map(|g| g[0]).collect();
                    let c2 = Constants::new(2);
                    let i_panel = constant_panel(
                        "Valores individuais (I)",
                        x.iter().map(|&v| Some(v)).collect(),
                        mu,
                        mu - 3.0 * sigma,
                        mu + 3.0 * sigma,
                    );
                    let mr: Vec<Option<f64>> = (0..x.len())
                        .map(|i| (i > 0).then(|| (x[i] - x[i - 1]).abs()))
                        .collect();
                    let mr_panel = constant_panel(
                        "Amplitude móvel (MR)",
                        mr,
                        c2.d2 * sigma,
                        0.0,
                        c2.D2 * sigma,
                    );
                    vec![i_panel, mr_panel]
                }
                ChartKind::XbarR | ChartKind::XbarS => {
                    let m = common_size(groups, *kind)?;
                    if m != *n {
                        return Err(format!(
                            "os limites são para subgrupos de {n} e os dados têm {m}"
                        ));
                    }
                    let c = Constants::new(m);
                    let means = groups.iter().map(|g| Some(mean(g))).collect();
                    let x_panel = constant_panel(
                        "Médias dos subgrupos (X̄)",
                        means,
                        mu,
                        mu - c.A * sigma,
                        mu + c.A * sigma,
                    );
                    let disp = if *kind == ChartKind::XbarR {
                        constant_panel(
                            "Amplitudes (R)",
                            groups.iter().map(|g| Some(range(g))).collect(),
                            c.d2 * sigma,
                            c.D1 * sigma,
                            c.D2 * sigma,
                        )
                    } else {
                        constant_panel(
                            "Desvios-padrão (S)",
                            groups.iter().map(|g| Some(sd(g))).collect(),
                            c.c4 * sigma,
                            c.B5 * sigma,
                            c.B6 * sigma,
                        )
                    };
                    vec![x_panel, disp]
                }
                _ => return Err("limites de variáveis com carta de atributos".into()),
            }
        }
        Limits::Proportion { kind, pbar, n } => {
            check(*kind, data)?;
            let ChartData::Attributes { counts, sizes } = data else {
                unreachable!()
            };
            let p = *pbar;
            if !(0.0..=1.0).contains(&p) {
                return Err(format!("proporção dos limites inválida ({p})"));
            }
            if *kind == ChartKind::Np {
                if sizes.iter().any(|&s| s != *n) {
                    return Err(format!(
                        "os limites np são para n = {n}; use a carta p para n variável"
                    ));
                }
                let c = n * p;
                let s = (n * p * (1.0 - p)).sqrt();
                vec![constant_panel(
                    "Número de defeituosos (np)",
                    counts.iter().map(|&v| Some(v)).collect(),
                    c,
                    (c - 3.0 * s).max(0.0),
                    c + 3.0 * s,
                )]
            } else {
                let mut panel = Panel {
                    name: "Proporção de defeituosos (p)".into(),
                    values: counts.iter().zip(sizes).map(|(c, n)| Some(c / n)).collect(),
                    center: vec![p; k],
                    lcl: Vec::with_capacity(k),
                    ucl: Vec::with_capacity(k),
                    violations: vec![],
                };
                for &ni in sizes {
                    let s = (p * (1.0 - p) / ni).sqrt();
                    panel.lcl.push((p - 3.0 * s).max(0.0));
                    panel.ucl.push((p + 3.0 * s).min(1.0));
                }
                if sizes.iter().any(|&s| s != sizes[0]) {
                    notes.push("tamanhos de amostra diferentes: limites variáveis".into());
                }
                vec![panel]
            }
        }
        Limits::Count { kind, mean: u } => {
            check(*kind, data)?;
            let ChartData::Attributes { counts, sizes } = data else {
                unreachable!()
            };
            let u = *u;
            if !(u >= 0.0) {
                return Err(format!("média dos limites inválida ({u})"));
            }
            if *kind == ChartKind::C {
                let s = u.sqrt();
                vec![constant_panel(
                    "Número de defeitos (c)",
                    counts.iter().map(|&v| Some(v)).collect(),
                    u,
                    (u - 3.0 * s).max(0.0),
                    u + 3.0 * s,
                )]
            } else {
                let mut panel = Panel {
                    name: "Defeitos por unidade (u)".into(),
                    values: counts.iter().zip(sizes).map(|(c, n)| Some(c / n)).collect(),
                    center: vec![u; k],
                    lcl: Vec::with_capacity(k),
                    ucl: Vec::with_capacity(k),
                    violations: vec![],
                };
                for &ni in sizes {
                    let s = (u / ni).sqrt();
                    panel.lcl.push((u - 3.0 * s).max(0.0));
                    panel.ucl.push(u + 3.0 * s);
                }
                if sizes.iter().any(|&s| s != sizes[0]) {
                    notes.push("tamanhos de amostra diferentes: limites variáveis".into());
                }
                vec![panel]
            }
        }
        Limits::Fixed { center, lcl, ucl } => {
            if !(lcl < center && center < ucl) {
                return Err(format!(
                    "limites fixos fora de ordem: LIC {lcl}, LC {center}, LSC {ucl}"
                ));
            }
            let values: Vec<Option<f64>> = match data {
                ChartData::Variables(g) => g.iter().map(|g| Some(mean(g))).collect(),
                ChartData::Attributes { counts, sizes } => {
                    counts.iter().zip(sizes).map(|(c, n)| Some(c / n)).collect()
                }
            };
            notes.push(
                "limites fixos (por exemplo, quantis da distribuição no modelo): as regras \
                 de zona supõem limites simétricos a 3σ"
                    .into(),
            );
            vec![constant_panel("Valores", values, *center, *lcl, *ucl)]
        }
    };
    let mut panels = panels;
    for (pi, panel) in panels.iter_mut().enumerate() {
        let mut z = panel.z();
        for &e in excluded {
            if e < z.len() {
                z[e] = None;
            }
        }
        // Zone and run rules on the location chart; only "beyond the limits" on the
        // dispersion chart (moving ranges are correlated, ranges are skewed).
        let set = if pi == 0 {
            rules.clone()
        } else {
            RuleSet(vec![Rule::Beyond3])
        };
        panel.violations = beyond_or_rules(panel, &z, &set);
    }
    Ok(Chart {
        kind: limits.kind(),
        phase: Phase::II,
        limits: limits.clone(),
        panels,
        excluded: excluded.to_vec(),
        rules: rules.clone(),
        notes,
    })
}

/// Rule 1 is "beyond the limits" (also below a lower limit truncated at 0 never fires);
/// the other rules run on `z`.
#[allow(clippy::needless_range_loop)] // `k` indexes the values and their positions
fn beyond_or_rules(panel: &Panel, z: &[Option<f64>], set: &RuleSet) -> Vec<Violation> {
    let mut out = Vec::new();
    let others = RuleSet(
        set.0
            .iter()
            .copied()
            .filter(|r| *r != Rule::Beyond3)
            .collect(),
    );
    let beyond = set.0.contains(&Rule::Beyond3);
    let (idx, vals): (Vec<usize>, Vec<f64>) = z
        .iter()
        .enumerate()
        .filter_map(|(i, v)| v.map(|v| (i, v)))
        .unzip();
    for k in 0..vals.len() {
        let i = idx[k];
        if beyond {
            let x = panel.values[i].unwrap_or(f64::NAN);
            if x > panel.ucl[i] || x < panel.lcl[i] {
                out.push(Violation {
                    rule: Rule::Beyond3,
                    index: i,
                });
            }
        }
        for rule in &others.0 {
            if rule.fires_at(&vals, k) {
                out.push(Violation {
                    rule: *rule,
                    index: i,
                });
            }
        }
    }
    out
}

fn constant_panel(name: &str, values: Vec<Option<f64>>, c: f64, lcl: f64, ucl: f64) -> Panel {
    let k = values.len();
    Panel {
        name: name.into(),
        values,
        center: vec![c; k],
        lcl: vec![lcl; k],
        ucl: vec![ucl; k],
        violations: vec![],
    }
}

/// Phase I: estimates the limits without the `excluded` subgroups and draws the chart.
pub fn phase1(
    kind: ChartKind,
    data: &ChartData,
    excluded: &[usize],
    rules: &RuleSet,
) -> Result<Chart, String> {
    let limits = estimate_limits(kind, data, excluded)?;
    let mut chart = apply(&limits, data, rules, excluded)?;
    chart.phase = Phase::I;
    if let Limits::Variables { sigma, kind, .. } = &limits {
        let how = match kind {
            ChartKind::IMr => "MR̄/d2",
            ChartKind::XbarR => "R̄/d2",
            _ => "S̄/c4",
        };
        chart
            .notes
            .insert(0, format!("σ estimado por {how} = {sigma:.6}"));
    }
    if !excluded.is_empty() {
        chart.notes.push(format!(
            "{} subgrupo(s) com causa atribuível fora da estimativa: {}",
            excluded.len(),
            excluded
                .iter()
                .map(|i| (i + 1).to_string())
                .collect::<Vec<_>>()
                .join(", ")
        ));
    }
    Ok(chart)
}

/// Phase I repeated: estimates, removes the points beyond the limits of either panel and
/// re-estimates until none is left (the usual "revised limits"). Returns the chart and
/// the removed subgroups, in the order they were removed. Use only when the removed
/// points have an assignable cause; the report lists them.
pub fn phase1_revised(
    kind: ChartKind,
    data: &ChartData,
    rules: &RuleSet,
    max_rounds: usize,
) -> Result<(Chart, Vec<usize>), String> {
    let mut excluded: Vec<usize> = Vec::new();
    for _ in 0..max_rounds.max(1) {
        let chart = phase1(kind, data, &excluded, &RuleSet::basic())?;
        // A large moving range belongs to two points: on I-MR only the I chart decides.
        let panels = if kind == ChartKind::IMr {
            &chart.panels[..1]
        } else {
            &chart.panels[..]
        };
        let mut new: Vec<usize> = panels
            .iter()
            .flat_map(|p| p.out_of_limits())
            .filter(|i| !excluded.contains(i))
            .collect();
        new.sort_unstable();
        new.dedup();
        if new.is_empty() {
            break;
        }
        excluded.extend(new);
    }
    let chart = phase1(kind, data, &excluded, rules)?;
    Ok((chart, excluded))
}

/// Constant limits from the distribution of the plotted statistic in the model
/// (`docs/plano_gemeo_digital_e_painel.md`, E4): `samples` are the values of the statistic
/// across replications for the same subgroup size. The limits are the 0.135% and 99.865%
/// quantiles (the probability outside ±3σ of a normal) and the centre is the median, so
/// they also work for skewed KPIs. At least 100 samples are required (each tail quantile
/// rests on very few values below that; more replications are better).
pub fn limits_from_model(samples: &[f64]) -> Result<Limits, String> {
    let mut v: Vec<f64> = samples.iter().copied().filter(|x| x.is_finite()).collect();
    if v.len() < 100 {
        return Err(format!(
            "limites pelo modelo exigem ao menos 100 replicações (há {})",
            v.len()
        ));
    }
    v.sort_by(|a, b| a.total_cmp(b));
    let q = |p: f64| crate::capability::quantile_sorted(&v, p);
    Ok(Limits::Fixed {
        center: q(0.5),
        lcl: q(0.00135),
        ucl: q(0.99865),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn vars(groups: &[&[f64]]) -> ChartData {
        ChartData::Variables(groups.iter().map(|g| g.to_vec()).collect())
    }

    #[test]
    fn xbar_r_limits_equal_the_textbook_formulas() {
        let data = vars(&[
            &[10.0, 12.0, 11.0, 9.0, 13.0],
            &[11.0, 10.0, 12.0, 12.0, 10.0],
            &[9.0, 11.0, 10.0, 12.0, 11.0],
            &[12.0, 13.0, 11.0, 10.0, 12.0],
        ]);
        let chart = phase1(ChartKind::XbarR, &data, &[], &RuleSet::basic()).unwrap();
        // x̿ = (11 + 11 + 10.6 + 11.6)/4 = 11.05; R̄ = (4 + 2 + 3 + 3)/4 = 3.
        let c = Constants::new(5);
        let x = &chart.panels[0];
        assert!((x.center[0] - 11.05).abs() < 1e-12);
        assert!((x.ucl[0] - (11.05 + c.A2 * 3.0)).abs() < 1e-12);
        assert!((x.lcl[0] - (11.05 - c.A2 * 3.0)).abs() < 1e-12);
        let r = &chart.panels[1];
        assert!((r.center[0] - 3.0).abs() < 1e-12);
        assert!((r.ucl[0] - c.D4 * 3.0).abs() < 1e-12);
        assert_eq!(r.lcl[0], 0.0);
        // Rounded factors of the table: UCL = 11.05 + 0.577·3 = 12.781.
        assert!((x.ucl[0] - 12.781).abs() < 2e-3);
    }

    #[test]
    fn xbar_s_limits_equal_the_textbook_formulas() {
        let data = vars(&[&[1.0, 2.0, 3.0], &[2.0, 4.0, 6.0], &[0.0, 1.0, 2.0]]);
        let chart = phase1(ChartKind::XbarS, &data, &[], &RuleSet::basic()).unwrap();
        // s = 1, 2, 1: S̄ = 4/3; x̿ = (2 + 4 + 1)/3 = 7/3.
        let c = Constants::new(3);
        let sbar = 4.0 / 3.0;
        assert!((chart.panels[0].ucl[0] - (7.0 / 3.0 + c.A3 * sbar)).abs() < 1e-12);
        assert!((chart.panels[1].ucl[0] - c.B4 * sbar).abs() < 1e-12);
        assert!((chart.panels[1].center[0] - sbar).abs() < 1e-12);
    }

    #[test]
    fn imr_uses_the_mean_moving_range() {
        let x = [5.0, 7.0, 6.0, 9.0, 8.0];
        let data = ChartData::Variables(x.iter().map(|&v| vec![v]).collect());
        let chart = phase1(ChartKind::IMr, &data, &[], &RuleSet::basic()).unwrap();
        // MR = 2, 1, 3, 1: MR̄ = 1.75; limits x̄ ± 2.66·MR̄ (2.66 = 3/d2(2)).
        let i = &chart.panels[0];
        assert!((i.center[0] - 7.0).abs() < 1e-12);
        assert!((i.ucl[0] - (7.0 + 3.0 / Constants::new(2).d2 * 1.75)).abs() < 1e-12);
        assert!((i.ucl[0] - (7.0 + 2.66 * 1.75)).abs() < 5e-3);
        let mr = &chart.panels[1];
        assert_eq!(mr.values[0], None);
        assert!((mr.center[0] - 1.75).abs() < 1e-12);
        assert!((mr.ucl[0] - 3.267 * 1.75).abs() < 2e-3);
    }

    #[test]
    fn excluded_points_leave_the_estimate() {
        let mut x = vec![10.0, 10.5, 9.5, 10.2, 9.8, 10.1, 9.9, 10.3];
        x.push(30.0); // assignable cause
        x.extend([10.0, 9.7, 10.4]);
        let data = ChartData::Variables(x.iter().map(|&v| vec![v]).collect());
        let all = phase1(ChartKind::IMr, &data, &[], &RuleSet::basic()).unwrap();
        let without = phase1(ChartKind::IMr, &data, &[8], &RuleSet::basic()).unwrap();
        assert!(without.panels[0].ucl[0] < all.panels[0].ucl[0]);
        // The excluded point is still drawn, but not flagged.
        assert_eq!(without.panels[0].values[8], Some(30.0));
        assert!(without.panels[0].violations.iter().all(|v| v.index != 8));
        // The revised phase I finds it by itself.
        let (rev, removed) = phase1_revised(ChartKind::IMr, &data, &RuleSet::basic(), 5).unwrap();
        assert_eq!(removed, vec![8]);
        assert_eq!(rev.limits, without.limits);
    }

    #[test]
    fn phase2_keeps_the_frozen_limits() {
        let limits = Limits::Variables {
            kind: ChartKind::IMr,
            n: 1,
            mean: 0.0,
            sigma: 1.0,
        };
        let data = ChartData::Variables(vec![vec![0.5], vec![3.5], vec![0.2]]);
        let chart = apply(&limits, &data, &RuleSet::basic(), &[]).unwrap();
        assert_eq!(chart.panels[0].ucl[0], 3.0);
        assert_eq!(chart.panels[0].violations.len(), 1);
        assert_eq!(chart.panels[0].violations[0].index, 1);
        // Moving ranges 3 and 3.3, below D2·σ = 3.686.
        assert!(chart.panels[1].violations.is_empty());
    }

    #[test]
    fn p_chart_with_variable_sample_size() {
        let data = ChartData::Attributes {
            counts: vec![5.0, 10.0, 4.0, 30.0],
            sizes: vec![100.0, 200.0, 100.0, 200.0],
        };
        let chart = phase1(ChartKind::P, &data, &[], &RuleSet::basic()).unwrap();
        let p = 49.0 / 600.0;
        let panel = &chart.panels[0];
        assert!((panel.center[0] - p).abs() < 1e-15);
        let ucl100 = p + 3.0 * (p * (1.0 - p) / 100.0).sqrt();
        let ucl200 = p + 3.0 * (p * (1.0 - p) / 200.0).sqrt();
        assert!((panel.ucl[0] - ucl100).abs() < 1e-15);
        assert!((panel.ucl[1] - ucl200).abs() < 1e-15);
        assert!(panel.ucl[1] < panel.ucl[0]);
        // 30/200 = 0.15 > UCL(200) = 0.1397.
        assert_eq!(panel.out_of_limits(), vec![3]);
        assert!(!chart.notes.is_empty());
    }

    #[test]
    fn np_c_and_u_charts() {
        let data = ChartData::Attributes {
            counts: vec![2.0, 4.0, 3.0, 5.0],
            sizes: vec![50.0; 4],
        };
        let np = phase1(ChartKind::Np, &data, &[], &RuleSet::basic()).unwrap();
        // p̄ = 14/200 = 0.07, np̄ = 3.5, σ = √(3.5·0.93) = 1.8042.
        let s = (3.5f64 * 0.93).sqrt();
        assert!((np.panels[0].center[0] - 3.5).abs() < 1e-12);
        assert!((np.panels[0].ucl[0] - (3.5 + 3.0 * s)).abs() < 1e-12);
        assert_eq!(np.panels[0].lcl[0], 0.0);

        let data = ChartData::Attributes {
            counts: vec![9.0, 16.0, 10.0, 13.0],
            sizes: vec![1.0; 4],
        };
        let c = phase1(ChartKind::C, &data, &[], &RuleSet::basic()).unwrap();
        // c̄ = 12, limits 12 ± 3√12 = [1.6077, 22.392].
        assert!((c.panels[0].lcl[0] - (12.0 - 3.0 * 12f64.sqrt())).abs() < 1e-12);
        assert!((c.panels[0].ucl[0] - 22.392).abs() < 1e-3);

        let data = ChartData::Attributes {
            counts: vec![10.0, 24.0, 12.0],
            sizes: vec![5.0, 10.0, 4.0],
        };
        let u = phase1(ChartKind::U, &data, &[], &RuleSet::basic()).unwrap();
        // ū = 46/19.
        let ub: f64 = 46.0 / 19.0;
        assert!((u.panels[0].ucl[2] - (ub + 3.0 * (ub / 4.0).sqrt())).abs() < 1e-12);
        assert!((u.panels[0].lcl[1] - (ub - 3.0 * (ub / 10.0).sqrt())).abs() < 1e-12);
    }

    #[test]
    fn wrong_inputs_are_explained() {
        let data = vars(&[&[1.0, 2.0], &[1.0, 2.0, 3.0]]);
        assert!(phase1(ChartKind::XbarR, &data, &[], &RuleSet::basic()).is_err());
        let att = ChartData::Attributes {
            counts: vec![1.0],
            sizes: vec![1.0],
        };
        assert!(phase1(ChartKind::XbarR, &att, &[], &RuleSet::basic()).is_err());
        let bad = ChartData::Attributes {
            counts: vec![5.0, 1.0],
            sizes: vec![4.0, 10.0],
        };
        assert!(phase1(ChartKind::P, &bad, &[], &RuleSet::basic()).is_err());
        let np_var = ChartData::Attributes {
            counts: vec![1.0, 1.0],
            sizes: vec![4.0, 10.0],
        };
        assert!(phase1(ChartKind::Np, &np_var, &[], &RuleSet::basic()).is_err());
    }

    #[test]
    fn model_limits_are_quantiles() {
        let s: Vec<f64> = (0..1001).map(|i| i as f64).collect();
        let Limits::Fixed { center, lcl, ucl } = limits_from_model(&s).unwrap() else {
            panic!()
        };
        assert_eq!(center, 500.0);
        assert!((lcl - 1.35).abs() < 1e-9 && (ucl - 998.65).abs() < 1e-9);
        assert!(limits_from_model(&s[..50]).is_err());
    }
}
