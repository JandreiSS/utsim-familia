//! The curriculum per level (`docs/plano_lean_seis_sigma.md`, 8.5; decision MB7): modules
//! with workload, learning objectives, prerequisites, the **tools of the app** they teach
//! (the keys of the competence map, [`crate::mentoria::all_tools`]) and the exercise
//! packages (stage L8). The default structure follows the DMAIC phases and is written for
//! UTSim; the company edits it, and its own teaching material goes as attachments of the
//! modules. Nothing here reproduces a book or a protected manual.

use serde_json::Value;

use super::{fields as tf, ids_in, ids_value, strings_in};
use crate::domain::enum_value;
use crate::id::Id;
use crate::op::{Edit, EntityRef, Kind};
use crate::programa::BeltLevel;
use crate::state::{Entity, State};

pub mod fields {
    pub const ORDER: &str = "ordem";
    pub const OBJECTIVES: &str = "objetivos";
    pub const PREREQUISITES: &str = "pre_requisitos";
    pub const TOOLS: &str = "ferramentas";
    /// Exercise packages (`.lssx`, stage L8) of the module, one per line.
    pub const EXERCISES: &str = "exercicios";
    /// Teaching material of the company attached to the module (attachment hashes).
    pub const MATERIAL: &str = "material";
}

#[derive(Debug, Clone, PartialEq)]
pub struct Module {
    pub id: Id,
    pub level: BeltLevel,
    pub order: i64,
    pub name: String,
    pub hours: f64,
    pub objectives: String,
    pub prerequisites: Vec<Id>,
    pub tools: Vec<String>,
    pub exercises: String,
}

impl Module {
    pub fn read(id: Id, e: &Entity) -> Self {
        Self {
            id,
            level: e.typed(tf::LEVEL).unwrap_or_default(),
            order: e.integer(fields::ORDER).unwrap_or(0),
            name: e.text(tf::NAME),
            hours: e.number(tf::HOURS).unwrap_or(0.0).max(0.0),
            objectives: e.text(fields::OBJECTIVES),
            prerequisites: ids_in(e, fields::PREREQUISITES),
            tools: strings_in(e, fields::TOOLS),
            exercises: e.text(fields::EXERCISES),
        }
    }

    pub fn entity_ref(id: Id) -> EntityRef {
        EntityRef::new(Kind::CurriculumModule, id)
    }
}

/// The modules of the program, by level and order.
pub fn modules(state: &State, program: Id) -> Vec<Module> {
    let key = program.to_string();
    let mut v: Vec<Module> = state
        .of_kind(Kind::CurriculumModule)
        .filter(|(_, e)| !e.is_deleted() && e.text(tf::PROGRAM) == key)
        .map(|(id, e)| Module::read(id, e))
        .collect();
    v.sort_by_key(|m| (m.level, m.order, m.id));
    v
}

pub fn of_level(modules: &[Module], level: BeltLevel) -> Vec<&Module> {
    modules.iter().filter(|m| m.level == level).collect()
}

/// Total hours of a level.
pub fn hours_of(modules: &[Module], level: BeltLevel) -> f64 {
    of_level(modules, level).iter().map(|m| m.hours).sum()
}

/// A module of the default structure.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Template {
    pub name: &'static str,
    pub hours: f64,
    pub objectives: &'static str,
    pub tools: &'static [&'static str],
}

const YELLOW: [Template; 4] = [
    Template {
        name: "Fundamentos do Lean Seis Sigma",
        hours: 4.0,
        objectives: "Explicar variação, desperdício e o papel de cada belt no programa.",
        tools: &[],
    },
    Template {
        name: "DMAIC e os desperdícios",
        hours: 4.0,
        objectives: "Situar um problema nas fases do DMAIC e reconhecer os desperdícios no processo.",
        tools: &["cinco_s"],
    },
    Template {
        name: "Pareto, Ishikawa e 5 porquês",
        hours: 4.0,
        objectives: "Priorizar causas com Pareto, organizá-las no Ishikawa e aprofundar com 5 porquês.",
        tools: &["pareto", "ishikawa", "cinco_porques"],
    },
    Template {
        name: "Ações e acompanhamento",
        hours: 4.0,
        objectives: "Registrar ações no quadro, acompanhar prazos e conferir a eficácia.",
        tools: &["acoes"],
    },
];

const GREEN: [Template; 7] = [
    Template {
        name: "Definir",
        hours: 12.0,
        objectives: "Escrever o termo de abertura, ouvir o cliente (VOC e CTQ), desenhar o SIPOC e planejar o projeto.",
        tools: &["termo", "voc_ctq", "sipoc", "cronograma"],
    },
    Template {
        name: "Medir",
        hours: 16.0,
        objectives: "Planejar a coleta, validar o sistema de medição e calcular a capacidade e a linha de base.",
        tools: &["plano_coleta", "msa", "capacidade"],
    },
    Template {
        name: "Analisar",
        hours: 16.0,
        objectives: "Levantar e priorizar causas e comprová-las com testes de hipótese.",
        tools: &[
            "pareto",
            "ishikawa",
            "cinco_porques",
            "matriz_ce",
            "testes_hipotese",
        ],
    },
    Template {
        name: "Melhorar",
        hours: 12.0,
        objectives: "Escolher e testar soluções, analisar riscos com o FMEA e testar no modelo de simulação antes da fábrica.",
        tools: &["fmea"],
    },
    Template {
        name: "Controlar",
        hours: 12.0,
        objectives: "Manter o ganho com cartas de controle, plano de controle e plano de resposta.",
        tools: &["cep", "plano_controle", "plano_resposta"],
    },
    Template {
        name: "Gestão do projeto",
        hours: 8.0,
        objectives: "Conduzir os tollgates, o quadro de ações, os riscos e o relatório A3.",
        tools: &["acoes", "a3"],
    },
    Template {
        name: "Simulação como instrumento",
        hours: 4.0,
        objectives: "Usar o modelo UTSim para medir, analisar e testar cenários com repetições.",
        tools: &[],
    },
];

