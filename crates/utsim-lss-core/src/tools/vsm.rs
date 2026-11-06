//! Value stream map (P4, `docs/plano_lean_seis_sigma.md`, 2.2 and 2.4): the current state
//! (Medir) and the future state (Melhorar) as deliverables. Each process is a row (cycle
//! time, setup, availability, operators, the inventory before it and whether it adds
//! value); the deliverable keeps the daily demand, the available time per day, the time
//! unit and which state it is. From these the **lead time ladder** follows: the takt
//! time, the wait of each inventory (inventory × takt, the time the inventory takes to be
//! consumed at the pace of the customer), the lead time, the value-added time, the process
//! cycle efficiency and the processes above the takt. A current map is filled from the
//! UTSim model (`utsim-diagram`); the future map starts as a copy of the current one.

use serde_json::Value;

use super::{Deliverable, ItemRef, items, new_item};
use crate::id::Id;
use crate::op::Edit;
use crate::state::State;

/// A process of the map.
pub const PROCESS_ITEM: &str = "processo_vsm";

pub mod fields {
    // Process row.
    pub const NAME: &str = "nome";
    pub const CYCLE: &str = "tempo_ciclo";
    pub const SETUP: &str = "setup";
    /// Percentage, 0 to 100.
    pub const UPTIME: &str = "disponibilidade";
    pub const OPERATORS: &str = "operadores";
    /// Units waiting before the process.
    pub const INVENTORY: &str = "estoque_antes";
    pub const VALUE_ADDED: &str = "agrega_valor";
    /// `modelo` when the row came from the UTSim model.
    pub const ORIGIN: &str = "origem";
    // Deliverable.
    /// Units a day the customer takes.
    pub const DEMAND: &str = "demanda_dia";
    /// Working time available per day, in the time unit of the map.
    pub const AVAILABLE: &str = "tempo_disponivel_dia";
    pub const TIME_UNIT: &str = "unidade_tempo";
    /// `atual` or `futuro`.
    pub const STATE: &str = "estado_vsm";
    /// The current map a future one was copied from.
    pub const FROM: &str = "copiado_de";
}

pub const CURRENT: &str = "atual";
pub const FUTURE: &str = "futuro";

/// A process row, read.
#[derive(Debug, Clone, PartialEq)]
pub struct Process {
    pub id: Id,
    pub name: String,
    pub cycle: Option<f64>,
    pub setup: Option<f64>,
    /// Fraction 0 to 1 (the row keeps the percentage).
    pub uptime: Option<f64>,
    pub operators: Option<f64>,
    pub inventory: Option<f64>,
    pub value_added: bool,
    pub from_model: bool,
}

impl Process {
    pub fn read(i: &ItemRef) -> Self {
        use fields as f;
        let n = |k: &str| i.entity.number(k);
        Self {
            id: i.id,
            name: i.text(f::NAME),
            cycle: n(f::CYCLE),
            setup: n(f::SETUP),
            uptime: n(f::UPTIME).map(|p| (p / 100.0).clamp(0.0, 1.0)),
            operators: n(f::OPERATORS),
            inventory: n(f::INVENTORY),
            value_added: i.entity.boolean(f::VALUE_ADDED),
            from_model: i.text(f::ORIGIN) == "modelo",
        }
    }

    /// The cycle time divided by the availability (the time a unit really takes there).
    pub fn effective_cycle(&self) -> Option<f64> {
        let c = self.cycle?;
        Some(match self.uptime {
            Some(u) if u > 0.0 => c / u,
            _ => c,
        })
    }
}

pub fn processes(state: &State, del: Id) -> Vec<Process> {
    items(state, del, PROCESS_ITEM)
        .iter()
        .map(Process::read)
        .collect()
}

/// The header of a map, read from the deliverable.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Header {
    pub demand: Option<f64>,
    pub available: Option<f64>,
    pub unit: String,
    pub future: bool,
    pub from: Option<Id>,
}

pub fn header(state: &State, del: Id) -> Header {
    use fields as f;
    let Some(e) = state.entity(del) else {
        return Header::default();
    };
    Header {
        demand: e.number(f::DEMAND).filter(|v| *v > 0.0),
        available: e.number(f::AVAILABLE).filter(|v| *v > 0.0),
        unit: e.text(f::TIME_UNIT),
        future: e.text(f::STATE) == FUTURE,
        from: e.id(f::FROM),
    }
}

