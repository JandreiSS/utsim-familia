//! Activity network (activity on node) derived from a model and a selection.
//!
//! Each selected object becomes an activity with a duration; each connection between two
//! selected objects is a finish-to-start precedence. Timed links (a `transport` or a
//! `transfer` on the origin object) become activities of their own between the two
//! objects. A `Separator` is an AND split, a `Combiner` an AND join; an object with several
//! outputs that is not a separator picks **one** of them (an OR), which classic PERT cannot
//! represent: see [`WeightedBranch`] and the warnings of the derived network. Rework loops
//! are cut at the back connection and the activities of the loop get their duration
//! multiplied by the expected number of passes `1 / (1 - p)`.

use std::collections::{BTreeSet, HashMap, VecDeque};

use serde::Serialize;
use utsim_core::rng::SimRng;
use utsim_core::task::{distance, path_length, travel_time};
use utsim_core::{Dist, OutputRule, TransferPath};
use utsim_model::{Model, ObjectDef, ObjectKind, is_mobile};
use utsim_objects::CombineMode;

use crate::PertError;

/// Which part of the model the network covers.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub enum Selection {
    /// A set of objects (the editor's multi-selection).
    Objects(Vec<String>),
    /// Every object on some path from `from` to `to` (the semantics of a `SpanDef`):
    /// the objects reachable from `from` that also reach `to`.
    Between { from: String, to: String },
}

impl Selection {
    /// Short description for reports.
    pub fn describe(&self) -> String {
        match self {
            Selection::Objects(names) => format!("objetos {}", names.join(", ")),
            Selection::Between { from, to } => format!("de {from} até {to}"),
        }
    }
}

/// How the network treats an object with `OutputRule::Weighted` and several outputs.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize)]
pub enum WeightedBranch {
    /// Keeps only the branch with the largest weight (the first one on a tie).
    #[default]
    MostLikely,
    /// One network per combination of branches, with its probability
    /// ([`derive_scenarios`]); [`derive`] returns the most probable one.
    Scenarios,
}

/// Options of [`derive`].
#[derive(Clone, Debug, Default)]
pub struct DeriveOptions {
    pub weighted: WeightedBranch,
}

/// Most scenarios [`derive_scenarios`] enumerates.
pub const MAX_SCENARIOS: usize = 64;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub enum ActivityKind {
    /// Start or end of the network (duration 0).
    Milestone,
    /// A model object.
    Object,
    /// A timed direct connection (`transfer`).
    Transfer,
    /// A trip of a mobile resource (`transport`).
    Transport,
}

/// One activity: its duration is `factor × (sum of parts)`, each part drawn independently.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Activity {
    pub name: String,
    pub kind: ActivityKind,
    /// The model object behind the activity (objects only).
    pub object: Option<String>,
    /// Object type as written in the model ("Processor", "Queue"...), or the link type.
    pub label: String,
    pub parts: Vec<Dist>,
    /// Expected number of passes (rework loops); 1 without loops.
    pub factor: f64,
    /// Assumption behind the duration, shown in reports.
    pub note: Option<String>,
}

impl Activity {
    /// Activity with one duration distribution (hand-built networks, tests).
    pub fn new(name: impl Into<String>, duration: Dist) -> Self {
        Activity {
            name: name.into(),
            kind: ActivityKind::Object,
            object: None,
            label: "Atividade".into(),
            parts: vec![duration],
            factor: 1.0,
            note: None,
        }
    }

    fn milestone(name: &str) -> Self {
        Activity {
            name: name.into(),
            kind: ActivityKind::Milestone,
            object: None,
            label: "Marco".into(),
            parts: Vec::new(),
            factor: 1.0,
            note: None,
        }
    }

    /// Mean duration (exact: sum of the part means times the factor).
    pub fn mean(&self) -> f64 {
        self.factor * self.parts.iter().map(Dist::mean).sum::<f64>()
    }

    /// One random duration; negative draws count as zero, as in the kernel.
    pub fn sample(&self, rng: &mut SimRng) -> f64 {
        self.factor
            * self
                .parts
                .iter()
                .map(|d| d.sample(rng).max(0.0))
                .sum::<f64>()
    }

