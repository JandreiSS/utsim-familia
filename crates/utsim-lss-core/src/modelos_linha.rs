//! Project templates of the specialised lines (`docs/plano_linhas_especializadas.md`, 1.8
//! and step 13): "Kaizen administrativo" and "DMAIC transacional" for the Lean Office line
//! (administrative processes). A template is an approach plus the extra checklist items of
//! each phase's tollgate, with the deliverables the line produces (administrative VSM,
//! %C&A measured per step, swimlanes by department, the line's report). The project
//! records the template's id in its `modelo` field, set right after it is created; a
//! project without one keeps the approach's checklists.

use crate::domain::{Approach, fields};
use crate::id::Id;
use crate::op::{Edit, EntityRef, Kind};
use crate::tollgate::{Check, Template};

/// One project template of a line.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ProjectModel {
    /// Stored in the project's `modelo` field.
    pub id: &'static str,
    pub name: &'static str,
    /// The approach the project is created with (its phases).
    pub approach: Approach,
    /// The line it belongs to (`Model.line` of the simulation models it imports).
    pub line: &'static str,
    pub description: &'static str,
}

/// The templates, in the order the app offers them.
pub const MODELS: [ProjectModel; 2] = [
    ProjectModel {
        id: "kaizen_administrativo",
        name: "Kaizen administrativo (Lean Office)",
        approach: Approach::Kaizen,
        line: "lean_office",
        description: "Evento de 5 dias sobre um processo administrativo: mapa do estado atual, \
                      estado futuro simulado e padrão novo.",
    },
    ProjectModel {
        id: "dmaic_transacional",
        name: "DMAIC transacional (Lean Office)",
        approach: Approach::Dmaic,
        line: "lean_office",
        description: "DMAIC de um processo administrativo, com lead time, %C&A por etapa, \
                      esperas por causa e passagens entre departamentos como medidas.",
    },
];

/// The template called `id`.
pub fn find(id: &str) -> Option<&'static ProjectModel> {
    MODELS.iter().find(|m| m.id == id)
}

/// The edit that records the template of a project.
pub fn set(project: Id, model: &ProjectModel) -> Edit {
    Edit::new(
        EntityRef::new(Kind::Project, project),
        fields::project::MODEL,
        model.id,
    )
}

/// The checklist items a template adds to the tollgate of phase `index` (0-based), after
/// the approach's own.
pub fn extra_checklist(model: &str, index: usize) -> Vec<Template> {
    match (model, index) {
        ("kaizen_administrativo", 0) => vec![
            (
                "Processo administrativo escolhido e estado atual mapeado (VSM administrativo do modelo da linha Lean Office)",
                true,
                None,
            ),
            (
                "Volume de casos, lead time e %C&A de cada etapa medidos antes do evento",
                true,
                None,
            ),
            (
                "Agenda dos 5 dias combinada com as áreas que o processo atravessa",
                false,
                None,
            ),
        ],
        ("kaizen_administrativo", 1) => vec![
            (
                "Estado futuro simulado e comparado com o atual (relatório da linha com --comparar)",
                true,
                None,
            ),
            (
                "Fluxograma de raias por departamento revisado com as áreas",
                true,
                None,
            ),
        ],
        ("kaizen_administrativo", 2) => vec![(
            "Lead time, % dentro do SLA e WIP de casos acompanhados depois do evento",
            true,
            None,
        )],
        ("dmaic_transacional", 0) => vec![
            (
                "SIPOC e fluxograma de raias por departamento do processo administrativo",
                true,
                None,
            ),
            (
                "Caso definido (o que anda pelo processo) e prazo do cliente (SLA)",
                true,
                Some(Check::CharterComplete),
            ),
        ],
        ("dmaic_transacional", 1) => vec![
            (
                "%C&A medido em cada etapa, numa amostra de casos",
                true,
                None,
            ),
            (
                "Linha de base de lead time, touch time e razão de atividade (relatório da linha Lean Office)",
                true,
                None,
            ),
        ],
        ("dmaic_transacional", 2) => vec![(
            "Pareto das esperas por causa e passagens entre departamentos como evidência na matriz C&E",
            true,
            None,
        )],
        ("dmaic_transacional", 3) => vec![(
            "Estado futuro simulado e comparado com o atual (VSM administrativo atual e futuro)",
            true,
            None,
        )],
        ("dmaic_transacional", 4) => vec![(
            "Lead time, % dentro do SLA e WIP de casos nas cartas do plano de controle",
            true,
            None,
        )],
        _ => Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn templates_have_items_for_every_phase_of_their_approach() {
        for m in MODELS {
            assert_eq!(find(m.id), Some(&m));
            for i in 0..m.approach.phases().len() {
                let extra = extra_checklist(m.id, i);
                assert!(!extra.is_empty(), "{} fase {i}", m.id);
                assert!(
                    extra.iter().any(|t| t.1),
                    "{} fase {i}: nada obrigatório",
                    m.id
                );
            }
            assert!(extra_checklist(m.id, m.approach.phases().len()).is_empty());
        }
        assert!(extra_checklist("dmaic", 0).is_empty());
        assert_eq!(find("nada"), None);
    }
}
