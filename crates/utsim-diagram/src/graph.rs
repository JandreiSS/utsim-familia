//! The process graph: every object of the model becomes a step with a flowchart symbol,
//! every connection an arrow, and the mobile resources become side notes.

use std::collections::{BTreeMap, HashMap};

use utsim_core::{OutputRule, TransferPath};
use utsim_model::{Model, ObjectDef, ObjectKind, is_mobile};
use utsim_objects::network::router;
use utsim_objects::{CombineMode, OnBlocked, SeparateMode};

use crate::text::{fold, num};

/// Standard symbols of the process flowchart of methods engineering.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Symbol {
    /// Where items enter the process (terminator).
    Inicio,
    /// Where items leave the process (terminator).
    Fim,
    /// Circle: the item is changed (processed, assembled, separated).
    Operacao,
    /// Arrow: the item moves (belt, forklift, AGV, timed transfer).
    Transporte,
    /// Square: the item is checked.
    Inspecao,
    /// "D": the item waits.
    Espera,
    /// Triangle: the item is stored.
    Armazenagem,
}

impl Symbol {
    pub const ALL: [Symbol; 7] = [
        Symbol::Inicio,
        Symbol::Fim,
        Symbol::Operacao,
        Symbol::Transporte,
        Symbol::Inspecao,
        Symbol::Espera,
        Symbol::Armazenagem,
    ];

    /// Name shown to the user.
    pub fn nome(self) -> &'static str {
        match self {
            Symbol::Inicio => "Início",
            Symbol::Fim => "Fim",
            Symbol::Operacao => "Operação",
            Symbol::Transporte => "Transporte",
            Symbol::Inspecao => "Inspeção",
            Symbol::Espera => "Espera",
            Symbol::Armazenagem => "Armazenagem",
        }
    }

    /// Short identifier without accents: CSS class, Mermaid class, command-line option.
    pub fn id(self) -> &'static str {
        match self {
            Symbol::Inicio => "inicio",
            Symbol::Fim => "fim",
            Symbol::Operacao => "operacao",
            Symbol::Transporte => "transporte",
            Symbol::Inspecao => "inspecao",
            Symbol::Espera => "espera",
            Symbol::Armazenagem => "armazenagem",
        }
    }

    /// What the symbol means (legend).
    pub fn significado(self) -> &'static str {
        match self {
            Symbol::Inicio => "entrada dos itens no processo",
            Symbol::Fim => "saída dos itens do processo",
            Symbol::Operacao => "o item é transformado (processo, montagem, separação)",
            Symbol::Transporte => "o item é movido (esteira, empilhadeira, AGV, transferência)",
            Symbol::Inspecao => "o item é conferido",
            Symbol::Espera => "o item aguarda (fila)",
            Symbol::Armazenagem => "o item fica guardado (porta-paletes)",
        }
    }

    /// Reads a symbol from text typed by the user ("inspecao", "Inspeção", "D"...).
    pub fn from_id(text: &str) -> Option<Symbol> {
        let t = fold(text.trim());
        Symbol::ALL
            .into_iter()
            .find(|s| s.id() == t)
            .or(match t.as_str() {
                "inicio" | "start" => Some(Symbol::Inicio),
                "d" | "atraso" | "fila" => Some(Symbol::Espera),
                "armazem" | "estoque" => Some(Symbol::Armazenagem),
                "processo" => Some(Symbol::Operacao),
                _ => None,
            })
    }
}

/// Kind of object a step came from.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NodeKind {
    Source,
    Queue,
    Processor,
    Combiner,
    Separator,
    Sink,
    Conveyor,
    Rack,
}

impl NodeKind {
    /// Portuguese name, as in the manual.
    pub fn nome(self) -> &'static str {
        match self {
            NodeKind::Source => "Chegada",
            NodeKind::Queue => "Fila",
            NodeKind::Processor => "Processo",
            NodeKind::Combiner => "Montagem",
            NodeKind::Separator => "Desmontagem",
            NodeKind::Sink => "Saída",
            NodeKind::Conveyor => "Esteira",
            NodeKind::Rack => "Porta-paletes",
        }
    }
}

