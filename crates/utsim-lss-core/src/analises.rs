//! Statistical analyses of a project's data sets (stage L3, `docs/plano_lean_seis_sigma.md`,
//! 2.2 and 2.3) and the measurement system analysis (P1, decision M1), over `utsim-stats`
//! and `utsim-quality`. An analysis is a **recipe** (which function, which columns, which
//! parameters) applied to a data set; the journal keeps the recipe, the data set with its
//! hash and origin, and the summary of the result, so the tollgate report cites it and
//! anyone runs it again on the same file. The recipe is recalculated, never typed.
//!
//! The measurement studies are the Gage R&R, the type 1 study, bias, linearity and the
//! attribute agreement (kappa). The Medir checks of the tollgate come from here:
//! [`msa_approved`] (a Gage R&R with %VE of the GRR ≤ 30 % or an attribute study with
//! kappa ≥ 0,75, or the justification recorded) and [`baseline_real`] (a
//! capability or chart on a data set of origin `Real`).

use crate::texto::{dx, fx, tx, vx};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use utsim_quality::arl;
use utsim_quality::attribute::{self, Rating};
use utsim_quality::charts::{self, ChartData, ChartKind};
use utsim_quality::memory::{Cusum, Ewma};
use utsim_quality::msa::{self, GageData, GageOptions, Measurement};
use utsim_quality::rules::RuleSet;
use utsim_stats::descriptive::{self, QuantileMethod};
use utsim_stats::hypothesis::{self, Alternative};
use utsim_stats::nonparametric::{self, RankMethod};
use utsim_stats::{anova2, doe, power, ranks, regression, variances};

use crate::dados::{self, DataOrigin, Dataset};
use crate::date::Date;
use crate::domain::enum_value;
use crate::id::Id;
use crate::op::{Edit, EntityRef, Kind};
use crate::state::{Entity, State};

pub mod fields {
    pub const PROJECT: &str = "projeto";
    pub const NAME: &str = "nome";
    pub const KIND: &str = "tipo";
    /// `estatistica`: an analysis of this module (the model's analyses have their own).
    pub const FAMILY: &str = "familia";
    pub const RECIPE: &str = "receita";
    pub const DATASET: &str = "conjunto";
    pub const DATASET_HASH: &str = "conjunto_hash";
    pub const ORIGIN: &str = "origem";
    pub const SUMMARY: &str = "resumo";
    pub const VERDICT: &str = "conclusao";
    pub const VERSION: &str = "versao";
    pub const DATE: &str = "data";
    /// A justification of an MSA outside the limits, recorded by the belt.
    pub const JUSTIFICATION: &str = "justificativa";
}

pub const FAMILY: &str = "estatistica";
/// Confidence level of the intervals and tests.
pub const LEVEL: f64 = 0.95;
/// %VE of the GRR accepted for the tollgate (conditional above 10 %).
pub const GRR_LIMIT: f64 = 30.0;
/// Significance of the bias and linearity tests.
pub const ALPHA: f64 = 1.0 - LEVEL;
/// The label of Fleiss' kappa over all ratings of an attribute study.
pub const KAPPA: &str = "kappa de Fleiss";
/// Kappa accepted for an attribute measurement system.
pub const KAPPA_LIMIT: f64 = 0.75;

/// What to compute, with the columns (0-based) of the data set.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "funcao")]
pub enum Recipe {
    #[serde(rename = "descritiva")]
    Descriptive { col: usize },
    #[serde(rename = "normalidade")]
    Normality { col: usize },
    /// Capability of normal data; `subgroup` > 1 groups consecutive values.
    #[serde(rename = "capacidade")]
    Capability {
        col: usize,
        lsl: Option<f64>,
        usl: Option<f64>,
        target: Option<f64>,
        subgroup: usize,
    },
    /// Control chart. Variables: `col` with `subgroup`; attributes: `col` the counts and
    /// `size_col` the sample sizes (for p, np and u).
    #[serde(rename = "carta")]
    Chart {
        kind: ChartKind,
        col: usize,
        subgroup: usize,
        size_col: Option<usize>,
        nelson: bool,
    },
    #[serde(rename = "t_uma_amostra")]
    TOne { col: usize, mu0: f64 },
    #[serde(rename = "t_duas_amostras")]
    TTwo { a: usize, b: usize, pooled: bool },
    #[serde(rename = "t_pareado")]
    TPaired { a: usize, b: usize },
    /// One-way ANOVA: the values of `col` grouped by the labels of `group`.
    #[serde(rename = "anova")]
    Anova { col: usize, group: usize },
    /// Chi-square of independence of two categorical columns.
    #[serde(rename = "qui_quadrado")]
    ChiSquare { a: usize, b: usize },
    #[serde(rename = "proporcao")]
    Proportion { x: u64, n: u64, p0: f64 },
    #[serde(rename = "duas_proporcoes")]
    TwoProportions { x1: u64, n1: u64, x2: u64, n2: u64 },
    #[serde(rename = "mann_whitney")]
    MannWhitney { a: usize, b: usize },
    #[serde(rename = "wilcoxon")]
    Wilcoxon { a: usize, b: usize },
    /// Gage R&R: part, operator and value columns.
    #[serde(rename = "gage_rr")]
    GageRr {
        part: usize,
        operator: usize,
        value: usize,
        tolerance: Option<f64>,
        anova: bool,
    },
    /// Type 1 gauge study: repeated measurements of one reference part.
    #[serde(rename = "msa_tipo1")]
    Type1 {
        col: usize,
        reference: f64,
        tolerance: f64,
    },
    /// Bias: repeated measurements of one reference part.
    #[serde(rename = "msa_vies")]
    Bias {
        col: usize,
        reference: f64,
        tolerance: Option<f64>,
    },
    /// Linearity: reference and measured value columns over several reference parts.
    #[serde(rename = "msa_linearidade")]
    Linearity {
        reference: usize,
        value: usize,
        variation: Option<f64>,
    },
    /// Attribute agreement: item, appraiser and response columns, and the known standard.
    #[serde(rename = "msa_atributos")]
    Attribute {
        item: usize,
        appraiser: usize,
        response: usize,
        standard: Option<usize>,
    },
    /// Two-level factorial experiment: the factor columns (coded `-1/0/+1` or actual
    /// levels), the response and the highest order of the interactions in the model.
    #[serde(rename = "experimento_fatorial")]
    Factorial {
        factors: Vec<usize>,
        response: usize,
        order: usize,
    },
    /// Equal variances: Levene (median) and Bartlett, values and group columns.
    #[serde(rename = "variancias")]
    Variances { col: usize, group: usize },
    #[serde(rename = "kruskal_wallis")]
    KruskalWallis { col: usize, group: usize },
    #[serde(rename = "mediana_mood")]
    MoodMedian { col: usize, group: usize },
    /// Friedman: value, treatment and block columns.
    #[serde(rename = "friedman")]
    Friedman {
        value: usize,
        treatment: usize,
        block: usize,
    },
    /// Two-way ANOVA: value and the columns of the two factors.
    #[serde(rename = "anova_dois_fatores")]
    TwoWayAnova { value: usize, a: usize, b: usize },
    /// Linear regression of `response` on the predictor columns.
    #[serde(rename = "regressao")]
    Regression {
        response: usize,
        predictors: Vec<usize>,
    },
    /// Sample size: `kind` 0 one mean (`a` difference, `b` deviation), 1 two means, 2 two
    /// proportions (`a`, `b`); target power and significance.
    #[serde(rename = "tamanho_amostra")]
    SampleSize {
        kind: u8,
        a: f64,
        b: f64,
        target: f64,
        alpha: f64,
    },
    /// EWMA chart of individual values: `lambda` (weight of the new point) and `l` (width of
    /// the limits in σ). The target and σ come from the data (mean and MR̄/1,128) when not
    /// given; the limits are the exact ones, narrower on the first points.
    #[serde(rename = "ewma")]
    Ewma {
        col: usize,
        lambda: f64,
        l: f64,
        target: Option<f64>,
        sigma: Option<f64>,
    },
    /// Two-sided tabular CUSUM of individual values: `k` (slack) and `h` (decision limit),
    /// both in σ; target and σ as in [`Recipe::Ewma`].
    #[serde(rename = "cusum")]
    Cusum {
        col: usize,
        k: f64,
        h: f64,
        target: Option<f64>,
        sigma: Option<f64>,
    },
    /// Response surface: the second-order model on the factor columns (coded, as in the
    /// worksheet of a central composite or a Box-Behnken), its coefficients and the
    /// stationary point.
    #[serde(rename = "superficie_resposta")]
    Surface {
        factors: Vec<usize>,
        response: usize,
    },
    /// Desirability of several responses (Derringer and Suich): a second-order model per
    /// response on the same factor columns and the setting with the best overall
    /// desirability.
    #[serde(rename = "desejabilidade")]
    Desirability {
        factors: Vec<usize>,
        responses: Vec<ResponseGoal>,
    },
}

pub use utsim_doe::Goal;

/// One response of a desirability: its column, its goal and its importance.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ResponseGoal {
    #[serde(rename = "coluna")]
    pub col: usize,
    #[serde(rename = "objetivo")]
    pub goal: utsim_doe::Goal,
    #[serde(rename = "importancia", default = "one")]
    pub importance: f64,
}

fn one() -> f64 {
    1.0
}

