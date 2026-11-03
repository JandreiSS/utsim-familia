//! Installation into per-version folders, per user, without administrator rights.
//!
//! ```text
//! <raiz>/                        %LOCALAPPDATA%\UTSim\launcher  (~/.local/share/utsim/launcher)
//!   produtos/<produto>/<versão>/ the extracted program + .utsim-versao.json
//!   produtos/<produto>/ativo.json  {"version": "…"}: the active version
//!   downloads/                   encrypted packages (and .parcial files being downloaded)
//!   logs/                        launcher log, collected by the error report
//!   estado.json                  launcher state (crate::state)
//! ```
//!
//! A version is extracted into a hidden temporary folder and renamed into place only when
//! complete, and the active version is switched by replacing `ativo.json` through a rename,
//! so an interruption at any moment leaves the previous version working.

use std::fs::{self, File};
use std::io::{BufReader, BufWriter, Write};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::code::AccessCode;
use crate::crypto;
use crate::manifest::{self, Package, Release};
use crate::{Error, Result};

const VERSION_INFO: &str = ".utsim-versao.json";
const ACTIVE: &str = "ativo.json";

/// Folders of a launcher installation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Layout {
    pub root: PathBuf,
}

impl Layout {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    /// `UTSIM_LAUNCHER_DIR`, else `%LOCALAPPDATA%\UTSim\launcher` on Windows and
    /// `$XDG_DATA_HOME/utsim/launcher` (or `~/.local/share/utsim/launcher`) elsewhere.
    pub fn default_root() -> Option<PathBuf> {
        if let Some(d) = std::env::var_os("UTSIM_LAUNCHER_DIR") {
            return Some(PathBuf::from(d));
        }
        if cfg!(windows) {
            std::env::var_os("LOCALAPPDATA")
                .map(|d| PathBuf::from(d).join("UTSim").join("launcher"))
        } else {
            std::env::var_os("XDG_DATA_HOME")
                .map(PathBuf::from)
                .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".local/share")))
                .map(|d| d.join("utsim").join("launcher"))
        }
    }

    pub fn products(&self) -> PathBuf {
        self.root.join("produtos")
    }

    pub fn product(&self, id: &str) -> PathBuf {
        self.products().join(id)
    }

    pub fn version(&self, id: &str, version: &str) -> PathBuf {
        self.product(id).join(version)
    }

    pub fn downloads(&self) -> PathBuf {
        self.root.join("downloads")
    }

    pub fn logs(&self) -> PathBuf {
        self.root.join("logs")
    }

    pub fn state_file(&self) -> PathBuf {
        self.root.join("estado.json")
    }

    /// Where the encrypted package of a release is downloaded.
    pub fn package_file(&self, product: &str, release: &Release, package: &Package) -> PathBuf {
        self.downloads().join(format!(
            "{product}-{}-{}.utpk",
            release.version, package.platform
        ))
    }
}

/// What is recorded inside each installed version folder.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct InstalledVersion {
    pub product: String,
    pub version: String,
    pub channel: String,
    pub executable: String,
    #[serde(default)]
    pub args: Vec<String>,
    #[serde(default)]
    pub notes: String,
    #[serde(default)]
    pub installed_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Active {
    version: String,
}

/// Writes `data` to `path` atomically (temporary file + rename).
pub fn write_atomic(path: &Path, data: &[u8]) -> Result<()> {
    let parent = path
        .parent()
        .ok_or_else(|| Error::Install("caminho sem pasta".into()))?;
    fs::create_dir_all(parent)?;
    let mut tmp_name = path.file_name().unwrap_or_default().to_os_string();
    tmp_name.push(format!(".tmp-{}", std::process::id()));
    let tmp = parent.join(tmp_name);
    {
        let mut f = File::create(&tmp)?;
        f.write_all(data)?;
        f.sync_all()?;
    }
    fs::rename(&tmp, path).inspect_err(|_| {
        let _ = fs::remove_file(&tmp);
    })?;
    Ok(())
}

/// Checks size and SHA-256 of a downloaded package against the manifest.
pub fn verify_package(file: &Path, package: &Package) -> Result<()> {
    let size = fs::metadata(file)?.len();
    if size != package.size {
        return Err(Error::Integrity(format!(
            "tamanho {size} bytes, o manifesto anuncia {}",
            package.size
        )));
    }
    let sum = crate::sha256_file(file)?;
    if !sum.eq_ignore_ascii_case(&package.sha256) {
        return Err(Error::Integrity(
            "o SHA-256 não confere com o manifesto assinado".into(),
        ));
    }
    if package.cipher != crypto::PACKAGE_CIPHER {
        return Err(Error::Format(format!(
            "cifra '{}' desconhecida; atualize o launcher",
            package.cipher
        )));
    }
    Ok(())
}

