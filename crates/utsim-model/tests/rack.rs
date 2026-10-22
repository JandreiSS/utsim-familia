//! Testes do porta-paletes (`Rack`): posições, permanência, bloqueio, quebras e teoria.
//!
//! Os valores exatos foram calculados à mão antes de rodar, e cada teste explica a conta.
//! Tempos em unidades arbitrárias do modelo.

use std::collections::HashMap;

use utsim_core::{RunReport, TraceEvent};
use utsim_model::Model;
use utsim_model::replay::Replay;

fn model(text: &str) -> Model {
    let m = Model::parse(text).expect("model parses");
    m.validate().expect("model is valid");
    m
}

fn run(text: &str) -> RunReport {
    model(text).run_replications().unwrap().remove(0)
}

fn replay(text: &str) -> Replay {
    model(text).replay(0).expect("replay runs")
}

fn assert_exact(what: &str, got: f64, want: f64) {
    assert!(
        (got - want).abs() < 1e-9,
        "{what}: obtido {got}, esperado {want}"
    );
}

fn assert_close(what: &str, got: f64, want: f64, rel_tol: f64) {
    let rel = (got - want).abs() / want.abs().max(1e-12);
    println!(
        "{what}: obtido {got:.4}, esperado {want:.4} (erro {:.2}%)",
        rel * 100.0
    );
    assert!(
        rel <= rel_tol,
        "{what}: obtido {got:.4}, esperado {want:.4} (erro {:.2}%)",
        rel * 100.0
    );
}

/// Índice de um bloco no `Replay` (ordem do arquivo).
fn index_of(r: &Replay, name: &str) -> usize {
    r.blocks
        .iter()
        .position(|b| b.name == name)
        .expect("block exists")
}

/// Registros `Slot` do bloco `block`: (instante, item, posição).
fn slot_records(r: &Replay, block: usize) -> Vec<(f64, u64, usize)> {
    r.records
        .iter()
        .filter_map(|rec| match rec.event {
            TraceEvent::Slot {
                item,
                block: b,
                slot,
            } if b == block => Some((rec.t, item, slot)),
            _ => None,
        })
        .collect()
}

/// Entradas no bloco `to`: (instante, item).
fn entries(r: &Replay, to: usize) -> Vec<(f64, u64)> {
    r.records
        .iter()
        .filter_map(|rec| match rec.event {
            TraceEvent::Moved { item, to: t, .. } if t == to => Some((rec.t, item)),
            _ => None,
        })
        .collect()
}

/// Fonte determinística e um porta-paletes `bays` × `levels` com permanência `dwell`,
/// seguido de sumidouro. `extra` vai dentro do bloco do porta-paletes.
fn rack_line(
    stop: f64,
    source: &str,
    bays: usize,
    levels: usize,
    dwell: &str,
    extra: &str,
) -> String {
    format!(
        r#"Model(
            name: "porta-paletes",
            run: (stop_time: {stop:?}),
            objects: [
                (name: "src", kind: Source(interarrival: {source})),
                (name: "rk", kind: Rack(bays: {bays}, levels: {levels}, dwell: Some({dwell})){extra}),
                (name: "out", kind: Sink),
            ],
            connections: [("src", "rk"), ("rk", "out")],
        )"#
    )
}

