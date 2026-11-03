//! Start-menu and desktop shortcuts, per user.
//!
//! A shortcut points at the **launcher** with `--abrir <produto>`, not at a version folder,
//! so it keeps working after updates. On Windows the `.lnk` is written by PowerShell through
//! the `WScript.Shell` COM object (no extra dependency, no console window); on Linux a
//! `.desktop` file is written.

use std::path::{Path, PathBuf};

use crate::{Error, Result};

/// A shortcut to create.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Shortcut {
    /// File name without extension, also the visible name.
    pub name: String,
    pub target: PathBuf,
    pub args: Vec<String>,
    pub description: String,
    pub icon: Option<PathBuf>,
}

/// `%APPDATA%\Microsoft\Windows\Start Menu\Programs\UTSim` or
/// `~/.local/share/applications`.
pub fn start_menu_dir() -> Option<PathBuf> {
    if cfg!(windows) {
        std::env::var_os("APPDATA").map(|a| {
            PathBuf::from(a)
                .join("Microsoft")
                .join("Windows")
                .join("Start Menu")
                .join("Programs")
                .join("UTSim")
        })
    } else {
        std::env::var_os("XDG_DATA_HOME")
            .map(PathBuf::from)
            .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".local/share")))
            .map(|d| d.join("applications"))
    }
}

/// The user's desktop folder.
pub fn desktop_dir() -> Option<PathBuf> {
    let home = std::env::var_os(if cfg!(windows) { "USERPROFILE" } else { "HOME" })?;
    Some(PathBuf::from(home).join("Desktop"))
}

/// File name of a shortcut in `dir`.
pub fn shortcut_path(dir: &Path, name: &str) -> PathBuf {
    let safe: String = name
        .chars()
        .map(|c| if "<>:\"/\\|?*".contains(c) { '_' } else { c })
        .collect();
    dir.join(if cfg!(windows) {
        format!("{safe}.lnk")
    } else {
        format!("{safe}.desktop")
    })
}

fn ps_quote(s: &str) -> String {
    format!("'{}'", s.replace('\'', "''"))
}

fn quote_arg(a: &str) -> String {
    if a.is_empty() || a.contains([' ', '\t', '"']) {
        format!("\"{}\"", a.replace('"', "\\\""))
    } else {
        a.to_string()
    }
}

/// PowerShell that writes `lnk` for `sc`.
pub fn powershell_script(sc: &Shortcut, lnk: &Path) -> String {
    let args: Vec<String> = sc.args.iter().map(|a| quote_arg(a)).collect();
    let workdir = sc.target.parent().unwrap_or(Path::new("."));
    let mut s = format!(
        "$s = (New-Object -ComObject WScript.Shell).CreateShortcut({}); $s.TargetPath = {}; $s.Arguments = {}; $s.WorkingDirectory = {}; $s.Description = {};",
        ps_quote(&lnk.to_string_lossy()),
        ps_quote(&sc.target.to_string_lossy()),
        ps_quote(&args.join(" ")),
        ps_quote(&workdir.to_string_lossy()),
        ps_quote(&sc.description),
    );
    if let Some(icon) = &sc.icon {
        s.push_str(&format!(
            " $s.IconLocation = {};",
            ps_quote(&format!("{},0", icon.to_string_lossy()))
        ));
    }
    s.push_str(" $s.Save()");
    s
}

/// Contents of a Linux `.desktop` entry for `sc`.
pub fn desktop_entry(sc: &Shortcut) -> String {
    let mut exec = vec![quote_arg(&sc.target.to_string_lossy())];
    exec.extend(sc.args.iter().map(|a| quote_arg(a)));
    let mut s = format!(
        "[Desktop Entry]\nType=Application\nName={}\nComment={}\nExec={}\nTerminal=false\nCategories=Science;Education;\n",
        sc.name,
        sc.description,
        exec.join(" ")
    );
    if let Some(icon) = &sc.icon {
        s.push_str(&format!("Icon={}\n", icon.to_string_lossy()));
    }
    s
}

/// Creates (or replaces) the shortcut in `dir`; returns its path.
pub fn create(sc: &Shortcut, dir: &Path) -> Result<PathBuf> {
    std::fs::create_dir_all(dir)?;
    let path = shortcut_path(dir, &sc.name);
    if cfg!(windows) {
        run_powershell(&powershell_script(sc, &path))?;
        if !path.is_file() {
            return Err(Error::Install("o atalho não foi criado".into()));
        }
    } else {
        std::fs::write(&path, desktop_entry(sc))?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755))?;
        }
    }
    Ok(path)
}

/// Removes a shortcut if it exists.
pub fn remove(dir: &Path, name: &str) -> Result<()> {
    let path = shortcut_path(dir, name);
    if path.exists() {
        std::fs::remove_file(path)?;
    }
    Ok(())
}

fn run_powershell(script: &str) -> Result<()> {
    let mut cmd = std::process::Command::new("powershell.exe");
    cmd.args([
        "-NoProfile",
        "-NonInteractive",
        "-ExecutionPolicy",
        "Bypass",
        "-Command",
        script,
    ]);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }
    let out = cmd
        .output()
        .map_err(|e| Error::Install(format!("PowerShell indisponível: {e}")))?;
    if !out.status.success() {
        return Err(Error::Install(format!(
            "falha ao criar o atalho: {}",
            String::from_utf8_lossy(&out.stderr).trim()
        )));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> Shortcut {
        Shortcut {
            name: "UTSim 3D (prévia)".into(),
            target: PathBuf::from(
                r"C:\Users\O'Neil\AppData\Local\Programs\UTSim Launcher\utsim-launcher.exe",
            ),
            args: vec!["--abrir".into(), "utsim-3d".into()],
            description: "Abre a versão ativa".into(),
            icon: None,
        }
    }

    #[test]
    fn powershell_quotes_single_quotes() {
        let s = powershell_script(&sample(), Path::new(r"C:\x\UTSim 3D (prévia).lnk"));
        assert!(s.contains("'C:\\Users\\O''Neil\\AppData"), "{s}");
        assert!(s.contains("$s.Arguments = '--abrir utsim-3d'"));
        assert!(s.ends_with("$s.Save()"));
    }

    #[test]
    fn desktop_entry_quotes_paths_with_spaces() {
        let e = desktop_entry(&sample());
        assert!(e.contains("Exec=\"C:\\Users\\O'Neil\\AppData\\Local\\Programs\\UTSim Launcher\\utsim-launcher.exe\" --abrir utsim-3d"), "{e}");
    }

    #[test]
    fn names_are_made_safe() {
        let p = shortcut_path(Path::new("d"), "a/b:c");
        assert!(p.to_string_lossy().contains("a_b_c"));
    }
}
