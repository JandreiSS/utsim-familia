//! Signatures of the journal (stage P7 of `docs/plano_lean_seis_sigma.md`, 4.5 and
//! LSS-RF-53), over the common crate `utsim-diario`.
//!
//! - Each person has an ed25519 key in their profile ([`Signer`], file
//!   [`KEY_FILE`] next to `config.json`). Its public part goes into the project in the field
//!   [`KEY_FIELD`] of the person, written by the person themselves.
//! - Every commit of a person with a key also writes one link of the person's chain:
//!   `cadeia/<pessoa>-<chave>/<sequencia>-<hash>.json`, with the SHA-256 of each operation
//!   file written. The chain is per key, so two computers with two keys never fork.
//! - [`check`] reads every operation and every link and says which operations are signed,
//!   which changed after being signed, which are missing and which carry no signature.
//! - Approvals (tollgate decisions, validated savings) can be sealed with the key of whoever
//!   decided ([`sign_approval`], files in `assinaturas/`).
//!
//! Tampering is revealed, never prevented: whoever has the files can edit them, and none of
//! this is a legally valid digital signature (the manual says so). The first key a person
//! registers with an operation signed by that same key counts; a key written by someone else
//! in the person's name is ignored and reported.

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::fmt;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};

use serde::{Deserialize, Serialize};
use serde_json::Value;
use utsim_diario::{self as diario, Encadeado};
pub use utsim_diario::{Chave, Elo, Problema, Selo};
use utsim_lss_core::certificacao::Certification;
use utsim_lss_core::domain::fields::saving as sf;
use utsim_lss_core::op::Op;
use utsim_lss_core::perfis::{Grant, Profile};
use utsim_lss_core::tollgate::DecisionRecord;
use utsim_lss_core::{EntityRef, Id, Kind, State, digest, format, privacidade};

use crate::{Error, Result, fsio, project};

/// First line of the key file, so a key of another app is never taken by mistake.
pub const KEY_HEADER: &str = "utsim-seis-sigma chave ed25519 1";
/// The key file, next to `config.json` in the profile (never in a project).
pub const KEY_FILE: &str = "chave_assinatura.txt";
/// The chains, one folder per person and key.
pub const CHAIN: &str = "cadeia";
/// The sealed approvals.
pub const SEALS: &str = "assinaturas";
/// Field of the person with their public key (hex), written by the person.
pub const KEY_FIELD: &str = utsim_lss_core::domain::fields::person::SIGNING_KEY;
/// Characters of the public key that name a chain folder and show in the screens.
pub const SHORT: usize = 16;

fn err(e: diario::Erro) -> Error {
    Error::Invalid(e.to_string())
}

/// The signing key of the person using this computer. Cheap to clone.
#[derive(Clone)]
pub struct Signer(Arc<Chave>);

impl Signer {
    pub fn new(chave: Chave) -> Self {
        Self(Arc::new(chave))
    }

    /// The key kept with the configuration at `config_file` (created on first use).
    pub fn load_or_create(config_file: &Path) -> Result<Self> {
        let path = key_path(config_file);
        Chave::carregar_ou_criar(&path, KEY_HEADER)
            .map(Self::new)
            .map_err(err)
    }

    /// Public key, hex.
    pub fn public(&self) -> String {
        self.0.publica()
    }

    /// The first characters of the public key.
    pub fn short(&self) -> String {
        short(&self.public())
    }

    fn chave(&self) -> &Chave {
        &self.0
    }
}

impl fmt::Debug for Signer {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Signer({})", self.short())
    }
}

impl PartialEq for Signer {
    fn eq(&self, other: &Self) -> bool {
        self.public() == other.public()
    }
}

/// Where the key of the configuration at `config_file` lives.
pub fn key_path(config_file: &Path) -> PathBuf {
    config_file
        .parent()
        .unwrap_or(Path::new("."))
        .join(KEY_FILE)
}

/// The first [`SHORT`] characters of a public key.
pub fn short(public: &str) -> String {
    public.chars().take(SHORT).collect()
}

