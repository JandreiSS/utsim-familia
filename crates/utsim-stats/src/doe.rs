//! Two-level factorial experiments: the full factorial `2^k` and the fractional `2^(k−p)`
//! with its generators, defining relation, resolution and alias structure, center points,
//! replicates and a run order drawn from a recorded seed; and the analysis of the results
//! (effects and coefficients, ANOVA with the error from replicates, center points or
//! pooled terms, the curvature test by the center points, Lenth's pseudo standard error
//! when no degree of freedom is left, and the data of the main effects and interaction
//! plots). Section 2.4 of `docs/plano_lean_seis_sigma.md` and item 13 of
//! `docs/plano_analises_e_dinamica.md`.
//!
//! A term is a bit mask over the factors: bit `i` is factor `i` (letter `A` + `i`), so
//! `0b011` is the interaction `AB`. Coded levels are `−1` and `+1`, `0` at the center.

use serde::{Deserialize, Serialize};

use crate::special::{f_sf, t_cdf, t_ppf};

/// Most factors of a design (a term is a `u32` mask; the letters go to `P`).
pub const MAX_FACTORS: usize = 16;

/// The letter of factor `i`: `A`, `B`, ...
pub fn letter(i: usize) -> char {
    (b'A' + i as u8) as char
}

/// The name of a term: `A`, `AB`, `ACD`; `I` for the identity (the mean).
pub fn term_name(mask: u32) -> String {
    if mask == 0 {
        return "I".to_string();
    }
    (0..MAX_FACTORS)
        .filter(|i| mask & (1 << i) != 0)
        .map(letter)
        .collect()
}

/// The order of a term (how many factors it multiplies).
pub fn order(mask: u32) -> usize {
    mask.count_ones() as usize
}

/// A generator of a fraction: the factor `factor` is set to the product of the base
/// factors in `word` (for `D = ABC`: factor 3, word `0b0111`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Generator {
    pub factor: usize,
    pub word: u32,
}

impl Generator {
    /// The word of the defining relation: `I = ABCD` for `D = ABC`.
    pub fn defining_word(self) -> u32 {
        self.word | (1 << self.factor)
    }
}

/// Usual generators of the `2^(k−p)` fractions of highest resolution, for 3 to 8 factors
/// (the last `p` factors are generated from the first `k − p`). `None` when the fraction
/// is not in the table or `p = 0`.
pub fn standard_generators(k: usize, p: usize) -> Option<Vec<Generator>> {
    const A: u32 = 1;
    const B: u32 = 2;
    const C: u32 = 4;
    const D: u32 = 8;
    const E: u32 = 16;
    const F: u32 = 32;
    let words: &[u32] = match (k, p) {
        (3, 1) => &[A | B],
        (4, 1) => &[A | B | C],
        (5, 1) => &[A | B | C | D],
        (5, 2) => &[A | B, A | C],
        (6, 1) => &[A | B | C | D | E],
        (6, 2) => &[A | B | C, B | C | D],
        (6, 3) => &[A | B, A | C, B | C],
        (7, 1) => &[A | B | C | D | E | F],
        (7, 2) => &[A | B | C | D, A | B | E],
        (7, 3) => &[A | B | C, B | C | D, A | C | D],
        (7, 4) => &[A | B, A | C, B | C, A | B | C],
        (8, 2) => &[A | B | C | D, A | B | E | F],
        (8, 3) => &[A | B | C, A | B | D, B | C | D | E],
        (8, 4) => &[B | C | D, A | C | D, A | B | C, A | B | D],
        _ => return None,
    };
    Some(
        words
            .iter()
            .enumerate()
            .map(|(i, w)| Generator {
                factor: k - p + i,
                word: *w,
            })
            .collect(),
    )
}

/// The words of the defining relation: every product of the generators' words (without
/// the identity), in increasing order of length and then of mask.
pub fn defining_relation(generators: &[Generator]) -> Vec<u32> {
    let base: Vec<u32> = generators.iter().map(|g| g.defining_word()).collect();
    let mut words = Vec::new();
    for subset in 1u32..(1 << base.len()) {
        let w = base
            .iter()
            .enumerate()
            .filter(|(i, _)| subset & (1 << i) != 0)
            .fold(0, |acc, (_, w)| acc ^ w);
        if w != 0 && !words.contains(&w) {
            words.push(w);
        }
    }
    words.sort_by_key(|w| (order(*w), *w));
    words
}

/// The resolution: the length of the shortest word (`None` for a full factorial).
pub fn resolution(words: &[u32]) -> Option<usize> {
    words.iter().map(|w| order(*w)).min()
}

