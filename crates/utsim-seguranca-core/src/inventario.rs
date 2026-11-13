//! Inventory of hazards and risks per work station, in five steps inspired by the logic of
//! occupational risk management (NR-1, cited only by number and theme):
//!
//! 1. **Identificar** the station, its activities and the people exposed ([`utsim_posto::Posto`]).
//! 2. **Levantar perigos** by group, with source, circumstance and possible injuries ([`Perigo`]).
//! 3. **Avaliar** each risk with the matrix and the existing controls.
//! 4. **Planejar ações** by the hierarchy of control, with owner and deadline ([`Acao`]).
//! 5. **Acompanhar** effectiveness with reassessments and history ([`Reavaliacao`]).

use serde::{Deserialize, Serialize};
use utsim_model::Model;
use utsim_model::safety::{HazardDef, HazardGroup};
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

/// Origin of the hazards read from the node of the model (`safety.hazards`). They are
/// edited on the node (its Segurança tab), not in the unit, and their id is
/// `no:<station>:<hazard>` so that actions and reassessments in the unit can point at them.
pub const ORIGEM_NO: &str = "no";

impl Perigo {
    /// The hazard `h` of the node linked to station `posto`, as the inventory shows it.
    pub fn do_no(posto: &str, h: &HazardDef) -> Perigo {
        Perigo {
            id: format!("{ORIGEM_NO}:{posto}:{}", h.id),
            posto: posto.to_string(),
            grupo: match h.group {
                HazardGroup::Physical => Grupo::Fisico,
                HazardGroup::Chemical => Grupo::Quimico,
                HazardGroup::Biological => Grupo::Biologico,
                HazardGroup::Ergonomic => Grupo::Ergonomico,
                HazardGroup::Accident => Grupo::Acidente,
            },
            descricao: h.description.clone(),
            fonte: h.source.clone(),
            circunstancia: h.circumstance.clone(),
            lesoes: h.injuries.clone(),
            controles: h.controls.clone(),
            probabilidade: h.probability as usize,
            severidade: h.severity as usize,
            origem: ORIGEM_NO.to_string(),
        }
    }

    /// The hazard as it goes to the node (`safety.hazards`): the same fields, the id is
    /// given by the node.
    pub fn para_o_no(&self) -> HazardDef {
        HazardDef {
            group: match self.grupo {
                Grupo::Fisico => HazardGroup::Physical,
                Grupo::Quimico => HazardGroup::Chemical,
                Grupo::Biologico => HazardGroup::Biological,
                Grupo::Ergonomico => HazardGroup::Ergonomic,
                Grupo::Acidente => HazardGroup::Accident,
            },
            description: self.descricao.clone(),
            source: self.fonte.clone(),
            circumstance: self.circunstancia.clone(),
            injuries: self.lesoes.clone(),
            controls: self.controles.clone(),
            probability: self.probabilidade.min(u8::MAX as usize) as u8,
            severity: self.severidade.min(u8::MAX as usize) as u8,
            origin: self.origem.clone(),
            ..HazardDef::default()
        }
    }

    /// Read from the node: edited there, not in the unit.
    pub fn eh_do_no(&self) -> bool {
        self.origem == ORIGEM_NO
    }
}

/// The hazards of the node that station `p` is (`Posto::objeto`) in `model`. Empty when
/// the station names no object or the model does not have it.
pub fn perigos_do_no(model: &Model, p: &Posto) -> Vec<Perigo> {
    let Some(nome) = p.objeto.as_deref() else {
        return Vec::new();
    };
    model
        .objects
        .iter()
        .find(|o| o.name == nome)
        .and_then(|o| o.safety.as_deref())
        .map(|s| s.hazards.iter().map(|h| Perigo::do_no(&p.id, h)).collect())
        .unwrap_or_default()
}

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

    /// Adds the hazards of the nodes linked to the stations (`Posto::modelo` and `objeto`).
    /// `modelo` gives the model of a station (`None` when it has none or it cannot be
    /// read), so the caller decides how to read and cache the files.
    pub fn com_nos<'m>(
        mut self,
        mut modelo: impl FnMut(&Posto) -> Option<&'m Model>,
    ) -> Inventario {
        let mut novos = Vec::new();
        for p in &self.postos {
            if let Some(m) = modelo(p) {
                novos.extend(perigos_do_no(m, p));
            }
        }
        self.perigos.extend(novos);
        self
    }

    /// [`Inventario::ler`] with the hazards of the linked nodes, each model file read once
    /// (a model that cannot be read is left out). For a one-off use, like the report.
    pub fn ler_com_nos(u: &Unidade) -> Inventario {
        let inv = Inventario::ler(u);
        let mut modelos = std::collections::BTreeMap::new();
        for p in &inv.postos {
            if let Some(m) = &p.modelo {
                let c = u.caminho_modelo(m);
                modelos
                    .entry(c.clone())
                    .or_insert_with(|| Model::load(&c).ok());
            }
        }
        inv.com_nos(|p| {
            p.modelo
                .as_ref()
                .and_then(|m| modelos.get(&u.caminho_modelo(m)))
                .and_then(|m| m.as_ref())
        })
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

/// After unit hazard `antigo` was copied to the node as `novo` (`no:<station>:<hazard>`):
/// its actions and reassessments now point at `novo` and the unit record goes. Returns how
/// many actions and reassessments moved.
pub fn passar_ao_no(u: &mut Unidade, antigo: &str, novo: &str) -> Result<usize, UnidadeErro> {
    let mut n = 0;
    for col in [ACOES, REAVALIACOES] {
        let mudar: Vec<(String, serde_json::Value)> = u
            .estado()
            .brutos(col)
            .filter(|(_, v)| v.get("perigo").and_then(|x| x.as_str()) == Some(antigo))
            .map(|(id, v)| {
                let mut v = v.clone();
                v["perigo"] = serde_json::Value::String(novo.to_string());
                (id.clone(), v)
            })
            .collect();
        for (id, v) in mudar {
            u.gravar(col, &id, &v)?;
            n += 1;
        }
    }
    u.remover(PERIGOS, antigo)?;
    Ok(n)
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