    pub fn is_milestone(&self) -> bool {
        self.kind == ActivityKind::Milestone
    }
}

/// Which branch combination a network stands for (`WeightedBranch::Scenarios`).
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Scenario {
    /// "q1→a, q2→c".
    pub label: String,
    pub probability: f64,
}

/// The activity network: activities, finish-to-start precedences, the start and end
/// milestones and what the derivation had to assume.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct ActivityNetwork {
    pub activities: Vec<Activity>,
    /// `(before, after)` indices into `activities`.
    pub precedences: Vec<(usize, usize)>,
    /// Index of the start milestone.
    pub start: usize,
    /// Index of the end milestone.
    pub end: usize,
    /// Problems the person should know about (cycles cut, OR branches, disconnections).
    pub warnings: Vec<String>,
    /// Hypotheses of the classic calculation (shown in every report).
    pub assumptions: Vec<String>,
    /// Selected objects, in model order (what the simulated mode follows).
    pub objects: Vec<String>,
    /// Connections between selected objects, in model order (before any cut).
    pub links: Vec<(String, String)>,
    pub scenario: Option<Scenario>,
}

/// Name of the activity of a transporter trip between two objects.
pub fn transport_name(from: &str, to: &str) -> String {
    format!("transporte {from}→{to}")
}

/// Name of the activity of a timed direct connection.
pub fn transfer_name(from: &str, to: &str) -> String {
    format!("transferência {from}→{to}")
}

impl ActivityNetwork {
    /// Network from activities and precedences: adds the start and end milestones, linked
    /// to every activity without a predecessor and without a successor. Refuses cycles.
    pub fn new(
        mut activities: Vec<Activity>,
        precedences: Vec<(usize, usize)>,
    ) -> Result<Self, PertError> {
        let n = activities.len();
        if let Some(&(a, b)) = precedences.iter().find(|&&(a, b)| a >= n || b >= n) {
            return Err(PertError::Invalid(format!(
                "precedência ({a}, {b}) fora das {n} atividades"
            )));
        }
        let mut seen = BTreeSet::new();
        let mut edges: Vec<(usize, usize)> = precedences
            .into_iter()
            .filter(|&(a, b)| a != b && seen.insert((a, b)))
            .collect();
        let (start, end) = (n, n + 1);
        activities.push(Activity::milestone("início"));
        activities.push(Activity::milestone("fim"));
        let has_pred: Vec<bool> = (0..n).map(|v| edges.iter().any(|e| e.1 == v)).collect();
        let has_succ: Vec<bool> = (0..n).map(|v| edges.iter().any(|e| e.0 == v)).collect();
        edges.extend((0..n).filter(|&v| !has_pred[v]).map(|v| (start, v)));
        edges.extend((0..n).filter(|&v| !has_succ[v]).map(|v| (v, end)));
        if n == 0 {
            edges.push((start, end));
        }
        let net = ActivityNetwork {
            activities,
            precedences: edges,
            start,
            end,
            warnings: Vec::new(),
            assumptions: Vec::new(),
            objects: Vec::new(),
            links: Vec::new(),
            scenario: None,
        };
        if net.order().len() != net.activities.len() {
            return Err(PertError::Cycle);
        }
        Ok(net)
    }

    /// Network from a table `(name, duration, predecessors by name)`, the way textbooks
    /// write it.
    pub fn from_table(rows: &[(&str, Dist, &[&str])]) -> Result<Self, PertError> {
        let index: HashMap<&str, usize> = rows.iter().enumerate().map(|(i, r)| (r.0, i)).collect();
        let mut edges = Vec::new();
        for (i, (name, _, preds)) in rows.iter().enumerate() {
            for p in *preds {
                let Some(&j) = index.get(p) else {
                    return Err(PertError::UnknownObject(format!(
                        "{p} (predecessora de {name})"
                    )));
                };
                edges.push((j, i));
            }
        }
        let acts = rows
            .iter()
            .map(|(name, d, _)| Activity::new(*name, d.clone()))
            .collect();
        ActivityNetwork::new(acts, edges)
    }

    pub fn len(&self) -> usize {
        self.activities.len()
    }