/// The terms confounded with `term` in a fraction: `term · word` for each word.
pub fn aliases(term: u32, words: &[u32]) -> Vec<u32> {
    let mut v: Vec<u32> = words.iter().map(|w| term ^ w).collect();
    v.sort_by_key(|w| (order(*w), *w));
    v
}

/// One run of a design.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Run {
    /// Order in the standard (Yates) table, from 1; center points after the factorial ones.
    pub std_order: usize,
    /// Order in which to run it, from 1.
    pub run_order: usize,
    /// Coded levels, one per factor: `−1`, `+1` or `0` at the center.
    pub levels: Vec<i8>,
    pub center: bool,
    /// Replicate of the factorial point, from 1.
    pub replicate: usize,
}

/// A two-level design.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Design {
    pub k: usize,
    pub generators: Vec<Generator>,
    pub replicates: usize,
    pub center_points: usize,
    pub seed: u64,
    /// In run order.
    pub runs: Vec<Run>,
}

impl Design {
    /// `p`, the number of generated factors.
    pub fn p(&self) -> usize {
        self.generators.len()
    }

    pub fn words(&self) -> Vec<u32> {
        defining_relation(&self.generators)
    }

    pub fn resolution(&self) -> Option<usize> {
        resolution(&self.words())
    }

    /// Distinct factorial points (`2^(k−p)`).
    pub fn points(&self) -> usize {
        1 << (self.k - self.p())
    }
}

/// The standard order of a full factorial in `k` factors: the first factor alternates
/// fastest (`−,+,−,+...`), the second every two runs, and so on.
pub fn full_factorial(k: usize) -> Vec<Vec<i8>> {
    (0..1usize << k)
        .map(|r| {
            (0..k)
                .map(|i| if r & (1 << i) == 0 { -1 } else { 1 })
                .collect()
        })
        .collect()
}

/// SplitMix64, to draw the run order from a recorded seed.
struct SplitMix(u64);

impl SplitMix {
    fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    fn below(&mut self, n: usize) -> usize {
        (self.next() % n as u64) as usize
    }
}

/// A `2^(k−p)` design: the base factors in standard order, the generated ones from the
/// generators, each point `replicates` times, `center_points` runs at the center, and the
/// run order shuffled by `seed` (the same seed gives the same order).
pub fn design(
    k: usize,
    generators: &[Generator],
    replicates: usize,
    center_points: usize,
    seed: u64,
) -> Result<Design, String> {
    if !(2..=MAX_FACTORS).contains(&k) {
        return Err(format!(
            "o experimento precisa de 2 a {MAX_FACTORS} fatores"
        ));
    }
    let p = generators.len();
    if p >= k - 1 {
        return Err("frações demais para o número de fatores".into());
    }
    let base = k - p;
    for (i, g) in generators.iter().enumerate() {
        if g.factor != base + i {
            return Err("os geradores definem os últimos fatores, em ordem".into());
        }
        if g.word == 0 || g.word >> base != 0 {
            return Err(format!(
                "o gerador de {} usa só os fatores básicos",
                letter(g.factor)
            ));
        }
        if order(g.word) < 2 {
            return Err(format!(
                "o gerador de {} confunde o fator com um efeito principal",
                letter(g.factor)
            ));
        }
    }
    let rows: Vec<Vec<i8>> = full_factorial(base)
        .into_iter()
        .map(|row| {
            let mut levels = row.clone();
            for g in generators {
                let v = (0..base)
                    .filter(|i| g.word & (1 << i) != 0)
                    .fold(1i8, |acc, i| acc * row[i]);
                levels.push(v);
            }
            levels
        })
        .collect();
    Ok(from_rows(
        k,
        generators,
        &rows,
        replicates,
        center_points,
        seed,
    ))
}

