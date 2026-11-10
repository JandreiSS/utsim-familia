//! The pack of the line Manufatura discreta (`linhas/manufatura/`): it loads, the copy on disk
//! is the one compiled in, every ready model belongs to the line with every object from a
//! preset (of the line or of the Quality pack), the node data (breakdowns, PPE, defects)
//! lives in the node's own fields, the presets place into a model that runs, and the
//! indicators run on each model.

use utsim_edit::Document;
use utsim_linhas::manufatura::{Manufatura, Meta};
use utsim_linhas::{MANUFATURA, Pack, Placement, find};
use utsim_model::{Model, ObjectKind};

fn repo(path: &str) -> String {
    format!("{}/../../{path}", env!("CARGO_MANIFEST_DIR"))
}

#[test]
fn the_pack_loads_and_matches_the_files() {
    let p = find(MANUFATURA).unwrap();
    assert_eq!(p.library.title(), "Manufatura discreta");
    assert!(!p.is_cross_cutting());
    let disk = Pack::load_dir(std::path::Path::new(&repo("linhas/manufatura"))).unwrap();
    assert_eq!(disk, p);
    assert_eq!(p.vocabulary.term("item"), "ordem");
    assert_eq!(p.vocabulary.term("Sink"), "expedição");
    for id in [
        "liberacao",
        "fila_ordens",
        "maquina",
        "maquina_troca",
        "montagem",
        "supermercado",
        "cartoes",
        "expedicao",
        "operadores",
        "operador",
    ] {
        assert!(p.preset(id).is_some(), "{id}");
    }
}

#[test]
fn every_preset_places_into_a_valid_model() {
    let p = find(MANUFATURA).unwrap();
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
    let p = find(MANUFATURA).unwrap();
    let qualidade = find(utsim_linhas::QUALIDADE).unwrap();
    assert_eq!(p.library.models.len(), 3);
    for r in &p.library.models {
        let mut m = Model::load(repo(&r.file)).unwrap();
        m.validate().unwrap();
        assert_eq!(m.line.as_deref(), Some(MANUFATURA), "{}", r.file);
        assert_eq!(m.name, r.name);
        for o in &m.objects {
            assert!(
                p.preset_of(o).is_some() || qualidade.preset_of(o).is_some(),
                "{}: {}",
                r.file,
                o.name
            );
            assert!(o.department.is_some(), "{}: {}", r.file, o.name);
            // Operators carry their PPE in the node's own field.
            if matches!(o.kind, ObjectKind::Transporter { .. }) {
                assert!(!o.ppe.is_empty(), "{}: {}", r.file, o.name);
            }
        }
        m.run.replications = 1;
        let k = Manufatura::rodar(&m, Some(&p), Meta::default()).unwrap();
        let rep = &k.reps[0];
        assert!(rep.ordens > 50, "{}: {}", r.file, rep.ordens);
        assert!(rep.no_prazo.is_some(), "{}", r.file);
        assert!(!rep.maquinas.is_empty(), "{}", r.file);
        for (n, o) in &rep.maquinas {
            assert!(o.oee > 0.0 && o.oee <= 1.0, "{}: {n} {}", r.file, o.oee);
        }
        if r.file.contains("usinagem") {
            // Changeover only when the product changes, from the matrix; breakdowns in the
            // node's own field.
            assert!(rep.maquinas["torno"].trocas_dia.is_some_and(|t| t > 10.0));
            assert!(rep.maquinas["torno"].em_troca > 0.05);
            assert!(m.objects.iter().any(|o| o.downtime.is_some()));
            assert_eq!(rep.por_produto.len(), 3);
        } else if r.file.contains("conwip") {
            // The cards are not orders nor machines; the line runs near the bottleneck.
            assert!(!rep.maquinas.keys().any(|n| n.starts_with("conwip")));
            assert!(rep.por_dia > 200.0, "{}", rep.por_dia);
        } else {
            // Defects in the node's own field, inspection and scrap from the Quality pack.
            assert!(m.objects.iter().any(|o| o.quality.is_some()));
            assert!(rep.refugo > 0);
            assert!(!rep.maquinas.contains_key("inspecao"));
            assert!(!rep.ocupacao.is_empty());
        }
    }
}
