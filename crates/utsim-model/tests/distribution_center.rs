//! Validação com um centro de distribuição (`examples/centro_de_distribuicao.ron`).
//!
//! O modelo junta quase todos os recursos do motor: Separator (caminhão vira 6 paletes),
//! Conveyor com e sem acúmulo, Processor com operador de setup (equipe de duas pessoas),
//! Dispatcher RoundRobin de empilhadeiras com casa, rede de caminhos, Rack e turnos.
//!
//! Os testes checam propriedades que valem para qualquer parâmetro estável (conservação,
//! Little, utilização, divisão de trabalho, determinismo e rastro), não um número fixo.
//! Valores medidos aparecem na saída com `cargo test -- --nocapture`.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use utsim_core::{RunReport, TraceEvent};
use utsim_model::Model;
use utsim_model::replay::Replay;

/// Chegada média de caminhões (s) e paletes por caminhão (Split(6)).
const TRUCK_MEAN: f64 = 1200.0;
const PALLETS_PER_TRUCK: f64 = 6.0;
const BAYS: usize = 6;
const LEVELS: usize = 6;
/// Tempo de parada das rodadas longas, usadas para estatística de vazão e Little.
const LONG_STOP: f64 = 1_000_000.0;
const LONG_REPLICATIONS: u32 = 4;

fn model_path() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/centro_de_distribuicao.ron")
}

/// Modelo do exemplo, já validado.
fn load() -> Model {
    let m = Model::load(model_path()).expect("o modelo lê");
    m.validate().expect("o modelo é válido");
    m
}

fn index_of(blocks: &[utsim_model::replay::ReplayBlock], name: &str) -> usize {
    blocks
        .iter()
        .position(|b| b.name == name)
        .expect("bloco existe")
}

fn state(report: &RunReport, block: &str, name: &str) -> f64 {
    report
        .block(block)
        .expect("bloco no relatório")
        .states
        .get(name)
        .copied()
        .unwrap_or(0.0)
}

#[test]
fn modelo_valida_e_roda_sem_erros() {
    let m = load();
    let reps = m.run_replications().expect("roda sem erro");
    assert_eq!(reps.len(), m.run.replications as usize);
    for r in &reps {
        assert!(r.events > 0, "a replicação processou eventos");
        assert!(
            r.block("expedicao").expect("sink").input > 0,
            "paletes chegaram à expedição"
        );
    }
}

#[test]
fn conservacao_por_replicacao_com_aquecimento_zero() {
    let mut m = load();
    m.run.warmup = 0.0;
    for rep in 0..m.run.replications {
        let r = m.replay(rep).expect("replay roda");
        let report = &r.report;
        let idx = |name: &str| index_of(&r.blocks, name);

        // Contagens de criação e destruição vindas do rastro. Só o Source e o Separator
        // criam itens (o Separator cria cópias ao dividir o caminhão), e só o Sink destrói.
        let mut created: HashMap<usize, u64> = HashMap::new();
        let mut created_total = 0u64;
        let mut destroyed_total = 0u64;
        for rec in &r.records {
            match rec.event {
                TraceEvent::Created { block, .. } => {
                    *created.entry(block).or_default() += 1;
                    created_total += 1;
                }
                TraceEvent::Destroyed { .. } => destroyed_total += 1,
                _ => {}
            }
        }
        let src = idx("chegada_caminhoes");
        let sep = idx("descarga");
        let sink = idx("expedicao");
        let created_src = created.get(&src).copied().unwrap_or(0) as f64;
        let created_sep = created.get(&sep).copied().unwrap_or(0) as f64;

        // Identidade por bloco: entradas + criadas − saídas − destruídas = conteúdo atual.
        // Para blocos comuns isso é exatamente entradas − saídas = conteúdo.
        let mut content_total = 0.0;
        for b in &report.blocks {
            let content = b.content_now as f64;
            content_total += content;
            let lhs = match b.name.as_str() {
                "chegada_caminhoes" => created_src - b.output as f64,
                "descarga" => b.input as f64 + created_sep - b.output as f64,
                "expedicao" => b.input as f64 - destroyed_total as f64,
                _ => b.input as f64 - b.output as f64,
            };
            assert_eq!(
                lhs, content,
                "rep {rep}, {}: entradas − saídas ≠ conteúdo ({} − {} vs {content})",
                b.name, b.input, b.output
            );
        }
        // Todo item criado está no sink, foi destruído, ou ainda está no sistema
        // (inclusive nos transportadores, que carregam os paletes).
        let sink_input = report.block("expedicao").expect("sink").input;
        assert_eq!(
            destroyed_total, sink_input,
            "rep {rep}: destruídos ≠ chegados ao sink"
        );
        assert_eq!(
            created_total as f64,
            destroyed_total as f64 + content_total,
            "rep {rep}: criados ≠ destruídos + no sistema"
        );
        println!(
            "rep {rep}: caminhões criados {created_src}, cópias do Separator {created_sep}, \
             sink {sink_input}, no sistema {content_total}"
        );
        assert!(created_src > 0.0 && sink > 0);
    }
}

