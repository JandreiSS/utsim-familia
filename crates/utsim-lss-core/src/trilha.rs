//! A trilha do projeto: o painel desenha as fases como estações ligadas por uma linha, o
//! portão de cada fase entre elas e, para a fase escolhida, os passos que faltam ("agora
//! faça"), cada um com o lugar do app onde se faz. Tudo sai do estado do projeto: nada é
//! gravado aqui, e os projetos antigos aparecem do mesmo jeito.
//!
//! Os passos de um projeto DMAIC vêm do roteiro ([`crate::roteiro`]); os de qualquer
//! abordagem vêm também do checklist do portão da fase (os itens que faltam).

use serde_json::Value;

use crate::domain::{self, Approach, PhaseStatus, Project};
use crate::roteiro::{self, Target};
use crate::state::State;
use crate::tollgate::{self, Check, ItemType, TollgateStatus};
use crate::tools::Tool;
use crate::{Id, op::Kind};

/// Uma estação da trilha: uma fase do projeto.
#[derive(Debug, Clone, PartialEq)]
pub struct Estacao {
    pub fase: Id,
    pub nome: String,
    pub status: PhaseStatus,
    /// O estado do portão da fase (`None`: o portão ainda não foi preparado).
    pub portao: Option<TollgateStatus>,
    /// Passos feitos e passos da fase.
    pub feitos: usize,
    pub total: usize,
    /// A fase em que o projeto está.
    pub atual: bool,
}

impl Estacao {
    /// A fração feita (0 a 1; uma fase sem passos conta como feita quando aprovada).
    pub fn progresso(&self) -> f32 {
        if self.total == 0 {
            if self.status == PhaseStatus::Approved {
                1.0
            } else {
                0.0
            }
        } else {
            self.feitos as f32 / self.total as f32
        }
    }
}

/// Um passo da fase.
#[derive(Debug, Clone, PartialEq)]
pub struct Passo {
    pub titulo: String,
    /// Para que serve, numa linha (vazio quando não há).
    pub para_que: String,
    pub feito: bool,
    /// Onde se faz: as telas, ferramentas e análises do app (sem as "em breve").
    pub alvos: Vec<(String, Target)>,
    /// Veio do checklist do portão (e não do roteiro).
    pub do_portao: bool,
}

/// O que o painel sugere para a fase.
#[derive(Debug, Clone, PartialEq)]
pub enum Proximo {
    /// Fazer o primeiro passo que falta (posição em [`passos`]).
    Passo(usize),
    /// Tudo feito: pedir a decisão do portão.
    PedirPortao,
    /// O portão foi enviado: esperar a decisão dos aprovadores.
    AguardarDecisao,
    /// O portão pediu para refazer: rever o que os aprovadores disseram.
    Refazer,
    /// A fase está aprovada: seguir para a próxima (posição), ou o projeto terminou.
    Seguir(Option<usize>),
}

/// Onde se resolve um item verificado do checklist.
pub fn alvo_do_check(c: &Check) -> Option<Target> {
    Some(match c {
        Check::CharterComplete | Check::CharterApproved => Target::Screen("termo"),
        Check::TeamDefined => Target::Screen("equipe"),
        Check::ScheduleDefined => Target::Screen("cronograma"),
        Check::RisksAssessed | Check::HighRisksTreated => Target::Screen("riscos"),
        Check::SavingEstimated | Check::SavingsForecast | Check::SavingsValidated => {
            Target::Screen("ganhos")
        }
        Check::PhaseActionsDone => Target::Screen("acoes"),
        Check::LessonsRecorded | Check::OwnerAccepted => Target::Screen("encerramento"),
        Check::MsaApproved => Target::Analysis("gage_rr"),
        Check::BaselineReal => Target::Analysis("capacidade"),
        Check::DeliverableExists(t) => {
            let tool: Tool = serde_json::from_value(Value::String(t.clone())).ok()?;
            Target::Deliverable(tool)
        }
    })
}

fn projeto(state: &State, project: Id) -> Option<Project> {
    state
        .entity(project)
        .filter(|e| e.kind == Kind::Project)
        .map(|e| Project::read(project, e))
}

