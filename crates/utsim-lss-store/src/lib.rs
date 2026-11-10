//! Storage of UTSim Seis Sigma (`docs/plano_lean_seis_sigma.md`, 4.4 and 4.5): the project
//! folder `<nome>.lss/`, made to live in a folder synced by a desktop client (the team's
//! shared Google Drive) without any server.
//!
//! ```text
//! Melhoria da linha 2.lss/
//!   projeto.json              manifest (written once)
//!   instantaneos/             full state up to some operations (anyone may write one)
//!     000184-<uuid>.json
//!   ops/AAAA/MM/<uuid>-<pessoa>.json   one file per operation, never changed
//!   dados/                    imported datasets (immutable)
//!   anexos/<sha256>.<ext>     attachments by content hash
//!   modelos/                  copy of a linked UTSim model
//!   presenca/<pessoa>.json    heartbeat: who has the project open, on which screen
//!   cadeia/<pessoa>-<chave>/  signed links of each person's operations (P7)
//!   assinaturas/              sealed approvals (P7)
//! ```
//!
//! - [`assinatura`]: the key of each person, the signed chain of the journal, its check
//!   on opening and the sealed approvals (stage P7);
//! - [`fsio`]: write-once and atomic writes, names worth reading;
//! - [`project`]: open, create, commit edits, pick up other people's operations, snapshots,
//!   presence, attachments, the local cache;
//! - [`program`]: the program folder above the portfolios (stage L6): finding the
//!   portfolios and projects, the coaching logs, the rewriting after an anonymisation;
//! - [`lssz`]: export and import of a project as one `.lssz` file (a zip of the folder);
//! - [`user`]: the person using this computer and the app preferences (outside the project).

use std::path::{Path, PathBuf};

use utsim_lss_core::Id;
use utsim_lss_core::format::FormatError;

pub mod assinatura;
pub mod fsio;
pub mod intake;
pub mod lssz;
pub mod planilha;
pub mod program;
pub mod project;
pub mod user;

pub use project::{ProjectFolder, Refresh};
pub use user::{Identity, UserConfig};

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("{}: {source}", path.display())]
    Io {
        path: PathBuf,
        source: std::io::Error,
    },
    #[error("já existe: {}", .0.display())]
    Exists(PathBuf),
    #[error("{} não é uma pasta de projeto do UTSim Seis Sigma (falta projeto.json)", .0.display())]
    NotAProject(PathBuf),
    #[error("{}: {source}", path.display())]
    Format { path: PathBuf, source: FormatError },
    #[error("arquivo .lssz inválido: {0}")]
    Archive(String),
    #[error("{0}")]
    Invalid(String),
}

impl Error {
    pub fn io(path: &Path, source: std::io::Error) -> Self {
        Error::Io {
            path: path.to_path_buf(),
            source,
        }
    }

    pub fn format(path: &Path, source: FormatError) -> Self {
        Error::Format {
            path: path.to_path_buf(),
            source,
        }
    }
}

pub type Result<T, E = Error> = std::result::Result<T, E>;

/// Time and randomness for new ids. The system one in the app; a fixed one in tests.
pub trait Clock {
    /// Unix time, milliseconds.
    fn now_ms(&self) -> i64;
    fn random(&self) -> [u8; 10];

    /// A new UUID v7.
    fn new_id(&self) -> Id {
        Id::v7(self.now_ms().max(0) as u64, self.random())
    }

    /// Offset of the local time from UTC now, minutes (for "today"). Tests keep UTC.
    fn utc_offset_minutes(&self) -> i32 {
        0
    }
}

/// The system clock and the operating system's random generator.
#[derive(Debug, Clone, Copy, Default)]
pub struct SystemClock;

impl Clock for SystemClock {
    fn now_ms(&self) -> i64 {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_millis() as i64)
    }

    fn random(&self) -> [u8; 10] {
        let mut r = [0u8; 10];
        if getrandom::getrandom(&mut r).is_err() {
            // Extremely unlikely; fall back to the clock's nanoseconds (still unique enough
            // together with the millisecond prefix of the id).
            let n = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_or(0, |d| d.as_nanos());
            r.copy_from_slice(&n.to_le_bytes()[..10]);
        }
        r
    }

    fn utc_offset_minutes(&self) -> i32 {
        use chrono::Offset;
        chrono::Local::now().offset().fix().local_minus_utc() / 60
    }
}
