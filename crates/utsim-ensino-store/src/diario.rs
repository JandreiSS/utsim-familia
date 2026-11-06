//! The signed attempt journal: each attempt is a JSON body signed by the student's key and
//! chained by hash to the previous one, with date and time. The body carries decisions
//! and answers, never the grade the teacher will use: the teacher simulates again.

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use utsim_ensino_core::nota::Tentativa;

use crate::chave::{Chave, verificar};
use crate::{Erro, Result, hex};

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

/// A signed attempt: the body exactly as signed, its hash and the signature.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct TentativaAssinada {
    pub corpo: String,
    pub hash: String,
    pub assinatura: String,
}

impl TentativaAssinada {
    pub fn registro(&self) -> Result<RegistroTentativa> {
        serde_json::from_str(&self.corpo).map_err(|e| Erro::Invalido(format!("tentativa: {e}")))
    }
}

pub fn sha256_hex(bytes: &[u8]) -> String {
    hex(&Sha256::digest(bytes))
}

/// Signs `r`, completing its chain fields from `anterior` (the last attempt of the journal).
pub fn assinar(
    chave: &Chave,
    mut r: RegistroTentativa,
    anterior: Option<&TentativaAssinada>,
) -> Result<TentativaAssinada> {
    match anterior {
        Some(a) => {
            r.sequencia = a.registro()?.sequencia + 1;
            r.anterior = a.hash.clone();
        }
        None => {
            r.sequencia = 1;
            r.anterior = String::new();
        }
    }
    let corpo = serde_json::to_string(&r).map_err(|e| Erro::Invalido(e.to_string()))?;
    Ok(TentativaAssinada {
        hash: sha256_hex(corpo.as_bytes()),
        assinatura: chave.assinar(corpo.as_bytes()),
        corpo,
    })
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
    let mut v = Verificacao::default();
    let mut lidas: Vec<(RegistroTentativa, &TentativaAssinada)> = Vec::new();
    for t in tentativas {
        if sha256_hex(t.corpo.as_bytes()) != t.hash {
            v.avisos
                .push("tentativa com hash que não confere (arquivo alterado)".into());
            continue;
        }
        if !verificar(publica, t.corpo.as_bytes(), &t.assinatura) {
            v.avisos
                .push("tentativa com assinatura inválida (arquivo alterado)".into());
            continue;
        }
        match t.registro() {
            Ok(r) if r.aluno != aluno => v
                .avisos
                .push(format!("tentativa assinada por outro aluno ({})", r.aluno)),
            Ok(r) => lidas.push((r, t)),
            Err(e) => v.avisos.push(e.to_string()),
        }
    }
    lidas.sort_by_key(|(r, _)| r.sequencia);
    let mut esperado = 1;
    let mut hash_anterior = String::new();
    let mut quebrada = false;
    for (r, t) in &lidas {
        if r.sequencia != esperado || r.anterior != hash_anterior {
            quebrada = true;
        }
        esperado = r.sequencia + 1;
        hash_anterior = t.hash.clone();
    }
    if quebrada {
        v.avisos
            .push("cadeia de tentativas quebrada: falta tentativa ou a ordem foi mudada".into());
    }
    v.validas = lidas.into_iter().map(|(r, _)| r).collect();
    v
}
