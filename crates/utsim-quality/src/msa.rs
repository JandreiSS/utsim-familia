//! Measurement system analysis (MSA) for variables: Gage R&R by ANOVA and by averages and
//! ranges, bias and linearity.
//!
//! **Gage R&R (crossed, balanced)**: `p` parts × `o` operators × `r` repetitions.
//! Two-factor random-effects ANOVA with interaction; when the interaction's p-value is
//! above `alpha_interaction` (0.25 by the usual convention) it is pooled into the error.
//! Variance components from the expected mean squares:
//! `σ²_rep = MS_E`, `σ²_PO = (MS_PO − MS_E)/r`, `σ²_O = (MS_O − MS_PO)/(p r)`,
//! `σ²_P = (MS_P − MS_PO)/(o r)` (with the pooled error in place of `MS_PO` when the
//! interaction is removed); negative estimates are truncated at 0 with a warning.
//! Repeatability (EV) is `σ_rep`, reproducibility (AV) is `√(σ²_O + σ²_PO)`,
//! `GRR = √(EV² + AV²)`, part variation `PV = σ_P`, total `TV = √(GRR² + PV²)`.
//! `%contribuição` uses variances, `%VE` (study variation) standard deviations, `%tolerância`
//! is `k·σ/(LSE − LIE)` with `k = 6` (or 5.15, declared), and the number of distinct
//! categories is `ndc = ⌊1.41·PV/GRR⌋`.
//!
//! **Averages and ranges**: `EV = R̿/d2*(r, p·o)`, `AV = √((X̄_diff/d2*(o, 1))² − EV²/(p r))`,
//! `PV = R_p/d2*(p, 1)`, with `d2*` computed ([`crate::constants::d2_star`]).
//!
//! The acceptance criteria (10%, 30%, ndc ≥ 5) are the usual rule of thumb; each company
//! may register its own.

use serde::{Deserialize, Serialize};

use crate::constants::d2_star;
use crate::prob::{f_sf, t_two_sided};

/// One measurement of a gauge study.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Measurement {
    pub part: String,
    pub operator: String,
    pub value: f64,
}

/// Balanced data `y[part][operator][rep]` with the labels.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GageData {
    pub parts: Vec<String>,
    pub operators: Vec<String>,
    pub y: Vec<Vec<Vec<f64>>>,
}

impl GageData {
    /// Groups the measurements by part and operator, in order of first appearance;
    /// repetitions are the order within each cell. Requires a balanced design with at
    /// least 2 parts, 2 operators and 2 repetitions.
    pub fn from_measurements(ms: &[Measurement]) -> Result<Self, String> {
        let mut parts: Vec<String> = Vec::new();
        let mut operators: Vec<String> = Vec::new();
        for m in ms {
            if !m.value.is_finite() {
                return Err(format!(
                    "valor inválido para a peça {} e o operador {}",
                    m.part, m.operator
                ));
            }
            if !parts.contains(&m.part) {
                parts.push(m.part.clone());
            }
            if !operators.contains(&m.operator) {
                operators.push(m.operator.clone());
            }
        }
        let mut y = vec![vec![Vec::new(); operators.len()]; parts.len()];
        for m in ms {
            let i = parts.iter().position(|p| *p == m.part).unwrap();
            let j = operators.iter().position(|o| *o == m.operator).unwrap();
            y[i][j].push(m.value);
        }
        let data = GageData {
            parts,
            operators,
            y,
        };
        data.check()?;
        Ok(data)
    }

    fn check(&self) -> Result<(), String> {
        let (p, o) = (self.parts.len(), self.operators.len());
        if p < 2 || o < 2 {
            return Err(format!(
                "o estudo exige ao menos 2 peças e 2 operadores (há {p} e {o})"
            ));
        }
        let r = self.y[0][0].len();
        for (i, row) in self.y.iter().enumerate() {
            for (j, cell) in row.iter().enumerate() {
                if cell.len() != r {
                    return Err(format!(
                        "estudo desbalanceado: peça {} com o operador {} tem {} repetições, \
                         a primeira célula tem {r}",
                        self.parts[i],
                        self.operators[j],
                        cell.len()
                    ));
                }
            }
        }
        if r < 2 {
            return Err("o estudo exige ao menos 2 repetições por peça e operador".into());
        }
        Ok(())
    }

