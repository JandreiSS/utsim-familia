//! Design of experiments of a project (P3, `docs/plano_lean_seis_sigma.md`, 2.4 and 3.4):
//! the belt describes the factors with their two levels, picks the full factorial or a
//! fraction, the replicates, the center points and the seed, and gets the design with the
//! defining relation, the resolution and the aliases, and the collection worksheet (a CSV
//! in run order, with the coded and the actual levels and an empty response column). The
//! filled worksheet is a data set like any other, and the analysis is the recipe
//! [`crate::analises::Recipe::Factorial`].
//!
//! For screening many factors, the plan can be a Plackett-Burman design (the matrices of
//! `utsim_doe`, 8, 12 or 16 runs for up to 15 factors): the main effects are free of each
//! other, and every two-factor interaction is partially confounded with them, so it serves
//! to find the few factors that matter before a factorial on them.
//!
//! When every factor names a model parameter (`objeto.campo`), the UTSim model runs the
//! experiment: one scenario per point of the design with common random numbers, the
//! replications as the replicates, and the results saved as a data set of origin
//! **Simulado** (the error of the analysis is the simulated variability).

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use utsim_model::experiment::{Scenario, ScenarioResult};
use utsim_stats::doe::{self, Design, Generator};

/// One factor of the experiment.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct FactorSpec {
    #[serde(rename = "nome")]
    pub name: String,
    /// Level `−1`, as written (a number, or a value of the model's `.ron`).
    #[serde(rename = "baixo")]
    pub low: String,
    /// Level `+1`.
    #[serde(rename = "alto")]
    pub high: String,
    /// Level at the center; empty for the midpoint of numeric levels.
    #[serde(rename = "centro", default)]
    pub center: String,
    /// The parameter of the model, `objeto.campo`, when the simulator runs the experiment.
    #[serde(rename = "parametro", default)]
    pub key: String,
}

/// The experiment as the belt describes it.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct ExperimentSpec {
    #[serde(rename = "nome")]
    pub name: String,
    #[serde(rename = "fatores")]
    pub factors: Vec<FactorSpec>,
    /// `p` of `2^(k−p)`: 0 for the full factorial, 1 for the half, 2 for the quarter...
    #[serde(rename = "fracao")]
    pub fraction: usize,
    /// A Plackett-Burman screening design instead of a fraction (`fraction` is ignored).
    #[serde(rename = "triagem", default)]
    pub screening: bool,
    #[serde(rename = "replicas")]
    pub replicates: usize,
    #[serde(rename = "pontos_centrais")]
    pub center_points: usize,
    #[serde(rename = "semente")]
    pub seed: u64,
    /// Name of the response column of the worksheet.
    #[serde(rename = "resposta")]
    pub response: String,
}

#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum ExpError {
    #[error("o experimento precisa de 2 a 8 fatores (a triagem de Plackett-Burman vai até 15)")]
    Factors,
    #[error("dê um nome a cada fator")]
    NoName,
    #[error("o fator {0} precisa de dois níveis diferentes")]
    Levels(String),
    #[error("a fração 2^({0}−{1}) não está na tabela do app")]
    Fraction(usize, usize),
    #[error("o fator {0} precisa do nível central (os níveis não são números)")]
    Center(String),
    #[error("o fator {0} não diz qual parâmetro do modelo ele muda (objeto.campo)")]
    NoKey(String),
    #[error("{0}")]
    Design(String),
}

/// Most factors of a factorial or a fraction (the table of fractions goes to 8).
pub const MAX_FACTORS: usize = 8;

/// Most factors of a Plackett-Burman screening (16 runs).
pub const MAX_SCREENING: usize = 15;

fn number(s: &str) -> Option<f64> {
    let t = s.trim();
    let t = if t.contains(',') {
        t.replace('.', "").replace(',', ".")
    } else {
        t.to_string()
    };
    t.parse::<f64>().ok().filter(|v| v.is_finite())
}

fn show(x: f64) -> String {
    let s = format!("{x:.6}");
    let s = s.trim_end_matches('0').trim_end_matches('.');
    s.replace('.', ",")
}

/// A cell of the worksheet (`;` separates the columns).
fn cell(s: &str) -> String {
    s.trim().replace([';', '\n', '\r'], ",")
}

impl FactorSpec {
    /// The level written for a coded value `−1`, `0` or `+1`.
    pub fn level(&self, coded: i8) -> String {
        match coded {
            -1 => self.low.trim().to_string(),
            1 => self.high.trim().to_string(),
            _ => {
                if !self.center.trim().is_empty() {
                    self.center.trim().to_string()
                } else {
                    match (number(&self.low), number(&self.high)) {
                        (Some(a), Some(b)) => show(0.5 * (a + b)),
                        _ => String::new(),
                    }
                }
            }
        }
    }
}

impl ExperimentSpec {
    pub fn k(&self) -> usize {
        self.factors.len()
    }