/// A two-level design from its distinct points in standard order (`±1` per factor), such
/// as the columns of a Plackett-Burman matrix: each point `replicates` times,
/// `center_points` runs at the center, and the run order shuffled by `seed`. With no
/// generators the design records no defining relation (a Plackett-Burman design has none;
/// its interactions are partially confounded with the main effects).
pub fn from_rows(
    k: usize,
    generators: &[Generator],
    rows: &[Vec<i8>],
    replicates: usize,
    center_points: usize,
    seed: u64,
) -> Design {
    let reps = replicates.max(1);
    let mut runs = Vec::new();
    let mut std = 0;
    for rep in 1..=reps {
        for levels in rows {
            std += 1;
            runs.push(Run {
                std_order: std,
                run_order: 0,
                levels: levels.clone(),
                center: false,
                replicate: rep,
            });
        }
    }
    for _ in 0..center_points {
        std += 1;
        runs.push(Run {
            std_order: std,
            run_order: 0,
            levels: vec![0; k],
            center: true,
            replicate: 1,
        });
    }
    let mut rng = SplitMix(seed);
    for i in (1..runs.len()).rev() {
        let j = rng.below(i + 1);
        runs.swap(i, j);
    }
    for (i, r) in runs.iter_mut().enumerate() {
        r.run_order = i + 1;
    }
    Design {
        k,
        generators: generators.to_vec(),
        replicates: reps,
        center_points,
        seed,
        runs,
    }
}

// --- Analysis -----------------------------------------------------------------------------------

/// The estimate of one term.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Effect {
    pub term: u32,
    /// Mean response at `+` minus the mean at `−`.
    pub effect: f64,
    /// Half the effect (the coefficient of the coded model).
    pub coef: f64,
    pub ss: f64,
    /// Standard error of the coefficient, t and p-value (from the ANOVA error or from
    /// Lenth's pseudo standard error).
    pub se: Option<f64>,
    pub t: Option<f64>,
    pub p_value: Option<f64>,
    /// Terms of the candidate model with the same column (confounded with this one).
    pub aliases: Vec<u32>,
}

/// One line of the ANOVA.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AnovaLine {
    pub source: String,
    pub df: f64,
    pub ss: f64,
    pub ms: f64,
    pub f: Option<f64>,
    pub p_value: Option<f64>,
}

/// Where the error of the tests came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ErrorKind {
    /// Residual degrees of freedom: replicates, center points or terms left out.
    Residual,
    /// No degree of freedom left: Lenth's pseudo standard error of the effects.
    Lenth,
}

/// Lenth's method for an unreplicated design.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Lenth {
    /// Pseudo standard error of the effects.
    pub pse: f64,
    pub df: f64,
    /// Margin of error: an effect beyond it is active.
    pub me: f64,
}

/// The analysis of a two-level experiment.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Analysis {
    pub n: usize,
    pub factorial_runs: usize,
    pub center_runs: usize,
    pub mean: f64,
    /// In order of the term (main effects first).
    pub effects: Vec<Effect>,
    pub anova: Vec<AnovaLine>,
    pub error: ErrorKind,
    pub lenth: Option<Lenth>,
    /// Curvature by the center points: mean of the factorial runs minus mean of the center.
    pub curvature: Option<f64>,
    pub r2: f64,
    /// Terms of the candidate model left out because their column is that of an earlier
    /// term (or of the mean).
    pub confounded: Vec<(u32, u32)>,
}

impl Analysis {
    pub fn effect(&self, term: u32) -> Option<&Effect> {
        self.effects.iter().find(|e| e.term == term)
    }
}

fn mean(v: &[f64]) -> f64 {
    v.iter().sum::<f64>() / v.len() as f64
}

fn median(mut v: Vec<f64>) -> f64 {
    v.sort_by(|a, b| a.total_cmp(b));
    let n = v.len();
    if n == 0 {
        f64::NAN
    } else if n % 2 == 1 {
        v[n / 2]
    } else {
        0.5 * (v[n / 2 - 1] + v[n / 2])
    }
}

/// Lenth's pseudo standard error of `effects`: `s0 = 1,5·median|e|`, then
/// `PSE = 1,5·median{|e| : |e| < 2,5·s0}`, with `m/3` degrees of freedom.
pub fn lenth(effects: &[f64], level: f64) -> Option<Lenth> {
    if effects.len() < 2 {
        return None;
    }
    let abs: Vec<f64> = effects.iter().map(|e| e.abs()).collect();
    let s0 = 1.5 * median(abs.clone());
    let kept: Vec<f64> = abs.into_iter().filter(|a| *a < 2.5 * s0).collect();
    let pse = 1.5 * median(kept);
    if pse.is_nan() || pse <= 0.0 {
        return None;
    }
    let df = effects.len() as f64 / 3.0;
    let me = t_ppf(1.0 - (1.0 - level) / 2.0, df) * pse;
    Some(Lenth { pse, df, me })
}

