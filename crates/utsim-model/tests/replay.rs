//! Testes de consistência do rastro (replay): o que foi gravado durante a execução
//! precisa contar a mesma história que o relatório e as séries amostradas.
//!
//! Para cada modelo de `examples/` (exceto `experimento*` e arquivos que não são um
//! `Model`), a replicação 0 é rodada com `Model::replay`, e o conteúdo e o estado de
//! cada bloco são reconstruídos aplicando os registros em ordem:
//! - conteúdo: criação, transferência, empacotamento, desempacotamento e destruição;
//! - estado: os registros `State`.
//!
//! Propriedades:
//! 1. em cada amostra da série, a reconstrução bate com `series` (ver o empate de tempo
//!    logo abaixo);
//! 2. conservação e ciclo de vida dos itens, com o conteúdo final igual a `content_now`;
//! 3. determinismo, e rastrear não muda `report.metrics()`;
//! 4. em `montagem_paletes.ron`, itens empacotados seguem o contêiner.
//!
//! Empate de tempo: a amostra é um ponto na ordem de emissão dos eventos. Os registros
//! com `t == s` que foram emitidos antes da amostra contam, os que vieram depois não.
//! Por isso o valor da série deve ser o de algum prefixo dos registros de `t == s`
//! (checado para todos os blocos juntos, com o mesmo prefixo).

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use utsim_core::{BlockReport, RunReport, TraceEvent, TraceRecord};
use utsim_model::Model;
use utsim_model::replay::Replay;

const MAX_STOP_TIME: f64 = 2000.0;
const SAMPLE_INTERVAL: f64 = 10.0;
/// Nome do estado usado pelo relatório quando o bloco ainda não entrou em nenhum.
const NO_STATE: &str = "none";

fn examples_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples")
}

/// Aplica os limites dos testes: parada em no máximo 2000, sem aquecimento e amostras a cada 10.
fn capped(mut model: Model) -> Model {
    model.run.stop_time = model.run.stop_time.min(MAX_STOP_TIME);
    model.run.warmup = 0.0;
    model.run.sample_interval = Some(SAMPLE_INTERVAL);
    model
        .validate()
        .unwrap_or_else(|e| panic!("{}: {e}", model.name));
    model
}

/// Modelos de `examples/` que leem como `Model` (nome do arquivo, modelo com os limites).
fn example_models() -> Vec<(String, Model)> {
    let mut paths: Vec<PathBuf> = std::fs::read_dir(examples_dir())
        .unwrap()
        .map(|e| e.unwrap().path())
        .filter(|p| p.extension().is_some_and(|e| e == "ron"))
        .filter(|p| {
            let name = p.file_name().unwrap().to_string_lossy();
            // `roteamento_rl` precisa de um agente externo: não roda com `replay`/`run`.
            !name.starts_with("experimento") && name != "roteamento_rl.ron"
        })
        .collect();
    paths.sort();
    let models: Vec<(String, Model)> = paths
        .iter()
        .filter_map(|p| {
            let file = p.file_name().unwrap().to_string_lossy().into_owned();
            match Model::load(p) {
                Ok(m) => Some((file, capped(m))),
                Err(e) => {
                    eprintln!("ignorado (não lê como Model): {file}: {e}");
                    None
                }
            }
        })
        .collect();
    assert!(!models.is_empty(), "nenhum modelo encontrado em examples/");
    models
}

/// Item referenciado por um registro (os registros de estado não têm item).
fn item_of(event: &TraceEvent) -> Option<u64> {
    match *event {
        TraceEvent::Created { item, .. }
        | TraceEvent::Moved { item, .. }
        | TraceEvent::Packed { item, .. }
        | TraceEvent::Unpacked { item, .. }
        | TraceEvent::Destroyed { item } => Some(item),
        TraceEvent::Slot { item, .. } => Some(item),
        TraceEvent::State { .. }
        | TraceEvent::Travel { .. }
        | TraceEvent::Tokens { .. }
        | TraceEvent::Fluid { .. } => None,
    }
}

/// Onde está um item, segundo os registros aplicados até agora.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Loc {
    /// Ocupa o bloco.
    In(usize),
    /// Dentro do contêiner, fora de qualquer bloco.
    Packed(u64),
}

