//! Comando que troca um campo entre "distribuição" e "dados reais", com desfazer.

use utsim_core::Dist;
use utsim_core::rng::stream;
use utsim_edit::{Command, Document, FieldSource};
use utsim_model::Model;
use utsim_model::dados::{self, DataInput, DataStatus};

const MODEL: &str = r#"Model(
    name: "m",
    run: (stop_time: 100.0),
    objects: [
        (name: "chegada", kind: Source(interarrival: Exp(mean: 1.0))),
        (name: "torno", kind: Processor(process: Const(1.0))),
        (name: "saida", kind: Sink),
    ],
    connections: [("chegada", "torno"), ("torno", "saida")],
)"#;

fn values() -> Vec<f64> {
    let mut rng = stream(31, 0, 0);
    (0..120)
        .map(|_| {
            Dist::Gamma {
                shape: 2.0,
                scale: 1.5,
            }
            .sample(&mut rng)
        })
        .collect()
}

fn doc(name: &str) -> (Document, std::path::PathBuf) {
    let dir = std::env::temp_dir().join(format!("utsim_edit_dados_{name}_{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("m.ron");
    std::fs::write(&path, MODEL).unwrap();
    (Document::open(&path).unwrap(), dir)
}

fn input() -> Box<DataInput> {
    let mut i = DataInput::with_values("", values());
    i.bootstrap = Some(49);
    Box::new(i)
}

#[test]
fn switch_to_data_and_back_with_undo() {
    let (mut d, _dir) = doc("ida_e_volta");
    let original = d.model().clone();
    d.apply(Command::SetFieldSource {
        field: "torno.process".into(),
        source: FieldSource::Data(input()),
    })
    .unwrap();
    let m = d.model();
    assert_eq!(m.data.len(), 1);
    assert_eq!(m.data[0].field, "torno.process");
    assert_eq!(m.data[0].status, DataStatus::Fitted);
    let fitted = dados::get_dist(m, "torno.process").unwrap().unwrap();
    assert_ne!(fitted, Dist::Const(1.0));
    assert_eq!(&fitted, &m.data[0].fitted.as_ref().unwrap().dist);

    // The analysis for the interface gives the same choice and a report.
    let fit = d.data_analysis("torno.process").unwrap();
    assert_eq!(fit.dist, fitted);
    assert!(fit.to_html("editor").contains("Escolha para o campo"));

    // Saved and reopened: same distribution, the record is reused.
    d.save().unwrap();
    let path = d.path().unwrap().to_path_buf();
    let reopened = Document::open(&path).unwrap();
    assert_eq!(reopened.model().data[0].status, DataStatus::Reused);
    assert_eq!(
        dados::get_dist(reopened.model(), "torno.process").unwrap(),
        Some(fitted.clone())
    );
    assert_eq!(Model::load(&path).unwrap(), *reopened.model());

    // Back to a distribution.
    d.apply(Command::SetFieldSource {
        field: "torno.process".into(),
        source: FieldSource::Dist(Dist::Exp { mean: 3.0 }),
    })
    .unwrap();
    assert!(d.model().data.is_empty());
    assert_eq!(
        dados::get_dist(d.model(), "torno.process").unwrap(),
        Some(Dist::Exp { mean: 3.0 })
    );

    assert!(d.undo());
    assert_eq!(d.model().data.len(), 1);
    assert_eq!(
        dados::get_dist(d.model(), "torno.process").unwrap(),
        Some(fitted)
    );
    assert!(d.undo());
    assert_eq!(d.model().data, original.data);
    assert_eq!(
        dados::get_dist(d.model(), "torno.process").unwrap(),
        Some(Dist::Const(1.0))
    );
    assert!(d.redo());
    assert_eq!(d.model().data.len(), 1);
}

#[test]
fn failures_change_nothing_and_renames_follow() {
    let (mut d, _dir) = doc("falhas");
    let before = d.model().clone();
    for (field, source) in [
        ("torno.capacity", FieldSource::Data(input())),
        ("fresa.process", FieldSource::Data(input())),
        (
            "torno.process",
            FieldSource::Data(Box::new(DataInput::with_source("", "nao_existe.csv", None))),
        ),
    ] {
        assert!(
            d.apply(Command::SetFieldSource {
                field: field.into(),
                source
            })
            .is_err()
        );
        assert_eq!(d.model(), &before);
    }
    assert!(!d.can_undo());

    d.apply(Command::SetFieldSource {
        field: "chegada.interarrival".into(),
        source: FieldSource::Data(input()),
    })
    .unwrap();
    d.apply(Command::Rename {
        from: "chegada".into(),
        to: "entrada".into(),
    })
    .unwrap();
    assert_eq!(d.model().data[0].field, "entrada.interarrival");
    d.apply(Command::Remove {
        name: "entrada".into(),
    })
    .unwrap();
    assert!(d.model().data.is_empty());
}