    pub fn reps(&self) -> usize {
        self.y[0][0].len()
    }
}

/// One line of the ANOVA table.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AnovaRow {
    pub source: String,
    pub df: f64,
    pub ss: f64,
    pub ms: f64,
    pub f: Option<f64>,
    pub p: Option<f64>,
}

/// One component of variation with its percentages.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Component {
    pub name: String,
    pub variance: f64,
    pub sd: f64,
    /// `k·σ` (study variation).
    pub study_var: f64,
    pub pct_contribution: f64,
    pub pct_study_var: f64,
    pub pct_tolerance: Option<f64>,
}

/// Options of a gauge study.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct GageOptions {
    /// Multiplier of σ in the study variation (6, or 5.15 for 99%).
    pub k: f64,
    /// Tolerance `LSE − LIE`, for `%tolerância`.
    pub tolerance: Option<f64>,
    /// Interaction pooled into the error when its p-value is above this.
    pub alpha_interaction: f64,
}

impl Default for GageOptions {
    fn default() -> Self {
        GageOptions {
            k: 6.0,
            tolerance: None,
            alpha_interaction: 0.25,
        }
    }
}

/// Result of a Gage R&R study (either method).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GageRR {
    pub method: String,
    pub parts: usize,
    pub operators: usize,
    pub reps: usize,
    /// ANOVA table (empty for averages and ranges).
    pub anova: Vec<AnovaRow>,
    pub interaction_pooled: bool,
    /// Total GRR, repeatability, reproducibility, operator, interaction, part, total.
    pub components: Vec<Component>,
    pub ndc: usize,
    pub ndc_raw: f64,
    pub k: f64,
    pub warnings: Vec<String>,
}

impl GageRR {
    /// The component with this name (`"GRR total"`, `"Repetibilidade"`...).
    pub fn component(&self, name: &str) -> Option<&Component> {
        self.components.iter().find(|c| c.name == name)
    }

    /// The usual verdict on `%VE` of the GRR: ≤ 10% acceptable, ≤ 30% conditional.
    pub fn verdict(&self) -> &'static str {
        let v = self.component(GRR).map_or(f64::NAN, |c| c.pct_study_var);
        if v <= 10.0 {
            "aceitável (%VE do GRR ≤ 10%)"
        } else if v <= 30.0 {
            "condicional (%VE do GRR entre 10% e 30%): depende da aplicação e do custo"
        } else {
            "inaceitável (%VE do GRR > 30%): melhorar o sistema de medição"
        }
    }
}

pub const GRR: &str = "GRR total";
pub const REPEATABILITY: &str = "Repetibilidade (EV)";
pub const REPRODUCIBILITY: &str = "Reprodutibilidade (AV)";
pub const OPERATOR: &str = "Operador";
pub const INTERACTION: &str = "Peça × operador";
pub const PART: &str = "Peça a peça (PV)";
pub const TOTAL: &str = "Variação total (TV)";

fn components(
    rep: f64,
    oper: f64,
    inter: f64,
    part: f64,
    options: &GageOptions,
    with_split: bool,
) -> (Vec<Component>, f64) {
    let reprod = oper + inter;
    let grr = rep + reprod;
    let total = grr + part;
    let mk = |name: &str, var: f64| {
        let sd = var.sqrt();
        Component {
            name: name.into(),
            variance: var,
            sd,
            study_var: options.k * sd,
            pct_contribution: 100.0 * var / total,
            pct_study_var: 100.0 * sd / total.sqrt(),
            pct_tolerance: options.tolerance.map(|t| 100.0 * options.k * sd / t),
        }
    };
    let mut out = vec![
        mk(GRR, grr),
        mk(REPEATABILITY, rep),
        mk(REPRODUCIBILITY, reprod),
    ];
    if with_split {
        out.push(mk(OPERATOR, oper));
        out.push(mk(INTERACTION, inter));
    }
    out.push(mk(PART, part));
    out.push(mk(TOTAL, total));
    let ratio = if grr > 0.0 {
        (part / grr).sqrt()
    } else {
        f64::INFINITY
    };
    (out, ratio)
}

fn ndc_of(ratio: f64) -> (usize, f64) {
    let raw = 1.41 * ratio;
    let n = if raw.is_finite() {
        (raw.floor() as usize).max(1)
    } else {
        usize::MAX
    };
    (n, raw)
}

