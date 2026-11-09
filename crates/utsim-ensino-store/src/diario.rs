//! The signed attempt journal, on the shared crate `utsim-diario` (decision E12): each
//! attempt is a JSON body signed by the student's key and chained by hash to the previous
//! one, with date and time. The body carries decisions
//! and answers, never the grade the teacher will use: the teacher simulates again.
//! Files written before E12 stay valid: the link has the same JSON fields.

use serde::{Deserialize, Serialize};
use utsim_diario::{Encadeado, Problema};
use utsim_ensino_core::nota::Tentativa;

use crate::chave::Chave;
use crate::{Erro, Result};

/// What is signed.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct RegistroTentativa {
    /// Student record (also the variant seed).
    pub aluno: String,
    pub pacote: String,
    pub pacote_hash: String,
    pub missao: String,
    pub versao_nucleo: String,
    /// Milliseconds since 1970 (UTC).
    pub quando: i64,
    /// 1 for the first attempt of the journal.
    pub sequencia: u64,
    /// Hash of the previous body (empty for the first).
    pub anterior: String,
    pub tentativa: Tentativa,
    /// The grade the student's app showed. Only informative: the teacher ignores it.
    #[serde(default)]
    pub nota_informada: Option<f64>,
}

/// A signed attempt: the body exactly as signed, its hash and the signature (a link of
/// the shared journal, same JSON fields as before E12).
pub type TentativaAssinada = utsim_diario::Elo;

impl Encadeado for RegistroTentativa {
    fn autor(&self) -> &str {
        &self.aluno
    }
    fn sequencia(&self) -> u64 {
        self.sequencia
    }
    fn anterior(&self) -> &str {
        &self.anterior
    }
    fn encadear(&mut self, sequencia: u64, anterior: String) {
        self.sequencia = sequencia;
        self.anterior = anterior;
    }
}

/// Reads the attempt in a signed link.
pub trait LerRegistro {
    fn registro(&self) -> Result<RegistroTentativa>;
}

impl LerRegistro for TentativaAssinada {
    fn registro(&self) -> Result<RegistroTentativa> {
        self.ler()
            .map_err(|e| Erro::Invalido(format!("tentativa: {e}")))
    }
}

pub use utsim_diario::sha256_hex;

/// Signs `r`, completing its chain fields from `anterior` (the last attempt of the journal).
pub fn assinar(
    chave: &Chave,
    r: RegistroTentativa,
    anterior: Option<&TentativaAssinada>,
) -> Result<TentativaAssinada> {
    utsim_diario::assinar(chave, r, anterior).map_err(Erro::from)
}

/// The attempts that pass, in order, and every problem found.
#[derive(Clone, Debug, Default)]
pub struct Verificacao {
    pub validas: Vec<RegistroTentativa>,
    pub avisos: Vec<String>,
}

/// Checks signatures, hashes and the chain of a journal signed by `publica`, for student
/// `aluno`. Attempts with a bad signature or hash are left out; a broken chain is flagged
/// but the attempts still count (the teacher decides).
pub fn verificar_diario(
    publica: &str,
    aluno: &str,
    tentativas: &[TentativaAssinada],
) -> Verificacao {
    let c = utsim_diario::conferir::<RegistroTentativa>(publica, aluno, tentativas);
    let mut avisos: Vec<String> = Vec::new();
    for p in &c.problemas {
        let a = match p {
            Problema::HashNaoConfere { .. } => {
                "tentativa com hash que não confere (arquivo alterado)".to_string()
            }
            Problema::AssinaturaInvalida { .. } => {
                "tentativa com assinatura inválida (arquivo alterado)".to_string()
            }
            Problema::OutroAutor { autor, .. } => {
                format!("tentativa assinada por outro aluno ({autor})")
            }
            Problema::CorpoInvalido { erro, .. } => format!("tentativa: {erro}"),
            Problema::Falta { .. } | Problema::ForaDeOrdem { .. } | Problema::Bifurcacao { .. } => {
                "cadeia de tentativas quebrada: falta tentativa ou a ordem foi mudada".to_string()
            }
        };
        if !avisos.contains(&a) {
            avisos.push(a);
        }
    }
    Verificacao {
        validas: c.validos,
        avisos,
    }
}
