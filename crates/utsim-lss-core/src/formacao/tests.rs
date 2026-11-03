use std::collections::BTreeMap;

use proptest::prelude::*;
use serde_json::Value;

use super::exams::{self, ExamInput, Mode, Problem, QuestionInput, QuestionKind};
use super::*;
use crate::perfis::{self, Profile};
use crate::programa::testkit::*;

fn class_input(level: BeltLevel, wave: Option<Id>, seats: u32) -> ClassInput {
    ClassInput {
        wave,
        name: "Turma GB 1".into(),
        level,
        start: Some(d(2026, 11, 2)),
        end: Some(d(2026, 11, 29)),
        place: "Sala 3".into(),
        seats,
    }
}

/// A program with a wave, a green belt class and `n` approved nominations.
struct Fixture {
    t: T,
    wave: Id,
    class: Id,
    people: Vec<Id>,
    manager: Id,
}

fn fixture(n: usize, seats: u32) -> Fixture {
    let mut t = T::new();
    let p = t.program;
    let wave = t.id();
    t.commit(
        new_wave(
            p,
            wave,
            "Onda 2026/2",
            Some(d(2026, 7, 1)),
            Some(d(2026, 12, 31)),
        )
        .unwrap(),
    );
    let class = t.id();
    t.commit(new_class(p, class, &class_input(BeltLevel::Green, Some(wave), seats)).unwrap());
    let manager = t.person("Gestora", None, None);
    let mut people = Vec::new();
    for i in 0..n {
        let who = t.person(&format!("Pessoa {i}"), Some(BeltLevel::Yellow), None);
        let nid = t.id();
        let opp = t.id();
        let noms = nominations(&t.s, p);
        let input = NominationInput {
            wave: Some(wave),
            person: Some(who),
            level: BeltLevel::Green,
            by: manager,
            justification: "Lidera a célula".into(),
            opportunity: Some(opp),
            manager: Some(manager),
            tenure_months: Some(12.0 + i as f64),
            function: "Analista".into(),
            rating: Some(3.0 + (i % 3) as f64),
            hours_available: Some(10.0),
            date: Some(d(2026, 8, 1 + i as u8 % 20)),
        };
        t.commit(nominate(p, nid, &input, &noms).unwrap());
        let nom = nominations(&t.s, p)
            .into_iter()
            .find(|x| x.id == nid)
            .unwrap();
        t.commit(manager_decide(&nom, manager, false, true, "", d(2026, 8, 25)).unwrap());
        people.push(who);
    }
    Fixture {
        t,
        wave,
        class,
        people,
        manager,
    }
}

#[test]
fn wave_targets_round_trip() {
    let mut t = T::new();
    let p = t.program;
    let w = t.id();
    let u = t.id();
    t.commit(new_wave(p, w, "Onda", None, None).unwrap());
    t.commit(vec![
        set_wave_target(w, BeltLevel::Green, None, 10),
        set_wave_target(w, BeltLevel::Green, Some(u), 4),
    ]);
    let wave = &waves(&t.s, p)[0];
    assert_eq!(wave.targets.get(&(BeltLevel::Green, None)), Some(&10));
    assert_eq!(wave.targets.get(&(BeltLevel::Green, Some(u))), Some(&4));
    t.commit(vec![set_wave_target(w, BeltLevel::Green, Some(u), 0)]);
    assert_eq!(waves(&t.s, p)[0].targets.len(), 1);
    assert_eq!(new_wave(p, w, "  ", None, None), Err(TrainingError::NoName));
}