impl Recipe {
    pub fn label(&self) -> &'static str {
        match self {
            Recipe::Descriptive { .. } => tx("analises-estatistica_descritiva"),
            Recipe::Normality { .. } => tx("analises-normalidade_anderson_darling"),
            Recipe::Capability { .. } => tx("analises-capacidade_do_processo"),
            Recipe::Chart { .. } => tx("analises-carta_de_controle"),
            Recipe::TOne { .. } => tx("analises-teste_t_de_uma_amostra"),
            Recipe::TTwo { .. } => tx("analises-teste_t_de_duas_amostras"),
            Recipe::TPaired { .. } => tx("analises-teste_t_pareado"),
            Recipe::Anova { .. } => tx("analises-anova_de_um_fator_com"),
            Recipe::ChiSquare { .. } => tx("analises-qui_quadrado_de_independencia"),
            Recipe::Proportion { .. } => tx("analises-teste_de_uma_proporcao"),
            Recipe::TwoProportions { .. } => tx("analises-teste_de_duas_proporcoes"),
            Recipe::MannWhitney { .. } => tx("analises-mann_whitney"),
            Recipe::Wilcoxon { .. } => tx("analises-wilcoxon_pareado"),
            Recipe::GageRr { .. } => tx("analises-msa_gage_r_r"),
            Recipe::Type1 { .. } => tx("analises-msa_estudo_do_tipo_1"),
            Recipe::Bias { .. } => tx("analises-msa_vies"),
            Recipe::Linearity { .. } => tx("analises-msa_linearidade"),
            Recipe::Attribute { .. } => tx("analises-msa_concordancia_de_atributos"),
            Recipe::Factorial { .. } => tx("analises-experimento_fatorial_2_k"),
            Recipe::Variances { .. } => tx("analises-igualdade_de_variancias_levene_e"),
            Recipe::KruskalWallis { .. } => tx("analises-kruskal_wallis"),
            Recipe::MoodMedian { .. } => tx("analises-teste_da_mediana_de_mood"),
            Recipe::Friedman { .. } => tx("analises-friedman"),
            Recipe::TwoWayAnova { .. } => tx("analises-anova_de_dois_fatores"),
            Recipe::Regression { .. } => tx("analises-regressao_linear"),
            Recipe::SampleSize { .. } => tx("analises-tamanho_de_amostra_e_poder"),
            Recipe::Ewma { .. } => tx("analises-carta_ewma"),
            Recipe::Cusum { .. } => tx("analises-carta_cusum"),
            Recipe::Surface { .. } => tx("analises-superficie_de_resposta"),
            Recipe::Desirability { .. } => tx("analises-desejabilidade"),
        }
    }

    /// The key of the kind (`capacidade`, `gage_rr`...).
    pub fn key(&self) -> String {
        serde_json::to_value(self)
            .ok()
            .and_then(|v| v.get("funcao").and_then(Value::as_str).map(String::from))
            .unwrap_or_default()
    }
}

/// The result: labelled numbers, a conclusion and what to draw.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct Outcome {
    #[serde(rename = "valores")]
    pub lines: Vec<(String, f64)>,
    #[serde(rename = "conclusao")]
    pub verdict: String,
    /// Values to draw (histogram or chart points).
    #[serde(rename = "serie", default)]
    pub series: Vec<f64>,
    /// Chart: centre, lower and upper limits of the first panel.
    #[serde(rename = "limites", default)]
    pub limits: Option<(f64, f64, f64)>,
    /// Chart: indices of the points that broke a rule.
    #[serde(rename = "sinais", default)]
    pub signals: Vec<usize>,
    #[serde(rename = "avisos", default)]
    pub notes: Vec<String>,
    /// Further charts (effects of an experiment).
    #[serde(rename = "graficos", default)]
    pub plots: Vec<Plot>,
    /// Further lines on the chart of `series`, one value per point (limits that change from
    /// point to point, the other side of a CUSUM).
    #[serde(rename = "linhas", default, skip_serializing_if = "Vec::is_empty")]
    pub lines_on_chart: Vec<(String, Vec<f64>)>,
    /// Where to mark the signals, when not on `series` (point, value).
    #[serde(rename = "marcas", default, skip_serializing_if = "Vec::is_empty")]
    pub signal_marks: Vec<(usize, f64)>,
}

/// A chart of a result: bars (one series) or lines, over labelled points.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct Plot {
    #[serde(rename = "titulo")]
    pub title: String,
    #[serde(rename = "barras")]
    pub bars: bool,
    #[serde(rename = "rotulos")]
    pub labels: Vec<String>,
    #[serde(rename = "series")]
    pub series: Vec<(String, Vec<f64>)>,
}

impl Outcome {
    pub fn value(&self, label: &str) -> Option<f64> {
        self.lines.iter().find(|(l, _)| l == label).map(|(_, v)| *v)
    }

    fn put(&mut self, label: &str, v: f64) {
        self.lines.push((label.to_string(), v));
    }
}

/// The text of a data set split in columns (values as text).
#[derive(Debug, Clone, PartialEq)]
pub struct Table {
    pub header: Vec<String>,
    pub rows: Vec<Vec<String>>,
    decimal_comma: bool,
}

impl Table {
    pub fn parse(text: &str) -> Table {
        let t = utsim_quality::data::parse(text);
        let (header, _) = dados::shape(text);
        Table {
            header,
            rows: t.rows,
            decimal_comma: t.decimal_comma,
        }
    }

    pub fn numbers(&self, col: usize) -> Vec<f64> {
        self.rows
            .iter()
            .filter_map(|r| r.get(col))
            .filter_map(|c| utsim_quality::data::number(c, self.decimal_comma))
            .collect()
    }

    /// Pairs of numbers of two columns (rows where both are numbers).
    pub fn pairs(&self, a: usize, b: usize) -> (Vec<f64>, Vec<f64>) {
        let mut x = Vec::new();
        let mut y = Vec::new();
        for r in &self.rows {
            let n = |c: usize| {
                r.get(c)
                    .and_then(|s| utsim_quality::data::number(s, self.decimal_comma))
            };
            if let (Some(u), Some(v)) = (n(a), n(b)) {
                x.push(u);
                y.push(v);
            }
        }
        (x, y)
    }

    pub fn texts(&self, col: usize) -> Vec<String> {
        self.rows
            .iter()
            .map(|r| r.get(col).map(|s| s.trim().to_string()).unwrap_or_default())
            .collect()
    }
}

/// The values of `col` split by the labels of `group` (sorted labels, empty ones out).
fn grouped(t: &Table, col: usize, group: usize) -> (Vec<String>, Vec<Vec<f64>>) {
    let labels = t.texts(group);
    let mut keys: Vec<String> = labels.iter().filter(|k| !k.is_empty()).cloned().collect();
    keys.sort();
    keys.dedup();
    let mut groups: Vec<Vec<f64>> = vec![Vec::new(); keys.len()];
    for (r, lab) in t.rows.iter().zip(&labels) {
        let Some(i) = keys.iter().position(|k| k == lab) else {
            continue;
        };
        if let Some(v) = r
            .get(col)
            .and_then(|s| utsim_quality::data::number(s, t.decimal_comma))
        {
            groups[i].push(v);
        }
    }
    (keys, groups)
}

fn need(v: &[f64], n: usize, what: &str) -> Result<(), String> {
    if v.len() < n {
        Err(fx(
            "analises-sao_precisos_ao_menos_valores",
            &[("what", vx(&what)), ("n", vx(&n)), ("v", vx(&v.len()))],
        ))
    } else {
        Ok(())
    }
}

fn verdict_p(p: f64, h1: &str) -> String {
    if p < 1.0 - LEVEL {
        fx(
            "analises-valor_p_0_05_ha",
            &[("p", dx(format!("{:.4}", p))), ("h1", vx(&h1))],
        )
    } else {
        fx(
            "analises-valor_p_0_05_sem",
            &[("p", dx(format!("{:.4}", p))), ("h1", vx(&h1))],
        )
    }
}

/// The desirability analysis (see [`Recipe::Desirability`]).
fn desirability(
    t: &Table,
    factors: &[usize],
    responses: &[ResponseGoal],
    o: &mut Outcome,
) -> Result<(), String> {
    if responses.is_empty() {
        return Err(tx("analises-desejabilidade_sem_resposta").into());
    }
    let k = factors.len();
    let cols: Vec<usize> = responses.iter().map(|r| r.col).collect();
    if cols.iter().any(|c| factors.contains(c)) {
        return Err(tx("analises-desejabilidade_coluna_fator").into());
    }
    let data = surface_data(t, factors, &cols, o)?;
    let terms = utsim_doe::quadratic_terms(k);
    let models = data
        .ys
        .iter()
        .map(|y| utsim_doe::Model::fit(&terms, &data.x, y, LEVEL))
        .collect::<Result<Vec<_>, String>>()?;
    let goals: Vec<utsim_doe::Desirability> = responses
        .iter()
        .map(|r| utsim_doe::Desirability {
            importance: r.importance,
            ..utsim_doe::Desirability::new(r.goal)
        })
        .collect();
    // The search stays where the experiment ran: the sphere of the axial points of a
    // rotatable composite, the cube otherwise.
    let radius = data.x.iter().flatten().fold(0.0f64, |a, v| a.max(v.abs()));
    let region = if radius > 1.0 + 1e-9 {
        utsim_doe::Region::Sphere(radius)
    } else {
        utsim_doe::Region::Cube(1.0)
    };
    let best = utsim_doe::optimize(&models, &goals, region)?;
    let header = |c: usize| t.header.get(c).cloned().unwrap_or_default();
    o.put("desejabilidade geral", best.overall);
    for i in 0..k {
        o.put(&format!("melhor ponto {}", doe::letter(i)), best.coded[i]);
    }
    for (i, map) in data.to_real.iter().enumerate() {
        if let Some((a, b)) = map {
            o.put(
                &format!("melhor ponto {} (real)", data.names[i]),
                a + b * best.coded[i],
            );
        }
    }
    for (j, r) in responses.iter().enumerate() {
        let h = header(r.col);
        o.put(&format!("prevista {h}"), best.responses[j]);
        o.put(&format!("desejabilidade de {h}"), best.desirabilities[j]);
        o.put(&format!("R² de {h}"), models[j].regression.r2);
    }
    o.verdict = if best.overall > 0.0 {
        fx(
            "analises-desejabilidade_achou",
            &[("d", dx(format!("{:.3}", best.overall)))],
        )
    } else {
        tx("analises-desejabilidade_nenhum").into()
    };
    o.notes.push(tx("analises-desejabilidade_nota").into());
    let mut labels: Vec<String> = responses.iter().map(|r| header(r.col)).collect();
    labels.push(tx("analises-desejabilidade_geral").into());
    let mut values = best.desirabilities.clone();
    values.push(best.overall);
    o.plots.push(Plot {
        title: tx("analises-desejabilidade_grafico").into(),
        bars: true,
        labels,
        series: vec![("d".into(), values)],
    });
    Ok(())
}

