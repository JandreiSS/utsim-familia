//! The pack of the line Logística e armazém (`linhas/logistica/`): it loads, the copy on
//! disk is the one compiled in, every ready model belongs to the line with every object
//! from a preset, the node data (maintenance and PPE) lives in the node's own fields, and
//! the indicators run on each model.

use utsim_linhas::logistica::{Armazem, Meta};
use utsim_linhas::{LOGISTICA, Pack, find};
use utsim_model::{Model, ObjectKind};

fn repo(path: &str) -> String {
    format!("{}/../../{path}", env!("CARGO_MANIFEST_DIR"))
}

#[test]
fn the_pack_loads_and_matches_the_files() {
    let p = find(LOGISTICA).unwrap();
    assert_eq!(p.library.title(), "Logística e armazém");
    assert!(!p.is_cross_cutting());
    let disk = Pack::load_dir(std::path::Path::new(&repo("linhas/logistica"))).unwrap();
    assert_eq!(disk, p);
    assert_eq!(p.vocabulary.term("item"), "carga");
    assert_eq!(p.vocabulary.term("Rack"), "porta-paletes");
    for id in [
        "caminhoes",
        "patio",
        "doca",
        "porta_paletes",
        "pedidos",
        "onda",
        "linhas",
        "separacao",
        "consolidacao",
        "expedicao",
        "empilhadeiras",
        "separadores",
        "empilhadeira",
        "separador",
    ] {
        assert!(p.preset(id).is_some(), "{id}");
    }
}

#[test]
fn every_ready_model_belongs_to_the_line_and_reports() {
    let p = find(LOGISTICA).unwrap();
    assert_eq!(p.library.models.len(), 3);
    for r in &p.library.models {
        let mut m = Model::load(repo(&r.file)).unwrap();
        m.validate().unwrap();
        assert_eq!(m.line.as_deref(), Some(LOGISTICA), "{}", r.file);
        assert_eq!(m.name, r.name);
        for o in &m.objects {
            assert!(p.preset_of(o).is_some(), "{}: {}", r.file, o.name);
            assert!(o.department.is_some(), "{}: {}", r.file, o.name);
            // Forklifts and pickers carry their PPE in the node's own field.
            if matches!(o.kind, ObjectKind::Transporter { .. }) {
                assert!(!o.ppe.is_empty(), "{}: {}", r.file, o.name);
            }
        }
        m.run.replications = 1;
        let k = Armazem::rodar(&m, Some(&p), Meta::default()).unwrap();
        let rep = &k.reps[0];
        if r.file.contains("ondas") {
            // Orders only: lines picked, orders consolidated with all their lines.
            assert_eq!(rep.caminhoes, 0);
            assert!(rep.pedidos > 1000, "{}", rep.pedidos);
            assert!(rep.linhas_por_pedido > 1.5);
            assert!(rep.no_prazo.is_some_and(|x| x > 0.5 && x < 1.0));
            assert!(k.linhas_por_hora.len() >= 8);
        } else {
            // Trucks at the docks: every one released, the pallets add up.
            assert!(rep.caminhoes >= 50, "{}: {}", r.file, rep.caminhoes);
            assert!(rep.paletes >= 20 * rep.caminhoes, "{}", r.file);
            assert!(
                rep.doca > 10.0 && rep.doca < 60.0,
                "{}: {}",
                r.file,
                rep.doca
            );
            assert!(!rep.docas.is_empty());
            // Forklifts break down: the maintenance lives in the node.
            assert!(
                m.objects
                    .iter()
                    .any(|o| o.preset.as_deref() == Some("logistica/empilhadeira")
                        && o.downtime.is_some()),
                "{}",
                r.file
            );
        }
        if r.file.contains("cd_") {
            // ABC: class A is the fastest to reach.
            assert!(rep.acesso["A"] < rep.acesso["B"]);
            assert!(rep.acesso["B"] < rep.acesso["C"]);
            assert!(rep.pedidos > 0);
        }
    }
}
