//! 8D problem solving (stage QS13 of `docs/plano_qualidade_e_seguranca.md`, section 3.7):
//! the eight disciplines in the project's own words, with containment by deadline, the root
//! cause tied to the Ishikawa diagram and the 5 whys of `utsim-ferramentas`, corrective
//! actions verified by data (a control chart or a study in the model) and prevention of
//! recurrence that updates the FMEA and the control plan.
//!
//! The 8D is a record of the unit folder (collection [`COLECAO`]). When the cause does not
//! show up, it goes to the portfolio of UTSim LSS as an idea ([`crate::seis_sigma`]).

use serde::{Deserialize, Serialize};
use utsim_ferramentas::cinco_porques::{self, WhyInput};
use utsim_ferramentas::ishikawa::{self, Input};

pub const COLECAO: &str = "oito_d";

/// The eight disciplines, written by the project.
pub const DISCIPLINAS: [(&str, &str); 8] = [
    (
        "D1 Equipe",
        "Quem resolve: pessoas de áreas diferentes que conhecem o processo, com um líder.",
    ),
    (
        "D2 Problema",
        "O que aconteceu, onde, quando, quanto e como foi percebido, com dados e sem culpar ninguém.",
    ),
    (
        "D3 Contenção",
        "O que protege o cliente já, enquanto a causa não é achada, com prazo e responsável.",
    ),
    (
        "D4 Causa raiz",
        "Por que aconteceu e por que não foi detectado, pelo Ishikawa e pelos 5 porquês, confirmado por dado.",
    ),
    (
        "D5 Ações corretivas",
        "O que elimina a causa raiz, escolhido entre alternativas.",
    ),
    (
        "D6 Implementação e verificação",
        "A ação feita e o efeito conferido por dado: carta de controle ou estudo no modelo.",
    ),
    (
        "D7 Prevenção",
        "O que impede que volte aqui e em processos parecidos: FMEA, plano de controle, padrões.",
    ),
    (
        "D8 Encerramento",
        "Reconhecer a equipe, registrar o que se aprendeu e fechar.",
    ),
];

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Problema {
    #[serde(default)]
    pub o_que: String,
    #[serde(default)]
    pub onde: String,
    #[serde(default)]
    pub quando: String,
    #[serde(default)]
    pub quanto: String,
    #[serde(default)]
    pub como_percebido: String,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Tarefa {
    pub acao: String,
    #[serde(default)]
    pub responsavel: String,
    /// `AAAA-MM-DD`.
    #[serde(default)]
    pub prazo: String,
    #[serde(default)]
    pub feita: bool,
}

/// A cause on the Ishikawa diagram (category of `utsim_ferramentas::ishikawa::SIX_M`).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Causa {
    pub id: String,
    pub categoria: String,
    pub texto: String,
    /// Sub-cause of another cause.
    #[serde(default)]
    pub pai: Option<String>,
    /// Confirmed by data as a root cause.
    #[serde(default)]
    pub confirmada: bool,
}

/// One "why" of a chain, under a cause or another why.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Porque {
    pub id: String,
    /// The why above (absent: right under the cause).
    #[serde(default)]
    pub pai: Option<String>,
    pub texto: String,
    /// Marked as the root cause of the chain.
    #[serde(default)]
    pub raiz: bool,
}

/// How a corrective action was checked.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Verificacao {
    /// `carta` (control chart on real data) or `modelo` (study in the UTSim model).
    pub metodo: String,
    /// The indicator before and after (escape, scrap, out-of-spec fraction...).
    pub indicador: String,
    pub antes: f64,
    pub depois: f64,
    /// The improvement is real (signal on the chart, significant paired difference).
    pub confirmada: bool,
    #[serde(default)]
    pub nota: String,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct AcaoCorretiva {
    pub tarefa: Tarefa,
    #[serde(default)]
    pub verificacao: Option<Verificacao>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Prevencao {
    #[serde(default)]
    pub fmea_atualizado: bool,
    #[serde(default)]
    pub plano_controle_atualizado: bool,
    #[serde(default)]
    pub outros_processos: String,
}

/// An 8D.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct OitoD {
    #[serde(default)]
    pub id: String,
    pub titulo: String,
    /// Work station (id in the unit).
    #[serde(default)]
    pub posto: String,
    /// Where it came from: `manual`, `carta` (a signal) or `simulacao`.
    #[serde(default)]
    pub origem: String,
    /// `AAAA-MM-DD`.
    #[serde(default)]
    pub aberto_em: String,
    #[serde(default)]
    pub equipe: Vec<String>,
    #[serde(default)]
    pub lider: String,
    #[serde(default)]
    pub problema: Problema,
    #[serde(default)]
    pub contencao: Vec<Tarefa>,
    #[serde(default)]
    pub causas: Vec<Causa>,
    #[serde(default)]
    pub porques: Vec<Porque>,
    #[serde(default)]
    pub acoes: Vec<AcaoCorretiva>,
    #[serde(default)]
    pub prevencao: Prevencao,
    #[serde(default)]
    pub licoes: String,
    #[serde(default)]
    pub encerrado_em: String,
    /// Idea in the Seis Sigma portfolio it was sent to.
    #[serde(default)]
    pub ideia_seis_sigma: Option<String>,
}

