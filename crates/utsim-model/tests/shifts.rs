//! Testes determinísticos de turnos programados (`shifts`): contagens exatas e frações
//! de estado calculadas à mão. Tempos em unidades arbitrárias do modelo.

use utsim_core::RunReport;
use utsim_model::Model;

fn run(text: &str) -> RunReport {
    let m = Model::parse(text).expect("model parses");
    m.validate().expect("model is valid");
    m.run_replications().unwrap().remove(0)
}

fn assert_close(what: &str, got: f64, want: f64, rel_tol: f64) {
    let rel = (got - want).abs() / want.abs().max(1e-12);
    assert!(
        rel <= rel_tol,
        "{what}: obtido {got:.4}, esperado {want:.4} (erro {:.2}%)",
        rel * 100.0
    );
}

/// Fonte sempre disponível (um item por unidade de tempo), fila ilimitada, uma máquina
/// com `process` e turnos opcionais, e um sumidouro.
fn line(stop: f64, process: &str, machine_extra: &str) -> String {
    format!(
        r#"Model(
            name: "turnos",
            run: (stop_time: {stop}),
            objects: [
                (name: "src", kind: Source(interarrival: Const(1.0), first_at_zero: true)),
                (name: "q", kind: Queue()),
                (name: "p", kind: Processor(process: {process}){machine_extra}),
                (name: "out", kind: Sink),
            ],
            connections: [("src", "q"), ("q", "p"), ("p", "out")],
        )"#
    )
}

#[test]
fn two_windows_per_cycle_give_exact_fractions_and_output() {
    // Janelas por ciclo de 24: [4, 5) e [12, 16), total 5 de 24 fora de turno.
    // Máquina de 1.0 sempre ocupada: 19 de cada 24 unidades processando.
    // Em 240 (10 ciclos): 190 unidades de processamento, cada uma vira um item
    // (a unidade que termina exatamente numa janela sai no reinício, sem perder tempo).
    let m = line(
        240.0,
        "Const(1.0)",
        ", shifts: (period: 24.0, off: [(start: 4.0, duration: 1.0), (start: 12.0, duration: 4.0)])",
    );
    let r = run(&m);
    let p = r.block("p").unwrap();
    assert_close("off_shift", p.states["off_shift"], 5.0 / 24.0, 1e-9);
    assert_close("processing", p.states["processing"], 19.0 / 24.0, 1e-9);
    assert_eq!(p.output, 190);
}

#[test]
fn processing_straddling_a_window_is_paused_and_resumed() {
    // Processo de 3.0, ciclo de 10, janela [4, 6).
    // A: entra em 0, termina em 3 (antes da janela).
    // B: entra em 3 e para em 4 com 2 unidades restantes (3 - 1 já feitas), retoma em 6
    //    e termina em 8. Sem a pausa terminaria em 6.
    // C: entra em 8, termina em 11. D: entra em 11, termina no início da janela [14, 16)
    //    e sai em 16.
    // Pela contagem em unidades de tempo ligado (uptime), a saída em 100 é
    // floor((100 - 2 * 10) / 3) = floor(80 / 3) = 26. A 26.ª unidade completa em 98.
    let m = line(
        100.0,
        "Const(3.0)",
        ", shifts: (period: 10.0, off: [(start: 4.0, duration: 2.0)])",
    );
    // Parando em 7.5 ainda não há a segunda saída (B só termina em 8).
    let early = run(&line(
        7.5,
        "Const(3.0)",
        ", shifts: (period: 10.0, off: [(start: 4.0, duration: 2.0)])",
    ));
    assert_eq!(early.block("p").unwrap().output, 1);

    let r = run(&m);
    assert_eq!(r.block("p").unwrap().output, 26);
}

#[test]
fn source_pauses_its_arrival_clock_while_off_shift() {
    // Chegadas a cada 1.0, primeira em 0. Janelas [5, 10), [15, 20), ... (start 5,
    // duration 5, período 10). Ao parar, a chegada pendente é cancelada e o tempo que
    // falta é guardado; ao retomar, a chegada sai na hora (o restante é 0 quando a
    // janela começa exatamente na chegada).
    // Em cada ciclo [10k, 10k+10) saem 5 itens (10k .. 10k+4). Em [0, 100): 50 itens.
    // A chegada em 100 acontece porque ShiftOn(100) e a chegada são processados
    // (eventos com tempo igual ao de parada são executados). Total: 51.
    let m = r#"Model(
        name: "fonte_com_turno",
        run: (stop_time: 100.0),
        objects: [
            (name: "src", kind: Source(interarrival: Const(1.0), first_at_zero: true),
             shifts: (period: 10.0, off: [(start: 5.0, duration: 5.0)])),
            (name: "q", kind: Queue()),
            (name: "out", kind: Sink),
        ],
        connections: [("src", "q"), ("q", "out")],
    )"#;
    let r = run(m);
    let src = r.block("src").unwrap();
    assert_eq!(src.output, 51);
    assert_eq!(r.block("out").unwrap().input, 51);
    assert_close("src off_shift", src.states["off_shift"], 0.5, 1e-9);
}

