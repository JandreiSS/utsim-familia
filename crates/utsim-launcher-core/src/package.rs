//! Building packages (author side): zip a program folder, encrypt it, wrap its key.

use std::fs::{self, File};
use std::io::{BufReader, BufWriter, Write};
use std::path::{Path, PathBuf};

use crate::code::AccessCode;
use crate::crypto::{self, Argon2Params, ContentKey, WrappedKey};
use crate::manifest::Package;
use crate::{Error, Result};

/// Zips every file under `src` (paths relative to it, `/` separators, sorted).
pub fn zip_dir(src: &Path, dest: &Path) -> Result<u64> {
    if !src.is_dir() {
        return Err(Error::Install(format!("{} não é uma pasta", src.display())));
    }
    let mut files = Vec::new();
    collect(src, src, &mut files)?;
    files.sort();
    let out = BufWriter::new(File::create(dest)?);
    let mut zip = zip::ZipWriter::new(out);
    let bad = |e: zip::result::ZipError| Error::Install(format!("zip: {e}"));
    for rel in &files {
        let full = src.join(rel);
        let name = rel
            .components()
            .map(|c| c.as_os_str().to_string_lossy())
            .collect::<Vec<_>>()
            .join("/");
        let size = fs::metadata(&full)?.len();
        #[allow(unused_mut)]
        let mut options = zip::write::SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Deflated)
            .large_file(size >= u64::from(u32::MAX));
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            options = options.unix_permissions(fs::metadata(&full)?.permissions().mode());
        }
        zip.start_file(name, options).map_err(bad)?;
        let mut f = BufReader::new(File::open(&full)?);
        std::io::copy(&mut f, &mut zip)?;
    }
    let mut out = zip.finish().map_err(bad)?;
    out.flush()?;
    Ok(files.len() as u64)
}

fn collect(root: &Path, dir: &Path, out: &mut Vec<PathBuf>) -> Result<()> {
    for e in fs::read_dir(dir)? {
        let e = e?;
        let path = e.path();
        let kind = e.file_type()?;
        if kind.is_dir() {
            collect(root, &path, out)?;
        } else if kind.is_file() {
            out.push(
                path.strip_prefix(root)
                    .map_err(|e| Error::Install(e.to_string()))?
                    .to_path_buf(),
            );
        }
    }
    Ok(())
}

/// An encrypted package written to disk.
#[derive(Debug)]
pub struct Sealed {
    pub file: PathBuf,
    pub size: u64,
    pub sha256: String,
    pub key: ContentKey,
}

/// Encrypts `plain` into `dest` under a new content key.
pub fn seal_file(plain: &Path, dest: &Path) -> Result<Sealed> {
    let key = ContentKey::generate()?;
    seal_file_with(plain, dest, key)
}

/// Encrypts `plain` into `dest` under `key`.
pub fn seal_file_with(plain: &Path, dest: &Path, key: ContentKey) -> Result<Sealed> {
    if let Some(parent) = dest.parent() {
        fs::create_dir_all(parent)?;
    }
    {
        let input = BufReader::new(File::open(plain)?);
        let mut out = BufWriter::new(File::create(dest)?);
        crypto::encrypt_stream(&key, input, &mut out, crypto::DEFAULT_CHUNK)?;
        out.flush()?;
        out.get_ref().sync_all()?;
    }
    Ok(Sealed {
        file: dest.to_path_buf(),
        size: fs::metadata(dest)?.len(),
        sha256: crate::sha256_file(dest)?,
        key,
    })
}

/// Wraps `key` for each code.
pub fn wrap_for_all<'a>(
    key: &ContentKey,
    codes: impl IntoIterator<Item = &'a AccessCode>,
    product: &str,
    version: &str,
    platform: &str,
    params: Argon2Params,
) -> Result<Vec<WrappedKey>> {
    let ctx = crypto::package_context(product, version, platform);
    codes
        .into_iter()
        .map(|c| crypto::wrap_key(key, c, &ctx, params))
        .collect()
}

/// The manifest entry of a sealed package.
pub fn package_entry(
    platform: &str,
    url: &str,
    sealed: &Sealed,
    wraps: Vec<WrappedKey>,
) -> Package {
    Package {
        platform: platform.to_string(),
        url: url.to_string(),
        file: sealed
            .file
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_default(),
        size: sealed.size,
        sha256: sealed.sha256.clone(),
        cipher: crypto::PACKAGE_CIPHER.to_string(),
        wraps,
        open_key: None,
    }
}
