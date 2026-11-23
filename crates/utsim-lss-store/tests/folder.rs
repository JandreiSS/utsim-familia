//! The project folder in temporary directories: two "computers" writing into the same
//! (synced) folder converge; files left by sync clients are tolerated; snapshots, cache,
//! presence, attachments and `.lssz` round trips.

use std::fs;
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};

use serde_json::Value;
use utsim_lss_core::domain::{Approach, Column, fields};
use utsim_lss_core::format;
use utsim_lss_core::{EntityRef, Id, Kind, State, kanban};
use utsim_lss_store::project::{self, ProjectFolder};
use utsim_lss_store::{Clock, Error, Identity, SystemClock, lssz};

/// Deterministic clock: every call advances one millisecond; random bytes from a counter.
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
        email: format!("{}@exemplo.com", name.to_lowercase()),
    }
}

fn title(pf: &ProjectFolder, card: Id) -> String {
    pf.state().entity(card).unwrap().text(fields::action::TITLE)
}

#[test]
fn create_commit_reopen() {
    let dir = tempfile::tempdir().unwrap();
    let clock = TestClock::new(1);
    let ana = who(1, "Ana");
    let mut pf = ProjectFolder::create(
        &dir.path().join("Linha 2"),
        "Reduzir refugo",
        Approach::Dmaic,
        &ana,
        &clock,
        None,
    )
    .unwrap();
    let root = pf.root().to_path_buf();
    assert!(root.ends_with("Linha 2.lss"));
    for sub in [
        "ops",
        "instantaneos",
        "dados",
        "anexos",
        "modelos",
        "presenca",
    ] {
        assert!(root.join(sub).is_dir(), "{sub}");
    }
    let manifest = fs::read_to_string(root.join("projeto.json")).unwrap();
    assert!(manifest.starts_with("{\n  \"formato\": 1,"));
    let p = pf.project_id();
    let card = clock.new_id();
    pf.commit(
        kanban::new_card(pf.state(), p, card, "Medir o refugo", Column::Todo),
        &clock,
    )
    .unwrap();
    // One file per operation, under ops/AAAA/MM.
    let n_ops = pf.journal().len();
    let files: Vec<_> = utsim_lss_store::fsio::json_files_deep(&root.join("ops"), 2);
    assert_eq!(files.len(), n_ops);
    let name = files[0].file_name().unwrap().to_string_lossy().into_owned();
    assert!(name.ends_with(&format!("-{}.json", ana.person)));
    assert!(
        files[0]
            .parent()
            .unwrap()
            .parent()
            .unwrap()
            .ends_with("2026")
    );

    let again = ProjectFolder::open(&root, &ana, None).unwrap();
    assert_eq!(again.state(), pf.state());
    assert_eq!(again.title(), "Reduzir refugo");
    assert_eq!(title(&again, card), "Medir o refugo");
    assert!(matches!(
        ProjectFolder::create(&root, "x", Approach::A3, &ana, &clock, None),
        Err(Error::Exists(_))
    ));
    assert!(matches!(
        ProjectFolder::open(dir.path(), &ana, None),
        Err(Error::NotAProject(_))
    ));
}

#[test]
fn two_computers_converge_and_see_the_conflict() {
    let dir = tempfile::tempdir().unwrap();
    let (ca, cb) = (TestClock::new(1), TestClock::new(2));
    let (ana, bia) = (who(1, "Ana"), who(2, "Bia"));
    let mut a = ProjectFolder::create(&dir.path().join("p"), "P", Approach::Dmaic, &ana, &ca, None)
        .unwrap();
    let mut b = ProjectFolder::open(a.root(), &bia, None).unwrap();
    b.register_me(&cb).unwrap();
    let p = a.project_id();
    let card = ca.new_id();
    a.commit(
        kanban::new_card(a.state(), p, card, "Original", Column::Todo),
        &ca,
    )
    .unwrap();
    assert_eq!(b.refresh(&cb).unwrap().new_ops, 5);
    a.refresh(&ca).unwrap();
    // Both edit the title without seeing each other's change.
    a.commit(
        vec![kanban::set_text(
            card,
            fields::action::TITLE,
            "Versão da Ana",
        )],
        &ca,
    )
    .unwrap();
    b.commit(
        vec![kanban::set_text(
            card,
            fields::action::TITLE,
            "Versão da Bia",
        )],
        &cb,
    )
    .unwrap();
    // Bia also moves the card: a different field, no conflict.
    let mv = kanban::move_card(b.state(), p, card, Column::Doing, 0);
    b.commit(mv, &cb).unwrap();
    a.refresh(&ca).unwrap();
    b.refresh(&cb).unwrap();
    assert_eq!(a.state(), b.state(), "same state on both computers");
    assert_eq!(a.state().conflicts.len(), 1);
    let c = a.state().conflicts[0].clone();
    assert_eq!(c.entity.id, card);
    let kept = title(&a, card);
    assert_eq!(Value::from(kept.clone()), c.kept.value);
    // Ana restores the other version; Bia sees it and the conflict goes away for both.
    a.commit(c.restore(), &ca).unwrap();
    b.refresh(&cb).unwrap();
    assert!(b.state().conflicts.is_empty());
    assert_eq!(Value::from(title(&b, card)), c.replaced.value);
    assert_eq!(
        b.state()
            .entity(card)
            .unwrap()
            .typed::<Column>(fields::action::COLUMN),
        Some(Column::Doing)
    );
    // A third opening from scratch gives the same state.
    let fresh = ProjectFolder::open(a.root(), &who(3, "Caio"), None).unwrap();
    assert_eq!(fresh.state(), b.state());
}