/// The coded factor columns of a response surface and its responses (one list per
/// response), with each factor's actual units (`real = a + b · coded`) and name.
struct SurfaceData {
    x: Vec<Vec<f64>>,
    ys: Vec<Vec<f64>>,
    to_real: Vec<Option<(f64, f64)>>,
    names: Vec<String>,
}

/// Reads the rows with every factor and response filled, codes the factor columns that are
/// not coded yet, and finds the actual levels next to the coded ones (a worksheet of the app).
fn surface_data(
    t: &Table,
    factors: &[usize],
    responses: &[usize],
    o: &mut Outcome,
) -> Result<SurfaceData, String> {
    let k = factors.len();
    if !(2..=utsim_doe::MAX_CCD_FACTORS).contains(&k) {
        return Err(fx(
            "analises-superficie_fatores",
            &[("n", vx(&utsim_doe::MAX_CCD_FACTORS))],
        ));
    }
    let num = |r: &Vec<String>, c: usize| {
        r.get(c)
            .and_then(|s| utsim_quality::data::number(s, t.decimal_comma))
    };
    let header = |c: usize| t.header.get(c).cloned().unwrap_or_default();
    // In a worksheet of the app the coded A, B... are followed by the actual levels.
    let real_col = |i: usize| {
        let col = factors[k - 1] + 1 + i;
        (header(factors[i]) == doe::letter(i).to_string()
            && !responses.contains(&col)
            && !factors.contains(&col)
            && col < t.header.len())
        .then_some(col)
    };
    let mut x: Vec<Vec<f64>> = Vec::new();
    let mut ys: Vec<Vec<f64>> = vec![Vec::new(); responses.len()];
    let mut real: Vec<Vec<Option<f64>>> = Vec::new();
    for r in &t.rows {
        let (Some(v), Some(xs)) = (
            responses
                .iter()
                .map(|c| num(r, *c))
                .collect::<Option<Vec<f64>>>(),
            factors
                .iter()
                .map(|c| num(r, *c))
                .collect::<Option<Vec<f64>>>(),
        ) else {
            continue;
        };
        for (y, v) in ys.iter_mut().zip(v) {
            y.push(v);
        }
        x.push(xs);
        real.push(
            (0..k)
                .map(|i| real_col(i).and_then(|c| num(r, c)))
                .collect(),
        );
    }
    let terms = utsim_doe::quadratic_terms(k);
    if x.len() < terms.len() + 2 {
        return Err(fx(
            "analises-superficie_corridas",
            &[("k", vx(&k)), ("n", vx(&(terms.len() + 2)))],
        ));
    }
    // Coded columns are symmetric around zero; other columns are coded by their extremes.
    for i in 0..k {
        let col: Vec<f64> = x.iter().map(|r| r[i]).collect();
        let (lo, hi) = col
            .iter()
            .fold((f64::INFINITY, f64::NEG_INFINITY), |(a, b), v| {
                (a.min(*v), b.max(*v))
            });
        if hi <= 3.0 && (lo + hi).abs() < 1e-6 && hi > 0.0 {
            continue;
        }
        let coded = doe::code(&col)
            .ok_or_else(|| fx("analises-o_fator_tem_um_so", &[("i", vx(&doe::letter(i)))]))?;
        for (r, v) in x.iter_mut().zip(coded) {
            r[i] = v;
        }
        o.notes.push(fx(
            "analises-superficie_codificada",
            &[("x", vx(&doe::letter(i)))],
        ));
    }
    // The actual units of each factor, by a straight line from the coded column.
    let to_real: Vec<Option<(f64, f64)>> = (0..k)
        .map(|i| {
            let pts: Vec<(f64, f64)> = x
                .iter()
                .zip(&real)
                .filter_map(|(r, re)| re[i].map(|v| (r[i], v)))
                .collect();
            if pts.len() < 2 {
                return None;
            }
            let n = pts.len() as f64;
            let mx = pts.iter().map(|p| p.0).sum::<f64>() / n;
            let my = pts.iter().map(|p| p.1).sum::<f64>() / n;
            let sxx: f64 = pts.iter().map(|p| (p.0 - mx).powi(2)).sum();
            if sxx <= 0.0 {
                return None;
            }
            let b = pts.iter().map(|p| (p.0 - mx) * (p.1 - my)).sum::<f64>() / sxx;
            // Actual levels are an exact line of the coded ones (up to the rounding of the
            // worksheet); any other column next to the factors is not.
            let a0 = my - b * mx;
            let span = pts.iter().map(|p| p.1.abs()).fold(0.0f64, f64::max) + 1.0;
            if pts.iter().any(|p| (a0 + b * p.0 - p.1).abs() > 1e-4 * span) {
                return None;
            }
            Some((my - b * mx, b))
        })
        .collect();
    let name = |i: usize| match real_col(i) {
        Some(c) if to_real[i].is_some() => header(c),
        _ => header(factors[i]),
    };
    if (0..k).any(|i| name(i) != doe::letter(i).to_string()) {
        o.notes.push(
            (0..k)
                .map(|i| format!("{} = {}", doe::letter(i), name(i)))
                .collect::<Vec<_>>()
                .join("; "),
        );
    }
    let names = (0..k).map(name).collect();
    Ok(SurfaceData {
        x,
        ys,
        to_real,
        names,
    })
}

/// The response surface analysis (see [`Recipe::Surface`]).
fn surface(t: &Table, factors: &[usize], response: usize, o: &mut Outcome) -> Result<(), String> {
    let k = factors.len();
    let data = surface_data(t, factors, &[response], o)?;
    let terms = utsim_doe::quadratic_terms(k);
    let x = &data.x;
    let y = &data.ys[0];
    let to_real = &data.to_real;
    let name = |i: usize| data.names[i].clone();

    let m = utsim_doe::Model::fit(&terms, x, y, LEVEL)?;
    let reg = &m.regression;
    o.put("corridas", y.len() as f64);
    o.put("R²", reg.r2);
    o.put("R² ajustado", reg.r2_adj);
    if reg.f_p_value.is_finite() {
        o.put("valor-p do modelo", reg.f_p_value);
    }
    for c in &reg.coefs {
        o.put(&format!("coef {}", c.name), c.estimate);
        if c.name != "constante" && c.p_value.is_finite() {
            o.put(&format!("valor-p {}", c.name), c.p_value);
        }
    }
    let radius = x.iter().flatten().fold(0.0f64, |a, v| a.max(v.abs()));
    match m.stationary() {
        Some(s) => {
            for i in 0..k {
                o.put(
                    &format!("ponto estacionário {}", doe::letter(i)),
                    s.coded[i],
                );
            }
            for (i, map) in to_real.iter().enumerate() {
                if let Some((a, b)) = map {
                    o.put(
                        &format!("ponto estacionário {} (real)", name(i)),
                        a + b * s.coded[i],
                    );
                }
            }
            o.put("resposta prevista no ponto estacionário", s.predicted);
            let dist = s.coded.iter().map(|v| v * v).sum::<f64>().sqrt();
            let what = match s.nature {
                utsim_doe::Nature::Maximum => tx("analises-superficie_maximo"),
                utsim_doe::Nature::Minimum => tx("analises-superficie_minimo"),
                utsim_doe::Nature::Saddle => tx("analises-superficie_sela"),
                utsim_doe::Nature::Ridge => tx("analises-superficie_crista"),
            };
            o.verdict = if dist > radius + 1e-9 {
                fx("analises-superficie_fora", &[("w", vx(&what))])
            } else {
                what.to_string()
            };
            o.notes.push(fx(
                "analises-superficie_autovalores",
                &[(
                    "v",
                    vx(&s
                        .eigenvalues
                        .iter()
                        .map(|v| format!("{v:.4}"))
                        .collect::<Vec<_>>()
                        .join("; ")),
                )],
            ));
        }
        None => {
            o.verdict = tx("analises-superficie_sem_ponto").into();
        }
    }
    if reg.df_error < 1.0 {
        o.notes.push(tx("analises-superficie_sem_erro").into());
    }
    let coefs = &reg.coefs[1..];
    o.plots.push(Plot {
        title: tx("analises-superficie_termos").into(),
        bars: true,
        labels: coefs.iter().map(|c| c.name.clone()).collect(),
        series: vec![(
            "|t|".into(),
            coefs
                .iter()
                .map(|c| if c.t.is_finite() { c.t.abs() } else { 0.0 })
                .collect(),
        )],
    });
    let steps: Vec<f64> = (0..=10)
        .map(|j| -radius + 2.0 * radius * j as f64 / 10.0)
        .collect();
    o.plots.push(Plot {
        title: tx("analises-superficie_por_fator").into(),
        bars: false,
        labels: steps.iter().map(|v| format!("{v:.2}")).collect(),
        series: (0..k)
            .map(|i| {
                (
                    doe::letter(i).to_string(),
                    steps
                        .iter()
                        .map(|v| {
                            let mut p = vec![0.0; k];
                            p[i] = *v;
                            m.predict(&p)
                        })
                        .collect(),
                )
            })
            .collect(),
    });
    Ok(())
}