#[test]
fn firstfree_fills_2x2_in_slot_order_and_reuses_the_freed_slot() {
    // Fonte: um item por unidade, o primeiro em t=0, cinco no total.
    // Porta-paletes 2×2 (4 posições), permanência Const(10), FirstFree.
    //   item 0: entra em t=0, slot 0 (nível 0, coluna 0), sai em 10.
    //   item 1: entra em t=1, slot 1; sai em 11.
    //   item 2: entra em t=2, slot 2 (nível 1, coluna 0); sai em 12.
    //   item 3: entra em t=3, slot 3; sai em 13.
    //   item 4: criado em t=4, o porta-paletes está cheio: a fonte bloqueia.
    //           O item 0 sai em 10, libera o slot 0, e o item 4 entra ali em t=10
    //           (é o único livre). Sai em 20.
    // Sequência esperada de slots: [0, 1, 2, 3, 0] nos instantes [0, 1, 2, 3, 10].
    // Permanência de todos os 5 itens: 10 (saída − entrada), média 10 exatamente.
    let text = rack_line(
        30.0,
        r#"Const(1.0), first_at_zero: true, max_arrivals: Some(5)"#,
        2,
        2,
        "Const(10.0)",
        "",
    );
    let r = replay(&text);
    let rk = index_of(&r, "rk");
    let slots = slot_records(&r, rk);
    let times: Vec<f64> = slots.iter().map(|s| s.0).collect();
    let positions: Vec<usize> = slots.iter().map(|s| s.2).collect();
    assert_eq!(times, vec![0.0, 1.0, 2.0, 3.0, 10.0]);
    assert_eq!(positions, vec![0, 1, 2, 3, 0]);
    // O quinto item é outro item (não o que saiu), mas ocupou o mesmo slot.
    assert_ne!(slots[4].1, slots[0].1);

    let rep = r.report.block("rk").expect("rack in report");
    assert_eq!(rep.input, 5);
    assert_eq!(rep.output, 5);
    assert_eq!(rep.content_now, 0);
    assert_eq!(rep.staytime.count, 5);
    assert_exact("permanência média", rep.staytime.mean, 10.0);
    assert_exact("permanência mínima", rep.staytime.min, 10.0);
    assert_exact("permanência máxima", rep.staytime.max, 10.0);
}

#[test]
fn dwell_const_gives_exact_staytime_when_downstream_is_free() {
    // Três itens em t=0, 1, 2. Sumidouro sempre livre, permanência Const(10).
    // Saídas em 10, 11 e 12: permanências exatamente 10, 10, 10.
    let r = run(&rack_line(
        50.0,
        "Const(1.0), first_at_zero: true, max_arrivals: Some(3)",
        3,
        1,
        "Const(10.0)",
        "",
    ));
    let rep = r.block("rk").unwrap();
    assert_eq!(rep.output, 3);
    assert_eq!(rep.staytime.count, 3);
    assert_exact("permanência média", rep.staytime.mean, 10.0);
    assert_exact("desvio padrão", rep.staytime.std_dev, 0.0);
}

#[test]
fn full_rack_blocks_upstream_source_with_exact_blocked_fraction() {
    // Porta-paletes 1×1 (uma posição), permanência Const(10), fonte Const(1) com o
    // primeiro item em t=0. Parada em t=100.
    // Fonte: o próximo intervalo começa quando o item sai.
    //   t=0: item 0 criado, entra, sai na hora. Próximo em t=1 (estado generating em [0,1)).
    //   t=1: item 1 criado, não cabe: fonte bloqueada até o item 0 sair em t=10.
    //   t=10: item 1 entra (slot livre) e sai da fonte, próximo em t=11.
    //   t=11: item 2 criado, bloqueado até t=20 (item 1 sai). Assim por diante.
    // Cada período de 10 unidades: 1 de generating e 9 de blocked. Em [0,100]:
    //   blocked = 10 × 9 = 90 e generating = 10 × 1 = 10.
    //   Frações: blocked 0.9, generating 0.1.
    // Entradas no porta-paletes: itens 0..10 (o item 10 entra em t=100): 11.
    // Saídas até t=100: itens 0..9 (saem em 10, 20, ..., 100): 10. Conteúdo final: 1.
    // Porta-paletes sempre com conteúdo > 0, então releasing = 1.0.
    let r = run(&rack_line(
        100.0,
        "Const(1.0), first_at_zero: true",
        1,
        1,
        "Const(10.0)",
        "",
    ));
    let src = r.block("src").unwrap();
    assert_exact("fração blocked", src.states["blocked"], 0.9);
    assert_exact("fração generating", src.states["generating"], 0.1);
    let rk = r.block("rk").unwrap();
    assert_eq!(rk.input, 11);
    assert_eq!(rk.output, 10);
    assert_eq!(rk.content_now, 1);
    assert_exact("fração releasing", rk.states["releasing"], 1.0);
    assert_eq!(rk.states.get("empty").copied().unwrap_or(0.0), 0.0);
}

