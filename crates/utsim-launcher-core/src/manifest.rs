//! The version manifest: which products exist, in which channels and versions, and where
//! each encrypted package is.
//!
//! On disk (and on Google Drive) it is one JSON file, a signed **envelope**:
//!
//! ```json
//! { "envelope": 1, "key_id": "3f9a…", "signature": "<base64>", "body": "<JSON text>" }
//! ```
//!
//! The signature covers the exact bytes of `body`, so no canonical JSON is needed. `body`
//! holds a [`Manifest`]. Both levels carry a format number; a launcher refuses numbers above
//! the ones it knows with a message asking for an update ([`Error::FutureFormat`]). Fields
//! unknown to this version are ignored, so additive changes keep the same number.

use std::cmp::Ordering;

use ed25519_dalek::{SigningKey, VerifyingKey};
use serde::{Deserialize, Serialize};

use crate::crypto::WrappedKey;
use crate::signature;
use crate::{Error, Result};

/// Envelope format this launcher understands.
pub const ENVELOPE_FORMAT: u32 = 1;
/// Body format this launcher understands.
pub const FORMAT: u32 = 1;
/// The stable channel, always visible whatever channel a tester follows.
pub const STABLE: &str = "estavel";
/// The test channel.
pub const TESTING: &str = "testes";

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Envelope {
    envelope: u32,
    key_id: String,
    signature: String,
    body: String,
}

/// Body of the manifest.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Manifest {
    pub format: u32,
    /// Grows by one at every publication; a launcher refuses a manifest older than the last
    /// one it accepted (protection against rollback to a vulnerable version).
    pub sequence: u64,
    /// RFC 3339 time of publication (informative).
    #[serde(default)]
    pub published_at: String,
    #[serde(default)]
    pub products: Vec<Product>,
}

/// One application of the family (the editor, the 3D preview, an unrelated tool...).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Product {
    /// Stable identifier (`utsim`, `utsim-3d`, `lean-seis-sigma`); also the install folder.
    pub id: String,
    /// Name shown to testers.
    pub name: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub releases: Vec<Release>,
}

/// One version of a product in one channel.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Release {
    pub version: String,
    pub channel: String,
    #[serde(default)]
    pub published_at: String,
    /// Release notes (Markdown, shown as plain text).
    #[serde(default)]
    pub notes: String,
    /// Program to start, relative to the version folder (`bin/utsim-view.exe`).
    pub executable: String,
    #[serde(default)]
    pub args: Vec<String>,
    #[serde(default)]
    pub packages: Vec<Package>,
}

/// The encrypted package of a release for one platform.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Package {
    /// `windows-x64`, `linux-x64`.
    pub platform: String,
    /// Download address; empty until the author pastes the Drive link (`utsim-publicar link`).
    #[serde(default)]
    pub url: String,
    /// File name in the publication folder (informative, used by the author's tool).
    #[serde(default)]
    pub file: String,
    /// Size of the encrypted file, bytes.
    pub size: u64,
    /// SHA-256 of the encrypted file, lowercase hex.
    pub sha256: String,
    /// [`crate::crypto::PACKAGE_CIPHER`].
    pub cipher: String,
    /// The content key, wrapped for each tester allowed to install this package.
    #[serde(default)]
    pub wraps: Vec<WrappedKey>,
}

impl Manifest {
    pub fn new() -> Self {
        Self {
            format: FORMAT,
            ..Self::default()
        }
    }

    /// Parses and checks a manifest file against the trusted public keys.
    pub fn from_signed(text: &str, trusted: &[VerifyingKey]) -> Result<Self> {
        let raw: serde_json::Value = serde_json::from_str(text)
            .map_err(|e| Error::Format(format!("o manifesto não é JSON válido: {e}")))?;
        let found = raw
            .get("envelope")
            .and_then(|v| v.as_u64())
            .ok_or_else(|| {
                Error::Format("o arquivo não é um manifesto do UTSim (sem 'envelope')".into())
            })?;
        check_format(found, ENVELOPE_FORMAT)?;
        let env: Envelope = serde_json::from_value(raw)
            .map_err(|e| Error::Format(format!("envelope do manifesto: {e}")))?;
        signature::verify(trusted, &env.key_id, env.body.as_bytes(), &env.signature)?;
        Self::from_body(&env.body)
    }