/// The public key in groups of four, for people to compare by eye.
pub fn grouped(public: &str) -> String {
    public
        .as_bytes()
        .chunks(4)
        .map(|c| String::from_utf8_lossy(c).into_owned())
        .collect::<Vec<_>>()
        .join(" ")
}

fn signers() -> &'static Mutex<HashMap<Id, Signer>> {
    static S: OnceLock<Mutex<HashMap<Id, Signer>>> = OnceLock::new();
    S.get_or_init(Default::default)
}

/// Makes `signer` the key of `person` in this process: every folder opened afterwards as
/// that person signs its commits. The app calls it once it knows who is using it.
pub fn register(person: Id, signer: Signer) {
    if let Ok(mut m) = signers().lock() {
        m.insert(person, signer);
    }
}

/// The key registered for `person` in this process.
pub fn signer_of(person: Id) -> Option<Signer> {
    signers().lock().ok().and_then(|m| m.get(&person).cloned())
}

/// One operation file covered by a link: its id and the SHA-256 of its exact bytes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OpHash {
    pub op: Id,
    pub hash: String,
}

/// A link of a person's chain: the operations of one commit.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Link {
    /// `<pessoa>/<chave curta>`.
    #[serde(rename = "autor")]
    pub author: String,
    #[serde(rename = "sequencia", default)]
    pub seq: u64,
    #[serde(rename = "anterior", default)]
    pub previous: String,
    #[serde(rename = "instante")]
    pub at_ms: i64,
    pub ops: Vec<OpHash>,
}

impl Encadeado for Link {
    fn autor(&self) -> &str {
        &self.author
    }
    fn sequencia(&self) -> u64 {
        self.seq
    }
    fn anterior(&self) -> &str {
        &self.previous
    }
    fn encadear(&mut self, sequencia: u64, anterior: String) {
        self.seq = sequencia;
        self.previous = anterior;
    }
}

/// Who signs a chain: the person and the short key.
pub fn chain_author(person: Id, short_key: &str) -> String {
    format!("{person}/{short_key}")
}

fn chain_dir(root: &Path, person: Id, short_key: &str) -> PathBuf {
    root.join(CHAIN).join(format!("{person}-{short_key}"))
}

fn read_links(dir: &Path) -> Vec<Elo> {
    fsio::json_files(dir)
        .into_iter()
        .filter_map(|p| fsio::read_text(&p).ok())
        .filter_map(|t| serde_json::from_str::<Elo>(&t).ok())
        .collect()
}

/// The last link of `signer`'s chain for `person` in the folder at `root`, if any.
pub fn last_link(root: &Path, person: Id, signer: &Signer) -> Option<Elo> {
    let short = signer.short();
    let links = read_links(&chain_dir(root, person, &short));
    if links.is_empty() {
        return None;
    }
    diario::conferir::<Link>(&signer.public(), &chain_author(person, &short), &links).ultimo
}

/// SHA-256 of the bytes of an operation file.
pub fn op_hash(bytes: &[u8]) -> String {
    diario::sha256_hex(bytes)
}

/// Signs the operations of one commit (`(id, bytes written)`) as the link after `last`,
/// writes it and returns it.
pub fn write_link(
    root: &Path,
    person: Id,
    signer: &Signer,
    last: Option<&Elo>,
    ops: &[(Id, &[u8])],
    at_ms: i64,
) -> Result<Elo> {
    let short = signer.short();
    let link = Link {
        author: chain_author(person, &short),
        seq: 0,
        previous: String::new(),
        at_ms,
        ops: ops
            .iter()
            .map(|(id, b)| OpHash {
                op: *id,
                hash: op_hash(b),
            })
            .collect(),
    };
    let elo = diario::assinar(signer.chave(), link, last).map_err(err)?;
    let seq = elo.ler::<Link>().map(|l| l.seq).unwrap_or(0);
    let path = chain_dir(root, person, &short).join(format!(
        "{seq:08}-{}.json",
        &elo.hash[..SHORT.min(elo.hash.len())]
    ));
    let text = serde_json::to_string_pretty(&elo).expect("link serializes");
    fsio::write_new(&path, text.as_bytes())?;
    Ok(elo)
}