/// One step of the process.
#[derive(Clone, Debug)]
pub struct Node {
    pub name: String,
    pub kind: NodeKind,
    pub symbol: Symbol,
    /// Mean time spent in the step (processing, belt travel, shelf dwell), when the model
    /// defines one. Queues have none without results.
    pub time: Option<f64>,
    /// Lines under the title: capacities, times, sizes.
    pub details: Vec<String>,
    /// Things worth noticing: breakdowns, shifts, splitting, junction, losses.
    pub notes: Vec<String>,
    /// Resources used by the step (teams, fleet), as short phrases.
    pub resources: Vec<String>,
    /// Number of arrows arriving.
    pub in_degree: usize,
    /// Number of arrows leaving.
    pub out_degree: usize,
    /// Results of a run, when requested.
    pub stats: Option<NodeStats>,
    /// Layer of the layered layout (0 = first).
    pub layer: usize,
    /// Top-left corner and size, in SVG units.
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
}

/// Figures of a run shown in the boxes. Every field is optional.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct NodeStats {
    /// Fraction of the time working, `0..=1`.
    pub utilization: Option<f64>,
    /// Mean content (for a queue: the mean queue length).
    pub mean_content: Option<f64>,
    /// Mean time spent in the object (for a queue: the mean wait).
    pub mean_stay: Option<f64>,
    /// Items that left the object per replication (arrivals created, for a source).
    pub items: Option<f64>,
}

/// Movement of the items along a connection.
#[derive(Clone, Debug, PartialEq)]
pub struct EdgeTransport {
    /// Transporter, fleet or "transferência" that carries the items.
    pub by: String,
    /// Distance in model units, when the positions are known.
    pub distance: Option<f64>,
    /// Mean travel time (estimated for forklifts and AGVs: it leaves out queueing).
    pub time: Option<f64>,
}

impl EdgeTransport {
    pub fn text(&self) -> String {
        let mut parts = Vec::new();
        if let Some(d) = self.distance {
            parts.push(format!("dist. {}", num(d)));
        }
        if let Some(t) = self.time {
            parts.push(format!("tempo {}", num(t)));
        }
        parts.push(self.by.clone());
        parts.join(" · ")
    }
}

/// An arrow of the flowchart.
#[derive(Clone, Debug)]
pub struct Edge {
    pub from: usize,
    pub to: usize,
    /// 0-based position among the arrows leaving `from` (the output port).
    pub out_port: usize,
    /// 0-based position among the arrows arriving at `to` (the input port).
    pub in_port: usize,
    /// Routing text: "60%", "alternado", "rótulo tipo = 2", "contêiner".
    pub label: Option<String>,
    /// Chance of an item taking this arrow, when the rule defines one.
    pub probability: Option<f64>,
    pub transport: Option<EdgeTransport>,
    /// The arrow goes back to an earlier step (loop): rework.
    pub rework: bool,
    /// Points the arrow passes through when it crosses layers (set by the layout).
    pub via: Vec<(f64, f64)>,
}

/// What a resource is, from how the model uses it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ResourceKind {
    Equipe,
    Frota,
    EquipeEFrota,
    Despachante,
    Transportador,
    Agv,
}

impl ResourceKind {
    pub fn nome(self) -> &'static str {
        match self {
            ResourceKind::Equipe => "Equipe",
            ResourceKind::Frota => "Frota",
            ResourceKind::EquipeEFrota => "Equipe e frota",
            ResourceKind::Despachante => "Despachante",
            ResourceKind::Transportador => "Transportador",
            ResourceKind::Agv => "AGV",
        }
    }
}

/// A team, a fleet or a single mobile resource, shown as a side note.
#[derive(Clone, Debug)]
pub struct Resource {
    pub name: String,
    pub kind: ResourceKind,
    /// Members of a team (empty for a single transporter).
    pub members: Vec<String>,
    /// Dispatch rule or speed, as a short phrase.
    pub detail: String,
    /// Steps (indices into [`FlowGraph::nodes`]) that use it.
    pub used_by: Vec<usize>,
}

/// Which way the flowchart grows.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Direction {
    /// Top to bottom (the usual process flowchart).
    #[default]
    Vertical,
    /// Left to right.
    Horizontal,
}

