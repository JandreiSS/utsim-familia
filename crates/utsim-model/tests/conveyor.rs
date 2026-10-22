//! Testes determinísticos da esteira cinemática (`Conveyor`), com valores calculados à mão.
//!
//! Esteira padrão dos testes: comprimento L = 10, velocidade v = 2, espaço por item
//! s = 2. Logo o trânsito livre dura L/v = 5, o espaçamento mínimo de entrada é
//! s/v = 1 e a capacidade é C = ⌊L/s⌋ = 5. Os instantes de entrada e de saída vêm do
//! rastro (`Moved`), então são exatos.

use utsim_core::{RunReport, TraceEvent};
use utsim_model::Model;

struct Out {
    report: RunReport,
    /// Instantes em que itens entraram na esteira `belt`.
    entries: Vec<f64>,
    /// Instantes em que itens saíram da esteira `belt`.
    exits: Vec<f64>,
}

fn run(text: &str) -> Out {
    let m = Model::parse(text).expect("model parses");
    m.validate().expect("model is valid");
    let mut sim = m.build(0);
    let belt = sim.block_id("belt").expect("block 'belt'").0;
    sim.enable_trace();
    let report = sim.run().expect("run ok");
    let (mut entries, mut exits) = (Vec::new(), Vec::new());
    for r in sim.take_trace() {
        if let TraceEvent::Moved { from, to, .. } = r.event {
            if to == belt {
                entries.push(r.t);
            }
            if from == belt {
                exits.push(r.t);
            }
        }
    }
    Out {
        report,
        entries,
        exits,
    }
}

fn assert_times(what: &str, got: &[f64], want: &[f64]) {
    assert_eq!(
        got.len(),
        want.len(),
        "{what}: obtido {got:?}, esperado {want:?}"
    );
    for (g, w) in got.iter().zip(want) {
        assert!(
            (g - w).abs() < 1e-9,
            "{what}: obtido {got:?}, esperado {want:?}"
        );
    }
}

fn assert_close(what: &str, got: f64, want: f64, rel_tol: f64) {
    let rel = (got - want).abs() / want.abs().max(1e-12);
    assert!(
        rel <= rel_tol,
        "{what}: obtido {got:.4}, esperado {want:.4} (erro {:.2}%)",
        rel * 100.0
    );
}

/// Fonte -> esteira -> `tail`, onde `tail` são objetos e conexões adicionais
/// ("sink" por padrão). `belt_extra` entra na definição da esteira (quebras, turnos).
fn model(
    stop: f64,
    source: &str,
    accumulating: bool,
    belt_extra: &str,
    tail_objects: &str,
    tail_conn: &str,
) -> String {
    format!(
        r#"Model(
            name: "esteira",
            run: (stop_time: {stop}),
            objects: [
                (name: "src", kind: Source({source})),
                (name: "belt", kind: Conveyor(length: 10.0, speed: 2.0, item_length: 2.0, accumulating: {accumulating}){belt_extra}),
                {tail_objects}
            ],
            connections: [("src", "belt"), {tail_conn}],
        )"#
    )
}

/// Esteira direto num sumidouro.
fn to_sink(stop: f64, source: &str, accumulating: bool, belt_extra: &str) -> String {
    model(
        stop,
        source,
        accumulating,
        belt_extra,
        r#"(name: "out", kind: Sink),"#,
        r#"("belt", "out")"#,
    )
}

/// Esteira -> sumidouro que fica fora de turno de 6.5 a 20.0 (saída fechada).
fn to_gated_sink(source: &str, accumulating: bool) -> String {
    model(
        30.0,
        source,
        accumulating,
        "",
        r#"(name: "out", kind: Sink, shifts: (period: 1000.0, off: [(start: 6.5, duration: 13.5)])),"#,
        r#"("belt", "out")"#,
    )
}