/// Conteúdo, estado e itens do modelo, reconstruídos aplicando os registros em ordem.
struct Ledger {
    content: Vec<usize>,
    state: Vec<&'static str>,
    items: HashMap<u64, Loc>,
}

impl Ledger {
    fn new(blocks: usize) -> Self {
        Ledger {
            content: vec![0; blocks],
            state: vec![NO_STATE; blocks],
            items: HashMap::new(),
        }
    }

    fn loc_of(&self, item: u64) -> Result<Loc, String> {
        self.items
            .get(&item)
            .copied()
            .ok_or_else(|| format!("item {item} desconhecido ou já destruído"))
    }

    fn apply(&mut self, rec: &TraceRecord) -> Result<(), String> {
        let n = self.content.len();
        let check_block = |b: usize| {
            if b < n {
                Ok(())
            } else {
                Err(format!("bloco {b} desconhecido"))
            }
        };
        match rec.event {
            TraceEvent::Created { item, block } => {
                check_block(block)?;
                if self.items.insert(item, Loc::In(block)).is_some() {
                    return Err(format!("item {item} criado duas vezes"));
                }
                self.content[block] += 1;
            }
            TraceEvent::Moved { item, from, to } => {
                check_block(from)?;
                check_block(to)?;
                let loc = self.loc_of(item)?;
                if loc != Loc::In(from) {
                    return Err(format!("item {item} movido de {from}, mas está em {loc:?}"));
                }
                self.items.insert(item, Loc::In(to));
                self.content[from] -= 1;
                self.content[to] += 1;
            }
            TraceEvent::Packed { item, container } => {
                let Loc::In(b) = self.loc_of(item)? else {
                    return Err(format!("item {item} empacotado sem estar em um bloco"));
                };
                if !matches!(self.items.get(&container), Some(Loc::In(_))) {
                    return Err(format!(
                        "contêiner {container} de {item} desconhecido ou empacotado"
                    ));
                }
                self.items.insert(item, Loc::Packed(container));
                self.content[b] -= 1;
            }
            TraceEvent::Travel { carrier, .. } => check_block(carrier)?,
            TraceEvent::Slot { block, .. } => check_block(block)?,
            TraceEvent::Tokens { flow, .. } => check_block(flow)?,
            TraceEvent::Fluid { net, .. } => check_block(net)?,
            TraceEvent::Unpacked { item, block } => {
                check_block(block)?;
                let Loc::Packed(container) = self.loc_of(item)? else {
                    return Err(format!(
                        "item {item} desempacotado, mas não estava empacotado"
                    ));
                };
                if !self.items.contains_key(&container) {
                    return Err(format!(
                        "item {item} desempacotado de contêiner {container} já destruído"
                    ));
                }
                self.items.insert(item, Loc::In(block));
                self.content[block] += 1;
            }
            TraceEvent::Destroyed { item } => {
                if let Loc::In(b) = self.loc_of(item)? {
                    self.content[b] -= 1;
                }
                self.items.remove(&item);
            }
            TraceEvent::State { block, state } => {
                check_block(block)?;
                self.state[block] = state.name();
            }
        }
        Ok(())
    }

    fn present(&self) -> usize {
        self.items
            .values()
            .filter(|l| matches!(l, Loc::In(_)))
            .count()
    }
}

/// Diferenças entre a série amostrada no índice `j` e a reconstrução atual.
fn sample_mismatches(ledger: &Ledger, blocks: &[BlockReport], j: usize) -> Vec<String> {
    blocks
        .iter()
        .enumerate()
        .filter_map(|(i, b)| {
            let p = &b.series[j];
            let (c, s) = (ledger.content[i], ledger.state[i]);
            (p.content != c || p.state != s).then(|| {
                format!(
                    "{}: série conteúdo {} estado {}, rastro conteúdo {c} estado {s}",
                    b.name, p.content, p.state
                )
            })
        })
        .collect()
}