/// One step of the ladder: the wait of the inventory before a process (the low step) and
/// the process time (the high step).
#[derive(Debug, Clone, PartialEq)]
pub struct Step {
    pub name: String,
    pub wait: Option<f64>,
    pub process: f64,
    pub value_added: bool,
}

/// The lead time ladder and the indicators of a map.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Ladder {
    /// Available time per day over the daily demand.
    pub takt: Option<f64>,
    pub steps: Vec<Step>,
    /// Sum of the waits (`None` without takt and with some inventory).
    pub wait: Option<f64>,
    pub process: f64,
    pub value_added: f64,
    pub lead_time: Option<f64>,
    /// Value-added time over the lead time.
    pub pce: Option<f64>,
    /// Processes whose effective cycle is above the takt.
    pub over_takt: Vec<String>,
    /// The process with the longest effective cycle.
    pub slowest: Option<String>,
    /// Inventory in days of demand.
    pub inventory_days: Option<f64>,
}

/// The ladder of the processes under a header.
pub fn ladder(procs: &[Process], h: &Header) -> Ladder {
    let takt = match (h.available, h.demand) {
        (Some(a), Some(d)) => Some(a / d),
        _ => None,
    };
    let mut steps = Vec::new();
    let mut wait = Some(0.0);
    let mut inv_total = 0.0;
    for p in procs {
        let w = match (p.inventory, takt) {
            (None, _) => Some(0.0),
            (Some(i), Some(t)) => Some(i.max(0.0) * t),
            (Some(i), None) if i <= 0.0 => Some(0.0),
            (Some(_), None) => None,
        };
        inv_total += p.inventory.unwrap_or(0.0).max(0.0);
        wait = match (wait, w) {
            (Some(a), Some(b)) => Some(a + b),
            _ => None,
        };
        steps.push(Step {
            name: p.name.clone(),
            wait: w,
            process: p.cycle.unwrap_or(0.0),
            value_added: p.value_added,
        });
    }
    let process: f64 = steps.iter().map(|s| s.process).sum();
    let value_added: f64 = steps
        .iter()
        .filter(|s| s.value_added)
        .map(|s| s.process)
        .sum();
    let lead_time = wait.map(|w| w + process);
    let over_takt = match takt {
        Some(t) => procs
            .iter()
            .filter(|p| p.effective_cycle().is_some_and(|c| c > t + 1e-12))
            .map(|p| p.name.clone())
            .collect(),
        None => Vec::new(),
    };
    let slowest = procs
        .iter()
        .filter_map(|p| p.effective_cycle().map(|c| (c, &p.name)))
        .max_by(|a, b| a.0.total_cmp(&b.0))
        .map(|(_, n)| n.clone());
    Ladder {
        takt,
        steps,
        wait,
        process,
        value_added,
        lead_time,
        pce: lead_time.filter(|l| *l > 0.0).map(|l| value_added / l),
        over_takt,
        slowest,
        inventory_days: h.demand.map(|d| inv_total / d),
    }
}

/// Edits that write the header of a map (the fields left `None` are not touched).
pub fn header_edits(
    del: Id,
    demand: Option<f64>,
    available: Option<f64>,
    unit: Option<&str>,
    future: Option<bool>,
) -> Vec<Edit> {
    use fields as f;
    let r = Deliverable::entity_ref(del);
    let mut e = Vec::new();
    if let Some(v) = demand {
        e.push(Edit::new(r, f::DEMAND, v));
    }
    if let Some(v) = available {
        e.push(Edit::new(r, f::AVAILABLE, v));
    }
    if let Some(v) = unit {
        e.push(Edit::new(r, f::TIME_UNIT, v));
    }
    if let Some(v) = future {
        e.push(Edit::new(r, f::STATE, if v { FUTURE } else { CURRENT }));
    }
    e
}

/// A process of the model, from the table of `utsim-diagram`.
#[derive(Debug, Clone, PartialEq)]
pub struct ModelProcess {
    pub name: String,
    pub cycle: f64,
    pub setup: f64,
    /// Fraction 0 to 1.
    pub uptime: f64,
    pub operators: f64,
    pub inventory: Option<f64>,
    pub value_added: bool,
}