/// A key a person registered in the project.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KeyInfo {
    pub public: String,
    pub short: String,
    /// Instant of the operation that registered it (Unix ms).
    pub since_ms: i64,
    /// The registration is in a link signed by this same key.
    pub proven: bool,
}

/// What [`check`] says about one person.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct PersonIntegrity {
    pub person: Id,
    pub keys: Vec<KeyInfo>,
    /// Operations covered by a valid link with the same hash.
    pub signed: usize,
    /// Operations from before the person's first key (nothing to check).
    pub before_key: usize,
    /// Valid links.
    pub links: usize,
}

impl PersonIntegrity {
    /// The instant of the first accepted key.
    pub fn first_key_ms(&self) -> Option<i64> {
        self.keys
            .iter()
            .filter(|k| k.proven)
            .map(|k| k.since_ms)
            .min()
    }
}

/// One thing the check found.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Finding {
    /// A problem of the chain of `person` with the key `short`.
    Chain {
        person: Id,
        short: String,
        problem: Problema,
    },
    /// The operation file changed after its author signed it.
    Altered { op: Id, author: Id },
    /// The operation changed after being signed, and its value is now the empty or
    /// placeholder value of an anonymisation (`utsim_lss_core::privacidade`).
    Redacted { op: Id, author: Id },
    /// The operation was signed by the chain of another person.
    OtherSigner { op: Id, author: Id, signer: Id },
    /// A link lists an operation whose file is not in the folder (it may still be
    /// syncing).
    Missing { op: Id, author: Id },
    /// An operation of a person who already signs, with no link (written by an older
    /// version, by another app, or by someone else in the person's name).
    Unsigned { op: Id, author: Id },
    /// A key in the person's name registered by someone else (ignored).
    ForeignKey { person: Id, by: Id, short: String },
    /// A registered key whose chain does not contain its own registration (ignored).
    KeyNotProven { person: Id, short: String },
    /// A chain folder with no key registered for it.
    ChainWithoutKey { folder: String },
}

impl Finding {
    /// Whether it points to a change of the history (and not to something that may still
    /// be syncing or that is only informative).
    pub fn severe(&self) -> bool {
        match self {
            Finding::Chain { problem, .. } => !matches!(problem, Problema::Falta { .. }),
            Finding::Altered { .. }
            | Finding::OtherSigner { .. }
            | Finding::ForeignKey { .. }
            | Finding::KeyNotProven { .. }
            | Finding::ChainWithoutKey { .. } => true,
            Finding::Redacted { .. } | Finding::Missing { .. } | Finding::Unsigned { .. } => false,
        }
    }
}

/// The result of [`check`].
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Integrity {
    pub people: Vec<PersonIntegrity>,
    pub findings: Vec<Finding>,
    /// Operation files read.
    pub ops: usize,
    /// Signed operations.
    pub signed: usize,
}

impl Integrity {
    pub fn person(&self, id: Id) -> Option<&PersonIntegrity> {
        self.people.iter().find(|p| p.person == id)
    }

    pub fn severe(&self) -> usize {
        self.findings.iter().filter(|f| f.severe()).count()
    }

    /// The accepted public keys of `person`.
    pub fn keys_of(&self, person: Id) -> Vec<&str> {
        self.person(person)
            .map(|p| {
                p.keys
                    .iter()
                    .filter(|k| k.proven)
                    .map(|k| k.public.as_str())
                    .collect()
            })
            .unwrap_or_default()
    }
}

struct OpFile {
    op: Op,
    hash: String,
}

fn redacted(op: &Op) -> bool {
    op.value == ""
        || (op.entity.kind == Kind::Person
            && op.value == privacidade::redacted_value(&op.field, op.entity.id))
}

