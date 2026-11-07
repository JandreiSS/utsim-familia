//! The program folder in temporary directories: created next to a portfolio and its
//! project, the scan maps them to the program's unit, operations of an author without the
//! profile are listed and ignored after a refresh and a reopening, the coaching log lives
//! in its own folder, and the anonymisation leaves no personal value in any file.

use std::fs;
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};

use utsim_lss_core::domain::{Approach, ProjectStatus, RoleKind, enum_value};
use utsim_lss_core::op::{Edit, EntityRef};
use utsim_lss_core::{Date, Id, Kind, mentoria, perfis, portfolio, privacidade, programa, roles};
use utsim_lss_store::program::{self, COACHING};
use utsim_lss_store::project::ProjectFolder;
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

fn who(n: u8, name: &str, email: &str) -> Identity {
    Identity {
        person: Id::v7(1_700_000_000_000, [n; 10]),
        name: name.into(),
        email: email.into(),
    }
}

fn every_file_text(dir: &Path) -> String {
    let mut out = String::new();
    for e in fs::read_dir(dir).unwrap().flatten() {
        let p = e.path();
        if p.is_dir() {
            out.push_str(&every_file_text(&p));
        } else if let Ok(t) = fs::read_to_string(&p) {
            out.push_str(&t);
        }
    }
    out
}

