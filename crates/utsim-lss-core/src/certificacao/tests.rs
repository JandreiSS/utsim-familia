use super::*;
use crate::formacao::{
    Attendance, ClassInput, NominationInput, Proposal, exams, mark, new_class, new_session,
    nominate,
};
use crate::mentoria::{Cell, Evidence};
use crate::programa::TollgateRecord;
use crate::programa::testkit::*;
use crate::tollgate::TollgateStatus;

fn tg(i: usize, changed: bool) -> TollgateRecord {
    TollgateRecord {
        id: Id::v7(1000 + i as u64, [i as u8; 10]),
        phase: Id::v7(2000 + i as u64, [i as u8; 10]),
        phase_name: format!("F{i}"),
        phase_index: i,
        status: TollgateStatus::Approved,
        round: 1,
        planned: None,
        submitted_on: None,
        approved_on: Some(d(2027, 1, 1 + i as u8)),
        verified: vec![],
        frozen_hash: format!("{i:064}"),
        changed_after_approval: changed,
    }
}

fn closed(id: Id, belt: Id, validated: f64, forecast: f64) -> ProjectRecord {
    let mut p = project(id, None, ProjectStatus::Closed);
    p.belts = vec![belt];
    p.members = vec![belt];
    p.closed_on = Some(d(2027, 3, 1));
    p.tollgates = (0..5).map(|i| tg(i, false)).collect();
    p.totals
        .by
        .insert((Category::Hard, SavingStatus::Validated), validated);
    p.totals
        .by
        .insert((Category::Hard, SavingStatus::Forecast), forecast);
    p
}

/// A green belt trainee with a class, attendance and exam done in a synthetic program.
struct World {
    t: T,
    class: formacao::Class,
    person: Id,
    exams: Vec<exams::Exam>,
    results: Vec<exams::ExamResult>,
}

fn world(attend: usize, score: f64) -> World {
    let mut t = T::new();
    let p = t.program;
    let class = t.id();
    t.commit(
        new_class(
            p,
            class,
            &ClassInput {
                wave: None,
                name: "GB".into(),
                level: BeltLevel::Green,
                start: None,
                end: None,
                place: String::new(),
                seats: 10,
            },
        )
        .unwrap(),
    );
    let person = t.person("Ana", Some(BeltLevel::Yellow), None);
    let manager = t.person("Gestor", None, None);
    let nid = t.id();
    t.commit(
        nominate(
            p,
            nid,
            &NominationInput {
                person: Some(person),
                level: BeltLevel::Green,
                by: manager,
                justification: "x".into(),
                opportunity: Some(Id::v7(1, [1; 10])),
                manager: Some(manager),
                ..NominationInput::default()
            },
            &[],
        )
        .unwrap(),
    );
    let me = t.me;
    t.commit(formacao::confirm_selection(
        class,
        &Proposal {
            selected: vec![nid],
            ..Proposal::default()
        },
        me,
        d(2026, 9, 1),
    ));
    for i in 0..5 {
        let s = t.id();
        t.commit(new_session(class, s, d(2026, 10, 1 + i), 4.0, &[], None));
        let a = if (i as usize) < attend {
            Attendance::Present
        } else {
            Attendance::Absent
        };
        t.commit(vec![mark(s, person, a)]);
    }
    // An exam and its result (the bank is not needed to record a result).
    let ex_id = t.id();
    let r = exams::Exam::entity_ref(ex_id);
    t.commit(vec![
        Edit::new(r, formacao::fields::CLASS, class.to_string()),
        Edit::new(r, formacao::fields::NAME, "Prova"),
    ]);
    let all = exams::exams(&t.s);
    let rid = t.id();
    t.commit(exams::record_result(
        rid,
        &all[0],
        person,
        score,
        &[],
        me,
        d(2026, 11, 1),
    ));
    let class = formacao::classes(&t.s, p)[0].clone();
    let results = exams::results(&t.s);
    World {
        t,
        class,
        person,
        exams: all,
        results,
    }
}

