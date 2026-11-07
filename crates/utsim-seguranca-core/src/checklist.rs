//! Machine safeguarding checklist and protective equipment register (stage QS15 of
//! `docs/plano_qualidade_e_seguranca.md`, sections 4.4 and 4.8).
//!
//! The checklist is the project's own, by theme, with items written by the project and the
//! machinery standard cited only by number and theme (NR-12). A company replaces or extends
//! it with the script of its technical responsible by CSV. Each answer is compliant, not
//! compliant or not applicable, with a photo (attachment by hash) and an action. A
//! non-compliant item becomes a hazard of the station in the inventory, marked with its
//! origin. The equipment register keeps type, use, the approval certificate number typed by
//! the user (CA), validity and replacement interval (NR-6, by number and theme).

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use utsim_posto::{Unidade, UnidadeErro};

use crate::inventario::{Grupo, PERIGOS, Perigo};

pub const MODELOS: &str = "checklists";
pub const INSPECOES: &str = "inspecoes_maquina";
pub const EPIS: &str = "epis";

/// Notice of the checklist screens and reports.
pub const AVISO_NR12: &str = "Checklist próprio do UTSim para estudo e organização, com a \
NR-12 (segurança no trabalho em máquinas e equipamentos) citada só por número e tema. Ele \
não reproduz a norma, não substitui a apreciação de riscos do profissional habilitado e não \
atesta conformidade.";

/// One item of a checklist.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Item {
    pub id: String,
    pub tema: String,
    pub texto: String,
    /// Severity step suggested for the hazard a non-compliance creates (0 lowest).
    #[serde(default)]
    pub severidade: usize,
}

/// A checklist (the project's or the company's).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Modelo {
    #[serde(default)]
    pub id: String,
    pub nome: String,
    pub itens: Vec<Item>,
}

/// The themes of the project's own checklist.
pub const TEMAS: [&str; 7] = [
    "Proteções fixas e móveis",
    "Intertravamento",
    "Parada de emergência",
    "Comandos",
    "Sinalização",
    "Manutenção e bloqueio",
    "Capacitação",
];

impl Modelo {
    /// The project's own checklist, written by the project.
    pub fn proprio() -> Modelo {
        let itens: [(&str, &str, usize); 16] = [
            (
                TEMAS[0],
                "As partes móveis que podem prender, cortar ou esmagar estão fora do alcance, atrás de proteção.",
                4,
            ),
            (TEMAS[0], "A proteção fixa só sai com ferramenta.", 3),
            (
                TEMAS[0],
                "A proteção móvel não deixa ver nem alcançar a zona de perigo com a máquina em movimento.",
                3,
            ),
            (
                TEMAS[1],
                "Ao abrir a proteção móvel, o movimento perigoso para.",
                4,
            ),
            (
                TEMAS[1],
                "O intertravamento não é burlado com calço, fita ou ímã.",
                4,
            ),
            (
                TEMAS[2],
                "Há dispositivo de parada de emergência ao alcance de quem opera.",
                4,
            ),
            (
                TEMAS[2],
                "A parada de emergência foi testada e para o movimento perigoso.",
                4,
            ),
            (
                TEMAS[3],
                "O comando de partida não age sozinho por toque acidental.",
                3,
            ),
            (
                TEMAS[3],
                "A máquina não volta a funcionar sozinha depois de faltar energia.",
                3,
            ),
            (
                TEMAS[4],
                "Os perigos da máquina estão sinalizados no posto.",
                2,
            ),
            (
                TEMAS[4],
                "As áreas de circulação perto da máquina estão demarcadas.",
                2,
            ),
            (
                TEMAS[5],
                "Há procedimento de bloqueio de energias antes de manutenção e limpeza.",
                4,
            ),
            (
                TEMAS[5],
                "Os pontos de bloqueio estão identificados e há cadeados individuais.",
                3,
            ),
            (TEMAS[5], "A manutenção da máquina tem registro em dia.", 2),
            (
                TEMAS[6],
                "Quem opera foi capacitado para esta máquina e o registro existe.",
                3,
            ),
            (
                TEMAS[6],
                "Quem faz manutenção foi capacitado para a manutenção desta máquina.",
                3,
            ),
        ];
        Modelo {
            id: "proprio".into(),
            nome: "Checklist próprio do UTSim".into(),
            itens: itens
                .iter()
                .enumerate()
                .map(|(i, (tema, texto, sev))| Item {
                    id: format!("p{:02}", i + 1),
                    tema: tema.to_string(),
                    texto: texto.to_string(),
                    severidade: *sev,
                })
                .collect(),
        }
    }

