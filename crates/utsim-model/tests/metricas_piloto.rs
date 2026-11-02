//! Métricas do polimento da v0.1 (achados do piloto): ocupação sem passar de 100%,
//! ocupação dentro do turno e sem bloqueio, fração de perdas e tempo entre dois pontos
//! do fluxo. Cada caso tem a resposta calculada à mão no comentário.

use utsim_core::{RunReport, summarize};
use utsim_model::Model;
use utsim_model::analysis::{bottleneck, model_stats, object_stats, pooled_histogram};

fn run_all(text: &str) -> (Model, Vec<RunReport>) {
    let m = Model::parse(text).expect("model parses");
    m.validate().expect("model is valid");
    let reports = m.run_replications().unwrap();
    (m, reports)
}

fn metric(r: &RunReport, key: &str) -> f64 {
    *r.metrics()
        .get(key)
        .unwrap_or_else(|| panic!("métrica {key} ausente"))
}

fn close(what: &str, got: f64, want: f64, tol: f64) {
    assert!(
        (got - want).abs() <= tol,
        "{what}: obtido {got:.6}, esperado {want:.6} (tolerância {tol})"
    );
}

/// A montagem recebe quatro componentes rápidos (um por unidade de tempo) enquanto
/// espera o contêiner (um a cada 10). Os componentes esperam DENTRO da montagem.
const MONTAGEM: &str = r#"Model(
    name: "montagem",
    run: (stop_time: 1000.0, replications: 2),
    objects: [
        (name: "conteineres", kind: Source(interarrival: Const(10.0), first_at_zero: true)),
        (name: "componentes", kind: Source(interarrival: Const(1.0), first_at_zero: true)),
        (name: "montagem", kind: Combiner(components: [4], process: Const(2.0))),
        (name: "inspecao", kind: Processor(process: Const(5.0))),
        (name: "saida", kind: Sink),
    ],
    connections: [("conteineres", "montagem"), ("componentes", "montagem"),
                  ("montagem", "inspecao"), ("inspecao", "saida")],
)"#;

#[test]
fn combiner_with_fast_components_stays_below_100_and_is_not_the_bottleneck() {
    // Ciclo de 10: o conjunto fica completo no instante em que o contêiner chega (ou no
    // quarto componente, no primeiro ciclo) e a montagem trabalha 2. Trabalho total em
    // [0, 1000]: 100 conjuntos * 2 = 200, ocupação 0,20. A inspeção processa 100
    // contêineres * 5 = 500, ocupação 0,50, e é o gargalo. O conteúdo médio da montagem
    // passa de 4 (componentes esperando), o que a conta antiga dividia por 1 (> 300%).
    let (m, reports) = run_all(MONTAGEM);
    let mont = reports[0].block("montagem").unwrap();
    assert!(mont.content_mean > 3.0, "conteúdo {}", mont.content_mean);
    let stats = object_stats(&m, &reports);
    let row = |n: &str| stats.iter().find(|s| s.name == n).unwrap();
    close(
        "montagem",
        row("montagem").utilization.unwrap().mean,
        0.2,
        1e-9,
    );
    close(
        "inspecao",
        row("inspecao").utilization.unwrap().mean,
        0.5,
        1e-9,
    );
    assert_eq!(stats[bottleneck(&stats).unwrap()].name, "inspecao");
    for s in &stats {
        for u in [
            s.utilization,
            s.utilization_incl_blocked,
            s.utilization_in_shift,
            s.utilization_excl_blocked,
        ]
        .into_iter()
        .flatten()
        {
            assert!(
                (0.0..=1.0 + 1e-12).contains(&u.mean),
                "{}: {}",
                s.name,
                u.mean
            );
        }
    }
    // As chaves nomeadas servem de objetivo e restrição.
    close(
        "chave",
        metric(&reports[0], "montagem.utilization"),
        0.2,
        1e-9,
    );
}

#[test]
fn processor_with_capacity_three_uses_mean_busy_servers() {
    // Chegada a cada 1, serviço 2, capacidade 3: em média 2 servidores ocupados, ou
    // 2/3 da capacidade. Pelo estado "processing" daria quase 100% (sempre há algum
    // item em serviço). Trabalho exato em [0, 1000]: itens 0..=998 inteiros (999 * 2
    // = 1998, menos o 998 que termina em 1000: ele também é inteiro) e o item 999 com 1:
    // 1999 unidades, 1,999 servidores, 0,66633 da capacidade.
    let (_, reports) = run_all(
        r#"Model(
        name: "cap3",
        run: (stop_time: 1000.0),
        objects: [
            (name: "src", kind: Source(interarrival: Const(1.0), first_at_zero: true)),
            (name: "p", kind: Processor(capacity: 3, process: Const(2.0))),
            (name: "out", kind: Sink),
        ],
        connections: [("src", "p"), ("p", "out")],
    )"#,
    );
    let r = &reports[0];
    assert!(metric(r, "p.state.processing") > 0.99);
    close("servers.busy", metric(r, "p.servers.busy"), 1.999, 1e-9);
    close("utilization", metric(r, "p.utilization"), 1.999 / 3.0, 1e-9);
    close(
        "incl_blocked",
        metric(r, "p.utilization_incl_blocked"),
        1.999 / 3.0,
        1e-9,
    );
}