fn map_with(person: Id, tools: &[&str]) -> CompetenceMap {
    let mut m = CompetenceMap::default();
    for t in tools {
        m.cells.insert(
            (person, t.to_string()),
            Cell {
                level: Some(SkillLevel::Applied),
                evidence: vec![Evidence::Deliverable {
                    project: Id::NIL,
                    deliverable: Id::NIL,
                }],
            },
        );
    }
    m
}

fn run(
    w: &World,
    projects: &[ProjectRecord],
    map: &CompetenceMap,
    opinions: &BTreeMap<(Id, BeltLevel), (Id, Option<Date>)>,
) -> Vec<Check> {
    let sessions = formacao::all_sessions(&w.t.s);
    let noms = formacao::nominations(&w.t.s, w.t.program);
    let ctx = Context {
        classes: std::slice::from_ref(&w.class),
        sessions: &sessions,
        nominations: &noms,
        exams: &w.exams,
        results: &w.results,
        projects,
        ideas: &[],
        assignments: &[],
        map,
        opinions,
    };
    let reqs = requirements(&w.t.s, w.t.program, BeltLevel::Green);
    check(&reqs, w.person, BeltLevel::Green, &ctx)
}

fn status_of(cs: &[Check], k: RequirementKind) -> &Check {
    cs.iter().find(|c| c.kind == k).unwrap()
}

#[test]
fn default_table_of_mb4() {
    let t = T::new();
    let g = requirements(&t.s, t.program, BeltLevel::Green);
    let kinds: Vec<RequirementKind> = g.iter().map(|r| r.kind).collect();
    assert_eq!(
        kinds,
        vec![
            RequirementKind::Training,
            RequirementKind::Exam,
            RequirementKind::ProjectsLed,
            RequirementKind::Savings,
            RequirementKind::Tools,
            RequirementKind::MbbOpinion
        ]
    );
    let b = requirements(&t.s, t.program, BeltLevel::Black);
    assert!(b.iter().any(|r| r.kind == RequirementKind::Coaching));
    assert_eq!(
        b.iter()
            .find(|r| r.kind == RequirementKind::ProjectsLed)
            .unwrap()
            .count,
        2
    );
    let y = requirements(&t.s, t.program, BeltLevel::Yellow);
    assert!(y.iter().any(|r| r.kind == RequirementKind::ProjectsJoined));
    assert!(!y.iter().any(|r| r.kind == RequirementKind::MbbOpinion));
    assert!(requirements(&t.s, t.program, BeltLevel::MasterBlack).is_empty());
}

#[test]
fn every_requirement_met_then_issue() {
    let w = world(5, 0.8);
    let proj = closed(Id::v7(500, [5; 10]), w.person, 10_000.0, 0.0);
    let map = map_with(w.person, &GREEN_TOOLS);
    let cert_id = Id::v7(600, [6; 10]);
    let ops: BTreeMap<_, _> = [((w.person, BeltLevel::Green), (cert_id, Some(d(2027, 4, 1))))]
        .into_iter()
        .collect();
    let cs = run(&w, std::slice::from_ref(&proj), &map, &ops);
    assert!(all_met(&cs), "{cs:#?}");
    // Each requirement keeps evidence with a hash.
    assert!(cs.iter().all(|c| !c.evidence.is_empty()));
    assert!(
        cs.iter()
            .flat_map(|c| &c.evidence)
            .all(|e| e.hash.len() == 64)
    );
    let mut t = T::new();
    let me = t.me;
    let p = t.program;
    assert_eq!(
        issue(
            p,
            cert_id,
            w.person,
            BeltLevel::Green,
            &cs,
            &[],
            me,
            false,
            d(2027, 4, 2)
        ),
        Err(CertError::NotMbb)
    );
    t.commit(
        issue(
            p,
            cert_id,
            w.person,
            BeltLevel::Green,
            &cs,
            &[],
            me,
            true,
            d(2027, 4, 2),
        )
        .unwrap(),
    );
    let certs = certifications(&t.s, p);
    assert_eq!(certs[0].number, Some(1));
    assert_eq!(certs[0].valid_until, Some(d(2030, 4, 1)));
    assert_eq!(certs[0].met.len(), cs.len());
    let m = programa::members(&t.s);
    let ana = m.iter().find(|x| x.id == w.person);
    // The person entity is created by the issue with the level and the date.
    assert_eq!(ana.and_then(|x| x.level), Some(BeltLevel::Green));
    // The next one gets number 2.
    assert_eq!(next_number(&certs), 2);
}

