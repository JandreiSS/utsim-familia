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
use std::collections::BTreeMap;

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
/// Id of the launcher itself in the manifest: its NSIS installer, published so an installed
/// launcher updates itself. Never shown as an app card.
pub const LAUNCHER: &str = "launcher";

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
    /// Accent colour of the product in the launcher, `#RRGGBB` (optional).
    #[serde(default, rename = "cor", skip_serializing_if = "Option::is_none")]
    pub color: Option<String>,
    /// One line under the name ("Gerenciador de projetos Lean Seis Sigma").
    #[serde(default, rename = "subtitulo", skip_serializing_if = "Option::is_none")]
    pub subtitle: Option<String>,
    /// Who makes it ("UTSim").
    #[serde(default, rename = "autor", skip_serializing_if = "Option::is_none")]
    pub author: Option<String>,
    /// Square icon, a PNG in base64 (at most [`ICON_MAX_BYTES`] bytes once decoded).
    #[serde(default, rename = "icone", skip_serializing_if = "Option::is_none")]
    pub icon: Option<String>,
    /// Access per channel; a channel not listed is [`Access::Code`].
    #[serde(default, rename = "acesso", skip_serializing_if = "BTreeMap::is_empty")]
    pub access: BTreeMap<String, Access>,
}

/// Who may install the releases of a product in a channel.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Access {
    /// Only testers with an access code wrapped into the package (the default).
    #[default]
    #[serde(rename = "Codigo")]
    Code,
    /// Anyone with the manifest: the content key of each package is published in it.
    #[serde(rename = "Aberto")]
    Open,
}

/// Largest icon accepted in a manifest (decoded PNG bytes).
pub const ICON_MAX_BYTES: usize = 256 * 1024;

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
    /// The content key itself, base64, when the package was published open to everyone
    /// ([`Access::Open`]). Anyone who reads the manifest can then decrypt the package.
    #[serde(
        default,
        rename = "chave_aberta",
        skip_serializing_if = "Option::is_none"
    )]
    pub open_key: Option<String>,
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
            if let Some(c) = &p.color {
                if parse_color(c).is_none() {
                    return Err(Error::Format(format!(
                        "cor inválida '{c}' em {} (use #RRGGBB)",
                        p.id
                    )));
                }
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

    /// The products shown as apps: every one but the launcher itself.
    pub fn apps(&self) -> impl Iterator<Item = &Product> {
        self.products.iter().filter(|p| p.id != LAUNCHER)
    }

    /// The launcher release newer than `running` (the version of this launcher) in `channel`
    /// with a package for `platform`, if any.
    pub fn launcher_update(
        &self,
        channel: &str,
        platform: &str,
        running: &str,
    ) -> Option<&Release> {
        self.product(LAUNCHER)?
            .latest(channel, platform)
            .filter(|r| compare_versions(&r.version, running).is_gt())
    }
}

impl Product {
    /// Access of `channel` (default: with code).
    pub fn access(&self, channel: &str) -> Access {
        self.access.get(channel).copied().unwrap_or_default()
    }

