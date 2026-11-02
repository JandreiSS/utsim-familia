//! Os modelos de referência dos estudos de caso (`docs/estudos_de_caso/*/gabarito/modelo.ron`)
//! continuam válidos e os números principais continuam dentro das faixas citadas nas
//! resoluções (`gabarito/resolucao.md`). As faixas são largas de propósito: os testes rodam
//! poucas replicações e rodadas curtas.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use utsim_core::stats::Summary;
use utsim_core::summarize;
use utsim_model::Model;
use utsim_model::experiment::{Experiment, apply_patches};

fn pasta(caso: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../docs/estudos_de_caso")
        .join(caso)
        .join("gabarito")
}

/// Roda o modelo de referência de um caso com poucas replicações e tempo curto, aplicando
/// `patches` (mesmas chaves dos cenários de experimento).
fn rodar(
    caso: &str,
    reps: u32,
    stop: f64,
    warmup: f64,
    patches: &[(&str, &str)],
) -> BTreeMap<String, Summary> {
    let caminho = pasta(caso).join("modelo.ron");
    let mut model = Model::load(&caminho).unwrap_or_else(|e| panic!("{caso}: {e}"));
    model.run.replications = reps;
    model.run.stop_time = stop;
    model.run.warmup = warmup;
    let set: BTreeMap<String, String> = patches
        .iter()
        .map(|(k, v)| (k.to_string(), v.to_string()))
        .collect();
    let model = apply_patches(&model, &set).unwrap_or_else(|e| panic!("{caso}: {e}"));
    summarize(
        &model
            .run_replications()
            .unwrap_or_else(|e| panic!("{caso}: {e}")),
    )
}

fn media(s: &BTreeMap<String, Summary>, chave: &str) -> f64 {
    s.get(chave)
        .unwrap_or_else(|| panic!("métrica ausente: {chave}"))
        .mean
}

fn entre(o_que: &str, x: f64, lo: f64, hi: f64) {
    assert!(
        (lo..=hi).contains(&x),
        "{o_que} = {x}, fora da faixa [{lo}, {hi}] citada na resolução"
    );
}

const DIA: f64 = 86400.0;

#[test]
fn todo_modelo_de_referencia_e_valido() {
    for caso in [
        "01-linha-de-montagem",
        "02-centro-de-distribuicao",
        "03-clinica-com-turnos",
    ] {
        let model =
            Model::load(pasta(caso).join("modelo.ron")).unwrap_or_else(|e| panic!("{caso}: {e}"));
        model.validate().unwrap_or_else(|e| panic!("{caso}: {e}"));
    }
    // Variante da clínica com 15% mais pacientes (usada pela otimização da pergunta 4).
    let variante = pasta("03-clinica-com-turnos").join("modelo_demanda_15.ron");
    Model::load(&variante)
        .and_then(|m| m.validate())
        .unwrap_or_else(|e| panic!("modelo_demanda_15: {e}"));
    // The clinic with a single source (arrivals by period) and calendars; compared with
    // modelo.ron in tests/chegadas_e_turnos.rs.
    let por_periodo = pasta("03-clinica-com-turnos").join("modelo_por_periodo.ron");
    Model::load(&por_periodo)
        .and_then(|m| m.validate())
        .unwrap_or_else(|e| panic!("modelo_por_periodo: {e}"));
}

#[test]
fn todo_experimento_de_referencia_roda() {
    for caso in [
        "01-linha-de-montagem",
        "02-centro-de-distribuicao",
        "03-clinica-com-turnos",
    ] {
        let caminho = pasta(caso).join("experimento.ron");
        let mut exp = Experiment::load(&caminho).unwrap_or_else(|e| panic!("{caso}: {e}"));
        exp.replications = Some(2);
        for sc in &mut exp.scenarios {
            // Rodada curta: dois dias (clínica: três) com o primeiro descartado.
            let (stop, warmup) = if caso.starts_with("03") {
                ("4320.0", "1440.0")
            } else if caso.starts_with("02") {
                ("40000.0", "14400.0")
            } else {
                ("259200.0", "86400.0")
            };
            sc.set
                .entry("run.stop_time".into())
                .or_insert_with(|| stop.into());
            sc.set
                .entry("run.warmup".into())
                .or_insert_with(|| warmup.into());
        }
        let resultado = exp.run().unwrap_or_else(|e| panic!("{caso}: {e}"));
        assert!(resultado.scenarios.len() >= 2, "{caso}");
    }
}

