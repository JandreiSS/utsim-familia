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

use serde::{Deserialize, Serialize};
use serde_json::Value;
use utsim_quality::attribute::{self, Rating};
use utsim_quality::charts::{self, ChartData, ChartKind};
use utsim_quality::msa::{self, GageData, GageOptions, Measurement};
use utsim_quality::rules::RuleSet;
use utsim_stats::descriptive::{self, QuantileMethod};
use utsim_stats::hypothesis::{self, Alternative};
use utsim_stats::nonparametric::{self, RankMethod};

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
}

impl Recipe {
    pub fn label(&self) -> &'static str {
        match self {
            Recipe::Descriptive { .. } => "Estatística descritiva",
            Recipe::Normality { .. } => "Normalidade (Anderson-Darling)",
            Recipe::Capability { .. } => "Capacidade do processo",
            Recipe::Chart { .. } => "Carta de controle",
            Recipe::TOne { .. } => "Teste t de uma amostra",
            Recipe::TTwo { .. } => "Teste t de duas amostras",
            Recipe::TPaired { .. } => "Teste t pareado",
            Recipe::Anova { .. } => "ANOVA de um fator com Tukey",
            Recipe::ChiSquare { .. } => "Qui-quadrado de independência",
            Recipe::Proportion { .. } => "Teste de uma proporção",
            Recipe::TwoProportions { .. } => "Teste de duas proporções",
            Recipe::MannWhitney { .. } => "Mann-Whitney",
            Recipe::Wilcoxon { .. } => "Wilcoxon pareado",
            Recipe::GageRr { .. } => "MSA: Gage R&R",
            Recipe::Type1 { .. } => "MSA: estudo do tipo 1",
            Recipe::Bias { .. } => "MSA: viés",
            Recipe::Linearity { .. } => "MSA: linearidade",
            Recipe::Attribute { .. } => "MSA: concordância de atributos",
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

fn need(v: &[f64], n: usize, what: &str) -> Result<(), String> {
    if v.len() < n {
        Err(format!(
            "{what}: são precisos ao menos {n} valores numéricos (há {})",
            v.len()
        ))
    } else {
        Ok(())
    }
}

fn verdict_p(p: f64, h1: &str) -> String {
    if p < 1.0 - LEVEL {
        format!("valor-p {p:.4} < 0,05: há evidência de que {h1}")
    } else {
        format!("valor-p {p:.4} ≥ 0,05: sem evidência de que {h1}")
    }
}

/// Runs a recipe on a table.
pub fn run(recipe: &Recipe, t: &Table) -> Result<Outcome, String> {
    let mut o = Outcome::default();
    match recipe {
        Recipe::Descriptive { col } => {
            let x = t.numbers(*col);
            need(&x, 1, "descritiva")?;
            let d =
                descriptive::describe(&x, LEVEL, QuantileMethod::default()).ok_or("sem valores")?;
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
            o.verdict = format!(
                "média {:.4} e desvio {:.4} de {} valores",
                d.mean, d.sd, d.n
            );
            o.series = x;
        }
        Recipe::Normality { col } => {
            let x = t.numbers(*col);
            need(&x, 3, "normalidade")?;
            let a = utsim_quality::normality::anderson_darling(&x).ok_or("sem variação")?;
            o.put("n", a.n as f64);
            o.put("A²", a.a2);
            o.put("A*²", a.a2_star);
            o.put("valor-p", a.p_value);
            o.verdict = if a.p_value < 0.05 {
                format!(
                    "valor-p {:.4} < 0,05: os dados não parecem normais",
                    a.p_value
                )
            } else {
                format!(
                    "valor-p {:.4} ≥ 0,05: sem evidência contra a normalidade",
                    a.p_value
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
                format!("Cpk {:.2} ≥ 1,33: processo capaz", c.cpk.value)
            } else if c.cpk.value >= 1.0 {
                format!("Cpk {:.2} entre 1 e 1,33: capaz com ressalva", c.cpk.value)
            } else {
                format!("Cpk {:.2} < 1: processo não capaz", c.cpk.value)
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
                    return Err("contagens e tamanhos com números de linhas diferentes".into());
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
            let p = ch.panels.first().ok_or("carta sem painel")?;
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
                "nenhuma regra violada: processo sob controle estatístico".into()
            } else {
                format!(
                    "{} ponto(s) com sinal: há causas especiais a investigar",
                    o.signals.len()
                )
            };
            o.notes = ch.notes.clone();
        }
        Recipe::TOne { col, mu0 } => {
            let x = t.numbers(*col);
            need(&x, 2, "teste t")?;
            let r = hypothesis::t_one_sample(&x, *mu0, LEVEL, Alternative::TwoSided)
                .ok_or("dados insuficientes")?;
            o.put("média", r.estimate);
            o.put("t", r.t);
            o.put("gl", r.df);
            o.put("valor-p", r.p_value);
            o.put("IC inferior", r.ci.low);
            o.put("IC superior", r.ci.high);
            o.verdict = verdict_p(r.p_value, &format!("a média difere de {mu0}"));
        }
        Recipe::TTwo { a, b, pooled } => {
            let (x, y) = (t.numbers(*a), t.numbers(*b));
            need(&x, 2, "amostra 1")?;
            need(&y, 2, "amostra 2")?;
            let r = hypothesis::t_two_sample(&x, &y, 0.0, *pooled, LEVEL, Alternative::TwoSided)
                .ok_or("dados insuficientes")?;
            o.put("diferença das médias", r.estimate);
            o.put("t", r.t);
            o.put("gl", r.df);
            o.put("valor-p", r.p_value);
            o.put("IC inferior", r.ci.low);
            o.put("IC superior", r.ci.high);
            o.verdict = verdict_p(r.p_value, "as médias diferem");
        }
        Recipe::TPaired { a, b } => {
            let (x, y) = t.pairs(*a, *b);
            need(&x, 2, "pares")?;
            let r = hypothesis::t_paired(&x, &y, 0.0, LEVEL, Alternative::TwoSided)
                .ok_or("dados insuficientes")?;
            o.put("diferença média", r.estimate);
            o.put("t", r.t);
            o.put("valor-p", r.p_value);
            o.put("IC inferior", r.ci.low);
            o.put("IC superior", r.ci.high);
            o.verdict = verdict_p(r.p_value, "há diferença entre os pares");
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
            let r =
                hypothesis::one_way_anova(&refs, LEVEL).ok_or("são precisos 2 grupos com dados")?;
            o.put("grupos", r.anova.groups as f64);
            o.put("F", r.anova.f);
            o.put("valor-p", r.anova.p_value);
            o.put("R²", r.r_squared);
            for (k, m) in keys.iter().zip(&r.anova.means) {
                o.put(&format!("média de {k}"), *m);
            }
            o.verdict = verdict_p(r.anova.p_value, "alguma média de grupo difere");
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
            let r = hypothesis::chi_square_independence(&table).ok_or("tabela insuficiente")?;
            o.put("X²", r.statistic);
            o.put("gl", r.df as f64);
            o.put("valor-p", r.p_value);
            o.verdict = verdict_p(r.p_value, "as duas variáveis estão associadas");
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
            .ok_or("proporção inválida (0 < p0 < 1, x ≤ n)")?;
            o.put("p", r.p_hat);
            o.put("valor-p", r.p_value);
            o.put("IC inferior", r.ci.low);
            o.put("IC superior", r.ci.high);
            o.verdict = verdict_p(r.p_value, &format!("a proporção difere de {p0}"));
        }
        Recipe::TwoProportions { x1, n1, x2, n2 } => {
            let r = hypothesis::two_proportions(*x1, *n1, *x2, *n2, LEVEL, Alternative::TwoSided)
                .ok_or("proporções inválidas")?;
            o.put("p1", r.p_hat[0]);
            o.put("p2", r.p_hat[1]);
            o.put("diferença", r.difference);
            o.put("valor-p", r.p_value);
            o.verdict = verdict_p(r.p_value, "as proporções diferem");
        }
        Recipe::MannWhitney { a, b } => {
            let (x, y) = (t.numbers(*a), t.numbers(*b));
            need(&x, 1, "amostra 1")?;
            need(&y, 1, "amostra 2")?;
            let r =
                nonparametric::mann_whitney(&x, &y, RankMethod::default(), Alternative::TwoSided)
                    .ok_or("dados insuficientes")?;
            o.put("U1", r.u1);
            o.put("valor-p", r.p_value);
            o.put("deslocamento (Hodges-Lehmann)", r.shift);
            o.verdict = verdict_p(r.p_value, "as distribuições diferem em posição");
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
            .ok_or("sem diferenças diferentes de zero")?;
            o.put("W+", r.w_plus);
            o.put("valor-p", r.p_value);
            o.put("pseudomediana", r.pseudo_median);
            o.verdict = verdict_p(r.p_value, "há diferença entre os pares");
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
            o.verdict = format!("{}; ndc = {}", g.verdict(), g.ndc);
            o.notes = g.warnings.clone();
        }
        Recipe::Type1 {
            col,
            reference,
            tolerance,
        } => {
            let x = t.numbers(*col);
            need(&x, 10, "estudo do tipo 1")?;
            let r = type1(&x, *reference, *tolerance)?;
            o.put("n", x.len() as f64);
            o.put("média", r.mean);
            o.put("desvio", r.sd);
            o.put("tendência", r.bias);
            o.put("Cg", r.cg);
            o.put("Cgk", r.cgk);
            o.verdict = if r.cg >= 1.33 && r.cgk >= 1.33 {
                format!(
                    "Cg {:.2} e Cgk {:.2} ≥ 1,33: instrumento capaz",
                    r.cg, r.cgk
                )
            } else {
                format!(
                    "Cg {:.2} ou Cgk {:.2} < 1,33: instrumento não capaz",
                    r.cg, r.cgk
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
            need(&x, 10, "estudo de viés")?;
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
                format!(
                    "viés de {:.4} significativo (valor-p {:.4}): o instrumento lê deslocado da referência",
                    b.bias, b.p_value
                )
            } else {
                format!(
                    "viés não significativo (valor-p {:.4}): o intervalo do viés contém zero",
                    b.p_value
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
                format!(
                    "o viés muda ao longo da faixa (inclinação {:.4}, valor-p {:.4}): há problema de linearidade",
                    l.slope, l.p_slope
                )
            } else if l.p_intercept < ALPHA {
                format!(
                    "viés constante ao longo da faixa (valor-p do intercepto {:.4}): corrigir o deslocamento",
                    l.p_intercept
                )
            } else {
                "sem viés nem problema de linearidade ao longo da faixa".to_string()
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
                Some(k) if k >= KAPPA_LIMIT => {
                    format!("kappa {k:.2} ≥ 0,75: concordância boa, sistema aceito")
                }
                Some(k) if k >= 0.4 => {
                    format!(
                        "kappa {k:.2} entre 0,40 e 0,75: concordância razoável, melhorar o critério"
                    )
                }
                Some(k) => format!("kappa {k:.2} < 0,40: concordância fraca, sistema não aceito"),
                None => "sem kappa: precisa de ao menos dois avaliadores ou repetições".to_string(),
            };
            o.notes = a.warnings.clone();
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
        return Err("informe a tolerância (LSE − LIE) positiva".into());
    }
    let mean = descriptive::mean(x);
    let sd = descriptive::std_dev(x);
    if sd.is_nan() || sd <= 0.0 {
        return Err("medições sem variação: Cg não existe".into());
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
    let r = a.recipe.as_ref().ok_or("análise sem receita")?;
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
