//! Ed25519 keys of the student (signs each attempt) and of the teacher (named in the
//! class invitation). Created on first use; the private key never leaves the profile.

use std::path::Path;

use ed25519_dalek::{Signature, Signer, SigningKey, Verifier, VerifyingKey};

use crate::{Erro, Result, hex, unhex};

const CABECALHO: &str = "utsim-ensino-chave-ed25519 v1";

/// A signing key.
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

    /// Reads the key at `path`, creating it when the file does not exist.
    pub fn carregar_ou_criar(path: &Path) -> Result<Chave> {
        if path.exists() {
            let txt = std::fs::read_to_string(path)?;
            let mut l = txt.lines();
            if l.next().map(str::trim) != Some(CABECALHO) {
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
        std::fs::write(path, format!("{CABECALHO}\n{}\n", hex(&c.0.to_bytes())))?;
        Ok(c)
    }

    /// The public key in hex.
    pub fn publica(&self) -> String {
        hex(self.0.verifying_key().as_bytes())
    }

    /// Hex signature of `msg`.
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
