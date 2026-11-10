//! The signed journal of stage P7 in temporary folders: a person with a key signs every
//! commit, the check finds nothing in an untouched folder, and then each kind of tampering
//! shows up (an edited file, a removed file, a removed link, an operation forged in
//! someone's name, a key registered by someone else), as do the approval seals.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use utsim_diario::{Chave, Problema};
use utsim_lss_core::domain::Approach;
use utsim_lss_core::op::{Edit, EntityRef};
use utsim_lss_core::tollgate::{Decision, DecisionRecord};
use utsim_lss_core::{Id, Kind};
use utsim_lss_store::assinatura::{self, Finding, SealStatus, Signer};
use utsim_lss_store::project::{OPS, ProjectFolder};
use utsim_lss_store::{Clock, Identity};

struct TestClock {
    now: AtomicU64,
    n: AtomicU64,
    salt: u8,
}

impl TestClock {
    fn new(salt: u8) -> Self {
        Self {
            now: AtomicU64::new(1_793_000_000_000),
            n: AtomicU64::new(0),
            salt,
        }
    }
}

impl Clock for TestClock {
    fn now_ms(&self) -> i64 {
        self.now.fetch_add(1, Ordering::Relaxed) as i64
    }
    fn random(&self) -> [u8; 10] {
        let n = self.n.fetch_add(1, Ordering::Relaxed);
        let mut r = [self.salt; 10];
        r[2..].copy_from_slice(&n.to_be_bytes());
        r
    }
}

fn who(n: u8, name: &str) -> Identity {
    Identity {
        person: Id::v7(1_700_000_000_000, [n; 10]),
        name: name.into(),
        email: format!("{}@empresa.com", name.to_lowercase()),
    }
}

fn signer(seed: u8) -> Signer {
    Signer::new(Chave::de_semente([seed; 32]))
}

fn note(pf: &ProjectFolder, text: &str) -> Edit {
    Edit::new(
        EntityRef::new(Kind::Project, pf.project_id()),
        "descricao",
        text,
    )
}

fn files_under(dir: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    if let Ok(rd) = fs::read_dir(dir) {
        for e in rd.flatten() {
            let p = e.path();
            if p.is_dir() {
                out.extend(files_under(&p));
            } else {
                out.push(p);
            }
        }
    }
    out.sort();
    out
}

/// A project of Ana (with a key) with three commits after the creation.
fn signed_project(dir: &Path, n: u8) -> (PathBuf, Identity, TestClock) {
    let ana = who(n, "Ana");
    assinatura::register(ana.person, signer(n));
    let clock = TestClock::new(n);
    let path = dir.join("Linha 2.lss");
    let mut pf =
        ProjectFolder::create(&path, "Linha 2", Approach::Dmaic, &ana, &clock, None).unwrap();
    assert!(pf.signer().is_some());
    for t in ["um", "dois", "três"] {
        let e = note(&pf, t);
        pf.commit(vec![e], &clock).unwrap();
    }
    (path, ana, clock)
}

#[test]
fn an_untouched_signed_folder_has_no_findings() {
    let dir = tempfile::tempdir().unwrap();
    let (path, ana, _) = signed_project(dir.path(), 11);
    let r = assinatura::check(&path);
    assert!(r.findings.is_empty(), "{:?}", r.findings);
    let p = r.person(ana.person).unwrap();
    assert_eq!(p.keys.len(), 1);
    assert!(p.keys[0].proven);
    // The creation is the first commit; the key is registered in the first signed commit.
    assert_eq!(p.signed + p.before_key, r.ops);
    assert!(p.signed >= 4, "{p:?}");
    assert!(p.links >= 4);
    // Reopening and writing again continues the same chain.
    let clock = TestClock::new(111);
    let mut pf = ProjectFolder::open(&path, &ana, None).unwrap();
    let e = note(&pf, "quatro");
    pf.commit(vec![e], &clock).unwrap();
    let r = assinatura::check(&path);
    assert!(r.findings.is_empty(), "{:?}", r.findings);
}

