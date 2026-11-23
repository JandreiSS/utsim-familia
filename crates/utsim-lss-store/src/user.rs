//! The person using this computer and the app preferences. They live in the user's
//! configuration folder, never in a project (a project is shared; who I am is not).
//!
//! - Windows: `%APPDATA%\UTSim\seis-sigma\config.json`, cache in
//!   `%LOCALAPPDATA%\UTSim\seis-sigma\cache\`;
//! - Linux and others: `$XDG_CONFIG_HOME/utsim/seis-sigma/` (or `~/.config/...`), cache in
//!   `$XDG_CACHE_HOME/utsim/seis-sigma/` (or `~/.cache/...`).
//!
//! `UTSIM_SEIS_SIGMA_DIR` replaces both (portable use and tests): `config.json` and
//! `cache/` inside it.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use utsim_lss_core::Id;

use crate::{Error, Result, fsio};

/// Environment variable that overrides the configuration and cache folders.
pub const DIR_ENV: &str = "UTSIM_SEIS_SIGMA_DIR";

/// Most recent projects remembered.
pub const MAX_RECENT: usize = 12;

/// Who I am: written into every operation as its author.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Identity {
    #[serde(rename = "pessoa")]
    pub person: Id,
    #[serde(rename = "nome")]
    pub name: String,
    #[serde(rename = "email")]
    pub email: String,
}

impl Identity {
    /// The identity UTSim LSS keeps on this computer (the same person and id in
    /// every app of the family that writes in a Seis Sigma folder); `None` before the first
    /// run of the Seis Sigma.
    pub fn local() -> Option<Identity> {
        AppDirs::standard().and_then(|d| Self::local_in(&d))
    }

    /// The identity kept in the configuration of `dirs`.
    pub fn local_in(dirs: &AppDirs) -> Option<Identity> {
        UserConfig::load(&dirs.config_file).identity
    }

    /// Checks the fields typed on the first run.
    pub fn validate(name: &str, email: &str) -> Result<(), String> {
        let name = name.trim();
        let email = email.trim();
        if name.chars().count() < 2 {
            return Err("Informe o seu nome (pelo menos 2 letras).".into());
        }
        if name.chars().count() > 80 {
            return Err("Nome longo demais (até 80 caracteres).".into());
        }
        let at = email.find('@');
        let ok = at.is_some_and(|i| i > 0 && email[i + 1..].contains('.') && !email.ends_with('.'))
            && !email.contains(char::is_whitespace)
            && email.len() <= 120;
        if !ok {
            return Err("Informe um e-mail válido, como ana@empresa.com.br.".into());
        }
        Ok(())
    }
}

/// Visual theme preference.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum Theme {
    #[default]
    #[serde(rename = "sistema")]
    System,
    #[serde(rename = "claro")]
    Light,
    #[serde(rename = "escuro")]
    Dark,
}

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct UserConfig {
    #[serde(rename = "identidade", default)]
    pub identity: Option<Identity>,
    /// Most recent first.
    #[serde(rename = "recentes", default)]
    pub recent: Vec<PathBuf>,
    #[serde(rename = "tema", default)]
    pub theme: Theme,
    #[serde(rename = "idioma", default)]
    pub language: Language,
    /// The project folders with automatic saving turned off (the default is on).
    #[serde(
        rename = "sem_automatico",
        default,
        skip_serializing_if = "Vec::is_empty"
    )]
    pub sem_automatico: Vec<PathBuf>,
}

/// Language of the interface (stage P8).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum Language {
    #[default]
    #[serde(rename = "pt-BR")]
    PtBr,
    #[serde(rename = "en")]
    En,
    #[serde(rename = "es")]
    Es,
    #[serde(rename = "fr")]
    Fr,
    #[serde(rename = "it")]
    It,
}

impl Language {
    pub const ALL: [Language; 5] = [
        Language::PtBr,
        Language::En,
        Language::Es,
        Language::Fr,
        Language::It,
    ];

