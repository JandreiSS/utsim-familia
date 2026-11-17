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

/// One writer per model file (`docs/plano_integracao_familia.md`, section 3.2): whoever
/// opens a model to edit it writes `.<file name>.trava` next to it and renews it while the
/// model is open. Another app finds it and opens the model read only. A lock not renewed
/// for [`TRAVA_EXPIRA_MS`] is abandoned (a crash, a machine turned off).
pub mod trava {
    use std::path::{Path, PathBuf};

    use serde::{Deserialize, Serialize};

    use crate::{Error, write_new, write_replace};

    /// A lock older than this without renewal is ignored.
    pub const TRAVA_EXPIRA_MS: u64 = 5 * 60 * 1000;
    /// How often the holder renews it.
    pub const RENOVAR_A_CADA_MS: u64 = 60 * 1000;

    #[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
    pub struct Trava {
        /// Product holding it (`utsim`, `qualidade`, `seguranca`...).
        pub app: String,
        /// Who, as the app shows them.
        pub pessoa: String,
        /// Computer, to tell two windows of the same person apart.
        #[serde(default)]
        pub maquina: String,
        pub desde_ms: u64,
        pub renovada_ms: u64,
    }

    impl Trava {
        pub fn expirada(&self, agora_ms: u64) -> bool {
            agora_ms.saturating_sub(self.renovada_ms) > TRAVA_EXPIRA_MS
        }

        fn mesma_dona(&self, app: &str, pessoa: &str, maquina: &str) -> bool {
            self.app == app && self.pessoa == pessoa && self.maquina == maquina
        }
    }

    /// The result of trying to take the lock.
    #[derive(Clone, Debug, PartialEq, Eq)]
    pub enum Estado {
        /// The lock is ours: edit and save.
        Minha,
        /// Someone else edits: open read only and say who.
        DeOutro(Trava),
    }

    pub fn agora_ms() -> u64 {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_millis() as u64)
    }

    /// The lock file of `modelo`.
    pub fn caminho(modelo: &Path) -> PathBuf {
        let nome = modelo
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        modelo.with_file_name(format!(".{nome}.trava"))
    }

    /// The current lock, when there is one that is not abandoned.
    pub fn ler(modelo: &Path, agora_ms: u64) -> Option<Trava> {
        let texto = std::fs::read_to_string(caminho(modelo)).ok()?;
        let t: Trava = serde_json::from_str(&texto).ok()?;
        (!t.expirada(agora_ms)).then_some(t)
    }

    /// Takes the lock for `app`/`pessoa`/`maquina`, or says who holds it.
    pub fn travar(
        modelo: &Path,
        app: &str,
        pessoa: &str,
        maquina: &str,
        agora_ms: u64,
    ) -> Result<Estado, Error> {
        let nova = Trava {
            app: app.into(),
            pessoa: pessoa.into(),
            maquina: maquina.into(),
            desde_ms: agora_ms,
            renovada_ms: agora_ms,
        };
        let bytes = serde_json::to_vec_pretty(&nova).expect("trava");
        let p = caminho(modelo);
        match write_new(&p, &bytes) {
            Ok(()) => Ok(Estado::Minha),
            Err(Error::Exists(_)) => match ler(modelo, agora_ms) {
                Some(t) if !t.mesma_dona(app, pessoa, maquina) => Ok(Estado::DeOutro(t)),
                // Ours from before, or abandoned: take it over.
                _ => {
                    write_replace(&p, &bytes)?;
                    Ok(Estado::Minha)
                }
            },
            Err(e) => Err(e),
        }
    }

    /// Renews our lock; fails with the holder when someone else took it.
    pub fn renovar(
        modelo: &Path,
        app: &str,
        pessoa: &str,
        maquina: &str,
        agora_ms: u64,
    ) -> Result<Estado, Error> {
        match ler(modelo, agora_ms) {
            Some(mut t) if t.mesma_dona(app, pessoa, maquina) => {
                t.renovada_ms = agora_ms;
                write_replace(
                    &caminho(modelo),
                    &serde_json::to_vec_pretty(&t).expect("trava"),
                )?;
                Ok(Estado::Minha)
            }
            Some(t) => Ok(Estado::DeOutro(t)),
            None => travar(modelo, app, pessoa, maquina, agora_ms),
        }
    }

    /// Releases our lock (someone else's is left alone).
    pub fn soltar(modelo: &Path, app: &str, pessoa: &str, maquina: &str) -> Result<(), Error> {
        let p = caminho(modelo);
        let Ok(texto) = std::fs::read_to_string(&p) else {
            return Ok(());
        };
        if let Ok(t) = serde_json::from_str::<Trava>(&texto)
            && t.mesma_dona(app, pessoa, maquina)
        {
            std::fs::remove_file(&p).map_err(|e| Error::io(&p, e))?;
        }
        Ok(())
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn one_writer_per_model() {
            let dir = tempfile::tempdir().unwrap();
            let m = dir.path().join("linha.ron");
            std::fs::write(&m, "Model()").unwrap();
            assert_eq!(caminho(&m), dir.path().join(".linha.ron.trava"));
            assert_eq!(
                travar(&m, "utsim", "ana", "pc1", 1_000).unwrap(),
                Estado::Minha
            );
            // Another app sees who edits.
            match travar(&m, "qualidade", "bruno", "pc2", 2_000).unwrap() {
                Estado::DeOutro(t) => {
                    assert_eq!((t.app.as_str(), t.pessoa.as_str()), ("utsim", "ana"))
                }
                e => panic!("{e:?}"),
            }
            // Renewal keeps it, the same owner reopening keeps it.
            assert_eq!(
                renovar(&m, "utsim", "ana", "pc1", 200_000).unwrap(),
                Estado::Minha
            );
            assert_eq!(
                travar(&m, "utsim", "ana", "pc1", 210_000).unwrap(),
                Estado::Minha
            );
            // Abandoned after five minutes without renewal: someone else takes it.
            let depois = 210_000 + TRAVA_EXPIRA_MS + 1;
            assert!(ler(&m, depois).is_none());
            assert_eq!(
                travar(&m, "qualidade", "bruno", "pc2", depois).unwrap(),
                Estado::Minha
            );
            // Releasing someone else's lock does nothing; releasing ours removes it.
            soltar(&m, "utsim", "ana", "pc1").unwrap();
            assert!(ler(&m, depois).is_some());
            soltar(&m, "qualidade", "bruno", "pc2").unwrap();
            assert!(!caminho(&m).exists());
        }
    }
}

