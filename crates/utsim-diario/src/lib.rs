//! The signed journal shared by UTSim Ensino and UTSim LSS (decision E12 of
//! `docs/plano_ensino_e_jogo.md`, base of P7 of `docs/plano_lean_seis_sigma.md`).
//!
//! Only primitives, no format of either app and no texts:
//!
//! - [`Chave`]: an ed25519 key per person, kept in the profile, public key in hex;
//! - [`Elo`]: a signed link. The body is kept as text, so the hash and the signature cover
//!   those exact bytes (no JSON canonicalisation);
//! - [`Encadeado`]: the record of each app carries its own chain per author (sequence and
//!   hash of the previous link of the same author);
//! - [`assinar`], [`conferir`] and [`conferir_desde`]: sign the next link and check a
//!   chain, whole or from the last link already checked;
//! - [`Selo`]: a detached signature (approval, certificate, list of roles). Several seals
//!   over the same body are several people signing it.
//!
//! A check returns [`Problema`]s without text: each app writes the message in its own
//! catalogue and language. Tampering is revealed, never prevented: whoever has the files can
//! edit them, and none of this is a legally valid digital signature.

use std::collections::BTreeMap;
use std::path::Path;

use ed25519_dalek::{Signature, Signer, SigningKey, Verifier, VerifyingKey};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

#[derive(Debug, thiserror::Error)]
pub enum Erro {
    #[error("{0}")]
    Io(#[from] std::io::Error),
    #[error("{0}")]
    Invalido(String),
}

pub type Result<T> = std::result::Result<T, Erro>;

/// Lower-case hex of `bytes`.
pub fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// Bytes of a hex string.
pub fn unhex(s: &str) -> Result<Vec<u8>> {
    if s.len() % 2 != 0 {
        return Err(Erro::Invalido("hexadecimal de tamanho ímpar".into()));
    }
    (0..s.len())
        .step_by(2)
        .map(|i| {
            u8::from_str_radix(s.get(i..i + 2).unwrap_or("zz"), 16)
                .map_err(|_| Erro::Invalido("hexadecimal inválido".into()))
        })
        .collect()
}

/// SHA-256 of `bytes` in hex.
pub fn sha256_hex(bytes: &[u8]) -> String {
    hex(&Sha256::digest(bytes))
}

/// An ed25519 signing key. The private part never leaves the profile.
pub struct Chave(SigningKey);

impl Chave {
    /// A new random key.
    pub fn gerar() -> Result<Chave> {
        let mut seed = [0u8; 32];
        getrandom::getrandom(&mut seed)
            .map_err(|e| Erro::Io(std::io::Error::other(e.to_string())))?;
        Ok(Chave(SigningKey::from_bytes(&seed)))
    }

    /// A key from a fixed seed (tests and synthetic classes).
    pub fn de_semente(seed: [u8; 32]) -> Chave {
        Chave(SigningKey::from_bytes(&seed))
    }

    /// Reads the key at `path` (`<cabecalho>\n<hex of the seed>\n`), creating it when the
    /// file does not exist. Each app passes its own header, so a key file of one app is not
    /// taken by another by mistake.
    pub fn carregar_ou_criar(path: &Path, cabecalho: &str) -> Result<Chave> {
        if path.exists() {
            let txt = std::fs::read_to_string(path)?;
            let mut l = txt.lines();
            if l.next().map(str::trim) != Some(cabecalho) {
                return Err(Erro::Invalido(format!(
                    "{}: chave desconhecida",
                    path.display()
                )));
            }
            let bytes = unhex(l.next().unwrap_or("").trim())?;
            let seed: [u8; 32] = bytes
                .try_into()
                .map_err(|_| Erro::Invalido("chave deve ter 32 bytes".into()))?;
            return Ok(Chave(SigningKey::from_bytes(&seed)));
        }
        let c = Chave::gerar()?;
        if let Some(d) = path.parent() {
            std::fs::create_dir_all(d)?;
        }
        std::fs::write(path, format!("{cabecalho}\n{}\n", hex(&c.0.to_bytes())))?;
        Ok(c)
    }

    /// The public key in hex (32 bytes).
    pub fn publica(&self) -> String {
        hex(self.0.verifying_key().as_bytes())
    }

