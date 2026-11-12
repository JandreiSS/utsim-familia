//! Who the person is: sex, height, body mass, age, experience and measured capacity.
//! Records the kernel never reads; the 3D body uses sex, height and mass, and the
//! measured load capacity enters the model warnings.

use utsim_core::{Capacity, Sex};
use utsim_model::Model;

fn model(person: &str, weight: f64) -> String {
    format!(
        r#"Model(
            name: "registro",
            run: (stop_time: 600.0),
            objects: [
                (name: "s", kind: Source(interarrival: Const(60.0),
                    labels: {{"weight": Const({weight:?})}}), position: (0.0, 0.0, 0.0)),
                (name: "q", kind: Queue(), transport: "ana", position: (1.0, 0.0, 0.0)),
                (name: "out", kind: Sink, position: (10.0, 0.0, 0.0)),
                (name: "ana", kind: Transporter(speed: 1.2), position: (1.0, 1.0, 0.0),
                    person: Some({person})),
            ],
            connections: [("s", "q"), ("q", "out")],
        )"#
    )
}

fn warnings(person: &str, weight: f64) -> Vec<String> {
    let m = Model::parse(&model(person, weight)).expect("model parses");
    m.validate().expect("model is valid");
    m.warnings()
}

#[test]
fn the_fields_read_in_portuguese_and_write_in_english() {
    let pt = r#"(sexo: Some(Feminino), altura_m: Some(1.62), massa_kg: Some(58.0), idade: Some(34),
        experiencia_anos: Some(6.0), capacidade: Some((carga_kg: Some(18.0))))"#;
    let m = Model::parse(&model(pt, 5.0)).unwrap();
    m.validate().unwrap();
    let p = m.objects.iter().find(|o| o.name == "ana").unwrap();
    let p = p.person.as_deref().unwrap();
    assert_eq!(p.sex, Some(Sex::Female));
    assert_eq!(p.height_m, Some(1.62));
    assert_eq!(p.mass_kg, Some(58.0));
    assert_eq!(p.age, Some(34));
    assert_eq!(p.experience_years, Some(6.0));
    assert_eq!(
        p.capacity,
        Some(Capacity {
            load_kg: Some(18.0),
            force_n: None
        })
    );
    let text = ron::to_string(p).unwrap();
    assert!(text.contains("sex:Some(Female)"), "{text}");
    assert!(text.contains("height_m:Some(1.62)"), "{text}");
    // Absent fields are not written: an old profile stays as it was.
    let old = ron::to_string(&utsim_core::Person::default()).unwrap();
    assert!(!old.contains("sex") && !old.contains("capacity"), "{old}");
    // And the summary of `utsim check` shows them.
    let lines = m.person_lines().join("\n");
    assert!(
        lines.contains(
            "sexo feminino, 1.62 m, 58 kg, 34 anos, 6 anos de experiência, capacidade medida 18 kg"
        ),
        "{lines}"
    );
}

#[test]
fn records_do_not_change_the_run() {
    let plain = Model::parse(&model("()", 5.0)).unwrap();
    let with = Model::parse(&model(
        "(sex: Some(Male), height_m: Some(1.80), mass_kg: Some(80.0), age: Some(50))",
        5.0,
    ))
    .unwrap();
    let a = plain.run_replications().unwrap().remove(0);
    let b = with.run_replications().unwrap().remove(0);
    assert_eq!(a.block("out").unwrap().input, b.block("out").unwrap().input);
    assert_eq!(
        a.block("ana").unwrap().states,
        b.block("ana").unwrap().states
    );
}

#[test]
fn invalid_values_are_errors() {
    for (person, field) in [
        ("(height_m: Some(0.0))", "height_m"),
        ("(mass_kg: Some(-70.0))", "mass_kg"),
        ("(age: Some(130))", "age"),
        ("(experience_years: Some(-1.0))", "experience_years"),
        ("(capacity: Some((load_kg: Some(0.0))))", "capacity.load_kg"),
        (
            "(capacity: Some((force_n: Some(-5.0))))",
            "capacity.force_n",
        ),
    ] {
        let e = Model::parse(&model(person, 5.0))
            .unwrap()
            .validate()
            .unwrap_err()
            .to_string();
        assert!(e.contains(&format!("ana.person.{field}")), "{person}: {e}");
    }
}

#[test]
fn heights_and_masses_out_of_range_are_warned() {
    // 162 is the height in centimetres; 150 kg is fine, 330 is probably pounds.
    let w = warnings("(height_m: Some(162.0), mass_kg: Some(330.0))", 5.0);
    assert!(
        w.iter().any(|x| x.contains("altura 162 m fora da faixa")),
        "{w:?}"
    );
    assert!(
        w.iter().any(|x| x.contains("massa 330 kg fora da faixa")),
        "{w:?}"
    );
    let w = warnings("(height_m: Some(1.95), mass_kg: Some(150.0))", 5.0);
    assert!(w.is_empty(), "{w:?}");
}

#[test]
fn a_woman_carrying_above_20_kg_is_warned_by_clt_390() {
    let w = warnings("(sex: Some(Female))", 22.0);
    assert!(w.iter().any(|x| x.contains("art. 390")), "{w:?}");
    assert!(
        !warnings("(sex: Some(Male))", 22.0)
            .iter()
            .any(|x| x.contains("art. 390"))
    );
    assert!(!warnings("()", 22.0).iter().any(|x| x.contains("art. 390")));
    assert!(
        !warnings("(sex: Some(Female))", 18.0)
            .iter()
            .any(|x| x.contains("art. 390"))
    );
}

#[test]
fn a_load_above_the_measured_capacity_is_warned() {
    let person = "(capacity: Some((load_kg: Some(18.0))))";
    let w = warnings(person, 20.0);
    assert!(
        w.iter()
            .any(|x| x == "ana: carrega até 20 kg, acima da capacidade medida dessa pessoa (18 kg)"),
        "{w:?}"
    );
    assert!(
        !warnings(person, 15.0)
            .iter()
            .any(|x| x.contains("capacidade medida")),
    );
}