#[test]
fn nomination_rules() {
    let f = fixture(1, 20);
    let p = f.t.program;
    let noms = nominations(&f.t.s, p);
    let base = NominationInput {
        wave: Some(f.wave),
        person: Some(f.people[0]),
        level: BeltLevel::Green,
        by: f.manager,
        justification: "x".into(),
        opportunity: Some(Id::v7(1, [1; 10])),
        manager: Some(f.manager),
        ..NominationInput::default()
    };
    // Duplicate of an active nomination.
    assert_eq!(
        nominate(p, Id::NIL, &base, &noms),
        Err(TrainingError::Duplicate)
    );
    let other = Id::v7(77, [7; 10]);
    let mut i = base.clone();
    i.person = Some(other);
    i.opportunity = None;
    assert_eq!(
        nominate(p, Id::NIL, &i, &noms),
        Err(TrainingError::NoOpportunity)
    );
    i.level = BeltLevel::Yellow;
    assert!(nominate(p, Id::NIL, &i, &noms).is_ok());
    i.manager = Some(other);
    assert_eq!(
        nominate(p, Id::NIL, &i, &noms),
        Err(TrainingError::BadManager)
    );
    i.manager = Some(f.manager);
    i.justification = " ".into();
    assert_eq!(
        nominate(p, Id::NIL, &i, &noms),
        Err(TrainingError::NoJustification)
    );
    // Only the manager decides; a refusal needs a reason.
    let n = &noms[0];
    assert_eq!(
        manager_decide(n, other, false, true, "", d(2026, 9, 1)),
        Err(TrainingError::NotTheManager)
    );
    assert_eq!(
        manager_decide(n, f.manager, false, false, "", d(2026, 9, 1)),
        Err(TrainingError::NoReason)
    );
    assert!(manager_decide(n, other, true, true, "", d(2026, 9, 1)).is_ok());
    assert!(n.approved());
}

#[test]
fn nomination_permissions_in_the_journal() {
    let mut f = fixture(1, 20);
    let p = f.t.program;
    let founder = f.t.me;
    let n = nominations(&f.t.s, p)[0].clone();
    let intruder = f.t.person("Outra", None, None);
    // Somebody else trying to approve, or to select, has no effect in the replay.
    f.t.commit_as(
        intruder,
        vec![Edit::new(
            Nomination::entity_ref(n.id),
            fields::MANAGER_REASON,
            "forjado",
        )],
    );
    f.t.commit_as(
        intruder,
        vec![Edit::new(
            Nomination::entity_ref(n.id),
            fields::STATUS,
            enum_value(NominationStatus::Selected),
        )],
    );
    // The person withdraws: allowed.
    f.t.commit_as(n.person.unwrap(), withdraw(&n, "mudou de área").unwrap());
    let a = perfis::replay(&f.t.ops, founder);
    let after = nominations(&a.state, p)[0].clone();
    assert_eq!(after.manager_reason, "");
    assert_eq!(after.status, NominationStatus::Withdrawn);
    assert_eq!(a.unauthorized.len(), 2);
    assert!(a.unauthorized.iter().all(|u| u.author == intruder));
    // The manager's approval was valid (the manager is not the nominee).
    assert_eq!(after.manager_approval, Some(true));
}

#[test]
fn criteria_evaluation() {
    let mut t = T::new();
    let p = t.program;
    let mut n = 1000;
    t.commit(default_criteria(p, || {
        n += 1;
        Id::v7(n, [3; 10])
    }));
    let cs = criteria(&t.s, p);
    assert_eq!(cs.len(), 5);
    assert!(cs[0].eliminatory && cs[1].eliminatory);
    let yellow = t.person("Y", Some(BeltLevel::Yellow), None);
    let none = t.person("N", None, None);
    let members = programa::members(&t.s);
    let ctx = CriteriaContext {
        members: &members,
        open_targets: [(None, BeltLevel::Black)].into_iter().collect(),
    };
    let mut nom = Nomination {
        person: Some(yellow),
        level: BeltLevel::Black,
        tenure_months: Some(6.0),
        rating: Some(5.0),
        hours_available: Some(8.0),
        ..Nomination::default()
    };
    // Black belt needs green belt concluded: the yellow belt fails.
    let ev = evaluate(&nom, &cs, &ctx);
    assert!(!ev.passed);
    assert_eq!(
        ev.failures().next().unwrap().kind,
        CriterionKind::PriorLevel
    );
    // Tenure exactly at the threshold passes (limit included).
    nom.level = BeltLevel::Green;
    let ev = evaluate(&nom, &cs, &ctx);
    assert!(ev.passed);
    // Score: rating 5/5 weight 2, hours 8/8 weight 1, no green target open: 0, weight 1.
    assert!((ev.score - 75.0).abs() < 1e-9, "{}", ev.score);
    nom.person = Some(none);
    nom.tenure_months = Some(5.9);
    assert!(!evaluate(&nom, &cs, &ctx).passed);
}