/// Amostras da série batem com a reconstrução do rastro. Devolve o número de amostras.
fn check_series(name: &str, replay: &Replay) -> Result<usize, String> {
    let blocks = &replay.report.blocks;
    let times: Vec<f64> = blocks[0].series.iter().map(|p| p.t).collect();
    for b in blocks {
        let same =
            b.series.len() == times.len() && b.series.iter().zip(&times).all(|(p, t)| p.t == *t);
        if !same {
            return Err(format!(
                "{name}: {} tem instantes de amostra diferentes",
                b.name
            ));
        }
    }
    let expected = (replay.stop_time / SAMPLE_INTERVAL).floor() as usize + 1;
    if times.len() != expected {
        return Err(format!(
            "{name}: {} amostras, esperadas {expected}",
            times.len()
        ));
    }
    if times.windows(2).any(|w| w[1] <= w[0]) {
        return Err(format!("{name}: instantes de amostra fora de ordem"));
    }

    let recs = &replay.records;
    let mut ledger = Ledger::new(blocks.len());
    let mut next = 0;
    for (j, &s) in times.iter().enumerate() {
        // Registros estritamente antes da amostra.
        while next < recs.len() && recs[next].t < s {
            ledger
                .apply(&recs[next])
                .map_err(|e| format!("{name}, t={}: {e}", recs[next].t))?;
            next += 1;
        }
        // Registros do mesmo instante: a amostra cai em algum ponto entre eles.
        let lo = next;
        let mut hi = lo;
        while hi < recs.len() && recs[hi].t == s {
            hi += 1;
        }
        let mut k = lo;
        let mut first_diff = Vec::new();
        loop {
            let diff = sample_mismatches(&ledger, blocks, j);
            if k == lo {
                first_diff = diff.clone();
            }
            if diff.is_empty() {
                break;
            }
            if k == hi {
                return Err(format!(
                    "{name}: amostra em t={s} não bate com nenhum ponto de {} registros do mesmo instante.\n  \
                     antes do instante: {first_diff:?}\n  depois do instante: {diff:?}",
                    hi - lo
                ));
            }
            ledger
                .apply(&recs[k])
                .map_err(|e| format!("{name}, t={}: {e}", recs[k].t))?;
            k += 1;
        }
        for r in &recs[k..hi] {
            ledger
                .apply(r)
                .map_err(|e| format!("{name}, t={}: {e}", r.t))?;
        }
        next = hi;
    }
    Ok(times.len())
}

/// Reconstrói o modelo inteiro e confere conservação e ordem dos registros.
fn check_conservation(name: &str, replay: &Replay) -> Result<(), String> {
    let recs = &replay.records;
    if let Some(w) = recs.windows(2).find(|w| w[1].t < w[0].t) {
        return Err(format!(
            "{name}: registros fora de ordem: t={} seguido de t={}",
            w[0].t, w[1].t
        ));
    }
    if let Some(r) = recs
        .iter()
        .find(|r| !(r.t >= 0.0 && r.t <= replay.stop_time))
    {
        return Err(format!(
            "{name}: registro em t={} fora de [0, {}]",
            r.t, replay.stop_time
        ));
    }

    let blocks = &replay.report.blocks;
    let mut ledger = Ledger::new(blocks.len());
    let mut created = 0usize;
    let mut destroyed = 0usize;
    for r in recs {
        ledger
            .apply(r)
            .map_err(|e| format!("{name}, t={}: {e}", r.t))?;
        match r.event {
            TraceEvent::Created { .. } => created += 1,
            TraceEvent::Destroyed { .. } => destroyed += 1,
            _ => {}
        }
    }
    if created != destroyed + ledger.items.len() {
        return Err(format!(
            "{name}: {created} criados, {destroyed} destruídos e {} presentes",
            ledger.items.len()
        ));
    }
    for (i, b) in blocks.iter().enumerate() {
        if ledger.content[i] != b.content_now {
            return Err(format!(
                "{name}: {} tem conteúdo final {} pelo rastro e {} no relatório",
                b.name, ledger.content[i], b.content_now
            ));
        }
    }
    let in_blocks: usize = ledger.content.iter().sum();
    if in_blocks != ledger.present() {
        return Err(format!(
            "{name}: soma dos conteúdos {in_blocks} diferente dos itens presentes em blocos {}",
            ledger.present()
        ));
    }
    Ok(())
}

fn same_metrics(
    a: &std::collections::BTreeMap<String, f64>,
    b: &std::collections::BTreeMap<String, f64>,
) -> Result<(), String> {
    if a.keys().ne(b.keys()) {
        return Err("conjuntos de métricas diferentes".into());
    }
    for (k, x) in a {
        let y = b[k];
        if !(x == &y || (x.is_nan() && y.is_nan())) {
            return Err(format!("{k}: {x} com rastro, {y} sem rastro"));
        }
    }
    Ok(())
}