    pub fn is_empty(&self) -> bool {
        self.activities.is_empty()
    }

    /// Index of the activity with this name.
    pub fn index(&self, name: &str) -> Option<usize> {
        self.activities.iter().position(|a| a.name == name)
    }

    /// Predecessors and successors of every activity, in precedence order.
    pub fn adjacency(&self) -> (Vec<Vec<usize>>, Vec<Vec<usize>>) {
        let n = self.activities.len();
        let mut preds = vec![Vec::new(); n];
        let mut succs = vec![Vec::new(); n];
        for &(a, b) in &self.precedences {
            succs[a].push(b);
            preds[b].push(a);
        }
        (preds, succs)
    }

    /// Topological order (Kahn, smallest index first). Shorter than `len()` with a cycle.
    pub fn order(&self) -> Vec<usize> {
        let n = self.activities.len();
        let (preds, succs) = self.adjacency();
        let mut indeg: Vec<usize> = preds.iter().map(Vec::len).collect();
        let mut ready: BTreeSet<usize> = (0..n).filter(|&v| indeg[v] == 0).collect();
        let mut order = Vec::with_capacity(n);
        while let Some(v) = ready.pop_first() {
            order.push(v);
            for &w in &succs[v] {
                indeg[w] -= 1;
                if indeg[w] == 0 {
                    ready.insert(w);
                }
            }
        }
        order
    }

    /// Mean duration of every activity.
    pub fn mean_durations(&self) -> Vec<f64> {
        self.activities.iter().map(Activity::mean).collect()
    }
}

/// Derives the activity network of `selection` (the most probable scenario when
/// `options.weighted` is `Scenarios`).
pub fn derive(
    model: &Model,
    selection: &Selection,
    options: &DeriveOptions,
) -> Result<ActivityNetwork, PertError> {
    let mut all = derive_scenarios(model, selection, options)?;
    let mut first = all.swap_remove(0);
    if all.len() + 1 > 1 {
        first.warnings.push(format!(
            "{} cenários de ramos ponderados; esta rede é o mais provável ({:.1}%)",
            all.len() + 1,
            100.0 * first.scenario.as_ref().map_or(1.0, |s| s.probability)
        ));
    }
    Ok(first)
}

/// Every network of `selection`: one with `WeightedBranch::MostLikely`, one per branch
/// combination of the weighted OR outputs with `Scenarios` (at most [`MAX_SCENARIOS`]),
/// most probable first.
pub fn derive_scenarios(
    model: &Model,
    selection: &Selection,
    options: &DeriveOptions,
) -> Result<Vec<ActivityNetwork>, PertError> {
    let base = Base::new(model, selection)?;
    let choices = base.or_choices(options.weighted);
    let mut combos: Vec<(Vec<usize>, f64)> = vec![(Vec::new(), 1.0)];
    let mut truncated = false;
    for c in &choices {
        let mut next = Vec::new();
        for (picked, p) in &combos {
            for (k, (_, w)) in c.options.iter().enumerate() {
                if next.len() >= MAX_SCENARIOS {
                    truncated = true;
                    break;
                }
                let mut v = picked.clone();
                v.push(k);
                next.push((v, p * w));
            }
        }
        combos = next;
    }
    // Most probable first; ties keep enumeration order.
    combos.sort_by(|a, b| b.1.total_cmp(&a.1));
    let scenario_mode = choices.iter().any(|c| c.options.len() > 1);
    combos
        .into_iter()
        .map(|(picked, p)| {
            let mut net = base.build(&choices, &picked)?;
            if scenario_mode {
                let label = choices
                    .iter()
                    .zip(&picked)
                    .filter(|(c, _)| c.options.len() > 1)
                    .map(|(c, &k)| {
                        let (to, _) = c.options[k];
                        format!("{}→{}", base.names[c.node], base.names[to])
                    })
                    .collect::<Vec<_>>()
                    .join(", ");
                net.scenario = Some(Scenario {
                    label,
                    probability: p,
                });
            }
            if truncated {
                net.warnings
                    .push(format!("cenários limitados aos {MAX_SCENARIOS} primeiros"));
            }
            Ok(net)
        })
        .collect()
}

