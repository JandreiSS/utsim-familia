//! The pack of the line Serviços e atendimento (`linhas/servicos/`): it loads, the copy
//! on disk is the one compiled in, every ready model belongs to the line with every object
//! from a preset, and the indicators run on each of them.

use utsim_linhas::servicos::{Atendimento, Meta};
use utsim_linhas::{Pack, SERVICOS, find};
use utsim_model::Model;

fn repo(path: &str) -> String {
    format!("{}/../../{path}", env!("CARGO_MANIFEST_DIR"))
}

#[test]
fn the_pack_loads_and_matches_the_files() {
    let p = find(SERVICOS).unwrap();
    assert_eq!(p.library.title(), "Serviços e atendimento");
    assert!(!p.is_cross_cutting());
    let disk = Pack::load_dir(std::path::Path::new(&repo("linhas/servicos"))).unwrap();
    assert_eq!(disk, p);
    assert_eq!(p.vocabulary.term("item"), "cliente");
    assert_eq!(p.vocabulary.term("Queue"), "fila de espera");
    for id in [
        "chegada",
        "fila",
        "guiche",
        "canal",
        "equipe",
        "atendente",
        "saida",
    ] {
        assert!(p.preset(id).is_some(), "{id}");
    }
}

#[test]
fn every_ready_model_belongs_to_the_line_and_reports() {
    let p = find(SERVICOS).unwrap();
    assert_eq!(p.library.models.len(), 3);
    for r in &p.library.models {
        let mut m = Model::load(repo(&r.file)).unwrap();
        m.validate().unwrap();
        assert_eq!(m.line.as_deref(), Some(SERVICOS), "{}", r.file);
        assert_eq!(m.name, r.name);
        for o in &m.objects {
            assert!(p.preset_of(o).is_some(), "{}: {}", r.file, o.name);
            assert!(o.department.is_some(), "{}: {}", r.file, o.name);
        }
        // A short run: the indicators come out and the customers add up.
        m.run.replications = 1;
        m.run.stop_time = m.run.warmup + 10080.0;
        let k = Atendimento::rodar(&m, Some(&p), Meta::default()).unwrap();
        let rep = &k.reps[0];
        assert!(rep.atendidos > 100, "{}: {}", r.file, rep.atendidos);
        assert!((0.0..=1.0).contains(&rep.nivel));
        assert!(!k.painel.is_empty(), "{}", r.file);
        assert!(!k.horas.is_empty(), "{}", r.file);
        let by_type: usize = rep.por_tipo.values().map(|g| g.atendidos).sum();
        assert_eq!(by_type, rep.atendidos, "{}", r.file);
    }
}