/// How the graph is built and drawn.
#[derive(Clone, Debug)]
pub struct Options {
    pub direction: Direction,
    /// Mean times in the boxes and on the arrows.
    pub show_times: bool,
    /// Distances of the transfers on the arrows.
    pub show_distances: bool,
    /// Fragments (case and accents ignored) of a name that make a processing step an
    /// inspection.
    pub inspection_keywords: Vec<String>,
    /// Symbol chosen for an object, by name; wins over everything else.
    pub overrides: BTreeMap<String, Symbol>,
    /// Figures of a run, by object name.
    pub stats: Option<BTreeMap<String, NodeStats>>,
    /// Replication the dynamic diagrams record (0 is the first).
    pub replication: u32,
    /// Time window of the man-machine chart (default: a tenth of the run after the warm-up).
    pub window: Option<(f64, f64)>,
    /// Lane (area) chosen for an object, by name, in the swimlane chart.
    pub areas: BTreeMap<String, String>,
    /// Hook for the floor plan: an SVG fragment drawn under the plan-view diagrams.
    pub plant_background: Option<String>,
    /// Second model of the spaghetti comparison (this one is the "before").
    pub compare: Option<Box<Model>>,
}

impl Default for Options {
    fn default() -> Self {
        Options {
            direction: Direction::Vertical,
            show_times: true,
            show_distances: true,
            inspection_keywords: [
                "inspec",
                "confer",
                "verific",
                "qualidade",
                "teste",
                "audit",
                "triagem",
                "conferencia",
            ]
            .iter()
            .map(|s| s.to_string())
            .collect(),
            overrides: BTreeMap::new(),
            stats: None,
            replication: 0,
            window: None,
            areas: BTreeMap::new(),
            plant_background: None,
            compare: None,
        }
    }
}

/// The whole flowchart: steps, arrows, side notes and, after layout, coordinates.
#[derive(Clone, Debug)]
pub struct FlowGraph {
    pub name: String,
    pub description: String,
    pub nodes: Vec<Node>,
    pub edges: Vec<Edge>,
    pub resources: Vec<Resource>,
    pub direction: Direction,
    /// Size of the drawing without the resource panel.
    pub width: f64,
    pub height: f64,
    pub has_stats: bool,
    pub show_times: bool,
}

impl FlowGraph {
    /// Index of the step called `name`.
    pub fn node_index(&self, name: &str) -> Option<usize> {
        self.nodes.iter().position(|n| n.name == name)
    }

    /// Arrows leaving step `i`, in port order.
    pub fn outgoing(&self, i: usize) -> Vec<&Edge> {
        let mut v: Vec<&Edge> = self.edges.iter().filter(|e| e.from == i).collect();
        v.sort_by_key(|e| e.out_port);
        v
    }
}

/// The symbol the model's object gets by default, before any override.
pub fn default_symbol(obj: &ObjectDef, options: &Options) -> Symbol {
    match &obj.kind {
        ObjectKind::Source { .. } => Symbol::Inicio,
        ObjectKind::Sink => Symbol::Fim,
        ObjectKind::Queue { .. } => Symbol::Espera,
        ObjectKind::Conveyor { .. } => Symbol::Transporte,
        ObjectKind::Rack { .. } => Symbol::Armazenagem,
        ObjectKind::Processor { .. }
        | ObjectKind::Combiner { .. }
        | ObjectKind::Separator { .. } => {
            let name = fold(&obj.name);
            if options
                .inspection_keywords
                .iter()
                .any(|k| !k.is_empty() && name.contains(&fold(k)))
            {
                Symbol::Inspecao
            } else {
                Symbol::Operacao
            }
        }
        // Mobile resources are not steps; the value is never drawn.
        ObjectKind::Transporter { .. } | ObjectKind::Dispatcher { .. } => Symbol::Transporte,
    }
}

fn dist_mean(d: &utsim_core::Dist) -> Option<f64> {
    let m = d.mean();
    m.is_finite().then_some(m)
}