/// One OR output: which successors remain possible and with what probability.
struct OrChoice {
    node: usize,
    /// `(successor, probability)`; one entry when the branch is fixed.
    options: Vec<(usize, f64)>,
}

/// The selection resolved to a graph of local indices, cycles already cut.
struct Base<'a> {
    model: &'a Model,
    /// Selected objects in model order.
    names: Vec<String>,
    defs: Vec<&'a ObjectDef>,
    /// Connections inside the selection, in model order.
    raw: Vec<(usize, usize)>,
    /// `raw` minus the back connections of rework loops.
    edges: Vec<(usize, usize)>,
    factor: Vec<f64>,
    warnings: Vec<String>,
}

impl<'a> Base<'a> {
    fn new(model: &'a Model, selection: &Selection) -> Result<Self, PertError> {
        let by_name: HashMap<&str, usize> = model
            .objects
            .iter()
            .enumerate()
            .map(|(i, o)| (o.name.as_str(), i))
            .collect();
        let lookup = |name: &str| {
            by_name
                .get(name)
                .copied()
                .ok_or_else(|| PertError::UnknownObject(name.to_string()))
        };
        let mut warnings = Vec::new();
        let mut chosen = vec![false; model.objects.len()];
        match selection {
            Selection::Objects(names) => {
                for n in names {
                    chosen[lookup(n)?] = true;
                }
            }
            Selection::Between { from, to } => {
                let (a, b) = (lookup(from)?, lookup(to)?);
                let fwd = reach(model, &by_name, a, true);
                let bwd = reach(model, &by_name, b, false);
                for i in 0..chosen.len() {
                    chosen[i] = fwd[i] && bwd[i];
                }
                if !chosen[a] || !chosen[b] {
                    return Err(PertError::NoPath {
                        from: from.clone(),
                        to: to.clone(),
                    });
                }
            }
        }
        let mobile: Vec<&str> = model
            .objects
            .iter()
            .zip(&chosen)
            .filter(|(o, c)| **c && is_mobile(&o.kind))
            .map(|(o, _)| o.name.as_str())
            .collect();
        if !mobile.is_empty() {
            warnings.push(format!(
                "recursos móveis não são atividades e ficaram de fora: {} (as viagens entram \
                 como atividades de transporte das ligações)",
                mobile.join(", ")
            ));
        }
        let mut local = HashMap::new();
        let mut names = Vec::new();
        let mut defs = Vec::new();
        for (o, c) in model.objects.iter().zip(&chosen) {
            if *c && !is_mobile(&o.kind) {
                local.insert(o.name.as_str(), names.len());
                names.push(o.name.clone());
                defs.push(o);
            }
        }
        if names.is_empty() {
            return Err(PertError::EmptySelection);
        }
        let mut seen = BTreeSet::new();
        let raw: Vec<(usize, usize)> = model
            .connections
            .iter()
            .filter_map(|(a, b)| Some((*local.get(a.as_str())?, *local.get(b.as_str())?)))
            .filter(|&(a, b)| a != b && seen.insert((a, b)))
            .collect();
        let mut base = Base {
            model,
            names,
            defs,
            edges: raw.clone(),
            raw,
            factor: Vec::new(),
            warnings,
        };
        base.factor = vec![1.0; base.names.len()];
        base.cut_cycles();
        Ok(base)
    }

    /// Output ports of `node` in the whole model (connection order), as model names.
    fn ports(&self, node: usize) -> Vec<&str> {
        let name = &self.names[node];
        self.model
            .connections
            .iter()
            .filter(|(a, _)| a == name)
            .map(|(_, b)| b.as_str())
            .collect()
    }

    /// Probability that `node` sends an item to `to` (`None` when the rule does not say).
    fn branch_probability(&self, node: usize, to: usize) -> Option<f64> {
        let OutputRule::Weighted(w) = &self.defs[node].output else {
            return None;
        };
        let ports = self.ports(node);
        let total: f64 = w.iter().take(ports.len()).sum();
        let port = ports.iter().position(|p| *p == self.names[to])?;
        (total > 0.0).then(|| w.get(port).copied().unwrap_or(0.0) / total)
    }

