//! Análises de saída (estatística avançada P1) sobre modelos de verdade: lei de Little
//! no M/M/1 com o valor teórico, registro de observações com instante sem mudar
//! nenhum resultado, aquecimento por MSER-5, replicações necessárias, médias em lotes e
//! gargalo móvel numa linha determinística com os períodos ativos calculados à mão.

use utsim_core::summarize;
use utsim_model::Model;
use utsim_model::analysis::{
    self, WarmupSeries, batch_means, little, precision, run_for_little, run_for_warmup, run_traced,
    shifting_bottleneck, warmup,
};
use utsim_stats::little::DEFAULT_TOLERANCE;
use utsim_stats::precision::Target;

fn model(text: &str) -> Model {
    let m = Model::parse(text).expect("modelo lê");
    m.validate().expect("modelo válido");
    m
}

/// M/M/1 com λ = 1 e μ = 1/0,8: ρ = 0,8, L = ρ/(1-ρ) = 4, W = 1/(μ-λ) = 4.
const MM1: &str = r#"Model(
    name: "M/M/1",
    run: (stop_time: 40000.0, warmup: 2000.0, seed: 5, replications: 6),
    objects: [
        (name: "chegada", kind: Source(interarrival: Exp(mean: 1.0))),
        (name: "fila", kind: Queue()),
        (name: "maquina", kind: Processor(process: Exp(mean: 0.8))),
        (name: "saida", kind: Sink),
    ],
    connections: [("chegada", "fila"), ("fila", "maquina"), ("maquina", "saida")],
)"#;

fn rel(got: f64, want: f64) -> f64 {
    (got - want).abs() / want
}

#[test]
fn little_fecha_no_mm1_com_o_valor_teorico() {
    let m = model(MM1);
    let (reports, transit) = run_for_little(&m).unwrap();
    assert!(transit.is_none());
    let r = little(&reports, None, 0.95, DEFAULT_TOLERANCE);
    let sys = r.system.expect("o modelo tem sumidouro");
    assert!(rel(sys.l.mean, 4.0) < 0.08, "L = {}", sys.l.mean);
    assert!(rel(sys.w.mean, 4.0) < 0.08, "W = {}", sys.w.mean);
    assert!(rel(sys.lambda.mean, 1.0) < 0.01, "λ = {}", sys.lambda.mean);
    // A lei vale caminho a caminho, a menos dos itens nas pontas da janela.
    assert!(sys.relative < 0.005, "{sys:?}");
    assert!(sys.holds);
    // Por objeto: fila (Lq = ρ²/(1-ρ) = 3,2) e máquina (ρ = 0,8).
    let fila = r.objects.iter().find(|o| o.name == "fila").unwrap();
    assert!(rel(fila.check.l.mean, 3.2) < 0.1, "{:?}", fila.check.l);
    let maq = r.objects.iter().find(|o| o.name == "maquina").unwrap();
    assert!(rel(maq.check.l.mean, 0.8) < 0.03);
    assert!(r.objects.iter().all(|o| o.check.holds), "{:?}", r.objects);
    // Sumidouros ficam de fora da lista por objeto.
    assert!(r.objects.iter().all(|o| o.kind != "sink"));
}

#[test]
fn little_soma_os_itens_em_transito() {
    // Saída da máquina leva 2 + 10/5 = 4 de viagem até o sumidouro: em média λ·4 = 0,5
    // item em trânsito (λ = 1/8), que não está no conteúdo de ninguém.
    let m = model(
        r#"Model(
            name: "transito",
            run: (stop_time: 20000.0, warmup: 500.0, seed: 2, replications: 3),
            objects: [
                (name: "chegada", kind: Source(interarrival: Exp(mean: 8.0))),
                (name: "fila", kind: Queue()),
                (name: "maquina", kind: Processor(process: Exp(mean: 4.0)),
                 transfer: (speed: 5.0, delay: 2.0)),
                (name: "saida", kind: Sink, position: (10.0, 0.0, 0.0)),
            ],
            connections: [("chegada", "fila"), ("fila", "maquina"), ("maquina", "saida")],
        )"#,
    );
    assert!(analysis::has_transfers(&m));
    let (reports, transit) = run_for_little(&m).unwrap();
    let transit = transit.expect("modelo com transfer");
    let r = little(&reports, Some(&transit), 0.95, DEFAULT_TOLERANCE);
    let moving = r.transit.unwrap().mean;
    assert!(rel(moving, 0.5) < 0.05, "em trânsito: {moving}");
    let sys = r.system.unwrap();
    assert!(sys.relative < 0.01, "{sys:?}");
    // Sem o trânsito, L fica 0,5 abaixo de λW e a verificação acusa.
    let sem = little(&reports, None, 0.95, DEFAULT_TOLERANCE)
        .system
        .unwrap();
    assert!(!sem.holds, "{sem:?}");
}