/// Analyses a two-level experiment. `x[r][i]` is the level of factor `i` in run `r`, coded
/// in `[−1, 1]` (`0` at the center; every other run all `±1`); `y[r]` its response. The
/// candidate model has every term up to `max_order` factors; a term whose column repeats
/// an earlier one is confounded with it (fractions) and left out.
pub fn analyze(
    x: &[Vec<f64>],
    y: &[f64],
    max_order: usize,
    level: f64,
) -> Result<Analysis, String> {
    if x.len() != y.len() || x.is_empty() {
        return Err(
            "as colunas dos fatores e a resposta precisam do mesmo número de linhas".into(),
        );
    }
    let k = x[0].len();
    if !(1..=MAX_FACTORS).contains(&k) || x.iter().any(|r| r.len() != k) {
        return Err("número de fatores inválido".into());
    }
    if y.iter().any(|v| !v.is_finite()) {
        return Err("resposta vazia ou não numérica em alguma linha".into());
    }
    let mut fx: Vec<Vec<f64>> = Vec::new();
    let mut fy = Vec::new();
    let mut cy = Vec::new();
    for (row, v) in x.iter().zip(y) {
        if row.iter().all(|l| l.abs() < 1e-9) {
            cy.push(*v);
        } else if row.iter().all(|l| (l.abs() - 1.0).abs() < 1e-9) {
            fx.push(row.iter().map(|l| l.signum()).collect());
            fy.push(*v);
        } else {
            return Err(
                "cada corrida deve ter todos os fatores em −1/+1 ou todos no centro".into(),
            );
        }
    }
    let nf = fy.len();
    if nf < 2 {
        return Err("o experimento precisa de ao menos duas corridas fatoriais".into());
    }
    let column = |mask: u32| -> Vec<f64> {
        fx.iter()
            .map(|r| {
                (0..k)
                    .filter(|i| mask & (1 << i) != 0)
                    .fold(1.0, |a, i| a * r[i])
            })
            .collect()
    };
    let max_order = max_order.clamp(1, k);
    let mut candidates: Vec<u32> = (1u32..(1 << k))
        .filter(|m| order(*m) <= max_order)
        .collect();
    candidates.sort_by_key(|m| (order(*m), *m));
    let mut kept: Vec<(u32, Vec<f64>)> = Vec::new();
    let mut confounded = Vec::new();
    for m in candidates {
        let c = column(m);
        let sum: f64 = c.iter().sum();
        if (sum.abs() - nf as f64).abs() < 1e-9 {
            confounded.push((m, 0));
            continue;
        }
        if let Some((first, _)) = kept.iter().find(|(_, o)| {
            let dot: f64 = o.iter().zip(&c).map(|(a, b)| a * b).sum();
            (dot.abs() - nf as f64).abs() < 1e-9
        }) {
            confounded.push((m, *first));
            continue;
        }
        if sum.abs() > 1e-9 {
            return Err(format!(
                "a coluna de {} não é balanceada: confira se o plano está completo",
                term_name(m)
            ));
        }
        if kept.iter().any(|(_, o)| {
            let dot: f64 = o.iter().zip(&c).map(|(a, b)| a * b).sum();
            dot.abs() > 1e-9
        }) {
            return Err(format!(
                "a coluna de {} não é ortogonal às outras: confira se o plano está completo",
                term_name(m)
            ));
        }
        kept.push((m, c));
    }
    let n = y.len();
    let grand = mean(y);
    let ybar_f = mean(&fy);
    let ss_total: f64 = y.iter().map(|v| (v - grand).powi(2)).sum();
    let mut effects: Vec<Effect> = kept
        .iter()
        .map(|(m, c)| {
            let s: f64 = c.iter().zip(&fy).map(|(a, b)| a * b).sum();
            let coef = s / nf as f64;
            Effect {
                term: *m,
                effect: 2.0 * coef,
                coef,
                ss: nf as f64 * coef * coef,
                se: None,
                t: None,
                p_value: None,
                aliases: confounded
                    .iter()
                    .filter(|(_, f)| f == m)
                    .map(|(a, _)| *a)
                    .collect(),
            }
        })
        .collect();
    let nc = cy.len();
    let curvature = (nc > 0).then(|| ybar_f - mean(&cy));
    let ss_curv = curvature.map_or(0.0, |d| (nf * nc) as f64 * d * d / (nf + nc) as f64);
    let ss_model: f64 = effects.iter().map(|e| e.ss).sum();
    let df_e = n as f64 - 1.0 - effects.len() as f64 - if nc > 0 { 1.0 } else { 0.0 };
    let ss_e = (ss_total - ss_model - ss_curv).max(0.0);
    let mut anova = Vec::new();
    let (error, len) = if df_e >= 1.0 {
        let mse = ss_e / df_e;
        let se = (mse / nf as f64).sqrt();
        for e in &mut effects {
            e.se = Some(se);
            if se > 0.0 {
                let t = e.coef / se;
                e.t = Some(t);
                e.p_value = Some(2.0 * (1.0 - t_cdf(t.abs(), df_e)));
            }
        }
        for e in &effects {
            let f = (mse > 0.0).then(|| e.ss / mse);
            anova.push(AnovaLine {
                source: term_name(e.term),
                df: 1.0,
                ss: e.ss,
                ms: e.ss,
                f,
                p_value: f.map(|f| f_sf(f, 1.0, df_e)),
            });
        }
        if nc > 0 {
            let f = (mse > 0.0).then(|| ss_curv / mse);
            anova.push(AnovaLine {
                source: "curvatura".into(),
                df: 1.0,
                ss: ss_curv,
                ms: ss_curv,
                f,
                p_value: f.map(|f| f_sf(f, 1.0, df_e)),
            });
        }
        anova.push(AnovaLine {
            source: "erro".into(),
            df: df_e,
            ss: ss_e,
            ms: mse,
            f: None,
            p_value: None,
        });
        (ErrorKind::Residual, None)
    } else {
        let all: Vec<f64> = effects.iter().map(|e| e.effect).collect();
        let l = lenth(&all, level);
        if let Some(l) = &l {
            for e in &mut effects {
                let t = e.effect / l.pse;
                e.se = Some(l.pse / 2.0);
                e.t = Some(t);
                e.p_value = Some(2.0 * (1.0 - t_cdf(t.abs(), l.df)));
            }
        }
        for e in &effects {
            anova.push(AnovaLine {
                source: term_name(e.term),
                df: 1.0,
                ss: e.ss,
                ms: e.ss,
                f: None,
                p_value: None,
            });
        }
        (ErrorKind::Lenth, l)
    };
    anova.push(AnovaLine {
        source: "total".into(),
        df: n as f64 - 1.0,
        ss: ss_total,
        ms: f64::NAN,
        f: None,
        p_value: None,
    });
    Ok(Analysis {
        n,
        factorial_runs: nf,
        center_runs: nc,
        mean: grand,
        effects,
        anova,
        error,
        lenth: len,
        curvature,
        r2: if ss_total > 0.0 {
            (ss_model + ss_curv) / ss_total
        } else {
            0.0
        },
        confounded,
    })
}