#[test]
fn concurrent_writers_in_threads_converge() {
    let dir = tempfile::tempdir().unwrap();
    let c0 = TestClock::new(9);
    let ana = who(1, "Ana");
    let root = ProjectFolder::create(
        &dir.path().join("p"),
        "P",
        Approach::Kaizen,
        &ana,
        &c0,
        None,
    )
    .unwrap()
    .root()
    .to_path_buf();
    let handles: Vec<_> = (0..2u8)
        .map(|i| {
            let root = root.clone();
            std::thread::spawn(move || {
                let me = who(10 + i, &format!("P{i}"));
                let clock = TestClock::new(20 + i);
                let mut pf = ProjectFolder::open(&root, &me, None).unwrap();
                let p = pf.project_id();
                for k in 0..60 {
                    let card = clock.new_id();
                    let edits =
                        kanban::new_card(pf.state(), p, card, &format!("{i}-{k}"), Column::Todo);
                    pf.commit(edits, &clock).unwrap();
                    // Everyone also edits a shared field of the project.
                    pf.commit(
                        vec![utsim_lss_core::Edit::new(
                            EntityRef::new(Kind::Project, p),
                            fields::project::GOAL,
                            format!("meta {i}-{k}"),
                        )],
                        &clock,
                    )
                    .unwrap();
                    if k % 7 == 0 {
                        pf.refresh(&clock).unwrap();
                    }
                }
                pf
            })
        })
        .collect();
    let mut folders: Vec<ProjectFolder> = handles.into_iter().map(|h| h.join().unwrap()).collect();
    for (i, pf) in folders.iter_mut().enumerate() {
        pf.refresh(&TestClock::new(40 + i as u8)).unwrap();
    }
    assert_eq!(folders[0].state(), folders[1].state());
    let fresh = ProjectFolder::open(&root, &ana, None).unwrap();
    assert_eq!(fresh.state(), folders[0].state());
    let board = kanban::board(fresh.state(), fresh.project_id());
    assert_eq!(board[&Column::Todo].len(), 120);
}