fn node_kind(kind: &ObjectKind) -> Option<NodeKind> {
    Some(match kind {
        ObjectKind::Source { .. } => NodeKind::Source,
        ObjectKind::Queue { .. } => NodeKind::Queue,
        ObjectKind::Processor { .. } => NodeKind::Processor,
        ObjectKind::Combiner { .. } => NodeKind::Combiner,
        ObjectKind::Separator { .. } => NodeKind::Separator,
        ObjectKind::Sink => NodeKind::Sink,
        ObjectKind::Conveyor { .. } => NodeKind::Conveyor,
        ObjectKind::Rack { .. } => NodeKind::Rack,
        ObjectKind::Transporter { .. } | ObjectKind::Dispatcher { .. } => return None,
    })
}

/// Details, notes and the step time of one object.
fn describe(obj: &ObjectDef) -> (Option<f64>, Vec<String>, Vec<String>) {
    let mut time = None;
    let mut details = Vec::new();
    let mut notes = Vec::new();
    match &obj.kind {
        ObjectKind::Source {
            interarrival,
            max_arrivals,
            on_blocked,
            schedule,
            ..
        } => {
            if let Some(m) = dist_mean(interarrival) {
                details.push(format!("chega a cada {} (média)", num(m)));
            }
            if let Some(n) = max_arrivals {
                details.push(format!("no máximo {n} chegadas"));
            }
            if schedule.is_some() {
                notes.push("chegadas por período".to_string());
            }
            if *on_blocked == OnBlocked::Drop {
                notes.push("perda: descarta se não puder sair".to_string());
            }
        }
        ObjectKind::Queue {
            capacity, batch, ..
        } => {
            details.push(match capacity {
                Some(c) => format!("capacidade {c}"),
                None => "capacidade ilimitada".to_string(),
            });
            if let Some(b) = batch {
                notes.push(format!("forma lotes de {}", b.size));
            }
        }
        ObjectKind::Processor {
            capacity,
            setup,
            process,
            ..
        } => {
            let p = dist_mean(process);
            let s = setup.as_ref().and_then(dist_mean);
            time = match (p, s) {
                (Some(p), Some(s)) => Some(p + s),
                (p, _) => p,
            };
            if let Some(t) = time {
                details.push(match (p, s) {
                    (Some(p), Some(s)) => {
                        format!("tempo {} (prep. {} + proc. {})", num(t), num(s), num(p))
                    }
                    _ => format!("tempo {}", num(t)),
                });
            }
            if *capacity > 1 {
                details.push(format!("{capacity} servidores em paralelo"));
            }
        }
        ObjectKind::Combiner {
            components,
            mode,
            setup,
            process,
        } => {
            let p = dist_mean(process);
            let s = setup.as_ref().and_then(dist_mean);
            time = match (p, s) {
                (Some(p), Some(s)) => Some(p + s),
                (p, _) => p,
            };
            if let Some(t) = time {
                details.push(format!("tempo {}", num(t)));
            }
            let total: usize = components.iter().sum();
            details.push(format!("monta {total} componente(s) no contêiner"));
            notes.push(
                match mode {
                    CombineMode::Pack => "componentes embalados no contêiner",
                    CombineMode::Join => "componentes consumidos",
                    CombineMode::Batch => "sai o conjunto, ainda separado",
                }
                .to_string(),
            );
        }
        ObjectKind::Separator { mode, process } => {
            time = dist_mean(process);
            if let Some(t) = time {
                details.push(format!("tempo {}", num(t)));
            }
            notes.push(match mode {
                SeparateMode::Unpack => "desembala o conteúdo".to_string(),
                SeparateMode::Split(n) => format!("divide cada item em {n}"),
            });
        }
        ObjectKind::Sink => {}
        ObjectKind::Conveyor {
            length,
            speed,
            accumulating,
            ..
        } => {
            if *speed > 0.0 {
                time = Some(length / speed);
            }
            if let Some(t) = time {
                details.push(format!("tempo {} (comprimento {})", num(t), num(*length)));
            }
            notes.push(
                if *accumulating {
                    "acumula itens no fim"
                } else {
                    "para inteira se a saída trava"
                }
                .to_string(),
            );
        }
        ObjectKind::Rack {
            bays,
            levels,
            dwell,
            ..
        } => {
            details.push(format!("{} posições ({bays}×{levels})", bays * levels));
            time = dwell.as_ref().and_then(dist_mean);
            if let Some(t) = time {
                details.push(format!("permanência mínima {}", num(t)));
            }
        }
        ObjectKind::Transporter { .. } | ObjectKind::Dispatcher { .. } => {}
    }
    if let Some(dt) = &obj.downtime {
        let part = |d: &utsim_core::Dist| dist_mean(d).map_or("?".to_string(), num);
        notes.push(format!(
            "quebras: MTBF {} / MTTR {}",
            part(&dt.mtbf),
            part(&dt.mttr)
        ));
    }
    if obj.shifts.is_some() {
        notes.push("trabalha em turnos".to_string());
    }
    if obj.on_entry.is_some() {
        notes.push("script na entrada".to_string());
    }
    (time, details, notes)
}

