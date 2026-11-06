//! The mission format (`missao.ron`, `formato: 1`): a UTSim model with a problem, a goal
//! and a small set of levers. Every lever becomes `apply_patches` keys over the base model.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

/// Mission format read by this version.
pub const FORMATO_MISSAO: u32 = 1;

fn um() -> u32 {
    1
}
fn um_f() -> f64 {
    1.0
}
fn dez() -> u32 {
    10
}
fn dois_f() -> f64 {
    2.0
}
fn duas_casas() -> u32 {
    2
}

/// One mission.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Missao {
    /// Stable identifier inside the package (`ato1/furadeira`).
    pub id: String,
    #[serde(default = "um")]
    pub formato: u32,
    pub titulo: String,
    /// Glossary ids taught or used by the mission.
    #[serde(default)]
    pub conceitos: Vec<String>,
    /// Model path inside the package.
    pub modelo: String,
    /// Statement path inside the package (simple Markdown).
    pub texto: String,
    /// Per-student variation of the parameters, drawn from the student's seed.
    #[serde(default)]
    pub variante: Vec<Faixa>,
    /// Measures of the initial state the student may see before predicting.
    #[serde(default)]
    pub medicoes: Vec<Medicao>,
    /// Questions answered before the run (prediction points).
    #[serde(default)]
    pub previsao: Vec<Previsao>,
    #[serde(default)]
    pub alavancas: Vec<Alavanca>,
    /// Money available for the decisions (sum of the lever costs).
    #[serde(default)]
    pub orcamento: Option<f64>,
    /// Other limits over the decisions (area, people).
    #[serde(default)]
    pub restricoes: Vec<Restricao>,
    pub metricas: Vec<Metrica>,
    /// Objective questions about the why (justification points).
    #[serde(default)]
    pub justificativa: Vec<Pergunta>,
    #[serde(default)]
    pub rodadas: Rodadas,
    /// Up to three hint levels: a guiding question, the tool, the path.
    #[serde(default)]
    pub dicas: Vec<String>,
    /// The reference decisions (computed by the calibration, checked by the golden test).
    #[serde(default)]
    pub referencia: Decisoes,
    #[serde(default)]
    pub animacao: Animacao,
    /// Tasks to assign to stations (act 4).
    #[serde(default)]
    pub balanceamento: Option<crate::pecas::Balanceamento>,
    /// Machines to place on a grid (act 5).
    #[serde(default)]
    pub arranjo: Option<crate::pecas::Arranjo>,
}

/// The decisions of an attempt: lever id to number (a quantity, the index of a choice or
/// a value).
pub type Decisoes = BTreeMap<String, f64>;

/// A parameter drawn per student, uniform in `[de, ate]` with `casas` decimals.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Faixa {
    pub chave: String,
    pub de: f64,
    pub ate: f64,
    /// Template where `{}` is replaced by the drawn number (`Exp(mean: {})`).
    #[serde(default)]
    pub modelo: Option<String>,
    #[serde(default = "duas_casas")]
    pub casas: u32,
}

/// A measure of the initial state shown before the prediction.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Medicao {
    pub rotulo: String,
    pub valor: Vec<Termo>,
    #[serde(default)]
    pub unidade: String,
    #[serde(default = "duas_casas")]
    pub casas: u32,
}

/// A term of a measure: `fator × chave`, evaluated per replication. `chave` is a metric of
/// the report (`saida.time_in_system.mean`), `custo` (the cost of the decisions),
/// `decisao.<id>` (the number of a lever) or `const` (just `fator`).
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Termo {
    pub chave: String,
    #[serde(default = "um_f")]
    pub fator: f64,
}

/// A prediction asked before the run.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub enum Previsao {
    /// Pick one option. The right one comes from the initial state (largest or smallest
    /// value of the options) or, for a conceptual question, is fixed.
    Escolha {
        id: String,
        pergunta: String,
        opcoes: Vec<OpcaoPrevisao>,
        resposta: Resposta,
    },
    /// A number, right when within the tolerance of the value of the initial state.
    Valor {
        id: String,
        pergunta: String,
        valor: Vec<Termo>,
        #[serde(default)]
        unidade: String,
        tolerancia: Tolerancia,
    },
}