impl OitoD {
    /// Whether each discipline has what it needs, in order D1 to D8.
    pub fn completas(&self) -> [bool; 8] {
        let p = &self.problema;
        [
            !self.equipe.is_empty() && !self.lider.trim().is_empty(),
            !p.o_que.trim().is_empty() && !p.onde.trim().is_empty() && !p.quanto.trim().is_empty(),
            !self.contencao.is_empty()
                && self
                    .contencao
                    .iter()
                    .all(|t| !t.prazo.is_empty() && !t.responsavel.is_empty()),
            self.causas.iter().any(|c| c.confirmada),
            !self.acoes.is_empty(),
            !self.acoes.is_empty()
                && self.acoes.iter().all(|a| {
                    a.tarefa.feita && a.verificacao.as_ref().is_some_and(|v| v.confirmada)
                }),
            self.prevencao.fmea_atualizado && self.prevencao.plano_controle_atualizado,
            !self.encerrado_em.is_empty() && !self.licoes.trim().is_empty(),
        ]
    }

    /// The first discipline still open (0-based), `None` when all are done.
    pub fn proxima(&self) -> Option<usize> {
        self.completas().iter().position(|c| !c)
    }

    /// The 8D can be closed only when D1 to D7 are done.
    pub fn pode_encerrar(&self) -> bool {
        self.completas()[..7].iter().all(|c| *c)
    }

    /// Containment tasks past the deadline on `hoje` and not done.
    pub fn contencao_atrasada(&self, hoje: &str) -> Vec<&Tarefa> {
        self.contencao
            .iter()
            .filter(|t| !t.feita && !t.prazo.is_empty() && t.prazo.as_str() < hoje)
            .collect()
    }

    /// Index of a cause or a why (causes first, then the whys), the ids the tools of
    /// `utsim-ferramentas` work with.
    fn indice(&self, id: &str) -> Option<u32> {
        self.causas
            .iter()
            .map(|c| c.id.as_str())
            .chain(self.porques.iter().map(|w| w.id.as_str()))
            .position(|x| x == id)
            .map(|i| i as u32)
    }

    /// The Ishikawa diagram of the causes, through `utsim-ferramentas` (ids are the
    /// positions in `causas`).
    pub fn ishikawa(&self) -> ishikawa::Diagram<u32> {
        let inputs: Vec<Input<u32>> = self
            .causas
            .iter()
            .enumerate()
            .map(|(i, c)| Input {
                id: i as u32,
                parent: c.pai.as_deref().and_then(|p| self.indice(p)),
                category: c.categoria.clone(),
                text: c.texto.clone(),
                status: if c.confirmada {
                    ishikawa::CauseStatus::Verified
                } else {
                    ishikawa::CauseStatus::Open
                },
                evidence: String::new(),
            })
            .collect();
        ishikawa::diagram(&self.problema.o_que, &inputs)
    }

    /// The 5-why chain under a cause, through `utsim-ferramentas` (`None`: no such cause).
    pub fn cadeia(&self, causa: &str) -> Option<cinco_porques::Chain<u32>> {
        let raiz = self.indice(causa)?;
        let texto = self.causas.iter().find(|c| c.id == causa)?.texto.clone();
        let inputs: Vec<WhyInput<u32>> = self
            .porques
            .iter()
            .filter_map(|w| {
                Some(WhyInput {
                    id: self.indice(&w.id)?,
                    parent: Some(match &w.pai {
                        Some(p) => self.indice(p)?,
                        None => raiz,
                    }),
                    text: w.texto.clone(),
                    evidence: String::new(),
                    root: w.raiz,
                })
            })
            .collect();
        Some(cinco_porques::chain(
            raiz,
            &self.problema.o_que,
            Some((raiz, texto)),
            &inputs,
        ))
    }