    /// Finds the back connections (iterative DFS from the roots, in model order), removes
    /// them and multiplies the activities of each loop by `1 / (1 - p)`.
    fn cut_cycles(&mut self) {
        let n = self.names.len();
        let mut out = vec![Vec::new(); n];
        let mut indeg = vec![0usize; n];
        for &(a, b) in &self.raw {
            out[a].push(b);
            indeg[b] += 1;
        }
        #[derive(Clone, Copy, PartialEq)]
        enum Mark {
            New,
            OnStack,
            Done,
        }
        let mut mark = vec![Mark::New; n];
        let mut back = Vec::new();
        for root in (0..n).filter(|&v| indeg[v] == 0).chain(0..n) {
            if mark[root] != Mark::New {
                continue;
            }
            let mut stack = vec![(root, 0usize)];
            mark[root] = Mark::OnStack;
            while let Some(top) = stack.last_mut() {
                let v = top.0;
                if top.1 < out[v].len() {
                    let w = out[v][top.1];
                    top.1 += 1;
                    match mark[w] {
                        Mark::New => {
                            mark[w] = Mark::OnStack;
                            stack.push((w, 0));
                        }
                        Mark::OnStack => back.push((v, w)),
                        Mark::Done => {}
                    }
                } else {
                    mark[v] = Mark::Done;
                    stack.pop();
                }
            }
        }
        self.edges.retain(|e| !back.contains(e));
        for &(u, v) in &back {
            // The loop: from `v` forward to `u` on what is left of the graph.
            let fwd = local_reach(n, &self.edges, v, true);
            let bwd = local_reach(n, &self.edges, u, false);
            let members: Vec<usize> = (0..n).filter(|&i| fwd[i] && bwd[i]).collect();
            let list = members
                .iter()
                .map(|&i| self.names[i].as_str())
                .collect::<Vec<_>>()
                .join(", ");
            match self.branch_probability(u, v) {
                Some(p) if p < 1.0 => {
                    let f = 1.0 / (1.0 - p);
                    for &i in &members {
                        self.factor[i] *= f;
                    }
                    self.warnings.push(format!(
                        "ciclo de retrabalho {} → {} cortado: as durações de {list} foram \
                         multiplicadas por 1/(1 - p) = {f:.4} (p = {p:.4}); o modo simulado mede \
                         o retrabalho de fato",
                        self.names[u], self.names[v]
                    ));
                }
                Some(_) => self.warnings.push(format!(
                    "ciclo {} → {} com probabilidade de volta 1: a rede foi cortada sem fator",
                    self.names[u], self.names[v]
                )),
                None => self.warnings.push(format!(
                    "ciclo {} → {} cortado sem fator: a regra de saída de {} não dá a \
                     probabilidade de retrabalho (use Weighted, ou o modo simulado)",
                    self.names[u], self.names[v], self.names[u]
                )),
            }
        }
    }

    /// AND split: separators and combiners that release everything (`Batch`).
    fn is_and_split(&self, node: usize) -> bool {
        matches!(
            self.defs[node].kind,
            ObjectKind::Separator { .. }
                | ObjectKind::Combiner {
                    mode: CombineMode::Batch,
                    ..
                }
        )
    }

    /// One choice per OR node (an object with several outputs that is not an AND split).
    fn or_choices(&self, policy: WeightedBranch) -> Vec<OrChoice> {
        let mut choices = Vec::new();
        for node in 0..self.names.len() {
            let succ: Vec<usize> = self
                .edges
                .iter()
                .filter(|e| e.0 == node)
                .map(|e| e.1)
                .collect();
            if succ.len() < 2 || self.is_and_split(node) {
                continue;
            }
            let weights: Option<Vec<f64>> = succ
                .iter()
                .map(|&s| self.branch_probability(node, s))
                .collect();
            let options = match (weights, policy) {
                (Some(w), WeightedBranch::Scenarios) => {
                    let total: f64 = w.iter().sum();
                    succ.iter()
                        .zip(&w)
                        .filter(|(_, p)| **p > 0.0)
                        .map(|(&s, &p)| (s, if total > 0.0 { p / total } else { 0.0 }))
                        .collect()
                }
                (Some(w), WeightedBranch::MostLikely) => {
                    let best = (0..succ.len()).fold(0, |b, k| if w[k] > w[b] { k } else { b });
                    vec![(succ[best], 1.0)]
                }
                (None, _) => vec![(succ[0], 1.0)],
            };
            let options = if options.is_empty() {
                vec![(succ[0], 1.0)]
            } else {
                options
            };
            choices.push(OrChoice { node, options });
        }
        choices
    }