    /// The name of the language in the language itself.
    pub fn native_name(self) -> &'static str {
        match self {
            Language::PtBr => "Português (Brasil)",
            Language::En => "English",
            Language::Es => "Español",
            Language::Fr => "Français",
            Language::It => "Italiano",
        }
    }
}

impl UserConfig {
    /// Loads the configuration; a missing or unreadable file gives the default (first run).
    pub fn load(path: &Path) -> UserConfig {
        fsio::read_text(path)
            .ok()
            .and_then(|t| utsim_lss_core::format::read(&t, "configuração").ok())
            .unwrap_or_default()
    }

    pub fn save(&self, path: &Path) -> Result<()> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir).map_err(|e| Error::io(dir, e))?;
        }
        fsio::write_replace(path, utsim_lss_core::format::write(self).as_bytes())
    }

    /// Puts a project at the top of the recent list.
    pub fn touch_recent(&mut self, project: &Path) {
        self.recent.retain(|p| p != project);
        self.recent.insert(0, project.to_path_buf());
        self.recent.truncate(MAX_RECENT);
    }
}

/// Where the configuration and the cache live.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AppDirs {
    pub config_file: PathBuf,
    pub cache: PathBuf,
}

impl AppDirs {
    pub fn standard() -> Option<AppDirs> {
        if let Some(d) = std::env::var_os(DIR_ENV).filter(|d| !d.is_empty()) {
            return Some(Self::under(Path::new(&d)));
        }
        let var = |k: &str| {
            std::env::var_os(k)
                .filter(|v| !v.is_empty())
                .map(PathBuf::from)
        };
        if cfg!(windows) {
            let config = var("APPDATA")?.join("UTSim").join("seis-sigma");
            let cache = var("LOCALAPPDATA")
                .map(|d| d.join("UTSim").join("seis-sigma").join("cache"))
                .unwrap_or_else(|| config.join("cache"));
            Some(AppDirs {
                config_file: config.join("config.json"),
                cache,
            })
        } else {
            let home = var("HOME");
            let config =
                var("XDG_CONFIG_HOME").or_else(|| home.as_ref().map(|h| h.join(".config")))?;
            let cache = var("XDG_CACHE_HOME")
                .or_else(|| home.as_ref().map(|h| h.join(".cache")))
                .unwrap_or_else(|| config.clone());
            Some(AppDirs {
                config_file: config.join("utsim").join("seis-sigma").join("config.json"),
                cache: cache.join("utsim").join("seis-sigma"),
            })
        }
    }

    pub fn under(dir: &Path) -> AppDirs {
        AppDirs {
            config_file: dir.join("config.json"),
            cache: dir.join("cache"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identity_validation() {
        assert!(Identity::validate("Ana Lima", "ana@empresa.com.br").is_ok());
        assert!(Identity::validate("A", "ana@empresa.com").is_err());
        for bad in ["", "ana", "@x.com", "ana@x", "ana@x.", "a na@x.com"] {
            assert!(Identity::validate("Ana", bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn config_round_trip_and_recent() {
        let dir = tempfile::tempdir().unwrap();
        let dirs = AppDirs::under(dir.path());
        assert_eq!(UserConfig::load(&dirs.config_file), UserConfig::default());
        let mut c = UserConfig {
            identity: Some(Identity {
                person: Id::v7(1, [1; 10]),
                name: "Ana".into(),
                email: "ana@x.com".into(),
            }),
            ..Default::default()
        };
        for i in 0..20 {
            c.touch_recent(Path::new(&format!("p{i}.lss")));
        }
        c.touch_recent(Path::new("p5.lss"));
        assert_eq!(c.recent.len(), MAX_RECENT);
        assert_eq!(c.recent[0], Path::new("p5.lss"));
        assert_eq!(
            c.recent
                .iter()
                .filter(|p| *p == Path::new("p5.lss"))
                .count(),
            1
        );
        c.theme = Theme::Dark;
        c.save(&dirs.config_file).unwrap();
        assert_eq!(UserConfig::load(&dirs.config_file), c);
        std::fs::write(&dirs.config_file, "{ quebrado").unwrap();
        assert_eq!(UserConfig::load(&dirs.config_file), UserConfig::default());
    }
}