#[test]
fn blocked_time_splits_the_three_utilisations() {
    // p1 (1) alimenta p2 (3) sem fila entre eles: a cada 3 unidades, p1 trabalha 1 e
    // fica 2 bloqueado. Ocupação 1/3, incluindo bloqueado 1, sem bloqueio
    // (1/3) / (1 - 2/3) = 1.
    let (_, reports) = run_all(
        r#"Model(
        name: "bloqueio",
        run: (stop_time: 3000.0),
        objects: [
            (name: "src", kind: Source(interarrival: Const(0.5), first_at_zero: true)),
            (name: "q", kind: Queue()),
            (name: "p1", kind: Processor(process: Const(1.0))),
            (name: "p2", kind: Processor(process: Const(3.0))),
            (name: "out", kind: Sink),
        ],
        connections: [("src", "q"), ("q", "p1"), ("p1", "p2"), ("p2", "out")],
    )"#,
    );
    let r = &reports[0];
    close("work", metric(r, "p1.utilization"), 1.0 / 3.0, 2e-3);
    close("blocked", metric(r, "p1.state.blocked"), 2.0 / 3.0, 2e-3);
    close(
        "incl_blocked",
        metric(r, "p1.utilization_incl_blocked"),
        1.0,
        2e-3,
    );
    close(
        "excl_blocked",
        metric(r, "p1.utilization_excl_blocked"),
        1.0,
        5e-3,
    );
    // p2 trabalha o tempo todo: 1 em todas as medidas.
    close("p2", metric(r, "p2.utilization"), 1.0, 2e-3);
}

#[test]
fn utilisation_in_shift_leaves_off_shift_time_out() {
    // Chegada a cada 1, serviço 0,5, turno de 10 com janela fora [8, 10). Os itens que
    // chegam fora do turno esperam e são atendidos logo depois; só os dois últimos
    // (chegadas 98 e 99, na última janela) ficam sem atendimento até o fim em 100.
    // Trabalho: 98 * 0,5 = 49 de 100, ocupação 0,49; fora de turno 0,2;
    // dentro do turno 0,49 / 0,8 = 0,6125.
    let (_, reports) = run_all(
        r#"Model(
        name: "turno",
        run: (stop_time: 100.0),
        objects: [
            (name: "src", kind: Source(interarrival: Const(1.0), first_at_zero: true)),
            (name: "q", kind: Queue()),
            (name: "p", kind: Processor(process: Const(0.5)),
             shifts: (period: 10.0, off: [(start: 8.0, duration: 2.0)])),
            (name: "out", kind: Sink),
        ],
        connections: [("src", "q"), ("q", "p"), ("p", "out")],
    )"#,
    );
    let r = &reports[0];
    close("off_shift", metric(r, "p.state.off_shift"), 0.2, 1e-9);
    close("utilization", metric(r, "p.utilization"), 0.49, 1e-9);
    close(
        "in_shift",
        metric(r, "p.utilization_in_shift"),
        0.6125,
        1e-9,
    );
}

const PERDAS: &str = r#"Model(
    name: "perdas",
    run: (stop_time: 300.0, warmup: WARM, replications: 2),
    objects: [
        (name: "src", kind: Source(interarrival: Const(1.0), first_at_zero: true, on_blocked: Drop)),
        (name: "p", kind: Processor(process: Const(2.5))),
        (name: "out", kind: Sink),
    ],
    connections: [("src", "p"), ("p", "out")],
)"#;

#[test]
fn drop_source_reports_lost_count_and_loss_fraction() {
    // Chegadas em 0, 1, 2, ...; o processo leva 2,5. Entra o item de 0, perdem-se 1 e
    // 2; entra o de 3 (livre desde 2,5), perdem-se 4 e 5; e assim por diante. Entram os
    // itens de 0, 3, ..., 300 (101) e perdem-se 200: fração 200/301.
    let (m, reports) = run_all(&PERDAS.replace("WARM", "0.0"));
    let r = &reports[0];
    close("lost", metric(r, "src.lost.count"), 200.0, 0.0);
    close("entered", metric(r, "src.entered.count"), 101.0, 0.0);
    close(
        "fraction",
        metric(r, "src.loss_fraction"),
        200.0 / 301.0,
        1e-12,
    );
    close(
        "model",
        metric(r, "model.loss_fraction"),
        200.0 / 301.0,
        1e-12,
    );
    close("model lost", metric(r, "model.lost.count"), 200.0, 0.0);
    // Resumo entre replicações e tabela do modelo.
    let flat = summarize(&reports);
    close("summary", flat["src.lost.count"].mean, 200.0, 0.0);
    let totals = model_stats(&reports);
    assert!(totals.iter().any(|(k, _)| k == "model.loss_fraction"));
    drop(m);
}