/// The note when the target or σ of a chart with memory came from the data.
fn base_note() -> &'static str {
    tx("analises-alvo_e_calculados_dos_proprios")
}

/// Target and σ of a chart with memory: the given ones, or the mean and MR̄/d2 of the data.
fn memory_base(x: &[f64], target: Option<f64>, sigma: Option<f64>) -> Result<(f64, f64), String> {
    need(x, 2, "carta")?;
    let mu = target.unwrap_or_else(|| x.iter().sum::<f64>() / x.len() as f64);
    let sd = match sigma {
        Some(s) => s,
        None => {
            let mr: f64 =
                x.windows(2).map(|w| (w[1] - w[0]).abs()).sum::<f64>() / (x.len() - 1) as f64;
            mr / 1.128
        }
    };
    if !(sd > 0.0 && sd.is_finite()) {
        return Err(tx("analises-precisa_ser_maior_que_zero").into());
    }
    Ok((mu, sd))
}

/// The ARL of a scheme after a shift of `shift` σ, by simulation with a fixed seed.
fn arl_of(scheme: &arl::Scheme, shift: f64) -> f64 {
    arl::simulate(scheme, shift, 20_000, 7, 100_000).arl
}

fn memory_verdict(first: Option<usize>, n: usize) -> String {
    match first {
        None => tx("analises-nenhum_sinal_a_media_nao").into(),
        Some(i) => fx(
            "analises-ponto_s_com_sinal_o",
            &[("n", vx(&n)), ("i", vx(&(i + 1)))],
        ),
    }
}

