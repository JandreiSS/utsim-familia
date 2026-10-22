//! Testes golden (snapshots): travam o resultado numérico de cada modelo de `examples/`.
//!
//! Para cada `.ron` que é um `Model`, rodam duas replicações (0 e 1) com a semente do
//! próprio modelo, com o tempo de parada limitado a 5000 e o aquecimento limitado a
//! um décimo do tempo de parada. As métricas de cada replicação (mais `events`) são
//! comparadas com `tests/golden/<nome>.json` na raiz do repositório.
//!
//! Para regenerar os snapshots após uma mudança de comportamento intencional:
//! `UTSIM_BLESS=1 cargo test -p utsim-model --test golden`. Revise o diff de
//! `tests/golden/` e explique a mudança na mensagem do commit.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use utsim_model::Model;

const MAX_STOP_TIME: f64 = 5000.0;
const REPLICATIONS: u32 = 2;
const MAX_LISTED: usize = 20;

type Metrics = BTreeMap<String, f64>;

#[derive(Serialize, Deserialize)]
struct Snapshot {
    model: String,
    replications: Vec<Metrics>,
}

fn repo_path(relative: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join(relative)
}

/// Roda o modelo com os limites de golden e devolve o snapshot correspondente.
fn run_snapshot(mut model: Model) -> Result<Snapshot, String> {
    let stop = model.run.stop_time.min(MAX_STOP_TIME);
    model.run.stop_time = stop;
    model.run.warmup = model.run.warmup.min(stop / 10.0);
    model.validate().map_err(|e| e.to_string())?;

    let mut replications = Vec::new();
    for r in 0..REPLICATIONS {
        let report = model
            .build(r)
            .run()
            .map_err(|e| format!("replicação {r}: {e}"))?;
        let mut metrics = report.metrics();
        metrics.insert("events".to_string(), report.events as f64);
        replications.push(metrics);
    }
    Ok(Snapshot {
        model: model.name.clone(),
        replications,
    })
}

fn same_value(old: f64, new: f64) -> bool {
    let diff = (old - new).abs();
    diff <= 1e-12 || diff <= 1e-9 * old.abs().max(new.abs())
}

/// Lista as diferenças entre o snapshot salvo e o resultado atual.
fn compare(old: &Snapshot, new: &Snapshot) -> Vec<String> {
    if old.replications.len() != new.replications.len() {
        return vec![format!(
            "número de replicações: snapshot {}, atual {}",
            old.replications.len(),
            new.replications.len()
        )];
    }
    let mut problems = Vec::new();
    let mut mismatches = Vec::new();
    for (i, (a, b)) in old.replications.iter().zip(&new.replications).enumerate() {
        for key in a.keys().filter(|k| !b.contains_key(*k)) {
            problems.push(format!("rep {i}: métrica removida: {key}"));
        }
        for key in b.keys().filter(|k| !a.contains_key(*k)) {
            problems.push(format!("rep {i}: métrica nova: {key}"));
        }
        for (key, &old_value) in a {
            if let Some(&new_value) = b.get(key) {
                if !same_value(old_value, new_value) {
                    mismatches.push(format!(
                        "rep {i} {key}: antes {old_value}, agora {new_value}"
                    ));
                }
            }
        }
    }
    let total = mismatches.len();
    problems.extend(mismatches.into_iter().take(MAX_LISTED));
    if total > MAX_LISTED {
        problems.push(format!(
            "... e mais {} métrica(s) diferente(s)",
            total - MAX_LISTED
        ));
    }
    problems
}

fn to_json(snapshot: &Snapshot) -> String {
    let mut text = serde_json::to_string_pretty(snapshot).expect("snapshot serializável");
    text.push('\n');
    text
}

#[test]
fn golden_snapshots() {
    let bless = std::env::var("UTSIM_BLESS").is_ok_and(|v| v == "1");
    let examples_dir = repo_path("examples");
    let golden_dir = repo_path("tests/golden");

    let mut paths: Vec<PathBuf> = std::fs::read_dir(&examples_dir)
        .unwrap_or_else(|e| panic!("não consegui ler {}: {e}", examples_dir.display()))
        .map(|entry| entry.expect("entrada de diretório legível").path())
        .filter(|p| p.extension().is_some_and(|e| e == "ron"))
        .collect();
    paths.sort();

    let mut failures: Vec<String> = Vec::new();
    let mut checked = 0;
    for path in &paths {
        let stem = path
            .file_stem()
            .expect("arquivo com nome")
            .to_string_lossy()
            .into_owned();
        let text = match std::fs::read_to_string(path) {
            Ok(text) => text,
            Err(e) => {
                failures.push(format!("{stem}: não foi possível ler o arquivo: {e}"));
                continue;
            }
        };
        let model = match Model::parse(&text) {
            Ok(model) => model,
            Err(e) => {
                eprintln!("golden: {} ignorado, não é um Model ({e})", path.display());
                continue;
            }
        };
        let snapshot = match run_snapshot(model) {
            Ok(snapshot) => snapshot,
            Err(e) => {
                failures.push(format!("{stem}: falha ao rodar o modelo: {e}"));
                continue;
            }
        };
        checked += 1;

        let snap_path = golden_dir.join(format!("{stem}.json"));
        if bless {
            let written = std::fs::create_dir_all(&golden_dir)
                .and_then(|()| std::fs::write(&snap_path, to_json(&snapshot)));
            match written {
                Ok(()) => eprintln!("golden: snapshot gravado em {}", snap_path.display()),
                Err(e) => failures.push(format!("{stem}: não foi possível gravar o snapshot: {e}")),
            }
            continue;
        }

        let saved_text = match std::fs::read_to_string(&snap_path) {
            Ok(text) => text,
            Err(_) => {
                failures.push(format!(
                    "{stem}: snapshot ausente em {}. Gere-o com: UTSIM_BLESS=1 cargo test -p utsim-model --test golden",
                    snap_path.display()
                ));
                continue;
            }
        };
        let saved: Snapshot = match serde_json::from_str(&saved_text) {
            Ok(saved) => saved,
            Err(e) => {
                failures.push(format!(
                    "{stem}: snapshot inválido ({}): {e}",
                    snap_path.display()
                ));
                continue;
            }
        };
        let problems = compare(&saved, &snapshot);
        if !problems.is_empty() {
            let lines: Vec<String> = problems.iter().map(|p| format!("    {p}")).collect();
            failures.push(format!(
                "{stem}: resultado diferente do snapshot {}\n{}",
                snap_path.display(),
                lines.join("\n")
            ));
        }
    }

    eprintln!(
        "golden: {checked} modelo(s) verificado(s), {} falha(s)",
        failures.len()
    );
    if !failures.is_empty() {
        panic!(
            "{} modelo(s) divergem dos snapshots golden. Se a mudança for intencional, revise o diff e rode: UTSIM_BLESS=1 cargo test -p utsim-model --test golden\n\n{}",
            failures.len(),
            failures.join("\n\n")
        );
    }
}