fn run_all(check: impl Fn(&str, &Model) -> Result<(), String>) {
    let mut failures = Vec::new();
    for (name, model) in example_models() {
        if let Err(e) = check(&name, &model) {
            failures.push(e);
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[test]
fn series_matches_the_reconstructed_trace() {
    run_all(|name, model| {
        let replay = model
            .replay(0)
            .map_err(|e| format!("{name}: replay falhou: {e}"))?;
        let samples = check_series(name, &replay)?;
        eprintln!("{name}: {samples} amostras conferidas");
        Ok(())
    });
}

#[test]
fn trace_is_conserved_and_ordered() {
    run_all(|name, model| {
        let replay = model
            .replay(0)
            .map_err(|e| format!("{name}: replay falhou: {e}"))?;
        check_conservation(name, &replay)?;
        eprintln!("{name}: {} registros conferidos", replay.records.len());
        Ok(())
    });
}

#[test]
fn replay_is_deterministic_and_tracing_changes_nothing() {
    run_all(|name, model| {
        let a = model
            .replay(0)
            .map_err(|e| format!("{name}: replay falhou: {e}"))?;
        let b = model
            .replay(0)
            .map_err(|e| format!("{name}: replay falhou: {e}"))?;
        if a.records != b.records {
            return Err(format!("{name}: dois replays da mesma replicação diferem"));
        }
        let direct: RunReport = model
            .build(0)
            .run()
            .map_err(|e| format!("{name}: execução direta falhou: {e}"))?;
        same_metrics(&a.report.metrics(), &direct.metrics())
            .map_err(|e| format!("{name}: rastrear mudou o resultado: {e}"))
    });
}

#[test]
fn packed_items_follow_their_containers() {
    let model = capped(
        Model::load(examples_dir().join("montagem_paletes.ron")).expect("montagem_paletes.ron"),
    );
    let replay = model.replay(0).expect("replay");
    let recs = &replay.records;

    // Índices de cada registro que menciona um item, e de cada destruição.
    let mut refs: HashMap<u64, Vec<usize>> = HashMap::new();
    let mut destroyed_at: HashMap<u64, usize> = HashMap::new();
    for (i, r) in recs.iter().enumerate() {
        if let Some(item) = item_of(&r.event) {
            refs.entry(item).or_default().push(i);
        }
        if let TraceEvent::Destroyed { item } = r.event {
            destroyed_at.insert(item, i);
        }
    }

    let (mut packs, mut unpacked, mut destroyed_in_container, mut still_packed) = (0, 0, 0, 0);
    for (i, r) in recs.iter().enumerate() {
        let TraceEvent::Packed { item, container } = r.event else {
            continue;
        };
        packs += 1;
        let t = r.t;
        let next = refs[&item].iter().copied().find(|&j| j > i);
        let Some(next) = next else {
            // Ainda empacotado no fim da execução: o contêiner não pode ter sido destruído.
            if destroyed_at.get(&container).is_some_and(|&c| c > i) {
                panic!(
                    "t={t}: item {item} empacotado em {container}, que foi destruído sem o item"
                );
            }
            still_packed += 1;
            continue;
        };
        match recs[next].event {
            TraceEvent::Unpacked { item: u, .. } if u == item => unpacked += 1,
            TraceEvent::Destroyed { .. } => {
                let ok = destroyed_at
                    .get(&container)
                    .is_some_and(|&c| c > i && c < next);
                assert!(
                    ok,
                    "t={t}: item {item} destruído sem o contêiner {container} ter sido destruído antes"
                );
                destroyed_in_container += 1;
            }
            ref other => panic!(
                "t={t}: item {item} empacotado em {container}, e o próximo registro é {other:?}"
            ),
        }
    }
    eprintln!(
        "montagem_paletes: {packs} empacotamentos: {unpacked} desempacotados, \
         {destroyed_in_container} destruídos com o contêiner, {still_packed} ainda empacotados no fim"
    );
    assert!(packs > 0, "o modelo não empacotou nada");
    assert!(unpacked > 0, "nenhum item foi desempacotado");
}