#[test]
fn rank_proposal_waiting_list_and_promotion() {
    let mut f = fixture(5, 3);
    let p = f.t.program;
    let cs: Vec<Criterion> = Vec::new();
    let noms = nominations(&f.t.s, p);
    let class = classes(&f.t.s, p)[0].clone();
    let members = programa::members(&f.t.s);
    let ctx = CriteriaContext {
        members: &members,
        open_targets: Default::default(),
    };
    let ranked = rank(&class, &noms, &cs, &ctx, &[], &[]);
    assert_eq!(ranked.len(), 5);
    // Without capacity recorded, the nomination's 10 h hold the default project hours.
    assert!(ranked.iter().all(|r| r.capacity.project_hours_week > 0.0));
    let prop = proposal(&ranked, class.seats);
    assert_eq!(prop.selected.len() + prop.waiting.len() + prop.out.len(), 5);
    assert!(prop.selected.len() <= 3);
    // Same input, same ranking.
    assert_eq!(ranked, rank(&class, &noms, &cs, &ctx, &[], &[]));
    let me = f.t.me;
    f.t.commit(confirm_selection(class.id, &prop, me, d(2026, 9, 1)));
    let noms = nominations(&f.t.s, p);
    assert_eq!(students(&noms, class.id).len(), prop.selected.len());
    let wl = waiting_list(&noms, class.id);
    assert_eq!(wl.iter().map(|n| n.id).collect::<Vec<_>>(), prop.waiting);
    // A selected trainee withdraws: the first of the waiting list takes the seat.
    if let (Some(first_sel), Some(first_wait)) = (prop.selected.first(), prop.waiting.first()) {
        let n = noms.iter().find(|n| n.id == *first_sel).unwrap().clone();
        f.t.commit(withdraw(&n, "saiu da empresa").unwrap());
        let noms = nominations(&f.t.s, p);
        f.t.commit(promote(&class, &noms, me, d(2026, 9, 2)));
        let noms = nominations(&f.t.s, p);
        assert!(
            noms.iter()
                .any(|n| n.id == *first_wait && n.status == NominationStatus::Selected)
        );
    }
}

#[test]
fn capacity_check_uses_free_hours() {
    let f = fixture(1, 3);
    let p = f.t.program;
    let n = nominations(&f.t.s, p)[0].clone();
    let class = classes(&f.t.s, p)[0].clone();
    let load = Load {
        person: f.people[0],
        capacity: 6.0,
        ..Load::default()
    };
    let c = capacity_check(&n, &class, &[], Some(&load));
    assert!(!c.fits, "{c:?}");
    let c = capacity_check(&n, &class, &[], None);
    assert_eq!(c.free_hours, 10.0);
}

fn held_session(
    t: &mut T,
    class: Id,
    date: Date,
    hours: f64,
    module: Id,
    marks: &[(Id, Attendance)],
) -> Id {
    let s = t.id();
    let me = t.me;
    t.commit(new_session(class, s, date, hours, &[module], Some(me)));
    let e: Vec<Edit> = marks.iter().map(|(p, a)| mark(s, *p, *a)).collect();
    t.commit(e);
    s
}