    /// The icon's PNG bytes, if the manifest has one of an acceptable size.
    pub fn icon_png(&self) -> Option<Vec<u8>> {
        use base64::Engine as _;
        let text = self.icon.as_deref()?.trim();
        let bytes = base64::engine::general_purpose::STANDARD
            .decode(text)
            .ok()?;
        (bytes.len() <= ICON_MAX_BYTES && bytes.starts_with(b"\x89PNG")).then_some(bytes)
    }

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

impl Package {
    /// Published open to everyone (no code needed).
    pub fn is_open(&self) -> bool {
        self.open_key
            .as_deref()
            .is_some_and(|k| !k.trim().is_empty())
    }
}

/// `#RRGGBB` (or `RRGGBB`) to its three bytes.
pub fn parse_color(text: &str) -> Option<[u8; 3]> {
    let h = text.trim().trim_start_matches('#');
    if h.len() != 6 || !h.bytes().all(|b| b.is_ascii_hexdigit()) {
        return None;
    }
    let byte = |i: usize| u8::from_str_radix(&h[i..i + 2], 16).ok();
    Some([byte(0)?, byte(2)?, byte(4)?])
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
            ..Product::default()
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

    /// A body written by the first launcher (no appearance, no access fields).
    const OLD_BODY: &str = r#"{
  "format": 1,
  "sequence": 4,
  "published_at": "2026-11-02T12:00:00Z",
  "products": [
    {
      "id": "utsim",
      "name": "UTSim",
      "description": "Editor",
      "releases": [
        {
          "version": "0.1.0",
          "channel": "estavel",
          "executable": "bin/utsim-view.exe",
          "packages": [
            {"platform": "windows-x64", "size": 10, "sha256": "00",
             "cipher": "xchacha20poly1305-stream-v1", "wraps": []}
          ]
        }
      ]
    }
  ]
}"#;

    fn envelope(sk: &SigningKey, body: &str) -> String {
        serde_json::json!({
            "envelope": 1,
            "key_id": signature::key_id(&sk.verifying_key()),
            "signature": signature::sign(sk, body.as_bytes()),
            "body": body,
        })
        .to_string()
    }

    #[test]
    fn old_manifests_still_parse_and_verify() {
        let sk = signature::generate().unwrap();
        let m = Manifest::from_signed(&envelope(&sk, OLD_BODY), &[sk.verifying_key()]).unwrap();
        let p = m.product("utsim").unwrap();
        assert_eq!(p.color, None);
        assert_eq!(p.subtitle, None);
        assert_eq!(p.icon_png(), None);
        assert_eq!(p.access(STABLE), Access::Code);
        assert!(!p.releases[0].packages[0].is_open());
        // Re-signing it writes none of the new fields: an old launcher reads the same body.
        let text = m.to_signed(&sk).unwrap();
        let env: serde_json::Value = serde_json::from_str(&text).unwrap();
        let body = env["body"].as_str().unwrap();
        for field in [
            "cor",
            "subtitulo",
            "autor",
            "icone",
            "acesso",
            "chave_aberta",
        ] {
            assert!(!body.contains(field), "{field} em {body}");
        }
    }

    #[test]
    fn new_fields_roundtrip_and_old_readers_ignore_them() {
        use base64::Engine as _;
        let sk = signature::generate().unwrap();
        let mut m = sample();
        let png = b"\x89PNG\r\n\x1a\nresto".to_vec();
        {
            let p = &mut m.products[0];
            p.color = Some("#7FD1FF".into());
            p.subtitle = Some("Simulação de processos".into());
            p.author = Some("UTSim".into());
            p.icon = Some(base64::engine::general_purpose::STANDARD.encode(&png));
            p.access.insert(TESTING.into(), Access::Open);
            p.releases[1].packages[0].open_key = Some("chave".into());
        }
        let text = m.to_signed(&sk).unwrap();
        let back = Manifest::from_signed(&text, &[sk.verifying_key()]).unwrap();
        assert_eq!(back, m);
        let p = back.product("utsim").unwrap();
        assert_eq!(p.icon_png(), Some(png));
        assert_eq!(p.access(TESTING), Access::Open);
        assert_eq!(p.access(STABLE), Access::Code);
        assert!(p.releases[1].packages[0].is_open());
        assert_eq!(
            parse_color(p.color.as_deref().unwrap()),
            Some([0x7f, 0xd1, 0xff])
        );

        // The first launcher's types (no new fields) still read the new body.
        #[derive(Deserialize)]
        struct OldProduct {
            id: String,
        }
        #[derive(Deserialize)]
        struct OldBody {
            format: u32,
            products: Vec<OldProduct>,
        }
        let env: serde_json::Value = serde_json::from_str(&text).unwrap();
        let old: OldBody = serde_json::from_str(env["body"].as_str().unwrap()).unwrap();
        assert_eq!((old.format, old.products[0].id.as_str()), (1, "utsim"));
    }

