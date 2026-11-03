//! Checks and `utsim check` lines for arrival tables, batches and item types
//! (`docs/plano_chegadas_e_equipes_3d.md`, C1 to C3).

use utsim_core::Dist;
use utsim_objects::{ArrivalSchedule, ArrivalTable, TableMode, TableRepeat};

use crate::{Model, ObjectKind};

/// Errors of a source's `table` (and of `table` together with `schedule`).
pub(crate) fn validate_source(
    name: &str,
    interarrival: &Dist,
    schedule: &Option<ArrivalSchedule>,
    table: &Option<ArrivalTable>,
) -> Vec<String> {
    let Some(t) = table else {
        return Vec::new();
    };
    let mut errors: Vec<String> = t
        .validate()
        .into_iter()
        .map(|e| format!("{name}: {e}"))
        .collect();
    if t.mode == TableMode::Sequence
        && t.repeat == TableRepeat::Loop
        && interarrival.validate().is_ok()
        && interarrival.mean() <= 0.0
    {
        errors.push(format!(
            "{name}: uma sequência em laço precisa de interarrival com média > 0 \
             (senão as linhas se repetem sem fim no mesmo instante)"
        ));
    }
    if schedule.is_some() {
        errors.push(format!(
            "{name}: use `schedule` (chegadas por período) ou `table` (tabela de chegadas), não os dois"
        ));
    }
    errors
}

/// Mean time between items of a source with a table, for the "too many arrivals" check
/// (`NaN` when it cannot be known, infinite when the table never repeats).
pub(crate) fn mean_gap(table: &ArrivalTable, interarrival: &Dist) -> f64 {
    let items = table.items_per_pass();
    match (table.mode, table.repeat) {
        (TableMode::Times, TableRepeat::Cycle(d)) => d / items,
        (TableMode::Sequence, TableRepeat::Loop) => {
            interarrival.mean() * table.rows.len() as f64 / items
        }
        _ => f64::INFINITY,
    }
}

fn num(v: f64) -> String {
    if v.fract() == 0.0 && v.abs() < 1e15 {
        format!("{v:.0}")
    } else {
        format!("{v}")
    }
}

impl Model {
    /// One line per source with an arrival table, for `utsim check`.
    pub fn arrival_lines(&self) -> Vec<String> {
        let mut out = Vec::new();
        for obj in &self.objects {
            let ObjectKind::Source {
                table: Some(t),
                interarrival,
                ..
            } = &obj.kind
            else {
                continue;
            };
            let items = t.items_per_pass();
            let line = match (t.mode, t.repeat) {
                (TableMode::Times, TableRepeat::Cycle(d)) => format!(
                    "{}: horário com {} linha(s), {} item(ns) a cada ciclo de {}",
                    obj.name,
                    t.rows.len(),
                    num(items),
                    num(d)
                ),
                (TableMode::Times, _) => format!(
                    "{}: horário com {} linha(s), {} item(ns) uma vez",
                    obj.name,
                    t.rows.len(),
                    num(items)
                ),
                (TableMode::Sequence, repeat) => format!(
                    "{}: sequência de {} linha(s), {} item(ns) por passagem, intervalo médio {} entre linhas{}",
                    obj.name,
                    t.rows.len(),
                    num(items),
                    num(interarrival.mean()),
                    if repeat == TableRepeat::Loop {
                        ", em laço"
                    } else {
                        ", uma vez"
                    }
                ),
            };
            out.push(line);
        }
        out
    }
}