#[test]
fn program_folder_scan_permissions_and_anonymisation() {
    let dir = tempfile::tempdir().unwrap();
    let drive = dir.path().join("Drive");
    fs::create_dir_all(drive.join("Planta 1")).unwrap();
    let ana = who(1, "Ana Souza", "ana.souza@empresa.com");
    let bia = who(2, "Bia Reis", "bia@empresa.com");
    let ca = TestClock::new(1);
    let cb = TestClock::new(2);

    // The MBB creates the program; its creator is an MBB from the start.
    let mut prog =
        ProjectFolder::create_program(&drive.join("Programa"), "Programa LSS", &ana, &ca).unwrap();
    assert!(prog.is_program());
    assert!(prog.root().join(COACHING).is_dir());
    let p = prog.project_id();
    let founder = prog.manifest().created_by;
    let u1 = ca.new_id();
    prog.commit(programa::add_unit(p, u1, "Planta 1"), &ca)
        .unwrap();

    // A portfolio and a project in the unit's folder.
    let mut port = ProjectFolder::create_portfolio(
        &drive.join("Planta 1").join("Carteira"),
        "Carteira Planta 1",
        "Planta 1",
        "Programa LSS",
        &ana,
        &ca,
        None,
    )
    .unwrap();
    let c = port.project_id();
    let idea = ca.new_id();
    port.commit(portfolio::new_idea(c, idea, "Setup longo"), &ca)
        .unwrap();
    let e = portfolio::move_idea(port.state(), idea, ProjectStatus::Screening, "").unwrap();
    port.commit(e, &ca).unwrap();
    let mut proj = ProjectFolder::create(
        &drive.join("Planta 1").join("Refugo"),
        "Refugo na prensa",
        Approach::Dmaic,
        &ana,
        &ca,
        None,
    )
    .unwrap();
    let e = roles::add_role(
        proj.state(),
        proj.project_id(),
        ca.new_id(),
        ana.person,
        RoleKind::GreenBelt,
    );
    proj.commit(e, &ca).unwrap();
    prog.commit(
        vec![Edit::new(
            programa::Unit::entity_ref(u1),
            programa::fields::PORTFOLIO,
            c.to_string(),
        )],
        &ca,
    )
    .unwrap();

    let auth = perfis::replay(prog.all_ops(), founder);
    let scan = program::scan(prog.root(), &auth.state, p, &ana, None, 0);
    assert_eq!(scan.portfolios.len(), 1);
    assert_eq!(scan.portfolios[0].unit, Some(u1));
    assert_eq!(
        scan.projects.len(),
        1,
        "the program folder itself is not a project"
    );
    let rec = &scan.projects[0].1;
    assert_eq!(rec.unit, Some(u1));
    assert_eq!(rec.portfolio, Some(c));
    assert_eq!(rec.belts, vec![ana.person]);
    assert_eq!(rec.folder, "Refugo.lss");
    assert_eq!(scan.ideas.len(), 1);
    assert_eq!(scan.ideas[0].status, ProjectStatus::Screening);
    assert_eq!(scan.ideas[0].unit, Some(u1));
    assert_eq!(
        scan.path_of(rec.id),
        Some(drive.join("Planta 1").join("Refugo.lss").as_path())
    );

    // Bia writes a target into the program folder by hand (no profile): listed, no effect.
    let mut prog_b = ProjectFolder::open_full(prog.root(), &bia).unwrap();
    let t = cb.new_id();
    let e = programa::set_target(
        &auth.state,
        p,
        t,
        u1,
        2026,
        programa::Metric::HardValidated,
        None,
        9e9,
    )
    .unwrap();
    prog_b.commit(e, &cb).unwrap();
    // Bia registers herself (allowed) and gets a champion profile from the MBB later.
    prog_b.register_me(&cb).unwrap();
    prog.refresh(&ca).unwrap();
    let auth = perfis::replay(prog.all_ops(), founder);
    assert!(auth.state.entity(t).is_none());
    assert!(!auth.unauthorized.is_empty());
    assert!(auth.unauthorized.iter().all(|u| u.author == bia.person));
    assert_eq!(
        auth.state.entity(bia.person).unwrap().text("nome"),
        "Bia Reis",
        "registering oneself is free"
    );
    // Reopened from the files: the same.
    let again = ProjectFolder::open_full(prog.root(), &ana).unwrap();
    assert_eq!(perfis::replay(again.all_ops(), founder), auth);

    // The coaching log of Ana in its own folder.
    let log_path = program::coaching_path(prog.root(), ana.person);
    let mut log = ProjectFolder::create_coaching(&log_path, ana.person, p, &ana, &ca).unwrap();
    assert_eq!(log.main_kind(), Some(Kind::CoachingLog));
    let today = Date::new(2026, 11, 3).unwrap();
    let mut belt = programa::Member::read(ana.person, auth.state.entity(ana.person).unwrap());
    belt.level = Some(programa::BeltLevel::Green);
    belt.consent = true;
    let s = ca.new_id();
    let e = mentoria::new_session(
        &belt,
        s,
        &mentoria::SessionInput {
            belt: ana.person,
            mentor: founder,
            date: today,
            minutes: 60.0,
            form: mentoria::Form::Individual,
            project: Some(rec.id),
            phase: "Medir".into(),
            topics: "conversa difícil com a supervisão".into(),
            tools: vec!["msa".into()],
        },
    )
    .unwrap();
    log.commit(e, &ca).unwrap();
    let log_auth = perfis::replay_coaching(log.all_ops(), ana.person, &[founder]);
    assert_eq!(mentoria::sessions(&log_auth.state).len(), 1);
    // The scan does not take the log for a project.
    let scan = program::scan(prog.root(), &auth.state, p, &ana, None, 0);
    assert_eq!(scan.projects.len(), 1);

    // Anonymisation: the operation, then the rewriting of the history.
    prog.commit(
        vec![Edit::new(
            programa::Member::entity_ref(ana.person),
            programa::fields::LEVEL,
            enum_value(programa::BeltLevel::Green),
        )],
        &ca,
    )
    .unwrap();
    let e = privacidade::anonymise(&perfis::replay(prog.all_ops(), founder).state, ana.person);
    prog.commit(e, &ca).unwrap();
    let e = privacidade::anonymise_coaching(&log_auth.state, ana.person);
    log.commit(e, &ca).unwrap();
    let st = perfis::replay(prog.all_ops(), founder).state;
    assert!(program::redact_person(prog.root(), &st, ana.person).unwrap() >= 2);
    assert!(program::redact_coaching(&log_path, ana.person).unwrap() >= 1);
    let text = every_file_text(prog.root());
    for original in ["Ana Souza", "ana.souza@empresa.com", "supervisão"] {
        assert!(!text.contains(original), "{original} survived");
    }
    // The folder still opens and converges after the rewriting.
    let reopened = ProjectFolder::open_full(prog.root(), &ana).unwrap();
    let st2 = perfis::replay(reopened.all_ops(), founder).state;
    let m = programa::Member::read(ana.person, st2.entity(ana.person).unwrap());
    assert!(m.anonymised);
    assert_eq!(m.name, privacidade::placeholder(ana.person));
    let _ = EntityRef::new(Kind::Person, ana.person);
}

