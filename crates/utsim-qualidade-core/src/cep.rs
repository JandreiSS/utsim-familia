//! Statistical process control on real data and its comparison with the simulated data
//! (stage QS12 of `docs/plano_qualidade_e_seguranca.md`, section 3.4).
//!
//! The CSV is read exactly as `utsim qualidade` reads it (`utsim_quality::data`: encoding,
//! decimal comma, label column, subgroups), so the limits are the same for the same file.
//! Phase I estimates the limits, phase II applies frozen limits. A signal opens a reaction
//! card with the reaction of the control plan line. Real and simulated values of the same
//! characteristic are compared side by side.

use serde::{Deserialize, Serialize};
use utsim_quality::charts::{self, Chart, ChartData, ChartKind, Limits};
use utsim_quality::data;
use utsim_quality::rules::RuleSet;

/// Where a data set comes from (the same rule as the Seis Sigma app: always shown).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Origem {
    Real,
    Gemeo,
    Simulado,
}

impl Origem {
    pub fn nome(self) -> &'static str {
        match self {
            Origem::Real => "Real",
            Origem::Gemeo => "Gêmeo",
            Origem::Simulado => "Simulado",
        }
    }
}

/// A data set of one characteristic, in time order, by subgroup.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Conjunto {
    pub nome: String,
    pub origem: Origem,
    pub subgrupos: Vec<Vec<f64>>,
    /// SHA-256 of the bytes read, so the analysis can be traced to the file.
    pub hash: String,
    #[serde(default)]
    pub avisos: Vec<String>,
}

impl Conjunto {
    pub fn valores(&self) -> Vec<f64> {
        self.subgrupos.iter().flatten().copied().collect()
    }

    /// Reads a CSV as `utsim qualidade` does, cutting into subgroups of `subgrupo` values
    /// when given.
    pub fn de_csv(
        nome: &str,
        origem: Origem,
        bytes: &[u8],
        subgrupo: Option<usize>,
    ) -> Result<Conjunto, String> {
        let (texto, latin1) = data::decode(bytes);
        let tabela = data::parse(&texto);
        if tabela.rows.is_empty() {
            return Err(format!("'{nome}' não tem linhas de dados"));
        }
        let (subgrupos, mut avisos) = data::subgroups(&tabela, subgrupo)?;
        if latin1 {
            avisos.insert(0, "arquivo lido como Latin-1 (não está em UTF-8)".into());
        }
        use sha2::{Digest, Sha256};
        let hash = Sha256::digest(bytes)
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect();
        Ok(Conjunto {
            nome: nome.to_string(),
            origem,
            subgrupos,
            hash,
            avisos,
        })
    }

    /// The simulated values of one characteristic (long output of [`crate::simulado`]),
    /// one subgroup per value, in time order of the replication.
    pub fn simulado(nome: &str, valores: &[f64]) -> Conjunto {
        Conjunto {
            nome: nome.to_string(),
            origem: Origem::Simulado,
            subgrupos: valores.iter().map(|v| vec![*v]).collect(),
            hash: String::new(),
            avisos: Vec::new(),
        }
    }

    fn dados(&self) -> ChartData {
        ChartData::Variables(self.subgrupos.clone())
    }

    /// I-MR for single values, X̄-R for subgroups.
    pub fn tipo(&self) -> ChartKind {
        if self.subgrupos.first().is_some_and(|g| g.len() > 1) {
            ChartKind::XbarR
        } else {
            ChartKind::IMr
        }
    }
}

/// Phase I: limits estimated from the data (`excluidos`: subgroups with an assigned cause).
pub fn fase1(c: &Conjunto, regras: &RuleSet, excluidos: &[usize]) -> Result<Chart, String> {
    charts::phase1(c.tipo(), &c.dados(), excluidos, regras)
}

/// Phase II: frozen limits of an earlier study applied to new data.
pub fn fase2(limites: &Limits, c: &Conjunto, regras: &RuleSet) -> Result<Chart, String> {
    charts::apply(limites, &c.dados(), regras, &[])
}