/// Os passos da fase `indice`: os do roteiro (DMAIC) e os itens do checklist do portão,
/// sem repetir um item que um passo do roteiro já cobre.
pub fn passos(state: &State, project: Id, indice: usize) -> Vec<Passo> {
    let mut v = Vec::new();
    let Some(p) = projeto(state, project) else {
        return v;
    };
    let fases = domain::phases(state, project);
    let Some(fase) = fases.get(indice) else {
        return v;
    };
    let mut cobertos: Vec<String> = Vec::new();
    if p.approach == Approach::Dmaic
        && let Some(ph) = roteiro::PHASES.get(indice)
    {
        for step in ph.steps {
            if let Some(c) = step.check {
                cobertos.push(c().key());
            }
            v.push(Passo {
                titulo: step.name.to_string(),
                para_que: step.purpose.to_string(),
                feito: roteiro::step_done(state, project, indice, step),
                alvos: step
                    .tools
                    .iter()
                    .filter(|t| t.target != Target::Soon)
                    .map(|t| (t.name.to_string(), t.target))
                    .collect(),
                do_portao: false,
            });
        }
    }
    if let Some(tg) = tollgate::of_phase(state, fase.id) {
        for r in tollgate::evaluate(state, project, &tg) {
            let it = r.item;
            if it
                .check
                .as_ref()
                .is_some_and(|c| cobertos.contains(&c.key()))
            {
                continue;
            }
            let alvos = match (&it.item_type, &it.check) {
                (ItemType::Verified, Some(c)) => alvo_do_check(c)
                    .map(|t| vec![(String::new(), t)])
                    .unwrap_or_default(),
                _ => vec![(String::new(), Target::Screen("fases"))],
            };
            v.push(Passo {
                titulo: it.text,
                para_que: String::new(),
                feito: r.ok,
                alvos,
                do_portao: true,
            });
        }
    }
    v
}

/// As estações da trilha, na ordem das fases.
pub fn estacoes(state: &State, project: Id) -> Vec<Estacao> {
    let atual = projeto(state, project).and_then(|p| p.current_phase);
    domain::phases(state, project)
        .iter()
        .enumerate()
        .map(|(i, f)| {
            let ps = passos(state, project, i);
            Estacao {
                fase: f.id,
                nome: f.name.clone(),
                status: f.status,
                portao: tollgate::of_phase(state, f.id).map(|t| t.status),
                feitos: ps.iter().filter(|p| p.feito).count(),
                total: ps.len(),
                atual: atual == Some(f.id),
            }
        })
        .collect()
}

/// A fase em que o projeto está (a primeira não aprovada, quando o projeto não diz).
pub fn indice_atual(state: &State, project: Id) -> Option<usize> {
    let es = estacoes(state, project);
    es.iter()
        .position(|e| e.atual)
        .or_else(|| es.iter().position(|e| e.status != PhaseStatus::Approved))
        .or(if es.is_empty() {
            None
        } else {
            Some(es.len() - 1)
        })
}

/// O que fazer agora na fase `indice`.
pub fn proximo(state: &State, project: Id, indice: usize) -> Proximo {
    let es = estacoes(state, project);
    let Some(e) = es.get(indice) else {
        return Proximo::Seguir(None);
    };
    if e.status == PhaseStatus::Approved || e.portao == Some(TollgateStatus::Approved) {
        let prox = (indice + 1 < es.len()).then_some(indice + 1);
        return Proximo::Seguir(prox);
    }
    match e.portao {
        Some(TollgateStatus::Submitted) => return Proximo::AguardarDecisao,
        Some(TollgateStatus::Redo) => return Proximo::Refazer,
        _ => {}
    }
    match passos(state, project, indice).iter().position(|p| !p.feito) {
        Some(i) => Proximo::Passo(i),
        None => Proximo::PedirPortao,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tools::testing::Fixture;

    #[test]
    fn um_projeto_novo_comeca_no_termo_da_fase_definir() {
        let f = Fixture::new();
        let es = estacoes(&f.state, f.project);
        assert_eq!(es.len(), 5, "DMAIC");
        assert_eq!(es[0].nome, "Definir");
        assert_eq!(indice_atual(&f.state, f.project), Some(0));
        let ps = passos(&f.state, f.project, 0);
        assert!(!ps.is_empty());
        assert_eq!(ps[0].titulo, "Problema e meta");
        assert!(
            ps[0]
                .alvos
                .iter()
                .any(|(_, t)| *t == Target::Screen("termo")),
            "o passo leva ao termo"
        );
        assert_eq!(proximo(&f.state, f.project, 0), Proximo::Passo(0));
        assert!(es.iter().all(|e| e.progresso() < 1.0));
    }

    #[test]
    fn os_itens_verificados_do_checklist_apontam_a_tela() {
        assert_eq!(
            alvo_do_check(&Check::TeamDefined),
            Some(Target::Screen("equipe"))
        );
        assert_eq!(
            alvo_do_check(&Check::DeliverableExists("fmea".into())),
            Some(Target::Deliverable(Tool::Fmea))
        );
        assert_eq!(
            alvo_do_check(&Check::DeliverableExists("nada".into())),
            None
        );
    }
}