#[test]
fn each_requirement_fails_with_its_reason() {
    // Attendance 3 of 5 (60 %), exam 50 %, no project, no tools, no opinion.
    let w = world(3, 0.5);
    let cs = run(&w, &[], &CompetenceMap::default(), &BTreeMap::new());
    for c in &cs {
        assert_eq!(c.status, Status::NotMet, "{c:?}");
        assert!(!c.reason.is_empty());
    }
    assert!(
        status_of(&cs, RequirementKind::Training)
            .reason
            .contains("60 %")
    );
    assert!(
        status_of(&cs, RequirementKind::Exam)
            .reason
            .contains("reaplicação")
    );
    assert!(
        status_of(&cs, RequirementKind::Tools)
            .reason
            .starts_with("faltam")
    );
    assert!(!all_met(&cs));
    let me = Id::v7(1, [1; 10]);
    assert_eq!(
        issue(
            Id::NIL,
            Id::NIL,
            w.person,
            BeltLevel::Green,
            &cs,
            &[],
            me,
            true,
            d(2027, 1, 1)
        ),
        Err(CertError::NotMet)
    );
}

#[test]
fn ambiguous_data_goes_to_the_mbb() {
    let w = world(5, 0.9);
    // Only a forecast saving: ambiguous. A tollgate changed after approval: ambiguous.
    let mut proj = closed(Id::v7(500, [5; 10]), w.person, 0.0, 5_000.0);
    proj.tollgates[2].changed_after_approval = true;
    let cs = run(&w, &[proj], &CompetenceMap::default(), &BTreeMap::new());
    let s = status_of(&cs, RequirementKind::Savings);
    assert_eq!(s.status, Status::Ambiguous, "{s:?}");
    assert!(s.reason.contains("só o validado conta"));
    let p = status_of(&cs, RequirementKind::ProjectsLed);
    assert_eq!(p.status, Status::Ambiguous, "{p:?}");
    // A training project never counts, and simulated savings are not savings at all.
    let mut tr = closed(Id::v7(501, [5; 10]), w.person, 99_999.0, 0.0);
    tr.training = true;
    let cs = run(&w, &[tr], &CompetenceMap::default(), &BTreeMap::new());
    assert_eq!(
        status_of(&cs, RequirementKind::Savings).status,
        Status::NotMet
    );
    assert_eq!(
        status_of(&cs, RequirementKind::ProjectsLed).status,
        Status::NotMet
    );
}

#[test]
fn savings_threshold_of_the_company() {
    let mut w = world(5, 0.9);
    let p = w.t.program;
    let rid = w.t.id();
    w.t.commit(set_requirement(
        p,
        rid,
        BeltLevel::Green,
        RequirementKind::Savings,
        1,
        50_000.0,
        &[],
    ));
    let reqs = requirements(&w.t.s, p, BeltLevel::Green);
    assert_eq!(reqs.len(), 1);
    let proj = closed(Id::v7(500, [5; 10]), w.person, 50_000.0, 0.0);
    let cs = run(
        &w,
        std::slice::from_ref(&proj),
        &CompetenceMap::default(),
        &BTreeMap::new(),
    );
    assert_eq!(cs[0].status, Status::Met, "{cs:?}");
    let proj = closed(Id::v7(500, [5; 10]), w.person, 49_999.99, 0.0);
    let cs = run(&w, &[proj], &CompetenceMap::default(), &BTreeMap::new());
    assert_eq!(cs[0].status, Status::NotMet);
}