/// Gage R&R by two-factor ANOVA.
pub fn gage_anova(data: &GageData, options: &GageOptions) -> Result<GageRR, String> {
    data.check()?;
    let (p, o, r) = (data.parts.len(), data.operators.len(), data.reps());
    let (pf, of, rf) = (p as f64, o as f64, r as f64);
    let cell: Vec<Vec<f64>> = data
        .y
        .iter()
        .map(|row| row.iter().map(|c| c.iter().sum::<f64>() / rf).collect())
        .collect();
    let part_mean: Vec<f64> = cell
        .iter()
        .map(|row| row.iter().sum::<f64>() / of)
        .collect();
    let oper_mean: Vec<f64> = (0..o)
        .map(|j| cell.iter().map(|row| row[j]).sum::<f64>() / pf)
        .collect();
    let grand = part_mean.iter().sum::<f64>() / pf;
    let ss_p = of * rf * part_mean.iter().map(|m| (m - grand).powi(2)).sum::<f64>();
    let ss_o = pf * rf * oper_mean.iter().map(|m| (m - grand).powi(2)).sum::<f64>();
    let mut ss_po = 0.0;
    let mut ss_e = 0.0;
    for i in 0..p {
        for j in 0..o {
            ss_po += (cell[i][j] - part_mean[i] - oper_mean[j] + grand).powi(2);
            ss_e += data.y[i][j]
                .iter()
                .map(|v| (v - cell[i][j]).powi(2))
                .sum::<f64>();
        }
    }
    ss_po *= rf;
    let (df_p, df_o) = (pf - 1.0, of - 1.0);
    let df_po = df_p * df_o;
    let df_e = pf * of * (rf - 1.0);
    let (ms_p, ms_o, ms_po, ms_e) = (ss_p / df_p, ss_o / df_o, ss_po / df_po, ss_e / df_e);
    let mut warnings = Vec::new();
    let f_po = ms_po / ms_e;
    let p_po = if ms_e > 0.0 {
        f_sf(f_po, df_po, df_e)
    } else {
        0.0
    };
    let pooled = p_po > options.alpha_interaction;
    let row = |s: &str, df: f64, ss: f64, f: Option<f64>, pv: Option<f64>| AnovaRow {
        source: s.into(),
        df,
        ss,
        ms: ss / df,
        f,
        p: pv,
    };
    let total_ss = ss_p + ss_o + ss_po + ss_e;
    let total_df = pf * of * rf - 1.0;
    let (rep, oper, inter, part, anova);
    if pooled {
        let ss_err = ss_po + ss_e;
        let df_err = df_po + df_e;
        let ms_err = ss_err / df_err;
        let fp = ms_p / ms_err;
        let fo = ms_o / ms_err;
        // The full table first (the interaction test), then the reduced model.
        anova = vec![
            row("Peça", df_p, ss_p, Some(fp), Some(f_sf(fp, df_p, df_err))),
            row(
                "Operador",
                df_o,
                ss_o,
                Some(fo),
                Some(f_sf(fo, df_o, df_err)),
            ),
            row("Repetibilidade", df_err, ss_err, None, None),
            row("Total", total_df, total_ss, None, None),
        ];
        warnings.push(format!(
            "interação peça × operador sem efeito (p = {p_po:.4} > {}): incorporada à \
             repetibilidade (modelo reduzido)",
            options.alpha_interaction
        ));
        rep = ms_err;
        oper = (ms_o - ms_err) / (pf * rf);
        inter = 0.0;
        part = (ms_p - ms_err) / (of * rf);
    } else {
        let fp = ms_p / ms_po;
        let fo = ms_o / ms_po;
        anova = vec![
            row("Peça", df_p, ss_p, Some(fp), Some(f_sf(fp, df_p, df_po))),
            row(
                "Operador",
                df_o,
                ss_o,
                Some(fo),
                Some(f_sf(fo, df_o, df_po)),
            ),
            row("Peça × operador", df_po, ss_po, Some(f_po), Some(p_po)),
            row("Repetibilidade", df_e, ss_e, None, None),
            row("Total", total_df, total_ss, None, None),
        ];
        rep = ms_e;
        inter = (ms_po - ms_e) / rf;
        oper = (ms_o - ms_po) / (pf * rf);
        part = (ms_p - ms_po) / (of * rf);
    }
    let mut trunc = |name: &str, v: f64| {
        if v < 0.0 {
            warnings.push(format!(
                "componente de variância de {name} estimado negativo ({v:.3e}): truncado em 0"
            ));
            0.0
        } else {
            v
        }
    };
    let (oper, inter, part) = (
        trunc("operador", oper),
        trunc("interação", inter),
        trunc("peça", part),
    );
    let (components, ratio) = components(rep, oper, inter, part, options, true);
    let (ndc, ndc_raw) = ndc_of(ratio);
    Ok(GageRR {
        method: "ANOVA".into(),
        parts: p,
        operators: o,
        reps: r,
        anova,
        interaction_pooled: pooled,
        components,
        ndc,
        ndc_raw,
        k: options.k,
        warnings,
    })
}