#[test]
fn attendance_completion_and_learning_entries() {
    let mut f = fixture(2, 5);
    let p = f.t.program;
    let mut n = 5000;
    f.t.commit(curriculum::install_default(p, BeltLevel::Green, || {
        n += 1;
        Id::v7(n, [5; 10])
    }));
    let mods = curriculum::modules(&f.t.s, p);
    assert_eq!(
        mods.len(),
        curriculum::default_templates(BeltLevel::Green).len()
    );
    let measure = mods.iter().find(|m| m.name == "Medir").unwrap().clone();
    let (a, b) = (f.people[0], f.people[1]);
    let class = f.class;
    for (i, mark_b) in [
        Attendance::Present,
        Attendance::Justified,
        Attendance::Present,
        Attendance::Absent,
        Attendance::Present,
    ]
    .into_iter()
    .enumerate()
    {
        held_session(
            &mut f.t,
            class,
            d(2026, 11, 2 + i as u8),
            4.0,
            measure.id,
            &[(a, Attendance::Present), (b, mark_b)],
        );
    }
    let sessions = all_sessions(&f.t.s);
    let k = classes(&f.t.s, p)[0].clone();
    let ca = completion(&k, &sessions, a);
    assert!(ca.ok && (ca.rate - 1.0).abs() < 1e-12);
    // 3 of 5 sessions: 60 %, below 80 %; the justified hours are recorded apart.
    let cb = completion(&k, &sessions, b);
    assert!(!cb.ok);
    assert!((cb.rate - 0.6).abs() < 1e-12);
    assert_eq!(cb.hours_justified, 4.0);
    // Exactly at the minimum passes.
    let mut k80 = k.clone();
    k80.min_attendance = 0.6;
    assert!(completion(&k80, &sessions, b).ok);
    // `estudou` for the tools of Medir only for who attended every session.
    let le = learning_entries(&sessions, &mods);
    assert!(
        le.iter()
            .any(|e| e.person == Some(a) && e.tool == "msa" && e.level == SkillLevel::Studied)
    );
    assert!(!le.iter().any(|e| e.person == Some(b)));
    // An exercise done right: `praticou`.
    let s0 = sessions[0].id;
    f.t.commit(vec![mark_exercise(s0, b, true)]);
    let le = learning_entries(&all_sessions(&f.t.s), &mods);
    assert!(le.iter().any(|e| e.person == Some(b)
        && e.tool == "capacidade"
        && e.level == SkillLevel::Practiced));
    // The map takes them.
    let map = mentoria::competence_map(&le, &[], |_| false);
    assert_eq!(map.level(a, "msa"), Some(SkillLevel::Studied));
}

#[test]
fn curriculum_order_and_instructors() {
    let mut t = T::new();
    let p = t.program;
    let mut n = 7000;
    t.commit(curriculum::install_default(p, BeltLevel::Yellow, || {
        n += 1;
        Id::v7(n, [6; 10])
    }));
    let mods = curriculum::modules(&t.s, p);
    let class = t.id();
    t.commit(new_class(p, class, &class_input(BeltLevel::Yellow, None, 10)).unwrap());
    let teacher = t.person("Instrutora", None, None);
    // The second module before the first: an order problem.
    let s1 = t.id();
    t.commit(new_session(
        class,
        s1,
        d(2026, 11, 3),
        4.0,
        &[mods[1].id],
        Some(teacher),
    ));
    let s2 = t.id();
    t.commit(new_session(
        class,
        s2,
        d(2026, 11, 4),
        4.0,
        &[mods[0].id],
        Some(teacher),
    ));
    let ss = all_sessions(&t.s);
    let refs: Vec<&ClassSession> = ss.iter().collect();
    assert_eq!(
        curriculum::order_problems(&mods, &refs),
        vec![(mods[1].id, mods[0].id)]
    );
    let iid = t.id();
    t.commit(add_instructor(p, iid, teacher, &[mods[0].id]));
    let ins = instructors(&t.s, p);
    assert_eq!(unqualified(&ss, &ins), vec![(s1, mods[1].id)]);
    // Hours given count only held sessions.
    assert_eq!(
        hours_given(&ss, teacher, d(2026, 1, 1), d(2026, 12, 31)),
        0.0
    );
    t.commit(vec![mark(s1, Id::v7(1, [1; 10]), Attendance::Present)]);
    let ss = all_sessions(&t.s);
    assert_eq!(
        hours_given(&ss, teacher, d(2026, 1, 1), d(2026, 12, 31)),
        4.0
    );
    assert_eq!(agenda_of(&ss, teacher).len(), 2);
    assert!((curriculum::hours_of(&mods, BeltLevel::Yellow) - 16.0).abs() < 1e-9);
}