#[test]
fn random_placement_is_deterministic_and_uses_only_free_slots() {
    // Mesmo cenário do primeiro teste (2×2, cinco itens), com Random.
    // Os quatro primeiros itens ocupam posições distintas, ou seja, uma permutação de
    // {0,1,2,3} (cada um só pode escolher entre as livres). O quinto só pode entrar
    // na única posição livre em t=10: a que o item 0 liberou.
    // Com a mesma semente, a sequência de posições tem de ser idêntica.
    let text = |seed: u64| {
        format!(
            r#"Model(
                name: "aleatorio",
                run: (stop_time: 30.0, seed: {seed}),
                objects: [
                    (name: "src", kind: Source(interarrival: Const(1.0), first_at_zero: true, max_arrivals: Some(5))),
                    (name: "rk", kind: Rack(bays: 2, levels: 2, dwell: Some(Const(10.0)), placement: Random)),
                    (name: "out", kind: Sink),
                ],
                connections: [("src", "rk"), ("rk", "out")],
            )"#
        )
    };
    let a = replay(&text(11));
    let b = replay(&text(11));
    let rk = index_of(&a, "rk");
    let sa = slot_records(&a, rk);
    assert_eq!(sa, slot_records(&b, rk), "mesma semente, mesmas posições");
    assert_eq!(sa.len(), 5);

    let mut first4: Vec<usize> = sa[..4].iter().map(|s| s.2).collect();
    first4.sort_unstable();
    assert_eq!(
        first4,
        vec![0, 1, 2, 3],
        "os quatro primeiros usam slots distintos"
    );
    assert_eq!(sa[4].0, 10.0);
    assert_eq!(sa[4].2, sa[0].2, "o quinto item usa a posição liberada");

    // Com sementes diferentes as posições mudam (a sorte do primeiro item varia).
    let mut firsts: Vec<usize> = (1..=8)
        .map(|s| slot_records(&replay(&text(s)), rk)[0].2)
        .collect();
    firsts.sort_unstable();
    firsts.dedup();
    assert!(
        firsts.len() >= 2,
        "sementes diferentes dão posições diferentes"
    );
}

#[test]
fn breakdown_during_dwell_delays_release_by_exactly_the_repair_time() {
    // Um item entra em t=5 (fonte Const(5), sem primeiro em zero, um item só).
    // Permanência Const(10): deveria sair em 15.
    // Quebra do porta-paletes: falha 8 unidades depois de ligar (MTBF Const(8)), reparo
    // em 4 (MTTR Const(4)).
    //   Relógio do porta-paletes: ligado desde t=0, falha em t=8.
    //   Em t=8 o item já tinha 3 de permanência, restam 7.
    //   Reparo em t=12: os 7 restantes correm, saída em 19.
    //   A próxima falha seria em 12 + 8 = 20, depois da saída. Nenhuma outra quebra antes.
    // Permanência = 19 − 5 = 14 = 10 + MTTR, ou seja, o atraso é exatamente o reparo.
    // Estado down em [8,12) = 4 e em [20,24) = 4 (até a parada em 30): 8/30.
    let text = |extra: &str| {
        format!(
            r#"Model(
                name: "quebra",
                run: (stop_time: 30.0),
                objects: [
                    (name: "src", kind: Source(interarrival: Const(5.0), max_arrivals: Some(1))),
                    (name: "rk", kind: Rack(bays: 1, levels: 1, dwell: Some(Const(10.0))){extra}),
                    (name: "out", kind: Sink),
                ],
                connections: [("src", "rk"), ("rk", "out")],
            )"#
        )
    };
    let r = run(&text(", downtime: (mtbf: Const(8.0), mttr: Const(4.0))"));
    let rk = r.block("rk").unwrap();
    assert_eq!(rk.output, 1);
    assert_exact("permanência com quebra", rk.staytime.mean, 14.0);
    assert_exact("estado down", rk.states["down"], 8.0 / 30.0);

    // Sem quebra, a mesma permanência é exatamente 10.
    let base = run(&text(""));
    assert_exact(
        "permanência sem quebra",
        base.block("rk").unwrap().staytime.mean,
        10.0,
    );
}

