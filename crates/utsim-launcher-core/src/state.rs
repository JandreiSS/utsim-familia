//! The launcher's own state on the tester's machine, and time helpers.

use std::collections::BTreeMap;
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::{Error, Result};

/// Kept in `estado.json`.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct LauncherState {
    /// The tester's access code, as typed (normalized). In `estado.json` it is protected with
    /// DPAPI on Windows ([`crate::protect`]); see the security notes in `docs/plano_launcher.md`.
    #[serde(default, with = "protected_code")]
    pub code: Option<String>,
    /// Channel followed per product (default: stable).
    #[serde(default)]
    pub channels: BTreeMap<String, String>,
    /// Highest manifest sequence accepted so far.
    #[serde(default)]
    pub last_sequence: u64,
    /// Manifest address replacing the built-in one (advanced; empty = built-in).
    #[serde(default)]
    pub manifest_url: Option<String>,
    /// Create a desktop shortcut when installing.
    #[serde(default)]
    pub desktop_shortcuts: bool,
    /// How the products are shown (cards or a table).
    #[serde(default)]
    pub view: ViewMode,
    /// Order of the products.
    #[serde(default)]
    pub sort: SortOrder,
    /// Unix time of the last manifest fetched from the network (0: never).
    #[serde(default)]
    pub checked_at: u64,
    /// Do not play the opening animation.
    #[serde(default)]
    pub skip_intro: bool,
    /// Dark, light or the system's theme.
    #[serde(default)]
    pub theme: Theme,
    /// Language of the window (`pt-BR`, `en`, `es`, `fr`, `it`); none follows the system.
    #[serde(default)]
    pub language: Option<String>,
}

/// The launcher's colours.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Theme {
    #[default]
    #[serde(rename = "escuro")]
    Dark,
    #[serde(rename = "claro")]
    Light,
    /// Follows the system's light or dark setting.
    #[serde(rename = "sistema")]
    System,
}

/// Products as cards or as a table.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum ViewMode {
    #[default]
    #[serde(rename = "grade")]
    Grid,
    #[serde(rename = "lista")]
    List,
}

/// Order of the products in the launcher.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum SortOrder {
    /// By name, A to Z.
    #[default]
    #[serde(rename = "nome")]
    Name,
    /// Newest release first.
    #[serde(rename = "recentes")]
    Recent,
    /// Products with an update first, then by name.
    #[serde(rename = "atualizacao")]
    Updates,
}

impl SortOrder {
    pub const ALL: [SortOrder; 3] = [SortOrder::Name, SortOrder::Recent, SortOrder::Updates];

    /// Label in the launcher.
    pub fn label(self) -> &'static str {
        match self {
            SortOrder::Name => "Nome (A–Z)",
            SortOrder::Recent => "Mais recentes",
            SortOrder::Updates => "Com atualização",
        }
    }
}

impl LauncherState {
    /// Reads the state; a missing or unreadable file gives the default state.
    pub fn load(path: &Path) -> Self {
        std::fs::read_to_string(path)
            .ok()
            .and_then(|t| serde_json::from_str(&t).ok())
            .unwrap_or_default()
    }

    /// Whether the file at `path` still has the access code unprotected (written by an older
    /// launcher, or DPAPI failed then); saving the state again protects it.
    pub fn code_unprotected_on_disk(path: &Path) -> bool {
        let Ok(text) = std::fs::read_to_string(path) else {
            return false;
        };
        let Ok(raw) = serde_json::from_str::<serde_json::Value>(&text) else {
            return false;
        };
        raw.get("code")
            .and_then(|c| c.as_str())
            .is_some_and(|c| !crate::protect::is_protected(c))
    }

    pub fn save(&self, path: &Path) -> Result<()> {
        let json = serde_json::to_vec_pretty(self).map_err(|e| Error::Format(e.to_string()))?;
        crate::install::write_atomic(path, &json)
    }

    /// Accepts a manifest sequence number, refusing one older than the last accepted
    /// (someone replaying an old manifest to push an old version).
    pub fn accept_sequence(&mut self, sequence: u64) -> Result<()> {
        if sequence < self.last_sequence {
            return Err(Error::Signature(format!(
                "o manifesto recebido (sequência {sequence}) é mais antigo do que o último aceito ({}); recusado",
                self.last_sequence
            )));
        }
        self.last_sequence = sequence;
        Ok(())
    }

    pub fn channel(&self, product: &str) -> &str {
        self.channels
            .get(product)
            .map_or(crate::manifest::STABLE, String::as_str)
    }
}

/// `code` on disk: protected when written; plain (old files) or protected when read. A
/// protected code that does not open here (another user or computer) reads as no code.
mod protected_code {
    use serde::{Deserialize, Deserializer, Serializer};

