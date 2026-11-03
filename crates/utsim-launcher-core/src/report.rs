//! Launcher log and the optional error report.
//!
//! There is no server: the report is a .zip the tester saves and sends to the author by any
//! means (e-mail, chat). It holds a summary of the machine and of the installed versions plus
//! the log files; it never holds the access code.

use std::fs::{self, File, OpenOptions};
use std::io::{BufWriter, Read, Write};
use std::path::{Path, PathBuf};

use crate::{Error, Result};

/// A log file that is rotated (to `.1`) past 1 MiB.
#[derive(Debug, Clone)]
pub struct Log {
    path: PathBuf,
}

const LOG_LIMIT: u64 = 1 << 20;
/// Largest file put in a report (the end of a longer log is kept).
const FILE_LIMIT: u64 = 8 << 20;

impl Log {
    pub fn new(dir: &Path, name: &str) -> Self {
        Self {
            path: dir.join(name),
        }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Appends one line with a UTC time stamp. Failures are ignored: the log never stops
    /// the launcher.
    pub fn line(&self, msg: &str) {
        let _ = self.try_line(msg);
    }

    fn try_line(&self, msg: &str) -> std::io::Result<()> {
        if let Some(parent) = self.path.parent() {
            fs::create_dir_all(parent)?;
        }
        if fs::metadata(&self.path).map(|m| m.len()).unwrap_or(0) > LOG_LIMIT {
            let mut old = self.path.clone().into_os_string();
            old.push(".1");
            let _ = fs::rename(&self.path, old);
        }
        let mut f = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.path)?;
        writeln!(
            f,
            "{} {}",
            crate::state::now_rfc3339(),
            msg.replace('\n', " | ")
        )
    }
}

/// Writes `utsim-relato-<data>.zip` into `dest_dir` with `info` (as `resumo.txt`) and every
/// file of `log_dirs` (each under a folder named after it); returns the path.
pub fn create_report(dest_dir: &Path, info: &str, log_dirs: &[PathBuf]) -> Result<PathBuf> {
    fs::create_dir_all(dest_dir)?;
    let path = dest_dir.join(format!("utsim-relato-{}.zip", crate::state::now_compact()));
    let bad = |e: zip::result::ZipError| Error::Install(format!("zip: {e}"));
    let mut zip = zip::ZipWriter::new(BufWriter::new(File::create(&path)?));
    let opts = zip::write::SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated);
    zip.start_file("resumo.txt", opts).map_err(bad)?;
    zip.write_all(info.as_bytes())?;
    for (i, dir) in log_dirs.iter().enumerate() {
        let label = dir
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| format!("pasta{i}"));
        let Ok(entries) = fs::read_dir(dir) else {
            continue;
        };
        let mut names: Vec<PathBuf> = entries
            .filter_map(|e| e.ok())
            .map(|e| e.path())
            .filter(|p| p.is_file())
            .collect();
        names.sort();
        for p in names {
            let name = format!(
                "{label}/{}",
                p.file_name().unwrap_or_default().to_string_lossy()
            );
            let mut f = File::open(&p)?;
            let len = f.metadata()?.len();
            if len > FILE_LIMIT {
                use std::io::Seek;
                f.seek(std::io::SeekFrom::Start(len - FILE_LIMIT))?;
            }
            zip.start_file(name, opts).map_err(bad)?;
            let mut buf = Vec::new();
            f.take(FILE_LIMIT).read_to_end(&mut buf)?;
            zip.write_all(&buf)?;
        }
    }
    zip.finish().map_err(bad)?.flush()?;
    Ok(path)
}

/// A short description of this machine for the report.
pub fn system_summary() -> String {
    format!(
        "launcher: {}\nsistema: {} {}\nplataforma: {}\ndata (UTC): {}\n",
        env!("CARGO_PKG_VERSION"),
        std::env::consts::OS,
        std::env::consts::ARCH,
        crate::manifest::current_platform(),
        crate::state::now_rfc3339()
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn report_contains_summary_and_logs() {
        let dir = tempfile::tempdir().unwrap();
        let logs = dir.path().join("logs");
        let log = Log::new(&logs, "launcher.log");
        log.line("instalou utsim 0.1.0");
        log.line("falhou\ncom duas linhas");
        let zip_path = create_report(
            &dir.path().join("saida"),
            &system_summary(),
            &[logs, dir.path().join("nao-existe")],
        )
        .unwrap();
        let mut z = zip::ZipArchive::new(File::open(zip_path).unwrap()).unwrap();
        let mut s = String::new();
        z.by_name("logs/launcher.log")
            .unwrap()
            .read_to_string(&mut s)
            .unwrap();
        assert!(s.contains("instalou utsim 0.1.0"));
        assert!(s.contains("falhou | com duas linhas"));
        assert!(z.by_name("resumo.txt").is_ok());
    }
}