#[test]
fn coaching_requirement_for_black_belt() {
    let bb = Id::v7(70, [7; 10]);
    let gb = Id::v7(71, [7; 10]);
    let proj = closed(Id::v7(72, [7; 10]), gb, 0.0, 0.0);
    let mut idea = idea(Id::v7(73, [7; 10]), None, ProjectStatus::Closed, 0.0);
    idea.project = Some(proj.id);
    let a = Assignment {
        id: Id::v7(74, [7; 10]),
        opportunity: None,
        idea: Some(idea.id),
        person: Some(gb),
        hours_week: 4.0,
        mentor: Some(bb),
        position: None,
        reason: String::new(),
        date: None,
        by: None,
        ended: false,
    };
    let req = Requirement {
        id: Id::NIL,
        level: BeltLevel::Black,
        kind: RequirementKind::Coaching,
        count: 1,
        value: 0.0,
        tools: vec![],
        active: true,
    };
    let map = CompetenceMap::default();
    let ops = BTreeMap::new();
    let mk = |assigns: &[Assignment]| {
        let ideas = [idea.clone()];
        let projects = [proj.clone()];
        let ctx = Context {
            classes: &[],
            sessions: &[],
            nominations: &[],
            exams: &[],
            results: &[],
            projects: &projects,
            ideas: &ideas,
            assignments: assigns,
            map: &map,
            opinions: &ops,
        };
        check(std::slice::from_ref(&req), bb, BeltLevel::Black, &ctx)[0].status
    };
    assert_eq!(mk(std::slice::from_ref(&a)), Status::Met);
    assert_eq!(mk(&[]), Status::NotMet);
}

#[test]
fn validity_warning_and_renewal() {
    let mut t = T::new();
    let p = t.program;
    let me = t.me;
    let person = Id::v7(80, [8; 10]);
    let c1 = t.id();
    t.commit(
        record_manual(
            p,
            c1,
            person,
            BeltLevel::Green,
            "Formação anterior ao programa",
            &[],
            me,
            true,
            d(2026, 1, 10),
        )
        .unwrap(),
    );
    let certs = certifications(&t.s, p);
    let c = &certs[0];
    assert!(c.manual);
    assert_eq!(c.valid_until, Some(d(2029, 1, 9)));
    assert_eq!(validity(c, &certs, d(2028, 6, 1)), Validity::Valid);
    assert_eq!(
        validity(c, &certs, d(2028, 10, 11)),
        Validity::Expiring { days: 90 }
    );
    assert_eq!(validity(c, &certs, d(2029, 1, 10)), Validity::Expired);
    assert_eq!(to_renew(&certs, d(2028, 12, 1)).len(), 1);
    // A 29 February start.
    assert_eq!(valid_until(d(2028, 2, 29)), d(2031, 2, 27));
    // Renewal: a project closed in the period.
    let mut proj = closed(Id::v7(81, [8; 10]), person, 0.0, 0.0);
    proj.closed_on = Some(d(2027, 5, 1));
    let rules = RenewalRules::read(&t.s, p);
    assert_eq!(rules.projects, 1);
    let map = CompetenceMap::default();
    let ops = BTreeMap::new();
    let projects = [proj];
    let ctx = Context {
        classes: &[],
        sessions: &[],
        nominations: &[],
        exams: &[],
        results: &[],
        projects: &projects,
        ideas: &[],
        assignments: &[],
        map: &map,
        opinions: &ops,
    };
    let rc = renewal_check(c, &rules, &ctx);
    assert!(rc.ok, "{rc:?}");
    let c2 = t.id();
    t.commit(renew(p, c2, c, &rc, &certs, me, true, d(2028, 12, 1)).unwrap());
    let certs = certifications(&t.s, p);
    let new = certs.iter().find(|x| x.id == c2).unwrap();
    assert_eq!(new.number, Some(2));
    assert_eq!(new.date, Some(d(2029, 1, 10)));
    assert_eq!(new.valid_until, Some(d(2032, 1, 9)));
    assert_eq!(
        validity(&certs[0], &certs, d(2028, 12, 2)),
        Validity::Renewed
    );
    let (cur, v) = current(&certs, person, d(2029, 2, 1)).unwrap();
    assert_eq!((cur.id, v), (c2, Validity::Valid));
    assert_eq!(
        renew(p, t.id(), c, &rc, &certs, me, true, d(2028, 12, 1)),
        Err(CertError::NotRenewable)
    );
    // Without anything in the period, the renewal is refused.
    let empty = Context {
        projects: &[],
        ..ctx
    };
    let rc = renewal_check(new, &rules, &empty);
    assert!(!rc.ok);
}

