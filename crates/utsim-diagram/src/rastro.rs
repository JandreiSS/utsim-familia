//! What the dynamic diagrams read from one replication: trips of the items, legs of the
//! mobile resources, states over time and the routes the items took.

use std::collections::BTreeMap;

use utsim_core::{RunReport, State, TraceEvent};
use utsim_model::{Model, ObjectKind};

/// An item going from one object to another.
#[derive(Clone, Debug)]
pub struct Viagem {
    pub t: f64,
    pub item: u64,
    pub de: usize,
    pub para: usize,
    /// Length of the path (timed transfers) or of the straight line between the objects.
    pub dist: f64,
}

/// A mobile resource moving along a path.
#[derive(Clone, Debug)]
pub struct Perna {
    pub t: f64,
    pub carrier: usize,
    pub dist: f64,
    pub path: Vec<[f64; 3]>,
}

/// One replication, boiled down.
#[derive(Clone, Debug)]
pub struct Rastro {
    pub semente: u64,
    pub replicacao: u32,
    pub parada: f64,
    pub aquecimento: f64,
    /// Object names, by block index.
    pub nomes: Vec<String>,
    pub pos: Vec<[f64; 3]>,
    pub viagens: Vec<Viagem>,
    pub pernas: Vec<Perna>,
    /// State changes `(time, state)` of each block.
    pub estados: Vec<Vec<(f64, State)>>,
    /// Routes of the items that went from a source to a sink, with how many took each,
    /// most used first.
    pub rotas: Vec<(Vec<usize>, usize)>,
    pub relatorio: RunReport,
}

/// Length of a polyline.
pub fn comprimento(path: &[[f64; 3]]) -> f64 {
    path.windows(2).map(|w| dist3(w[0], w[1])).sum()
}

pub fn dist3(a: [f64; 3], b: [f64; 3]) -> f64 {
    a.iter()
        .zip(&b)
        .map(|(p, q)| (p - q) * (p - q))
        .sum::<f64>()
        .sqrt()
}

/// Runs replication `rep` recording its trace.
pub fn capturar(model: &Model, rep: u32) -> Result<Rastro, String> {
    let replay = model.replay(rep).map_err(|e| e.to_string())?;
    let n = model.objects.len();
    let pos: Vec<[f64; 3]> = model.objects.iter().map(|o| o.position).collect();
    let is =
        |b: usize, f: fn(&ObjectKind) -> bool| model.objects.get(b).is_some_and(|o| f(&o.kind));
    let mut viagens = Vec::new();
    let mut pernas = Vec::new();
    let mut estados: Vec<Vec<(f64, State)>> = vec![Vec::new(); n];
    let mut transit: BTreeMap<u64, Vec<[f64; 3]>> = BTreeMap::new();
    let mut jornadas: BTreeMap<u64, Vec<usize>> = BTreeMap::new();
    for r in &replay.records {
        match &r.event {
            TraceEvent::Created { item, block } if *block < n => {
                jornadas.entry(*item).or_default().push(*block);
            }
            TraceEvent::Transit { item, path, .. } => {
                transit.insert(*item, path.clone());
            }
            TraceEvent::Moved { item, from, to } if *from < n && *to < n => {
                let dist = transit
                    .remove(item)
                    .map_or_else(|| dist3(pos[*from], pos[*to]), |p| comprimento(&p));
                viagens.push(Viagem {
                    t: r.t,
                    item: *item,
                    de: *from,
                    para: *to,
                    dist,
                });
                let j = jornadas.entry(*item).or_default();
                if j.is_empty() {
                    j.push(*from);
                }
                j.push(*to);
            }
            TraceEvent::Travel { carrier, path, .. } if *carrier < n => pernas.push(Perna {
                t: r.t,
                carrier: *carrier,
                dist: comprimento(path),
                path: path.clone(),
            }),
            TraceEvent::State { block, state } if *block < n => {
                estados[*block].push((r.t, *state));
            }
            _ => {}
        }
    }
    let mut count: BTreeMap<Vec<usize>, usize> = BTreeMap::new();
    for j in jornadas.into_values() {
        let (Some(&a), Some(&b)) = (j.first(), j.last()) else {
            continue;
        };
        if is(a, |k| matches!(k, ObjectKind::Source { .. }))
            && is(b, |k| matches!(k, ObjectKind::Sink))
        {
            *count.entry(j).or_default() += 1;
        }
    }
    let mut rotas: Vec<(Vec<usize>, usize)> = count.into_iter().collect();
    rotas.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
    Ok(Rastro {
        semente: model.run.seed,
        replicacao: rep,
        parada: model.run.stop_time,
        aquecimento: model.run.warmup,
        nomes: model.objects.iter().map(|o| o.name.clone()).collect(),
        pos,
        viagens,
        pernas,
        estados,
        rotas,
        relatorio: replay.report,
    })
}

impl Rastro {
    /// Text for the footers: what the data is.
    pub fn origem(&self, model: &Model) -> String {
        format!(
            "simulação do modelo \"{}\": semente {}, replicação {}, até t = {}.",
            model.name,
            self.semente,
            self.replicacao,
            crate::text::num(self.parada)
        )
    }

    /// The report of a block, by name.
    pub fn bloco(&self, name: &str) -> Option<&utsim_core::BlockReport> {
        self.relatorio.blocks.iter().find(|b| b.name == name)
    }
}