#[test]
fn tampering_shows_up() {
    let dir = tempfile::tempdir().unwrap();
    let (path, ana, _) = signed_project(dir.path(), 12);
    let ops = files_under(&path.join(OPS));
    // An edited file: same operation, another value.
    let edited = ops
        .iter()
        .find(|p| fs::read_to_string(p).unwrap().contains("\"dois\""))
        .unwrap()
        .clone();
    let t = fs::read_to_string(&edited)
        .unwrap()
        .replace("\"dois\"", "\"vinte\"");
    fs::write(&edited, t).unwrap();
    // A removed file.
    let removed = ops
        .iter()
        .find(|p| fs::read_to_string(p).unwrap().contains("\"três\""))
        .unwrap()
        .clone();
    fs::remove_file(&removed).unwrap();
    let r = assinatura::check(&path);
    assert!(
        r.findings
            .iter()
            .any(|f| matches!(f, Finding::Altered { author, .. } if *author == ana.person)),
        "{:?}",
        r.findings
    );
    assert!(
        r.findings
            .iter()
            .any(|f| matches!(f, Finding::Missing { .. }))
    );
    assert!(r.severe() >= 1);
    // A removed link in the middle of the chain.
    let chain = files_under(&path.join(assinatura::CHAIN));
    fs::remove_file(&chain[1]).unwrap();
    let r = assinatura::check(&path);
    assert!(
        r.findings.iter().any(|f| matches!(
            f,
            Finding::Chain {
                problem: Problema::Falta { .. },
                ..
            }
        )),
        "{:?}",
        r.findings
    );
}

#[test]
fn a_forged_operation_and_a_foreign_key_show_up() {
    let dir = tempfile::tempdir().unwrap();
    let (path, ana, _) = signed_project(dir.path(), 13);
    // Bia has no key; she writes an operation as if Ana did, and a key in Ana's name.
    let bia = who(14, "Bia");
    let clock = TestClock::new(14);
    let mut pf = ProjectFolder::open(&path, &bia, None).unwrap();
    assert!(pf.signer().is_none());
    let e = Edit::new(
        EntityRef::new(Kind::Person, ana.person),
        assinatura::KEY_FIELD,
        signer(99).public(),
    );
    pf.commit(vec![e], &clock).unwrap();
    let forged = files_under(&path.join(OPS))
        .into_iter()
        .find(|p| fs::read_to_string(p).unwrap().contains("\"três\""))
        .unwrap();
    let text = fs::read_to_string(&forged).unwrap();
    let new_id = Id::v7(1_793_000_000_500, [77; 10]);
    let old_name = forged.file_name().unwrap().to_string_lossy().into_owned();
    let old_id = old_name.split('-').take(5).collect::<Vec<_>>().join("-");
    let forged_text = text
        .replace(&old_id, &new_id.to_string())
        .replace("\"três\"", "\"falso\"");
    fs::write(
        forged.with_file_name(format!("{new_id}-{}.json", ana.person)),
        forged_text,
    )
    .unwrap();
    let r = assinatura::check(&path);
    assert!(
        r.findings.iter().any(|f| matches!(
            f,
            Finding::ForeignKey { person, by, .. } if *person == ana.person && *by == bia.person
        )),
        "{:?}",
        r.findings
    );
    assert!(
        r.findings.iter().any(|f| matches!(
            f,
            Finding::Unsigned { op, author } if *op == new_id && *author == ana.person
        )),
        "{:?}",
        r.findings
    );
    // The foreign key is not one of Ana's keys.
    assert_eq!(r.keys_of(ana.person), vec![signer(13).public().as_str()]);
}

#[test]
fn approval_seals() {
    let dir = tempfile::tempdir().unwrap();
    let (path, ana, clock) = signed_project(dir.path(), 15);
    let pf = ProjectFolder::open(&path, &ana, None).unwrap();
    let mut d = DecisionRecord {
        id: Id::v7(1_793_000_000_900, [5; 10]),
        tollgate: Id::v7(1_793_000_000_901, [5; 10]),
        approver: Some(ana.person),
        decision: Some(Decision::Approve),
        reservations: Vec::new(),
        comment: String::new(),
        date: None,
        round: 1,
        hash: "abc".into(),
        content: BTreeMap::new(),
    };
    let a = assinatura::tollgate_approval(pf.project_id(), &d).unwrap();
    assert_eq!(a.decision, "aprovar");
    let r = assinatura::check(&path);
    assert_eq!(assinatura::seal_status(&path, &r, &a), SealStatus::None);
    assinatura::sign_approval(&path, &signer(15), &a, clock.now_ms()).unwrap();
    assert!(matches!(
        assinatura::seal_status(&path, &r, &a),
        SealStatus::Valid { .. }
    ));
    // The frozen content changed after the seal.
    d.hash = "def".into();
    let b = assinatura::tollgate_approval(pf.project_id(), &d).unwrap();
    assert!(matches!(
        assinatura::seal_status(&path, &r, &b),
        SealStatus::Outdated { .. }
    ));
    // A seal in Ana's name with someone else's key does not count.
    assinatura::sign_approval(&path, &signer(42), &b, clock.now_ms()).unwrap();
    assert!(matches!(
        assinatura::seal_status(&path, &r, &b),
        SealStatus::Outdated { .. }
    ));
    let mut c = b.clone();
    c.record = Id::v7(1_793_000_000_950, [6; 10]);
    assinatura::sign_approval(&path, &signer(42), &c, clock.now_ms()).unwrap();
    assert_eq!(assinatura::seal_status(&path, &r, &c), SealStatus::Invalid);
}