#[test]
fn tolerates_sync_leftovers() {
    let dir = tempfile::tempdir().unwrap();
    let clock = TestClock::new(1);
    let ana = who(1, "Ana");
    let mut a = ProjectFolder::create(&dir.path().join("p"), "P", Approach::A3, &ana, &clock, None)
        .unwrap();
    let root = a.root().to_path_buf();
    let before = a.state().clone();
    let month = utsim_lss_store::fsio::json_files_deep(&root.join("ops"), 2)[0]
        .parent()
        .unwrap()
        .to_path_buf();

    // A conflict copy of an existing operation file: same operation, ignored.
    let some = utsim_lss_store::fsio::json_files(&month)[0].clone();
    let copy = some.with_file_name(format!(
        "{} (1).json",
        some.file_stem().unwrap().to_string_lossy()
    ));
    fs::copy(&some, &copy).unwrap();
    // Temporary files of a sync client and of ours.
    fs::write(month.join(".abc.json.1234.tmp"), "{").unwrap();
    fs::write(month.join("~$rascunho.json"), "x").unwrap();
    fs::write(month.join("desktop.ini"), "x").unwrap();

    // A new operation from Bia that is still arriving (half written).
    let bia = who(2, "Bia");
    let b = ProjectFolder::open(&root, &bia, None).unwrap();
    let ops = b.state().prepare(
        bia.person,
        0,
        [kanban::set_text(
            clock.new_id(),
            fields::action::TITLE,
            "chegando",
        )],
        || clock.new_id(),
    );
    let text = format::write(&ops[0]);
    let path = b.op_path(&ops[0]);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(&path, &text[..text.len() / 2]).unwrap();
    // And one written by a newer version of the app.
    let newer = b.state().prepare(
        bia.person,
        0,
        [kanban::set_text(
            clock.new_id(),
            fields::action::TITLE,
            "futuro",
        )],
        || clock.new_id(),
    );
    let newer_path = b.op_path(&newer[0]);
    fs::write(
        &newer_path,
        format::write(&newer[0]).replace("\"formato\": 1", "\"formato\": 7"),
    )
    .unwrap();

    let r = a.refresh(&clock).unwrap();
    assert_eq!(r.new_ops, 0);
    assert_eq!(r.pending, 2);
    assert_eq!(a.state(), &before);
    let reasons: Vec<&String> = a.pending().values().collect();
    assert!(
        reasons.iter().any(|m| m.contains("formato 7")),
        "{reasons:?}"
    );

    // The file finishes arriving: picked up on the next refresh.
    fs::write(&path, &text).unwrap();
    let r = a.refresh(&clock).unwrap();
    assert_eq!((r.new_ops, r.pending), (1, 1));
    let reopened = ProjectFolder::open(&root, &ana, None).unwrap();
    assert_eq!(reopened.state(), a.state());
    assert_eq!(reopened.pending().len(), 1);
}

#[test]
fn snapshots_and_late_operations() {
    let dir = tempfile::tempdir().unwrap();
    let clock = TestClock::new(1);
    let ana = who(1, "Ana");
    let mut a = ProjectFolder::create(
        &dir.path().join("p"),
        "P",
        Approach::Dmaic,
        &ana,
        &clock,
        None,
    )
    .unwrap();
    let p = a.project_id();
    // Bia works offline from the start (sees only the first operations).
    let bia = who(2, "Bia");
    let b = ProjectFolder::open(a.root(), &bia, None).unwrap();
    let early = b.state().clone();
    for k in 0..30 {
        let card = clock.new_id();
        a.commit(
            kanban::new_card(a.state(), p, card, &format!("c{k}"), Column::Todo),
            &clock,
        )
        .unwrap();
    }
    let snap_path = a.write_snapshot(&clock).unwrap();
    assert!(
        snap_path
            .file_name()
            .unwrap()
            .to_string_lossy()
            .starts_with(&format!("{:06}-", a.journal().len()))
    );
    let opened = ProjectFolder::open(a.root(), &ana, None).unwrap();
    assert_eq!(opened.load_info().from_snapshot, a.journal().len());
    assert_eq!(opened.load_info().files_read, 0);
    assert_eq!(opened.state(), a.state());

    // Bia's offline edit has a small Lamport clock: older than the snapshot's end.
    let late = early.prepare(
        bia.person,
        clock.now_ms(),
        [utsim_lss_core::Edit::new(
            EntityRef::new(Kind::Project, p),
            fields::project::PROBLEM,
            "refugo de 8 %",
        )],
        || clock.new_id(),
    );
    utsim_lss_store::fsio::write_new(&a.op_path(&late[0]), format::write(&late[0]).as_bytes())
        .unwrap();
    let r = a.refresh(&clock).unwrap();
    assert!(r.full_replay);
    let mut all: Vec<_> = a.journal().tail().cloned().collect();
    assert_eq!(all.len(), a.journal().len());
    all.sort_by_key(|o| o.key());
    assert_eq!(a.state(), &State::replay(&all));
    let reopened = ProjectFolder::open(a.root(), &ana, None).unwrap();
    assert_eq!(reopened.state(), a.state());
    assert_eq!(
        reopened
            .state()
            .entity(p)
            .unwrap()
            .text(fields::project::PROBLEM),
        "refugo de 8 %"
    );
}