/// Checks every operation and every link of the folder at `root`.
pub fn check(root: &Path) -> Integrity {
    let mut files: BTreeMap<Id, OpFile> = BTreeMap::new();
    for f in fsio::json_files_deep(&root.join(project::OPS), 2) {
        let Ok(text) = fsio::read_text(&f) else {
            continue;
        };
        let Ok(op) = format::read_op(&text) else {
            continue;
        };
        let hash = op_hash(text.as_bytes());
        files.insert(op.id, OpFile { op, hash });
    }
    let mut out = Integrity {
        ops: files.len(),
        ..Integrity::default()
    };
    // Candidate keys: every value of the key field of a person, in journal order.
    let mut ordered: Vec<&OpFile> = files
        .values()
        .filter(|f| f.op.entity.kind == Kind::Person && f.op.field == KEY_FIELD)
        .collect();
    ordered.sort_by_key(|f| (f.op.lamport, f.op.id));
    let mut candidates: BTreeMap<Id, Vec<(String, i64, Id)>> = BTreeMap::new();
    for f in ordered {
        let Some(public) = f.op.value.as_str() else {
            continue;
        };
        if f.op.author != f.op.entity.id {
            out.findings.push(Finding::ForeignKey {
                person: f.op.entity.id,
                by: f.op.author,
                short: short(public),
            });
            continue;
        }
        let list = candidates.entry(f.op.entity.id).or_default();
        if !list.iter().any(|(p, ..)| p == public) {
            list.push((public.to_string(), f.op.at_ms, f.op.id));
        }
    }
    // The chains on disk, by folder name.
    let mut dirs: BTreeMap<String, PathBuf> = BTreeMap::new();
    if let Ok(rd) = std::fs::read_dir(root.join(CHAIN)) {
        for e in rd.flatten() {
            if e.path().is_dir() {
                dirs.insert(e.file_name().to_string_lossy().into_owned(), e.path());
            }
        }
    }
    let mut used_dirs = BTreeSet::new();
    // op id → (signer person, hash in the link)
    let mut covered: BTreeMap<Id, (Id, String)> = BTreeMap::new();
    let mut people: BTreeMap<Id, PersonIntegrity> = BTreeMap::new();
    for (person, keys) in &candidates {
        let pi = people.entry(*person).or_insert_with(|| PersonIntegrity {
            person: *person,
            ..PersonIntegrity::default()
        });
        for (public, since_ms, reg_op) in keys {
            let s = short(public);
            let name = format!("{person}-{s}");
            let links = dirs.get(&name).map(|d| read_links(d)).unwrap_or_default();
            used_dirs.insert(name);
            let c = diario::conferir::<Link>(public, &chain_author(*person, &s), &links);
            let proven = c
                .validos
                .iter()
                .any(|l| l.ops.iter().any(|o| o.op == *reg_op));
            pi.keys.push(KeyInfo {
                public: public.clone(),
                short: s.clone(),
                since_ms: *since_ms,
                proven,
            });
            if !proven {
                out.findings.push(Finding::KeyNotProven {
                    person: *person,
                    short: s,
                });
                continue;
            }
            pi.links += c.validos.len();
            for p in c.problemas {
                out.findings.push(Finding::Chain {
                    person: *person,
                    short: s.clone(),
                    problem: p,
                });
            }
            for l in &c.validos {
                for o in &l.ops {
                    covered.entry(o.op).or_insert((*person, o.hash.clone()));
                }
            }
        }
    }
    for name in dirs.keys() {
        if !used_dirs.contains(name) {
            out.findings.push(Finding::ChainWithoutKey {
                folder: name.clone(),
            });
        }
    }
    // Each operation against the links.
    for (id, f) in &files {
        let author = f.op.author;
        match covered.get(id) {
            Some((signer, _)) if *signer != author => out.findings.push(Finding::OtherSigner {
                op: *id,
                author,
                signer: *signer,
            }),
            Some((_, hash)) if *hash == f.hash => {
                out.signed += 1;
                if let Some(p) = people.get_mut(&author) {
                    p.signed += 1;
                }
            }
            Some(_) if redacted(&f.op) => {
                out.findings.push(Finding::Redacted { op: *id, author });
            }
            Some(_) => out.findings.push(Finding::Altered { op: *id, author }),
            None => {
                let first = people.get(&author).and_then(PersonIntegrity::first_key_ms);
                match first {
                    Some(t) if f.op.at_ms >= t => {
                        out.findings.push(Finding::Unsigned { op: *id, author })
                    }
                    Some(_) => {
                        if let Some(p) = people.get_mut(&author) {
                            p.before_key += 1;
                        }
                    }
                    None => {}
                }
            }
        }
    }
    for (id, (signer, _)) in &covered {
        if !files.contains_key(id) {
            out.findings.push(Finding::Missing {
                op: *id,
                author: *signer,
            });
        }
    }
    out.people = people.into_values().collect();
    out
}

