//! The pack of the line Alimentação e food service (`linhas/alimentacao/`): it loads, the copy
//! on disk is the one compiled in, every ready model belongs to the line with every object
//! from a preset, the node data (breakdowns, PPE) lives in the node's own fields, the presets
//! place into a model that runs, and the indicators run on each model.

use utsim_edit::Document;
use utsim_linhas::alimentacao::{Alimentacao, Meta};
use utsim_linhas::{ALIMENTACAO, Pack, Placement, find};
use utsim_model::{Model, ObjectKind};

fn repo(path: &str) -> String {
    format!("{}/../../{path}", env!("CARGO_MANIFEST_DIR"))
}

#[test]
fn the_pack_loads_and_matches_the_files() {
    let p = find(ALIMENTACAO).unwrap();
    assert_eq!(p.library.title(), "Alimentação e food service");
    assert!(!p.is_cross_cutting());
    let disk = Pack::load_dir(std::path::Path::new(&repo("linhas/alimentacao"))).unwrap();
    assert_eq!(disk, p);
    assert_eq!(p.vocabulary.term("item"), "pedido");
    assert_eq!(p.vocabulary.term("Separator"), "comanda");
    for id in [
        "pedidos",
        "fila",
        "caixa",
        "comanda",
        "estacao",
        "montagem",
        "retirada",
        "mesas",
        "entrega",
        "equipe",
        "funcionario",
        "saida",
    ] {
        assert!(p.preset(id).is_some(), "{id}");
    }
}

#[test]
fn every_preset_places_into_a_valid_model() {
    let p = find(ALIMENTACAO).unwrap();
    for pr in &p.library.presets {
        let mut doc = Document::new("teste");
        let names = p
            .apply(
                &mut doc,
                &Placement {
                    preset: &pr.id,
                    name: None,
                    values: Default::default(),
                    position: [0.0, 0.0, 0.0],
                    department: None,
                },
            )
            .unwrap_or_else(|e| panic!("{}: {e}", pr.id));
        assert!(!names.is_empty(), "{}", pr.id);
    }
}

#[test]
fn every_ready_model_belongs_to_the_line_and_reports() {
    let p = find(ALIMENTACAO).unwrap();
    assert_eq!(p.library.models.len(), 3);
    for r in &p.library.models {
        let mut m = Model::load(repo(&r.file)).unwrap();
        m.validate().unwrap();
        assert_eq!(m.line.as_deref(), Some(ALIMENTACAO), "{}", r.file);
        assert_eq!(m.name, r.name);
        for o in &m.objects {
            assert!(p.preset_of(o).is_some(), "{}: {}", r.file, o.name);
            assert!(o.department.is_some(), "{}: {}", r.file, o.name);
            if matches!(o.kind, ObjectKind::Transporter { .. }) {
                assert!(!o.ppe.is_empty(), "{}: {}", r.file, o.name);
            }
        }
        m.run.replications = 1;
        let k = Alimentacao::rodar(&m, Some(&p), Meta::default()).unwrap();
        let rep = &k.reps[0];
        assert!(rep.pedidos_dia > 50.0, "{}: {}", r.file, rep.pedidos_dia);
        assert!(rep.tempo > 0.0, "{}", r.file);
        // Every model has a busy hour at lunch or dinner.
        let (hora, _) = rep.pico().unwrap();
        assert!((11..=21).contains(&hora), "{}: {hora}", r.file);
        if r.file.contains("lanchonete") {
            // The comanda splits the order between the stations and the assembly gathers it.
            assert!(rep.estacoes.contains_key("chapa"));
            assert!(rep.desistencias > 0);
            assert_eq!(rep.estacoes["chapa"].feitas, rep.estacoes["caixa"].feitas);
        } else if r.file.contains("restaurante") {
            let s = &rep.mesas["salao"];
            assert!(s.ocupacao > 0.3 && s.ocupacao < 1.0, "{}", s.ocupacao);
            assert!(s.giro_dia > 1.0);
            assert!(!rep.estacoes.contains_key("salao"));
        } else {
            // Food gets cold at the pickup counter at dinner.
            assert!(rep.descartes > 0);
            assert_eq!(rep.desistencias, 0);
            assert!(m.objects.iter().any(|o| o.downtime.is_some()));
        }
    }
}