#[test]
fn ensino_records_own_practice_in_the_program() {
    use utsim_lss_store::program::TeachingError;
    let dir = tempfile::tempdir().unwrap();
    let ana = who(1, "Ana Souza", "ana@empresa.com");
    let bia = who(2, "Bia Reis", "bia@empresa.com");
    let carla = who(3, "Carla Lima", "carla@empresa.com");
    let ca = TestClock::new(1);
    let cb = TestClock::new(2);
    let root =
        ProjectFolder::create_program(&dir.path().join("Programa"), "Programa LSS", &ana, &ca)
            .unwrap()
            .root()
            .to_path_buf();
    // Bia opened the program once: she is in the people list.
    let mut b = ProjectFolder::open(&root, &bia, None).unwrap();
    b.register_me(&cb).unwrap();
    let id = program::record_teaching(
        &root,
        &bia,
        &cb,
        "fmea",
        mentoria::Level::Practiced,
        "Caso final do UTSim Ensino, nota 8,5",
    )
    .unwrap();
    let pf = ProjectFolder::open(&root, &ana, None).unwrap();
    let e = mentoria::entries(pf.state());
    let got = e.iter().find(|x| x.id == id).unwrap();
    assert_eq!(got.source, mentoria::Source::Teaching);
    assert_eq!(got.person, Some(bia.person));
    assert_eq!(got.by, Some(bia.person));
    // On the map, with the origin, and the replay keeps it (no unauthorized op).
    let map = mentoria::competence_map(&e, &[], |p| p == ana.person);
    assert_eq!(
        map.level(bia.person, "fmea"),
        Some(mentoria::Level::Practiced)
    );
    let rep = perfis::replay(pf.all_ops(), ana.person);
    assert!(rep.unauthorized.is_empty(), "{:?}", rep.unauthorized);
    // Written by hand past the app (no check before writing): an Ensino entry raised to
    // `demonstrou` is listed by the replay and never reaches the map at that level.
    let forged = Id::v7(1_793_000_100_000, [9; 10]);
    let r = EntityRef::new(Kind::Competence, forged);
    b.refresh(&cb).unwrap();
    b.commit(
        vec![
            Edit::new(
                r,
                mentoria::fields::SOURCE,
                enum_value(mentoria::Source::Teaching),
            ),
            Edit::new(r, mentoria::fields::PERSON, bia.person.to_string()),
            Edit::new(
                r,
                mentoria::fields::LEVEL,
                enum_value(mentoria::Level::Demonstrated),
            ),
            Edit::new(r, mentoria::fields::TOOL, "a3"),
        ],
        &cb,
    )
    .unwrap();
    let pf = ProjectFolder::open(&root, &ana, None).unwrap();
    let rep = perfis::replay(pf.all_ops(), ana.person);
    assert!(
        rep.unauthorized
            .iter()
            .any(|u| u.field == mentoria::fields::LEVEL)
    );
    let e = mentoria::entries(&rep.state);
    let map = mentoria::competence_map(&e, &[], |p| p == ana.person);
    assert_ne!(
        map.level(bia.person, "a3"),
        Some(mentoria::Level::Demonstrated)
    );
    // Refusals: someone not in the program, a level Ensino may not give, an unknown tool.
    assert!(matches!(
        program::record_teaching(&root, &carla, &ca, "fmea", mentoria::Level::Studied, ""),
        Err(TeachingError::NotAMember(_))
    ));
    assert!(matches!(
        program::record_teaching(&root, &bia, &cb, "fmea", mentoria::Level::Demonstrated, ""),
        Err(TeachingError::Level)
    ));
    assert!(matches!(
        program::record_teaching(&root, &bia, &cb, "xyz", mentoria::Level::Studied, ""),
        Err(TeachingError::Tool(_))
    ));
    assert!(matches!(
        program::record_teaching(
            &dir.path().join("nada"),
            &bia,
            &cb,
            "fmea",
            mentoria::Level::Studied,
            ""
        ),
        Err(TeachingError::Store(_))
    ));
}