/// Gage R&R by averages and ranges.
#[allow(clippy::needless_range_loop)] // `i`, `j` index the data and both mean vectors
pub fn gage_average_range(data: &GageData, options: &GageOptions) -> Result<GageRR, String> {
    data.check()?;
    let (p, o, r) = (data.parts.len(), data.operators.len(), data.reps());
    let range = |v: &[f64]| {
        let (lo, hi) = v.iter().fold((f64::INFINITY, f64::NEG_INFINITY), |a, &x| {
            (a.0.min(x), a.1.max(x))
        });
        hi - lo
    };
    let mut rbar_sum = 0.0;
    let mut oper_mean = vec![0.0; o];
    let mut part_mean = vec![0.0; p];
    for i in 0..p {
        for j in 0..o {
            let c = &data.y[i][j];
            rbar_sum += range(c);
            let m = c.iter().sum::<f64>() / r as f64;
            oper_mean[j] += m / p as f64;
            part_mean[i] += m / o as f64;
        }
    }
    let rbar = rbar_sum / (p * o) as f64;
    let ev = rbar / d2_star(r, p * o);
    let xdiff = range(&oper_mean);
    let av2 = (xdiff / d2_star(o, 1)).powi(2) - ev * ev / (p * r) as f64;
    let mut warnings = Vec::new();
    let av2 = if av2 < 0.0 {
        warnings.push("reprodutibilidade estimada negativa: truncada em 0".into());
        0.0
    } else {
        av2
    };
    let rp = range(&part_mean);
    let pv = rp / d2_star(p, 1);
    let (components, ratio) = components(ev * ev, av2, 0.0, pv * pv, options, false);
    let (ndc, ndc_raw) = ndc_of(ratio);
    warnings.push(format!(
        "constantes: d2*({r}, {}) = {:.4}, d2*({o}, 1) = {:.4}, d2*({p}, 1) = {:.4}",
        p * o,
        d2_star(r, p * o),
        d2_star(o, 1),
        d2_star(p, 1)
    ));
    Ok(GageRR {
        method: "Médias e amplitudes".into(),
        parts: p,
        operators: o,
        reps: r,
        anova: vec![],
        interaction_pooled: false,
        components,
        ndc,
        ndc_raw,
        k: options.k,
        warnings,
    })
}

/// Bias study: repeated measurements of one reference.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Bias {
    pub reference: f64,
    pub n: usize,
    pub mean: f64,
    pub bias: f64,
    pub sd: f64,
    pub t: f64,
    pub df: f64,
    pub p_value: f64,
    pub ci: (f64, f64),
}

/// Bias of `values` against `reference`, with the t test of `bias = 0` and its interval.
pub fn bias(reference: f64, values: &[f64], level: f64) -> Result<Bias, String> {
    let n = values.len();
    if n < 2 {
        return Err("o estudo de viés exige ao menos 2 medições".into());
    }
    let nf = n as f64;
    let mean = values.iter().sum::<f64>() / nf;
    let sd = (values.iter().map(|x| (x - mean).powi(2)).sum::<f64>() / (nf - 1.0)).sqrt();
    let b = mean - reference;
    let se = sd / nf.sqrt();
    let df = nf - 1.0;
    let t = if se > 0.0 {
        b / se
    } else {
        f64::INFINITY * b.signum()
    };
    let tq = utsim_stats::t_quantile(level, n - 1);
    Ok(Bias {
        reference,
        n,
        mean,
        bias: b,
        sd,
        t,
        df,
        p_value: if se > 0.0 {
            t_two_sided(t, df)
        } else if b == 0.0 {
            1.0
        } else {
            0.0
        },
        ci: (b - tq * se, b + tq * se),
    })
}