/// A reaction card opened by a signal of the chart.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Cartao {
    /// Subgroup with the signal (1-based, as on the chart).
    pub subgrupo: usize,
    pub painel: String,
    pub regra: String,
    /// The reaction of the control plan line.
    pub reacao: String,
    #[serde(default)]
    pub tratado: bool,
}

/// One card per subgroup with a signal on any panel.
pub fn cartoes(chart: &Chart, reacao: &str) -> Vec<Cartao> {
    let mut out: Vec<Cartao> = Vec::new();
    for p in &chart.panels {
        for v in &p.violations {
            if out.iter().any(|c| c.subgrupo == v.index + 1) {
                continue;
            }
            out.push(Cartao {
                subgrupo: v.index + 1,
                painel: p.name.clone(),
                regra: format!("{:?}", v.rule),
                reacao: reacao.to_string(),
                tratado: false,
            });
        }
    }
    out.sort_by_key(|c| c.subgrupo);
    out
}

/// Real and simulated values of the same characteristic, side by side.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Comparacao {
    pub n_real: usize,
    pub n_simulado: usize,
    pub media_real: f64,
    pub media_simulada: f64,
    pub dp_real: f64,
    pub dp_simulado: f64,
    /// Welch t test of the difference of means (real minus simulated).
    pub diferenca: f64,
    pub ic95: (f64, f64),
    pub p_valor: f64,
    /// Ratio of the standard deviations (real over simulated).
    pub razao_dp: f64,
    /// The model reproduces the real mean (interval of the difference contains zero).
    pub media_compativel: bool,
    /// Residuals of the real values against the simulated mean, in simulated standard
    /// deviations, smoothed by an EWMA (λ = 0.2): a drift shows up as a run away from zero.
    pub ewma_residuos: Vec<f64>,
    /// First real value where the residual EWMA leaves ±3 of its own limit (drift).
    pub deriva_em: Option<usize>,
}

fn media_dp(v: &[f64]) -> (f64, f64) {
    let n = v.len().max(1) as f64;
    let m = v.iter().sum::<f64>() / n;
    let s = if v.len() > 1 {
        (v.iter().map(|x| (x - m).powi(2)).sum::<f64>() / (n - 1.0)).sqrt()
    } else {
        0.0
    };
    (m, s)
}

pub fn comparar(real: &Conjunto, simulado: &Conjunto) -> Result<Comparacao, String> {
    let (r, s) = (real.valores(), simulado.valores());
    if r.len() < 2 || s.len() < 2 {
        return Err("cada conjunto precisa de ao menos 2 valores".into());
    }
    let t = utsim_stats::hypothesis::t_two_sample(
        &r,
        &s,
        0.0,
        false,
        0.95,
        utsim_stats::hypothesis::Alternative::TwoSided,
    )
    .ok_or("teste t não calculado")?;
    let (mr, sr) = media_dp(&r);
    let (ms, ss) = media_dp(&s);
    let lambda: f64 = 0.2;
    // Limit of the EWMA of standardized residuals with the asymptotic variance.
    let limite = 3.0 * (lambda / (2.0 - lambda)).sqrt();
    let mut z = 0.0;
    let mut ewma = Vec::with_capacity(r.len());
    let mut deriva_em = None;
    for (i, x) in r.iter().enumerate() {
        let e = if ss > 0.0 { (x - ms) / ss } else { 0.0 };
        z = lambda * e + (1.0 - lambda) * z;
        ewma.push(z);
        if deriva_em.is_none() && z.abs() > limite {
            deriva_em = Some(i + 1);
        }
    }
    let ic = (t.ci.low, t.ci.high);
    Ok(Comparacao {
        n_real: r.len(),
        n_simulado: s.len(),
        media_real: mr,
        media_simulada: ms,
        dp_real: sr,
        dp_simulado: ss,
        diferenca: t.estimate,
        ic95: ic,
        p_valor: t.p_value,
        razao_dp: if ss > 0.0 { sr / ss } else { f64::NAN },
        media_compativel: ic.0 <= 0.0 && ic.1 >= 0.0,
        ewma_residuos: ewma,
        deriva_em,
    })
}