impl Previsao {
    pub fn id(&self) -> &str {
        match self {
            Previsao::Escolha { id, .. } | Previsao::Valor { id, .. } => id,
        }
    }
    pub fn pergunta(&self) -> &str {
        match self {
            Previsao::Escolha { pergunta, .. } | Previsao::Valor { pergunta, .. } => pergunta,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct OpcaoPrevisao {
    pub rotulo: String,
    /// The value that decides `Maior`/`Menor` (empty for `Fixa`).
    #[serde(default)]
    pub valor: Vec<Termo>,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq)]
pub enum Resposta {
    /// The option with the largest value in the initial state.
    Maior,
    /// The option with the smallest value in the initial state.
    Menor,
    /// A conceptual question: the right option is this index.
    Fixa(usize),
}

/// A decision the student makes.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub enum Alavanca {
    /// An integer from `de` to `ate`, written to `chave` (through `modelo` when given).
    /// Each unit above `inicial` costs `custo_unitario`.
    Quantidade {
        id: String,
        rotulo: String,
        chave: String,
        de: i64,
        ate: i64,
        #[serde(default)]
        inicial: Option<i64>,
        #[serde(default)]
        custo_unitario: f64,
        #[serde(default)]
        modelo: Option<String>,
    },
    /// One option among several; each option sets its own keys and has its own cost.
    Escolha {
        id: String,
        rotulo: String,
        opcoes: Vec<OpcaoAlavanca>,
        #[serde(default)]
        inicial: usize,
    },
    /// A number from `de` to `ate` in steps of `passo`. Each unit above `inicial` costs
    /// `custo_unitario`.
    Valor {
        id: String,
        rotulo: String,
        chave: String,
        de: f64,
        ate: f64,
        passo: f64,
        #[serde(default)]
        inicial: Option<f64>,
        #[serde(default)]
        custo_unitario: f64,
        #[serde(default)]
        modelo: Option<String>,
        #[serde(default)]
        unidade: String,
    },
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct OpcaoAlavanca {
    pub rotulo: String,
    /// Keys and values written when the option is chosen.
    #[serde(default)]
    pub ajustes: BTreeMap<String, String>,
    #[serde(default)]
    pub custo: f64,
    /// The number of the option in `decisao.<id>` terms (its index when absent).
    #[serde(default)]
    pub numero: Option<f64>,
}

impl Alavanca {
    pub fn id(&self) -> &str {
        match self {
            Alavanca::Quantidade { id, .. }
            | Alavanca::Escolha { id, .. }
            | Alavanca::Valor { id, .. } => id,
        }
    }
    pub fn rotulo(&self) -> &str {
        match self {
            Alavanca::Quantidade { rotulo, .. }
            | Alavanca::Escolha { rotulo, .. }
            | Alavanca::Valor { rotulo, .. } => rotulo,
        }
    }
    /// The decision of the initial state.
    pub fn inicial(&self) -> f64 {
        match self {
            Alavanca::Quantidade { de, inicial, .. } => inicial.unwrap_or(*de) as f64,
            Alavanca::Escolha { inicial, .. } => *inicial as f64,
            Alavanca::Valor { de, inicial, .. } => inicial.unwrap_or(*de),
        }
    }
    /// Every allowed decision, in order (the values of a quantity or a value, the indexes
    /// of a choice).
    pub fn valores(&self) -> Vec<f64> {
        match self {
            Alavanca::Quantidade { de, ate, .. } => (*de..=*ate).map(|v| v as f64).collect(),
            Alavanca::Escolha { opcoes, .. } => (0..opcoes.len()).map(|i| i as f64).collect(),
            Alavanca::Valor { de, ate, passo, .. } => {
                let mut out = Vec::new();
                if *passo <= 0.0 {
                    return vec![*de];
                }
                let n = ((ate - de) / passo + 1e-9).floor() as i64;
                for i in 0..=n {
                    out.push(arredondar(de + i as f64 * passo, 6));
                }
                out
            }
        }
    }
    /// Whether `v` is one of [`Alavanca::valores`].
    pub fn aceita(&self, v: f64) -> bool {
        self.valores().iter().any(|x| (x - v).abs() < 1e-9)
    }
    /// Cost of decision `v`.
    pub fn custo(&self, v: f64) -> f64 {
        match self {
            Alavanca::Quantidade { custo_unitario, .. }
            | Alavanca::Valor { custo_unitario, .. } => {
                (v - self.inicial()).max(0.0) * custo_unitario
            }
            Alavanca::Escolha { opcoes, .. } => opcoes
                .get(v.round().max(0.0) as usize)
                .map_or(0.0, |o| o.custo),
        }
    }
    /// The number of decision `v` in `decisao.<id>` terms.
    pub fn numero(&self, v: f64) -> f64 {
        match self {
            Alavanca::Escolha { opcoes, .. } => opcoes
                .get(v.round().max(0.0) as usize)
                .and_then(|o| o.numero)
                .unwrap_or(v),
            _ => v,
        }
    }
    /// The model keys written by decision `v`.
    pub fn ajustes(&self, v: f64) -> BTreeMap<String, String> {
        let mut out = BTreeMap::new();
        match self {
            Alavanca::Quantidade { chave, modelo, .. } => {
                let n = format!("{}", v.round() as i64);
                out.insert(chave.clone(), aplicar_modelo(modelo.as_deref(), &n));
            }
            Alavanca::Valor { chave, modelo, .. } => {
                let n = numero_ron(v);
                out.insert(chave.clone(), aplicar_modelo(modelo.as_deref(), &n));
            }
            Alavanca::Escolha { opcoes, .. } => {
                if let Some(o) = opcoes.get(v.round().max(0.0) as usize) {
                    out.extend(o.ajustes.clone());
                }
            }
        }
        out
    }
    /// Text of decision `v` for the screen and the reports.
    pub fn descrever(&self, v: f64) -> String {
        match self {
            Alavanca::Quantidade { .. } => format!("{}", v.round() as i64),
            Alavanca::Valor { unidade, .. } => {
                let n = numero_ron(v).replace('.', ",");
                if unidade.is_empty() {
                    n
                } else {
                    format!("{n} {unidade}")
                }
            }
            Alavanca::Escolha { opcoes, .. } => opcoes
                .get(v.round().max(0.0) as usize)
                .map_or_else(|| "?".into(), |o| o.rotulo.clone()),
        }
    }
}

/// A RON number: always with a decimal point, so it parses as a float.
pub fn numero_ron(v: f64) -> String {
    let s = format!("{}", arredondar(v, 6));
    if s.contains('.') || s.contains('e') || s.contains("inf") || s.contains("NaN") {
        s
    } else {
        format!("{s}.0")
    }
}

pub(crate) fn arredondar(v: f64, casas: u32) -> f64 {
    let f = 10f64.powi(casas as i32);
    (v * f).round() / f
}

fn aplicar_modelo(modelo: Option<&str>, n: &str) -> String {
    match modelo {
        Some(m) => m.replace("{}", n),
        None => n.to_string(),
    }
}

/// A limit over the decisions (`valor` uses only `custo`, `decisao.<id>` and `const`).
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Restricao {
    pub rotulo: String,
    pub valor: Vec<Termo>,
    pub maximo: f64,
}

/// A measure of the result that enters the grade.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Metrica {
    pub nome: String,
    pub rotulo: String,
    #[serde(default)]
    pub unidade: String,
    pub valor: Vec<Termo>,
    pub objetivo: Objetivo,
    /// The goal; without it the measure only counts in the performance.
    #[serde(default)]
    pub meta: Option<f64>,
    #[serde(default = "um_f")]
    pub peso: f64,
    #[serde(default = "tolerancia_padrao")]
    pub tolerancia: Tolerancia,
    #[serde(default = "duas_casas")]
    pub casas: u32,
}

fn tolerancia_padrao() -> Tolerancia {
    Tolerancia::Relativa(0.02)
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub enum Objetivo {
    Menor,
    Maior,
}

impl Objetivo {
    /// Whether `a` is at least as good as `b`.
    pub fn melhor_ou_igual(self, a: f64, b: f64) -> bool {
        match self {
            Objetivo::Menor => a <= b,
            Objetivo::Maior => a >= b,
        }
    }
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq)]
pub enum Tolerancia {
    /// A fraction of the reference value (`0.02` is 2 %).
    Relativa(f64),
    /// In the unit of the measure.
    Absoluta(f64),
}

impl Tolerancia {
    /// Half-width of the band around `referencia`.
    pub fn faixa(self, referencia: f64) -> f64 {
        match self {
            Tolerancia::Relativa(r) => r.abs() * referencia.abs(),
            Tolerancia::Absoluta(a) => a.abs(),
        }
    }
}

/// An objective question with a fixed answer.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Pergunta {
    pub pergunta: String,
    pub opcoes: Vec<String>,
    pub certa: usize,
}

