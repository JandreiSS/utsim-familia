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
//! To find the best setting of the few factors left, the plan can be a response surface
//! (`utsim_doe`): the central composite (rotatable or face centered) or the Box-Behnken,
//! analysed by the recipe [`crate::analises::Recipe::Surface`] (the second-order model and
//! its stationary point).
//!
//! When a factor has more than two levels (three suppliers, four machines), the plan is a
//! general factorial: every combination of the levels each factor lists, with replicates,
//! analysed by the recipe [`crate::analises::Recipe::GeneralFactorial`] (the analysis of
//! variance of the main effects and the two-factor interactions).
//!
//! Runs that cannot all be made in the same conditions (days, batches, shifts) go in
//! blocks: a factorial or a fraction in 2 or 4 blocks, confounding the blocks with the
//! highest-order interactions it can, and the central composite in two blocks (the
//! factorial part and the axial part, with the axial distance that keeps the blocks
//! orthogonal to the model). The worksheet gets a `bloco` column, and the analyses read it.
//!
//! When every factor names a model parameter (`objeto.campo`), the UTSim model runs the
//! experiment: one scenario per point of the design with common random numbers, the
//! replications as the replicates, and the results saved as a data set of origin
//! **Simulado** (the error of the analysis is the simulated variability).

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use utsim_model::experiment::{Scenario, ScenarioResult};

use crate::texto::{dx, fx, tx, vx};
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
    /// The levels of a general factorial, separated by `;` (the two-level plans use `low`
    /// and `high`).
    #[serde(rename = "niveis", default, skip_serializing_if = "String::is_empty")]
    pub levels: String,
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
    /// A general factorial with the levels each factor lists (`fraction` and `screening`
    /// are ignored).
    #[serde(rename = "geral", default, skip_serializing_if = "is_false")]
    pub general: bool,
    /// A response surface plan instead of a two-level one (`fraction` and `screening` are
    /// ignored, and so are the replicates).
    #[serde(
        rename = "superficie",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub surface: Option<Surface>,
    /// Blocks of the plan: 1, 2 or 4 for a factorial or a fraction ([`block_words`]), 1 or
    /// 2 for a central composite; 0 is read as 1. A screening and a Box-Behnken have one.
    #[serde(rename = "blocos", default, skip_serializing_if = "one_block")]
    pub blocks: usize,
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

fn is_false(b: &bool) -> bool {
    !*b
}

fn one_block(b: &usize) -> bool {
    *b <= 1
}

/// A response surface plan.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Surface {
    /// Central composite with the axial points at `α = (2^k)^(1/4)` (rotatable).
    #[serde(rename = "composto_central")]
    CentralComposite,
    /// Central composite with `α = 1`: three levels per factor.
    #[serde(rename = "composto_face")]
    FaceCentered,
    #[serde(rename = "box_behnken")]
    BoxBehnken,
}

impl Surface {
    pub const ALL: [Surface; 3] = [
        Surface::CentralComposite,
        Surface::FaceCentered,
        Surface::BoxBehnken,
    ];

    /// Whether the plan exists for `k` factors (central composite 2 to 6, Box-Behnken 3 to 5).
    pub fn fits(self, k: usize) -> bool {
        match self {
            Surface::BoxBehnken => (3..=5).contains(&k),
            _ => (2..=utsim_doe::MAX_CCD_FACTORS).contains(&k),
        }
    }

    /// The design for `k` factors. In two blocks (central composite only) the centers are
    /// split between the factorial block and the axial block, and the rotatable plan takes
    /// the `α` of orthogonal blocks.
    pub fn design(
        self,
        k: usize,
        centers: usize,
        blocks: usize,
        seed: u64,
    ) -> Result<utsim_doe::Design, String> {
        let b = if blocks == 2 { 2 } else { 1 };
        let split = if b == 2 {
            centers_split(centers)
        } else {
            (centers, 0)
        };
        match self {
            Surface::CentralComposite => utsim_doe::central_composite(
                k,
                if b == 2 {
                    utsim_doe::Alpha::OrthogonalBlocks
                } else {
                    utsim_doe::Alpha::Rotatable
                },
                split,
                b,
                seed,
            ),
            Surface::FaceCentered => {
                utsim_doe::central_composite(k, utsim_doe::Alpha::FaceCentered, split, b, seed)
            }
            Surface::BoxBehnken => {
                if centers == 0 {
                    return Err(tx("experimentos-bb_precisa_centro").into());
                }
                utsim_doe::box_behnken(k, centers, seed)
            }
        }
    }
}