#[test]
fn observacoes_com_instante_nao_mudam_resultados() {
    let m = model(MM1);
    let plain = m.run_replications().unwrap();
    let timed = m
        .run_replications_timed(&[
            ("saida".into(), "time_in_system".into()),
            ("fila".into(), "staytime".into()),
        ])
        .unwrap();
    let (a, b) = (summarize(&plain), summarize(&timed));
    assert_eq!(a.len(), b.len());
    for (k, s) in &a {
        assert_eq!(s.mean.to_bits(), b[k].mean.to_bits(), "{k}");
        assert_eq!(s.half_width.to_bits(), b[k].half_width.to_bits(), "{k}");
    }
    for (p, t) in plain.iter().zip(&timed) {
        assert_eq!(p.events, t.events);
        let sink = t.block("saida").unwrap();
        let obs = sink.extra["time_in_system"].observations.as_ref().unwrap();
        assert_eq!(obs.len() as u64, sink.extra["time_in_system"].count);
        // Depois do aquecimento, em ordem de tempo, e com a mesma média.
        assert!(obs.first().unwrap()[0] >= 2000.0);
        assert!(obs.windows(2).all(|w| w[0][0] <= w[1][0]));
        let mean = obs.iter().map(|o| o[1]).sum::<f64>() / obs.len() as f64;
        assert!(rel(mean, sink.extra["time_in_system"].mean) < 1e-9);
        let fila = &t.block("fila").unwrap().staytime;
        assert_eq!(fila.observations.as_ref().unwrap().len() as u64, fila.count);
        // O que não foi pedido não guarda nada (nem aparece no JSON).
        assert!(t.block("maquina").unwrap().staytime.observations.is_none());
        assert!(
            p.block("saida").unwrap().extra["time_in_system"]
                .observations
                .is_none()
        );
    }
    let json = serde_json::to_string(&plain[0]).unwrap();
    assert!(!json.contains("observations"));
}

#[test]
fn aquecimento_mser5_no_wip_de_um_sistema_que_parte_vazio() {
    // M/M/1 com ρ = 0,9 partindo vazio: o WIP sobe até L = 9 nas primeiras centenas
    // de unidades de tempo.
    let m = model(
        r#"Model(
            name: "transiente",
            run: (stop_time: 20000.0, warmup: 0.0, seed: 9, replications: 10,
                  sample_interval: 20.0),
            objects: [
                (name: "chegada", kind: Source(interarrival: Exp(mean: 1.0))),
                (name: "fila", kind: Queue()),
                (name: "maquina", kind: Processor(process: Exp(mean: 0.9))),
                (name: "saida", kind: Sink),
            ],
            connections: [("chegada", "fila"), ("fila", "maquina"), ("maquina", "saida")],
        )"#,
    );
    let reports = run_for_warmup(&m, &WarmupSeries::Wip).unwrap();
    let w = warmup(&reports, &WarmupSeries::Wip, 5);
    assert_eq!(w.times[0], 0.0);
    assert_eq!(w.mean[0], 0.0);
    assert_eq!(w.welch.len(), w.mean.len() - 5);
    let mser = w.mser.as_ref().unwrap();
    assert!(!mser.too_short, "{mser:?}");
    let cut = w.suggested.unwrap();
    assert!(cut > 0.0 && cut < 5000.0, "corte em {cut}");
    // O que sobra tem média perto da teórica.
    let kept = &w.mean[mser.cut..];
    let mean = kept.iter().sum::<f64>() / kept.len() as f64;
    assert!(rel(mean, 9.0) < 0.2, "WIP médio {mean}");

    // Pela medida: tempo no sistema na ordem das saídas.
    let series = WarmupSeries::parse("saida.time_in_system").unwrap();
    let reports = run_for_warmup(&m, &series).unwrap();
    let w = warmup(&reports, &series, 25);
    assert!(w.mean.len() > 10_000);
    assert!(w.suggested.is_some());
    assert!(run_for_warmup(&m, &WarmupSeries::parse("nada.x").unwrap()).is_err());
}

