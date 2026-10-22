//! Token flows (phase 6, contract): logic drawn as a graph of activities, in the
//! spirit of the commercial tools' fluxo de processo. Tokens are not items: they are lightweight
//! "threads of logic" that wait, decide, hold resources and can carry an item
//! reference. A flow runs as one block of the simulation, listening to the physical
//! objects through the kernel (subscriptions) and acting on them (creating items,
//! setting labels).
//!
//! The schema below is the contract; the flow engine front implements [`Flow`].

use std::collections::{BTreeMap, HashMap, HashSet};

use serde::{Deserialize, Serialize};
use utsim_core::{Behavior, BlockId, Ctx, Dist, Item, State};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct FlowDef {
    pub name: String,
    /// Shared counting resources (e.g. "docas" with capacity 3).
    #[serde(default)]
    pub resources: Vec<ResourceDef>,
    pub activities: Vec<ActivityDef>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ResourceDef {
    pub name: String,
    pub capacity: usize,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ActivityDef {
    pub name: String,
    pub kind: Activity,
    /// Where tokens go next. Most activities use one; `Decide` picks among several;
    /// `Sink` has none.
    #[serde(default)]
    pub next: Vec<String>,
}

/// Which object event a flow reacts to.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum BlockEvent {
    Entry,
    Exit,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum DecideRule {
    /// Random branch with relative weights, one per `next`.
    Probability(Vec<f64>),
    /// The branch number (1-based) is the value of this token label.
    ByLabel(String),
    /// One condition per `next`: the first true one wins; none true = last branch.
    Condition(Vec<String>),
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum Activity {
    /// Creates tokens at random intervals.
    Source {
        interarrival: Dist,
        #[serde(default)]
        first_at_zero: bool,
        #[serde(default)]
        limit: Option<u64>,
    },
    /// Creates `count` tokens at each listed time.
    Schedule { times: Vec<(f64, u32)> },
    /// Creates a token, carrying the item, every time `event` happens at `block`.
    OnEvent { block: String, event: BlockEvent },
    /// Holds the token for a sampled time.
    Delay { time: Dist },
    /// Sends the token to one of `next`.
    Decide { rule: DecideRule },
    /// Sets labels from expressions: `name` sets a token label, `item.name` a label
    /// of the carried item.
    Assign { labels: BTreeMap<String, String> },
    /// Waits until `count` units of the resource are free, then holds them.
    Acquire {
        resource: String,
        #[serde(default = "one")]
        count: usize,
    },
    Release {
        resource: String,
        #[serde(default = "one")]
        count: usize,
    },
    /// Waits for the next `event` at `block`; the token then carries that item.
    WaitFor { block: String, event: BlockEvent },
    /// Creates an item inside object `into` (input `port`), carried by the token.
    CreateItem {
        into: String,
        #[serde(default)]
        port: usize,
        #[serde(default)]
        labels: BTreeMap<String, String>,
    },
    /// Collects `size` tokens and continues with one of them.
    Batch { size: usize },
    /// Ends the token.
    Sink,
}

fn one() -> usize {
    1
}

impl FlowDef {
    /// Structural checks; `objects` are the model's object names. Messages in Portuguese.
    pub fn validate(&self, objects: &HashSet<&str>) -> Vec<String> {
        let mut errors = Vec::new();
        let f = &self.name;
        let mut names = HashSet::new();
        for a in &self.activities {
            if !names.insert(a.name.as_str()) {
                errors.push(format!("{f}: atividade repetida '{}'", a.name));
            }
        }
        let resources: HashSet<&str> = self.resources.iter().map(|r| r.name.as_str()).collect();
        for r in &self.resources {
            if r.capacity == 0 {
                errors.push(format!("{f}: recurso '{}' com capacidade 0", r.name));
            }
        }
        for a in &self.activities {
            let at = format!("{f}.{}", a.name);
            for n in &a.next {
                if !names.contains(n.as_str()) {
                    errors.push(format!("{at}: próxima atividade '{n}' não existe"));
                }
            }
            let wants_next = !matches!(a.kind, Activity::Sink);
            if wants_next && a.next.is_empty() {
                errors.push(format!("{at}: precisa de uma próxima atividade (next)"));
            }
            match &a.kind {
                Activity::OnEvent { block, .. }
                | Activity::WaitFor { block, .. }
                | Activity::CreateItem { into: block, .. } => {
                    if !objects.contains(block.as_str()) {
                        errors.push(format!("{at}: objeto '{block}' não existe"));
                    }
                }
                Activity::Acquire { resource, .. } | Activity::Release { resource, .. } => {
                    if !resources.contains(resource.as_str()) {
                        errors.push(format!("{at}: recurso '{resource}' não existe"));
                    }
                }
                Activity::Decide {
                    rule: DecideRule::Probability(w),
                } if w.len() != a.next.len() => {
                    errors.push(format!("{at}: um peso por próxima atividade"));
                }
                Activity::Decide {
                    rule: DecideRule::Condition(c),
                } if c.len() != a.next.len() => {
                    errors.push(format!("{at}: uma condição por próxima atividade"));
                }
                Activity::Batch { size: 0 } => errors.push(format!("{at}: size deve ser >= 1")),
                _ => {}
            }
        }
        errors
    }
}

/// The flow engine: one simulation block per flow. Contract stub — the flow engine
/// front implements the activities.
pub struct Flow {
    pub def: FlowDef,
    /// Object name → block id, for activities that reference objects.
    pub objects: HashMap<String, BlockId>,
}

impl Flow {
    pub fn new(def: FlowDef, objects: HashMap<String, BlockId>) -> Self {
        Flow { def, objects }
    }
}

impl Behavior for Flow {
    fn kind(&self) -> &'static str {
        "flow"
    }

    fn reset(&mut self, cx: &mut Ctx) {
        cx.set_state(State::Idle);
        cx.error("fluxo de tokens: em construção na fase 6");
    }

    fn accepts(&self, _item: &Item, _content: usize, _port: usize) -> bool {
        false
    }
}