#[test]
fn certification_requests_for_the_committee() {
    let mut f = fixture(2, 5);
    let p = f.t.program;
    let class = classes(&f.t.s, p)[0].clone();
    let noms = nominations(&f.t.s, p);
    let prop = Proposal {
        selected: noms.iter().map(|n| n.id).collect(),
        ..Proposal::default()
    };
    let me = f.t.me;
    f.t.commit(confirm_selection(class.id, &prop, me, d(2026, 9, 1)));
    let noms = nominations(&f.t.s, p);
    let reqs = certification_requests(&noms, &[], &[]);
    assert_eq!(reqs.len(), 2);
    let assigned = Assignment {
        id: Id::NIL,
        opportunity: None,
        idea: None,
        person: Some(f.people[0]),
        hours_week: 4.0,
        mentor: None,
        position: None,
        reason: String::new(),
        date: None,
        by: None,
        ended: false,
    };
    assert_eq!(certification_requests(&noms, &[], &[assigned]).len(), 1);
}

// --- Teams ------------------------------------------------------------------------------------

fn trainee(i: u64, area: &str, unit: u64) -> teams::Trainee {
    teams::Trainee {
        id: Id::v7(i, [i as u8; 10]),
        unit: Some(Id::v7(10_000 + unit, [0; 10])),
        area: area.into(),
        function: "analista".into(),
        has_project: true,
        has_champion: true,
        avoid: Vec::new(),
    }
}

#[test]
fn teams_respect_area_cap_and_conflicts() {
    let mut ts: Vec<teams::Trainee> = (0..12)
        .map(|i| {
            trainee(
                i + 1,
                ["producao", "qualidade", "logistica"][i as usize % 3],
                i % 2,
            )
        })
        .collect();
    let (a, b) = (ts[0].id, ts[3].id);
    ts[0].avoid = vec![b];
    let rules = teams::Rules {
        group_size: 4,
        max_same_area: 2,
        need_project: true,
    };
    let s = teams::suggest(&ts, &rules);
    assert_eq!(s.groups.len(), 3);
    assert!(
        s.violations.iter().all(|v| !teams::is_placement(v)),
        "{:?}",
        s.violations
    );
    assert!(!s.groups.iter().any(|g| g.contains(&a) && g.contains(&b)));
    assert_eq!(teams::check(&s.groups, &ts, &rules), s.violations);
    // A trainee without project is flagged, not moved.
    ts[5].has_project = false;
    let s2 = teams::suggest(&ts, &rules);
    assert!(
        s2.violations
            .iter()
            .any(|v| matches!(v, teams::Violation::NoProject { .. }))
    );
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(64))]

    /// No hard constraint is violated when a solution exists (enough areas to mix), and
    /// the same input gives the same distribution.
    #[test]
    fn teams_are_feasible_and_deterministic(n in 2usize..30, size in 2usize..7, areas in 3usize..6) {
        let ts: Vec<teams::Trainee> = (0..n as u64)
            .map(|i| trainee(i + 1, &format!("area{}", i as usize % areas), i % 3))
            .collect();
        let rules = teams::Rules { group_size: size, max_same_area: 2, need_project: false };
        let s = teams::suggest(&ts, &rules);
        let (groups, _, max) = teams::shape(n, &rules);
        // Feasible when every group can hold its people with at most 2 per area.
        if max <= 2 * areas {
            prop_assert!(s.violations.iter().all(|v| !teams::is_placement(v)), "{:?}", s.violations);
        }
        prop_assert_eq!(s.groups.len(), groups);
        let mut all: Vec<Id> = s.groups.iter().flatten().copied().collect();
        all.sort();
        let mut ids: Vec<Id> = ts.iter().map(|t| t.id).collect();
        ids.sort();
        prop_assert_eq!(all, ids);
        prop_assert_eq!(&s, &teams::suggest(&ts, &rules));
        let mut rev = ts.clone();
        rev.reverse();
        prop_assert_eq!(s.groups, teams::suggest(&rev, &rules).groups);
    }
}

#[test]
fn groups_saved_moved_and_confirmed() {
    let mut t = T::new();
    let class = t.id();
    let (a, b, c) = (t.id(), t.id(), t.id());
    let mut n = 900;
    let e = save_groups(class, &[], &[vec![a, b], vec![c]], || {
        n += 1;
        Id::v7(n, [9; 10])
    });
    t.commit(e);
    let gs = groups(&t.s, class);
    assert_eq!(gs.len(), 2);
    assert_eq!(gs[0].name, "Grupo 1");
    let to = gs[1].id;
    t.commit(move_to_group(&gs, b, to));
    let gs = groups(&t.s, class);
    assert_eq!(gs[0].members, vec![a]);
    assert_eq!(gs[1].members, vec![c, b]);
    let me = t.me;
    t.commit(confirm_groups(&gs, me, d(2026, 9, 3)));
    assert!(
        groups(&t.s, class)
            .iter()
            .all(|g| g.confirmed_by == Some(me))
    );
}