/// Runs a recipe on a table.
pub fn run(recipe: &Recipe, t: &Table) -> Result<Outcome, String> {
    let mut o = Outcome::default();
    match recipe {
        Recipe::Descriptive { col } => {
            let x = t.numbers(*col);
            need(&x, 1, "descritiva")?;
            let d = descriptive::describe(&x, LEVEL, QuantileMethod::default())
                .ok_or(tx("analises-sem_valores"))?;
            for (l, v) in [
                ("n", d.n as f64),
                ("média", d.mean),
                ("desvio-padrão", d.sd),
                ("mínimo", d.min),
                ("Q1", d.q1),
                ("mediana", d.median),
                ("Q3", d.q3),
                ("máximo", d.max),
                ("assimetria", d.skewness),
                ("curtose", d.kurtosis),
            ] {
                o.put(l, v);
            }
            if let Some(ci) = d.mean_ci {
                o.put("IC 95 % da média (inferior)", ci.low);
                o.put("IC 95 % da média (superior)", ci.high);
            }
            o.verdict = fx(
                "analises-media_e_desvio_de_valores",
                &[
                    ("mean", dx(format!("{:.4}", d.mean))),
                    ("sd", dx(format!("{:.4}", d.sd))),
                    ("n", vx(&d.n)),
                ],
            );
            o.series = x;
        }
        Recipe::Normality { col } => {
            let x = t.numbers(*col);
            need(&x, 3, "normalidade")?;
            let a = utsim_quality::normality::anderson_darling(&x)
                .ok_or(tx("analises-sem_variacao"))?;
            o.put("n", a.n as f64);
            o.put("A²", a.a2);
            o.put("A*²", a.a2_star);
            o.put("valor-p", a.p_value);
            o.verdict = if a.p_value < 0.05 {
                fx(
                    "analises-valor_p_0_05_os",
                    &[("p_value", dx(format!("{:.4}", a.p_value)))],
                )
            } else {
                fx(
                    "analises-valor_p_0_05_sem-2",
                    &[("p_value", dx(format!("{:.4}", a.p_value)))],
                )
            };
            o.series = x;
        }
        Recipe::Capability {
            col,
            lsl,
            usl,
            target,
            subgroup,
        } => {
            let x = t.numbers(*col);
            need(&x, 2, "capacidade")?;
            let size = (*subgroup).max(1);
            let groups: Vec<Vec<f64>> = x
                .chunks(size)
                .filter(|c| c.len() == size)
                .map(<[f64]>::to_vec)
                .collect();
            let spec = utsim_quality::capability::Spec {
                lsl: *lsl,
                usl: *usl,
                target: *target,
            };
            let c = utsim_quality::capability::capability(
                &groups,
                &spec,
                &utsim_quality::capability::Options::default(),
            )?;
            o.put("n", c.n as f64);
            o.put("média", c.mean);
            o.put("sigma de curto prazo", c.sigma_within);
            o.put("sigma global", c.sigma_overall);
            if let Some(cp) = &c.cp {
                o.put("Cp", cp.value);
            }
            o.put("Cpk", c.cpk.value);
            o.put("Cpk IC inferior", c.cpk.lo);
            if let Some(pp) = &c.pp {
                o.put("Pp", pp.value);
            }
            o.put("Ppk", c.ppk.value);
            o.put("PPM total (global)", c.ppm_overall.total);
            o.put("nível sigma (com deslocamento de 1,5)", c.sigma_level);
            if let Some(n) = &c.normality {
                o.put("valor-p A-D", n.p_value);
            }
            o.verdict = if c.cpk.value >= 1.33 {
                fx(
                    "analises-cpk_1_33_processo_capaz",
                    &[("value", dx(format!("{:.2}", c.cpk.value)))],
                )
            } else if c.cpk.value >= 1.0 {
                fx(
                    "analises-cpk_entre_1_e_1",
                    &[("value", dx(format!("{:.2}", c.cpk.value)))],
                )
            } else {
                fx(
                    "analises-cpk_1_processo_nao_capaz",
                    &[("value", dx(format!("{:.2}", c.cpk.value)))],
                )
            };
            o.notes = c.notes.clone();
            o.series = x;
        }
        Recipe::Chart {
            kind,
            col,
            subgroup,
            size_col,
            nelson,
        } => {
            let data = if kind.is_attribute() {
                let counts = t.numbers(*col);
                let sizes = match size_col {
                    Some(s) => t.numbers(*s),
                    None => vec![1.0; counts.len()],
                };
                if counts.len() != sizes.len() {
                    return Err(tx("analises-contagens_e_tamanhos_com_numeros").into());
                }
                ChartData::Attributes { counts, sizes }
            } else {
                let x = t.numbers(*col);
                let size = if *kind == ChartKind::IMr {
                    1
                } else {
                    (*subgroup).max(2)
                };
                ChartData::Variables(
                    x.chunks(size)
                        .filter(|c| c.len() == size)
                        .map(<[f64]>::to_vec)
                        .collect(),
                )
            };
            let rules = if *nelson {
                RuleSet::nelson()
            } else {
                RuleSet::western_electric()
            };
            let ch = charts::phase1(*kind, &data, &[], &rules)?;
            let p = ch.panels.first().ok_or(tx("analises-carta_sem_painel"))?;
            o.series = p.values.iter().map(|v| v.unwrap_or(f64::NAN)).collect();
            o.limits = Some((
                p.center.first().copied().unwrap_or(f64::NAN),
                p.lcl.first().copied().unwrap_or(f64::NAN),
                p.ucl.first().copied().unwrap_or(f64::NAN),
            ));
            let mut sig: Vec<usize> = ch
                .panels
                .iter()
                .flat_map(|p| p.violations.iter().map(|v| v.index))
                .collect();
            sig.sort_unstable();
            sig.dedup();
            o.put("subgrupos", data.len() as f64);
            if let Some((c, l, u)) = o.limits {
                o.put("linha central", c);
                o.put("LIC", l);
                o.put("LSC", u);
            }
            o.put("pontos com sinal", sig.len() as f64);
            o.signals = sig;
            o.verdict = if ch.in_control() {
                tx("analises-nenhuma_regra_violada_processo_sob").into()
            } else {
                fx(
                    "analises-ponto_s_com_sinal_ha",
                    &[("signals", vx(&o.signals.len()))],
                )
            };
            o.notes = ch.notes.clone();
        }
        Recipe::TOne { col, mu0 } => {
            let x = t.numbers(*col);
            need(&x, 2, tx("analises-teste_t"))?;
            let r = hypothesis::t_one_sample(&x, *mu0, LEVEL, Alternative::TwoSided)
                .ok_or(tx("analises-dados_insuficientes"))?;
            o.put("média", r.estimate);
            o.put("t", r.t);
            o.put("gl", r.df);
            o.put("valor-p", r.p_value);
            o.put("IC inferior", r.ci.low);
            o.put("IC superior", r.ci.high);
            o.verdict = verdict_p(
                r.p_value,
                &fx("analises-a_media_difere_de", &[("mu0", vx(&mu0))]),
            );
        }
        Recipe::TTwo { a, b, pooled } => {
            let (x, y) = (t.numbers(*a), t.numbers(*b));
            need(&x, 2, tx("analises-amostra_1"))?;
            need(&y, 2, tx("analises-amostra_2"))?;
            let r = hypothesis::t_two_sample(&x, &y, 0.0, *pooled, LEVEL, Alternative::TwoSided)
                .ok_or(tx("analises-dados_insuficientes"))?;
            o.put("diferença das médias", r.estimate);
            o.put("t", r.t);
            o.put("gl", r.df);
            o.put("valor-p", r.p_value);
            o.put("IC inferior", r.ci.low);
            o.put("IC superior", r.ci.high);
            o.verdict = verdict_p(r.p_value, tx("analises-as_medias_diferem"));
        }
        Recipe::TPaired { a, b } => {
            let (x, y) = t.pairs(*a, *b);
            need(&x, 2, "pares")?;
            let r = hypothesis::t_paired(&x, &y, 0.0, LEVEL, Alternative::TwoSided)
                .ok_or(tx("analises-dados_insuficientes"))?;
            o.put("diferença média", r.estimate);
            o.put("t", r.t);
            o.put("valor-p", r.p_value);
            o.put("IC inferior", r.ci.low);
            o.put("IC superior", r.ci.high);
            o.verdict = verdict_p(r.p_value, tx("analises-ha_diferenca_entre_os_pares"));
        }
        Recipe::Anova { col, group } => {
            let labels = t.texts(*group);
            let mut keys: Vec<String> = labels.clone();
            keys.sort();
            keys.dedup();
            keys.retain(|k| !k.is_empty());
            let mut groups: Vec<Vec<f64>> = vec![Vec::new(); keys.len()];
            for (r, lab) in t.rows.iter().zip(&labels) {
                let Some(i) = keys.iter().position(|k| k == lab) else {
                    continue;
                };
                if let Some(v) = r
                    .get(*col)
                    .and_then(|s| utsim_quality::data::number(s, t.decimal_comma))
                {
                    groups[i].push(v);
                }
            }
            let refs: Vec<&[f64]> = groups.iter().map(Vec::as_slice).collect();
            let r = hypothesis::one_way_anova(&refs, LEVEL)
                .ok_or(tx("analises-sao_precisos_2_grupos_com"))?;
            o.put("grupos", r.anova.groups as f64);
            o.put("F", r.anova.f);
            o.put("valor-p", r.anova.p_value);
            o.put("R²", r.r_squared);
            for (k, m) in keys.iter().zip(&r.anova.means) {
                o.put(&format!("média de {k}"), *m);
            }
            o.verdict = verdict_p(r.anova.p_value, tx("analises-alguma_media_de_grupo_difere"));
        }
        Recipe::ChiSquare { a, b } => {
            let (ra, rb) = (t.texts(*a), t.texts(*b));
            let mut ka: Vec<String> = ra.clone();
            ka.sort();
            ka.dedup();
            ka.retain(|k| !k.is_empty());
            let mut kb: Vec<String> = rb.clone();
            kb.sort();
            kb.dedup();
            kb.retain(|k| !k.is_empty());
            let mut table = vec![vec![0.0; kb.len()]; ka.len()];
            for (x, y) in ra.iter().zip(&rb) {
                if let (Some(i), Some(j)) = (
                    ka.iter().position(|k| k == x),
                    kb.iter().position(|k| k == y),
                ) {
                    table[i][j] += 1.0;
                }
            }
            let r = hypothesis::chi_square_independence(&table)
                .ok_or(tx("analises-tabela_insuficiente"))?;
            o.put("X²", r.statistic);
            o.put("gl", r.df as f64);
            o.put("valor-p", r.p_value);
            o.verdict = verdict_p(r.p_value, tx("analises-as_duas_variaveis_estao_associadas"));
        }
        Recipe::Proportion { x, n, p0 } => {
            let r = hypothesis::one_proportion(
                *x,
                *n,
                *p0,
                Default::default(),
                LEVEL,
                Alternative::TwoSided,
            )
            .ok_or(tx("analises-proporcao_invalida_0_p0_1"))?;
            o.put("p", r.p_hat);
            o.put("valor-p", r.p_value);
            o.put("IC inferior", r.ci.low);
            o.put("IC superior", r.ci.high);
            o.verdict = verdict_p(
                r.p_value,
                &fx("analises-a_proporcao_difere_de", &[("p0", vx(&p0))]),
            );
        }
        Recipe::TwoProportions { x1, n1, x2, n2 } => {
            let r = hypothesis::two_proportions(*x1, *n1, *x2, *n2, LEVEL, Alternative::TwoSided)
                .ok_or(tx("analises-proporcoes_invalidas"))?;
            o.put("p1", r.p_hat[0]);
            o.put("p2", r.p_hat[1]);
            o.put("diferença", r.difference);
            o.put("valor-p", r.p_value);
            o.verdict = verdict_p(r.p_value, tx("analises-as_proporcoes_diferem"));
        }
        Recipe::MannWhitney { a, b } => {
            let (x, y) = (t.numbers(*a), t.numbers(*b));
            need(&x, 1, tx("analises-amostra_1"))?;
            need(&y, 1, tx("analises-amostra_2"))?;
            let r =
                nonparametric::mann_whitney(&x, &y, RankMethod::default(), Alternative::TwoSided)
                    .ok_or(tx("analises-dados_insuficientes"))?;
            o.put("U1", r.u1);
            o.put("valor-p", r.p_value);
            o.put("deslocamento (Hodges-Lehmann)", r.shift);
            o.verdict = verdict_p(
                r.p_value,
                tx("analises-as_distribuicoes_diferem_em_posicao"),
            );
        }
        Recipe::Wilcoxon { a, b } => {
            let (x, y) = t.pairs(*a, *b);
            need(&x, 1, "pares")?;
            let r = nonparametric::wilcoxon_paired(
                &x,
                &y,
                0.0,
                RankMethod::default(),
                Alternative::TwoSided,
            )
            .ok_or(tx("analises-sem_diferencas_diferentes_de_zero"))?;
            o.put("W+", r.w_plus);
            o.put("valor-p", r.p_value);
            o.put("pseudomediana", r.pseudo_median);
            o.verdict = verdict_p(r.p_value, tx("analises-ha_diferenca_entre_os_pares"));
        }
        Recipe::GageRr {
            part,
            operator,
            value,
            tolerance,
            anova,
        } => {
            let ms: Vec<Measurement> = t
                .rows
                .iter()
                .filter_map(|r| {
                    Some(Measurement {
                        part: r.get(*part)?.trim().to_string(),
                        operator: r.get(*operator)?.trim().to_string(),
                        value: utsim_quality::data::number(r.get(*value)?, t.decimal_comma)?,
                    })
                })
                .collect();
            let data = GageData::from_measurements(&ms)?;
            let opts = GageOptions {
                tolerance: *tolerance,
                ..GageOptions::default()
            };
            let g = if *anova {
                msa::gage_anova(&data, &opts)?
            } else {
                msa::gage_average_range(&data, &opts)?
            };
            for c in &g.components {
                o.put(&format!("%VE {}", c.name), c.pct_study_var);
                if let Some(pt) = c.pct_tolerance {
                    o.put(&format!("%tolerância {}", c.name), pt);
                }
            }
            o.put("ndc", g.ndc as f64);
            o.verdict = fx(
                "analises-ndc",
                &[("verdict", vx(&g.verdict())), ("ndc", vx(&g.ndc))],
            );
            o.notes = g.warnings.clone();
        }
        Recipe::Type1 {
            col,
            reference,
            tolerance,
        } => {
            let x = t.numbers(*col);
            need(&x, 10, tx("analises-estudo_do_tipo_1"))?;
            let r = type1(&x, *reference, *tolerance)?;
            o.put("n", x.len() as f64);
            o.put("média", r.mean);
            o.put("desvio", r.sd);
            o.put("tendência", r.bias);
            o.put("Cg", r.cg);
            o.put("Cgk", r.cgk);
            o.verdict = if r.cg >= 1.33 && r.cgk >= 1.33 {
                fx(
                    "analises-cg_e_cgk_1_33",
                    &[
                        ("cg", dx(format!("{:.2}", r.cg))),
                        ("cgk", dx(format!("{:.2}", r.cgk))),
                    ],
                )
            } else {
                fx(
                    "analises-cg_ou_cgk_1_33",
                    &[
                        ("cg", dx(format!("{:.2}", r.cg))),
                        ("cgk", dx(format!("{:.2}", r.cgk))),
                    ],
                )
            };
            o.series = x;
        }
        Recipe::Bias {
            col,
            reference,
            tolerance,
        } => {
            let x = t.numbers(*col);
            need(&x, 10, tx("analises-estudo_de_vies"))?;
            let b = msa::bias(*reference, &x, LEVEL)?;
            o.put("n", b.n as f64);
            o.put("média", b.mean);
            o.put("viés", b.bias);
            o.put("desvio", b.sd);
            o.put("t", b.t);
            o.put("valor-p", b.p_value);
            o.put("IC inferior", b.ci.0);
            o.put("IC superior", b.ci.1);
            if let Some(tol) = tolerance.filter(|t| *t > 0.0) {
                o.put("%viés da tolerância", 100.0 * b.bias.abs() / tol);
            }
            o.verdict = if b.p_value < ALPHA {
                fx(
                    "analises-vies_de_significativo_valor_p",
                    &[
                        ("bias", dx(format!("{:.4}", b.bias))),
                        ("p_value", dx(format!("{:.4}", b.p_value))),
                    ],
                )
            } else {
                fx(
                    "analises-vies_nao_significativo_valor_p",
                    &[("p_value", dx(format!("{:.4}", b.p_value)))],
                )
            };
            o.series = x;
        }
        Recipe::Linearity {
            reference,
            value,
            variation,
        } => {
            let (r, v) = t.pairs(*reference, *value);
            let pairs: Vec<(f64, f64)> = r.into_iter().zip(v).collect();
            let l = msa::linearity(&pairs, *variation, LEVEL)?;
            o.put("n", l.n as f64);
            o.put("inclinação", l.slope);
            o.put("intercepto", l.intercept);
            o.put("valor-p da inclinação", l.p_slope);
            o.put("valor-p do intercepto", l.p_intercept);
            o.put("R²", l.r2);
            o.put("%linearidade", l.pct_linearity);
            if let Some(lin) = l.linearity {
                o.put("linearidade", lin);
            }
            for b in &l.per_reference {
                o.put(&format!("viés em {}", b.reference), b.bias);
            }
            o.verdict = if l.p_slope < ALPHA {
                fx(
                    "analises-o_vies_muda_ao_longo",
                    &[
                        ("slope", dx(format!("{:.4}", l.slope))),
                        ("p_slope", dx(format!("{:.4}", l.p_slope))),
                    ],
                )
            } else if l.p_intercept < ALPHA {
                fx(
                    "analises-vies_constante_ao_longo_da",
                    &[("p_intercept", dx(format!("{:.4}", l.p_intercept)))],
                )
            } else {
                tx("analises-sem_vies_nem_problema_de").to_string()
            };
            o.series = l.per_reference.iter().map(|b| b.bias).collect();
        }
        Recipe::Attribute {
            item,
            appraiser,
            response,
            standard,
        } => {
            let ratings: Vec<Rating> = t
                .rows
                .iter()
                .filter_map(|row| {
                    let cell = |c: usize| row.get(c).map(|s| s.trim().to_string());
                    Some(Rating {
                        item: cell(*item)?,
                        appraiser: cell(*appraiser)?,
                        response: cell(*response)?,
                        standard: standard.and_then(cell).filter(|s| !s.is_empty()),
                    })
                })
                .filter(|r| !r.item.is_empty() && !r.response.is_empty())
                .collect();
            let a = attribute::attribute_agreement(&ratings, LEVEL)?;
            o.put("itens", a.items as f64);
            o.put("avaliadores", a.appraisers.len() as f64);
            o.put("repetições", a.trials as f64);
            o.put("% entre avaliadores", a.between.percent);
            if let Some(s) = &a.all_vs_standard {
                o.put("% todos contra o padrão", s.percent);
            }
            let kappa = a.fleiss_all.as_ref().map(|f| f.overall.kappa);
            if let Some(k) = kappa {
                o.put(KAPPA, k);
            }
            for r in &a.appraisers {
                if let Some(w) = &r.within {
                    o.put(&format!("% dentro de {}", r.appraiser), w.percent);
                }
                if let Some(s) = &r.vs_standard {
                    o.put(&format!("% {} contra o padrão", r.appraiser), s.percent);
                }
            }
            o.verdict = match kappa {
                Some(k) if k >= KAPPA_LIMIT => fx(
                    "analises-kappa_0_75_concordancia_boa",
                    &[("k", dx(format!("{:.2}", k)))],
                ),
                Some(k) if k >= 0.4 => fx(
                    "analises-kappa_entre_0_40_e",
                    &[("k", dx(format!("{:.2}", k)))],
                ),
                Some(k) => fx(
                    "analises-kappa_0_40_concordancia_fraca",
                    &[("k", dx(format!("{:.2}", k)))],
                ),
                None => tx("analises-sem_kappa_precisa_de_ao").to_string(),
            };
            o.notes = a.warnings.clone();
        }
        Recipe::Variances { col, group } => {
            let (keys, groups) = grouped(t, *col, *group);
            let refs: Vec<&[f64]> = groups.iter().map(Vec::as_slice).collect();
            let l = variances::levene(&refs).ok_or(tx("analises-sao_precisos_2_grupos_com-2"))?;
            o.put("Levene (mediana)", l.statistic);
            o.put("valor-p Levene", l.p_value);
            if let Some(b) = variances::bartlett(&refs) {
                o.put("Bartlett", b.statistic);
                o.put("valor-p Bartlett", b.p_value);
            }
            for (k, s) in keys.iter().zip(&l.sds) {
                o.put(&format!("desvio de {k}"), *s);
            }
            o.verdict = verdict_p(l.p_value, tx("analises-as_variancias_dos_grupos_diferem"));
            o.notes
                .push(tx("analises-levene_com_a_mediana_serve").into());
        }
        Recipe::KruskalWallis { col, group } => {
            let (keys, groups) = grouped(t, *col, *group);
            let refs: Vec<&[f64]> = groups.iter().map(Vec::as_slice).collect();
            let r =
                ranks::kruskal_wallis(&refs).ok_or(tx("analises-sao_precisos_2_grupos_com-3"))?;
            o.put("H", r.statistic);
            o.put("gl", r.df);
            o.put("valor-p", r.p_value);
            for (k, m) in keys.iter().zip(&r.per_group) {
                o.put(&format!("posto médio de {k}"), *m);
            }
            o.verdict = verdict_p(r.p_value, tx("analises-alguma_mediana_de_grupo_difere"));
        }
        Recipe::MoodMedian { col, group } => {
            let (keys, groups) = grouped(t, *col, *group);
            let refs: Vec<&[f64]> = groups.iter().map(Vec::as_slice).collect();
            let r = ranks::mood_median(&refs).ok_or(tx("analises-sao_precisos_2_grupos_e"))?;
            o.put("χ²", r.statistic);
            o.put("gl", r.df);
            o.put("valor-p", r.p_value);
            for (k, m) in keys.iter().zip(&r.per_group) {
                o.put(&format!("mediana de {k}"), *m);
            }
            o.verdict = verdict_p(r.p_value, tx("analises-alguma_mediana_de_grupo_difere"));
        }
        Recipe::Friedman {
            value,
            treatment,
            block,
        } => {
            let tr = t.texts(*treatment);
            let bl = t.texts(*block);
            let mut kt: Vec<String> = tr.iter().filter(|s| !s.is_empty()).cloned().collect();
            kt.sort();
            kt.dedup();
            let mut kb: Vec<String> = bl.iter().filter(|s| !s.is_empty()).cloned().collect();
            kb.sort();
            kb.dedup();
            let mut table = vec![vec![f64::NAN; kt.len()]; kb.len()];
            for (r, (a, b)) in t.rows.iter().zip(tr.iter().zip(&bl)) {
                let (Some(i), Some(j)) = (
                    kb.iter().position(|k| k == b),
                    kt.iter().position(|k| k == a),
                ) else {
                    continue;
                };
                if let Some(v) = r
                    .get(*value)
                    .and_then(|s| utsim_quality::data::number(s, t.decimal_comma))
                {
                    table[i][j] = v;
                }
            }
            if table.iter().flatten().any(|v| v.is_nan()) {
                return Err(tx("analises-cada_bloco_precisa_de_um").into());
            }
            let r = ranks::friedman(&table).ok_or(tx("analises-sao_precisos_2_blocos_e"))?;
            o.put("Q", r.statistic);
            o.put("gl", r.df);
            o.put("valor-p", r.p_value);
            for (k, m) in kt.iter().zip(&r.per_group) {
                o.put(&format!("posto médio de {k}"), *m);
            }
            o.verdict = verdict_p(r.p_value, tx("analises-algum_tratamento_difere"));
        }
        Recipe::TwoWayAnova { value, a, b } => {
            let (ta, tb) = (t.texts(*a), t.texts(*b));
            let obs: Vec<(String, String, f64)> = t
                .rows
                .iter()
                .zip(ta.into_iter().zip(tb))
                .filter_map(|(r, (x, y))| {
                    let v = r
                        .get(*value)
                        .and_then(|s| utsim_quality::data::number(s, t.decimal_comma))?;
                    (!x.is_empty() && !y.is_empty()).then_some((x, y, v))
                })
                .collect();
            let r = anova2::two_way(&obs)?;
            let ha = t.header.get(*a).cloned().unwrap_or_else(|| "A".into());
            let hb = t.header.get(*b).cloned().unwrap_or_else(|| "B".into());
            let name = |s: &str| match s {
                "A" => ha.clone(),
                "B" => hb.clone(),
                "A×B" => format!("{ha} × {hb}"),
                x => x.to_string(),
            };
            for l in &r.lines {
                o.put(&format!("SQ {}", name(l.source)), l.ss);
                if let Some(p) = l.p_value {
                    o.put(&format!("valor-p {}", name(l.source)), p);
                }
            }
            let active: Vec<String> = r
                .lines
                .iter()
                .filter(|l| l.p_value.is_some_and(|p| p < ALPHA))
                .map(|l| name(l.source))
                .collect();
            o.verdict = if active.is_empty() {
                tx("analises-nenhum_efeito_significativo_a_5").into()
            } else {
                fx(
                    "analises-efeitos_significativos_a_5",
                    &[("active", vx(&active.join(", ")))],
                )
            };
            if r.replicates == 1 {
                o.notes
                    .push(tx("analises-sem_repeticoes_a_interacao_vira").into());
            }
            o.plots.push(Plot {
                title: fx("analises-interacao", &[("ha", vx(&ha)), ("hb", vx(&hb))]),
                bars: false,
                labels: r.levels_a.clone(),
                series: r
                    .levels_b
                    .iter()
                    .enumerate()
                    .map(|(j, lb)| {
                        (
                            format!("{hb} = {lb}"),
                            r.cells.iter().map(|row| row[j]).collect(),
                        )
                    })
                    .collect(),
            });
        }
        Recipe::Regression {
            response,
            predictors,
        } => {
            if predictors.is_empty() {
                return Err(tx("analises-escolha_ao_menos_um_preditor").into());
            }
            let mut x: Vec<Vec<f64>> = vec![Vec::new(); predictors.len()];
            let mut y = Vec::new();
            for r in &t.rows {
                let num = |c: usize| {
                    r.get(c)
                        .and_then(|s| utsim_quality::data::number(s, t.decimal_comma))
                };
                let (Some(v), Some(xs)) = (
                    num(*response),
                    predictors
                        .iter()
                        .map(|c| num(*c))
                        .collect::<Option<Vec<f64>>>(),
                ) else {
                    continue;
                };
                y.push(v);
                for (col, val) in x.iter_mut().zip(xs) {
                    col.push(val);
                }
            }
            let names: Vec<String> = predictors
                .iter()
                .map(|c| t.header.get(*c).cloned().unwrap_or_default())
                .collect();
            let r = regression::fit(&names, &x, &y, LEVEL)?;
            o.put("n", r.n as f64);
            o.put("R²", r.r2);
            o.put("R² ajustado", r.r2_adj);
            o.put("erro padrão residual", r.s);
            o.put("F", r.f);
            o.put("valor-p do modelo", r.f_p_value);
            for c in &r.coefs {
                o.put(&format!("coeficiente {}", c.name), c.estimate);
                o.put(&format!("valor-p {}", c.name), c.p_value);
                if let Some(v) = c.vif {
                    o.put(&format!("VIF {}", c.name), v);
                }
            }
            let eq = r
                .coefs
                .iter()
                .skip(1)
                .map(|c| format!("{:+.4}·{}", c.estimate, c.name))
                .collect::<Vec<_>>()
                .join(" ");
            o.verdict = fx(
                "analises-r2",
                &[
                    (
                        "y",
                        vx(&t
                            .header
                            .get(*response)
                            .cloned()
                            .unwrap_or_else(|| "y".into())),
                    ),
                    ("estimate", dx(format!("{:.4}", r.coefs[0].estimate))),
                    ("eq", vx(&eq)),
                    ("r2", dx(format!("{:.3}", r.r2))),
                    (
                        "a",
                        vx(&if r.f_p_value < ALPHA {
                            "modelo significativo a 5 %"
                        } else {
                            "modelo não significativo a 5 %"
                        }),
                    ),
                ],
            );
            if r.coefs.iter().any(|c| c.vif.is_some_and(|v| v > 5.0)) {
                o.notes
                    .push(tx("analises-vif_acima_de_5_preditores").into());
            }
            o.notes
                .push(tx("analises-o_grafico_mostra_os_residuos").into());
            o.series = r.residuals.clone();
            o.limits = Some((0.0, -2.0 * r.s, 2.0 * r.s));
        }
        Recipe::Surface { factors, response } => {
            surface(t, factors, *response, &mut o)?;
        }
        Recipe::Desirability { factors, responses } => {
            desirability(t, factors, responses, &mut o)?;
        }
        Recipe::Ewma {
            col,
            lambda,
            l,
            target,
            sigma,
        } => {
            let x = t.numbers(*col);
            let (mu, sd) = memory_base(&x, *target, *sigma)?;
            let e = Ewma {
                lambda: *lambda,
                l: *l,
                target: mu,
                sigma: sd,
                asymptotic: false,
            };
            let r = e.run(&x)?;
            let steady = Ewma {
                asymptotic: true,
                ..e
            }
            .half_width(0);
            o.put("pontos", x.len() as f64);
            o.put("alvo", mu);
            o.put("σ", sd);
            o.put("λ", *lambda);
            o.put("L", *l);
            o.put("LIC (estável)", mu - steady);
            o.put("LSC (estável)", mu + steady);
            let scheme = arl::Scheme::Ewma {
                lambda: *lambda,
                l: *l,
                asymptotic: false,
            };
            o.put("ARL sob controle", arl_of(&scheme, 0.0));
            o.put("ARL com desvio de 1σ", arl_of(&scheme, 1.0));
            o.put("pontos com sinal", r.signals.len() as f64);
            o.series = r.z;
            o.lines_on_chart = vec![
                ("alvo".into(), vec![mu; x.len()]),
                ("LIC".into(), r.lcl),
                ("LSC".into(), r.ucl),
            ];
            o.signals = r.signals;
            o.verdict = memory_verdict(o.signals.first().copied(), o.signals.len());
            o.notes
                .push(tx("analises-a_ewma_acumula_desvios_pequenos").into());
            if target.is_none() || sigma.is_none() {
                o.notes.push(base_note().into());
            }
        }
        Recipe::Cusum {
            col,
            k,
            h,
            target,
            sigma,
        } => {
            let x = t.numbers(*col);
            let (mu, sd) = memory_base(&x, *target, *sigma)?;
            let c = Cusum {
                k: *k,
                h: *h,
                target: mu,
                sigma: sd,
            };
            let r = c.run(&x)?;
            o.put("pontos", x.len() as f64);
            o.put("alvo", mu);
            o.put("σ", sd);
            o.put("K", k * sd);
            o.put("H", r.h);
            o.put("ARL sob controle", arl::cusum_siegmund(*k, *h, 0.0));
            o.put("ARL com desvio de 1σ", arl::cusum_siegmund(*k, *h, 1.0));
            o.put("sinais para cima", r.signals_up.len() as f64);
            o.put("sinais para baixo", r.signals_down.len() as f64);
            if let Some((_, m)) = r.shift_estimates.first() {
                o.put("média estimada depois do desvio", *m);
            }
            let n = x.len();
            o.lines_on_chart = vec![
                (
                    tx("analises-c_baixa").into(),
                    r.lower.iter().map(|v| -v).collect(),
                ),
                ("H".into(), vec![r.h; n]),
                (tx("analises-h").into(), vec![-r.h; n]),
            ];
            let mut marks: Vec<(usize, f64)> = r
                .signals_up
                .iter()
                .map(|&i| (i, r.upper[i]))
                .chain(r.signals_down.iter().map(|&i| (i, -r.lower[i])))
                .collect();
            marks.sort_by_key(|m| m.0);
            let mut sig: Vec<usize> = marks.iter().map(|m| m.0).collect();
            sig.dedup();
            o.series = r.upper;
            o.signal_marks = marks;
            o.verdict = memory_verdict(sig.first().copied(), sig.len());
            o.signals = sig;
            o.notes
                .push(tx("analises-c_acumula_os_desvios_para").into());
            if target.is_none() || sigma.is_none() {
                o.notes.push(base_note().into());
            }
        }
        Recipe::SampleSize {
            kind,
            a,
            b,
            target,
            alpha,
        } => {
            let d = match kind {
                0 => power::Design::OneMean { delta: *a, sd: *b },
                1 => power::Design::TwoMeans { delta: *a, sd: *b },
                _ => power::Design::TwoProportions { p1: *a, p2: *b },
            };
            let n = power::sample_size(d, *target, *alpha)
                .ok_or(tx("analises-parametros_impossiveis_confira_a_diferenca"))?;
            o.put("n", n as f64);
            if let Some(p) = power::power(d, n, *alpha) {
                o.put("poder com esse n", p);
            }
            o.verdict = match kind {
                0 => fx("analises-observacoes", &[("n", vx(&n))]),
                _ => fx("analises-observacoes_em_cada_grupo", &[("n", vx(&n))]),
            };
        }
        Recipe::Factorial {
            factors,
            response,
            order,
        } => {
            if factors.len() < 2 {
                return Err(tx("analises-escolha_ao_menos_dois_fatores").into());
            }
            let num = |r: &Vec<String>, c: usize| {
                r.get(c)
                    .and_then(|s| utsim_quality::data::number(s, t.decimal_comma))
            };
            let mut cols: Vec<Vec<f64>> = vec![Vec::new(); factors.len()];
            let mut y = Vec::new();
            for r in &t.rows {
                let (Some(v), Some(xs)) = (
                    num(r, *response),
                    factors
                        .iter()
                        .map(|c| num(r, *c))
                        .collect::<Option<Vec<f64>>>(),
                ) else {
                    continue;
                };
                y.push(v);
                for (c, x) in cols.iter_mut().zip(xs) {
                    c.push(x);
                }
            }
            if y.len() < 4 {
                return Err(tx("analises-o_experimento_precisa_de_ao").into());
            }
            for (i, c) in cols.iter_mut().enumerate() {
                let coded = c.iter().all(|v| [-1.0, 0.0, 1.0].contains(v));
                if !coded {
                    *c = doe::code(c).ok_or_else(|| {
                        fx("analises-o_fator_tem_um_so", &[("i", vx(&doe::letter(i)))])
                    })?;
                }
            }
            let x: Vec<Vec<f64>> = (0..y.len())
                .map(|r| cols.iter().map(|c| c[r]).collect())
                .collect();
            // A screening design (Plackett-Burman) has no interaction column orthogonal to
            // the main effects: the analysis falls back to the main effects alone.
            let a = match doe::analyze(&x, &y, (*order).max(1), LEVEL) {
                Ok(a) => a,
                Err(e) if *order > 1 => match doe::analyze(&x, &y, 1, LEVEL) {
                    Ok(a) => {
                        o.notes.push(tx("analises-plano_de_triagem_como_o").into());
                        a
                    }
                    Err(_) => return Err(e),
                },
                Err(e) => return Err(e),
            };
            let k = factors.len();
            let header = |c: usize| t.header.get(c).cloned().unwrap_or_default();
            // In a worksheet of the app the coded A, B... are followed by the actual levels
            // under the factors' names; elsewhere the column's own name is the name.
            let name = |i: usize| {
                let h = header(factors[i]);
                let col = factors[k - 1] + 1 + i;
                if h == doe::letter(i).to_string() && col != *response && !factors.contains(&col) {
                    let real = header(col);
                    if !real.is_empty() {
                        return real;
                    }
                }
                h
            };
            let label = |i: usize| {
                let n = name(i);
                if n == doe::letter(i).to_string() {
                    n
                } else {
                    format!("{} {}", doe::letter(i), n)
                }
            };
            if (0..k).any(|i| name(i) != doe::letter(i).to_string()) {
                o.notes.push(
                    (0..k)
                        .map(|i| format!("{} = {}", doe::letter(i), name(i)))
                        .collect::<Vec<_>>()
                        .join("; "),
                );
            }
            o.put("corridas", a.n as f64);
            o.put("média", a.mean);
            o.put("R²", a.r2);
            for e in &a.effects {
                let n = doe::term_name(e.term);
                o.put(&format!("efeito {n}"), e.effect);
                if let Some(p) = e.p_value {
                    o.put(&format!("valor-p {n}"), p);
                }
            }
            if let Some(c) = a.curvature {
                o.put("curvatura", c);
                if let Some(p) = a
                    .anova
                    .iter()
                    .find(|l| l.source == "curvatura")
                    .and_then(|l| l.p_value)
                {
                    o.put("valor-p curvatura", p);
                }
            }
            if let Some(l) = &a.lenth {
                o.put("PSE de Lenth", l.pse);
                o.put("margem de erro de Lenth", l.me);
                o.notes
                    .push(tx("analises-sem_grau_de_liberdade_para").into());
            }
            for (m, f) in &a.confounded {
                o.notes.push(if *f == 0 {
                    fx(
                        "analises-confundido_com_a_media",
                        &[("m", vx(&doe::term_name(*m)))],
                    )
                } else {
                    fx(
                        "analises-confundido_com",
                        &[
                            ("m", vx(&doe::term_name(*m))),
                            ("f", vx(&doe::term_name(*f))),
                        ],
                    )
                });
            }
            let active: Vec<String> = a
                .effects
                .iter()
                .filter(|e| e.p_value.is_some_and(|p| p < ALPHA))
                .map(|e| doe::term_name(e.term))
                .collect();
            o.verdict = if active.is_empty() {
                tx("analises-nenhum_termo_ativo_a_5").to_string()
            } else {
                fx(
                    "analises-termos_ativos_a_5",
                    &[("active", vx(&active.join(", ")))],
                )
            };
            if a.anova
                .iter()
                .any(|l| l.source == "curvatura" && l.p_value.is_some_and(|p| p < ALPHA))
            {
                o.verdict
                    .push_str(tx("analises-curvatura_significativa_a_resposta_nao"));
            }
            // Pareto of the effects, main effects and the interactions in the model.
            let mut by: Vec<&doe::Effect> = a.effects.iter().collect();
            by.sort_by(|p, q| q.effect.abs().total_cmp(&p.effect.abs()));
            o.plots.push(Plot {
                title: tx("analises-pareto_dos_efeitos_valor_absoluto").into(),
                bars: true,
                labels: by.iter().map(|e| doe::term_name(e.term)).collect(),
                series: vec![(
                    "|efeito|".into(),
                    by.iter().map(|e| e.effect.abs()).collect(),
                )],
            });
            o.plots.push(Plot {
                title: tx("analises-efeitos_principais").into(),
                bars: false,
                labels: vec![tx("analises-baixo_1").into(), tx("analises-alto_1").into()],
                series: (0..k)
                    .map(|i| {
                        let (lo, hi) = doe::main_effect_means(&x, &y, i);
                        (label(i), vec![lo, hi])
                    })
                    .collect(),
            });
            for e in a.effects.iter().filter(|e| doe::order(e.term) == 2).take(6) {
                let ij: Vec<usize> = (0..k).filter(|i| e.term & (1 << i) != 0).collect();
                let (i, j) = (ij[0], ij[1]);
                let m = doe::interaction_means(&x, &y, i, j);
                o.plots.push(Plot {
                    title: fx(
                        "analises-interacao-2",
                        &[("i", vx(&doe::letter(i))), ("j", vx(&doe::letter(j)))],
                    ),
                    bars: false,
                    labels: vec![
                        fx("analises-baixo", &[("i", vx(&doe::letter(i)))]),
                        fx("analises-alto", &[("i", vx(&doe::letter(i)))]),
                    ],
                    series: vec![
                        (
                            fx("analises-baixo", &[("j", vx(&doe::letter(j)))]),
                            m[0].to_vec(),
                        ),
                        (
                            fx("analises-alto", &[("j", vx(&doe::letter(j)))]),
                            m[1].to_vec(),
                        ),
                    ],
                });
            }
        }
    }
    Ok(o)
}