/// The user's own settings of the family apps: one JSON file per app in the configuration
/// folder of the system (`%APPDATA%\UTSim`, else `$XDG_CONFIG_HOME/UTSim`, else
/// `~/.config/UTSim`). A missing or unreadable file reads as the default (first run).
pub mod config {
    use serde::Serialize;
    use serde::de::DeserializeOwned;
    use std::path::{Path, PathBuf};

    /// `<configuration folder>/UTSim/<nome>`, or `None` without any of the variables.
    pub fn arquivo(nome: &str) -> Option<PathBuf> {
        let base = std::env::var_os("APPDATA")
            .or_else(|| std::env::var_os("XDG_CONFIG_HOME"))
            .map(PathBuf::from)
            .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")))?;
        Some(base.join("UTSim").join(nome))
    }

    /// The settings in `arquivo`, or the default ones (no file, or one that does not read).
    pub fn ler<T: DeserializeOwned + Default>(arquivo: &Path) -> T {
        std::fs::read_to_string(arquivo)
            .ok()
            .and_then(|t| serde_json::from_str(&t).ok())
            .unwrap_or_default()
    }

    /// Writes the settings as indented JSON, creating the folder.
    pub fn gravar<T: Serialize>(arquivo: &Path, valor: &T) -> Result<(), String> {
        if let Some(pai) = arquivo.parent() {
            std::fs::create_dir_all(pai).map_err(|e| e.to_string())?;
        }
        let t = serde_json::to_string_pretty(valor).map_err(|e| e.to_string())?;
        std::fs::write(arquivo, t).map_err(|e| e.to_string())
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[derive(Debug, Default, PartialEq, serde::Serialize, serde::Deserialize)]
        struct C {
            #[serde(default)]
            autor: String,
        }

        #[test]
        fn ida_e_volta_e_padrao() {
            let d = tempfile::tempdir().unwrap();
            let a = d.path().join("sub").join("c.json");
            assert_eq!(ler::<C>(&a), C::default());
            let c = C {
                autor: "Ana".into(),
            };
            gravar(&a, &c).unwrap();
            assert_eq!(ler::<C>(&a), c);
            let t = std::fs::read_to_string(&a).unwrap();
            assert_eq!(t, "{\n  \"autor\": \"Ana\"\n}");
            std::fs::write(&a, "{ quebrado").unwrap();
            assert_eq!(ler::<C>(&a), C::default());
        }
    }
}
