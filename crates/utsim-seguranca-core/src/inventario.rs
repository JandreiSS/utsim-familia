//! Inventory of hazards and risks per work station, in five steps inspired by the logic of
//! occupational risk management (NR-1, cited only by number and theme):
//!
//! 1. **Identificar** the station, its activities and the people exposed ([`utsim_posto::Posto`]).
//! 2. **Levantar perigos** by group, with source, circumstance and possible injuries ([`Perigo`]).
//! 3. **Avaliar** each risk with the matrix and the existing controls.
//! 4. **Planejar ações** by the hierarchy of control, with owner and deadline ([`Acao`]).
//! 5. **Acompanhar** effectiveness with reassessments and history ([`Reavaliacao`]).

use serde::{Deserialize, Serialize};
use utsim_posto::{Posto, Unidade, UnidadeErro};

use crate::matriz::Matriz;

pub const PERIGOS: &str = "perigos";
pub const ACOES: &str = "acoes";
pub const REAVALIACOES: &str = "reavaliacoes";
/// Collection with one record, `matriz`, holding the matrix of the unit.
pub const CONFIG: &str = "config_seguranca";
pub const MATRIZ: &str = "matriz";

/// Group of the hazard.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum Grupo {
    Fisico,
    Quimico,
    Biologico,
    Ergonomico,
    #[default]
    Acidente,
}

impl Grupo {
    pub const TODOS: [Grupo; 5] = [
        Grupo::Fisico,
        Grupo::Quimico,
        Grupo::Biologico,
        Grupo::Ergonomico,
        Grupo::Acidente,
    ];

    pub fn nome(self) -> &'static str {
        match self {
            Grupo::Fisico => "Físico",
            Grupo::Quimico => "Químico",
            Grupo::Biologico => "Biológico",
            Grupo::Ergonomico => "Ergonômico",
            Grupo::Acidente => "Acidente e mecânico",
        }
    }
}

/// A hazard of a station and its assessment.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Perigo {
    #[serde(default)]
    pub id: String,
    pub posto: String,
    #[serde(default)]
    pub grupo: Grupo,
    pub descricao: String,
    #[serde(default)]
    pub fonte: String,
    #[serde(default)]
    pub circunstancia: String,
    #[serde(default)]
    pub lesoes: String,
    /// Controls already in place.
    #[serde(default)]
    pub controles: String,
    /// Steps of the matrix, from 0 (lowest).
    #[serde(default)]
    pub probabilidade: usize,
    #[serde(default)]
    pub severidade: usize,
    /// Where the hazard came from (`"manual"`, `"exposicao"`, `"checklist"`).
    #[serde(default)]
    pub origem: String,
}

/// Hierarchy of control, from the most effective.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum Hierarquia {
    Eliminar,
    Substituir,
    #[default]
    Engenharia,
    Administrativo,
    Epi,
}

impl Hierarquia {
    pub const TODAS: [Hierarquia; 5] = [
        Hierarquia::Eliminar,
        Hierarquia::Substituir,
        Hierarquia::Engenharia,
        Hierarquia::Administrativo,
        Hierarquia::Epi,
    ];

    pub fn nome(self) -> &'static str {
        match self {
            Hierarquia::Eliminar => "Eliminar o perigo",
            Hierarquia::Substituir => "Substituir",
            Hierarquia::Engenharia => "Controle de engenharia",
            Hierarquia::Administrativo => "Controle administrativo",
            Hierarquia::Epi => "EPI",
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Situacao {
    #[default]
    Aberta,
    EmAndamento,
    Concluida,
    Cancelada,
}

impl Situacao {
    pub const TODAS: [Situacao; 4] = [
        Situacao::Aberta,
        Situacao::EmAndamento,
        Situacao::Concluida,
        Situacao::Cancelada,
    ];

    pub fn nome(self) -> &'static str {
        match self {
            Situacao::Aberta => "Aberta",
            Situacao::EmAndamento => "Em andamento",
            Situacao::Concluida => "Concluída",
            Situacao::Cancelada => "Cancelada",
        }
    }
}

/// An action of the plan.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Acao {
    #[serde(default)]
    pub id: String,
    pub perigo: String,
    #[serde(default)]
    pub hierarquia: Hierarquia,
    pub descricao: String,
    #[serde(default)]
    pub responsavel: String,
    /// Deadline `AAAA-MM-DD`.
    #[serde(default)]
    pub prazo: String,
    #[serde(default)]
    pub situacao: Situacao,
    /// Checked effective after done.
    #[serde(default)]
    pub eficaz: Option<bool>,
}

/// A later assessment of a hazard (step 5).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Reavaliacao {
    #[serde(default)]
    pub id: String,
    pub perigo: String,
    /// Date `AAAA-MM-DD`.
    pub data: String,
    pub probabilidade: usize,
    pub severidade: usize,
    #[serde(default)]
    pub nota: String,
}

/// The five steps, for the screen and the report.
pub const ETAPAS: [&str; 5] = [
    "1. Identificar o posto",
    "2. Levantar perigos",
    "3. Avaliar riscos",
    "4. Planejar ações",
    "5. Acompanhar",
];

