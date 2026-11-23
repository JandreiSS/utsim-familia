//! Near misses, incidents and safety indicators (stage QS16 of
//! `docs/plano_qualidade_e_seguranca.md`, section 4.7).
//!
//! People are recorded only by the code of the restricted folder (`utsim_posto::restrita`,
//! LGPD). The indicators use a configurable base of exposure hours; the days charged by a
//! technical standard are not embedded: the company types the days it charges for each
//! case. Monthly counts go on a `c` or `u` control chart of `utsim-quality`.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use utsim_ferramentas::pareto::{Pareto, pareto};

pub const INCIDENTES: &str = "incidentes";
pub const HORAS: &str = "horas_homem";
pub const CONFIG_INDICADORES: &str = "indicadores";

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum Tipo {
    #[default]
    QuaseAcidente,
    SemAfastamento,
    ComAfastamento,
    DanoMaterial,
}

impl Tipo {
    pub const TODOS: [Tipo; 4] = [
        Tipo::QuaseAcidente,
        Tipo::SemAfastamento,
        Tipo::ComAfastamento,
        Tipo::DanoMaterial,
    ];

    pub fn nome(self) -> &'static str {
        match self {
            Tipo::QuaseAcidente => "Quase-acidente",
            Tipo::SemAfastamento => "Incidente sem afastamento",
            Tipo::ComAfastamento => "Incidente com afastamento",
            Tipo::DanoMaterial => "Dano material",
        }
    }

    /// Whether it injured someone (counts in the frequency rate).
    pub fn com_lesao(self) -> bool {
        matches!(self, Tipo::SemAfastamento | Tipo::ComAfastamento)
    }
}

/// A recorded event.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Incidente {
    #[serde(default)]
    pub id: String,
    /// `AAAA-MM-DD`.
    pub data: String,
    #[serde(default)]
    pub hora: String,
    #[serde(default)]
    pub posto: String,
    #[serde(default)]
    pub zona: String,
    #[serde(default)]
    pub tipo: Tipo,
    pub descricao: String,
    /// Causes, one per item (for the Pareto).
    #[serde(default)]
    pub causas: Vec<String>,
    #[serde(default)]
    pub acoes: String,
    /// Hazard of the inventory it relates to.
    #[serde(default)]
    pub perigo: Option<String>,
    /// People involved, by code of the restricted folder only (`P-...`).
    #[serde(default)]
    pub pessoas: Vec<String>,
    /// Days away from work (with absence).
    #[serde(default)]
    pub dias_perdidos: f64,
    /// Days charged for permanent effects, typed by the company from the table it uses.
    #[serde(default)]
    pub dias_debitados: f64,
}

impl Incidente {
    /// Problems before saving: people only by code, date set.
    pub fn problemas(&self) -> Vec<String> {
        let mut out = Vec::new();
        if self.data.len() != 10 {
            out.push("a data deve ser AAAA-MM-DD".into());
        }
        for p in &self.pessoas {
            if !p.starts_with("P-") {
                out.push(format!(
                    "'{p}' não é um código de pessoa: cadastre a pessoa na pasta restrita e use o código"
                ));
            }
        }
        out
    }

    pub fn mes(&self) -> &str {
        self.data.get(..7).unwrap_or("")
    }
}

/// Whether `mes` is a month `AAAA-MM`: four digits, a dash and 01 to 12 (`2025/03` and
/// `2025-13` are not).
pub fn mes_valido(mes: &str) -> bool {
    let b = mes.as_bytes();
    b.len() == 7
        && b[4] == b'-'
        && b[..4].iter().all(u8::is_ascii_digit)
        && b[5..].iter().all(u8::is_ascii_digit)
        && matches!(mes[5..].parse::<u32>(), Ok(1..=12))
}

/// Exposure hours of one month (`AAAA-MM`).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct HorasHomem {
    #[serde(default)]
    pub id: String,
    pub mes: String,
    pub horas: f64,
}

/// Base of the rates: events per `base` exposure hours (1 000 000 by default).
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Config {
    pub base: f64,
}

impl Default for Config {
    fn default() -> Self {
        Config { base: 1_000_000.0 }
    }
}

/// The indicators over a period.
#[derive(Clone, Debug, PartialEq)]
pub struct Indicadores {
    pub horas: f64,
    pub por_tipo: BTreeMap<Tipo, usize>,
    /// Injuries (with and without absence) per `base` hours.
    pub taxa_frequencia: f64,
    /// Injuries with absence per `base` hours.
    pub taxa_frequencia_afastamento: f64,
    /// Days lost and charged per `base` hours.
    pub taxa_gravidade: f64,
    /// Near misses per injury (`None` without injuries).
    pub razao_quase: Option<f64>,
    /// Days since the last injury, on `hoje`.
    pub dias_sem_acidente: Option<i64>,
    pub pareto_causas: Pareto,
    pub pareto_postos: Pareto,
    /// Events by zone of the layout (the map of incidents).
    pub por_zona: BTreeMap<String, usize>,
    /// By month: (month, events with injury, exposure hours).
    pub mensal: Vec<(String, f64, f64)>,
}