#[test]
fn free_flow_takes_length_over_speed() {
    // Itens entram em 0, 10 e 20 (espaçamento da fonte 10, bem acima de s/v = 1).
    // Cada um leva L/v = 10/2 = 5 e sai em 5, 15 e 25. Tempo na esteira: 5 para todos.
    let o = run(&to_sink(
        100.0,
        "interarrival: Const(10.0), first_at_zero: true, max_arrivals: Some(3)",
        true,
        "",
    ));
    assert_times("entradas", &o.entries, &[0.0, 10.0, 20.0]);
    assert_times("saídas", &o.exits, &[5.0, 15.0, 25.0]);
    let belt = o.report.block("belt").unwrap();
    assert_eq!(belt.staytime.min, 5.0);
    assert_eq!(belt.staytime.max, 5.0);
}

#[test]
fn entry_spacing_limits_throughput_to_speed_over_item_length() {
    // A fonte gera a cada 0.25, quatro vezes mais rápido que s/v = 1. O item 0 entra em 0
    // e o seguinte só em 1 (o anterior precisa andar s = 2 a v = 2), depois em 2, 3, ...
    // Cada entrada k sai em k + 5. A fonte espera (OnBlocked::Wait), então o k-ésimo
    // item entra exatamente em k. Isso também prova que `wake_inputs` religa a fonte.
    let o = run(&to_sink(
        12.0,
        "interarrival: Const(0.25), first_at_zero: true",
        true,
        "",
    ));
    let want_in: Vec<f64> = (0..=12).map(|k| k as f64).collect();
    assert_times("entradas", &o.entries, &want_in);
    // Saídas até t = 12: entradas 0..=7 saem em 5..=12.
    let want_out: Vec<f64> = (5..=12).map(|k| k as f64).collect();
    assert_times("saídas", &o.exits, &want_out);
}

#[test]
fn accumulation_behind_slow_processor_releases_one_per_processing_time() {
    // Entradas em 0, 1, 2, 3 (fonte com Const(1.0) = s/v, exatamente no limite).
    // Processador serial de 10 à frente.
    //  - Item 0 chega ao fim em 5 e entra no processador (5..15).
    //  - Item 1 chegaria em 6, mas espera no fim até 15: sai da esteira em 15.
    //  - Item 2 (entrou em 2) para em L - s = 8 e, quando o item 1 sai (15), anda s = 2
    //    em 1 e chega ao fim em 16 = 15 + s/v; só sai em 25, quando o processador libera.
    //  - Item 3 para em L - 2s = 6; chega ao fim em 25 + ... = 26 e sai em 35.
    // Saídas da esteira: 5, 15, 25, 35 (de 10 em 10, ritmo do processador).
    let o = run(&model(
        100.0,
        "interarrival: Const(1.0), first_at_zero: true, max_arrivals: Some(4)",
        true,
        "",
        r#"(name: "p", kind: Processor(process: Const(10.0))), (name: "out", kind: Sink),"#,
        r#"("belt", "p"), ("p", "out")"#,
    ));
    assert_times("entradas", &o.entries, &[0.0, 1.0, 2.0, 3.0]);
    assert_times("saídas", &o.exits, &[5.0, 15.0, 25.0, 35.0]);
}