/// The centers of a central composite in two blocks: the extra one in the factorial block.
fn centers_split(centers: usize) -> (usize, usize) {
    (centers.div_ceil(2), centers / 2)
}

/// What a run of a plan is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RunKind {
    Factorial,
    Axial,
    Center,
}

/// One run of any plan, with the coded levels.
#[derive(Debug, Clone, PartialEq)]
pub struct PlanRun {
    pub run_order: usize,
    pub std_order: usize,
    pub kind: RunKind,
    /// The block, from 1.
    pub block: usize,
    pub coded: Vec<f64>,
}

/// The runs of the plan in run order, and the lines that describe it (aliases, resolution,
/// what the surface estimates).
#[derive(Debug, Clone, PartialEq)]
pub struct Plan {
    pub runs: Vec<PlanRun>,
    pub lines: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum ExpError {
    #[error("{}", tx("experimentos-erro_fatores"))]
    Factors,
    #[error("{}", tx("experimentos-erro_sem_nome"))]
    NoName,
    #[error("{}", fx("experimentos-erro_niveis", &[("fator", vx(.0))]))]
    Levels(String),
    #[error("{}", fx("experimentos-erro_fracao", &[("k", vx(.0)), ("p", vx(.1))]))]
    Fraction(usize, usize),
    #[error("{}", fx("experimentos-erro_centro", &[("fator", vx(.0))]))]
    Center(String),
    #[error("{}", fx("experimentos-erro_sem_chave", &[("fator", vx(.0))]))]
    NoKey(String),
    #[error("{}", fx("experimentos-erro_axial", &[("fator", vx(.0))]))]
    Axial(String),
    #[error("{}", fx("experimentos-erro_blocos", &[("n", vx(.0)), ("opcoes", vx(.1))]))]
    Blocks(usize, String),
    #[error("{}", fx("experimentos-erro_niveis_geral", &[("fator", vx(.0)), ("max", vx(&MAX_LEVELS))]))]
    GeneralLevels(String),
    #[error("{0}")]
    Design(String),
}

/// Most factors of a factorial or a fraction (the table of fractions goes to 8).
pub const MAX_FACTORS: usize = 8;

/// Most factors of a Plackett-Burman screening (16 runs).
pub const MAX_SCREENING: usize = 15;

/// Most levels of a factor of a general factorial.
pub const MAX_LEVELS: usize = 10;

/// Most runs of a general factorial, replicates included.
pub const MAX_GENERAL_RUNS: usize = 1000;

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

impl FactorSpec {
    /// The levels of a general factorial, in the order written.
    pub fn level_list(&self) -> Vec<String> {
        self.levels
            .split(';')
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect()
    }

    /// The level written for any coded value: the written levels at `−1`, `0` and `+1`,
    /// and in between or beyond (the axial points) by the numeric levels; empty when the
    /// levels are not numbers.
    pub fn level_at(&self, x: f64) -> String {
        for c in [-1i8, 0, 1] {
            if (x - f64::from(c)).abs() < 1e-9 {
                return self.level(c);
            }
        }
        match (number(&self.low), number(&self.high)) {
            (Some(a), Some(b)) => show(0.5 * (a + b) + x * 0.5 * (b - a)),
            _ => String::new(),
        }
    }
}

impl ExperimentSpec {
    pub fn k(&self) -> usize {
        self.factors.len()
    }

