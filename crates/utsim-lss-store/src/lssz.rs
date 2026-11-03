//! `.lssz`: a whole project in one file, to send to someone outside the shared folder
//! (`docs/plano_lean_seis_sigma.md`, 4.4). It is a zip of the `<nome>.lss/` folder without
//! the presence files and the temporary leftovers.

use std::fs::{self, File};
use std::io::{self, BufReader, BufWriter, Write};
use std::path::{Component, Path, PathBuf};

use crate::project::{self, MANIFEST, PRESENCE};
use crate::{Error, Result};

pub const EXTENSION: &str = "lssz";

/// Largest total size accepted on import (protection against zip bombs).
const MAX_IMPORT_BYTES: u64 = 4 * 1024 * 1024 * 1024;

fn zip_err(e: zip::result::ZipError) -> Error {
    Error::Archive(e.to_string())
}

/// Files of the folder that go into the archive, as relative paths with `/`.
fn entries(root: &Path) -> Result<Vec<(PathBuf, String)>> {
    let mut out = Vec::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        for e in fs::read_dir(&dir).map_err(|e| Error::io(&dir, e))? {
            let e = e.map_err(|e| Error::io(&dir, e))?;
            let path = e.path();
            let name = e.file_name().to_string_lossy().into_owned();
            if name.starts_with('.') || name.starts_with('~') || name.ends_with(".tmp") {
                continue;
            }
            let rel = path.strip_prefix(root).unwrap_or(&path);
            let rel_text: Vec<String> = rel
                .components()
                .map(|c| c.as_os_str().to_string_lossy().into_owned())
                .collect();
            if rel_text.first().map(String::as_str) == Some(PRESENCE) {
                continue;
            }
            let ty = e.file_type().map_err(|e| Error::io(&path, e))?;
            if ty.is_dir() {
                stack.push(path);
            } else if ty.is_file() {
                out.push((path.clone(), rel_text.join("/")));
            }
        }
    }
    out.sort_by(|a, b| a.1.cmp(&b.1));
    Ok(out)
}

/// Writes `dest` (replacing it) with the project folder `root`.
pub fn export(root: &Path, dest: &Path) -> Result<()> {
    if !project::is_project(root) {
        return Err(Error::NotAProject(root.to_path_buf()));
    }
    let top = root
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| "projeto.lss".into());
    let tmp = dest.with_extension("lssz.tmp");
    let write = || -> Result<()> {
        let file = File::create(&tmp).map_err(|e| Error::io(&tmp, e))?;
        let mut zip = zip::ZipWriter::new(BufWriter::new(file));
        let opts = zip::write::SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Deflated);
        for (path, rel) in entries(root)? {
            zip.start_file(format!("{top}/{rel}"), opts)
                .map_err(zip_err)?;
            let mut src = File::open(&path).map_err(|e| Error::io(&path, e))?;
            io::copy(&mut src, &mut zip).map_err(|e| Error::io(&path, e))?;
        }
        let mut w = zip.finish().map_err(zip_err)?;
        w.flush().map_err(|e| Error::io(&tmp, e))?;
        Ok(())
    };
    if let Err(e) = write() {
        let _ = fs::remove_file(&tmp);
        return Err(e);
    }
    fs::rename(&tmp, dest).map_err(|e| Error::io(dest, e))
}

/// Unpacks a `.lssz` into `dest_parent/<nome>.lss` and returns that folder. Refuses to
/// replace an existing folder and any entry that would land outside it.
pub fn import(file: &Path, dest_parent: &Path) -> Result<PathBuf> {
    let f = File::open(file).map_err(|e| Error::io(file, e))?;
    let mut archive = zip::ZipArchive::new(BufReader::new(f)).map_err(zip_err)?;
    // All entries must sit under one top folder ending in `.lss`.
    let mut top: Option<String> = None;
    let mut total = 0u64;
    for i in 0..archive.len() {
        let entry = archive.by_index(i).map_err(zip_err)?;
        let rel = entry
            .enclosed_name()
            .ok_or_else(|| Error::Archive(format!("caminho inseguro: {}", entry.name())))?;
        let first = match rel.components().next() {
            Some(Component::Normal(c)) => c.to_string_lossy().into_owned(),
            _ => {
                return Err(Error::Archive(format!(
                    "caminho inseguro: {}",
                    entry.name()
                )));
            }
        };
        if !first.to_ascii_lowercase().ends_with(".lss") {
            return Err(Error::Archive(format!(
                "{} fora de uma pasta .lss",
                entry.name()
            )));
        }
        match &top {
            None => top = Some(first),
            Some(t) if *t == first => {}
            Some(_) => return Err(Error::Archive("mais de um projeto no arquivo".into())),
        }
        total = total.saturating_add(entry.size());
    }
    let top = top.ok_or_else(|| Error::Archive("arquivo vazio".into()))?;
    if total > MAX_IMPORT_BYTES {
        return Err(Error::Archive("conteúdo grande demais".into()));
    }
    let target = dest_parent.join(&top);
    if target.exists() {
        return Err(Error::Exists(target));
    }
    fs::create_dir_all(&target).map_err(|e| Error::io(&target, e))?;
    let unpack = |archive: &mut zip::ZipArchive<BufReader<File>>| -> Result<()> {
        for i in 0..archive.len() {
            let mut entry = archive.by_index(i).map_err(zip_err)?;
            let rel = entry
                .enclosed_name()
                .ok_or_else(|| Error::Archive(format!("caminho inseguro: {}", entry.name())))?;
            let out = dest_parent.join(rel);
            if entry.is_dir() {
                fs::create_dir_all(&out).map_err(|e| Error::io(&out, e))?;
                continue;
            }
            if let Some(d) = out.parent() {
                fs::create_dir_all(d).map_err(|e| Error::io(d, e))?;
            }
            let mut w = File::create(&out).map_err(|e| Error::io(&out, e))?;
            io::copy(&mut entry, &mut w).map_err(|e| Error::io(&out, e))?;
        }
        if !target.join(MANIFEST).is_file() {
            return Err(Error::Archive("sem projeto.json".into()));
        }
        Ok(())
    };
    if let Err(e) = unpack(&mut archive) {
        // Only the folder this call just created.
        let _ = fs::remove_dir_all(&target);
        return Err(e);
    }
    Ok(target)
}