    /// The network for one pick of every OR choice.
    fn build(&self, choices: &[OrChoice], picked: &[usize]) -> Result<ActivityNetwork, PertError> {
        let n = self.names.len();
        let mut warnings = self.warnings.clone();
        let mut edges = self.edges.clone();
        for (c, &k) in choices.iter().zip(picked) {
            let keep = c.options[k].0;
            let dropped: Vec<&str> = edges
                .iter()
                .filter(|e| e.0 == c.node && e.1 != keep)
                .map(|e| self.names[e.1].as_str())
                .collect();
            if c.options.len() == 1 {
                let rule = match &self.defs[c.node].output {
                    OutputRule::Weighted(_) => "Weighted: ficou o ramo mais provável".to_string(),
                    other => format!(
                        "{}: os ramos são tratados como servidores paralelos equivalentes e a \
                         rede usa o primeiro",
                        rule_name(other)
                    ),
                };
                warnings.push(format!(
                    "{} escolhe uma saída (OU, não E). {rule}: {} (fora: {})",
                    self.names[c.node],
                    self.names[keep],
                    dropped.join(", ")
                ));
            }
            edges.retain(|e| e.0 != c.node || e.1 == keep);
        }
        // Keep what the roots of the cut graph still reach.
        let mut indeg = vec![0usize; n];
        for &(_, b) in &self.edges {
            indeg[b] += 1;
        }
        let mut alive = vec![false; n];
        let mut queue: VecDeque<usize> = (0..n).filter(|&v| indeg[v] == 0).collect();
        for &v in &queue {
            alive[v] = true;
        }
        while let Some(v) = queue.pop_front() {
            for &(a, b) in &edges {
                if a == v && !alive[b] {
                    alive[b] = true;
                    queue.push_back(b);
                }
            }
        }
        let gone: Vec<&str> = (0..n)
            .filter(|&v| !alive[v])
            .map(|v| self.names[v].as_str())
            .collect();
        if !gone.is_empty() {
            warnings.push(format!(
                "fora da rede por estarem só nos ramos não escolhidos: {}",
                gone.join(", ")
            ));
        }
        edges.retain(|&(a, b)| alive[a] && alive[b]);
        let parts = components(n, &edges, &alive);
        if parts > 1 {
            warnings.push(format!(
                "a seleção forma {parts} partes desconectadas: cada parte vai do início ao fim \
                 da rede em paralelo"
            ));
        }

        let router = utsim_objects::network::router(self.model.network.as_ref());
        let mut activities = Vec::new();
        let mut index = vec![usize::MAX; n];
        for v in (0..n).filter(|&v| alive[v]) {
            index[v] = activities.len();
            let succ = edges.iter().filter(|e| e.0 == v).count();
            let mut act = object_activity(self.defs[v], succ);
            act.factor = self.factor[v];
            activities.push(act);
        }
        let mut precedences = Vec::new();
        let mut transport_used = false;
        for &(a, b) in &edges {
            let (from, to) = (self.defs[a], self.defs[b]);
            let link = if let Some(t) = &from.transport {
                transport_used = true;
                Some(transport_activity(self.model, &*router, from, to, t))
            } else {
                self.model.effective_transfer(from).and_then(|spec| {
                    let path = match spec.path {
                        TransferPath::Straight => vec![from.position, to.position],
                        TransferPath::Network => router.route(from.position, to.position),
                    };
                    let time = spec.delay + path_length(&path) / spec.speed;
                    (time > 0.0).then(|| Activity {
                        name: transfer_name(&from.name, &to.name),
                        kind: ActivityKind::Transfer,
                        object: None,
                        label: "Transferência".into(),
                        parts: vec![Dist::Const(time)],
                        factor: 1.0,
                        note: Some(format!(
                            "delay {} + distância {:.2} / velocidade {}",
                            spec.delay,
                            path_length(&path),
                            spec.speed
                        )),
                    })
                })
            };
            match link {
                Some(act) => {
                    let k = activities.len();
                    activities.push(act);
                    precedences.push((index[a], k));
                    precedences.push((k, index[b]));
                }
                None => precedences.push((index[a], index[b])),
            }
        }

        let mut assumptions = vec![
            "precedências fim-início pelas ligações do modelo; atividade no nó".to_string(),
            "filas valem 0 e recursos compartilhados (operadores, transportadores, capacidades) \
             não esperam: só o modo simulado mede essas esperas, por isso os números dos modos \
             não são comparáveis sem essa ressalva"
                .to_string(),
        ];
        if transport_used {
            assumptions.push(
                "transporte: viagem vazia da posição do recurso até a origem + carga + viagem \
                 carregada + descarga, com a distância da rede de caminhos e o perfil de \
                 aceleração"
                    .to_string(),
            );
        }
        let stops: Vec<&str> = (0..n)
            .filter(|&v| {
                alive[v] && (self.defs[v].downtime.is_some() || self.defs[v].shifts.is_some())
            })
            .map(|v| self.names[v].as_str())
            .collect();
        if !stops.is_empty() {
            assumptions.push(format!(
                "quebras e turnos ficam de fora do cálculo clássico ({})",
                stops.join(", ")
            ));
        }
        let mut net = ActivityNetwork::new(activities, precedences)?;
        net.warnings = warnings;
        net.assumptions = assumptions;
        net.objects = self.names.clone();
        net.links = self
            .raw
            .iter()
            .map(|&(a, b)| (self.names[a].clone(), self.names[b].clone()))
            .collect();
        Ok(net)
    }
}