#[test]
fn release_after_blocking_takes_item_length_over_speed_per_item() {
    // Saída fechada de 6.5 a 20. Fonte Const(2): entradas em 0, 2, 4, 6, 8, 10.
    //  - Item 0 sai em 5 (livre). Item 1 chegaria em 7 mas a saída está fechada: espera
    //    na ponta até 20 (estado blocked, 13 unidades).
    //  - Item 2 (entrou em 4): vai a L - s = 8 (para em 8). Item 3 (entrou em 6): para
    //    em L - 2s = 6. Item 4 (entrou em 8): para em 4. Item 5 (entrou em 10): 2.
    //    Todas as entradas são aceitas (conteúdo <= 4 < C = 5; zona de entrada livre).
    //  - Em 20 o item 1 sai; cada seguinte só precisa andar s = 2, ou seja, s/v = 1:
    //    e_i = max(t_in + L/v, d_(i-1) + s/v) = 21, 22, 23, 24.
    // Estados em 30: blocked 13/30 (7..20), empty 6/30 (24..30), releasing 11/30.
    let o = run(&to_gated_sink(
        "interarrival: Const(2.0), first_at_zero: true, max_arrivals: Some(6)",
        true,
    ));
    assert_times("entradas", &o.entries, &[0.0, 2.0, 4.0, 6.0, 8.0, 10.0]);
    assert_times("saídas", &o.exits, &[5.0, 20.0, 21.0, 22.0, 23.0, 24.0]);
    let belt = o.report.block("belt").unwrap();
    assert_close("blocked", belt.states["blocked"], 13.0 / 30.0, 1e-9);
    assert_close("empty", belt.states["empty"], 6.0 / 30.0, 1e-9);
    assert_close("releasing", belt.states["releasing"], 11.0 / 30.0, 1e-9);
}

#[test]
fn non_accumulating_belt_delays_every_item_by_the_blocked_time() {
    // Mesma saída fechada (6.5 a 20), só três itens: entradas 0, 2, 4.
    //  - Item 0 sai em 5. Item 1 chega ao fim em 7 e a correia toda para até 20 (13).
    //  - Item 2 estava em 6 quando parou (entrou em 4, andou 3 * 2); falta 4, ou seja,
    //    2 unidades de tempo depois de retomar: chega em 22 = 9 (sem bloqueio) + 13.
    //  - Com acúmulo (teste anterior) o item 2 sairia em 21: 1 antes.
    let src = "interarrival: Const(2.0), first_at_zero: true, max_arrivals: Some(3)";
    let o = run(&to_gated_sink(src, false));
    assert_times("saídas sem acúmulo", &o.exits, &[5.0, 20.0, 22.0]);
    let o = run(&to_gated_sink(src, true));
    assert_times("saídas com acúmulo", &o.exits, &[5.0, 20.0, 21.0]);
}

#[test]
fn non_accumulating_belt_can_accept_items_while_stopped_if_the_entry_zone_is_clear() {
    // Mesma situação, seis itens. A correia para em 7 e só volta em 20.
    //  - Item 3 entrou em 6 e está em 2 quando a correia para: a zona de entrada está
    //    livre, então o item 4 entra em 8 (conteúdo 3 < 5) e fica parado na posição 0.
    //  - O item 5 (criado em 10) só entra quando o item 4 andar s: a correia volta em 20,
    //    então entra em 21.
    //  - Chegadas ao fim (a partir de 20, andando): item 1 sai em 20; item 2 (pos 6) em
    //    20 + 4/2 = 22; item 3 (pos 2) em 20 + 8/2 = 24; item 4 (pos 0) em 20 + 5 = 25;
    //    item 5 entrou em 21: chega em 26.
    let o = run(&to_gated_sink(
        "interarrival: Const(2.0), first_at_zero: true, max_arrivals: Some(6)",
        false,
    ));
    assert_times("entradas", &o.entries, &[0.0, 2.0, 4.0, 6.0, 8.0, 21.0]);
    assert_times("saídas", &o.exits, &[5.0, 20.0, 22.0, 24.0, 25.0, 26.0]);
}

#[test]
fn capacity_is_floor_of_length_over_item_length() {
    // Processador de 1000 à frente: o item 0 fica lá. Fonte Const(1.0), então entradas
    // em 0, 1, 2, 3, 4 enchem a esteira (C = 5) e o item 5 só entra quando o item 0
    // chega ao fim (t = 5) e libera uma vaga: entrada em 5. Depois a esteira fica
    // cheia (5 itens) e nada mais entra até 1005 (> stop = 100): 6 entradas, no máximo 5
    // itens ao mesmo tempo.
    let o = run(&model(
        100.0,
        "interarrival: Const(1.0), first_at_zero: true",
        true,
        "",
        r#"(name: "p", kind: Processor(process: Const(1000.0))), (name: "out", kind: Sink),"#,
        r#"("belt", "p"), ("p", "out")"#,
    ));
    assert_times("entradas", &o.entries, &[0.0, 1.0, 2.0, 3.0, 4.0, 5.0]);
    assert_times("saídas", &o.exits, &[5.0]);
    let belt = o.report.block("belt").unwrap();
    assert_eq!(belt.content_max, 5.0);
    assert_eq!(belt.content_now, 5);
    assert_eq!(belt.input, 6);
}

