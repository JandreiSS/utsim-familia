//! Ideas sent to a portfolio by another app (the 8D of UTSim Qualidade): the idea with its
//! origin, who sent it and the attached files copied by content, and the refusals.

use std::fs;
use std::sync::atomic::{AtomicU64, Ordering};

use utsim_lss_core::{Id, portfolio};
use utsim_lss_store::intake::{self, IntakeError};
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

#[test]
fn idea_from_the_8d_with_attachments() {
    let dir = tempfile::tempdir().unwrap();
    let ana = who(1, "Ana Souza", "ana@empresa.com");
    let rui = who(4, "Rui Prado", "rui@empresa.com");
    let ca = TestClock::new(1);
    let cr = TestClock::new(4);
    let root = ProjectFolder::create_portfolio(
        &dir.path().join("Carteira"),
        "Carteira da planta 1",
        "Planta 1",
        "Programa LSS",
        &ana,
        &ca,
        None,
    )
    .unwrap()
    .root()
    .to_path_buf();
    let files = dir.path().join("8D-0042");
    fs::create_dir_all(&files).unwrap();
    fs::write(files.join("relatorio-8d.pdf"), b"%PDF-1.4 relatorio").unwrap();
    fs::write(
        files.join("pareto.csv"),
        "defeito;n
rebarba;30
",
    )
    .unwrap();
    fs::write(
        files.join("copia.csv"),
        "defeito;n
rebarba;30
",
    )
    .unwrap();
    let sub = portfolio::Submission {
        title: "Rebarba no furo da tampa".into(),
        process: "Usinagem da tampa".into(),
        problem: "Reclamações de clientes por rebarba; a contenção do 8D não segurou".into(),
        data: "Pareto de defeitos de 3 meses".into(),
        rough_gain: 48_000.0,
        source: portfolio::SOURCE_8D.into(),
        reference: "8D-0042".into(),
    };
    let id = intake::submit_idea(
        &root,
        &rui,
        &cr,
        &sub,
        &[
            files.join("relatorio-8d.pdf"),
            files.join("pareto.csv"),
            files.join("copia.csv"),
        ],
    )
    .unwrap();
    let pf = ProjectFolder::open(&root, &ana, None).unwrap();
    let ideas = portfolio::ideas(pf.state(), pf.project_id());
    let idea = ideas.iter().find(|i| i.id == id).unwrap();
    assert_eq!(idea.title, "Rebarba no furo da tampa");
    assert_eq!(idea.rough_gain, 48_000.0);
    assert_eq!(
        portfolio::source_of(idea),
        Some((portfolio::SOURCE_8D.to_string(), "8D-0042".to_string()))
    );
    assert_eq!(idea.entity.id(portfolio::fields::SENT_BY), Some(rui.person));
    let att = portfolio::attachments(pf.state(), id);
    assert_eq!(att.len(), 3);
    assert_eq!(att[0].name, "relatorio-8d.pdf");
    assert_eq!(att[1].file, att[2].file, "same content, one file");
    for a in &att {
        assert!(intake::attachment_path(&root, &a.file).is_file());
    }
    // Refusals: no title, no origin, not a portfolio, a file that is not there.
    let mut bad = sub.clone();
    bad.title = " ".into();
    assert!(matches!(
        intake::submit_idea(&root, &rui, &cr, &bad, &[]),
        Err(IntakeError::NoTitle)
    ));
    let mut bad = sub.clone();
    bad.source.clear();
    assert!(matches!(
        intake::submit_idea(&root, &rui, &cr, &bad, &[]),
        Err(IntakeError::NoSource)
    ));
    let proj = ProjectFolder::create(
        &dir.path().join("Projeto"),
        "Projeto",
        utsim_lss_core::domain::Approach::Dmaic,
        &ana,
        &ca,
        None,
    )
    .unwrap()
    .root()
    .to_path_buf();
    assert!(matches!(
        intake::submit_idea(&proj, &rui, &cr, &sub, &[]),
        Err(IntakeError::NotAPortfolio)
    ));
    let before = portfolio::ideas(pf.state(), pf.project_id()).len();
    assert!(matches!(
        intake::submit_idea(&root, &rui, &cr, &sub, &[files.join("nao-existe.pdf")]),
        Err(IntakeError::Store(_))
    ));
    let pf = ProjectFolder::open(&root, &ana, None).unwrap();
    assert_eq!(portfolio::ideas(pf.state(), pf.project_id()).len(), before);
    let _ = Id::v7(0, [0; 10]);
}