/// The content key of `package`: from the tester's wrap when `code` is listed in it, else
/// the key published in the (signed) manifest for a package open to everyone.
pub fn content_key(
    product: &str,
    release: &Release,
    package: &Package,
    code: Option<&AccessCode>,
) -> Result<crypto::ContentKey> {
    let ctx = crypto::package_context(product, &release.version, &package.platform);
    if let Some(code) = code {
        if package.wraps.iter().any(|w| w.id == code.id()) {
            return crypto::unwrap_for(&package.wraps, code, &ctx);
        }
    }
    match package.open_key.as_deref() {
        Some(k) if !k.trim().is_empty() => crypto::ContentKey::from_base64(k.trim()),
        _ => match code {
            Some(code) => crypto::unwrap_for(&package.wraps, code, &ctx),
            None => Err(Error::NoAccess),
        },
    }
}

/// Checks, decrypts and extracts a downloaded package into its version folder; returns the
/// folder. Does not change the active version (see [`activate`]). `code` may be `None` for a
/// package published open to everyone.
pub fn install(
    layout: &Layout,
    product: &str,
    release: &Release,
    package: &Package,
    code: Option<&AccessCode>,
    encrypted: &Path,
) -> Result<PathBuf> {
    if !manifest::valid_id(product) || !manifest::valid_version(&release.version) {
        return Err(Error::Install("produto ou versão com nome inválido".into()));
    }
    verify_package(encrypted, package)?;
    let key = content_key(product, release, package, code)?;

    let product_dir = layout.product(product);
    fs::create_dir_all(&product_dir)?;
    fs::create_dir_all(layout.downloads())?;
    let tag = format!("{}-{}", std::process::id(), unique());
    let zip_path = layout
        .downloads()
        .join(format!(".{product}-{}-{tag}.zip", release.version));
    let staging = product_dir.join(format!(".instalando-{}-{tag}", release.version));

    let result = (|| -> Result<PathBuf> {
        {
            let input = BufReader::new(File::open(encrypted)?);
            let mut out = BufWriter::new(File::create(&zip_path)?);
            crypto::decrypt_stream(&key, input, &mut out)?;
            out.flush()?;
        }
        extract_zip(&zip_path, &staging)?;
        let exe = staging.join(&release.executable);
        if !manifest::relative_path_ok(&release.executable) || !exe.is_file() {
            return Err(Error::Install(format!(
                "o pacote não contém o programa {}",
                release.executable
            )));
        }
        let info = InstalledVersion {
            product: product.to_string(),
            version: release.version.clone(),
            channel: release.channel.clone(),
            executable: release.executable.clone(),
            args: release.args.clone(),
            notes: release.notes.clone(),
            installed_at: crate::state::now_rfc3339(),
        };
        let json = serde_json::to_vec_pretty(&info)
            .map_err(|e| Error::Install(format!("metadados: {e}")))?;
        fs::write(staging.join(VERSION_INFO), json)?;

        let target = layout.version(product, &release.version);
        if target.exists() {
            // Reinstalling the same version (repair).
            if active_version(layout, product).as_deref() == Some(release.version.as_str()) {
                let old = product_dir.join(format!(".antiga-{}-{tag}", release.version));
                fs::rename(&target, &old).map_err(|e| in_use(e, &release.version))?;
                fs::rename(&staging, &target)?;
                let _ = fs::remove_dir_all(&old);
                return Ok(target);
            }
            fs::remove_dir_all(&target).map_err(|e| in_use(e, &release.version))?;
        }
        fs::rename(&staging, &target)?;
        Ok(target)
    })();
    let _ = fs::remove_file(&zip_path);
    if staging.exists() {
        let _ = fs::remove_dir_all(&staging);
    }
    result
}

fn in_use(e: std::io::Error, version: &str) -> Error {
    Error::Install(format!(
        "não consegui substituir a versão {version} ({e}); feche o programa e tente de novo"
    ))
}

fn unique() -> u128 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0)
}

/// Extracts a zip into `dest`, refusing entries that would escape it.
pub fn extract_zip(zip_path: &Path, dest: &Path) -> Result<()> {
    let bad = |e: zip::result::ZipError| Error::Integrity(format!("zip: {e}"));
    let mut archive = zip::ZipArchive::new(BufReader::new(File::open(zip_path)?)).map_err(bad)?;
    fs::create_dir_all(dest)?;
    for i in 0..archive.len() {
        let mut entry = archive.by_index(i).map_err(bad)?;
        let rel = entry.enclosed_name().ok_or_else(|| {
            Error::Integrity(format!("caminho perigoso no pacote: {}", entry.name()))
        })?;
        let out = dest.join(rel);
        if entry.is_dir() {
            fs::create_dir_all(&out)?;
            continue;
        }
        if let Some(parent) = out.parent() {
            fs::create_dir_all(parent)?;
        }
        let mut f = BufWriter::new(File::create(&out)?);
        std::io::copy(&mut entry, &mut f)?;
        f.flush()?;
        #[cfg(unix)]
        if let Some(mode) = entry.unix_mode() {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&out, fs::Permissions::from_mode(mode & 0o755))?;
        }
    }
    Ok(())
}