/// Routing of one output port: text and chance.
fn route_of(rule: &OutputRule, port: usize, n: usize) -> (Option<String>, Option<f64>) {
    if n < 2 {
        return (None, None);
    }
    match rule {
        OutputRule::FirstAvailable => (Some(format!("prioridade {}", port + 1)), None),
        OutputRule::RoundRobin => (
            Some(format!("{}% (alternado)", num(100.0 / n as f64))),
            Some(1.0 / n as f64),
        ),
        OutputRule::Random => (
            Some(format!("{}% (aleatório)", num(100.0 / n as f64))),
            Some(1.0 / n as f64),
        ),
        OutputRule::ByLabel(l) => (Some(format!("rótulo {l} = {}", port + 1)), None),
        OutputRule::Weighted(w) => {
            let total: f64 = w.iter().sum();
            match w.get(port) {
                Some(v) if total > 0.0 => (
                    Some(format!("{}%", num(100.0 * v / total))),
                    Some(v / total),
                ),
                _ => (None, None),
            }
        }
        OutputRule::Script(_) => (Some(format!("script → saída {}", port + 1)), None),
    }
}

/// Builds the graph of a model. The layout is done by [`crate::layout::apply`].
pub fn build(model: &Model, options: &Options) -> FlowGraph {
    // Steps: every object that sits in the port graph.
    let mut index: HashMap<&str, usize> = HashMap::new();
    let mut nodes: Vec<Node> = Vec::new();
    let mut objs: Vec<&ObjectDef> = Vec::new();
    for obj in &model.objects {
        let Some(kind) = node_kind(&obj.kind) else {
            continue;
        };
        // The option wins over the model's `symbol`, which wins over the inferred one.
        let symbol = options
            .overrides
            .get(&obj.name)
            .copied()
            .or_else(|| obj.symbol.as_deref().and_then(Symbol::from_id))
            .unwrap_or_else(|| default_symbol(obj, options));
        let (time, mut details, notes) = describe(obj);
        if !options.show_times {
            details.retain(|d| {
                !["tempo", "chega a cada", "permanência mínima"]
                    .iter()
                    .any(|p| d.starts_with(p))
            });
        }
        index.insert(obj.name.as_str(), nodes.len());
        nodes.push(Node {
            name: obj.name.clone(),
            kind,
            symbol,
            time,
            details,
            notes,
            resources: Vec::new(),
            in_degree: 0,
            out_degree: 0,
            stats: options
                .stats
                .as_ref()
                .and_then(|s| s.get(&obj.name))
                .cloned(),
            layer: 0,
            x: 0.0,
            y: 0.0,
            w: 0.0,
            h: 0.0,
        });
        objs.push(obj);
    }

    // Arrows, with the port numbers the engine uses (order of the connections).
    let mut edges: Vec<Edge> = Vec::new();
    let mut out_count = vec![0usize; nodes.len()];
    let mut in_count = vec![0usize; nodes.len()];
    for (from, to) in &model.connections {
        let (Some(&a), Some(&b)) = (index.get(from.as_str()), index.get(to.as_str())) else {
            continue;
        };
        edges.push(Edge {
            from: a,
            to: b,
            out_port: out_count[a],
            in_port: in_count[b],
            label: None,
            probability: None,
            transport: None,
            rework: false,
            via: Vec::new(),
        });
        out_count[a] += 1;
        in_count[b] += 1;
    }
    for (i, node) in nodes.iter_mut().enumerate() {
        node.out_degree = out_count[i];
        node.in_degree = in_count[i];
        if in_count[i] > 1 {
            node.notes
                .push(format!("junção de {} entradas", in_count[i]));
        }
        if out_count[i] > 1 {
            node.notes
                .push(format!("ramifica em {} saídas", out_count[i]));
        }
    }

    // Routing labels.
    for e in &mut edges {
        let obj = objs[e.from];
        let (label, p) = route_of(&obj.output, e.out_port, out_count[e.from]);
        e.label = label;
        e.probability = p;
        if let ObjectKind::Combiner { components, .. } = &objs[e.to].kind {
            let port = match e.in_port {
                0 => "contêiner".to_string(),
                k => format!(
                    "{}× componente",
                    components.get(k - 1).copied().unwrap_or(1)
                ),
            };
            e.label = Some(match e.label.take() {
                Some(l) => format!("{l} · {port}"),
                None => port,
            });
        }
    }

    mark_rework(&mut edges, nodes.len());
    let positions_known = !model
        .objects
        .iter()
        .all(|o| o.position.iter().all(|c| *c == 0.0));
    add_transport(model, &objs, &mut edges, positions_known, options);
    let resources = add_resources(model, &objs, &mut nodes);

    FlowGraph {
        name: model.name.clone(),
        description: model.description.clone(),
        nodes,
        edges,
        resources,
        direction: options.direction,
        width: 0.0,
        height: 0.0,
        has_stats: options.stats.is_some(),
        show_times: options.show_times,
    }
}

