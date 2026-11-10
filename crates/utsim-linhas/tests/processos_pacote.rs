//! The pack of the line Processos contínuos e batelada (`linhas/processos/`): it loads, the copy
//! on disk is the one compiled in, every ready model belongs to the line with every object from
//! a preset (of the line or of the Quality pack), the node data (breakdowns, PPE, defects)
//! lives in the node's own fields, the presets place into a model that runs, and the
//! indicators run on each model.

use utsim_edit::Document;
use utsim_linhas::processos::{Meta, Processos};
use utsim_linhas::{PROCESSOS, Pack, Placement, find};
use utsim_model::{Model, ObjectKind};

fn repo(path: &str) -> String {
    format!("{}/../../{path}", env!("CARGO_MANIFEST_DIR"))
}

#[test]
fn the_pack_loads_and_matches_the_files() {
    let p = find(PROCESSOS).unwrap();
    assert_eq!(p.library.title(), "Processos contínuos e batelada");
    assert!(!p.is_cross_cutting());
    let disk = Pack::load_dir(std::path::Path::new(&repo("linhas/processos"))).unwrap();
    assert_eq!(disk, p);
    assert_eq!(p.vocabulary.term("item"), "lote");
    assert_eq!(p.vocabulary.term("setup"), "em limpeza");
    for id in [
        "lotes",
        "fila_lotes",
        "reator",
        "equipamento",
        "pulmao",
        "envase",
        "tanque_produto",
        "operadores",
        "operador",
    ] {
        assert!(p.preset(id).is_some(), "{id}");
    }
}

#[test]
fn every_preset_places_into_a_valid_model() {
    let p = find(PROCESSOS).unwrap();
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
    let p = find(PROCESSOS).unwrap();
    let qualidade = find(utsim_linhas::QUALIDADE).unwrap();
    assert_eq!(p.library.models.len(), 3);
    for r in &p.library.models {
        let mut m = Model::load(repo(&r.file)).unwrap();
        m.validate().unwrap();
        assert_eq!(m.line.as_deref(), Some(PROCESSOS), "{}", r.file);
        assert_eq!(m.name, r.name);
        for o in &m.objects {
            assert!(
                p.preset_of(o).is_some() || qualidade.preset_of(o).is_some(),
                "{}: {}",
                r.file,
                o.name
            );
            assert!(o.department.is_some(), "{}: {}", r.file, o.name);
            if matches!(o.kind, ObjectKind::Transporter { .. }) {
                assert!(!o.ppe.is_empty(), "{}: {}", r.file, o.name);
            }
        }
        // Every reactor has a recipe per product and cleans only when the product changes.
        assert!(m.objects.iter().any(|o| matches!(
            &o.kind,
            ObjectKind::Processor {
                recipe: Some(_),
                changeover: Some(_),
                ..
            }
        )));
        m.run.replications = 1;
        let k = Processos::rodar(&m, Some(&p), Meta::default()).unwrap();
        let rep = &k.reps[0];
        assert!(rep.lotes > 30, "{}: {}", r.file, rep.lotes);
        assert!(rep.volume_dia.is_some_and(|v| v > 0.0), "{}", r.file);
        assert!(
            rep.rendimento.is_some_and(|y| y > 0.9 && y < 1.0),
            "{}",
            r.file
        );
        assert!(
            rep.reatores
                .values()
                .any(|x| x.limpezas_dia.is_some_and(|l| l > 0.5)),
            "{}",
            r.file
        );
        if r.file.contains("xarope") {
            // The batches feed a product tank and a continuous filler of the fluid network.
            assert!(rep.tanques.contains_key("tanque_xarope"));
            assert!(rep.envasadas_dia.is_some_and(|u| u > 500.0));
        } else if r.file.contains("tintas") {
            // Defects in the node's own field, inspection and scrap from the Quality pack.
            assert!(m.objects.iter().any(|o| o.quality.is_some()));
            assert!(!rep.reatores.contains_key("laboratorio"));
            assert!(!rep.reatores.contains_key("ajuste_cor"));
            assert!(!rep.ocupacao.is_empty());
        } else {
            assert_eq!(rep.por_produto.len(), 3);
            assert!(m.objects.iter().any(|o| o.downtime.is_some()));
        }
    }
}