    /// Parses the body alone (already verified).
    fn from_body(body: &str) -> Result<Self> {
        let raw: serde_json::Value = serde_json::from_str(body)
            .map_err(|e| Error::Format(format!("corpo do manifesto: {e}")))?;
        let found = raw
            .get("format")
            .and_then(|v| v.as_u64())
            .ok_or_else(|| Error::Format("manifesto sem 'format'".into()))?;
        check_format(found, FORMAT)?;
        let m: Manifest = serde_json::from_value(raw)
            .map_err(|e| Error::Format(format!("corpo do manifesto: {e}")))?;
        m.validate()?;
        Ok(m)
    }

    /// Serializes and signs; the result is the file to publish.
    pub fn to_signed(&self, key: &SigningKey) -> Result<String> {
        self.validate()?;
        let body = serde_json::to_string_pretty(self)
            .map_err(|e| Error::Format(format!("manifesto: {e}")))?;
        let env = Envelope {
            envelope: ENVELOPE_FORMAT,
            key_id: signature::key_id(&key.verifying_key()),
            signature: signature::sign(key, body.as_bytes()),
            body,
        };
        let mut text = serde_json::to_string_pretty(&env)
            .map_err(|e| Error::Format(format!("manifesto: {e}")))?;
        text.push('\n');
        Ok(text)
    }

    fn validate(&self) -> Result<()> {
        for p in &self.products {
            if !valid_id(&p.id) {
                return Err(Error::Format(format!(
                    "id de produto inválido '{}' (use letras minúsculas, dígitos e '-')",
                    p.id
                )));
            }
            for r in &p.releases {
                if !valid_version(&r.version) {
                    return Err(Error::Format(format!(
                        "versão inválida '{}' em {}",
                        r.version, p.id
                    )));
                }
                if !valid_id(&r.channel) {
                    return Err(Error::Format(format!("canal inválido '{}'", r.channel)));
                }
                if !relative_path_ok(&r.executable) {
                    return Err(Error::Format(format!(
                        "executável inválido '{}' em {} {}",
                        r.executable, p.id, r.version
                    )));
                }
            }
        }
        Ok(())
    }

    pub fn product(&self, id: &str) -> Option<&Product> {
        self.products.iter().find(|p| p.id == id)
    }

    pub fn product_mut(&mut self, id: &str) -> Option<&mut Product> {
        self.products.iter_mut().find(|p| p.id == id)
    }
}

impl Product {
    /// Releases a tester following `channel` sees, newest first. The stable channel is
    /// always included.
    pub fn visible(&self, channel: &str) -> Vec<&Release> {
        let mut v: Vec<&Release> = self
            .releases
            .iter()
            .filter(|r| r.channel == channel || r.channel == STABLE)
            .collect();
        v.sort_by(|a, b| compare_versions(&b.version, &a.version));
        v
    }

    /// Newest release visible in `channel` that has a package for `platform`.
    pub fn latest(&self, channel: &str, platform: &str) -> Option<&Release> {
        self.visible(channel)
            .into_iter()
            .find(|r| r.package(platform).is_some())
    }

    /// Channels used by any release, stable first.
    pub fn channels(&self) -> Vec<String> {
        let mut v: Vec<String> = Vec::new();
        for r in &self.releases {
            if !v.contains(&r.channel) {
                v.push(r.channel.clone());
            }
        }
        v.sort_by_key(|c| (c != STABLE, c != TESTING, c.clone()));
        v
    }

    pub fn release(&self, version: &str) -> Option<&Release> {
        self.releases.iter().find(|r| r.version == version)
    }
}

impl Release {
    pub fn package(&self, platform: &str) -> Option<&Package> {
        self.packages.iter().find(|p| p.platform == platform)
    }
}

fn check_format(found: u64, supported: u32) -> Result<()> {
    if found > u64::from(supported) {
        return Err(Error::FutureFormat {
            found: u32::try_from(found).unwrap_or(u32::MAX),
            supported,
        });
    }
    if found == 0 {
        return Err(Error::Format("formato de manifesto 0 não existe".into()));
    }
    Ok(())
}

/// Ids of products and channels: lowercase ASCII letters, digits and `-`, 1 to 40 chars.
pub fn valid_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 40
        && id
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
        && !id.starts_with('-')
}

/// Versions also name folders: digits, letters, `.`, `-`, `+`, no `..`.
pub fn valid_version(v: &str) -> bool {
    !v.is_empty()
        && v.len() <= 40
        && v.bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'-' | b'+'))
        && !v.contains("..")
        && !v.starts_with('.')
}

