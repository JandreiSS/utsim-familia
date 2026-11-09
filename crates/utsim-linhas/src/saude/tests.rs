//! Indicators of Saúde e hospital on small cases with answers known by hand.

use super::*;

fn run(text: &str, meta: Meta) -> Saude {
    let m = Model::parse(text).unwrap();
    Saude::rodar(&m, None, meta).unwrap()
}

fn near(what: &str, got: f64, want: f64) {
    assert!((got - want).abs() < 1e-9, "{what}: {got}, esperado {want}");
}

/// Three patients: A (green) and C (blue) at t = 0, B (orange) at t = 1. Classification 2
/// minutes each, in arrival order; one consulting room of 10 minutes; the wait calls the
/// most serious first.
const PS: &str = r#"Model(name: "ps", run: (stop_time: 100.0), objects: [
    (name: "a", kind: Source(interarrival: Const(1000.0), first_at_zero: true, max_arrivals: 1,
        labels: {"gravidade": Const(4.0)})),
    (name: "c", kind: Source(interarrival: Const(1000.0), first_at_zero: true, max_arrivals: 1,
        labels: {"gravidade": Const(5.0)})),
    (name: "b", kind: Source(interarrival: Const(1.0), max_arrivals: 1,
        labels: {"gravidade": Const(2.0)})),
    (name: "recepcao", kind: Queue()),
    (name: "triagem", kind: Processor(process: Const(2.0))),
    (name: "espera", kind: Queue(order: Some(Asc("gravidade")))),
    (name: "consultorio", kind: Processor(process: Const(10.0))),
    (name: "alta", kind: Sink),
], connections: [("a", "recepcao"), ("c", "recepcao"), ("b", "recepcao"), ("recepcao", "triagem"),
                 ("triagem", "espera"), ("espera", "consultorio"), ("consultorio", "alta")])"#;

#[test]
fn wait_for_the_doctor_by_colour_by_hand() {
    // Classification: A 0..2, C 2..4, B 4..6. A sees the doctor 2..12. At 12 the wait
    // holds C (since 4) and B (since 6): B, orange, goes first, 12..22; C 22..32.
    // Waits after the classification: A 0, B 6, C 18. Door to doctor: 2, 11, 22.
    let k = run(PS, Meta::default());
    let r = &k.reps[0];
    assert_eq!(r.atendidos, 3);
    near("espera", r.espera.unwrap(), 8.0);
    near("porta", r.porta_medico.unwrap(), 35.0 / 3.0);
    near("permanencia", r.permanencia, (12.0 + 21.0 + 32.0) / 3.0);
    near("laranja", r.por_cor[&2].espera.unwrap(), 6.0);
    near("azul", r.por_cor[&5].espera.unwrap(), 18.0);
    assert_eq!(r.no_alvo, Some(1.0));
    // With 5 minutes for orange, B misses its target.
    let mut meta = Meta::default();
    meta.alvo[1] = 5.0;
    let k = run(PS, meta);
    assert_eq!(k.reps[0].por_cor[&2].no_alvo, Some(0.0));
    near("no alvo", k.reps[0].no_alvo.unwrap(), 2.0 / 3.0);
    // Census: 2 present in 0..1, 3 in 1..12, 2 in 12..22, 1 in 22..32, none after.
    assert_eq!(k.censo.len(), 2);
    near("censo 0h", k.censo[0].1, (2.0 + 33.0 + 20.0 + 10.0) / 60.0);
    near("censo 1h", k.censo[1].1, 0.0);
}

/// P1 and P2 (yellow) at t = 0 into a wait with a patience of 3 minutes and a room of 10;
/// F, booked, does not come and goes out by output 2.
const SAIDAS: &str = r#"Model(name: "saidas", run: (stop_time: 100.0), objects: [
    (name: "p", kind: Source(interarrival: Const(1000.0), first_at_zero: true, max_arrivals: 1, batch: Some(Const(2.0)),
        labels: {"gravidade": Const(3.0)})),
    (name: "f", kind: Source(interarrival: Const(1000.0), first_at_zero: true, max_arrivals: 1,
        labels: {"agendado": Const(1.0), "presenca": Const(2.0)}), output: ByLabel("presenca")),
    (name: "espera", kind: Queue(order: Some(Asc("gravidade")), patience: Some(Const(3.0)))),
    (name: "consultorio", kind: Processor(process: Const(10.0))),
    (name: "alta", kind: Sink),
    (name: "faltas", kind: Sink),
], connections: [("p", "espera"), ("f", "espera"), ("f", "faltas"), ("espera", "consultorio"),
                 ("consultorio", "alta")])"#;

#[test]
fn left_without_being_seen_and_no_shows_by_hand() {
    // P1 sees the doctor 0..10; P2 gives up at 3. F goes straight to `faltas`.
    let k = run(SAIDAS, Meta::default());
    let r = &k.reps[0];
    assert_eq!(r.atendidos, 1);
    assert_eq!(r.abandonos, 1);
    near("abandono", r.abandono, 0.5);
    assert_eq!(r.faltas, 1);
    assert_eq!(r.por_cor[&3].abandonos, 1);
    assert_eq!(r.por_cor[&3].atendidos, 1);
    near("espera", r.espera.unwrap(), 0.0);
}

/// Three patients at t = 0 for two beds of 50 minutes, behind a wait for a bed.
const LEITOS: &str = r#"Model(name: "leitos", run: (stop_time: 200.0), objects: [
    (name: "entrada", kind: Source(interarrival: Const(1000.0), first_at_zero: true, max_arrivals: 1, batch: Some(Const(3.0)),
        labels: {"gravidade": Const(3.0)})),
    (name: "espera_leito", kind: Queue()),
    (name: "leitos", kind: Processor(capacity: 2, process: Const(50.0))),
    (name: "alta", kind: Sink),
], connections: [("entrada", "espera_leito"), ("espera_leito", "leitos"), ("leitos", "alta")])"#;

#[test]
fn beds_by_hand() {
    // Two beds taken 0..50, one 50..100: 150 bed-minutes of 400. The third patient waited
    // 50 for a bed: mean wait (0 + 0 + 50) / 3.
    let k = run(LEITOS, Meta::default());
    let r = &k.reps[0];
    near("ocupacao", r.leitos["leitos"], 150.0 / 400.0);
    near("internacao", r.internacao.unwrap(), 50.0);
    near("espera_leito", r.espera_leito.unwrap(), 50.0 / 3.0);
    assert_eq!(r.atendidos, 3);
    // Nobody saw a doctor: no wait for one.
    assert_eq!(r.consultas, 0);
    assert_eq!(r.espera, None);
}

#[test]
fn colours_have_names() {
    assert_eq!(cor(1), "vermelho");
    assert_eq!(cor(5), "azul");
    assert_eq!(cor(7), "G7");
    assert_eq!(Meta::default().alvo_de(2), Some(10.0));
    assert_eq!(Meta::default().alvo_de(0), None);
}