const BLACK: [Template; 6] = [
    Template {
        name: "Estatística ampliada",
        hours: 24.0,
        objectives: "Escolher o teste certo para cada pergunta, com poder e tamanho de amostra.",
        tools: &["testes_hipotese", "capacidade"],
    },
    Template {
        name: "Regressão",
        hours: 16.0,
        objectives: "Modelar o Y pelos X com regressão múltipla e seus diagnósticos.",
        tools: &["regressao"],
    },
    Template {
        name: "Planejamento de experimentos",
        hours: 24.0,
        objectives: "Planejar e analisar fatoriais, fracionados e superfície de resposta.",
        tools: &["doe"],
    },
    Template {
        name: "DMADV e fluxo de valor",
        hours: 16.0,
        objectives: "Projetar um processo novo e mapear o fluxo de valor atual e futuro.",
        tools: &["vsm", "kaizen"],
    },
    Template {
        name: "Liderança da mudança",
        hours: 8.0,
        objectives: "Engajar as partes interessadas e sustentar a mudança.",
        tools: &[],
    },
    Template {
        name: "Mentoria de green belts",
        hours: 8.0,
        objectives: "Acompanhar green belts com o diário de mentoria e as revisões de tollgate.",
        tools: &[],
    },
];

/// The default modules of a level.
pub fn default_templates(level: BeltLevel) -> &'static [Template] {
    match level {
        BeltLevel::Yellow => &YELLOW,
        BeltLevel::Green => &GREEN,
        BeltLevel::Black => &BLACK,
        BeltLevel::MasterBlack => &[],
    }
}

/// Edits of a new module.
#[allow(clippy::too_many_arguments)]
pub fn add_module(
    program: Id,
    id: Id,
    level: BeltLevel,
    order: i64,
    name: &str,
    hours: f64,
    objectives: &str,
    tools: &[&str],
    prerequisites: &[Id],
) -> Vec<Edit> {
    let r = Module::entity_ref(id);
    vec![
        Edit::new(r, tf::PROGRAM, program.to_string()),
        Edit::new(r, tf::LEVEL, enum_value(level)),
        Edit::new(r, fields::ORDER, order),
        Edit::new(r, tf::NAME, name.trim()),
        Edit::new(r, tf::HOURS, hours.max(0.0)),
        Edit::new(r, fields::OBJECTIVES, objectives.trim()),
        Edit::new(
            r,
            fields::TOOLS,
            Value::from(tools.iter().map(|t| Value::from(*t)).collect::<Vec<_>>()),
        ),
        Edit::new(r, fields::PREREQUISITES, ids_value(prerequisites)),
    ]
}

/// Edits that install the default structure of a level: each module requires the one
/// before it.
pub fn install_default(program: Id, level: BeltLevel, mut ids: impl FnMut() -> Id) -> Vec<Edit> {
    let mut e = Vec::new();
    let mut prev: Option<Id> = None;
    for (i, t) in default_templates(level).iter().enumerate() {
        let id = ids();
        let pre: Vec<Id> = prev.into_iter().collect();
        e.extend(add_module(
            program,
            id,
            level,
            i as i64 + 1,
            t.name,
            t.hours,
            t.objectives,
            t.tools,
            &pre,
        ));
        prev = Some(id);
    }
    e
}

/// Edit that sets the tools of a module.
pub fn set_tools(module: Id, tools: &[String]) -> Edit {
    Edit::new(
        Module::entity_ref(module),
        fields::TOOLS,
        Value::from(
            tools
                .iter()
                .map(|t| Value::from(t.as_str()))
                .collect::<Vec<_>>(),
        ),
    )
}

/// Prerequisites not given in the sessions before the first session of `module`: the
/// order of the sessions breaks the curriculum. `(module, missing prerequisite)`.
pub fn order_problems(modules: &[Module], sessions: &[&super::ClassSession]) -> Vec<(Id, Id)> {
    let first = |m: Id| {
        sessions
            .iter()
            .filter(|s| s.modules.contains(&m))
            .filter_map(|s| s.date)
            .min()
    };
    let mut v = Vec::new();
    for m in modules {
        let Some(f) = first(m.id) else { continue };
        for p in &m.prerequisites {
            if first(*p).is_none_or(|d| d > f) {
                v.push((m.id, *p));
            }
        }
    }
    v
}