/// Marks the arrows that close a loop (found by a depth-first walk from the entries).
pub(crate) fn mark_rework(edges: &mut [Edge], n: usize) {
    let mut adj: Vec<Vec<usize>> = vec![Vec::new(); n];
    for (k, e) in edges.iter().enumerate() {
        adj[e.from].push(k);
    }
    let mut has_in = vec![false; n];
    for e in edges.iter() {
        has_in[e.to] = true;
    }
    // 0 = new, 1 = on the stack, 2 = done.
    let mut state = vec![0u8; n];
    let starts: Vec<usize> = (0..n)
        .filter(|&i| !has_in[i])
        .chain(0..n)
        .collect::<Vec<_>>();
    for s in starts {
        if state[s] != 0 {
            continue;
        }
        state[s] = 1;
        let mut stack: Vec<(usize, usize)> = vec![(s, 0)];
        while let Some(&(v, next)) = stack.last() {
            if let Some(&k) = adj[v].get(next) {
                if let Some(top) = stack.last_mut() {
                    top.1 += 1;
                }
                let w = edges[k].to;
                match state[w] {
                    0 => {
                        state[w] = 1;
                        stack.push((w, 0));
                    }
                    1 => edges[k].rework = true,
                    _ => {}
                }
            } else {
                state[v] = 2;
                stack.pop();
            }
        }
    }
}

fn distance_of(
    model: &Model,
    a: &ObjectDef,
    b: &ObjectDef,
    path: TransferPath,
    positions_known: bool,
) -> Option<f64> {
    if !positions_known {
        return None;
    }
    let straight = || {
        a.position
            .iter()
            .zip(&b.position)
            .map(|(p, q)| (p - q) * (p - q))
            .sum::<f64>()
            .sqrt()
    };
    match (path, &model.network) {
        (TransferPath::Network, Some(def)) => {
            let r = router(Some(def));
            let pts = r.route(a.position, b.position);
            Some(
                pts.windows(2)
                    .map(|w| {
                        w[0].iter()
                            .zip(&w[1])
                            .map(|(p, q)| (p - q) * (p - q))
                            .sum::<f64>()
                            .sqrt()
                    })
                    .sum(),
            )
        }
        _ => Some(straight()),
    }
}

/// Speed and handling time of the transporter (or first member of the fleet) named `name`.
fn carrier_of(model: &Model, name: &str) -> Option<(f64, f64)> {
    let obj = model.objects.iter().find(|o| o.name == name)?;
    match &obj.kind {
        ObjectKind::Transporter {
            speed,
            load_time,
            unload_time,
            ..
        } => Some((
            *speed,
            dist_mean(load_time).unwrap_or(0.0) + dist_mean(unload_time).unwrap_or(0.0),
        )),
        ObjectKind::Dispatcher { team, .. } => team.first().and_then(|m| carrier_of(model, m)),
        _ => None,
    }
}