    /// Hex signature (64 bytes) of `msg`.
    pub fn assinar(&self, msg: &[u8]) -> String {
        hex(&self.0.sign(msg).to_bytes())
    }
}

/// Checks a hex signature of `msg` by the hex public key `publica`.
pub fn verificar(publica: &str, msg: &[u8], assinatura: &str) -> bool {
    let Ok(pk) = unhex(publica) else {
        return false;
    };
    let Ok(pk): std::result::Result<[u8; 32], _> = pk.try_into() else {
        return false;
    };
    let Ok(vk) = VerifyingKey::from_bytes(&pk) else {
        return false;
    };
    let Ok(sig) = unhex(assinatura) else {
        return false;
    };
    let Ok(sig): std::result::Result<[u8; 64], _> = sig.try_into() else {
        return false;
    };
    vk.verify(msg, &Signature::from_bytes(&sig)).is_ok()
}

/// A signed link: the body exactly as signed, its hash and the signature.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct Elo {
    pub corpo: String,
    pub hash: String,
    pub assinatura: String,
}

impl Elo {
    /// The record in the body.
    pub fn ler<T: DeserializeOwned>(&self) -> Result<T> {
        serde_json::from_str(&self.corpo).map_err(|e| Erro::Invalido(e.to_string()))
    }
}

/// A record that carries its chain per author.
pub trait Encadeado {
    /// Who signs (student record, person id).
    fn autor(&self) -> &str;
    /// 1 for the first link of the author.
    fn sequencia(&self) -> u64;
    /// Hash of the previous link of the same author, empty for the first.
    fn anterior(&self) -> &str;
    /// Sets the chain fields (called by [`assinar`]).
    fn encadear(&mut self, sequencia: u64, anterior: String);
}

/// Signs `r` as the link after `ultimo` (the last link of the same author), filling its
/// sequence and previous hash.
pub fn assinar<T: Encadeado + Serialize + DeserializeOwned>(
    chave: &Chave,
    mut r: T,
    ultimo: Option<&Elo>,
) -> Result<Elo> {
    match ultimo {
        Some(u) => {
            let anterior: T = u.ler()?;
            r.encadear(anterior.sequencia() + 1, u.hash.clone());
        }
        None => r.encadear(1, String::new()),
    }
    let corpo = serde_json::to_string(&r).map_err(|e| Erro::Invalido(e.to_string()))?;
    Ok(Elo {
        hash: sha256_hex(corpo.as_bytes()),
        assinatura: chave.assinar(corpo.as_bytes()),
        corpo,
    })
}

/// What a check found. `indice` is the position in the slice given to the check.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Problema {
    /// The body does not match the hash (file edited). The link is left out.
    HashNaoConfere { indice: usize },
    /// The signature does not match the public key. The link is left out.
    AssinaturaInvalida { indice: usize },
    /// The link was signed for another author. The link is left out.
    OutroAutor { indice: usize, autor: String },
    /// The body is not a record of the app. The link is left out.
    CorpoInvalido { indice: usize, erro: String },
    /// Links are missing after `depois_de` (0: the start is missing). Only a hole before
    /// a link that is present: a missing end is never reported, because a shared folder
    /// syncs late and a copy may have only the start of another author's chain.
    Falta { depois_de: u64 },
    /// The link `sequencia` does not point to the link before it.
    ForaDeOrdem { sequencia: u64 },
    /// Two different links with the same `sequencia` (a backup restored and written on).
    Bifurcacao { sequencia: u64 },
}

/// The links that pass, in chain order, and every problem found.
#[derive(Clone, Debug)]
pub struct Conferencia<T> {
    pub validos: Vec<T>,
    /// The last valid link in chain order: the starting point of the next
    /// [`conferir_desde`].
    pub ultimo: Option<Elo>,
    pub problemas: Vec<Problema>,
}

impl<T> Conferencia<T> {
    /// No problem at all.
    pub fn limpa(&self) -> bool {
        self.problemas.is_empty()
    }
}

/// Checks hashes, signatures (by `publica`), author and the chain of `elos`, whole.
///
/// A link with a bad hash, signature, author or body is left out. A broken chain (hole,
/// order, bifurcation) is reported, but its links still count: the person decides. The same
/// link twice (a duplicate copy of the synced folder) counts once.
pub fn conferir<T: Encadeado + DeserializeOwned>(
    publica: &str,
    autor: &str,
    elos: &[Elo],
) -> Conferencia<T> {
    conferir_desde(publica, autor, None, elos)
}