#[test]
fn local_cache_is_optional() {
    let dir = tempfile::tempdir().unwrap();
    let cache = dir.path().join("cache");
    let clock = TestClock::new(1);
    let ana = who(1, "Ana");
    let mut a = ProjectFolder::create(
        &dir.path().join("p"),
        "P",
        Approach::Pdca,
        &ana,
        &clock,
        Some(&cache),
    )
    .unwrap();
    let p = a.project_id();
    a.commit(
        kanban::new_card(a.state(), p, clock.new_id(), "x", Column::Todo),
        &clock,
    )
    .unwrap();
    a.save_cache().unwrap();
    let cached = ProjectFolder::open(a.root(), &ana, Some(&cache)).unwrap();
    assert!(cached.load_info().from_cache);
    assert_eq!(cached.load_info().files_read, 0);
    assert_eq!(cached.state(), a.state());
    // New operation after the cache: read on top of it.
    a.commit(
        kanban::new_card(a.state(), p, clock.new_id(), "y", Column::Todo),
        &clock,
    )
    .unwrap();
    let cached = ProjectFolder::open(a.root(), &ana, Some(&cache)).unwrap();
    assert_eq!(cached.load_info().files_read, 5);
    assert_eq!(cached.state(), a.state());
    // Corrupt or deleted cache: ignored.
    let file = cache.join(p.to_string()).join("estado.json");
    fs::write(&file, "{ nada").unwrap();
    assert_eq!(
        ProjectFolder::open(a.root(), &ana, Some(&cache))
            .unwrap()
            .state(),
        a.state()
    );
    fs::remove_dir_all(&cache).unwrap();
    let plain = ProjectFolder::open(a.root(), &ana, Some(&cache)).unwrap();
    assert!(!plain.load_info().from_cache);
    assert_eq!(plain.state(), a.state());
}

#[test]
fn presence_heartbeats() {
    let dir = tempfile::tempdir().unwrap();
    let clock = TestClock::new(1);
    let (ana, bia) = (who(1, "Ana"), who(2, "Bia"));
    let a = ProjectFolder::create(
        &dir.path().join("p"),
        "P",
        Approach::Dmaic,
        &ana,
        &clock,
        None,
    )
    .unwrap();
    let b = ProjectFolder::open(a.root(), &bia, None).unwrap();
    let card = EntityRef::new(Kind::Action, clock.new_id());
    a.heartbeat(&clock, "acoes", Some(card)).unwrap();
    b.heartbeat(&clock, "termo", None).unwrap();
    b.heartbeat(&clock, "acoes", None).unwrap();
    let now = clock.now_ms();
    let seen_by_b = b.others(now);
    assert_eq!(seen_by_b.len(), 1);
    assert_eq!(seen_by_b[0].name, "Ana");
    assert_eq!(seen_by_b[0].editing, Some(card));
    assert_eq!(a.others(now)[0].screen, "acoes");
    // Stale after a few minutes; gone after leaving.
    assert!(b.others(now + 10 * 60_000).is_empty());
    a.leave();
    assert!(b.others(now).is_empty());
    // A broken presence file is ignored.
    fs::write(a.root().join("presenca").join("lixo.json"), "{").unwrap();
    assert!(b.others(now).is_empty());
}

#[test]
fn attachments_by_hash() {
    let dir = tempfile::tempdir().unwrap();
    let clock = TestClock::new(1);
    let ana = who(1, "Ana");
    let a = ProjectFolder::create(
        &dir.path().join("p"),
        "P",
        Approach::Dmaic,
        &ana,
        &clock,
        None,
    )
    .unwrap();
    let src = dir.path().join("Foto Antes.JPG");
    fs::write(&src, b"imagem").unwrap();
    let n1 = a.add_attachment(&src).unwrap();
    let n2 = a.add_attachment(&src).unwrap();
    assert_eq!(n1, n2);
    // sha256("imagem")
    assert!(n1.ends_with(".jpg") && n1.len() == 64 + 4, "{n1}");
    assert_eq!(
        fs::read(a.root().join("anexos").join(&n1)).unwrap(),
        b"imagem"
    );
}

#[test]
fn lssz_round_trip() {
    let dir = tempfile::tempdir().unwrap();
    let clock = TestClock::new(1);
    let ana = who(1, "Ana");
    let mut a = ProjectFolder::create(
        &dir.path().join("Linha"),
        "P",
        Approach::Dmaic,
        &ana,
        &clock,
        None,
    )
    .unwrap();
    let p = a.project_id();
    a.commit(
        kanban::new_card(a.state(), p, clock.new_id(), "x", Column::Todo),
        &clock,
    )
    .unwrap();
    a.heartbeat(&clock, "acoes", None).unwrap();
    let file = dir.path().join("Linha.lssz");
    lssz::export(a.root(), &file).unwrap();
    let elsewhere = dir.path().join("outro");
    fs::create_dir_all(&elsewhere).unwrap();
    let imported = lssz::import(&file, &elsewhere).unwrap();
    assert_eq!(imported, elsewhere.join("Linha.lss"));
    assert!(
        !imported.join("presenca").exists(),
        "presence is not exported"
    );
    let b = ProjectFolder::open(&imported, &ana, None).unwrap();
    assert_eq!(b.state(), a.state());
    assert!(matches!(
        lssz::import(&file, &elsewhere),
        Err(Error::Exists(_))
    ));
    assert!(matches!(
        lssz::export(dir.path(), &file),
        Err(Error::NotAProject(_))
    ));
}