/// Linearity study: bias regressed on the reference value.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Linearity {
    pub n: usize,
    pub slope: f64,
    pub intercept: f64,
    pub se_slope: f64,
    pub se_intercept: f64,
    pub p_slope: f64,
    pub p_intercept: f64,
    pub r2: f64,
    pub s: f64,
    /// Bias per reference value, in increasing order of the reference.
    pub per_reference: Vec<Bias>,
    /// `|slope|·process variation` and `100·|slope|`, when a process variation is given.
    pub linearity: Option<f64>,
    pub pct_linearity: f64,
}

/// Linearity from `(reference, measured)` pairs over several reference parts.
pub fn linearity(
    pairs: &[(f64, f64)],
    process_variation: Option<f64>,
    level: f64,
) -> Result<Linearity, String> {
    let n = pairs.len();
    if n < 3 {
        return Err("o estudo de linearidade exige ao menos 3 medições".into());
    }
    let nf = n as f64;
    let x: Vec<f64> = pairs.iter().map(|p| p.0).collect();
    let y: Vec<f64> = pairs.iter().map(|p| p.1 - p.0).collect();
    let mx = x.iter().sum::<f64>() / nf;
    let my = y.iter().sum::<f64>() / nf;
    let sxx: f64 = x.iter().map(|v| (v - mx).powi(2)).sum();
    if !(sxx > 0.0) {
        return Err("linearidade exige ao menos duas referências diferentes".into());
    }
    let sxy: f64 = x.iter().zip(&y).map(|(a, b)| (a - mx) * (b - my)).sum();
    let syy: f64 = y.iter().map(|v| (v - my).powi(2)).sum();
    let slope = sxy / sxx;
    let intercept = my - slope * mx;
    let sse: f64 = x
        .iter()
        .zip(&y)
        .map(|(a, b)| (b - intercept - slope * a).powi(2))
        .sum();
    let df = nf - 2.0;
    let s = (sse / df).sqrt();
    let se_slope = s / sxx.sqrt();
    let se_intercept = s * (1.0 / nf + mx * mx / sxx).sqrt();
    let p = |est: f64, se: f64| {
        if se > 0.0 {
            t_two_sided(est / se, df)
        } else if est == 0.0 {
            1.0
        } else {
            0.0
        }
    };
    let mut refs: Vec<f64> = x.clone();
    refs.sort_by(|a, b| a.total_cmp(b));
    refs.dedup();
    let mut per_reference = Vec::new();
    for r in refs {
        let vals: Vec<f64> = pairs.iter().filter(|p| p.0 == r).map(|p| p.1).collect();
        if vals.len() >= 2 {
            per_reference.push(bias(r, &vals, level)?);
        }
    }
    Ok(Linearity {
        n,
        slope,
        intercept,
        se_slope,
        se_intercept,
        p_slope: p(slope, se_slope),
        p_intercept: p(intercept, se_intercept),
        r2: if syy > 0.0 {
            sxy * sxy / (sxx * syy)
        } else {
            0.0
        },
        s,
        per_reference,
        linearity: process_variation.map(|pv| slope.abs() * pv),
        pct_linearity: 100.0 * slope.abs(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use rand::SeedableRng;
    use rand_distr::{Distribution, Normal};
    use rand_pcg::Pcg64;

    fn data(y: Vec<Vec<Vec<f64>>>) -> GageData {
        GageData {
            parts: (0..y.len()).map(|i| format!("P{}", i + 1)).collect(),
            operators: (0..y[0].len()).map(|j| format!("O{}", j + 1)).collect(),
            y,
        }
    }

    #[test]
    fn anova_sums_of_squares_by_hand() {
        // 2 parts × 2 operators × 2 repetitions:
        //          O1        O2
        //   P1   1, 3      2, 4
        //   P2   5, 7      8, 8
        // cells 2, 3, 6, 8; part means 2.5, 7; operator means 4, 5.5; grand 4.75.
        // SS_P = 4·(2.25² + 2.25²) = 40.5; SS_O = 4·(0.75² + 0.75²) = 4.5;
        // SS_PO = 2·Σ(cell − part − oper + grand)² = 2·4·0.25² = 0.5;
        // SS_E = 2 + 2 + 2 + 0 = 6; total = 51.5 = Σ(y − 4.75)².
        let d = data(vec![
            vec![vec![1.0, 3.0], vec![2.0, 4.0]],
            vec![vec![5.0, 7.0], vec![8.0, 8.0]],
        ]);
        let g = gage_anova(
            &d,
            &GageOptions {
                alpha_interaction: 1.0, // keep the interaction
                ..GageOptions::default()
            },
        )
        .unwrap();
        let ss: Vec<f64> = g.anova.iter().map(|r| r.ss).collect();
        let want = [40.5, 4.5, 0.5, 6.0, 51.5];
        for (a, b) in ss.iter().zip(want) {
            assert!((a - b).abs() < 1e-12, "{ss:?}");
        }
        let total: f64 = [1.0, 3.0, 2.0, 4.0, 5.0, 7.0, 8.0, 8.0f64]
            .iter()
            .map(|v| (v - 4.75).powi(2))
            .sum();
        assert!((total - 51.5).abs() < 1e-12);
        // MS_E = 6/4 = 1.5, MS_PO = 0.5: interaction component negative → 0.
        assert_eq!(g.component(INTERACTION).unwrap().variance, 0.0);
        assert!(g.warnings.iter().any(|w| w.contains("negativo")));
        // σ²_O = (4.5 − 0.5)/4 = 1; σ²_P = (40.5 − 0.5)/4 = 10.
        assert!((g.component(OPERATOR).unwrap().variance - 1.0).abs() < 1e-12);
        assert!((g.component(PART).unwrap().variance - 10.0).abs() < 1e-12);
        assert!((g.component(REPEATABILITY).unwrap().variance - 1.5).abs() < 1e-12);
        // The percentages of contribution of GRR and parts add to 100.
        let sum = g.component(GRR).unwrap().pct_contribution
            + g.component(PART).unwrap().pct_contribution;
        assert!((sum - 100.0).abs() < 1e-9);
    }

    #[test]
    fn pooling_the_interaction() {
        let d = data(vec![
            vec![vec![1.0, 3.0], vec![2.0, 4.0]],
            vec![vec![5.0, 7.0], vec![8.0, 8.0]],
        ]);
        let g = gage_anova(&d, &GageOptions::default()).unwrap();
        // F_PO = 0.5/1.5 = 0.333, p > 0.25: pooled, MS_err = 6.5/5 = 1.3.
        assert!(g.interaction_pooled);
        assert!((g.component(REPEATABILITY).unwrap().variance - 1.3).abs() < 1e-12);
        assert!((g.component(OPERATOR).unwrap().variance - (4.5 - 1.3) / 4.0).abs() < 1e-12);
        assert_eq!(g.anova.len(), 4);
    }

    /// Simulated study with known components: σ_P = 2, σ_O = `so`, σ_PO = 0, σ_rep = 0.5.
    fn simulated(so: f64, seed: u64) -> GageData {
        let mut rng = Pcg64::seed_from_u64(seed);
        let part = Normal::new(0.0, 2.0).unwrap();
        let oper = Normal::new(0.0, so.max(1e-12)).unwrap();
        let rep = Normal::new(0.0, 0.5).unwrap();
        let pe: Vec<f64> = (0..10).map(|_| part.sample(&mut rng)).collect();
        let oe: Vec<f64> = (0..3)
            .map(|_| if so > 0.0 { oper.sample(&mut rng) } else { 0.0 })
            .collect();
        data(
            (0..10)
                .map(|i| {
                    (0..3)
                        .map(|j| {
                            (0..3)
                                .map(|_| 50.0 + pe[i] + oe[j] + rep.sample(&mut rng))
                                .collect()
                        })
                        .collect()
                })
                .collect(),
        )
    }

    #[test]
    fn no_operator_effect_gives_small_reproducibility() {
        let mut av = 0.0;
        let mut ev = 0.0;
        for seed in 0..40 {
            let g = gage_anova(&simulated(0.0, seed), &GageOptions::default()).unwrap();
            av += g.component(REPRODUCIBILITY).unwrap().variance;
            ev += g.component(REPEATABILITY).unwrap().variance;
        }
        // Averaged over seeds: reproducibility close to 0 against repeatability 0.25.
        assert!(av / 40.0 < 0.03, "{}", av / 40.0);
        assert!((ev / 40.0 - 0.25).abs() < 0.03, "{}", ev / 40.0);
    }

    #[test]
    fn anova_and_averages_ranges_agree() {
        let mut diff_ev = 0.0;
        let mut diff_pv = 0.0;
        for seed in 0..30 {
            let d = simulated(0.3, 100 + seed);
            let a = gage_anova(&d, &GageOptions::default()).unwrap();
            let b = gage_average_range(&d, &GageOptions::default()).unwrap();
            diff_ev += (a.component(REPEATABILITY).unwrap().sd
                - b.component(REPEATABILITY).unwrap().sd)
                .abs();
            diff_pv += (a.component(PART).unwrap().sd - b.component(PART).unwrap().sd).abs()
                / a.component(PART).unwrap().sd;
        }
        // Repeatability: same target 0.5; mean absolute difference small.
        assert!(diff_ev / 30.0 < 0.06, "{}", diff_ev / 30.0);
        // Part variation from a single range of 10 means is noisier: within 25% on average.
        assert!(diff_pv / 30.0 < 0.25, "{}", diff_pv / 30.0);
    }

    #[test]
    fn ndc_and_tolerance() {
        let d = simulated(0.2, 7);
        let g = gage_anova(
            &d,
            &GageOptions {
                tolerance: Some(30.0),
                ..GageOptions::default()
            },
        )
        .unwrap();
        let grr = g.component(GRR).unwrap();
        let pv = g.component(PART).unwrap();
        assert_eq!(g.ndc, (1.41 * pv.sd / grr.sd).floor() as usize);
        assert!((grr.pct_tolerance.unwrap() - 100.0 * 6.0 * grr.sd / 30.0).abs() < 1e-9);
        // %VE of GRR and PV combine in quadrature to 100%.
        let q = (grr.pct_study_var.powi(2) + pv.pct_study_var.powi(2)).sqrt();
        assert!((q - 100.0).abs() < 1e-9);
    }

    #[test]
    fn unbalanced_studies_are_refused() {
        let ms = vec![
            Measurement {
                part: "a".into(),
                operator: "x".into(),
                value: 1.0,
            },
            Measurement {
                part: "a".into(),
                operator: "x".into(),
                value: 1.1,
            },
            Measurement {
                part: "b".into(),
                operator: "x".into(),
                value: 2.0,
            },
            Measurement {
                part: "a".into(),
                operator: "y".into(),
                value: 1.0,
            },
        ];
        assert!(GageData::from_measurements(&ms).is_err());
    }

    #[test]
    fn bias_and_linearity_by_hand() {
        // Bias: 10.1, 10.3, 9.9, 10.3 against 10: mean 10.15, bias 0.15, s = 0.19149,
        // t = 0.15/(0.19149/2) = 1.5667 with 3 d.o.f.
        let b = bias(10.0, &[10.1, 10.3, 9.9, 10.3], 0.95).unwrap();
        assert!((b.bias - 0.15).abs() < 1e-12);
        assert!((b.t - 1.5667).abs() < 1e-4);
        assert!(b.p_value > 0.05 && b.ci.0 < 0.0 && b.ci.1 > 0.0);
        // Linearity: bias = 0.1 + 0.02·ref exactly.
        let pairs: Vec<(f64, f64)> = [2.0, 4.0, 6.0, 8.0, 10.0]
            .iter()
            .flat_map(|&r| [(r, r + 0.1 + 0.02 * r), (r, r + 0.1 + 0.02 * r)])
            .collect();
        let l = linearity(&pairs, Some(6.0), 0.95).unwrap();
        assert!((l.slope - 0.02).abs() < 1e-12);
        assert!((l.intercept - 0.1).abs() < 1e-12);
        assert!((l.r2 - 1.0).abs() < 1e-12);
        assert!((l.pct_linearity - 2.0).abs() < 1e-9);
        assert!((l.linearity.unwrap() - 0.12).abs() < 1e-12);
        assert_eq!(l.per_reference.len(), 5);
    }
}
