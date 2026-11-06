//! Exposure on known tracks and on an example model.

use std::collections::BTreeSet;
use std::path::Path;

use utsim_exposicao::{Move, Options, Track, analyze, measure};
use utsim_model::{Model, ZoneDef, ZoneKind};
use utsim_nav::Role;

fn track(name: &str, role: Role, moves: Vec<Move>) -> Track {
    Track {
        object: 0,
        name: name.into(),
        role,
        ppe: BTreeSet::new(),
        moves,
    }
}

fn empty_model() -> Model {
    Model::parse(r#"Model(name: "vazio", run: (stop_time: 20.0), objects: [], connections: [])"#)
        .unwrap()
}

#[test]
fn near_miss_count_is_stable_with_a_smaller_step() {
    // A person stands at the origin; a forklift passes from x = -10 to 10 at y = 1 in 20
    // time units, then back at y = 5 (never closer than 5 m).
    let person = track(
        "pessoa",
        Role::Person,
        vec![Move {
            t0: 0.0,
            t1: 40.0,
            p0: [0.0, 0.0],
            p1: [0.0, 0.0],
        }],
    );
    let forklift = track(
        "empilhadeira",
        Role::Forklift,
        vec![
            Move {
                t0: 0.0,
                t1: 20.0,
                p0: [-10.0, 1.0],
                p1: [10.0, 1.0],
            },
            Move {
                t0: 20.0,
                t1: 40.0,
                p0: [10.0, 5.0],
                p1: [-10.0, 5.0],
            },
        ],
    );
    let model = empty_model();
    let tracks = [person, forklift];
    let mut durations = Vec::new();
    for step in [0.1, 0.01, 0.001] {
        let opts = Options {
            step,
            ..Options::default()
        };
        let e = measure(&model, &tracks, 0, 0.0, 40.0, opts);
        assert_eq!(e.near_miss_count(), 1, "passo {step}");
        let n = &e.near_misses[0];
        assert!((n.min_distance - 1.0).abs() < 0.01);
        durations.push(n.duration);
    }
    // Closer than 1.5 m while |x| < sqrt(1.25): 2 * 1.118 = 2.236 time units.
    for d in durations {
        assert!((d - 2.236).abs() < 0.11, "{d}");
    }
}

#[test]
fn zone_time_and_entries_without_ppe() {
    let mut model = empty_model();
    model.zones.push(ZoneDef {
        name: "corredor".into(),
        kind: ZoneKind::Vehicle,
        polygon: vec![[0.0, 0.0], [10.0, 0.0], [10.0, 10.0], [0.0, 10.0]],
        level: Some(4),
        ppe: vec!["colete".into()],
    });
    let mut person = track(
        "pessoa",
        Role::Person,
        vec![
            Move {
                t0: 0.0,
                t1: 20.0,
                p0: [-5.0, 5.0],
                p1: [15.0, 5.0],
            },
            Move {
                t0: 20.0,
                t1: 40.0,
                p0: [15.0, 5.0],
                p1: [-5.0, 5.0],
            },
        ],
    );
    let e = measure(
        &model,
        std::slice::from_ref(&person),
        0,
        0.0,
        40.0,
        Options::default(),
    );
    let z = &e.zones[0];
    assert!((z.time - 20.0).abs() < 1e-9);
    assert_eq!(z.entries, 2);
    assert_eq!(z.entries_without_ppe, 2);
    assert_eq!(z.missing_ppe, vec!["colete".to_string()]);
    person.ppe.insert("colete".into());
    let e = measure(&model, &[person], 0, 0.0, 40.0, Options::default());
    assert_eq!(e.entries_without_ppe(), 0);
    // Heat map: the person-time adds up to the window.
    let total: f64 = e.heat.presence.iter().sum();
    assert!((total - 40.0).abs() < 1e-6);
}

#[test]
fn example_model_runs_and_repeats() {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/pessoas_carregando.ron");
    let mut model = Model::load(&path).unwrap();
    model.run.stop_time = model.run.stop_time.min(2000.0);
    model.run.warmup = 0.0;
    model.zones.push(ZoneDef {
        name: "tudo".into(),
        kind: ZoneKind::Other,
        polygon: vec![[-1e4, -1e4], [1e4, -1e4], [1e4, 1e4], [-1e4, 1e4]],
        level: None,
        ppe: Vec::new(),
    });
    let opts = Options {
        step: 1.0,
        cell: 2.0,
        ..Options::default()
    };
    let a = analyze(&model, 0, opts).unwrap();
    let b = analyze(&model, 0, opts).unwrap();
    assert_eq!(a, b);
    // Every person is inside the huge zone the whole window.
    for z in &a.zones {
        assert!(
            (z.time - (a.to - a.from)).abs() < 1e-6,
            "{}: {}",
            z.person,
            z.time
        );
    }
}

#[test]
fn crossing_example_has_vehicle_exposure_and_near_misses() {
    let path =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/cruzamento_empilhadeira.ron");
    let model = Model::load(&path).unwrap();
    let e = analyze(&model, 0, Options::default()).unwrap();
    let corredor = e
        .zones
        .iter()
        .find(|z| z.zone == "corredor_empilhadeira" && z.person == "operador")
        .unwrap();
    assert!(corredor.time > 0.0 && corredor.entries > 0);
    assert_eq!(corredor.entries_without_ppe, corredor.entries);
    assert_eq!(corredor.missing_ppe, vec!["colete".to_string()]);
    let ruido = e.zones.iter().find(|z| z.zone == "ruido_prensa").unwrap();
    // The operator serves the press, so wears its ear protection.
    assert_eq!(ruido.entries_without_ppe, 0);
    assert!(
        e.near_miss_count() > 0,
        "nenhum quase-conflito no cruzamento"
    );
    assert!(e.near_misses.iter().all(|n| n.min_distance < 1.5));
}

#[test]
fn report_always_has_the_legal_notice() {
    let path =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/cruzamento_empilhadeira.ron");
    let mut model = Model::load(&path).unwrap();
    model.run.replications = 2;
    let reps = utsim_exposicao::analyze_all(&model, Options::default()).unwrap();
    let html = utsim_exposicao::relatorio::html(&model, &reps);
    assert!(html.contains(utsim_exposicao::relatorio::AVISO_LEGAL));
    assert!(html.contains(utsim_exposicao::relatorio::AVISO_SIMULADO));
    let empty = utsim_exposicao::relatorio::html(&model, &[]);
    assert!(empty.contains(utsim_exposicao::relatorio::AVISO_LEGAL));
}
