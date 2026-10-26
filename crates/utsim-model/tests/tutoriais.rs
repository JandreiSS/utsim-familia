//! Os modelos dos tutoriais do manual (`examples/tutorial_*.ron`) continuam válidos e os
//! resultados principais continuam dentro das faixas citadas no texto dos tutoriais
//! (`docs/manual/src/tutoriais/`). As faixas são largas de propósito: os testes rodam poucas
//! replicações e rodadas curtas, e o texto cita as rodadas completas.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use utsim_core::stats::Summary;
use utsim_core::summarize;
use utsim_model::Model;
use utsim_model::experiment::{Experiment, apply_patches};

fn examples() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples")
}

/// Roda um tutorial com poucas replicações e tempo curto, aplicando `patches` (mesmas
/// chaves dos cenários de experimento).
fn run(file: &str, reps: u32, stop: f64, patches: &[(&str, &str)]) -> BTreeMap<String, Summary> {
    let mut model = Model::load(examples().join(file)).unwrap_or_else(|e| panic!("{file}: {e}"));
    model.run.replications = reps;
    model.run.stop_time = model.run.stop_time.min(stop);
    model.run.warmup = model.run.warmup.min(model.run.stop_time / 10.0);
    let set: BTreeMap<String, String> = patches
        .iter()
        .map(|(k, v)| (k.to_string(), v.to_string()))
        .collect();
    let model = apply_patches(&model, &set).unwrap_or_else(|e| panic!("{file}: {e}"));
    summarize(
        &model
            .run_replications()
            .unwrap_or_else(|e| panic!("{file}: {e}")),
    )
}

fn mean(s: &BTreeMap<String, Summary>, key: &str) -> f64 {
    s.get(key)
        .unwrap_or_else(|| panic!("métrica ausente: {key}"))
        .mean
}

fn between(what: &str, x: f64, lo: f64, hi: f64) {
    assert!(
        (lo..=hi).contains(&x),
        "{what} = {x}, fora da faixa [{lo}, {hi}] citada no tutorial"
    );
}

#[test]
fn every_tutorial_model_is_valid_and_runs() {
    let mut found = 0;
    for entry in std::fs::read_dir(examples()).unwrap() {
        let path = entry.unwrap().path();
        let name = path.file_name().unwrap().to_string_lossy().into_owned();
        if !(name.starts_with("tutorial_") && name.ends_with(".ron")) {
            continue;
        }
        let mut model = Model::load(&path).unwrap_or_else(|e| panic!("{name}: {e}"));
        model.validate().unwrap_or_else(|e| panic!("{name}: {e}"));
        model.run.replications = 2;
        model.run.stop_time = model.run.stop_time.min(5000.0);
        model.run.warmup = model.run.warmup.min(model.run.stop_time / 10.0);
        let reports = model
            .run_replications()
            .unwrap_or_else(|e| panic!("{name}: {e}"));
        assert_eq!(reports.len(), 2, "{name}");
        found += 1;
    }
    assert!(found >= 6, "só {found} modelos de tutorial encontrados");
}

#[test]
fn every_tutorial_experiment_runs() {
    let mut found = 0;
    for entry in std::fs::read_dir(examples()).unwrap() {
        let path = entry.unwrap().path();
        let name = path.file_name().unwrap().to_string_lossy().into_owned();
        if !(name.starts_with("experimento_tutorial_") && name.ends_with(".ron")) {
            continue;
        }
        let mut exp = Experiment::load(&path).unwrap_or_else(|e| panic!("{name}: {e}"));
        exp.replications = Some(2);
        for sc in &mut exp.scenarios {
            sc.set
                .entry("run.stop_time".into())
                .or_insert_with(|| "5000.0".into());
            sc.set
                .entry("run.warmup".into())
                .or_insert_with(|| "500.0".into());
        }
        let result = exp.run().unwrap_or_else(|e| panic!("{name}: {e}"));
        assert!(result.scenarios.len() >= 2, "{name}");
        found += 1;
    }
    assert!(
        found >= 5,
        "só {found} experimentos de tutorial encontrados"
    );
}