/// Checks only `novos`, after `ultimo`, the last link already checked (the `ultimo` of a
/// previous [`Conferencia`]). With `ultimo` = `None` it is [`conferir`].
///
/// A new link with a sequence up to that of `ultimo` and another hash is a
/// [`Problema::Bifurcacao`] (the caller passes only links it has not checked yet), and it
/// counts. `ultimo` itself among `novos` is ignored.
pub fn conferir_desde<T: Encadeado + DeserializeOwned>(
    publica: &str,
    autor: &str,
    ultimo: Option<&Elo>,
    novos: &[Elo],
) -> Conferencia<T> {
    let mut problemas = Vec::new();
    let (seq0, hash0) = match ultimo.map(Elo::ler::<T>) {
        Some(Ok(r)) => (
            r.sequencia(),
            ultimo.map(|u| u.hash.clone()).unwrap_or_default(),
        ),
        _ => (0, String::new()),
    };
    // Valid links by sequence, without repeated hashes.
    let mut por_seq: BTreeMap<u64, Vec<(T, &Elo)>> = BTreeMap::new();
    let mut vistos = std::collections::BTreeSet::new();
    vistos.insert(hash0.clone());
    for (indice, e) in novos.iter().enumerate() {
        if sha256_hex(e.corpo.as_bytes()) != e.hash {
            problemas.push(Problema::HashNaoConfere { indice });
            continue;
        }
        if !verificar(publica, e.corpo.as_bytes(), &e.assinatura) {
            problemas.push(Problema::AssinaturaInvalida { indice });
            continue;
        }
        let r: T = match e.ler() {
            Ok(r) => r,
            Err(x) => {
                problemas.push(Problema::CorpoInvalido {
                    indice,
                    erro: x.to_string(),
                });
                continue;
            }
        };
        if r.autor() != autor {
            problemas.push(Problema::OutroAutor {
                indice,
                autor: r.autor().to_string(),
            });
            continue;
        }
        if !vistos.insert(e.hash.clone()) {
            continue;
        }
        por_seq.entry(r.sequencia()).or_default().push((r, e));
    }
    // The chain: each sequence follows the one before, and points to one of its links.
    let mut validos = Vec::new();
    let mut ultimo_valido = ultimo.cloned();
    let mut seq_ant = seq0;
    let mut hashes_ant: Vec<String> = vec![hash0];
    for (seq, grupo) in por_seq {
        if grupo.len() > 1 || (seq <= seq0 && seq0 > 0) {
            problemas.push(Problema::Bifurcacao { sequencia: seq });
        }
        if seq <= seq0 && seq0 > 0 {
            // An older branch: it counts, but the chain goes on from `ultimo`.
            validos.extend(grupo.into_iter().map(|(r, _)| r));
            continue;
        }
        if seq != seq_ant + 1 {
            problemas.push(Problema::Falta { depois_de: seq_ant });
        } else if grupo
            .iter()
            .any(|(r, _)| !hashes_ant.iter().any(|h| h == r.anterior()))
        {
            problemas.push(Problema::ForaDeOrdem { sequencia: seq });
        }
        seq_ant = seq;
        hashes_ant = grupo.iter().map(|(_, e)| e.hash.clone()).collect();
        ultimo_valido = grupo.last().map(|(_, e)| (*e).clone());
        validos.extend(grupo.into_iter().map(|(r, _)| r));
    }
    Conferencia {
        validos,
        ultimo: ultimo_valido,
        problemas,
    }
}

/// A detached signature over a body: approval, certificate, list of roles, key change.
/// Several people sign the same body with several seals ([`Selo::hash_corpo`] tells them
/// apart). `publica` is always the key of whoever signed, never a key in the content.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct Selo {
    pub corpo: String,
    pub publica: String,
    pub assinatura: String,
    /// Milliseconds since 1970 (UTC), given by the caller (never read from the clock here).
    pub quando: i64,
}

/// What a seal signs: the instant and the body, so neither changes unnoticed.
fn mensagem_selo(quando: i64, corpo: &str) -> Vec<u8> {
    format!("{quando}\n{corpo}").into_bytes()
}

impl Selo {
    /// SHA-256 of the body: two seals with the same value sign the same content.
    pub fn hash_corpo(&self) -> String {
        sha256_hex(self.corpo.as_bytes())
    }
}

/// Seals `corpo` with `chave` at `quando`.
pub fn selar<T: Serialize>(chave: &Chave, corpo: &T, quando: i64) -> Result<Selo> {
    let corpo = serde_json::to_string(corpo).map_err(|e| Erro::Invalido(e.to_string()))?;
    Ok(Selo {
        assinatura: chave.assinar(&mensagem_selo(quando, &corpo)),
        publica: chave.publica(),
        corpo,
        quando,
    })
}

/// The body of `selo` when its signature matches `selo.publica`. Whether that key belongs
/// to the right person is for the caller's keyring.
pub fn conferir_selo<T: DeserializeOwned>(selo: &Selo) -> Result<T> {
    if !verificar(
        &selo.publica,
        &mensagem_selo(selo.quando, &selo.corpo),
        &selo.assinatura,
    ) {
        return Err(Erro::Invalido("selo com assinatura inválida".into()));
    }
    serde_json::from_str(&selo.corpo).map_err(|e| Erro::Invalido(e.to_string()))
}
