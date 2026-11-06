//! The DMAIC roadmap (`docs/plano_lean_seis_sigma.md`, 1.4 and 2): the five phases split
//! in steps, the tools that fit each step, the decisions that send the project back, and
//! what the project already has. Written for UTSim from the method and the tools of the
//! plan; the screen draws it as a vertical flowchart.
//!
//! A tool of a step points to where it lives in the app: a deliverable tool, a screen of
//! the project, an analysis of the "Dados e análises" screen, or nothing yet ("em breve").

use crate::analises;
use crate::state::State;
use crate::tollgate::{self, Check};
use crate::tools::{self, Tool};
use crate::{Id, domain};

/// Where a tool of the roadmap opens.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Target {
    /// A deliverable tool of the "Ferramentas" screen.
    Deliverable(Tool),
    /// A screen of the project, by its key (`termo`, `equipe`, `cronograma`...).
    Screen(&'static str),
    /// An analysis of the "Dados e análises" screen, by its key (`capacidade`...).
    Analysis(&'static str),
    /// Not in the app yet.
    Soon,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ToolRef {
    pub name: &'static str,
    pub target: Target,
}

const fn t(name: &'static str, target: Target) -> ToolRef {
    ToolRef { name, target }
}

#[derive(Debug, Clone, Copy)]
pub struct Step {
    pub name: &'static str,
    /// What the step is for, in one line.
    pub purpose: &'static str,
    pub tools: &'static [ToolRef],
    /// A verified tollgate check that also marks the step as done.
    pub check: Option<fn() -> Check>,
}

#[derive(Debug, Clone, Copy)]
pub struct Phase {
    pub name: &'static str,
    pub question: &'static str,
    pub steps: &'static [Step],
}

/// A decision after a phase: the question and the phase to go back to when the answer is
/// no.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Decision {
    /// The decision comes after this phase (index).
    pub after: usize,
    pub question: &'static str,
    /// The phase to go back to on "não".
    pub back_to: usize,
}

use Target::*;

pub const PHASES: [Phase; 5] = [
    Phase {
        name: "Definir",
        question: "Qual é o problema, quanto ele custa e o que o projeto vai entregar?",
        steps: &[
            Step {
                name: "Problema e meta",
                purpose: "Descrever o problema com dados e uma meta mensurável e com prazo.",
                tools: &[t("Termo de abertura", Screen("termo"))],
                check: Some(|| Check::CharterComplete),
            },
            Step {
                name: "Impacto e justificativa",
                purpose: "Estimar o ganho e ligar o projeto a um objetivo da empresa.",
                tools: &[
                    t("Ganhos", Screen("ganhos")),
                    t("Termo de abertura", Screen("termo")),
                ],
                check: Some(|| Check::SavingEstimated),
            },
            Step {
                name: "Patrocínio e aprovação",
                purpose: "Ter o termo aprovado pelo patrocinador.",
                tools: &[t("Termo de abertura", Screen("termo"))],
                check: Some(|| Check::CharterApproved),
            },
            Step {
                name: "Escopo",
                purpose: "Dizer o que entra e o que fica de fora, e os limites do processo.",
                tools: &[
                    t("Termo de abertura", Screen("termo")),
                    t("SIPOC", Deliverable(Tool::Sipoc)),
                ],
                check: None,
            },
            Step {
                name: "Voz do cliente",
                purpose: "Traduzir o que o cliente quer em características críticas mensuráveis.",
                tools: &[
                    t("VOC e árvore CTQ", Deliverable(Tool::Voc)),
                    t("Kano", Deliverable(Tool::Voc)),
                ],
                check: None,
            },
            Step {
                name: "Mapa do processo",
                purpose: "Ver o processo de ponta a ponta antes de medir.",
                tools: &[
                    t("SIPOC", Deliverable(Tool::Sipoc)),
                    t("Fluxograma e SIPOC do modelo", Screen("modelo")),
                    t("Mapa do fluxo de valor", Soon),
                ],
                check: Some(|| Check::DeliverableExists("sipoc".into())),
            },
            Step {
                name: "Equipe e cronograma",
                purpose: "Definir papéis, responsabilidades, riscos e o plano do projeto.",
                tools: &[
                    t("Equipe e RACI", Screen("equipe")),
                    t("Cronograma", Screen("cronograma")),
                    t("Riscos", Screen("riscos")),
                ],
                check: Some(|| Check::TeamDefined),
            },
        ],
    },
    Phase {
        name: "Medir",
        question: "Como o processo se comporta hoje, com dados confiáveis?",
        steps: &[
            Step {
                name: "Definição operacional do Y",
                purpose: "Combinar como o Y é medido, para todos medirem igual.",
                tools: &[t("Plano de coleta", Deliverable(Tool::DataCollection))],
                check: None,
            },
            Step {
                name: "Plano de coleta",
                purpose: "Decidir o quê, onde, quando, quanto e quem coleta, com a estratificação.",
                tools: &[t("Plano de coleta", Deliverable(Tool::DataCollection))],
                check: Some(|| Check::DeliverableExists("plano_coleta".into())),
            },
            Step {
                name: "Sistema de medição",
                purpose: "Confirmar que a medição separa as peças melhor que o próprio erro.",
                tools: &[
                    t("Gage R&R", Analysis("gage_rr")),
                    t("Estudo do tipo 1", Analysis("msa_tipo1")),
                    t("Viés e linearidade", Analysis("msa_linearidade")),
                    t("Concordância de atributos", Analysis("msa_atributos")),
                ],
                check: Some(|| Check::MsaApproved),
            },
            Step {
                name: "Linha de base",
                purpose: "Medir estabilidade e capacidade do processo atual, com dado real.",
                tools: &[
                    t("Estatística descritiva", Analysis("descritiva")),
                    t("Normalidade", Analysis("normalidade")),
                    t("Carta de controle", Analysis("carta")),
                    t("Capacidade", Analysis("capacidade")),
                    t("Replicações do modelo", Screen("modelo")),
                ],
                check: Some(|| Check::BaselineReal),
            },
            Step {
                name: "Estratificação",
                purpose: "Separar os dados por turno, máquina, produto, para ver onde o problema está.",
                tools: &[
                    t("Pareto", Deliverable(Tool::Pareto)),
                    t("Estatística descritiva", Analysis("descritiva")),
                ],
                check: Some(|| Check::DeliverableExists("pareto".into())),
            },
        ],
    },
    Phase {
        name: "Analisar",
        question: "Quais causas explicam o problema, comprovadas com dados?",
        steps: &[
            Step {
                name: "Causas potenciais",
                purpose: "Levantar com a equipe todas as causas possíveis.",
                tools: &[
                    t("Ishikawa", Deliverable(Tool::Ishikawa)),
                    t("5 porquês", Deliverable(Tool::FiveWhys)),
                ],
                check: Some(|| Check::DeliverableExists("ishikawa".into())),
            },
            Step {
                name: "Priorização das causas",
                purpose: "Escolher as causas a investigar primeiro.",
                tools: &[
                    t("Matriz causa-efeito", Deliverable(Tool::CauseEffect)),
                    t("Pareto", Deliverable(Tool::Pareto)),
                ],
                check: None,
            },
            Step {
                name: "Comprovação com dados",
                purpose: "Testar cada causa priorizada com dados ou no modelo.",
                tools: &[
                    t("Testes t", Analysis("t_duas_amostras")),
                    t("ANOVA", Analysis("anova")),
                    t("Qui-quadrado", Analysis("qui_quadrado")),
                    t("Proporções", Analysis("duas_proporcoes")),
                    t("Mann-Whitney", Analysis("mann_whitney")),
                    t("Correlação e regressão", Soon),
                ],
                check: None,
            },
            Step {
                name: "Gargalo e desperdícios",
                purpose: "Achar onde o fluxo para e quanto do tempo agrega valor.",
                tools: &[
                    t("Gargalo e caminho crítico do modelo", Screen("modelo")),
                    t("Mapa do fluxo de valor", Soon),
                ],
                check: None,
            },
            Step {
                name: "Riscos do processo atual",
                purpose: "Listar os modos de falha e o que já existe para preveni-los.",
                tools: &[t("FMEA", Deliverable(Tool::Fmea))],
                check: Some(|| Check::DeliverableExists("fmea".into())),
            },
        ],
    },
    Phase {
        name: "Melhorar",
        question: "Que solução ataca as causas, e ela funciona?",
        steps: &[
            Step {
                name: "Gerar e escolher soluções",
                purpose: "Propor soluções para cada causa comprovada e escolher com critérios.",
                tools: &[t("Ações", Screen("acoes")), t("Matriz de seleção", Soon)],
                check: None,
            },
            Step {
                name: "Testar antes de mudar",
                purpose: "Comparar a solução com a situação atual no modelo, com repetições.",
                tools: &[
                    t("Estudo de cenários", Screen("modelo")),
                    t("Planejamento de experimentos", Soon),
                ],
                check: None,
            },
            Step {
                name: "Piloto",
                purpose: "Aplicar em pequena escala e comparar antes e depois com dados.",
                tools: &[
                    t("Teste t pareado", Analysis("t_pareado")),
                    t("Capacidade", Analysis("capacidade")),
                ],
                check: None,
            },
            Step {
                name: "Riscos da solução",
                purpose: "Revisar o FMEA com as ações tomadas.",
                tools: &[t("FMEA", Deliverable(Tool::Fmea))],
                check: None,
            },
            Step {
                name: "Implantação e ganho",
                purpose: "Executar o plano de ações e ter o ganho previsto validado.",
                tools: &[t("Ações", Screen("acoes")), t("Ganhos", Screen("ganhos"))],
                check: Some(|| Check::SavingsForecast),
            },
        ],
    },
    Phase {
        name: "Controlar",
        question: "Como manter o ganho depois que o projeto acabar?",
        steps: &[
            Step {
                name: "Plano de controle",
                purpose: "Dizer o que é medido, como, por quem e com que frequência.",
                tools: &[t("Plano de controle", Deliverable(Tool::ControlPlan))],
                check: Some(|| Check::DeliverableExists("plano_controle".into())),
            },
            Step {
                name: "Monitoramento",
                purpose: "Acompanhar o processo com cartas de controle.",
                tools: &[
                    t("Carta de controle", Analysis("carta")),
                    t("Painel do gêmeo", Soon),
                ],
                check: None,
            },
            Step {
                name: "Plano de resposta",
                purpose: "Combinar o que fazer quando a carta der sinal.",
                tools: &[t("Plano de resposta", Deliverable(Tool::ResponsePlan))],
                check: None,
            },
            Step {
                name: "Padronização",
                purpose: "Atualizar procedimentos e treinar quem opera.",
                tools: &[
                    t("Trabalho padronizado", Soon),
                    t("A3", Deliverable(Tool::A3)),
                ],
                check: None,
            },
            Step {
                name: "Ganho validado e encerramento",
                purpose: "Validar o ganho com a controladoria, passar ao dono do processo e registrar as lições.",
                tools: &[
                    t("Ganhos", Screen("ganhos")),
                    t("Encerramento e lições", Screen("encerramento")),
                    t("Relatórios", Screen("relatorios")),
                ],
                check: Some(|| Check::OwnerAccepted),
            },
        ],
    },
];

pub const DECISIONS: [Decision; 2] = [
    Decision {
        after: 2,
        question: "A causa raiz foi confirmada com dados?",
        back_to: 2,
    },
    Decision {
        after: 3,
        question: "A meta foi atingida no piloto?",
        back_to: 3,
    },
];

/// Every tool named in the roadmap.
pub fn all_tools() -> Vec<ToolRef> {
    PHASES
        .iter()
        .flat_map(|p| p.steps.iter().flat_map(|s| s.tools.iter().copied()))
        .collect()
}

/// Whether a tool already has something in the project.
pub fn tool_used(state: &State, project: Id, target: Target) -> bool {
    match target {
        Deliverable(tool) => tools::of_tool(state, project, tool)
            .iter()
            .any(|d| !tools::all_items(state, d.id).is_empty()),
        Analysis(key) => analises::analyses(state, project)
            .iter()
            .any(|a| a.recipe.as_ref().is_some_and(|r| r.key() == key)),
        Screen(_) | Soon => false,
    }
}

/// Whether a step is done in the project: its tollgate check passes, or one of its tools
/// (deliverable or analysis) already has content.
pub fn step_done(state: &State, project: Id, phase_index: usize, step: &Step) -> bool {
    if let Some(c) = step.check {
        let phase = domain::phases(state, project)
            .get(phase_index)
            .map(|p| p.id)
            .unwrap_or(project);
        if tollgate::verify(state, project, phase, &c()) {
            return true;
        }
    }
    step.tools
        .iter()
        .any(|t| tool_used(state, project, t.target))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::programa::testkit::*;

    /// Every tool of the roadmap exists in the app or is marked "em breve".
    #[test]
    fn every_tool_exists_or_is_soon() {
        let analyses = [
            "descritiva",
            "normalidade",
            "capacidade",
            "carta",
            "t_uma_amostra",
            "t_duas_amostras",
            "t_pareado",
            "anova",
            "qui_quadrado",
            "proporcao",
            "duas_proporcoes",
            "mann_whitney",
            "wilcoxon",
            "gage_rr",
            "msa_tipo1",
            "msa_vies",
            "msa_linearidade",
            "msa_atributos",
        ];
        for t in all_tools() {
            match t.target {
                Deliverable(tool) => assert!(Tool::ALL.contains(&tool), "{}", t.name),
                Analysis(k) => assert!(analyses.contains(&k), "{}: {k}", t.name),
                Screen(k) => assert!(!k.is_empty()),
                Soon => {}
            }
        }
        assert_eq!(PHASES.len(), 5);
        assert!(PHASES.iter().all(|p| !p.steps.is_empty()));
        for d in DECISIONS {
            assert!(d.back_to <= d.after && d.after < PHASES.len());
        }
    }

    #[test]
    fn steps_done_from_the_project() {
        let mut t = T::new();
        let p = t.program;
        let step = &PHASES[1].steps[4];
        assert!(!step_done(&t.s, p, 1, step));
        let d = t.id();
        t.commit(tools::Deliverable::create(
            p,
            None,
            Tool::Pareto,
            d,
            "Pareto",
        ));
        let i = t.id();
        t.commit(vec![
            crate::Edit::new(
                tools::item_ref(i),
                tools::fields::item::PROJECT,
                p.to_string(),
            ),
            crate::Edit::new(
                tools::item_ref(i),
                tools::fields::item::DELIVERABLE,
                d.to_string(),
            ),
            crate::Edit::new(
                tools::item_ref(i),
                tools::fields::item::ITEM_KIND,
                tools::pareto::ITEM,
            ),
        ]);
        assert!(step_done(&t.s, p, 1, step));
    }
}