#[test]
fn shift_stop_pauses_transit_by_exactly_the_stopped_time() {
    // Esteira fora de turno de 3 a 5 (2 unidades). Fonte Const(2), três itens.
    //  - Item 0 entra em 0, está em 6 quando para (3 * 2), retoma em 5: falta 4, sai em 7.
    //  - Item 1 entra em 2, está em 2 em t = 3; falta 8, ou seja, 4 depois de 5: sai em 9.
    //  - Item 2 seria criado em 4, mas a entrada está fechada durante a parada: entra em
    //    5 (a retomada reabre a entrada) e sai em 10.
    let o = run(&to_sink(
        50.0,
        "interarrival: Const(2.0), first_at_zero: true, max_arrivals: Some(3)",
        true,
        r#", shifts: (period: 1000.0, off: [(start: 3.0, duration: 2.0)])"#,
    ));
    assert_times("entradas", &o.entries, &[0.0, 2.0, 5.0]);
    assert_times("saídas", &o.exits, &[7.0, 9.0, 10.0]);
}

#[test]
fn breakdown_pauses_transit_by_exactly_the_repair_time() {
    // Quebra em 3 (mtbf 3) durante 2 (mttr): um item que entra em 0 sai em 5 + 2 = 7.
    let o = run(&to_sink(
        7.5,
        "interarrival: Const(100.0), first_at_zero: true, max_arrivals: Some(1)",
        true,
        r#", downtime: (mtbf: Const(3.0), mttr: Const(2.0))"#,
    ));
    assert_times("saídas", &o.exits, &[7.0]);
    let belt = o.report.block("belt").unwrap();
    // down de 3 a 5 em 7.5 de simulação.
    assert_close("down", belt.states["down"], 2.0 / 7.5, 1e-9);
}

#[test]
fn steady_state_rate_is_the_minimum_of_arrival_belt_and_downstream_rates() {
    // Velocidade/espaço dá v/s = 1 item por unidade de tempo. Em 10000 unidades:
    //  A) chegada 0.5/u, processador 0.8/u  -> mínimo 0.5 (chegadas): 5000 itens.
    //  B) chegada 2.0/u, processador 0.8/u  -> mínimo 0.8 (processador): 8000.
    //  C) chegada 2.0/u, processador 4.0/u  -> mínimo 1.0 (esteira): 10000.
    // Perdas de transiente: no máximo C = 5 itens na esteira mais o do processador,
    // ou seja, bem menos que 0.5% de 5000. Vale para os dois tipos de esteira.
    let cases = [
        ("Const(2.0)", "Const(1.25)", 5000.0),
        ("Const(0.5)", "Const(1.25)", 8000.0),
        ("Const(0.5)", "Const(0.25)", 10000.0),
    ];
    for accumulating in [true, false] {
        for (arrival, process, want) in cases {
            let o = run(&model(
                10000.0,
                &format!("interarrival: {arrival}, first_at_zero: true"),
                accumulating,
                "",
                &format!(
                    r#"(name: "p", kind: Processor(process: {process})), (name: "out", kind: Sink),"#
                ),
                r#"("belt", "p"), ("p", "out")"#,
            ));
            let got = o.report.block("out").unwrap().input as f64;
            assert_close(
                &format!("vazão ({arrival}, {process}, acúmulo={accumulating})"),
                got,
                want,
                0.005,
            );
        }
    }
}