#[test]
fn caso_1_linha_de_montagem() {
    // Janela de 4 dias (5 dias de rodada, 1 de aquecimento); 3 inspetores e buffers de 2.
    let s = rodar("01-linha-de-montagem", 6, 5.0 * DIA, DIA, &[]);
    let por_dia = media(&s, "expedicao.input") / 4.0;
    entre("peças aprovadas por dia", por_dia, 760.0, 910.0);
    // Cerca de 12% das peças inspecionadas vão ao retrabalho.
    let insp = media(&s, "inspecao.output");
    let retrab = media(&s, "retrabalho.input");
    entre("fração de retrabalho", retrab / insp, 0.09, 0.15);
    // A solda passa uns 18% do tempo parada (quebra contada no relógio, inclusive à noite).
    entre("solda parada", media(&s, "solda.state.down"), 0.10, 0.28);
    // Com 3 bancadas e 3 inspetores, perto de 1,8 bancada ocupada.
    entre(
        "bancadas ocupadas",
        media(&s, "inspecao.content.mean"),
        1.4,
        2.3,
    );

    // Com 2 inspetores a inspeção vira o gargalo e a vazão cai.
    let dois = rodar(
        "01-linha-de-montagem",
        6,
        5.0 * DIA,
        DIA,
        &[("inspetores.team", "[\"inspetor_1\", \"inspetor_2\"]")],
    );
    assert!(
        media(&dois, "expedicao.input") < media(&s, "expedicao.input") - 100.0,
        "dois inspetores deveriam produzir bem menos que três"
    );

    // Buffer antes da solda ajuda; depois da solda não muda quase nada.
    let antes = rodar(
        "01-linha-de-montagem",
        6,
        5.0 * DIA,
        DIA,
        &[("fila_solda.capacity", "8")],
    );
    let depois = rodar(
        "01-linha-de-montagem",
        6,
        5.0 * DIA,
        DIA,
        &[("fila_cal.capacity", "8")],
    );
    let base = media(&s, "expedicao.input");
    assert!(
        media(&antes, "expedicao.input") > base + 40.0,
        "buffer antes da solda deveria elevar a vazão"
    );
    assert!(
        (media(&depois, "expedicao.input") - base).abs() < 40.0,
        "buffer depois da solda não deveria mudar a vazão"
    );
}

#[test]
fn caso_2_centro_de_distribuicao() {
    // Janela de 24 h (28 h de rodada, 4 h de aquecimento).
    let s = rodar("02-centro-de-distribuicao", 6, 100800.0, 14400.0, &[]);
    // Um caminhão a cada 29 min: uns 50 em 24 h, 10 paletes cada.
    entre(
        "caminhões em 24 h",
        media(&s, "caminhoes.output"),
        38.0,
        62.0,
    );
    // Cada palete vira 6 pedidos.
    let paletes = media(&s, "quebra.input");
    let pedidos = media(&s, "quebra.output");
    entre("pedidos por palete", pedidos / paletes, 5.9, 6.1);
    // O porta-paletes de 96 posições fica com uns 40 paletes.
    entre(
        "paletes guardados",
        media(&s, "porta_paletes.content.mean"),
        25.0,
        60.0,
    );
    // Espera do palete na recepção com 3 empilhadeiras: ordem de 4 min.
    entre(
        "espera na recepção",
        media(&s, "recebimento.staytime.mean"),
        100.0,
        600.0,
    );
    // A quarta empilhadeira nunca é usada na equipe de 3.
    assert_eq!(media(&s, "emp_4.output"), 0.0);
    // Com 5 pickers a fila de pedidos é longa; com 10 ela cai muito.
    let cinco = media(&s, "fila_pedidos.staytime.mean");
    let team10 = "[\"picker_1\", \"picker_2\", \"picker_3\", \"picker_4\", \"picker_5\", \
                  \"picker_6\", \"picker_7\", \"picker_8\", \"picker_9\", \"picker_10\"]";
    let dez = rodar(
        "02-centro-de-distribuicao",
        6,
        100800.0,
        14400.0,
        &[("pickers.team", team10)],
    );
    assert!(
        media(&dez, "fila_pedidos.staytime.mean") < cinco / 3.0,
        "10 pickers deveriam reduzir muito a fila de pedidos"
    );
    // Embalagem perto do picking com 6 pickers também resolve a fila de pedidos.
    let team6 =
        "[\"picker_1\", \"picker_2\", \"picker_3\", \"picker_4\", \"picker_5\", \"picker_6\"]";
    let perto = rodar(
        "02-centro-de-distribuicao",
        6,
        100800.0,
        14400.0,
        &[
            ("pickers.team", team6),
            ("fila_embalagem.position", "(60.0, -42.0, 0.0)"),
            ("embalagem.position", "(60.0, -44.0, 0.0)"),
        ],
    );
    assert!(
        media(&perto, "fila_pedidos.staytime.mean") < cinco / 3.0,
        "embalagem perto com 6 pickers deveria reduzir muito a fila de pedidos"
    );
}