#[test]
fn the_role_list_signed_by_the_founder() {
    use utsim_lss_core::perfis::{Grant, Profile};
    let dir = tempfile::tempdir().unwrap();
    let (path, ana, clock) = signed_project(dir.path(), 16);
    // Bia signs too, with her own key registered in the folder.
    let bia = who(17, "Bia");
    assinatura::register(bia.person, signer(17));
    let mut pf = ProjectFolder::open(&path, &bia, None).unwrap();
    let e = note(&pf, "Bia esteve aqui");
    pf.commit(vec![e], &clock).unwrap();
    let program = pf.project_id();
    let g = |n: u8, person: Id, profile: Profile| Grant {
        id: Id::v7(1_793_000_001_000 + n as u64, [n; 10]),
        person: Some(person),
        profile,
        unit: None,
    };
    let mut grants = vec![
        g(1, ana.person, Profile::Mbb),
        g(2, bia.person, Profile::Belt),
    ];
    let check = |grants: &[Grant]| {
        let r = assinatura::check(&path);
        let seals = assinatura::Seals::read(&path);
        assinatura::roles_check(&seals, &r, program, ana.person, grants)
    };
    assert_eq!(check(&grants).signed, None);
    // Bia is not the founder nor an MBB: her seal does not count.
    let a = assinatura::roles_approval(program, &grants, bia.person);
    assinatura::sign_approval(&path, &signer(17), &a, clock.now_ms()).unwrap();
    let c = check(&grants);
    assert_eq!(c.signed, None);
    assert_eq!(c.not_entitled, vec![bia.person]);
    // The founder signs: the list is current.
    let a = assinatura::roles_approval(program, &grants, ana.person);
    assinatura::sign_approval(&path, &signer(16), &a, clock.now_ms()).unwrap();
    let c = check(&grants);
    assert!(c.current(), "{c:?}");
    assert_eq!(c.signed.map(|(p, _)| p), Some(ana.person));
    // A grant added afterwards (Bia made an MBB by someone) is out of the signed list.
    grants.push(g(3, bia.person, Profile::Mbb));
    let c = check(&grants);
    assert!(!c.current());
    assert_eq!(c.unsigned.len(), 1);
    assert_eq!(c.unsigned[0].profile, Profile::Mbb);
}

#[test]
fn a_certificate_approval_covers_its_content() {
    use utsim_lss_core::certificacao::Certification;
    let program = Id::v7(1_793_000_002_000, [8; 10]);
    let issuer = Id::v7(1_793_000_002_001, [8; 10]);
    let mut c = Certification {
        id: Id::v7(1_793_000_002_002, [8; 10]),
        number: Some(7),
        person: Some(Id::v7(1_793_000_002_003, [8; 10])),
        level: Default::default(),
        date: "2026-11-10".parse().ok(),
        valid_until: "2029-11-10".parse().ok(),
        met: Vec::new(),
        issuer: Some(issuer),
        manual: false,
        note: String::new(),
        renewal_of: None,
        revoked: false,
        opinion: String::new(),
        opinion_by: None,
        opinion_on: None,
    };
    let a = assinatura::certificate_approval(program, &c).unwrap();
    assert_eq!(a.person, issuer);
    c.valid_until = "2030-11-10".parse().ok();
    let b = assinatura::certificate_approval(program, &c).unwrap();
    assert_ne!(a.content, b.content, "the validity is covered");
    c.number = None;
    assert!(assinatura::certificate_approval(program, &c).is_none());
}