    #[test]
    fn bad_colours_and_icons_are_handled() {
        let sk = signature::generate().unwrap();
        let mut m = sample();
        m.products[0].color = Some("azul".into());
        assert!(m.to_signed(&sk).is_err());
        assert_eq!(parse_color("#12345"), None);
        assert_eq!(parse_color("3d8bfd"), Some([0x3d, 0x8b, 0xfd]));
        // An icon that is not a PNG is ignored, not an error.
        m.products[0].color = None;
        m.products[0].icon = Some("bm90IGEgcG5n".into());
        let back = Manifest::from_signed(&m.to_signed(&sk).unwrap(), &[sk.verifying_key()]);
        assert_eq!(back.unwrap().products[0].icon_png(), None);
    }

    #[test]
    fn version_order() {
        use Ordering::*;
        assert_eq!(compare_versions("0.10.0", "0.9.1"), Greater);
        assert_eq!(compare_versions("0.9.1", "0.9.1-rc2"), Greater);
        assert_eq!(compare_versions("0.9.1-rc10", "0.9.1-rc9"), Greater);
        assert_eq!(compare_versions("1.0", "1.0.0"), Equal);
        assert_eq!(compare_versions("1.0.0+abc", "1.0.0"), Equal);
        assert_eq!(compare_versions("0.2.0", "0.2.0-dev"), Greater);
        assert_eq!(compare_versions("0.2.0-rc1", "0.2.0-dev"), Greater);
        assert_eq!(compare_versions("0.2.0-rc1", "0.1.9"), Greater);
    }

    fn launcher_release(version: &str, channel: &str) -> Release {
        Release {
            version: version.into(),
            channel: channel.into(),
            executable: "utsim-launcher-instalador.exe".into(),
            packages: vec![Package {
                platform: "windows-x64".into(),
                size: 1,
                sha256: "00".into(),
                cipher: crate::crypto::PACKAGE_CIPHER.into(),
                ..Package::default()
            }],
            ..Release::default()
        }
    }

    #[test]
    fn launcher_update_compares_versions() {
        let mut m = sample();
        assert!(m.launcher_update(STABLE, "windows-x64", "0.1.0").is_none());
        m.products.push(Product {
            id: LAUNCHER.into(),
            name: "UTSim Launcher".into(),
            releases: vec![
                launcher_release("0.2.0", STABLE),
                launcher_release("0.3.0-rc1", TESTING),
            ],
            ..Product::default()
        });
        let up = |ch, running| {
            m.launcher_update(ch, "windows-x64", running)
                .map(|r| r.version.clone())
        };
        assert_eq!(up(STABLE, "0.1.0").as_deref(), Some("0.2.0"));
        assert_eq!(up(STABLE, "0.2.0-dev").as_deref(), Some("0.2.0"));
        assert_eq!(up(STABLE, "0.2.0-rc3").as_deref(), Some("0.2.0"));
        assert_eq!(up(STABLE, "0.2.0"), None);
        assert_eq!(up(STABLE, "0.2.1"), None);
        // The test channel sees the release candidate too; stable never does.
        assert_eq!(up(TESTING, "0.2.0").as_deref(), Some("0.3.0-rc1"));
        assert_eq!(up(TESTING, "0.3.0-rc1"), None);
        assert_eq!(up(TESTING, "0.3.0"), None);
        // No package for this platform: nothing to offer.
        assert_eq!(m.launcher_update(STABLE, "linux-x64", "0.1.0"), None);
        // The launcher is not an app.
        assert!(m.apps().all(|p| p.id != LAUNCHER));
        assert_eq!(m.apps().count(), m.products.len() - 1);
    }
}
