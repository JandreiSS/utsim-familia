//! Core of the UTSim family launcher. No window, no global state: everything here is
//! plain functions over files and an abstract HTTP transport, so it is tested without
//! network access.
//!
//! The pieces, in the order a tester's machine uses them:
//! 1. [`manifest`]: the signed JSON envelope listing products, channels and versions
//!    ([`signature`] checks the ed25519 signature against the keys built into the launcher).
//! 2. [`code`]: the per-tester access code (`IDID-XXXX-XXXX-XXXX-XXXX`).
//! 3. [`download`]: resumable download over a [`transport::Transport`], going through the
//!    Google Drive virus-scan confirmation page ([`drive`]) when needed.
//! 4. [`crypto`]: the package is checked (size, SHA-256), its content key is unwrapped with
//!    the access code (Argon2id + XChaCha20-Poly1305) and the package is decrypted.
//! 5. [`install`]: extraction into a per-version folder and atomic switch of the active
//!    version; [`shortcuts`] and [`report`] (error report as a .zip) complete it.
//!
//! Messages meant for the tester ([`Error`]'s `Display`) are in Brazilian Portuguese.

pub mod code;
pub mod crypto;
pub mod download;
pub mod drive;
pub mod install;
pub mod manifest;
pub mod package;
pub mod report;
pub mod shortcuts;
pub mod signature;
pub mod state;
pub mod transport;

use std::io;

/// Every failure of the launcher core, with a message for the tester.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("erro de arquivo: {0}")]
    Io(#[from] io::Error),
    #[error("{0}")]
    Format(String),
    #[error(
        "este manifesto usa o formato {found}, mais novo do que este launcher entende (até {supported}). Atualize o launcher."
    )]
    FutureFormat { found: u32, supported: u32 },
    #[error("assinatura inválida: {0}")]
    Signature(String),
    #[error("código de acesso inválido: {0}")]
    Code(String),
    #[error(
        "este código de acesso não dá acesso a este pacote (código errado, revogado ou sem permissão para o canal)"
    )]
    NoAccess,
    #[error("o pacote está corrompido ou foi alterado: {0}")]
    Integrity(String),
    #[error("falha de rede: {0}")]
    Network(String),
    #[error("Google Drive: {0}")]
    Drive(String),
    #[error("operação cancelada")]
    Cancelled,
    #[error("{0}")]
    Install(String),
}

pub type Result<T> = std::result::Result<T, Error>;

/// Fills `buf` with bytes from the operating system's secure random source.
pub fn random_bytes(buf: &mut [u8]) -> Result<()> {
    getrandom::getrandom(buf).map_err(|e| Error::Io(io::Error::other(e.to_string())))
}

/// Lowercase hex SHA-256 of a byte slice.
pub fn sha256_hex(data: &[u8]) -> String {
    use sha2::Digest;
    hex::encode(sha2::Sha256::digest(data))
}

/// Lowercase hex SHA-256 of a file, read in blocks.
pub fn sha256_file(path: &std::path::Path) -> Result<String> {
    use sha2::Digest;
    use std::io::Read;
    let mut file = std::fs::File::open(path)?;
    let mut hasher = sha2::Sha256::new();
    let mut buf = vec![0u8; 1 << 16];
    loop {
        let n = file.read(&mut buf)?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    Ok(hex::encode(hasher.finalize()))
}