// --- Exams ------------------------------------------------------------------------------------

fn bank_fixture() -> (T, Id, Vec<exams::Question>) {
    let mut t = T::new();
    let module = t.id();
    for d in 1..=3u8 {
        for i in 0..4 {
            let id = t.id();
            let input = QuestionInput {
                module: Some(module),
                kind: QuestionKind::Numeric,
                statement: format!("Questão {d}.{i}"),
                difficulty: d,
                number: Some(10.0 + i as f64),
                tolerance: 0.5,
                ..QuestionInput::default()
            };
            t.commit(exams::add_question(id, &input).unwrap());
        }
    }
    let id = t.id();
    t.commit(
        exams::add_question(
            id,
            &QuestionInput {
                module: Some(module),
                kind: QuestionKind::Practical,
                statement: "Eixos".into(),
                difficulty: 2,
                tolerance: 0.01,
                problem: Some(Problem::Cpk {
                    n: 30,
                    mean: 10.0,
                    sd: 0.2,
                    lsl: 9.4,
                    usl: 10.6,
                }),
                ..QuestionInput::default()
            },
        )
        .unwrap(),
    );
    let bank = exams::bank(&t.s);
    (t, module, bank)
}

fn exam_of(
    t: &mut T,
    bank: &[exams::Question],
    module: Id,
    seed: u64,
    retake_of: Option<Id>,
) -> exams::Exam {
    let id = t.id();
    let class = Id::v7(4242, [4; 10]);
    let p = t.program;
    let e = exams::new_exam(
        p,
        id,
        &ExamInput {
            class,
            name: "Prova GB".into(),
            modules: vec![module],
            counts: [2, 3, 1],
            seed,
            mode: Mode::App,
            min_grade: 0.7,
            date: Some(d(2026, 12, 1)),
            retake_of,
            counts_for_cert: true,
        },
        bank,
    )
    .unwrap();
    t.commit(e);
    exams::exams(&t.s).into_iter().find(|x| x.id == id).unwrap()
}

#[test]
fn same_seed_same_exam() {
    let (_, module, bank) = bank_fixture();
    let a = exams::draw(&bank, &[module], [2, 3, 1], 42).unwrap();
    assert_eq!(a, exams::draw(&bank, &[module], [2, 3, 1], 42).unwrap());
    assert_ne!(a, exams::draw(&bank, &[module], [2, 3, 1], 43).unwrap());
    assert_eq!(a.len(), 6);
    assert_eq!(
        exams::draw(&bank, &[module], [5, 0, 0], 1),
        Err(exams::ExamError::NotEnough {
            difficulty: 1,
            have: 4,
            need: 5
        })
    );
    assert_eq!(
        exams::draw(&bank, &[module], [0, 0, 0], 1),
        Err(exams::ExamError::Empty)
    );
}