#[test]
fn opinions_only_from_an_mbb() {
    let mbb = Id::v7(90, [9; 10]);
    let other = Id::v7(91, [9; 10]);
    let person = Id::v7(92, [9; 10]);
    let mut t = T::new();
    let p = t.program;
    let a = t.id();
    t.commit(
        record_opinion(
            p,
            a,
            person,
            BeltLevel::Green,
            "Pronto",
            other,
            d(2027, 1, 1),
        )
        .unwrap(),
    );
    let certs = certifications(&t.s, p);
    assert!(opinions(&certs, |x| x == mbb).is_empty());
    assert_eq!(opinions(&certs, |x| x == other).len(), 1);
    assert_eq!(
        record_opinion(p, a, person, BeltLevel::Green, " ", mbb, d(2027, 1, 1)),
        Err(CertError::NoOpinion)
    );
}

#[test]
fn certification_funnel_counts_and_exits() {
    let w = world(5, 0.8);
    let sessions = formacao::all_sessions(&w.t.s);
    let noms = formacao::nominations(&w.t.s, w.t.program);
    let map = CompetenceMap::default();
    let ops = BTreeMap::new();
    let proj = closed(Id::v7(500, [5; 10]), w.person, 1.0, 0.0);
    let projects = [proj];
    let ctx = Context {
        classes: std::slice::from_ref(&w.class),
        sessions: &sessions,
        nominations: &noms,
        exams: &w.exams,
        results: &w.results,
        projects: &projects,
        ideas: &[],
        assignments: &[],
        map: &map,
        opinions: &ops,
    };
    let f = funnel(None, BeltLevel::Green, &ctx, &[]);
    // The nomination has no manager approval: it stops at the first stage.
    assert_eq!(f.counts.get(&Stage::Nominated), Some(&1));
    assert_eq!(f.counts.get(&Stage::ManagerApproved), None);
    assert_eq!(
        f.exits.keys().next().map(|(s, r)| (*s, r.as_str())),
        Some((Stage::ManagerApproved, "aguardando o gestor"))
    );
    let mut approved = noms.clone();
    approved[0].manager_approval = Some(true);
    let ctx2 = Context {
        nominations: &approved,
        ..ctx
    };
    let cert = Certification {
        id: Id::NIL,
        number: Some(1),
        person: Some(w.person),
        level: BeltLevel::Green,
        date: None,
        valid_until: None,
        met: vec![],
        issuer: None,
        manual: false,
        note: String::new(),
        renewal_of: None,
        revoked: false,
        opinion: String::new(),
        opinion_by: None,
        opinion_on: None,
    };
    let f = funnel(None, BeltLevel::Green, &ctx2, &[cert]);
    for s in Stage::ALL {
        assert_eq!(f.counts.get(&s), Some(&1), "{s:?}");
    }
    assert!(f.exits.is_empty());
}