    /// The fractions offered for this number of factors: `p` from 0 up.
    pub fn fractions(k: usize) -> Vec<usize> {
        std::iter::once(0)
            .chain((1..k).filter(|p| doe::standard_generators(k, *p).is_some()))
            .collect()
    }

    pub fn generators(&self) -> Result<Vec<Generator>, ExpError> {
        if self.fraction == 0 {
            return Ok(Vec::new());
        }
        doe::standard_generators(self.k(), self.fraction)
            .ok_or(ExpError::Fraction(self.k(), self.fraction))
    }

    /// The number of runs of the Plackett-Burman design for `k` factors (one replicate,
    /// no center points).
    pub fn screening_runs(k: usize) -> Option<usize> {
        utsim_doe::pb_runs(k).filter(|_| (2..=MAX_SCREENING).contains(&k))
    }

    /// The design, after checking the factors.
    pub fn design(&self) -> Result<Design, ExpError> {
        let k = self.k();
        let most = if self.screening {
            MAX_SCREENING
        } else {
            MAX_FACTORS
        };
        if !(2..=most).contains(&k) {
            return Err(ExpError::Factors);
        }
        for f in &self.factors {
            if f.name.trim().is_empty() {
                return Err(ExpError::NoName);
            }
            if f.low.trim().is_empty() || f.high.trim() == f.low.trim() || f.high.trim().is_empty()
            {
                return Err(ExpError::Levels(f.name.clone()));
            }
            if self.center_points > 0 && f.level(0).is_empty() {
                return Err(ExpError::Center(f.name.clone()));
            }
        }
        if self.screening {
            let n = Self::screening_runs(k).ok_or(ExpError::Factors)?;
            let m = utsim_doe::pb_matrix(n)
                .ok_or_else(|| ExpError::Design(format!("sem Plackett-Burman de {n} corridas")))?;
            let rows: Vec<Vec<i8>> = m.into_iter().map(|r| r[..k].to_vec()).collect();
            return Ok(doe::from_rows(
                k,
                &[],
                &rows,
                self.replicates.max(1),
                self.center_points,
                self.seed,
            ));
        }
        doe::design(
            k,
            &self.generators()?,
            self.replicates.max(1),
            self.center_points,
            self.seed,
        )
        .map_err(ExpError::Design)
    }

    /// Whether every factor names a model parameter (the simulator can run it).
    pub fn on_model(&self) -> Result<(), ExpError> {
        match self.factors.iter().find(|f| !f.key.contains('.')) {
            Some(f) => Err(ExpError::NoKey(f.name.clone())),
            None => Ok(()),
        }
    }

    fn response_name(&self) -> String {
        if self.response.trim().is_empty() {
            "resposta".to_string()
        } else {
            cell(&self.response)
        }
    }
}

/// The header of the factor columns: the coded `A`, `B`... and the actual levels.
fn factor_header(spec: &ExperimentSpec) -> Vec<String> {
    let mut h: Vec<String> = (0..spec.k()).map(|i| doe::letter(i).to_string()).collect();
    h.extend(spec.factors.iter().map(|f| cell(&f.name)));
    h
}

fn factor_cells(spec: &ExperimentSpec, levels: &[i8]) -> Vec<String> {
    let mut c: Vec<String> = levels.iter().map(|l| l.to_string()).collect();
    c.extend(
        spec.factors
            .iter()
            .zip(levels)
            .map(|(f, l)| cell(&f.level(*l))),
    );
    c
}

/// The collection worksheet in run order: `ordem;ordem_padrao;centro;A;B;...;<fatores>;
/// <resposta>` with the response empty, to be filled and imported as a data set.
pub fn worksheet(spec: &ExperimentSpec, d: &Design) -> String {
    let mut h = vec![
        "ordem".to_string(),
        "ordem_padrao".to_string(),
        "centro".to_string(),
    ];
    h.extend(factor_header(spec));
    h.push(spec.response_name());
    let mut out = h.join(";");
    out.push('\n');
    for r in &d.runs {
        let mut c = vec![
            r.run_order.to_string(),
            r.std_order.to_string(),
            if r.center { "sim" } else { "não" }.to_string(),
        ];
        c.extend(factor_cells(spec, &r.levels));
        c.push(String::new());
        out.push_str(&c.join(";"));
        out.push('\n');
    }
    out
}