/// A path that stays inside the version folder.
pub fn relative_path_ok(p: &str) -> bool {
    let path = std::path::Path::new(p);
    !p.is_empty()
        && !path.is_absolute()
        && !p.starts_with('/')
        && !p.starts_with('\\')
        && !p.contains(':')
        && path
            .components()
            .all(|c| matches!(c, std::path::Component::Normal(_)))
}

/// Orders versions like `0.10.0 > 0.9.1 > 0.9.1-rc2 > 0.9.1-rc1`.
pub fn compare_versions(a: &str, b: &str) -> Ordering {
    fn split(v: &str) -> (Vec<u64>, Option<&str>) {
        let v = v.split('+').next().unwrap_or(v);
        let (main, pre) = match v.split_once('-') {
            Some((m, p)) => (m, Some(p)),
            None => (v, None),
        };
        let nums = main
            .split('.')
            .map(|p| p.parse::<u64>().unwrap_or(0))
            .collect();
        (nums, pre)
    }
    let (na, pa) = split(a);
    let (nb, pb) = split(b);
    let len = na.len().max(nb.len());
    for i in 0..len {
        let x = na.get(i).copied().unwrap_or(0);
        let y = nb.get(i).copied().unwrap_or(0);
        match x.cmp(&y) {
            Ordering::Equal => {}
            o => return o,
        }
    }
    match (pa, pb) {
        (None, None) => Ordering::Equal,
        (None, Some(_)) => Ordering::Greater,
        (Some(_), None) => Ordering::Less,
        (Some(x), Some(y)) => compare_prerelease(x, y),
    }
}

fn compare_prerelease(a: &str, b: &str) -> Ordering {
    // Split into runs of digits and non-digits so `rc10 > rc9`.
    fn runs(s: &str) -> Vec<(bool, &str)> {
        let mut out = Vec::new();
        let mut start = 0;
        let bytes = s.as_bytes();
        for i in 1..=bytes.len() {
            if i == bytes.len() || bytes[i].is_ascii_digit() != bytes[start].is_ascii_digit() {
                out.push((bytes[start].is_ascii_digit(), &s[start..i]));
                start = i;
            }
        }
        out
    }
    let (ra, rb) = (runs(a), runs(b));
    for (x, y) in ra.iter().zip(rb.iter()) {
        let o = match (x.0, y.0) {
            (true, true) => {
                x.1.parse::<u64>()
                    .unwrap_or(0)
                    .cmp(&y.1.parse::<u64>().unwrap_or(0))
            }
            _ => x.1.cmp(y.1),
        };
        if o != Ordering::Equal {
            return o;
        }
    }
    ra.len().cmp(&rb.len())
}