/// Investigação do piloto (docs/estudos_de_caso/README.md): a fila da embalagem cresce com
/// mais pickers, com as mesas "ociosas" boa parte do tempo. Não é erro do motor: a fila só
/// existe com as 5 mesas cheias, a mesa não fica presa na esteira de saída, e a fila vem de
/// rajadas (a separação, com mais pickers, entrega mais caixas por segundo do que as mesas
/// embalam enquanto há paletes esperando).
#[test]
fn caso_2_fila_da_embalagem_vem_de_rajadas_e_nao_do_motor() {
    let team8 = "[\"picker_1\", \"picker_2\", \"picker_3\", \"picker_4\", \"picker_5\", \
                 \"picker_6\", \"picker_7\", \"picker_8\"]";
    let perto = [
        ("pickers.team", team8),
        ("fila_embalagem.position", "(60.0, -42.0, 0.0)"),
        ("embalagem.position", "(60.0, -44.0, 0.0)"),
    ];
    let caminho = pasta("02-centro-de-distribuicao").join("modelo.ron");
    let mut model = Model::load(&caminho).unwrap();
    model.run.replications = 1;
    model.run.stop_time = 72000.0;
    model.run.warmup = 14400.0;
    model.run.sample_interval = Some(5.0);
    let set: BTreeMap<String, String> = perto
        .iter()
        .map(|(k, v)| (k.to_string(), v.to_string()))
        .collect();
    let model = apply_patches(&model, &set).unwrap();
    let report = model.run_replications().unwrap().remove(0);
    let fila = report.block("fila_embalagem").unwrap();
    let mesas = report.block("embalagem").unwrap();
    // A fila só tem caixas quando as 5 mesas estão cheias (o motor não deixa mesa parada).
    let mut com_fila = 0;
    for (f, m) in fila.series.iter().zip(&mesas.series) {
        if f.content > 0 {
            com_fila += 1;
            assert_eq!(m.content, 5, "t={}: fila com caixas e mesa livre", f.t);
        }
    }
    assert!(com_fila > 0, "o cenário deveria formar fila");
    // A esteira de saída não segura a mesa: nada fica "pronto e preso" na embalagem.
    let m = report.metrics();
    assert_eq!(m["embalagem.servers.blocked"], 0.0);
    assert!((m["embalagem.servers.busy"] - mesas.content_mean).abs() < 1e-9);
    // Ocupação por mesa moderada, e bem diferente do estado "idle" da embalagem inteira.
    let ocupacao = m["embalagem.utilization"];
    assert!((0.3..0.7).contains(&ocupacao), "ocupação {ocupacao}");
    assert!(
        m["embalagem.state.idle"] > 0.2,
        "sem mesa trabalhando é frequente"
    );
}

#[test]
fn caso_3_clinica_com_turnos() {
    // Janela de 10 dias (11 dias de rodada, 1 de aquecimento); escala atual.
    let s = rodar("03-clinica-com-turnos", 6, 11.0 * 1440.0, 1440.0, &[]);
    // 220 pacientes por dia: as seis faixas de chegada somam isso.
    let por_dia = media(&s, "saida.input") / 10.0;
    entre("pacientes por dia", por_dia, 200.0, 240.0);
    // Urgentes passam à frente dos normais.
    assert!(media(&s, "fila_urgente.staytime.mean") < media(&s, "fila_normal.staytime.mean"));
    // Escala atual folgada: espera de normais abaixo de 15 min.
    assert!(media(&s, "fila_normal.staytime.mean") < 15.0);
    // Cerca de 12% dos pacientes são urgentes.
    let urg = media(&s, "fila_urgente.input");
    let nor = media(&s, "fila_normal.input");
    entre("fração de urgentes", urg / (urg + nor), 0.09, 0.15);

    // Escala enxuta demais estoura a espera dos normais.
    let enxuta = rodar(
        "03-clinica-com-turnos",
        6,
        11.0 * 1440.0,
        1440.0,
        &[
            ("medicos_S1.capacity", "4"),
            ("medicos_S2.capacity", "4"),
            ("medicos_S3.capacity", "2"),
            ("triagem_S1.capacity", "2"),
            ("triagem_S3.capacity", "1"),
        ],
    );
    assert!(media(&enxuta, "fila_normal.staytime.mean") > 60.0);

    // Escala ótima: cumpre a meta de 15 min.
    let otima = rodar(
        "03-clinica-com-turnos",
        6,
        11.0 * 1440.0,
        1440.0,
        &[
            ("medicos_S1.capacity", "5"),
            ("medicos_S2.capacity", "5"),
            ("medicos_S3.capacity", "3"),
        ],
    );
    entre(
        "espera dos normais na escala ótima",
        media(&otima, "fila_normal.staytime.mean"),
        6.0,
        20.0,
    );
}