/// What an approval seal signs.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Approval {
    #[serde(rename = "projeto")]
    pub project: Id,
    /// [`APPROVAL_TOLLGATE`], [`APPROVAL_SAVING`], [`APPROVAL_CERTIFICATE`] or
    /// [`APPROVAL_ROLES`].
    #[serde(rename = "tipo")]
    pub kind: String,
    /// The decision or the saving.
    #[serde(rename = "registro")]
    pub record: Id,
    /// Who decided.
    #[serde(rename = "pessoa")]
    pub person: Id,
    #[serde(rename = "decisao")]
    pub decision: String,
    /// Hash of the content decided on.
    #[serde(rename = "conteudo")]
    pub content: String,
    /// The signed items themselves, when the content is a list (the role list).
    #[serde(rename = "itens", default, skip_serializing_if = "Vec::is_empty")]
    pub items: Vec<String>,
}

pub const APPROVAL_TOLLGATE: &str = "tollgate";
pub const APPROVAL_SAVING: &str = "ganho";
pub const APPROVAL_CERTIFICATE: &str = "certificado";
pub const APPROVAL_ROLES: &str = "papeis";

/// The approval of a tollgate decision: the decision and the frozen content hash.
pub fn tollgate_approval(project: Id, d: &DecisionRecord) -> Option<Approval> {
    let person = d.approver?;
    let decision = serde_json::to_value(d.decision?).ok()?;
    Some(Approval {
        project,
        kind: APPROVAL_TOLLGATE.into(),
        record: d.id,
        person,
        decision: decision.as_str().unwrap_or_default().to_string(),
        content: d.hash.clone(),
        items: Vec::new(),
    })
}

/// Fields of a saving that its validation covers.
pub const SAVING_FIELDS: [&str; 7] = [
    sf::DESCRIPTION,
    sf::CATEGORY,
    sf::VALUE,
    sf::RATIONALE,
    utsim_lss_core::savings::fields::MONTHLY,
    utsim_lss_core::savings::fields::START,
    utsim_lss_core::savings::fields::MONTHS,
];

/// The approval of a validated saving (`None` before it is validated).
pub fn saving_approval(state: &State, project: Id, saving: Id) -> Option<Approval> {
    let e = state.entity(saving)?;
    let person = e.id(utsim_lss_core::savings::fields::VALIDATED_BY)?;
    let content: BTreeMap<String, Value> = SAVING_FIELDS
        .iter()
        .filter_map(|f| e.get(f).map(|v| (f.to_string(), v.clone())))
        .collect();
    Some(Approval {
        project,
        kind: APPROVAL_SAVING.into(),
        record: saving,
        person,
        decision: "validado".into(),
        content: digest::hash_map(&content),
        items: Vec::new(),
    })
}

/// The approval of an issued certificate by its issuer (the MBB): the number, the person,
/// the level, the dates and the requirements met with their evidence.
pub fn certificate_approval(program: Id, c: &Certification) -> Option<Approval> {
    let person = c.issuer?;
    let number = c.number?;
    let level = serde_json::to_value(c.level).ok()?;
    let mut content: BTreeMap<String, Value> = BTreeMap::new();
    content.insert("numero".into(), Value::from(number));
    content.insert(
        "pessoa".into(),
        Value::from(c.person.map(|p| p.to_string()).unwrap_or_default()),
    );
    content.insert("nivel".into(), level.clone());
    content.insert(
        "data".into(),
        Value::from(c.date.map(|d| d.to_string()).unwrap_or_default()),
    );
    content.insert(
        "valida_ate".into(),
        Value::from(c.valid_until.map(|d| d.to_string()).unwrap_or_default()),
    );
    content.insert(
        "requisitos".into(),
        serde_json::to_value(&c.met).unwrap_or_default(),
    );
    content.insert("manual".into(), Value::from(c.manual));
    content.insert("observacao".into(), Value::from(c.note.clone()));
    Some(Approval {
        project: program,
        kind: APPROVAL_CERTIFICATE.into(),
        record: c.id,
        person,
        decision: level.as_str().unwrap_or_default().to_string(),
        content: digest::hash_map(&content),
        items: Vec::new(),
    })
}

