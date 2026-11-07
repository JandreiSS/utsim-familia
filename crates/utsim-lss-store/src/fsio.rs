//! File primitives that are safe in a folder synced by a desktop client. They live in the
//! common crate `utsim-pasta` (decision G6 of `docs/plano_qualidade_e_seguranca.md`) and are
//! re-exported here with this crate's [`Error`].

use std::path::Path;

use crate::Error;

pub use utsim_pasta::{is_candidate, json_files, json_files_deep};

impl From<utsim_pasta::Error> for Error {
    fn from(e: utsim_pasta::Error) -> Self {
        match e {
            utsim_pasta::Error::Io { path, source } => Error::Io { path, source },
            utsim_pasta::Error::Exists(p) => Error::Exists(p),
        }
    }
}

/// Writes a new file; fails with [`Error::Exists`] if the name is taken. Never overwrites.
pub fn write_new(target: &Path, bytes: &[u8]) -> Result<(), Error> {
    Ok(utsim_pasta::write_new(target, bytes)?)
}

/// Writes or replaces a file atomically (single-writer files only).
pub fn write_replace(target: &Path, bytes: &[u8]) -> Result<(), Error> {
    Ok(utsim_pasta::write_replace(target, bytes)?)
}

/// Reads a whole file as text.
pub fn read_text(path: &Path) -> Result<String, Error> {
    Ok(utsim_pasta::read_text(path)?)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn errors_keep_their_kind() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("x.json");
        write_new(&p, b"1").unwrap();
        assert!(matches!(write_new(&p, b"2"), Err(Error::Exists(_))));
        assert!(matches!(
            read_text(&dir.path().join("nada")),
            Err(Error::Io { .. })
        ));
    }
}
