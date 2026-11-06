//! The cross-cutting packs Quality and Safety: every preset places valid objects that
//! serialise and read back, an inspection wired to three exits validates, the ready
//! models validate, and a model stays readable without the packs.

use std::collections::BTreeMap;

use utsim_edit::{Command, Document};
use utsim_linhas::{LEAN_OFFICE, Pack, Placement, QUALIDADE, SEGURANCA, active, builtin, find};
use utsim_model::Model;
use utsim_model::quality::{ZoneDef, ZoneKind};

fn pack(id: &str) -> Pack {
    find(id).unwrap_or_else(|| panic!("{id} is built in"))
}

fn repo(path: &str) -> String {
    format!("{}/../../{path}", env!("CARGO_MANIFEST_DIR"))
}

fn place(doc: &mut Document, p: &Pack, preset: &str, name: &str, x: f64) -> Vec<String> {
    p.apply(
        doc,
        &Placement {
            preset,
            name: Some(name),
            values: BTreeMap::new(),
            position: [x, 0.0, 0.0],
            department: None,
        },
    )
    .unwrap()
}

#[test]
fn packs_are_cross_cutting_and_on_disk() {
    for id in [QUALIDADE, SEGURANCA] {
        let p = pack(id);
        assert!(p.is_cross_cutting(), "{id}");
        let disk = Pack::load_dir(std::path::Path::new(&repo(&format!("linhas/{id}")))).unwrap();
        assert_eq!(disk, p);
    }
    assert!(!pack(LEAN_OFFICE).is_cross_cutting());
    assert_eq!(pack(QUALIDADE).library.presets.len(), 6);
    assert_eq!(
        pack(QUALIDADE).library.title(),
        "Qualidade (pacote transversal)"
    );
    assert!(builtin().len() >= 3);
}

#[test]
fn every_preset_places_an_object_that_reads_back() {
    for id in [QUALIDADE, SEGURANCA] {
        let p = pack(id);
        for preset in &p.library.presets {
            let mut doc = Document::new("m");
            let names = place(&mut doc, &p, &preset.id, "x", 0.0);
            assert_eq!(names[0], "x");
            let m = doc.model();
            assert_eq!(m.packs, [id], "{id}/{}", preset.id);
            assert_eq!(m.line, None, "a cross-cutting pack is never the line");
            assert_eq!(
                m.objects[0].preset.as_deref(),
                Some(format!("{id}/{}", preset.id).as_str())
            );
            let again = Model::parse(&doc.to_text()).unwrap();
            assert_eq!(&again, m, "{id}/{}", preset.id);
        }
    }
}

#[test]
fn inspection_with_three_exits_validates() {
    let q = pack(QUALIDADE);
    let mut doc = Document::new("inspeção");
    place(&mut doc, &q, "processo_defeito", "torno", 0.0);
    place(&mut doc, &q, "estacao_inspecao", "inspecao", 4.0);
    place(&mut doc, &q, "cliente", "cliente", 8.0);
    place(&mut doc, &q, "retrabalho", "retrabalho", 4.0);
    place(&mut doc, &q, "refugo", "refugo", 8.0);
    doc.apply(Command::Add(Box::new(
        utsim_edit::preset::parse_object(
            r#"(name: "chegada", kind: Source(interarrival: Exp(mean: 2.0)))"#,
        )
        .unwrap(),
    )))
    .unwrap();
    doc.apply(Command::Add(Box::new(
        utsim_edit::preset::parse_object(r#"(name: "fila", kind: Queue())"#).unwrap(),
    )))
    .unwrap();
    for (a, b) in [
        ("chegada", "torno"),
        ("torno", "fila"),
        ("fila", "inspecao"),
        ("inspecao", "cliente"),
        ("inspecao", "retrabalho"),
        ("inspecao", "refugo"),
        ("retrabalho", "fila"),
    ] {
        doc.apply(Command::Connect {
            from: a.into(),
            to: b.into(),
        })
        .unwrap();
    }
    doc.model().validate().unwrap();
    assert_eq!(doc.model().packs, ["qualidade"]);
    assert!(doc.model().run_replications().is_ok());
    Model::parse(&doc.to_text()).unwrap().validate().unwrap();
}

#[test]
fn measurement_inspection_and_safety_posts_validate() {
    let q = pack(QUALIDADE);
    let s = pack(SEGURANCA);
    let mut doc = Document::new("misto");
    place(&mut doc, &s, "posto_ruidoso", "prensa", 0.0);
    place(&mut doc, &s, "posto_solda", "solda", 2.0);
    place(&mut doc, &s, "doca_carga", "doca", 4.0);
    place(&mut doc, &q, "inspecao_medicao", "medicao", 6.0);
    let m = doc.model();
    assert_eq!(m.packs, ["seguranca", "qualidade"]);
    assert!(m.objects[0].ppe.contains(&"protetor_auricular".to_string()));
    assert_eq!(m.objects[2].ppe, ["capacete", "botina", "colete"]);
}

#[test]
fn zone_command_adds_a_zone_once() {
    let mut doc = Document::new("zonas");
    let zone = ZoneDef {
        name: "corredor".into(),
        kind: ZoneKind::Vehicle,
        polygon: vec![[0.0, 0.0], [10.0, 0.0], [10.0, 3.0]],
        level: Some(4),
        ppe: vec!["colete".into()],
    };
    doc.apply(Command::AddZone(zone.clone())).unwrap();
    assert!(doc.apply(Command::AddZone(zone)).is_err());
    let again = Model::parse(&doc.to_text()).unwrap();
    assert_eq!(again.zones.len(), 1);
}

#[test]
fn active_packs_are_line_plus_packs() {
    let m = Model::load(repo("examples/aprovacao_de_compras.ron")).unwrap();
    let ids: Vec<String> = active(&m).iter().map(|p| p.id().to_string()).collect();
    assert_eq!(ids, ["lean_office"]);

    let mut m = m;
    m.packs = vec![
        "qualidade".into(),
        "seguranca".into(),
        "qualidade".into(),
        "nada".into(),
    ];
    let ids: Vec<String> = active(&m).iter().map(|p| p.id().to_string()).collect();
    assert_eq!(ids, ["lean_office", "qualidade", "seguranca"]);
}

#[test]
fn ready_models_validate_and_do_not_need_the_packs() {
    for id in [QUALIDADE, SEGURANCA] {
        let p = pack(id);
        assert!(!p.library.models.is_empty());
        for r in &p.library.models {
            let m = Model::load(repo(&r.file)).unwrap_or_else(|e| panic!("{}: {e}", r.file));
            m.validate().unwrap();
            assert!(m.packs.iter().any(|x| x == id), "{}", r.file);
            // Plain `.ron`: without the `packs` line it is the same model.
            let text = std::fs::read_to_string(repo(&r.file)).unwrap();
            let bare: String = text
                .lines()
                .filter(|l| !l.trim_start().starts_with("packs:"))
                .collect::<Vec<_>>()
                .join(
                    "
",
                );
            let again = Model::parse(&bare).unwrap();
            again.validate().unwrap();
            assert!(again.packs.is_empty());
            assert_eq!(again.objects, m.objects);
            assert!(m.run_replications().is_ok());
        }
    }
}