#[test]
fn lssz_rejects_unsafe_archives() {
    let dir = tempfile::tempdir().unwrap();
    let make = |name: &str, entries: &[&str]| {
        let path = dir.path().join(name);
        let mut z = zip::ZipWriter::new(fs::File::create(&path).unwrap());
        for e in entries {
            z.start_file(*e, zip::write::SimpleFileOptions::default())
                .unwrap();
            std::io::Write::write_all(&mut z, b"{}").unwrap();
        }
        z.finish().unwrap();
        path
    };
    let out = dir.path().join("dest");
    fs::create_dir_all(&out).unwrap();
    for (name, entries) in [
        ("fuga.lssz", &["../fora.json"][..]),
        ("solto.lssz", &["projeto.json"][..]),
        (
            "dois.lssz",
            &["a.lss/projeto.json", "b.lss/projeto.json"][..],
        ),
        ("sem.lssz", &["a.lss/ops/x.json"][..]),
    ] {
        let f = make(name, entries);
        assert!(lssz::import(&f, &out).is_err(), "{name}");
    }
    assert!(!out.join("a.lss").exists(), "partial import removed");
    assert!(!dir.path().join("fora.json").exists());
}

#[test]
fn newer_project_format_is_refused() {
    let dir = tempfile::tempdir().unwrap();
    let clock = TestClock::new(1);
    let ana = who(1, "Ana");
    let a = ProjectFolder::create(
        &dir.path().join("p"),
        "P",
        Approach::Dmaic,
        &ana,
        &clock,
        None,
    )
    .unwrap();
    let m = a.root().join(project::MANIFEST);
    let text = fs::read_to_string(&m)
        .unwrap()
        .replace("\"formato\": 1", "\"formato\": 2");
    fs::write(&m, text).unwrap();
    let err = ProjectFolder::open(a.root(), &ana, None).unwrap_err();
    assert!(
        err.to_string()
            .contains("projeto no formato 2; esta versão do UTSim LSS lê até 1"),
        "{err}"
    );
}

#[test]
fn system_clock_makes_v7_ids() {
    let c = SystemClock;
    let (a, b) = (c.new_id(), c.new_id());
    assert_ne!(a, b);
    assert_eq!(a.version(), 7);
    assert!(c.now_ms() > 1_700_000_000_000);
    let _ = Path::new("");
}

#[test]
fn portfolio_folder_is_a_journal_with_its_unit_and_program() {
    let dir = tempfile::tempdir().unwrap();
    let clock = TestClock::new(4);
    let ana = who(1, "Ana");
    let pf = ProjectFolder::create_portfolio(
        &dir.path().join("Carteira Estamparia"),
        "Carteira Estamparia",
        "Planta 1",
        "Programa LSS",
        &ana,
        &clock,
        None,
    )
    .unwrap();
    assert!(pf.is_portfolio());
    assert_eq!(pf.title(), "Carteira Estamparia");
    let root = pf.root().to_path_buf();
    assert!(project::is_project(&root), "same folder layout");
    let again = ProjectFolder::open(&root, &ana, None).unwrap();
    assert!(again.is_portfolio());
    let c = utsim_lss_core::portfolio::read(again.state(), again.project_id()).unwrap();
    assert_eq!(
        utsim_lss_core::portfolio::name_of(again.state(), c.unit),
        "Planta 1"
    );
    assert_eq!(
        utsim_lss_core::portfolio::name_of(again.state(), c.program),
        "Programa LSS"
    );
    // A project folder is not a portfolio.
    let p = ProjectFolder::create(
        &dir.path().join("Linha 2"),
        "Linha 2",
        Approach::Kaizen,
        &ana,
        &clock,
        None,
    )
    .unwrap();
    assert!(!p.is_portfolio());
}

#[test]
fn system_clock_offset_is_within_a_day() {
    let off = SystemClock.utc_offset_minutes();
    assert!((-14 * 60..=14 * 60).contains(&off), "{off}");
}