/// Makes `version` (already installed) the one that opens.
pub fn activate(layout: &Layout, product: &str, version: &str) -> Result<()> {
    if !layout
        .version(product, version)
        .join(VERSION_INFO)
        .is_file()
    {
        return Err(Error::Install(format!(
            "a versão {version} não está instalada"
        )));
    }
    let json = serde_json::to_vec(&Active {
        version: version.to_string(),
    })
    .map_err(|e| Error::Install(e.to_string()))?;
    write_atomic(&layout.product(product).join(ACTIVE), &json)
}

/// Version that opens, if any.
pub fn active_version(layout: &Layout, product: &str) -> Option<String> {
    let text = fs::read_to_string(layout.product(product).join(ACTIVE)).ok()?;
    let a: Active = serde_json::from_str(&text).ok()?;
    layout
        .version(product, &a.version)
        .join(VERSION_INFO)
        .is_file()
        .then_some(a.version)
}

/// Metadata of an installed version.
pub fn installed(layout: &Layout, product: &str, version: &str) -> Option<InstalledVersion> {
    let text = fs::read_to_string(layout.version(product, version).join(VERSION_INFO)).ok()?;
    serde_json::from_str(&text).ok()
}

/// Installed versions of a product, newest first.
pub fn installed_versions(layout: &Layout, product: &str) -> Vec<String> {
    let Ok(dir) = fs::read_dir(layout.product(product)) else {
        return Vec::new();
    };
    let mut v: Vec<String> = dir
        .filter_map(|e| e.ok())
        .filter(|e| e.path().join(VERSION_INFO).is_file())
        .filter_map(|e| e.file_name().into_string().ok())
        .filter(|n| !n.starts_with('.'))
        .collect();
    v.sort_by(|a, b| manifest::compare_versions(b, a));
    v
}

/// Removes an installed version; the active one is refused.
pub fn remove_version(layout: &Layout, product: &str, version: &str) -> Result<()> {
    if active_version(layout, product).as_deref() == Some(version) {
        return Err(Error::Install(
            "esta é a versão ativa; ative outra antes de removê-la".into(),
        ));
    }
    let dir = layout.version(product, version);
    if !manifest::valid_version(version) || !dir.join(VERSION_INFO).is_file() {
        return Err(Error::Install(format!(
            "a versão {version} não está instalada"
        )));
    }
    fs::remove_dir_all(&dir).map_err(|e| in_use(e, version))
}

/// Removes old versions, keeping the active one and the `keep` newest others; returns what
/// was removed.
pub fn remove_old(layout: &Layout, product: &str, keep: usize) -> Result<Vec<String>> {
    let active = active_version(layout, product);
    let mut removed = Vec::new();
    let others: Vec<String> = installed_versions(layout, product)
        .into_iter()
        .filter(|v| Some(v) != active.as_ref())
        .collect();
    for v in others.into_iter().skip(keep) {
        remove_version(layout, product, &v)?;
        removed.push(v);
    }
    Ok(removed)
}

/// Leftovers of interrupted installations.
pub fn clean_leftovers(layout: &Layout) {
    let Ok(products) = fs::read_dir(layout.products()) else {
        return;
    };
    for p in products.filter_map(|e| e.ok()) {
        if let Ok(dir) = fs::read_dir(p.path()) {
            for e in dir.filter_map(|e| e.ok()) {
                let name = e.file_name().to_string_lossy().to_string();
                if name.starts_with(".instalando-") || name.starts_with(".antiga-") {
                    let _ = fs::remove_dir_all(e.path());
                }
            }
        }
    }
    if let Ok(dir) = fs::read_dir(layout.downloads()) {
        for e in dir.filter_map(|e| e.ok()) {
            if e.file_name().to_string_lossy().starts_with('.') {
                let _ = fs::remove_file(e.path());
            }
        }
    }
}

/// Full path of the active version's program, its arguments and folder.
pub fn launch_command(layout: &Layout, product: &str) -> Result<(PathBuf, Vec<String>, PathBuf)> {
    let version = active_version(layout, product)
        .ok_or_else(|| Error::Install(format!("{product} não está instalado")))?;
    let info = installed(layout, product, &version)
        .ok_or_else(|| Error::Install("metadados da versão ausentes".into()))?;
    if !manifest::relative_path_ok(&info.executable) {
        return Err(Error::Install("caminho do programa inválido".into()));
    }
    let dir = layout.version(product, &version);
    let exe = dir.join(&info.executable);
    if !exe.is_file() {
        return Err(Error::Install(format!(
            "o programa {} sumiu da pasta; reinstale a versão",
            info.executable
        )));
    }
    Ok((exe, info.args, dir))
}

/// Starts the active version of a product (does not wait for it).
pub fn launch(layout: &Layout, product: &str) -> Result<std::process::Child> {
    let (exe, args, dir) = launch_command(layout, product)?;
    std::process::Command::new(&exe)
        .args(&args)
        .current_dir(dir)
        .spawn()
        .map_err(|e| Error::Install(format!("não consegui abrir {}: {e}", exe.display())))
}