/// Platform string of this build.
pub fn current_platform() -> &'static str {
    if cfg!(all(windows, target_arch = "x86_64")) {
        "windows-x64"
    } else if cfg!(all(windows, target_arch = "aarch64")) {
        "windows-arm64"
    } else if cfg!(all(target_os = "linux", target_arch = "x86_64")) {
        "linux-x64"
    } else if cfg!(target_os = "macos") {
        "macos"
    } else {
        "desconhecida"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> Manifest {
        let mut m = Manifest::new();
        m.sequence = 3;
        m.products.push(Product {
            id: "utsim".into(),
            name: "UTSim".into(),
            description: "Editor".into(),
            releases: vec![
                Release {
                    version: "0.1.0".into(),
                    channel: STABLE.into(),
                    executable: "bin/utsim-view.exe".into(),
                    packages: vec![Package {
                        platform: "windows-x64".into(),
                        size: 10,
                        sha256: "00".into(),
                        cipher: crate::crypto::PACKAGE_CIPHER.into(),
                        ..Package::default()
                    }],
                    ..Release::default()
                },
                Release {
                    version: "0.2.0-rc1".into(),
                    channel: TESTING.into(),
                    executable: "bin/utsim-view.exe".into(),
                    packages: vec![Package {
                        platform: "windows-x64".into(),
                        ..Package::default()
                    }],
                    ..Release::default()
                },
            ],
        });
        m
    }

    #[test]
    fn signed_roundtrip() {
        let sk = signature::generate().unwrap();
        let text = sample().to_signed(&sk).unwrap();
        let back = Manifest::from_signed(&text, &[sk.verifying_key()]).unwrap();
        assert_eq!(back, sample());
    }

    #[test]
    fn tampered_body_is_rejected() {
        let sk = signature::generate().unwrap();
        let text = sample().to_signed(&sk).unwrap();
        let mut env: serde_json::Value = serde_json::from_str(&text).unwrap();
        let body = env["body"].as_str().unwrap().replace("0.1.0", "9.9.9");
        env["body"] = serde_json::Value::String(body);
        let r = Manifest::from_signed(&env.to_string(), &[sk.verifying_key()]);
        assert!(matches!(r, Err(Error::Signature(_))), "{r:?}");

        // Signed by a key the launcher does not trust.
        let other = signature::generate().unwrap();
        let r = Manifest::from_signed(&text, &[other.verifying_key()]);
        assert!(matches!(r, Err(Error::Signature(_))));
    }

    #[test]
    fn future_formats_are_rejected_with_a_message() {
        let sk = signature::generate().unwrap();
        // Envelope from the future: refused before trying to verify.
        let env = r#"{"envelope": 2, "key_id": "x", "signature": "x", "body": "{}", "novo": 1}"#;
        let err = Manifest::from_signed(env, &[sk.verifying_key()]).unwrap_err();
        assert!(matches!(
            err,
            Error::FutureFormat {
                found: 2,
                supported: 1
            }
        ));
        assert!(err.to_string().contains("Atualize o launcher"), "{err}");

        // Correctly signed body from the future.
        let body = r#"{"format": 7, "sequence": 1, "coisa_nova": true}"#;
        let env = serde_json::json!({
            "envelope": 1,
            "key_id": signature::key_id(&sk.verifying_key()),
            "signature": signature::sign(&sk, body.as_bytes()),
            "body": body,
        });
        let err = Manifest::from_signed(&env.to_string(), &[sk.verifying_key()]).unwrap_err();
        assert!(matches!(err, Error::FutureFormat { found: 7, .. }), "{err}");
        assert!(err.to_string().contains("formato 7"));
    }

    #[test]
    fn unknown_fields_are_ignored() {
        let sk = signature::generate().unwrap();
        let body = r#"{"format": 1, "sequence": 1, "extra": [1, 2], "products": []}"#;
        let env = serde_json::json!({
            "envelope": 1,
            "key_id": signature::key_id(&sk.verifying_key()),
            "signature": signature::sign(&sk, body.as_bytes()),
            "body": body,
        });
        let m = Manifest::from_signed(&env.to_string(), &[sk.verifying_key()]).unwrap();
        assert_eq!(m.sequence, 1);
    }

    #[test]
    fn hostile_paths_are_rejected() {
        let sk = signature::generate().unwrap();
        let mut m = sample();
        m.products[0].releases[0].executable = "../../Windows/x.exe".into();
        assert!(m.to_signed(&sk).is_err());
        m.products[0].releases[0].executable = "C:\\x.exe".into();
        assert!(m.to_signed(&sk).is_err());
        m.products[0].releases[0].executable = "bin/ok.exe".into();
        m.products[0].releases[0].version = "../0.1".into();
        assert!(m.to_signed(&sk).is_err());
        m.products[0].releases[0].version = "0.1.0".into();
        m.products[0].id = "Produto Com Espaço".into();
        assert!(m.to_signed(&sk).is_err());
    }

    #[test]
    fn channels_and_latest() {
        let m = sample();
        let p = m.product("utsim").unwrap();
        assert_eq!(p.latest(STABLE, "windows-x64").unwrap().version, "0.1.0");
        assert_eq!(
            p.latest(TESTING, "windows-x64").unwrap().version,
            "0.2.0-rc1"
        );
        assert!(p.latest(STABLE, "linux-x64").is_none());
        assert_eq!(p.channels(), vec![STABLE.to_string(), TESTING.to_string()]);
    }

    #[test]
    fn version_order() {
        use Ordering::*;
        assert_eq!(compare_versions("0.10.0", "0.9.1"), Greater);
        assert_eq!(compare_versions("0.9.1", "0.9.1-rc2"), Greater);
        assert_eq!(compare_versions("0.9.1-rc10", "0.9.1-rc9"), Greater);
        assert_eq!(compare_versions("1.0", "1.0.0"), Equal);
        assert_eq!(compare_versions("1.0.0+abc", "1.0.0"), Equal);
    }
}