#[test]
fn loss_metrics_follow_the_warm_up_window() {
    let (_, reports) = run_all(&PERDAS.replace("WARM", "100.0"));
    let r = &reports[0];
    let src = r.block("src").unwrap();
    let lost = metric(r, "src.lost.count");
    let entered = metric(r, "src.entered.count");
    assert_eq!(lost, src.extra["lost"].count as f64);
    assert_eq!(entered, src.output as f64);
    close(
        "fraction",
        metric(r, "src.loss_fraction"),
        lost / (lost + entered),
        1e-12,
    );
    // Dois de cada três itens se perdem; com a janela de 200 unidades a conta é exata
    // a menos de uma chegada nas bordas.
    close("fraction", metric(r, "src.loss_fraction"), 2.0 / 3.0, 5e-3);
}

#[test]
fn waiting_sources_have_no_loss_metrics() {
    let (_, reports) = run_all(
        &PERDAS
            .replace("WARM", "0.0")
            .replace("on_blocked: Drop", ""),
    );
    assert!(!reports[0].metrics().contains_key("model.loss_fraction"));
    assert!(!reports[0].metrics().contains_key("src.loss_fraction"));
}

const MEDICAO: &str = r#"Model(
    name: "medicao",
    run: (stop_time: 100.0, histogram_bins: 4, replications: 3),
    objects: [
        (name: "src", kind: Source(interarrival: Const(2.0), first_at_zero: true,
                                   max_arrivals: Some(5))),
        (name: "p", kind: Processor(process: Const(1.0))),
        (name: "q", kind: Queue()),
        (name: "s", kind: Processor(process: Const(3.0))),
        (name: "out", kind: Sink),
    ],
    connections: [("src", "p"), ("p", "q"), ("q", "s"), ("s", "out")],
    spans: [(name: "p_ate_saida", from: "p", to: "out")],
)"#;

#[test]
fn measure_records_the_time_between_two_points() {
    // Chegadas em 0, 2, 4, 6, 8; p sai em 1, 3, 5, 7, 9. O serviço s (3) é sequencial:
    // saídas em 4, 7, 10, 13, 16. Tempo de p até a saída: 3, 4, 5, 6, 7 (média 5).
    let (m, reports) = run_all(MEDICAO);
    let r = &reports[0];
    close("mean", metric(r, "p_ate_saida.value.mean"), 5.0, 1e-12);
    close("count", metric(r, "p_ate_saida.value.count"), 5.0, 0.0);
    let t = &r.block("p_ate_saida").unwrap().extra["value"];
    assert_eq!((t.min, t.max), (3.0, 7.0));
    // Histograma reunido das réplicas: 15 observações.
    let h = pooled_histogram(&reports, "p_ate_saida", "value").unwrap();
    assert_eq!(h.counts.iter().sum::<u64>(), 15);
    // Aparece como linha da tabela de resultados e como métrica de experimento.
    let stats = object_stats(&m, &reports);
    let row = stats.iter().find(|s| s.name == "p_ate_saida").unwrap();
    assert_eq!(row.kind, "span");
    assert!(row.extra.iter().any(|(k, s)| k == "value" && s.mean == 5.0));
    assert!(summarize(&reports).contains_key("p_ate_saida.value.mean"));
}

#[test]
fn measure_names_and_endpoints_are_validated() {
    let bad = MEDICAO.replace("to: \"out\"", "to: \"nada\"");
    let err = Model::parse(&bad)
        .unwrap()
        .validate()
        .unwrap_err()
        .to_string();
    assert!(err.contains("'nada' não é um objeto"), "{err}");
    let dup = MEDICAO.replace("name: \"p_ate_saida\"", "name: \"out\"");
    let err = Model::parse(&dup)
        .unwrap()
        .validate()
        .unwrap_err()
        .to_string();
    assert!(err.contains("já é usado"), "{err}");
}

#[test]
fn models_without_measures_serialise_without_the_field() {
    let m = Model::parse(PERDAS.replace("WARM", "0.0").as_str()).unwrap();
    assert!(!ron::to_string(&m).unwrap().contains("spans"));
}