/// One grant of the role list as signed: `<pessoa>:<perfil>:<unidade>`.
pub fn grant_item(g: &Grant) -> String {
    let profile = serde_json::to_value(g.profile)
        .ok()
        .and_then(|v| v.as_str().map(str::to_string))
        .unwrap_or_default();
    format!(
        "{}:{profile}:{}",
        g.person.map(|p| p.to_string()).unwrap_or_default(),
        g.unit.map(|u| u.to_string()).unwrap_or_default()
    )
}

/// The role list of the program as `signer` signs it: every grant, sorted.
pub fn roles_approval(program: Id, grants: &[Grant], signer: Id) -> Approval {
    let mut items: Vec<String> = grants.iter().map(grant_item).collect();
    items.sort();
    items.dedup();
    let text = items.join("\n");
    Approval {
        project: program,
        kind: APPROVAL_ROLES.into(),
        record: program,
        person: signer,
        decision: "lista".into(),
        content: diario::sha256_hex(text.as_bytes()),
        items,
    }
}

/// How the current role list stands against its signatures.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct RolesCheck {
    /// The last list signed by someone entitled to: who, when.
    pub signed: Option<(Id, i64)>,
    /// Grants of the current list that are not in the last signed list.
    pub unsigned: Vec<Grant>,
    /// Items of the last signed list that are no longer in the current one.
    pub removed: Vec<String>,
    /// Seals of the list by people who were not entitled to sign it (not the founder nor
    /// an MBB of the list signed before).
    pub not_entitled: Vec<Id>,
}

impl RolesCheck {
    /// The current list is the signed one.
    pub fn current(&self) -> bool {
        self.signed.is_some() && self.unsigned.is_empty() && self.removed.is_empty()
    }
}

/// Checks the role list of the program `program` (created by `founder`) against the seals
/// of its folder: the signatures count in order, each one by the founder or by an MBB of
/// the list signed before it, with an accepted key of the signer.
pub fn roles_check(
    seals: &Seals,
    integrity: &Integrity,
    program: Id,
    founder: Id,
    grants: &[Grant],
) -> RolesCheck {
    let mut list: Vec<(&Selo, &Approval)> = seals
        .valid
        .iter()
        .filter(|(_, a)| a.kind == APPROVAL_ROLES && a.record == program)
        .filter(|(s, a)| integrity.keys_of(a.person).contains(&s.publica.as_str()))
        .filter(|(_, a)| {
            let mut items = a.items.clone();
            items.sort();
            diario::sha256_hex(items.join("\n").as_bytes()) == a.content
        })
        .map(|(s, a)| (s, a))
        .collect();
    list.sort_by_key(|(s, _)| s.quando);
    let mbb = serde_json::to_value(Profile::Mbb)
        .ok()
        .and_then(|v| v.as_str().map(str::to_string))
        .unwrap_or_default();
    let mut out = RolesCheck::default();
    let mut last: Option<&Approval> = None;
    for (s, a) in list {
        let entitled = a.person == founder
            || last.is_some_and(|l| {
                l.items
                    .iter()
                    .any(|i| i.starts_with(&format!("{}:{mbb}:", a.person)))
            });
        if !entitled {
            if !out.not_entitled.contains(&a.person) {
                out.not_entitled.push(a.person);
            }
            continue;
        }
        out.signed = Some((a.person, s.quando));
        last = Some(a);
    }
    if let Some(l) = last {
        let current: Vec<String> = grants.iter().map(grant_item).collect();
        out.unsigned = grants
            .iter()
            .filter(|g| !l.items.contains(&grant_item(g)))
            .cloned()
            .collect();
        out.removed = l
            .items
            .iter()
            .filter(|i| !current.contains(i))
            .cloned()
            .collect();
    }
    out
}