#[test]
fn breakdown_and_shift_show_down_first_and_stack_correctly() {
    // Falhas: mtbf 7 e reparo 2, sem aleatoriedade (Const). O relógio do mtbf começa no
    // fim de cada reparo. Intervalos de quebra em [0, 100):
    //   [7,9) [16,18) [25,27) [34,36) [43,45) [52,54) [61,63) [70,72) [79,81) [88,90) [97,99)
    // Total: 11 * 2 = 22 de quebra.
    // Janelas de turno [8,10), [18,20), ..., [98,100): 10 * 2 = 20 fora de turno.
    // Sobreposições (quebra e turno ao mesmo tempo, exibido como down): [8,9), [79,80),
    // [88,90), [98,99) = 1 + 1 + 2 + 1 = 5.
    // Só turno (off_shift): 20 - 5 = 15.
    let m = r#"Model(
        name: "quebra_e_turno",
        run: (stop_time: 100.0),
        objects: [
            (name: "src", kind: Source(interarrival: Const(1.0), first_at_zero: true)),
            (name: "q", kind: Queue()),
            (name: "p", kind: Processor(process: Const(1.0)),
             downtime: (mtbf: Const(7.0), mttr: Const(2.0)),
             shifts: (period: 10.0, off: [(start: 8.0, duration: 2.0)])),
            (name: "out", kind: Sink),
        ],
        connections: [("src", "q"), ("q", "p"), ("p", "out")],
    )"#;
    let r = run(m);
    let p = r.block("p").unwrap();
    assert_close("down", p.states["down"], 0.22, 1e-9);
    assert_close("off_shift", p.states["off_shift"], 0.15, 1e-9);
}

/// Mensagem de erro de validação de um modelo com `shifts` inválido.
fn shifts_error(shifts: &str) -> String {
    let text = format!(
        r#"Model(
            name: "invalido",
            run: (stop_time: 100.0),
            objects: [
                (name: "src", kind: Source(interarrival: Const(1.0))),
                (name: "p", kind: Processor(process: Const(1.0)), shifts: {shifts}),
                (name: "out", kind: Sink),
            ],
            connections: [("src", "p"), ("p", "out")],
        )"#
    );
    let m = Model::parse(&text).expect("model parses");
    m.validate()
        .expect_err("model with bad shifts must be rejected")
        .to_string()
}

#[test]
fn invalid_shift_tables_are_rejected_by_validate() {
    let overlap = shifts_error(
        "(period: 10.0, off: [(start: 1.0, duration: 3.0), (start: 2.0, duration: 1.0)])",
    );
    assert!(overlap.contains("shifts"), "sobreposição: {overlap}");

    let beyond = shifts_error("(period: 10.0, off: [(start: 9.0, duration: 2.0)])");
    assert!(beyond.contains("shifts"), "fora do período: {beyond}");

    let zero = shifts_error("(period: 10.0, off: [(start: 1.0, duration: 0.0)])");
    assert!(zero.contains("shifts"), "duração zero: {zero}");

    let period = shifts_error("(period: 0.0, off: [])");
    assert!(period.contains("shifts"), "período zero: {period}");

    let negative = shifts_error("(period: -5.0, off: [])");
    assert!(negative.contains("shifts"), "período negativo: {negative}");
}

#[test]
fn valid_shift_table_passes_validate() {
    // Controle: a mesma forma de modelo com janelas válidas não é rejeitada.
    let text = r#"Model(
        name: "valido",
        run: (stop_time: 100.0),
        objects: [
            (name: "src", kind: Source(interarrival: Const(1.0))),
            (name: "p", kind: Processor(process: Const(1.0)),
             shifts: (period: 10.0, off: [(start: 1.0, duration: 2.0), (start: 5.0, duration: 4.0)])),
            (name: "out", kind: Sink),
        ],
        connections: [("src", "p"), ("p", "out")],
    )"#;
    Model::parse(text)
        .unwrap()
        .validate()
        .expect("valid shifts");
}
