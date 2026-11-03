//! Ed25519 keys and signatures of the manifest.
//!
//! The private key lives only on the author's machine (written by `utsim-publicar`); the
//! launcher carries the public key(s) it trusts, built in at compile time. Public keys are
//! written as `ed25519:<base64 of 32 bytes>`; a key is named in the manifest by its *id*, the
//! first 8 bytes of SHA-256 of the key in hex, which allows rotating keys later (the launcher
//! may trust an old and a new key at the same time).

use base64::Engine;
use base64::engine::general_purpose::STANDARD as B64;
use ed25519_dalek::{Signature, Signer, SigningKey, VerifyingKey};
use zeroize::Zeroize;

use crate::{Error, Result};

const PUBLIC_PREFIX: &str = "ed25519:";
const PRIVATE_HEADER: &str = "utsim-chave-privada-ed25519 v1";

/// A new random signing key.
pub fn generate() -> Result<SigningKey> {
    let mut seed = [0u8; 32];
    crate::random_bytes(&mut seed)?;
    let key = SigningKey::from_bytes(&seed);
    seed.zeroize();
    Ok(key)
}

/// `ed25519:<base64>` form of a public key.
pub fn public_to_text(key: &VerifyingKey) -> String {
    format!("{PUBLIC_PREFIX}{}", B64.encode(key.as_bytes()))
}

/// Parses `ed25519:<base64>`.
pub fn public_from_text(text: &str) -> Result<VerifyingKey> {
    let b64 = text
        .trim()
        .strip_prefix(PUBLIC_PREFIX)
        .ok_or_else(|| Error::Signature("chave pública sem o prefixo 'ed25519:'".into()))?;
    let bytes: [u8; 32] = B64
        .decode(b64)
        .map_err(|e| Error::Signature(format!("chave pública em base64 inválida: {e}")))?
        .as_slice()
        .try_into()
        .map_err(|_| Error::Signature("chave pública deve ter 32 bytes".into()))?;
    VerifyingKey::from_bytes(&bytes).map_err(|e| Error::Signature(format!("chave pública: {e}")))
}

/// Short id of a public key (16 hex digits).
pub fn key_id(key: &VerifyingKey) -> String {
    crate::sha256_hex(key.as_bytes())[..16].to_string()
}

/// Text file contents for a private key. Keep it out of any repository.
pub fn private_to_text(key: &SigningKey) -> String {
    format!(
        "{PRIVATE_HEADER}\n{}\n",
        B64.encode(key.to_bytes().as_slice())
    )
}

pub fn private_from_text(text: &str) -> Result<SigningKey> {
    let mut lines = text.lines();
    if lines.next().map(str::trim) != Some(PRIVATE_HEADER) {
        return Err(Error::Signature(
            "arquivo de chave privada desconhecido".into(),
        ));
    }
    let mut bytes = B64
        .decode(lines.next().unwrap_or("").trim())
        .map_err(|e| Error::Signature(format!("chave privada em base64 inválida: {e}")))?;
    let seed: [u8; 32] = bytes
        .as_slice()
        .try_into()
        .map_err(|_| Error::Signature("chave privada deve ter 32 bytes".into()))?;
    bytes.zeroize();
    Ok(SigningKey::from_bytes(&seed))
}

/// Base64 signature of `message`.
pub fn sign(key: &SigningKey, message: &[u8]) -> String {
    B64.encode(key.sign(message).to_bytes())
}

/// Checks `signature` (base64) of `message` by the trusted key named `id`.
pub fn verify(trusted: &[VerifyingKey], id: &str, message: &[u8], signature: &str) -> Result<()> {
    if trusted.is_empty() {
        return Err(Error::Signature(
            "este launcher não tem chave pública embutida (build de desenvolvimento)".into(),
        ));
    }
    let key = trusted
        .iter()
        .find(|k| key_id(k) == id)
        .ok_or_else(|| Error::Signature(format!("assinado por uma chave desconhecida ({id})")))?;
    let bytes: [u8; 64] = B64
        .decode(signature)
        .map_err(|e| Error::Signature(format!("base64 inválido: {e}")))?
        .as_slice()
        .try_into()
        .map_err(|_| Error::Signature("a assinatura deve ter 64 bytes".into()))?;
    key.verify_strict(message, &Signature::from_bytes(&bytes))
        .map_err(|_| Error::Signature("o manifesto foi alterado depois de assinado".into()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sign_verify_and_text_forms() {
        let sk = generate().unwrap();
        let vk = sk.verifying_key();
        let back = public_from_text(&public_to_text(&vk)).unwrap();
        assert_eq!(back, vk);
        let sk2 = private_from_text(&private_to_text(&sk)).unwrap();
        assert_eq!(sk2.to_bytes(), sk.to_bytes());

        let sig = sign(&sk, b"corpo");
        verify(&[vk], &key_id(&vk), b"corpo", &sig).unwrap();
        assert!(verify(&[vk], &key_id(&vk), b"corpo!", &sig).is_err());
        let other = generate().unwrap().verifying_key();
        assert!(verify(&[other], &key_id(&vk), b"corpo", &sig).is_err());
        assert!(verify(&[], &key_id(&vk), b"corpo", &sig).is_err());
    }
}