/// The type 1 gauge study: `Cg = 0,2·T / (6·s)` and `Cgk = (0,1·T − |x̄ − ref|) / (3·s)`,
/// with `T` the tolerance (the usual 20 % of the tolerance and 4 standard deviations on
/// each side for Cg; ≥ 1,33 is capable).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Type1 {
    pub mean: f64,
    pub sd: f64,
    pub bias: f64,
    pub cg: f64,
    pub cgk: f64,
}

pub fn type1(x: &[f64], reference: f64, tolerance: f64) -> Result<Type1, String> {
    if tolerance.is_nan() || tolerance <= 0.0 {
        return Err(tx("analises-informe_a_tolerancia_lse_lie").into());
    }
    let mean = descriptive::mean(x);
    let sd = descriptive::std_dev(x);
    if sd.is_nan() || sd <= 0.0 {
        return Err(tx("analises-medicoes_sem_variacao_cg_nao").into());
    }
    let bias = mean - reference;
    Ok(Type1 {
        mean,
        sd,
        bias,
        cg: 0.2 * tolerance / (6.0 * sd),
        cgk: (0.1 * tolerance - bias.abs()) / (3.0 * sd),
    })
}

// --- The journal ------------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq)]
pub struct Analysis {
    pub id: Id,
    pub name: String,
    pub recipe: Option<Recipe>,
    pub dataset: Option<Id>,
    pub dataset_hash: String,
    pub origin: DataOrigin,
    pub outcome: Outcome,
    pub verdict: String,
    pub justification: String,
    pub date: Option<Date>,
}