#[test]
fn vazao_e_little_do_porta_paletes_em_rodada_longa() {
    let mut m = load();
    m.run.stop_time = LONG_STOP;
    m.run.replications = LONG_REPLICATIONS;
    let reps = m.run_replications().expect("roda");
    let window = (LONG_STOP - m.run.warmup) * reps.len() as f64;

    let sink_input: f64 = reps
        .iter()
        .map(|r| r.block("expedicao").expect("sink").input as f64)
        .sum();
    let throughput = sink_input / window;
    let expected = PALLETS_PER_TRUCK / TRUCK_MEAN;
    let err = (throughput - expected).abs() / expected;
    println!(
        "vazão no sink: {throughput:.6} paletes/s, esperado λ·paletes = {expected:.6} \
         (erro {:.2}%)",
        err * 100.0
    );
    assert!(err <= 0.03, "vazão {throughput} vs {expected}");

    // Little no porta-paletes: L = λ·W, com λ = saídas/janela e W = permanência média.
    let rack_out: f64 = reps
        .iter()
        .map(|r| r.block("porta_paletes").expect("rack").output as f64)
        .sum();
    let lambda = rack_out / window;
    let l = reps
        .iter()
        .map(|r| r.block("porta_paletes").expect("rack").content_mean)
        .sum::<f64>()
        / reps.len() as f64;
    let w = reps
        .iter()
        .map(|r| r.block("porta_paletes").expect("rack").staytime.mean)
        .sum::<f64>()
        / reps.len() as f64;
    let little_err = (l - lambda * w).abs() / (lambda * w);
    println!(
        "porta-paletes: L = {l:.3}, λ·W = {lambda:.6} × {w:.1} = {:.3} (erro {:.2}%)",
        lambda * w,
        little_err * 100.0
    );
    assert!(
        little_err <= 0.05,
        "Little do rack: L {l} vs λW {}",
        lambda * w
    );
}

#[test]
fn utilizacao_entre_zero_e_noventa_por_cento() {
    let m = load();
    for (rep, r) in m.run_replications().expect("roda").iter().enumerate() {
        for b in &r.blocks {
            let busy = match b.kind.as_str() {
                "processor" => state(r, &b.name, "processing") + state(r, &b.name, "setup"),
                "transporter" => 1.0 - state(r, &b.name, "idle"),
                _ => continue,
            };
            println!("rep {rep}, {}: ocupado {:.4}", b.name, busy);
            assert!(
                busy > 0.0 && busy < 0.9,
                "rep {rep}, {}: fração ocupada {busy} fora de (0; 0,9)",
                b.name
            );
        }
    }
}

#[test]
fn empilhadeiras_dividem_as_viagens_no_round_robin() {
    let m = load();
    let r = m.replay(0).expect("replay roda");
    let idx = |name: &str| index_of(&r.blocks, name);
    let inspection = idx("inspecao");
    let forklifts = [idx("empilhadeira_1"), idx("empilhadeira_2")];
    // Cada palete que sai da inspeção entra numa empilhadeira: conta essas entradas.
    let mut trips = [0u64; 2];
    for rec in &r.records {
        if let TraceEvent::Moved { from, to, .. } = rec.event {
            if from == inspection {
                if let Some(k) = forklifts.iter().position(|&f| f == to) {
                    trips[k] += 1;
                }
            }
        }
    }
    let total: u64 = trips.iter().sum();
    assert!(total > 0, "houve viagens");
    for (k, &n) in trips.iter().enumerate() {
        let share = n as f64 / total as f64;
        println!(
            "empilhadeira_{}: {n} viagens ({:.1}%)",
            k + 1,
            share * 100.0
        );
        assert!(
            share >= 0.35,
            "empilhadeira_{}: fatia {share:.3} < 0,35",
            k + 1
        );
    }
}

#[test]
fn mesma_semente_gera_mesmas_metricas() {
    let m = load();
    let a = m.run_replications().expect("roda");
    let b = m.run_replications().expect("roda de novo");
    assert_eq!(a.len(), b.len());
    for (x, y) in a.iter().zip(&b) {
        assert_eq!(x.metrics(), y.metrics(), "métricas iguais na mesma semente");
        assert_eq!(x.events, y.events, "mesmo número de eventos");
    }
}

#[test]
fn rastro_tem_viagens_slots_e_posicoes_validas() {
    let mut m = load();
    m.run.stop_time = 3000.0;
    m.run.warmup = 0.0;
    let r: Replay = m.replay(0).expect("replay roda");
    let count = |f: &dyn Fn(&TraceEvent) -> bool| r.records.iter().filter(|x| f(&x.event)).count();
    let travels = count(&|e| matches!(e, TraceEvent::Travel { .. }));
    let slots = count(&|e| matches!(e, TraceEvent::Slot { .. }));
    let moved = count(&|e| matches!(e, TraceEvent::Moved { .. }));
    println!("rastro (3000 s): {travels} viagens, {slots} posições, {moved} transferências");
    assert!(travels > 0, "há viagens de empilhadeira");
    assert!(slots > 0, "há posições ocupadas no porta-paletes");
    assert!(moved > 0, "há transferências");
    for rec in &r.records {
        if let TraceEvent::Slot { slot, .. } = rec.event {
            assert!(
                slot < BAYS * LEVELS,
                "posição {slot} fora do porta-paletes ({} posições)",
                BAYS * LEVELS
            );
        }
    }
}