#[test]
fn papers_carry_no_key_and_vary_per_trainee() {
    let (mut t, module, bank) = bank_fixture();
    // The practical question is drawn for sure: only 5 of difficulty 2, 5 asked.
    let ex = {
        let id = t.id();
        let p = t.program;
        t.commit(
            exams::new_exam(
                p,
                id,
                &ExamInput {
                    class: Id::v7(1, [1; 10]),
                    name: "P".into(),
                    modules: vec![module],
                    counts: [0, 5, 0],
                    seed: 7,
                    mode: Mode::App,
                    min_grade: 0.7,
                    date: None,
                    retake_of: None,
                    counts_for_cert: true,
                },
                &bank,
            )
            .unwrap(),
        );
        exams::exams(&t.s).into_iter().find(|x| x.id == id).unwrap()
    };
    assert_eq!(ex.seed, 7);
    let (a, b) = (Id::v7(11, [1; 10]), Id::v7(12, [2; 10]));
    let pa = exams::paper(&ex, &bank, a).unwrap();
    let pb = exams::paper(&ex, &bank, b).unwrap();
    assert_eq!(pa, exams::paper(&ex, &bank, a).unwrap());
    let da = &pa
        .items
        .iter()
        .find(|i| i.kind == QuestionKind::Practical)
        .unwrap()
        .data;
    let db = &pb
        .items
        .iter()
        .find(|i| i.kind == QuestionKind::Practical)
        .unwrap()
        .data;
    assert_eq!(da.len(), 30);
    assert_ne!(da, db);
    // Publishing writes the papers, never the key nor the recipe.
    let edits = exams::publish(&ex, &bank, &[a, b]).unwrap();
    let text = serde_json::to_string(
        &edits
            .iter()
            .map(|e| (&e.field, &e.value))
            .collect::<Vec<_>>(),
    )
    .unwrap();
    assert!(!text.contains(exams::fields::KEY));
    assert!(!text.contains("receita"));
    assert!(!text.contains("tolerancia"));
    let mut cs = T::new();
    cs.commit(edits);
    let pubs = exams::published(&cs.s);
    assert_eq!(pubs[0].papers.len(), 2);
    assert_eq!(pubs[0].papers[&a], pa);
}

#[test]
fn grading_with_tolerance_limits() {
    let (mut t, module, bank) = bank_fixture();
    let ex = exam_of(&mut t, &bank, module, 42, None);
    let me = Id::v7(31, [3; 10]);
    let paper = exams::paper(&ex, &bank, me).unwrap();
    let mut answers = BTreeMap::new();
    for it in &paper.items {
        let q = bank.iter().find(|q| q.id == it.question).unwrap();
        let exp = exams::expected(q, &it.data).as_f64().unwrap();
        // At the limit of the tolerance, as typed text with a decimal comma.
        let v = exp + q.tolerance;
        answers.insert(q.id, Value::from(format!("{v}").replace('.', ",")));
    }
    let g = exams::grade(&ex, &bank, me, &answers).unwrap();
    assert!(g.items.iter().all(|i| i.right), "{:?}", g.items);
    assert_eq!(g.score, 1.0);
    // Just beyond the tolerance: wrong.
    let first = paper.items[0].question;
    let q = bank.iter().find(|q| q.id == first).unwrap();
    let exp = exams::expected(q, &paper.items[0].data).as_f64().unwrap();
    answers.insert(first, Value::from(exp + q.tolerance + 1e-3));
    let g = exams::grade(&ex, &bank, me, &answers).unwrap();
    assert!(!g.items.iter().find(|i| i.question == first).unwrap().right);
    assert!((g.score - 5.0 / 6.0).abs() < 1e-12 && g.passed);
}

#[test]
fn practical_answer_is_the_crates_answer() {
    let p = Problem::Cpk {
        n: 30,
        mean: 10.0,
        sd: 0.2,
        lsl: 9.4,
        usl: 10.6,
    };
    let data = p.data(&mut exams::Rng::new(3));
    let groups: Vec<Vec<f64>> = data.iter().map(|x| vec![*x]).collect();
    let c = utsim_quality::capability::capability(
        &groups,
        &utsim_quality::capability::Spec {
            lsl: Some(9.4),
            usl: Some(10.6),
            target: None,
        },
        &utsim_quality::capability::Options::default(),
    )
    .unwrap();
    assert_eq!(p.expected(&data), Some(c.cpk.value));
    let m = Problem::Dpmo {
        defects: 25,
        units: 500,
        opportunities: 4,
    };
    assert_eq!(m.expected(&[25.0, 500.0, 4.0]), Some(12_500.0));
    // The generator is fixed: these first values never change.
    let mut r = exams::Rng::new(0);
    assert_eq!(r.next_u64(), 0xE220_A839_7B1D_CDAF);
}

