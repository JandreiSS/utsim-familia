//! The pack of the line Saúde e hospital (`linhas/saude/`): it loads, the copy on disk is
//! the one compiled in, every ready model belongs to the line with every object from a
//! preset, the node data (equipment stops and PPE) lives in the node's own fields, and the
//! indicators run on each model.

use utsim_linhas::saude::{Meta, Saude};
use utsim_linhas::{Pack, SAUDE, find};
use utsim_model::{Model, ObjectKind};

fn repo(path: &str) -> String {
    format!("{}/../../{path}", env!("CARGO_MANIFEST_DIR"))
}

#[test]
fn the_pack_loads_and_matches_the_files() {
    let p = find(SAUDE).unwrap();
    assert_eq!(p.library.title(), "Saúde e hospital");
    assert!(!p.is_cross_cutting());
    let disk = Pack::load_dir(std::path::Path::new(&repo("linhas/saude"))).unwrap();
    assert_eq!(disk, p);
    assert_eq!(p.vocabulary.term("item"), "paciente");
    assert_eq!(p.vocabulary.term("Sink"), "alta");
    for id in [
        "chegada",
        "agenda",
        "recepcao",
        "espera",
        "triagem",
        "consultorio",
        "procedimento",
        "exame",
        "medicacao",
        "leitos",
        "alta",
        "medicos",
        "enfermagem",
        "medico",
        "enfermeiro",
    ] {
        assert!(p.preset(id).is_some(), "{id}");
    }
}

#[test]
fn every_ready_model_belongs_to_the_line_and_reports() {
    let p = find(SAUDE).unwrap();
    assert_eq!(p.library.models.len(), 3);
    for r in &p.library.models {
        let mut m = Model::load(repo(&r.file)).unwrap();
        m.validate().unwrap();
        assert_eq!(m.line.as_deref(), Some(SAUDE), "{}", r.file);
        assert_eq!(m.name, r.name);
        for o in &m.objects {
            assert!(p.preset_of(o).is_some(), "{}: {}", r.file, o.name);
            assert!(o.department.is_some(), "{}: {}", r.file, o.name);
            // Doctors and nurses carry their PPE in the node's own field.
            if matches!(o.kind, ObjectKind::Transporter { .. }) {
                assert!(!o.ppe.is_empty(), "{}: {}", r.file, o.name);
            }
        }
        m.run.replications = 1;
        let k = Saude::rodar(&m, Some(&p), Meta::default()).unwrap();
        let rep = &k.reps[0];
        assert!(rep.atendidos > 50, "{}: {}", r.file, rep.atendidos);
        assert!(!k.censo.is_empty());
        if r.file.contains("pronto_socorro") {
            // Five colours, the doctor and the nurse together in the procedure room, the
            // CT scanner's stops in its own node.
            assert_eq!(rep.por_cor.len(), 5);
            assert!(rep.espera.is_some_and(|w| w < 60.0), "{:?}", rep.espera);
            assert!(rep.no_alvo.is_some_and(|x| x > 0.8));
            assert!(m.objects.iter().any(|o| matches!(
                &o.kind,
                ObjectKind::Processor { also, .. } if !also.is_empty()
            )));
            assert!(
                m.objects
                    .iter()
                    .any(|o| o.preset.as_deref() == Some("saude/exame") && o.downtime.is_some())
            );
            assert!(!rep.leitos.is_empty());
        } else if r.file.contains("ambulatorio") {
            // Booked patients that miss the appointment, every patient sees a doctor.
            assert!(rep.faltas > 0);
            assert_eq!(rep.abandonos, 0);
            assert!(rep.consultas > 0);
        } else {
            // Beds only: no consultation, occupancy close to the target.
            assert_eq!(rep.consultas, 0);
            assert_eq!(rep.espera, None);
            for (u, o) in &rep.leitos {
                assert!(*o > 0.5 && *o < 1.0, "{u}: {o}");
            }
        }
    }
}