/// How a mission is run.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Rodadas {
    #[serde(default = "dez")]
    pub replicacoes: u32,
    /// Runs allowed before the discount (each run "costs a week of factory").
    #[serde(default)]
    pub orcamento: Option<u32>,
    /// Points lost per run above `orcamento`.
    #[serde(default = "dois_f")]
    pub desconto_por_extra: f64,
}

impl Default for Rodadas {
    fn default() -> Self {
        Rodadas {
            replicacoes: 10,
            orcamento: None,
            desconto_por_extra: 2.0,
        }
    }
}

/// The animated replication.
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
pub struct Animacao {
    /// Simulated time recorded for the 2D view (default: 480 or the run, if shorter).
    #[serde(default)]
    pub duracao: Option<f64>,
    /// Time unit name shown under the clock (`min`).
    #[serde(default)]
    pub unidade: Option<String>,
}

impl Missao {
    pub fn alavanca(&self, id: &str) -> Option<&Alavanca> {
        self.alavancas.iter().find(|a| a.id() == id)
    }

    /// The decisions of the initial state.
    pub fn decisoes_iniciais(&self) -> Decisoes {
        let mut d: Decisoes = self
            .alavancas
            .iter()
            .map(|a| (a.id().to_string(), a.inicial()))
            .collect();
        if let Some(b) = self.balanceamento.as_ref().filter(|b| b.editavel) {
            d.extend(b.decisoes(&b.inicial));
        }
        if let Some(a) = &self.arranjo {
            d.extend(a.decisoes(&a.inicial));
        }
        d
    }