#[test]
fn standing_with_one_retake() {
    let (mut t, module, bank) = bank_fixture();
    let ex = exam_of(&mut t, &bank, module, 1, None);
    let person = Id::v7(51, [5; 10]);
    let class = ex.class.unwrap();
    let me = t.me;
    let r1 = t.id();
    t.commit(exams::record_result(
        r1,
        &ex,
        person,
        0.5,
        &[],
        me,
        d(2026, 12, 2),
    ));
    let all = exams::exams(&t.s);
    let rs = exams::results(&t.s);
    let s = exams::standing(&all, &rs, class, person);
    assert!(!s.passed && s.may_retake && s.attempts == 1);
    let re = exam_of(&mut t, &bank, module, 2, Some(ex.id));
    let r2 = t.id();
    t.commit(exams::record_result(
        r2,
        &re,
        person,
        0.7,
        &[],
        me,
        d(2026, 12, 9),
    ));
    let all = exams::exams(&t.s);
    let rs = exams::results(&t.s);
    let s = exams::standing(&all, &rs, class, person);
    assert!(s.passed, "{s:?}");
    assert_eq!(s.evidence, Some(r2));
    assert_eq!(s.best, Some(0.7));
}

#[test]
fn question_validation() {
    let ok = QuestionInput {
        kind: QuestionKind::MultipleChoice,
        statement: "?".into(),
        options: vec!["a".into(), "b".into()],
        right_option: Some(1),
        difficulty: 1,
        ..QuestionInput::default()
    };
    assert!(exams::add_question(Id::NIL, &ok).is_ok());
    let mut bad = ok.clone();
    bad.right_option = Some(2);
    assert_eq!(
        exams::add_question(Id::NIL, &bad),
        Err(exams::ExamError::BadOptions)
    );
    bad.statement.clear();
    assert_eq!(
        exams::add_question(Id::NIL, &bad),
        Err(exams::ExamError::NoStatement)
    );
    let p = QuestionInput {
        kind: QuestionKind::Practical,
        statement: "x".into(),
        problem: Some(Problem::Cpk {
            n: 30,
            mean: 1.0,
            sd: 1.0,
            lsl: 2.0,
            usl: 1.0,
        }),
        ..QuestionInput::default()
    };
    assert!(matches!(
        exams::add_question(Id::NIL, &p),
        Err(exams::ExamError::BadProblem(_))
    ));
    assert_eq!(exams::parse_number(" 1.234,5 "), Some(1234.5));
    assert_eq!(exams::parse_number("1,25"), Some(1.25));
    assert_eq!(exams::parse_number("x"), None);
}

#[test]
fn class_folder_trainees_write_only_their_own_answers() {
    let staff = Id::v7(1, [1; 10]);
    let (a, b) = (Id::v7(2, [2; 10]), Id::v7(3, [3; 10]));
    let w = perfis::TrainingWriters {
        staff: vec![staff],
        students: vec![a, b],
    };
    let mut t = T::new();
    t.ops.clear();
    let exam = Id::v7(5, [5; 10]);
    let ans = Id::v7(6, [6; 10]);
    let mut m = BTreeMap::new();
    m.insert(Id::v7(7, [7; 10]), Value::from(1));
    let edits = exams::answer(ans, exam, a, &m, None);
    assert!(perfis::check_training(&t.s, a, &w, &edits).is_ok());
    assert!(perfis::check_training(&t.s, b, &w, &edits).is_err());
    t.commit_as(a, edits);
    // B tries to take A's paper; A hands in; then A tries to change it.
    t.commit_as(
        b,
        vec![Edit::new(
            exams::Answers::entity_ref(ans),
            fields::PERSON,
            b.to_string(),
        )],
    );
    t.commit_as(a, exams::answer(ans, exam, a, &m, Some(d(2026, 12, 1))));
    t.commit_as(
        a,
        vec![Edit::new(
            exams::Answers::entity_ref(ans),
            exams::fields::ANSWERS,
            Value::Null,
        )],
    );
    // Neither a trainee nor a stranger writes an exam.
    t.commit_as(
        a,
        vec![Edit::new(exams::Exam::entity_ref(exam), "nome", "forjada")],
    );
    let r = perfis::replay_training(&t.ops, &w);
    let got = exams::all_answers(&r.state);
    assert_eq!(got.len(), 1);
    assert_eq!(got[0].person, Some(a));
    assert_eq!(got[0].answers.len(), 1);
    assert_eq!(r.unauthorized.len(), 3);
    assert!(Profile::Instructor != Profile::Student);
}