fn rule_name(rule: &OutputRule) -> &'static str {
    match rule {
        OutputRule::FirstAvailable => "FirstAvailable",
        OutputRule::RoundRobin => "RoundRobin",
        OutputRule::Random => "Random",
        OutputRule::ByLabel(_) => "ByLabel",
        OutputRule::Weighted(_) => "Weighted",
        OutputRule::Script(_) => "Script",
    }
}

/// Objects reachable from (`forward`) or reaching (`!forward`) object `start`.
fn reach(model: &Model, by_name: &HashMap<&str, usize>, start: usize, forward: bool) -> Vec<bool> {
    let edges: Vec<(usize, usize)> = model
        .connections
        .iter()
        .filter_map(|(a, b)| Some((*by_name.get(a.as_str())?, *by_name.get(b.as_str())?)))
        .collect();
    local_reach(model.objects.len(), &edges, start, forward)
}

fn local_reach(n: usize, edges: &[(usize, usize)], start: usize, forward: bool) -> Vec<bool> {
    let mut seen = vec![false; n];
    seen[start] = true;
    let mut queue = VecDeque::from([start]);
    while let Some(v) = queue.pop_front() {
        for &(a, b) in edges {
            let (x, y) = if forward { (a, b) } else { (b, a) };
            if x == v && !seen[y] {
                seen[y] = true;
                queue.push_back(y);
            }
        }
    }
    seen
}

/// Weakly connected components among the alive nodes.
fn components(n: usize, edges: &[(usize, usize)], alive: &[bool]) -> usize {
    let mut comp = vec![usize::MAX; n];
    let mut count = 0;
    for s in (0..n).filter(|&v| alive[v]) {
        if comp[s] != usize::MAX {
            continue;
        }
        comp[s] = count;
        let mut queue = VecDeque::from([s]);
        while let Some(v) = queue.pop_front() {
            for &(a, b) in edges {
                for (x, y) in [(a, b), (b, a)] {
                    if x == v && comp[y] == usize::MAX {
                        comp[y] = count;
                        queue.push_back(y);
                    }
                }
            }
        }
        count += 1;
    }
    count
}