/// The defining relation, the resolution and the aliases of the main effects and the
/// two-factor interactions, in words (`I = ABCD`, "resolução IV", `A = BCD`...); for a
/// Plackett-Burman screening, what it estimates and what it confounds.
pub fn alias_lines(spec: &ExperimentSpec, d: &Design) -> Vec<String> {
    if spec.screening {
        let n = d.runs.iter().filter(|r| !r.center).count() / d.replicates.max(1);
        let mut v = vec![
            format!("Plackett-Burman de {n} corridas para {} fatores (triagem)", d.k),
            "efeitos principais livres entre si (resolução III)".to_string(),
            "cada interação de dois fatores fica parcialmente confundida com os efeitos principais: use a triagem para achar os poucos fatores que importam e estude-os depois num fatorial".to_string(),
        ];
        if n > d.k + 1 {
            v.push(format!(
                "{} coluna(s) da matriz sem fator dão graus de liberdade ao erro",
                n - 1 - d.k
            ));
        }
        return v;
    }
    let words = d.words();
    if words.is_empty() {
        return vec!["fatorial completo: nenhum efeito confundido".to_string()];
    }
    let roman = ["", "I", "II", "III", "IV", "V", "VI", "VII", "VIII"];
    let mut v = vec![format!(
        "I = {}",
        words
            .iter()
            .map(|w| doe::term_name(*w))
            .collect::<Vec<_>>()
            .join(" = ")
    )];
    if let Some(r) = d.resolution() {
        v.push(format!(
            "resolução {}",
            roman.get(r).copied().unwrap_or("alta")
        ));
    }
    let k = d.k;
    let mut seen: Vec<u32> = Vec::new();
    let mut terms: Vec<u32> = (1u32..(1 << k)).filter(|m| doe::order(*m) <= 2).collect();
    terms.sort_by_key(|m| (doe::order(*m), *m));
    for m in terms {
        if seen.contains(&m) {
            continue;
        }
        let al: Vec<u32> = doe::aliases(m, &words)
            .into_iter()
            .filter(|a| doe::order(*a) <= 3)
            .collect();
        seen.extend(al.iter().copied());
        if al.is_empty() {
            continue;
        }
        v.push(format!(
            "{} = {}",
            doe::term_name(m),
            al.iter()
                .map(|a| doe::term_name(*a))
                .collect::<Vec<_>>()
                .join(" = ")
        ));
    }
    v
}

/// The distinct points of the design (factorial points in standard order, then the
/// center once), as model scenarios named `P1`, `P2`...; the first scenario has the
/// levels of the first point.
pub fn scenarios(spec: &ExperimentSpec, d: &Design) -> Result<Vec<(Vec<i8>, Scenario)>, ExpError> {
    spec.on_model()?;
    let mut points: Vec<(usize, Vec<i8>)> = Vec::new();
    for r in &d.runs {
        if !points.iter().any(|(_, l)| *l == r.levels) {
            points.push((r.std_order, r.levels.clone()));
        }
    }
    points.sort_by_key(|(s, _)| *s);
    Ok(points
        .into_iter()
        .enumerate()
        .map(|(i, (_, levels))| {
            let set: BTreeMap<String, String> = spec
                .factors
                .iter()
                .zip(&levels)
                .map(|(f, l)| (f.key.trim().to_string(), f.level(*l)))
                .collect();
            (
                levels,
                Scenario {
                    name: format!("P{}", i + 1),
                    set,
                },
            )
        })
        .collect())
}

/// The results of the simulated experiment as a data set: one row per point and
/// replication, with the coded and the actual levels and one column per KPI.
pub fn simulated_csv(
    spec: &ExperimentSpec,
    points: &[(Vec<i8>, Scenario)],
    results: &[ScenarioResult],
    measures: &[String],
) -> String {
    let mut h = vec!["ponto".to_string(), "replicacao".to_string()];
    h.extend(factor_header(spec));
    h.extend(measures.iter().map(|m| cell(m)));
    let mut out = h.join(";");
    out.push('\n');
    for ((levels, sc), res) in points.iter().zip(results) {
        let reps = measures
            .iter()
            .filter_map(|m| res.per_replication.get(m).map(Vec::len))
            .max()
            .unwrap_or(0);
        for r in 0..reps {
            let mut c = vec![sc.name.clone(), (r + 1).to_string()];
            c.extend(factor_cells(spec, levels));
            for m in measures {
                c.push(
                    res.per_replication
                        .get(m)
                        .and_then(|v| v.get(r))
                        .map(|v| show(*v))
                        .unwrap_or_default(),
                );
            }
            out.push_str(&c.join(";"));
            out.push('\n');
        }
    }
    out
}

/// The column indices of the coded factors `A`, `B`... in a worksheet or simulated data
/// set, and of the first column after the factors (the response).
pub fn columns(header: &[String]) -> (Vec<usize>, Option<usize>) {
    let mut f = Vec::new();
    for i in 0..MAX_SCREENING {
        let l = doe::letter(i).to_string();
        match header.iter().position(|h| *h == l) {
            Some(c) => f.push(c),
            None => break,
        }
    }
    // The response: after the coded factors and their actual levels (a worksheet of the
    // app), or right after the coded factors when there are no level columns.
    let after = f.last().and_then(|c| {
        let with_levels = c + 1 + f.len();
        if with_levels < header.len() {
            Some(with_levels)
        } else {
            Some(c + 1).filter(|x| *x < header.len())
        }
    });
    (f, after)
}

#[cfg(test)]
mod tests;