    pub fn serialize<S: Serializer>(code: &Option<String>, s: S) -> Result<S::Ok, S::Error> {
        match code {
            Some(c) => s.serialize_str(&crate::protect::protect(c)),
            None => s.serialize_none(),
        }
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<Option<String>, D::Error> {
        let stored = Option::<String>::deserialize(d)?;
        Ok(stored.and_then(|s| crate::protect::unprotect(&s)))
    }
}

fn civil(secs: u64) -> (i64, u32, u32, u32, u32, u32) {
    let days = (secs / 86_400) as i64;
    let rem = secs % 86_400;
    // Howard Hinnant's days-to-civil.
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    let y = yoe + era * 400 + i64::from(m <= 2);
    (
        y,
        m,
        d,
        (rem / 3600) as u32,
        (rem % 3600 / 60) as u32,
        (rem % 60) as u32,
    )
}

/// Seconds since the Unix epoch, now.
pub fn now_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// Current UTC time, `2026-11-02T14:05:09Z`.
pub fn now_rfc3339() -> String {
    rfc3339(now_secs())
}

pub fn rfc3339(secs: u64) -> String {
    let (y, mo, d, h, mi, s) = civil(secs);
    format!("{y:04}-{mo:02}-{d:02}T{h:02}:{mi:02}:{s:02}Z")
}

/// Current UTC time for file names, `20261102-140509`.
pub fn now_compact() -> String {
    let (y, mo, d, h, mi, s) = civil(now_secs());
    format!("{y:04}{mo:02}{d:02}-{h:02}{mi:02}{s:02}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dates() {
        assert_eq!(rfc3339(0), "1970-01-01T00:00:00Z");
        assert_eq!(rfc3339(1_793_620_800), "2026-11-02T12:00:00Z");
        assert_eq!(rfc3339(951_782_400), "2000-02-29T00:00:00Z");
    }

    #[test]
    fn rollback_is_refused_and_state_roundtrips() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("estado.json");
        let mut s = LauncherState::load(&path);
        assert_eq!(s, LauncherState::default());
        s.accept_sequence(5).unwrap();
        s.accept_sequence(5).unwrap();
        assert!(s.accept_sequence(4).is_err());
        s.channels.insert("utsim".into(), "testes".into());
        s.save(&path).unwrap();
        let back = LauncherState::load(&path);
        assert_eq!(back.last_sequence, 5);
        assert_eq!(back.channel("utsim"), "testes");
        assert_eq!(back.channel("outro"), "estavel");
    }

    #[test]
    fn view_and_sort_are_remembered_and_old_files_still_load() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("estado.json");
        // A file written by the first launcher.
        std::fs::write(&path, r#"{"code": null, "last_sequence": 3}"#).unwrap();
        let mut s = LauncherState::load(&path);
        assert_eq!(
            (s.view, s.sort, s.last_sequence),
            (ViewMode::Grid, SortOrder::Name, 3)
        );
        s.view = ViewMode::List;
        s.sort = SortOrder::Updates;
        s.save(&path).unwrap();
        let back = LauncherState::load(&path);
        assert_eq!((back.view, back.sort), (ViewMode::List, SortOrder::Updates));
        let text = std::fs::read_to_string(&path).unwrap();
        assert!(
            text.contains("\"lista\"") && text.contains("\"atualizacao\""),
            "{text}"
        );
    }

    #[test]
    fn access_code_is_protected_on_disk_and_old_files_migrate() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("estado.json");
        let code = "AB12-CDEF-GHJK-MNPQ-RSTV";
        // A file of an older launcher: the code in plain text still loads.
        std::fs::write(
            &path,
            format!(r#"{{"code": "{code}", "last_sequence": 2}}"#),
        )
        .unwrap();
        assert!(LauncherState::code_unprotected_on_disk(&path));
        let s = LauncherState::load(&path);
        assert_eq!(s.code.as_deref(), Some(code));
        // Saved again: protected on Windows, the same code read back.
        s.save(&path).unwrap();
        let text = std::fs::read_to_string(&path).unwrap();
        assert_eq!(LauncherState::load(&path).code.as_deref(), Some(code));
        if cfg!(windows) {
            assert!(!text.contains(code), "{text}");
            assert!(text.contains(crate::protect::PREFIX), "{text}");
            assert!(!LauncherState::code_unprotected_on_disk(&path));
        }
        // A protected code that does not open here reads as no code; the rest still loads.
        std::fs::write(
            &path,
            r#"{"code": "dpapi:AAAAAAAAAAAAAAAAAAAAAA==", "last_sequence": 7}"#,
        )
        .unwrap();
        let s = LauncherState::load(&path);
        assert_eq!((s.code, s.last_sequence), (None, 7));
        // No code at all.
        std::fs::write(&path, r#"{"last_sequence": 1}"#).unwrap();
        assert!(!LauncherState::code_unprotected_on_disk(&path));
    }
}