    /// Plain text summary (for the idea in the Seis Sigma portfolio and the report).
    pub fn resumo(&self) -> String {
        let p = &self.problema;
        let feitas = self.completas().iter().filter(|c| **c).count();
        let mut s = format!(
            "8D \"{}\" ({} de 8 disciplinas). Problema: {} | onde: {} | quando: {} | quanto: {}.",
            self.titulo, feitas, p.o_que, p.onde, p.quando, p.quanto
        );
        let confirmadas: Vec<&str> = self
            .causas
            .iter()
            .filter(|c| c.confirmada)
            .map(|c| c.texto.as_str())
            .collect();
        if confirmadas.is_empty() {
            s.push_str(" Causa raiz ainda não confirmada.");
        } else {
            s.push_str(&format!(" Causas confirmadas: {}.", confirmadas.join(", ")));
        }
        if !self.contencao.is_empty() {
            let c: Vec<&str> = self.contencao.iter().map(|t| t.acao.as_str()).collect();
            s.push_str(&format!(" Contenção: {}.", c.join(", ")));
        }
        s
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn exemplo() -> OitoD {
        OitoD {
            titulo: "Trinca na flange".into(),
            equipe: vec!["ana".into(), "bruno".into()],
            lider: "ana".into(),
            problema: Problema {
                o_que: "Trinca na flange".into(),
                onde: "torno 2".into(),
                quando: "depois do setup".into(),
                quanto: "3 % das peças".into(),
                como_percebido: "reclamação do cliente".into(),
            },
            contencao: vec![Tarefa {
                acao: "Inspeção 100 % das flanges".into(),
                responsavel: "qualidade".into(),
                prazo: "2026-11-10".into(),
                feita: false,
            }],
            causas: vec![
                Causa {
                    id: "c1".into(),
                    categoria: "metodo".into(),
                    texto: "Setup sem primeira peça".into(),
                    pai: None,
                    confirmada: true,
                },
                Causa {
                    id: "c2".into(),
                    categoria: "maquina".into(),
                    texto: "Folga no cabeçote".into(),
                    pai: None,
                    confirmada: false,
                },
            ],
            porques: vec![
                Porque {
                    id: "w1".into(),
                    pai: None,
                    texto: "Não há padrão de setup".into(),
                    raiz: false,
                },
                Porque {
                    id: "w2".into(),
                    pai: Some("w1".into()),
                    texto: "O padrão nunca foi escrito".into(),
                    raiz: true,
                },
            ],
            ..OitoD::default()
        }
    }

    #[test]
    fn disciplines_advance_in_order() {
        let mut d = exemplo();
        assert_eq!(&d.completas()[..4], &[true, true, true, true]);
        assert_eq!(d.proxima(), Some(4));
        assert!(!d.pode_encerrar());
        assert_eq!(d.contencao_atrasada("2026-11-12").len(), 1);
        d.acoes.push(AcaoCorretiva {
            tarefa: Tarefa {
                acao: "Padrão de setup com primeira peça".into(),
                responsavel: "processos".into(),
                prazo: "2026-11-20".into(),
                feita: true,
            },
            verificacao: Some(Verificacao {
                metodo: "modelo".into(),
                indicador: "escape".into(),
                antes: 0.03,
                depois: 0.002,
                confirmada: true,
                nota: String::new(),
            }),
        });
        d.prevencao = Prevencao {
            fmea_atualizado: true,
            plano_controle_atualizado: true,
            outros_processos: String::new(),
        };
        assert!(d.pode_encerrar());
        d.licoes = "Primeira peça em todo setup".into();
        d.encerrado_em = "2026-12-01".into();
        assert_eq!(d.proxima(), None);
    }

    #[test]
    fn tools_of_the_common_crate() {
        let d = exemplo();
        let ishi = d.ishikawa();
        assert_eq!(ishi.effect, "Trinca na flange");
        let c = d.cadeia("c1").unwrap();
        assert_eq!(c.whys.len(), 2);
        assert!(d.cadeia("nada").is_none());
        assert!(d.resumo().contains("Setup sem primeira peça"));
    }
}