    /// Reads a company checklist: `tema;item;severidade` (severity optional, 0 to 4).
    pub fn de_csv(nome: &str, texto: &str) -> Result<Modelo, String> {
        let mut itens = Vec::new();
        for (k, linha) in texto.trim_start_matches('\u{feff}').lines().enumerate() {
            let linha = linha.trim();
            if linha.is_empty() || linha.to_lowercase().starts_with("tema") {
                continue;
            }
            let sep = if linha.contains(';') { ';' } else { ',' };
            let c: Vec<&str> = linha.splitn(3, sep).map(str::trim).collect();
            if c.len() < 2 || c[0].is_empty() || c[1].is_empty() {
                return Err(format!("linha {}: esperado tema;item;severidade", k + 1));
            }
            let severidade = match c.get(2) {
                Some(s) if !s.is_empty() => s
                    .parse::<usize>()
                    .map_err(|_| format!("linha {}: severidade deve ser inteiro", k + 1))?,
                _ => 2,
            };
            itens.push(Item {
                id: format!("e{:03}", itens.len() + 1),
                tema: c[0].to_string(),
                texto: c[1].to_string(),
                severidade,
            });
        }
        if itens.is_empty() {
            return Err("o checklist não tem itens".into());
        }
        Ok(Modelo {
            id: String::new(),
            nome: nome.to_string(),
            itens,
        })
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Resposta {
    #[default]
    SemResposta,
    Conforme,
    NaoConforme,
    NaoSeAplica,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct RespostaItem {
    pub resposta: Resposta,
    #[serde(default)]
    pub observacao: String,
    /// Photo, attachment name in the unit (`<sha256>.<ext>`).
    #[serde(default)]
    pub foto: Option<String>,
    #[serde(default)]
    pub acao: String,
}

/// An inspection of a machine with a checklist.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct InspecaoMaquina {
    #[serde(default)]
    pub id: String,
    pub posto: String,
    pub maquina: String,
    pub modelo: String,
    /// `AAAA-MM-DD`.
    pub data: String,
    #[serde(default)]
    pub responsavel: String,
    /// Answer by item id.
    #[serde(default)]
    pub respostas: BTreeMap<String, RespostaItem>,
}

impl InspecaoMaquina {
    pub fn nao_conformes<'a>(&'a self, modelo: &'a Modelo) -> Vec<(&'a Item, &'a RespostaItem)> {
        modelo
            .itens
            .iter()
            .filter_map(|i| {
                let r = self.respostas.get(&i.id)?;
                (r.resposta == Resposta::NaoConforme).then_some((i, r))
            })
            .collect()
    }

    pub fn pendentes<'a>(&self, modelo: &'a Modelo) -> Vec<&'a Item> {
        modelo
            .itens
            .iter()
            .filter(|i| {
                self.respostas
                    .get(&i.id)
                    .is_none_or(|r| r.resposta == Resposta::SemResposta)
            })
            .collect()
    }
}

/// Hazards of the station from the non-compliant items, marked `origem: "checklist:<id da
/// inspeção>:<item>"` so they are not created twice. Probability is left at the lowest
/// step for the user to assess; severity comes from the item.
pub fn perigos_do_checklist(insp: &InspecaoMaquina, modelo: &Modelo) -> Vec<Perigo> {
    insp.nao_conformes(modelo)
        .into_iter()
        .map(|(item, r)| Perigo {
            id: String::new(),
            posto: insp.posto.clone(),
            grupo: Grupo::Acidente,
            descricao: format!("{}: {}", insp.maquina, item.texto),
            fonte: format!("checklist de máquina, tema {}", item.tema),
            circunstancia: r.observacao.clone(),
            lesoes: String::new(),
            controles: String::new(),
            probabilidade: 0,
            severidade: item.severidade,
            origem: format!("checklist:{}:{}", insp.id, item.id),
        })
        .collect()
}

/// Writes the hazards of the non-compliances into the unit, skipping those already there.
/// Returns how many were created.
pub fn gravar_perigos(
    u: &mut Unidade,
    insp: &InspecaoMaquina,
    modelo: &Modelo,
) -> Result<usize, UnidadeErro> {
    let existentes: Vec<Perigo> = u.estado().lista(PERIGOS);
    let mut n = 0;
    for p in perigos_do_checklist(insp, modelo) {
        if existentes.iter().any(|e| e.origem == p.origem) {
            continue;
        }
        u.gravar(PERIGOS, "", &p)?;
        n += 1;
    }
    Ok(n)
}

/// A protective equipment of the register.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Epi {
    #[serde(default)]
    pub id: String,
    /// Name as in `utsim_model::quality::PPE_NAMES` when it is one of them.
    pub tipo: String,
    #[serde(default)]
    pub descricao: String,
    /// What it protects against and when to use it.
    #[serde(default)]
    pub uso: String,
    /// Approval certificate number, typed by the user (never checked by the app).
    #[serde(default)]
    pub ca: String,
    /// Validity of the certificate, `AAAA-MM-DD`.
    #[serde(default)]
    pub validade_ca: String,
    /// Replacement interval in days.
    #[serde(default)]
    pub troca_dias: Option<u32>,
}

impl Epi {
    pub fn vencido(&self, hoje: &str) -> bool {
        !self.validade_ca.is_empty() && self.validade_ca.as_str() < hoje
    }
}

/// Equipment of the stations without a register entry, and register entries past validity.
pub fn pendencias_epi(postos: &[utsim_posto::Posto], epis: &[Epi], hoje: &str) -> Vec<String> {
    let mut out = Vec::new();
    for p in postos {
        for e in &p.epi {
            if !epis.iter().any(|x| &x.tipo == e) {
                out.push(format!(
                    "posto {}: '{e}' não está no cadastro de EPI",
                    p.nome
                ));
            }
        }
    }
    for e in epis.iter().filter(|e| e.vencido(hoje)) {
        out.push(format!(
            "EPI {} ({}): CA com validade vencida em {}",
            e.tipo, e.ca, e.validade_ca
        ));
    }
    out
}