/// Parses the VSM table of `utsim-diagram`
/// (`processo,ciclo,setup,disponibilidade,operadores,estoque_anterior,agrega_valor`).
pub fn from_diagram_csv(csv: &str) -> Vec<ModelProcess> {
    let mut v = Vec::new();
    for line in csv.lines().skip(1) {
        let c = split_csv(line);
        if c.len() < 7 || c[0].trim().is_empty() {
            continue;
        }
        let n = |s: &str| s.trim().parse::<f64>().ok().filter(|x| x.is_finite());
        v.push(ModelProcess {
            name: c[0].trim().to_string(),
            cycle: n(&c[1]).unwrap_or(0.0),
            setup: n(&c[2]).unwrap_or(0.0),
            uptime: n(&c[3]).unwrap_or(1.0),
            operators: n(&c[4]).unwrap_or(0.0),
            inventory: n(&c[5]),
            value_added: c[6].trim() == "true",
        });
    }
    v
}

/// Splits a CSV line with `"` quotes.
fn split_csv(line: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut quoted = false;
    let mut chars = line.chars().peekable();
    while let Some(ch) = chars.next() {
        match ch {
            '"' if quoted && chars.peek() == Some(&'"') => {
                cur.push('"');
                chars.next();
            }
            '"' => quoted = !quoted,
            ',' if !quoted => out.push(std::mem::take(&mut cur)),
            _ => cur.push(ch),
        }
    }
    out.push(cur);
    out
}

/// The edits of the rows of the model's processes added to a map (marked `modelo`).
pub fn model_rows_edits(
    state: &State,
    project: Id,
    del: Id,
    procs: &[ModelProcess],
    mut new_id: impl FnMut() -> Id,
) -> Vec<Edit> {
    use fields as f;
    let mut out = Vec::new();
    for (i, p) in procs.iter().enumerate() {
        let mut e = new_item(
            state,
            project,
            del,
            PROCESS_ITEM,
            new_id(),
            None,
            vec![
                (f::NAME, Value::from(p.name.as_str())),
                (f::CYCLE, Value::from(p.cycle)),
                (f::SETUP, Value::from(p.setup)),
                (f::UPTIME, Value::from((p.uptime * 1000.0).round() / 10.0)),
                (f::OPERATORS, Value::from(p.operators)),
                (f::VALUE_ADDED, Value::from(p.value_added)),
                (f::ORIGIN, Value::from("modelo")),
            ],
        );
        if let Some(inv) = p.inventory {
            let r = e[0].entity;
            e.push(Edit::new(r, f::INVENTORY, (inv * 100.0).round() / 100.0));
        }
        // Orders after the ones already there, in the model's sequence.
        for x in &mut e {
            if x.field == super::fields::item::ORDER {
                if let Some(o) = x.value.as_f64() {
                    x.value = Value::from(o + i as f64);
                }
            }
        }
        out.extend(e);
    }
    out
}

/// The edits that copy a map into another deliverable (the future state from the current
/// one): the header, marked future and pointing at the source, and every row.
pub fn copy_edits(
    state: &State,
    project: Id,
    from: Id,
    to: Id,
    mut new_id: impl FnMut() -> Id,
) -> Vec<Edit> {
    use fields as f;
    let h = header(state, from);
    let mut out = header_edits(to, h.demand, h.available, Some(h.unit.as_str()), Some(true));
    out.push(Edit::new(
        Deliverable::entity_ref(to),
        f::FROM,
        from.to_string(),
    ));
    for (i, it) in items(state, from, PROCESS_ITEM).iter().enumerate() {
        let id = new_id();
        let r = super::item_ref(id);
        let mut e = new_item(state, project, to, PROCESS_ITEM, id, None, Vec::new());
        for x in &mut e {
            if x.field == super::fields::item::ORDER {
                x.value = Value::from(1.0 + i as f64);
            }
        }
        out.extend(e);
        for k in [
            f::NAME,
            f::CYCLE,
            f::SETUP,
            f::UPTIME,
            f::OPERATORS,
            f::INVENTORY,
            f::VALUE_ADDED,
        ] {
            if let Some(v) = it.entity.get(k) {
                out.push(Edit::new(r, k, v.clone()));
            }
        }
    }
    out
}

#[cfg(test)]
mod tests;