    /// The level of factor `i` at the coded value of a run: in a general factorial the
    /// coded value is the position of the level in the list, from 1.
    pub fn level_text(&self, i: usize, coded: f64) -> String {
        let f = &self.factors[i];
        if self.general && self.surface.is_none() {
            let j = coded.round() as usize;
            f.level_list()
                .get(j.wrapping_sub(1))
                .cloned()
                .unwrap_or_default()
        } else {
            f.level_at(coded)
        }
    }

    /// The general factorial: every combination of the listed levels (the first factor
    /// changing fastest), replicated, in an order drawn from the seed.
    fn general_plan(&self) -> Result<Plan, ExpError> {
        let k = self.k();
        if !(2..=MAX_FACTORS).contains(&k) {
            return Err(ExpError::Factors);
        }
        let mut counts = Vec::new();
        for f in &self.factors {
            if f.name.trim().is_empty() {
                return Err(ExpError::NoName);
            }
            let l = f.level_list();
            let mut d = l.clone();
            d.sort();
            d.dedup();
            if !(2..=MAX_LEVELS).contains(&l.len()) || d.len() != l.len() {
                return Err(ExpError::GeneralLevels(f.name.clone()));
            }
            counts.push(l.len());
        }
        if self.center_points > 0 {
            return Err(ExpError::Design(tx("experimentos-geral_sem_centro").into()));
        }
        let points: usize = counts.iter().product();
        let reps = self.replicates.max(1);
        if points * reps > MAX_GENERAL_RUNS {
            return Err(ExpError::Design(fx(
                "experimentos-geral_grande",
                &[("n", vx(&(points * reps))), ("max", vx(&MAX_GENERAL_RUNS))],
            )));
        }
        let rows: Vec<Vec<i8>> = (0..points)
            .map(|mut p| {
                counts
                    .iter()
                    .map(|c| {
                        let v = p % c;
                        p /= c;
                        (v + 1) as i8
                    })
                    .collect()
            })
            .collect();
        let d = doe::from_rows(k, &[], &rows, reps, 0, self.seed);
        let mut runs: Vec<PlanRun> = d
            .runs
            .iter()
            .map(|r| PlanRun {
                run_order: r.run_order,
                std_order: r.std_order,
                kind: RunKind::Factorial,
                block: 1,
                coded: r.levels.iter().map(|l| f64::from(*l)).collect(),
            })
            .collect();
        runs.sort_by_key(|r| r.run_order);
        let mut lines = vec![fx(
            "experimentos-linha_geral",
            &[
                (
                    "niveis",
                    vx(&counts
                        .iter()
                        .map(|c| c.to_string())
                        .collect::<Vec<_>>()
                        .join(" × ")),
                ),
                ("pontos", vx(&points)),
                ("replicas", vx(&reps)),
            ],
        )];
        if reps == 1 {
            lines.push(tx("experimentos-geral_sem_replica").into());
        }
        Ok(Plan { runs, lines })
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
                .ok_or_else(|| ExpError::Design(fx("experimentos-sem_pb", &[("n", vx(&n))])))?;
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

    /// The numbers of blocks this plan accepts: 2 and 4 for a factorial or a fraction
    /// when no main effect would be confounded with them, 2 for a central composite.
    pub fn block_options(&self) -> Vec<usize> {
        let k = self.k();
        let mut v = vec![1];
        if self.general && self.surface.is_none() {
            return v;
        }
        match self.surface {
            Some(Surface::BoxBehnken) => {}
            Some(_) => v.push(2),
            None if self.screening || !(2..=MAX_FACTORS).contains(&k) => {}
            None => {
                if let Ok(g) = self.generators() {
                    let words = doe::defining_relation(&g);
                    v.extend(
                        [2, 4]
                            .into_iter()
                            .filter(|n| block_words(k, &words, *n).is_some()),
                    );
                }
            }
        }
        v
    }

    fn check_blocks(&self) -> Result<usize, ExpError> {
        let n = self.blocks.max(1);
        let options = self.block_options();
        if options.contains(&n) {
            Ok(n)
        } else {
            let list: Vec<String> = options.iter().map(|o| o.to_string()).collect();
            Err(ExpError::Blocks(n, list.join(", ")))
        }
    }

    /// The plan of any kind: the runs in run order and what it estimates.
    pub fn plan(&self) -> Result<Plan, ExpError> {
        if self.general && self.surface.is_none() {
            self.check_blocks()?;
            return self.general_plan();
        }
        let Some(surface) = self.surface else {
            let d = self.design()?;
            let nb = self.check_blocks()?;
            let gens = if nb > 1 {
                block_words(d.k, &d.words(), nb).unwrap_or_default()
            } else {
                Vec::new()
            };
            let mut std: Vec<&doe::Run> = d.runs.iter().collect();
            std.sort_by_key(|r| r.std_order);
            let mut center = 0;
            let mut runs: Vec<PlanRun> = std
                .into_iter()
                .map(|r| {
                    let block = if r.center {
                        center += 1;
                        (center - 1) % nb + 1
                    } else {
                        block_of(&r.levels, &gens)
                    };
                    PlanRun {
                        run_order: r.run_order,
                        std_order: r.std_order,
                        kind: if r.center {
                            RunKind::Center
                        } else {
                            RunKind::Factorial
                        },
                        block,
                        coded: r.levels.iter().map(|l| f64::from(*l)).collect(),
                    }
                })
                .collect();
            // The drawn order is kept inside each block, one block after the other.
            runs.sort_by_key(|r| (r.block, r.run_order));
            for (i, r) in runs.iter_mut().enumerate() {
                r.run_order = i + 1;
            }
            let mut lines = alias_lines(self, &d);
            if nb > 1 {
                let mut group = gens.clone();
                if let [a, b] = gens[..] {
                    group.push(a ^ b);
                }
                lines.push(fx(
                    "experimentos-linha_blocos",
                    &[
                        ("n", vx(&nb)),
                        (
                            "termos",
                            vx(&group
                                .iter()
                                .map(|w| doe::term_name(*w))
                                .collect::<Vec<_>>()
                                .join(", ")),
                        ),
                    ],
                ));
            }
            return Ok(Plan { runs, lines });
        };
        let k = self.k();
        if !surface.fits(k) {
            return Err(ExpError::Design(match surface {
                Surface::BoxBehnken => tx("experimentos-bb_faixa").into(),
                _ => fx(
                    "experimentos-ccd_faixa",
                    &[("max", vx(&utsim_doe::MAX_CCD_FACTORS))],
                ),
            }));
        }
        for f in &self.factors {
            if f.name.trim().is_empty() {
                return Err(ExpError::NoName);
            }
            if f.low.trim().is_empty() || f.high.trim() == f.low.trim() || f.high.trim().is_empty()
            {
                return Err(ExpError::Levels(f.name.clone()));
            }
            if f.level(0).is_empty() {
                return Err(ExpError::Center(f.name.clone()));
            }
            if surface == Surface::CentralComposite
                && (number(&f.low).is_none() || number(&f.high).is_none())
            {
                return Err(ExpError::Axial(f.name.clone()));
            }
        }
        let nb = self.check_blocks()?;
        let d = surface
            .design(k, self.center_points, nb, self.seed)
            .map_err(ExpError::Design)?;
        let terms = utsim_doe::quadratic_terms(k).len() + 1;
        let alpha = d
            .alpha
            .map(|a| show(a).replace(',', "."))
            .unwrap_or_default();
        let nc = self.center_points;
        let mut lines = vec![match surface {
            Surface::CentralComposite => fx(
                "experimentos-linha_ccd",
                &[
                    ("fatoriais", vx(&(1 << k))),
                    ("axiais", vx(&(2 * k))),
                    ("alfa", dx(alpha)),
                    ("centros", vx(&nc)),
                ],
            ),
            Surface::FaceCentered => fx(
                "experimentos-linha_face",
                &[
                    ("fatoriais", vx(&(1 << k))),
                    ("axiais", vx(&(2 * k))),
                    ("centros", vx(&nc)),
                ],
            ),
            Surface::BoxBehnken => fx(
                "experimentos-linha_bb",
                &[("pontos", vx(&(d.points.len() - nc))), ("centros", vx(&nc))],
            ),
        }];
        if nb == 2 {
            let (c1, c2) = centers_split(nc);
            lines.push(fx(
                "experimentos-linha_blocos_ccd",
                &[("c1", vx(&c1)), ("c2", vx(&c2))],
            ));
        }
        lines.push(fx(
            "experimentos-linha_segunda_ordem",
            &[("termos", vx(&terms))],
        ));
        if d.points.len() <= terms {
            lines.push(tx("experimentos-sem_gl_erro").into());
        }
        let mut runs: Vec<PlanRun> = d
            .points
            .iter()
            .map(|p| PlanRun {
                run_order: p.run_order,
                std_order: p.std_order,
                kind: match p.kind {
                    utsim_doe::Kind::Factorial => RunKind::Factorial,
                    utsim_doe::Kind::Axial => RunKind::Axial,
                    utsim_doe::Kind::Center => RunKind::Center,
                },
                block: p.block,
                coded: p.coded.clone(),
            })
            .collect();
        runs.sort_by_key(|r| r.run_order);
        Ok(Plan { runs, lines })
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

/// A coded level: `-1`, `0` and `1` as integers, the axial ones with a decimal comma.
fn coded_cell(x: f64) -> String {
    if (x - x.round()).abs() < 1e-9 {
        format!("{}", x.round() as i64)
    } else {
        show(x)
    }
}

fn factor_cells(spec: &ExperimentSpec, levels: &[f64]) -> Vec<String> {
    let mut c: Vec<String> = levels.iter().map(|l| coded_cell(*l)).collect();
    c.extend(
        levels
            .iter()
            .enumerate()
            .map(|(i, l)| cell(&spec.level_text(i, *l))),
    );
    c
}

/// The collection worksheet in run order: `ordem;ordem_padrao;centro;A;B;...;<fatores>;
/// <resposta>` with the response empty, to be filled and imported as a data set. The
/// column `centro` says `sim`, `não` or, for the axial points of a surface, `axial`; a
/// plan in blocks has a `bloco` column right after it.
pub fn worksheet(spec: &ExperimentSpec, plan: &Plan) -> String {
    let blocked = plan.runs.iter().any(|r| r.block > 1);
    let mut h = vec![
        "ordem".to_string(),
        "ordem_padrao".to_string(),
        "centro".to_string(),
    ];
    if blocked {
        h.push(BLOCK_COLUMN.to_string());
    }
    h.extend(factor_header(spec));
    h.push(spec.response_name());
    let mut out = h.join(";");
    out.push('\n');
    for r in &plan.runs {
        let mut c = vec![
            r.run_order.to_string(),
            r.std_order.to_string(),
            match r.kind {
                RunKind::Center => "sim",
                RunKind::Axial => "axial",
                RunKind::Factorial => "não",
            }
            .to_string(),
        ];
        if blocked {
            c.push(r.block.to_string());
        }
        c.extend(factor_cells(spec, &r.coded));
        c.push(String::new());
        out.push_str(&c.join(";"));
        out.push('\n');
    }
    out
}

/// The name of the block column of a worksheet (the analyses look for it).
pub const BLOCK_COLUMN: &str = "bloco";

/// The lowest order among a term and its aliases in a fraction (0 when the term is in the
/// defining relation).
fn lowest_alias(term: u32, words: &[u32]) -> usize {
    std::iter::once(term)
        .chain(words.iter().map(|w| term ^ w))
        .map(doe::order)
        .min()
        .unwrap_or(0)
}

/// The block generators of a two-level plan in `n` blocks (2 or 4), for `k` factors and the
/// `words` of the fraction's defining relation (none for a full factorial). The blocks are
/// confounded with the generators (and, in 4 blocks, with their product too); the choice
/// makes the lowest-order effect confounded with a block, aliases included, as high as
/// possible, then the fewest of them at that order. `None` when a main effect would be
/// confounded with the blocks.
pub fn block_words(k: usize, words: &[u32], n: usize) -> Option<Vec<u32>> {
    let all: Vec<u32> = (1u32..(1 << k)).collect();
    // The key: the lowest order confounded, how many of the group are above it, and the
    // generators' own order (high first).
    let key = |group: &[u32]| {
        let lows: Vec<usize> = group.iter().map(|b| lowest_alias(*b, words)).collect();
        let min = lows.iter().copied().min().unwrap_or(0);
        let above = lows.iter().filter(|l| **l > min).count();
        let own: usize = group.iter().map(|b| doe::order(*b)).sum();
        (min, above, own)
    };
    let mut best: Option<((usize, usize, usize), Vec<u32>)> = None;
    match n {
        2 => {
            for b in &all {
                let kk = key(&[*b]);
                if best.as_ref().is_none_or(|(bk, _)| kk > *bk) {
                    best = Some((kk, vec![*b]));
                }
            }
        }
        4 => {
            for (i, b1) in all.iter().enumerate() {
                for b2 in &all[i + 1..] {
                    let b3 = b1 ^ b2;
                    if b3 < *b2 {
                        continue;
                    }
                    let kk = key(&[*b1, *b2, b3]);
                    if best.as_ref().is_none_or(|(bk, _)| kk > *bk) {
                        best = Some((kk, vec![*b1, *b2]));
                    }
                }
            }
        }
        _ => return None,
    }
    best.filter(|(kk, _)| kk.0 >= 2).map(|(_, g)| g)
}

/// The block of a factorial run, from 1: one bit per generator whose sign differs from its
/// sign at the run with every factor low, so that run is in block 1.
fn block_of(levels: &[i8], gens: &[u32]) -> usize {
    1 + gens
        .iter()
        .enumerate()
        .map(|(j, g)| {
            let s: i32 = (0..levels.len())
                .filter(|i| g & (1 << i) != 0)
                .map(|i| i32::from(levels[i]))
                .product();
            let low = if doe::order(*g) % 2 == 0 { 1 } else { -1 };
            if s == low { 0 } else { 1 << j }
        })
        .sum::<usize>()
}

/// The defining relation, the resolution and the aliases of the main effects and the
/// two-factor interactions, in words (`I = ABCD`, "resolução IV", `A = BCD`...); for a
/// Plackett-Burman screening, what it estimates and what it confounds.
pub fn alias_lines(spec: &ExperimentSpec, d: &Design) -> Vec<String> {
    if spec.screening {
        let n = d.runs.iter().filter(|r| !r.center).count() / d.replicates.max(1);
        let mut v = vec![
            fx("experimentos-pb_titulo", &[("n", vx(&n)), ("k", vx(&d.k))]),
            tx("experimentos-pb_livres").to_string(),
            tx("experimentos-pb_confundida").to_string(),
        ];
        if n > d.k + 1 {
            v.push(fx(
                "experimentos-pb_colunas_livres",
                &[("n", vx(&(n - 1 - d.k)))],
            ));
        }
        return v;
    }
    let words = d.words();
    if words.is_empty() {
        return vec![tx("experimentos-fatorial_completo").to_string()];
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
        v.push(match roman.get(r) {
            Some(x) => fx("experimentos-resolucao", &[("r", vx(x))]),
            None => tx("experimentos-resolucao_alta").to_string(),
        });
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
pub fn scenarios(
    spec: &ExperimentSpec,
    plan: &Plan,
) -> Result<Vec<(Vec<f64>, Scenario)>, ExpError> {
    spec.on_model()?;
    let mut points: Vec<(usize, Vec<f64>)> = Vec::new();
    for r in &plan.runs {
        if !points.iter().any(|(_, l)| *l == r.coded) {
            points.push((r.std_order, r.coded.clone()));
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
                .enumerate()
                .map(|(i, (f, l))| (f.key.trim().to_string(), spec.level_text(i, *l)))
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
    points: &[(Vec<f64>, Scenario)],
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