fn dias(data: &str) -> Option<i64> {
    // Days since 1970-01-01 of `AAAA-MM-DD` (proleptic Gregorian, civil calendar).
    let y: i64 = data.get(..4)?.parse().ok()?;
    let m: i64 = data.get(5..7)?.parse().ok()?;
    let d: i64 = data.get(8..10)?.parse().ok()?;
    let y = if m <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let mp = (m + 9) % 12;
    let doy = (153 * mp + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    Some(era * 146_097 + doe - 719_468)
}

/// Indicators of the events between `de` and `ate` (inclusive, `AAAA-MM-DD`) with the
/// exposure hours of the months in the period.
pub fn indicadores(
    incidentes: &[Incidente],
    horas: &[HorasHomem],
    config: Config,
    de: &str,
    ate: &str,
    hoje: &str,
) -> Indicadores {
    let no_periodo: Vec<&Incidente> = incidentes
        .iter()
        .filter(|i| i.data.as_str() >= de && i.data.as_str() <= ate)
        .collect();
    let mes_de = de.get(..7).unwrap_or("");
    let mes_ate = ate.get(..7).unwrap_or("");
    let hh: Vec<&HorasHomem> = horas
        .iter()
        .filter(|h| h.mes.as_str() >= mes_de && h.mes.as_str() <= mes_ate)
        .collect();
    let total_h: f64 = hh.iter().map(|h| h.horas).sum();
    let taxa = |n: f64| {
        if total_h > 0.0 {
            n * config.base / total_h
        } else {
            0.0
        }
    };
    let mut por_tipo = BTreeMap::new();
    for i in &no_periodo {
        *por_tipo.entry(i.tipo).or_insert(0) += 1;
    }
    let lesoes = no_periodo.iter().filter(|i| i.tipo.com_lesao()).count() as f64;
    let afast = no_periodo
        .iter()
        .filter(|i| i.tipo == Tipo::ComAfastamento)
        .count() as f64;
    let dias_tot: f64 = no_periodo
        .iter()
        .map(|i| i.dias_perdidos + i.dias_debitados)
        .sum();
    let quase = no_periodo
        .iter()
        .filter(|i| i.tipo == Tipo::QuaseAcidente)
        .count() as f64;
    let ultimo = incidentes
        .iter()
        .filter(|i| i.tipo.com_lesao() && i.data.as_str() <= hoje)
        .filter_map(|i| dias(&i.data))
        .max();
    let dias_sem_acidente = match (ultimo, dias(hoje)) {
        (Some(u), Some(h)) => Some(h - u),
        _ => None,
    };
    let mut causas: Vec<(String, f64)> = Vec::new();
    let mut postos: Vec<(String, f64)> = Vec::new();
    let mut por_zona = BTreeMap::new();
    for i in &no_periodo {
        for c in &i.causas {
            causas.push((c.clone(), 1.0));
        }
        if !i.posto.is_empty() {
            postos.push((i.posto.clone(), 1.0));
        }
        if !i.zona.is_empty() {
            *por_zona.entry(i.zona.clone()).or_insert(0) += 1;
        }
    }
    let mut mensal: BTreeMap<String, (f64, f64)> = BTreeMap::new();
    for h in &hh {
        mensal.entry(h.mes.clone()).or_default().1 += h.horas;
    }
    for i in &no_periodo {
        let e = mensal.entry(i.mes().to_string()).or_default();
        if i.tipo.com_lesao() {
            e.0 += 1.0;
        }
    }
    Indicadores {
        horas: total_h,
        por_tipo,
        taxa_frequencia: taxa(lesoes),
        taxa_frequencia_afastamento: taxa(afast),
        taxa_gravidade: taxa(dias_tot),
        razao_quase: (lesoes > 0.0).then(|| quase / lesoes),
        dias_sem_acidente,
        pareto_causas: pareto(&causas, 0.0),
        pareto_postos: pareto(&postos, 0.0),
        por_zona,
        mensal: mensal.into_iter().map(|(m, (n, h))| (m, n, h)).collect(),
    }
}

/// Monthly injuries on a `u` chart (events per `base` hours, the hours of each month as the
/// size), from `utsim-quality`. `None` with fewer than two months with hours.
pub fn carta_mensal(ind: &Indicadores, config: Config) -> Option<utsim_quality::Chart> {
    use utsim_quality::charts::{ChartData, ChartKind, phase1};
    let meses: Vec<&(String, f64, f64)> = ind.mensal.iter().filter(|m| m.2 > 0.0).collect();
    if meses.len() < 2 {
        return None;
    }
    let data = ChartData::Attributes {
        counts: meses.iter().map(|m| m.1).collect(),
        sizes: meses.iter().map(|m| m.2 / config.base).collect(),
    };
    phase1(
        ChartKind::U,
        &data,
        &[],
        &utsim_quality::rules::RuleSet::western_electric(),
    )
    .ok()
}