#[test]
fn infinite_server_like_rack_matches_littles_law() {
    // M/M/∞: chegadas de Poisson com taxa λ = 1 (fonte Exp de média 1, sem bloqueio,
    // porque a capacidade 100 nunca enche com ~20 itens). Permanência Exp de média 20.
    // Teoria: L = λ·W = 1 × 20 = 20 itens em média no porta-paletes.
    // Tolerância: 5%. Quatro replicações de 20000 unidades, aquecimento de 1000.
    let m = model(
        r#"Model(
            name: "M/M/inf",
            run: (stop_time: 20000.0, warmup: 1000.0, seed: 5, replications: 4),
            objects: [
                (name: "chegada", kind: Source(interarrival: Exp(mean: 1.0))),
                (name: "rk", kind: Rack(bays: 10, levels: 10, dwell: Some(Exp(mean: 20.0)))),
                (name: "saida", kind: Sink),
            ],
            connections: [("chegada", "rk"), ("rk", "saida")],
        )"#,
    );
    let reports = m.run_replications().unwrap();
    let mean_content: f64 = reports
        .iter()
        .map(|r| r.block("rk").unwrap().content_mean)
        .sum::<f64>()
        / reports.len() as f64;
    let mean_stay: f64 = reports
        .iter()
        .map(|r| r.block("rk").unwrap().staytime.mean)
        .sum::<f64>()
        / reports.len() as f64;
    assert_close("L médio (conteúdo)", mean_content, 20.0, 0.05);
    assert_close("W médio (permanência)", mean_stay, 20.0, 0.05);
}

#[test]
fn every_entering_item_has_exactly_one_slot_record_inside_the_rack() {
    // Modelo com bloqueio real: 3×2 = 6 posições, chegadas de Poisson (taxa 1), permanência
    // Uniform(3, 12) e alocação Random. Propriedades verificadas no rastro:
    //   1. cada item que entra no porta-paletes tem exatamente um registro Slot, no mesmo
    //      instante da entrada;
    //   2. toda posição está entre 0 e 5;
    //   3. nunca há dois itens na mesma posição ao mesmo tempo (ocupação reconstruída).
    let r = replay(
        r#"Model(
            name: "rastro",
            run: (stop_time: 300.0, seed: 3),
            objects: [
                (name: "src", kind: Source(interarrival: Exp(mean: 1.0))),
                (name: "rk", kind: Rack(bays: 3, levels: 2, dwell: Some(Uniform(min: 3.0, max: 12.0)), placement: Random)),
                (name: "out", kind: Sink),
            ],
            connections: [("src", "rk"), ("rk", "out")],
        )"#,
    );
    let rk = index_of(&r, "rk");
    let ins = entries(&r, rk);
    let slots = slot_records(&r, rk);
    assert!(ins.len() > 50, "poucos itens entraram: {}", ins.len());
    assert_eq!(ins.len(), slots.len());

    let mut per_item: HashMap<u64, Vec<(f64, usize)>> = HashMap::new();
    for &(t, item, slot) in &slots {
        per_item.entry(item).or_default().push((t, slot));
    }
    for &(t, item) in &ins {
        let recs = per_item.get(&item).expect("item tem registro Slot");
        assert_eq!(recs.len(), 1, "item com {} registros Slot", recs.len());
        assert_eq!(recs[0].0, t, "o Slot é gravado no instante da entrada");
        assert!(
            recs[0].1 < 3 * 2,
            "posição {} fora do porta-paletes",
            recs[0].1
        );
    }

    // Ocupação reconstruída: entradas ocupam, saídas do porta-paletes liberam.
    let mut occupant: Vec<Option<u64>> = vec![None; 6];
    let mut slot_of: HashMap<u64, usize> = HashMap::new();
    for rec in &r.records {
        match rec.event {
            TraceEvent::Slot { item, block, slot } if block == rk => {
                assert!(
                    occupant[slot].is_none(),
                    "posição {slot} ocupada duas vezes"
                );
                occupant[slot] = Some(item);
                slot_of.insert(item, slot);
            }
            TraceEvent::Moved { item, from, .. } if from == rk => {
                let slot = slot_of.remove(&item).expect("item saiu de uma posição");
                assert_eq!(occupant[slot], Some(item));
                occupant[slot] = None;
            }
            _ => {}
        }
    }
    println!("itens que entraram no porta-paletes: {}", ins.len());
}