#[test]
fn replicacoes_necessarias_e_lotes() {
    let mut m = model(MM1);
    m.run.stop_time = 10000.0;
    m.run.replications = 4;
    let reports = m.run_replications().unwrap();
    let metrics = analysis::default_precision_metrics(&reports);
    assert!(metrics.contains(&"maquina.utilization".to_string()));
    assert!(metrics.contains(&"saida.time_in_system.mean".to_string()));
    let rows = precision(&reports, &metrics, Target::Relative(0.01), 0.95);
    for (k, p) in &rows {
        let n = p.needed.unwrap();
        // Com o desvio atual, n replicações atingem o alvo e n - 1 não.
        let s = p.current.std_dev;
        let hw = |n: usize| utsim_stats::t_quantile(0.95, n - 1) * s / (n as f64).sqrt();
        assert!(hw(n) <= p.target, "{k}");
        assert!(n == 2 || hw(n - 1) > p.target, "{k}");
        assert_eq!(p.more.unwrap(), n.saturating_sub(4));
    }

    // Uma rodada longa só: médias em lotes do tempo no sistema cobrem W = 4.
    let mut long = model(MM1);
    long.run.replications = 1;
    long.run.stop_time = 200_000.0;
    let reports = long
        .run_replications_timed(&[("saida".into(), "time_in_system".into())])
        .unwrap();
    let b = batch_means(&reports[0], "saida", "time_in_system", 0.95).unwrap();
    assert!(b.independent, "{b:?}");
    assert!(b.batch_size > 1);
    assert!(
        (b.summary.mean - 4.0).abs() <= 1.5 * b.summary.half_width,
        "{b:?}"
    );
    // Sem observações com instante não há lotes.
    let plain = long.run_replications().unwrap();
    assert!(batch_means(&plain[0], "saida", "time_in_system", 0.95).is_none());
}

#[test]
fn gargalo_movel_numa_linha_deterministica() {
    // Chegadas a cada 1, "a" leva 2 e "b" leva 3, sem fila entre eles. Com s o início
    // da primeira peça em "a":
    // - "a" processa [s, s+2], entrega a "b" e pega a próxima no mesmo instante: o
    //   primeiro período ativo de "a" é [s, s+4] (depois fica bloqueada até s+5);
    // - "b" trabalha sem parar de s+2 até o fim (300): um único período ativo;
    // - depois, "a" trabalha 2 em cada ciclo de 3, dentro do período mais longo de "b".
    // Gargalo momentâneo: "a" em [s, s+2] (só ela ativa), "b" de s+2 em diante. Os
    // períodos de "a" e "b" se sobrepõem em [s+2, s+4]: gargalo compartilhado por 2.
    // Logo "a": único 2, compartilhado 2; "b": compartilhado 2, único 300 - (s+2) - 2.
    let m = model(
        r#"Model(
            name: "linha",
            run: (stop_time: 300.0, seed: 1, replications: 2),
            objects: [
                (name: "chegada", kind: Source(interarrival: Const(1.0))),
                (name: "a", kind: Processor(process: Const(2.0))),
                (name: "b", kind: Processor(process: Const(3.0))),
                (name: "saida", kind: Sink),
            ],
            connections: [("chegada", "a"), ("a", "b"), ("b", "saida")],
        )"#,
    );
    let traced = run_traced(&m).unwrap();
    let shares = shifting_bottleneck(&traced, 0.95);
    assert_eq!(shares[0].name, "b");
    let a = shares.iter().find(|s| s.name == "a").unwrap();
    let b = &shares[0];
    let first_processing = |block: usize| {
        traced[0]
            .1
            .iter()
            .find(|r| {
                matches!(r.event, utsim_core::TraceEvent::State { block: b, state }
                    if b == block && state == utsim_core::State::Processing)
            })
            .unwrap()
            .t
    };
    let s = first_processing(1);
    assert_eq!(first_processing(2), s + 2.0);
    let close = |got: f64, want: f64| assert!((got - want).abs() < 1e-12, "{got} vs {want}");
    close(a.sole.mean, 2.0 / 300.0);
    close(a.shifting.mean, 2.0 / 300.0);
    close(b.shifting.mean, 2.0 / 300.0);
    close(b.sole.mean, (300.0 - (s + 2.0) - 2.0) / 300.0);
    // Determinística: as duas replicações dão o mesmo.
    assert_eq!(b.sole.half_width, 0.0);
    // O mesmo recurso é o gargalo pela ocupação.
    let stats = analysis::object_stats(&m, &m.run_replications().unwrap());
    assert_eq!(stats[analysis::bottleneck(&stats).unwrap()].name, "b");
}
