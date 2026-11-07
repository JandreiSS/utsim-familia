//! File primitives that are safe in a folder synced by a desktop client (the user's Drive),
//! shared by the storage of UTSim Seis Sigma (`utsim-lss-store`, which re-exports them as
//! `fsio`) and by the unit folder of the quality and safety apps (`utsim-posto`). Decision
//! G6 of `docs/plano_qualidade_e_seguranca.md`.
//!
//! - [`write_new`] never replaces an existing file: the content goes to a temporary file in
//!   the same folder (hidden, `.tmp`), is flushed to disk and then linked or renamed to the
//!   final name. A reader therefore sees either nothing or the whole file.
//! - [`write_replace`] is for files with a single writer (one's own presence, the local
//!   cache): same temporary file, then an atomic rename over the old one.
//! - [`is_candidate`] says which names a reader should look at: sync clients leave
//!   temporary and partial files around (`.tmp`, `~$x`, `.x.partial`, `desktop.ini`).

use std::fs::{self, File, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("{}: {source}", path.display())]
    Io {
        path: PathBuf,
        source: std::io::Error,
    },
    #[error("já existe: {}", .0.display())]
    Exists(PathBuf),
}

impl Error {
    pub fn io(path: &Path, source: std::io::Error) -> Self {
        Error::Io {
            path: path.to_path_buf(),
            source,
        }
    }
}

/// Suffix of our temporary files.
const TMP_SUFFIX: &str = ".tmp";

fn temp_path(target: &Path) -> PathBuf {
    let mut r = [0u8; 6];
    let _ = getrandom::getrandom(&mut r);
    let tag: String = r.iter().map(|b| format!("{b:02x}")).collect();
    let name = target
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    target.with_file_name(format!(".{name}.{tag}{TMP_SUFFIX}"))
}

fn write_temp(target: &Path, bytes: &[u8]) -> Result<PathBuf, Error> {
    if let Some(dir) = target.parent() {
        fs::create_dir_all(dir).map_err(|e| Error::io(dir, e))?;
    }
    let tmp = temp_path(target);
    let mut f = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&tmp)
        .map_err(|e| Error::io(&tmp, e))?;
    let res = f.write_all(bytes).and_then(|_| f.sync_all());
    drop(f);
    if let Err(e) = res {
        let _ = fs::remove_file(&tmp);
        return Err(Error::io(&tmp, e));
    }
    Ok(tmp)
}

/// Writes a new file; fails with [`Error::Exists`] if the name is taken. Never overwrites.
pub fn write_new(target: &Path, bytes: &[u8]) -> Result<(), Error> {
    if target.exists() {
        return Err(Error::Exists(target.to_path_buf()));
    }
    let tmp = write_temp(target, bytes)?;
    // A hard link fails if the target exists: atomic and never overwrites. Some virtual
    // file systems of sync clients do not support links; then check and rename (the names
    // are unique ids, so the window between the two is harmless).
    match fs::hard_link(&tmp, target) {
        Ok(()) => {
            let _ = fs::remove_file(&tmp);
            Ok(())
        }
        Err(e) if e.kind() == io::ErrorKind::AlreadyExists => {
            let _ = fs::remove_file(&tmp);
            Err(Error::Exists(target.to_path_buf()))
        }
        Err(_) => {
            if target.exists() {
                let _ = fs::remove_file(&tmp);
                Err(Error::Exists(target.to_path_buf()))
            } else {
                fs::rename(&tmp, target).map_err(|e| {
                    let _ = fs::remove_file(&tmp);
                    Error::io(target, e)
                })
            }
        }
    }
}

/// Writes or replaces a file atomically (single-writer files only).
pub fn write_replace(target: &Path, bytes: &[u8]) -> Result<(), Error> {
    let tmp = write_temp(target, bytes)?;
    fs::rename(&tmp, target).map_err(|e| {
        let _ = fs::remove_file(&tmp);
        Error::io(target, e)
    })
}

/// Whether a file name is worth reading as a JSON document of the project.
pub fn is_candidate(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    lower.ends_with(".json")
        && !lower.starts_with('.')
        && !lower.starts_with('~')
        && !lower.starts_with("~$")
}

/// JSON files directly in `dir` (none if it does not exist), sorted by name.
pub fn json_files(dir: &Path) -> Vec<PathBuf> {
    let mut v: Vec<PathBuf> = fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .filter(|e| e.file_type().is_ok_and(|t| t.is_file()))
        .filter(|e| is_candidate(&e.file_name().to_string_lossy()))
        .map(|e| e.path())
        .collect();
    v.sort();
    v
}

/// JSON files anywhere under `dir`, up to `depth` levels, sorted.
pub fn json_files_deep(dir: &Path, depth: usize) -> Vec<PathBuf> {
    let mut out = json_files(dir);
    if depth > 0 {
        let mut subdirs: Vec<PathBuf> = fs::read_dir(dir)
            .into_iter()
            .flatten()
            .flatten()
            .filter(|e| e.file_type().is_ok_and(|t| t.is_dir()))
            .filter(|e| !e.file_name().to_string_lossy().starts_with('.'))
            .map(|e| e.path())
            .collect();
        subdirs.sort();
        for d in subdirs {
            out.extend(json_files_deep(&d, depth - 1));
        }
    }
    out
}

/// Reads a whole file as text.
pub fn read_text(path: &Path) -> Result<String, Error> {
    let mut f = File::open(path).map_err(|e| Error::io(path, e))?;
    let mut s = String::new();
    io::Read::read_to_string(&mut f, &mut s).map_err(|e| Error::io(path, e))?;
    Ok(s)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn write_new_never_overwrites() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("a").join("x.json");
        write_new(&p, b"1").unwrap();
        assert!(matches!(write_new(&p, b"2"), Err(Error::Exists(_))));
        assert_eq!(fs::read(&p).unwrap(), b"1");
        // No temporary files left behind.
        let names: Vec<_> = fs::read_dir(p.parent().unwrap())
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        assert_eq!(names, ["x.json"]);
        write_replace(&p, b"3").unwrap();
        assert_eq!(fs::read(&p).unwrap(), b"3");
    }

    #[test]
    fn candidates_skip_sync_leftovers() {
        assert!(is_candidate("0192-ab.json"));
        assert!(is_candidate("0192-ab (1).json"));
        assert!(is_candidate("X.JSON"));
        for bad in [
            ".0192.json.ab12.tmp",
            ".x.json",
            "~$x.json",
            "x.json.partial",
            "desktop.ini",
            "x.tmp",
        ] {
            assert!(!is_candidate(bad), "{bad}");
        }
    }

    #[test]
    fn deep_listing() {
        let dir = tempfile::tempdir().unwrap();
        write_new(&dir.path().join("2026/11/a.json"), b"{}").unwrap();
        write_new(&dir.path().join("2026/12/b.json"), b"{}").unwrap();
        write_new(&dir.path().join("c.json"), b"{}").unwrap();
        fs::write(dir.path().join("2026/11/.a.json.1.tmp"), b"{").unwrap();
        let names: Vec<_> = json_files_deep(dir.path(), 2)
            .iter()
            .map(|p| p.file_name().unwrap().to_string_lossy().into_owned())
            .collect();
        assert_eq!(names, ["c.json", "a.json", "b.json"]);
        assert!(json_files(&dir.path().join("nada")).is_empty());
    }
}
