//! The Lean Office pack: its twelve presets make valid objects, a model built only with
//! them runs and reads back without loss, and the ready model belongs to the line.

use std::collections::BTreeMap;

use utsim_edit::{Command, Document};
use utsim_linhas::{LEAN_OFFICE, Pack, Placement, Role, find};
use utsim_model::Model;

fn pack() -> Pack {
    find(LEAN_OFFICE).expect("Lean Office is built in")
}

fn repo(path: &str) -> String {
    format!("{}/../../{path}", env!("CARGO_MANIFEST_DIR"))
}

#[test]
fn twelve_presets_one_per_role() {
    let p = pack();
    assert_eq!(p.library.name, "Lean Office");
    assert_eq!(p.library.presets.len(), 12);
    for role in Role::ALL {
        assert!(p.library.presets.iter().any(|x| x.role == role), "{role:?}");
    }
    // The pack on disk is the one compiled in.
    let disk = Pack::load_dir(std::path::Path::new(&repo("linhas/lean_office"))).unwrap();
    assert_eq!(disk, p);
    assert_eq!(p.vocabulary.term("item"), "caso");
    assert_eq!(p.vocabulary.term("time_in_system"), "lead time");
}

fn place(doc: &mut Document, p: &Pack, preset: &str, name: &str, values: &[(&str, &str)], x: f64) {
    let at = Placement {
        preset,
        name: Some(name),
        values: values
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect::<BTreeMap<_, _>>(),
        position: [x, 0.0, 0.0],
        department: Some("Escritório"),
    };
    p.apply(doc, &at).unwrap();
}

#[test]
fn a_model_made_only_of_presets_runs() {
    let p = pack();
    let mut doc = Document::new("só predefinições");
    place(
        &mut doc,
        &p,
        "entrada",
        "pedidos",
        &[("intervalo", "Exp(mean: 20.0)")],
        0.0,
    );
    place(
        &mut doc,
        &p,
        "caixa_entrada",
        "caixa",
        &[("ordem", "Desc(\"prioridade\")")],
        1.0,
    );
    place(&mut doc, &p, "posto", "analise", &[("ca", "0.9")], 2.0);
    place(&mut doc, &p, "verificacao", "conferencia", &[], 3.0);
    place(&mut doc, &p, "espera_informacao", "cliente", &[], 3.0);
    place(&mut doc, &p, "aprovacao", "aprovacao", &[], 4.0);
    place(&mut doc, &p, "tramitacao", "malote", &[], 5.0);
    place(&mut doc, &p, "lote", "lote", &[("tamanho", "5")], 6.0);
    place(&mut doc, &p, "reuniao", "comite", &[], 8.0);
    place(&mut doc, &p, "conclusao", "fim", &[], 9.0);
    place(&mut doc, &p, "equipe", "equipe", &[], 2.0);
    place(&mut doc, &p, "pessoa", "aprovadores", &[], 4.0);
    let links = [
        ("pedidos", "caixa"),
        ("caixa", "analise"),
        ("analise", "conferencia"),
        ("conferencia", "aprovacao"),
        ("conferencia", "cliente"),
        ("cliente", "caixa"),
        ("aprovacao", "malote"),
        ("aprovacao", "caixa"),
        ("malote", "lote"),
        ("lote", "comite_pauta"),
        ("comite", "fim"),
        ("comite", "caixa"),
    ];
    let commands = links
        .iter()
        .map(|(a, b)| Command::Connect {
            from: a.to_string(),
            to: b.to_string(),
        })
        .collect();
    doc.apply(Command::Batch(commands)).unwrap();
    let m = doc.model();
    assert_eq!(m.objects.len(), 15);
    for o in &m.objects {
        let preset = p.preset_of(o).unwrap_or_else(|| panic!("{}", o.name));
        assert_eq!(o.department.as_deref(), Some("Escritório"));
        if o.name == "equipe_1" {
            assert_eq!(preset.id, "pessoa");
        }
    }
    m.validate().unwrap();
    let reports = m.run_replications().unwrap();
    let done = reports[0]
        .blocks
        .iter()
        .find(|b| b.name == "fim")
        .unwrap()
        .input;
    assert!(done > 0);
    let again = Model::parse(&doc.to_text()).unwrap();
    assert_eq!(&again, m);
}

#[test]
fn the_ready_model_belongs_to_the_line() {
    let p = pack();
    assert!(!p.library.models.is_empty());
    for r in &p.library.models {
        let m = Model::load(repo(&r.file)).unwrap();
        m.validate().unwrap();
        assert_eq!(m.line.as_deref(), Some(LEAN_OFFICE), "{}", r.file);
        assert_eq!(m.name, r.name);
        for o in &m.objects {
            assert!(p.preset_of(o).is_some(), "{}: {}", r.file, o.name);
            assert!(o.department.is_some(), "{}: {}", r.file, o.name);
        }
    }
}