/// Everything of the safety inventory of a unit, read from its state.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Inventario {
    pub matriz: Matriz,
    pub postos: Vec<Posto>,
    pub perigos: Vec<Perigo>,
    pub acoes: Vec<Acao>,
    pub reavaliacoes: Vec<Reavaliacao>,
}

impl Inventario {
    pub fn ler(u: &Unidade) -> Inventario {
        let e = u.estado();
        Inventario {
            matriz: e.um(CONFIG, MATRIZ).unwrap_or_default(),
            postos: u.postos(),
            perigos: e.lista(PERIGOS),
            acoes: e.lista(ACOES),
            reavaliacoes: e.lista(REAVALIACOES),
        }
    }

    pub fn perigos_do_posto(&self, posto: &str) -> Vec<&Perigo> {
        self.perigos.iter().filter(|p| p.posto == posto).collect()
    }

    pub fn acoes_do_perigo(&self, perigo: &str) -> Vec<&Acao> {
        let mut v: Vec<&Acao> = self.acoes.iter().filter(|a| a.perigo == perigo).collect();
        v.sort_by_key(|a| a.hierarquia);
        v
    }

    /// The last reassessment of a hazard, if any (latest date).
    pub fn ultima_reavaliacao(&self, perigo: &str) -> Option<&Reavaliacao> {
        self.reavaliacoes
            .iter()
            .filter(|r| r.perigo == perigo)
            .max_by(|a, b| a.data.cmp(&b.data).then(a.id.cmp(&b.id)))
    }

    /// Current level of a hazard: the last reassessment, or the first assessment.
    pub fn nivel_atual(&self, p: &Perigo) -> usize {
        match self.ultima_reavaliacao(&p.id) {
            Some(r) => self.matriz.nivel(r.probabilidade, r.severidade),
            None => self.matriz.nivel(p.probabilidade, p.severidade),
        }
    }

    /// Hazards whose level asks for an action (above the lowest) and have none open or
    /// done.
    pub fn sem_acao(&self) -> Vec<&Perigo> {
        self.perigos
            .iter()
            .filter(|p| self.nivel_atual(p) > 0)
            .filter(|p| {
                !self
                    .acoes_do_perigo(&p.id)
                    .iter()
                    .any(|a| a.situacao != Situacao::Cancelada)
            })
            .collect()
    }

    /// Actions past the deadline on `hoje` (`AAAA-MM-DD`) and not finished.
    pub fn atrasadas(&self, hoje: &str) -> Vec<&Acao> {
        self.acoes
            .iter()
            .filter(|a| {
                matches!(a.situacao, Situacao::Aberta | Situacao::EmAndamento)
                    && !a.prazo.is_empty()
                    && a.prazo.as_str() < hoje
            })
            .collect()
    }
}

/// Saves the matrix of the unit (it replaces the previous one in the whole register).
pub fn gravar_matriz(u: &mut Unidade, m: &Matriz) -> Result<(), UnidadeErro> {
    let p = m.problemas();
    if !p.is_empty() {
        return Err(UnidadeErro(p.join("\n")));
    }
    u.gravar(CONFIG, MATRIZ, m).map(|_| ())
}

/// Hazards suggested by the exposure of a simulated model: one per zone where some
/// person of a station stayed, with the missing equipment as circumstance. Marked
/// `origem: "exposicao"` and never assessed (probability and severity stay to the user).
pub fn perigos_da_exposicao(
    postos: &[Posto],
    exposicao: &utsim_exposicao::Exposure,
    model: &utsim_model::Model,
) -> Vec<Perigo> {
    let mut out = Vec::new();
    for z in exposicao.zones.iter().filter(|z| z.time > 0.0) {
        let atendidos: Vec<String> = model
            .served_by(&z.person)
            .iter()
            .map(|o| o.name.clone())
            .collect();
        for posto in postos.iter().filter(|p| {
            p.objeto
                .as_ref()
                .is_some_and(|o| atendidos.contains(o) || *o == z.person)
        }) {
            let grupo = match z.kind {
                utsim_model::ZoneKind::Noise | utsim_model::ZoneKind::Heat => Grupo::Fisico,
                _ => Grupo::Acidente,
            };
            out.push(Perigo {
                id: String::new(),
                posto: posto.id.clone(),
                grupo,
                descricao: format!("Exposição na zona '{}' ({:?})", z.zone, z.kind),
                fonte: format!("zona de risco '{}' do modelo", z.zone),
                circunstancia: if z.missing_ppe.is_empty() {
                    format!("{} entrou {} vezes na simulação", z.person, z.entries)
                } else {
                    format!(
                        "{} entrou {} vezes sem {} na simulação",
                        z.person,
                        z.entries,
                        z.missing_ppe.join(", ")
                    )
                },
                lesoes: String::new(),
                controles: String::new(),
                probabilidade: 0,
                severidade: 0,
                origem: "exposicao".into(),
            });
        }
    }
    out
}