    /// Whether the student assigns tasks or places machines (the pieces of act 4 and 5).
    pub fn tem_pecas(&self) -> bool {
        self.balanceamento.as_ref().is_some_and(|b| b.editavel) || self.arranjo.is_some()
    }

    /// The model keys of the levers and of the pieces, for the objects of the model.
    pub fn ajustes_completos(
        &self,
        decisoes: &Decisoes,
        objetos: &[String],
    ) -> BTreeMap<String, String> {
        let d = self.completar(decisoes);
        let mut out = self.ajustes(&d);
        if let Some(b) = &self.balanceamento {
            out.extend(b.ajustes(&b.atribuicao(&d)));
        }
        if let Some(a) = &self.arranjo {
            out.extend(a.ajustes(&a.posicoes(&d), objetos));
        }
        out
    }

    /// `decisoes` completed with the initial value of the missing levers.
    pub fn completar(&self, decisoes: &Decisoes) -> Decisoes {
        let mut out = self.decisoes_iniciais();
        for (k, v) in decisoes {
            if out.contains_key(k) {
                out.insert(k.clone(), *v);
            }
        }
        out
    }

    /// The total cost of the decisions.
    pub fn custo(&self, decisoes: &Decisoes) -> f64 {
        let d = self.completar(decisoes);
        self.alavancas.iter().map(|a| a.custo(d[a.id()])).sum()
    }

    /// The model keys of the decisions (initial values for the missing levers).
    pub fn ajustes(&self, decisoes: &Decisoes) -> BTreeMap<String, String> {
        let d = self.completar(decisoes);
        let mut out = BTreeMap::new();
        for a in &self.alavancas {
            out.extend(a.ajustes(d[a.id()]));
        }
        out
    }

    /// Reads a mission from RON text, refusing a newer format.
    pub fn parse(text: &str) -> Result<Missao, String> {
        let m: Missao = ron::Options::default()
            .with_default_extension(ron::extensions::Extensions::IMPLICIT_SOME)
            .from_str(text)
            .map_err(|e| format!("erro de sintaxe: {e}"))?;
        if m.formato > FORMATO_MISSAO {
            return Err(format!(
                "missão no formato {}; esta versão lê até o formato {FORMATO_MISSAO}",
                m.formato
            ));
        }
        Ok(m)
    }

    /// Every combination of lever values (the search space of the reference).
    pub fn combinacoes(&self) -> Vec<Decisoes> {
        let mut out = vec![Decisoes::new()];
        for a in &self.alavancas {
            let mut next = Vec::new();
            for d in &out {
                for v in a.valores() {
                    let mut e = d.clone();
                    e.insert(a.id().to_string(), v);
                    next.push(e);
                }
            }
            out = next;
        }
        out
    }
}