/// Activity of one object; `successors` is its number of outputs inside the network.
fn object_activity(obj: &ObjectDef, successors: usize) -> Activity {
    let (label, parts, note): (&str, Vec<Dist>, Option<String>) = match &obj.kind {
        ObjectKind::Source { .. } => ("Source", vec![], Some("marco: chegada".into())),
        ObjectKind::Sink => ("Sink", vec![], Some("marco: saída".into())),
        ObjectKind::Queue { .. } => (
            "Queue",
            vec![],
            Some("espera em fila: 0 no cálculo clássico, medida no modo simulado".into()),
        ),
        ObjectKind::Processor {
            capacity,
            setup,
            process,
            operators,
        } => {
            let mut notes = Vec::new();
            if *capacity > 1 {
                notes.push(format!(
                    "capacidade {capacity} não muda a duração de um item"
                ));
            }
            if let Some(op) = operators {
                notes.push(format!(
                    "operadores da equipe {} só no modo simulado",
                    op.team
                ));
            }
            (
                "Processor",
                setup.iter().cloned().chain([process.clone()]).collect(),
                (!notes.is_empty()).then(|| notes.join("; ")),
            )
        }
        ObjectKind::Combiner { setup, process, .. } => (
            "Combiner",
            setup.iter().cloned().chain([process.clone()]).collect(),
            Some("junção E: começa quando a última entrada chega".into()),
        ),
        ObjectKind::Separator { process, .. } => (
            "Separator",
            vec![process.clone()],
            (successors > 1).then(|| "divisão E: as saídas seguem em paralelo".into()),
        ),
        ObjectKind::Conveyor { length, speed, .. } => (
            "Conveyor",
            vec![Dist::Const(length / speed)],
            Some("tempo mínimo de percurso (length / speed)".into()),
        ),
        ObjectKind::Rack { dwell, .. } => (
            "Rack",
            dwell.iter().cloned().collect(),
            dwell.is_none().then(|| "sem dwell: 0".into()),
        ),
        // Mobile resources never reach here (filtered out of the selection).
        ObjectKind::Transporter { .. } | ObjectKind::Dispatcher { .. } => ("Recurso", vec![], None),
    };
    Activity {
        name: obj.name.clone(),
        kind: ActivityKind::Object,
        object: Some(obj.name.clone()),
        label: label.into(),
        parts,
        factor: 1.0,
        note,
    }
}

/// Trip of the mobile resource `carrier` (a transporter, or the first one of a
/// dispatcher's team) carrying an item from `from` to `to`.
fn transport_activity(
    model: &Model,
    router: &dyn utsim_core::Router,
    from: &ObjectDef,
    to: &ObjectDef,
    carrier: &str,
) -> Activity {
    let find = |name: &str| model.objects.iter().find(|o| o.name == name);
    let vehicle = match find(carrier).map(|o| &o.kind) {
        Some(ObjectKind::Dispatcher { team, .. }) => team.first().and_then(|m| find(m)),
        _ => find(carrier),
    };
    let mut act = Activity {
        name: transport_name(&from.name, &to.name),
        kind: ActivityKind::Transport,
        object: None,
        label: "Transporte".into(),
        parts: Vec::new(),
        factor: 1.0,
        note: None,
    };
    if let Some(
        v @ ObjectDef {
            kind:
                ObjectKind::Transporter {
                    speed,
                    acceleration,
                    deceleration,
                    load_time,
                    unload_time,
                    ..
                },
            ..
        },
    ) = vehicle
    {
        let trip = |a: [f64; 3], b: [f64; 3]| {
            let len = if a == b {
                0.0
            } else {
                path_length(&router.route(a, b))
            };
            (len, travel_time(len, *speed, *acceleration, *deceleration))
        };
        let (empty_len, empty) = trip(v.position, from.position);
        let (loaded_len, loaded) = trip(from.position, to.position);
        act.parts = vec![
            Dist::Const(empty),
            load_time.clone(),
            Dist::Const(loaded),
            unload_time.clone(),
        ];
        act.note = Some(format!(
            "{}: viagem vazia {empty_len:.2} ({empty:.2}) + carga + viagem carregada \
             {loaded_len:.2} ({loaded:.2}) + descarga; reta de {:.2}",
            v.name,
            distance(from.position, to.position)
        ));
    } else {
        act.note = Some(format!("recurso '{carrier}' não encontrado: duração 0"));
    }
    act
}