impl Analysis {
    pub fn read(id: Id, e: &Entity) -> Self {
        Self {
            id,
            name: e.text(fields::NAME),
            recipe: e.typed(fields::RECIPE),
            dataset: e.id(fields::DATASET),
            dataset_hash: e.text(fields::DATASET_HASH),
            origin: e.typed(fields::ORIGIN).unwrap_or_default(),
            outcome: e.typed(fields::SUMMARY).unwrap_or_default(),
            verdict: e.text(fields::VERDICT),
            justification: e.text(fields::JUSTIFICATION),
            date: e.opt_text(fields::DATE).and_then(|s| s.parse().ok()),
        }
    }

    pub fn entity_ref(id: Id) -> EntityRef {
        EntityRef::new(Kind::Analysis, id)
    }

    /// Fleiss' kappa of an attribute agreement study.
    pub fn kappa(&self) -> Option<f64> {
        matches!(self.recipe, Some(Recipe::Attribute { .. }))
            .then(|| self.outcome.value(KAPPA))
            .flatten()
    }

    /// The `%VE` of the GRR of a Gage R&R.
    pub fn grr(&self) -> Option<f64> {
        matches!(self.recipe, Some(Recipe::GageRr { .. }))
            .then(|| self.outcome.value(&format!("%VE {}", msa::GRR)))
            .flatten()
    }
}