/// Marks the arrows whose items are carried by a transporter or a timed transfer.
fn add_transport(
    model: &Model,
    objs: &[&ObjectDef],
    edges: &mut [Edge],
    positions_known: bool,
    options: &Options,
) {
    for e in edges.iter_mut() {
        let (a, b) = (objs[e.from], objs[e.to]);
        if let Some(carrier) = &a.transport {
            let path = if model.network.is_some() {
                TransferPath::Network
            } else {
                TransferPath::Straight
            };
            let distance = distance_of(model, a, b, path, positions_known);
            let time = carrier_of(model, carrier)
                .filter(|(speed, _)| *speed > 0.0)
                .and_then(|(speed, handling)| distance.map(|d| d / speed + handling));
            e.transport = Some(EdgeTransport {
                by: carrier.clone(),
                distance: distance.filter(|_| options.show_distances),
                time: time.filter(|_| options.show_times),
            });
        } else if let Some(t) = model.effective_transfer(a) {
            let distance = distance_of(model, a, b, t.path, positions_known);
            let time = distance.map(|d| t.delay + d / t.speed);
            e.transport = Some(EdgeTransport {
                by: "transferência".to_string(),
                distance: distance.filter(|_| options.show_distances),
                time: time.filter(|_| options.show_times),
            });
        }
    }
}

/// Teams, fleets and loose transporters, and the steps that use them.
fn add_resources(model: &Model, objs: &[&ObjectDef], nodes: &mut [Node]) -> Vec<Resource> {
    // Who uses what.
    let mut by_transport: HashMap<&str, Vec<usize>> = HashMap::new();
    let mut by_operators: HashMap<&str, Vec<usize>> = HashMap::new();
    for (i, obj) in objs.iter().enumerate() {
        if let Some(t) = &obj.transport {
            by_transport.entry(t.as_str()).or_default().push(i);
            nodes[i].resources.push(format!("transporte: {t}"));
        }
        if let ObjectKind::Processor {
            operators: Some(op),
            ..
        } = &obj.kind
        {
            by_operators.entry(op.team.as_str()).or_default().push(i);
            let when = match (op.setup, op.process) {
                (true, true) => "preparação e processo",
                (true, false) => "só na preparação",
                (false, true) => "só no processo",
                (false, false) => "sem uso",
            };
            nodes[i]
                .resources
                .push(format!("operadores: {} ×{} ({when})", op.team, op.count));
        }
    }
    let mut in_team: HashMap<&str, ()> = HashMap::new();
    for obj in &model.objects {
        if let ObjectKind::Dispatcher { team, .. } = &obj.kind {
            for m in team {
                in_team.insert(m.as_str(), ());
            }
        }
    }
    let mut out = Vec::new();
    for obj in &model.objects {
        let name = obj.name.as_str();
        let mut used: Vec<usize> = Vec::new();
        let transports = by_transport.get(name);
        let operates = by_operators.get(name);
        used.extend(transports.into_iter().flatten());
        used.extend(operates.into_iter().flatten());
        used.sort_unstable();
        used.dedup();
        match &obj.kind {
            ObjectKind::Dispatcher { team, rule } => {
                let kind = match (transports.is_some(), operates.is_some()) {
                    (true, true) => ResourceKind::EquipeEFrota,
                    (true, false) => ResourceKind::Frota,
                    (false, true) => ResourceKind::Equipe,
                    (false, false) => ResourceKind::Despachante,
                };
                out.push(Resource {
                    name: obj.name.clone(),
                    kind,
                    members: team.clone(),
                    detail: format!("regra {rule:?}"),
                    used_by: used,
                });
            }
            ObjectKind::Transporter { speed, agv, .. } if !in_team.contains_key(name) => {
                out.push(Resource {
                    name: obj.name.clone(),
                    kind: if *agv {
                        ResourceKind::Agv
                    } else {
                        ResourceKind::Transportador
                    },
                    members: Vec::new(),
                    detail: format!("velocidade {}", num(*speed)),
                    used_by: used,
                });
            }
            _ => {}
        }
    }
    out
}

/// Whether `model` has a kind of object this crate draws as a step.
pub fn is_step(kind: &ObjectKind) -> bool {
    !is_mobile(kind)
}