#[test]
fn tutorial_01_matches_the_mm1_formulas() {
    // Teoria: rho 0,8, Lq 3,2, Wq 16, W 20.
    let s = run("tutorial_01_caixa.ron", 4, 40000.0, &[]);
    between("rho", mean(&s, "caixa.content.mean"), 0.75, 0.85);
    between("Lq", mean(&s, "fila.content.mean"), 2.4, 4.2);
    between("Wq", mean(&s, "fila.staytime.mean"), 12.0, 21.0);
    between("W", mean(&s, "saida.time_in_system.mean"), 15.5, 26.0);
    // Lei de Little na fila: Lq = lambda x Wq, com lambda = 0,2.
    let little = 0.2 * mean(&s, "fila.staytime.mean");
    let lq = mean(&s, "fila.content.mean");
    assert!(
        (little - lq).abs() < 0.1 * lq,
        "Little: {little} contra {lq}"
    );
}

#[test]
fn tutorial_02_bottleneck_is_the_drill() {
    let s = run("tutorial_02_linha.ron", 4, 30000.0, &[]);
    let (corte, furacao, pintura) = (
        mean(&s, "corte.content.mean"),
        mean(&s, "furacao.content.mean"),
        mean(&s, "pintura.content.mean"),
    );
    between("furacao", furacao, 0.82, 0.97);
    between("corte", corte, 0.68, 0.82);
    between("pintura", pintura, 0.55, 0.70);
    assert!(furacao > corte && furacao > pintura);

    let two = run(
        "tutorial_02_linha.ron",
        4,
        30000.0,
        &[("furacao.capacity", "2")],
    );
    assert!(mean(&two, "fila_furacao.content.mean") < 2.0);
    assert!(mean(&two, "saida.time_in_system.mean") < 0.6 * mean(&s, "saida.time_in_system.mean"));

    // Com mais demanda, a segunda furadeira aumenta a produção.
    let high = [("chegada.interarrival", "Exp(mean: 3.4)")];
    let base = run("tutorial_02_linha.ron", 4, 30000.0, &high);
    let high2 = [
        ("chegada.interarrival", "Exp(mean: 3.4)"),
        ("furacao.capacity", "2"),
    ];
    let more = run("tutorial_02_linha.ron", 4, 30000.0, &high2);
    between(
        "furacao saturada",
        mean(&base, "furacao.content.mean"),
        0.99,
        1.0,
    );
    assert!(mean(&more, "saida.input") > 1.03 * mean(&base, "saida.input"));
}

#[test]
fn tutorial_03_distance_changes_the_result() {
    let near = run("tutorial_03_operadores.ron", 4, 40000.0, &[]);
    let far = [
        ("maq1.position", "(15.0, -45.0, 0.0)"),
        ("maq3.position", "(15.0, 55.0, 0.0)"),
        ("ana.position", "(10.0, -20.0, 0.0)"),
        ("bruno.position", "(10.0, 30.0, 0.0)"),
    ];
    let far = run("tutorial_03_operadores.ron", 4, 40000.0, &far);
    between(
        "espera por operador (5 m)",
        mean(&near, "maq2.state.waiting_operator"),
        0.01,
        0.06,
    );
    between(
        "espera por operador (50 m)",
        mean(&far, "maq2.state.waiting_operator"),
        0.15,
        0.45,
    );
    assert!(
        mean(&far, "saida.time_in_system.mean") > 5.0 * mean(&near, "saida.time_in_system.mean")
    );

    // Contraprova: afastar objetos ligados diretamente não muda nada sem `transfer`.
    let away = [
        ("fila.position", "(-200.0, 5.0, 0.0)"),
        ("chegada.position", "(-205.0, 5.0, 0.0)"),
    ];
    let away = run("tutorial_03_operadores.ron", 4, 40000.0, &away);
    for key in [
        "saida.time_in_system.mean",
        "fila.content.mean",
        "saida.input",
    ] {
        assert_eq!(mean(&near, key), mean(&away, key), "{key}");
    }

    // Com `transfer`, a mesma distância passa a custar tempo.
    let timed = [
        ("fila.position", "(-15.0, 5.0, 0.0)"),
        ("chegada.position", "(-20.0, 5.0, 0.0)"),
        ("run.transfer", "(speed: 1.2)"),
    ];
    let timed = run("tutorial_03_operadores.ron", 4, 40000.0, &timed);
    assert!(
        mean(&timed, "saida.time_in_system.mean") > 3.0 * mean(&near, "saida.time_in_system.mean")
    );
}