/// The statistical analyses of a project.
pub fn analyses(state: &State, project: Id) -> Vec<Analysis> {
    let key = project.to_string();
    let mut v: Vec<Analysis> = state
        .of_kind(Kind::Analysis)
        .filter(|(_, e)| !e.is_deleted() && e.text(fields::PROJECT) == key)
        .filter(|(_, e)| e.text(fields::FAMILY) == FAMILY)
        .map(|(id, e)| Analysis::read(id, e))
        .collect();
    v.sort_by_key(|a| (a.date, a.id));
    v
}

/// Edits of an analysis of `dataset`.
pub fn record(
    project: Id,
    id: Id,
    name: &str,
    dataset: &Dataset,
    recipe: &Recipe,
    outcome: &Outcome,
    date: Date,
) -> Vec<Edit> {
    let r = Analysis::entity_ref(id);
    vec![
        Edit::new(r, fields::PROJECT, project.to_string()),
        Edit::new(r, fields::FAMILY, FAMILY),
        Edit::new(
            r,
            fields::NAME,
            if name.trim().is_empty() {
                recipe.label()
            } else {
                name.trim()
            },
        ),
        Edit::new(r, fields::KIND, recipe.key()),
        Edit::new(
            r,
            fields::RECIPE,
            serde_json::to_value(recipe).expect("recipe"),
        ),
        Edit::new(r, fields::DATASET, dataset.id.to_string()),
        Edit::new(r, fields::DATASET_HASH, dataset.hash.as_str()),
        Edit::new(r, fields::ORIGIN, enum_value(dataset.origin)),
        Edit::new(
            r,
            fields::SUMMARY,
            serde_json::to_value(outcome).expect("outcome"),
        ),
        Edit::new(r, fields::VERDICT, outcome.verdict.as_str()),
        Edit::new(
            r,
            fields::VERSION,
            format!("utsim-lss-core {}", env!("CARGO_PKG_VERSION")),
        ),
        Edit::new(r, fields::DATE, date.to_string()),
    ]
}

/// Runs the recipe of a saved analysis again on the data set's text: the same numbers.
pub fn rerun(a: &Analysis, text: &str) -> Result<Outcome, String> {
    let r = a
        .recipe
        .as_ref()
        .ok_or(tx("analises-analise_sem_receita"))?;
    run(r, &Table::parse(text))
}

/// The Medir check "MSA aprovado": a Gage R&R with `%VE` of the GRR ≤ 30 %, or an attribute
/// agreement study with Fleiss' kappa ≥ 0,75 (for a characteristic judged by people), or
/// either one with a justification recorded.
pub fn msa_approved(state: &State, project: Id) -> bool {
    analyses(state, project).iter().any(|a| {
        let justified = !a.justification.trim().is_empty();
        a.grr().is_some_and(|g| g <= GRR_LIMIT + 1e-9 || justified)
            || a.kappa()
                .is_some_and(|k| k + 1e-9 >= KAPPA_LIMIT || justified)
    })
}

/// The Medir check "linha de base com dado de origem Real": a capability or a control chart
/// on a data set of origin `Real`.
pub fn baseline_real(state: &State, project: Id) -> bool {
    analyses(state, project).iter().any(|a| {
        a.origin == DataOrigin::Real
            && matches!(
                a.recipe,
                Some(Recipe::Capability { .. } | Recipe::Chart { .. })
            )
    })
}

#[cfg(test)]
mod tests;