/// Mean response at the low and the high level of factor `i` (main effects plot), over
/// the factorial runs.
pub fn main_effect_means(x: &[Vec<f64>], y: &[f64], i: usize) -> (f64, f64) {
    let at = |s: f64| {
        let v: Vec<f64> = x
            .iter()
            .zip(y)
            .filter(|(r, _)| (r[i] - s).abs() < 1e-9)
            .map(|(_, v)| *v)
            .collect();
        if v.is_empty() { f64::NAN } else { mean(&v) }
    };
    (at(-1.0), at(1.0))
}

/// Mean response at the four combinations of factors `i` and `j` (interaction plot):
/// `[[i−j−, i+j−], [i−j+, i+j+]]`, so each inner pair is a line from `i−` to `i+`.
pub fn interaction_means(x: &[Vec<f64>], y: &[f64], i: usize, j: usize) -> [[f64; 2]; 2] {
    let at = |a: f64, b: f64| {
        let v: Vec<f64> = x
            .iter()
            .zip(y)
            .filter(|(r, _)| (r[i] - a).abs() < 1e-9 && (r[j] - b).abs() < 1e-9)
            .map(|(_, v)| *v)
            .collect();
        if v.is_empty() { f64::NAN } else { mean(&v) }
    };
    [
        [at(-1.0, -1.0), at(1.0, -1.0)],
        [at(-1.0, 1.0), at(1.0, 1.0)],
    ]
}

/// Codes a column of actual levels into `[−1, 1]`: the smallest value is `−1`, the
/// largest `+1`, the middle `0`. `None` when the column has a single value.
pub fn code(values: &[f64]) -> Option<Vec<f64>> {
    let lo = values.iter().copied().fold(f64::INFINITY, f64::min);
    let hi = values.iter().copied().fold(f64::NEG_INFINITY, f64::max);
    if hi.is_nan() || lo.is_nan() || hi <= lo {
        return None;
    }
    let mid = 0.5 * (lo + hi);
    let half = 0.5 * (hi - lo);
    Some(values.iter().map(|v| (v - mid) / half).collect())
}

#[cfg(test)]
mod tests;