#[test]
fn tutorial_04_two_forklifts_are_enough() {
    let one = run(
        "tutorial_04_armazem.ron",
        4,
        30000.0,
        &[("frota.team", "[\"empilhadeira_1\"]")],
    );
    let two = run("tutorial_04_armazem.ron", 4, 30000.0, &[]);
    assert!(mean(&one, "recebimento.content.mean") > 15.0);
    assert!(mean(&two, "recebimento.content.mean") < 12.0);
    assert!(mean(&two, "expedicao.input") > 1.2 * mean(&one, "expedicao.input"));
    // Little: 1200 s de permanência a cada 90 s dá cerca de 13 paletes guardados.
    between(
        "porta-paletes",
        mean(&two, "porta_paletes.content.mean"),
        11.0,
        16.5,
    );
    between(
        "ocupação máxima",
        mean(&two, "porta_paletes.content.max"),
        15.0,
        18.0,
    );
    between(
        "empilhadeira 3 parada",
        mean(&two, "empilhadeira_3.state.idle"),
        0.99,
        1.0,
    );
}

#[test]
fn tutorial_05_fitted_distributions_in_the_model() {
    let s = run("tutorial_05_ajuste.ron", 4, 20000.0, &[]);
    between(
        "utilização por atendente",
        mean(&s, "atendentes.content.mean") / 3.0,
        0.52,
        0.66,
    );
    between("espera na fila", mean(&s, "fila.staytime.mean"), 0.5, 1.5);
    let flat = run(
        "tutorial_05_ajuste.ron",
        4,
        20000.0,
        &[("atendentes.process", "Const(5.115)")],
    );
    let expo = run(
        "tutorial_05_ajuste.ron",
        4,
        20000.0,
        &[("atendentes.process", "Exp(mean: 5.115)")],
    );
    let wait = |x: &BTreeMap<String, Summary>| mean(x, "fila.staytime.mean");
    assert!(wait(&flat) < wait(&s) && wait(&s) < wait(&expo));
}

#[test]
fn tutorial_05_data_files_have_the_documented_shape() {
    let dir = examples().join("dados");
    let service = std::fs::read_to_string(dir.join("tutorial_05_atendimento.csv")).unwrap();
    let values: Vec<f64> = service
        .lines()
        .skip(1)
        .map(|l| {
            l.split(';')
                .nth(1)
                .unwrap()
                .replace(',', ".")
                .parse()
                .unwrap()
        })
        .collect();
    assert_eq!(values.len(), 80);
    let avg = values.iter().sum::<f64>() / values.len() as f64;
    between("média do atendimento", avg, 5.0, 5.2);
    let arrivals = std::fs::read_to_string(dir.join("tutorial_05_chegadas.csv")).unwrap();
    assert_eq!(arrivals.lines().count(), 101);
}

#[test]
fn tutorial_06_number_of_servers() {
    let at = |c: &str| {
        run(
            "tutorial_06_atendimento.ron",
            4,
            8000.0,
            &[("atendentes.capacity", c)],
        )
    };
    let (three, four, five) = (at("3"), at("4"), at("5"));
    assert!(mean(&three, "fila.staytime.mean") > 2.5);
    between("espera com 4", mean(&four, "fila.staytime.mean"), 0.6, 1.6);
    assert!(mean(&five, "fila.staytime.mean") < 0.6);
    // Com 3 atendentes uma parte dos clientes vai embora.
    assert!(mean(&three, "saida.input") < 0.97 * mean(&five, "saida.input"));
}