/// Seals `approval` with `signer` and writes it in `assinaturas/`.
pub fn sign_approval(
    root: &Path,
    signer: &Signer,
    approval: &Approval,
    now_ms: i64,
) -> Result<PathBuf> {
    let selo = diario::selar(signer.chave(), approval, now_ms).map_err(err)?;
    let path = root.join(SEALS).join(format!(
        "{}-{}-{}-{now_ms}.json",
        approval.record,
        approval.person,
        signer.short()
    ));
    let text = serde_json::to_string_pretty(&selo).expect("seal serializes");
    fsio::write_new(&path, text.as_bytes())?;
    Ok(path)
}

/// How a record stands against its seals.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SealStatus {
    /// No seal.
    None,
    /// Sealed by an accepted key of the person who decided, over the current content.
    Valid { at_ms: i64 },
    /// Sealed by the person, but over another content or decision (it changed after).
    Outdated { at_ms: i64 },
    /// A seal with a bad signature or by a key that is not the person's.
    Invalid,
}

/// The seals of a folder: those whose signature matches their key, and how many seal
/// files name each record (to tell a bad seal from none).
#[derive(Debug, Clone, Default)]
pub struct Seals {
    pub valid: Vec<(Selo, Approval)>,
    pub files: BTreeMap<Id, usize>,
}

impl Seals {
    /// Reads `assinaturas/` of the folder at `root`.
    pub fn read(root: &Path) -> Seals {
        let mut out = Seals::default();
        for p in fsio::json_files(&root.join(SEALS)) {
            let record = p
                .file_name()
                .and_then(|n| n.to_str())
                .and_then(|n| n.get(..36))
                .and_then(|n| n.parse::<Id>().ok());
            if let Some(r) = record {
                *out.files.entry(r).or_default() += 1;
            }
            let Ok(t) = fsio::read_text(&p) else {
                continue;
            };
            let Ok(s) = serde_json::from_str::<Selo>(&t) else {
                continue;
            };
            if let Ok(a) = diario::conferir_selo::<Approval>(&s) {
                out.valid.push((s, a));
            }
        }
        out
    }

    /// The seal of `expected` by an accepted key of the person, over the same content.
    pub fn valid_seal(&self, integrity: &Integrity, expected: &Approval) -> Option<&Selo> {
        let keys = integrity.keys_of(expected.person);
        self.valid
            .iter()
            .find(|(s, a)| a == expected && keys.contains(&s.publica.as_str()))
            .map(|(s, _)| s)
    }

    /// How `expected` stands: sealed by an accepted key of the person who decided, over
    /// the same content or another one, by a bad seal, or not at all.
    pub fn status(&self, integrity: &Integrity, expected: &Approval) -> SealStatus {
        let keys = integrity.keys_of(expected.person);
        let mut best = SealStatus::None;
        let mut good = 0;
        for (s, a) in self
            .valid
            .iter()
            .filter(|(_, a)| a.record == expected.record && a.person == expected.person)
        {
            if !keys.contains(&s.publica.as_str()) {
                continue;
            }
            good += 1;
            if a == expected {
                return SealStatus::Valid { at_ms: s.quando };
            }
            best = SealStatus::Outdated { at_ms: s.quando };
        }
        let raw = self.files.get(&expected.record).copied().unwrap_or(0);
        if best == SealStatus::None && raw > good {
            return SealStatus::Invalid;
        }
        best
    }
}

/// [`Seals::status`] reading the folder at `root`.
pub fn seal_status(root: &Path, integrity: &Integrity, expected: &Approval) -> SealStatus {
    Seals::read(root).status(integrity, expected)
}

/// The key registration edit of `person` when the state does not have `signer`'s key yet.
pub fn key_edit(state: &State, person: Id, signer: &Signer) -> Option<utsim_lss_core::op::Edit> {
    let public = signer.public();
    let have = state
        .entity(person)
        .and_then(|e| e.get(KEY_FIELD))
        .and_then(Value::as_str)
        == Some(public.as_str());
    (!have).then(|| {
        utsim_lss_core::op::Edit::new(EntityRef::new(Kind::Person, person), KEY_FIELD, public)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn grouped_and_short() {
        assert_eq!(grouped("abcdefgh12"), "abcd efgh 12");
        assert_eq!(short(&"a".repeat(64)).len(), SHORT);
    }
}
