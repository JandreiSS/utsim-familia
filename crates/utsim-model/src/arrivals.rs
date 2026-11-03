//! Checks and `utsim check` lines for arrival tables, batches and item types
//! (`docs/plano_chegadas_e_equipes_3d.md`, C1 to C3).

use std::collections::{BTreeMap, HashSet};

use serde::{Deserialize, Serialize};
use utsim_core::{Dist, ITEM_TYPE_LABEL};
use utsim_objects::{ArrivalSchedule, ArrivalTable, ItemTypeSpec, TableMode, TableRepeat};

use crate::{Model, ObjectKind};

/// One item type of the model (`item_types`): what it is called, how it looks in 3D and
/// the constant labels its items start with.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ItemTypeDef {
    pub name: String,
    /// Colour of its items in 3D, "#rrggbb".
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub color: Option<String>,
    /// Look of its items in 3D: an asset of the built-in library or a `.glb` file.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub asset: Option<String>,
    /// Labels every item of the type starts with (the source's and the table row's
    /// labels of the same name win).
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub labels: BTreeMap<String, f64>,
}

/// A named floor area (D8 of the plan): a waiting area, break room or office zone. It is
/// not part of the flow; transporters use it as home or break place, and the 3D view
/// draws it on the floor.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct AreaDef {
    pub name: String,
    /// Centre of the area on the floor.
    #[serde(default)]
    pub position: [f64; 3],
    /// Length along x and width along y, in metres.
    #[serde(default = "default_area_size")]
    pub size: [f64; 2],
}

fn default_area_size() -> [f64; 2] {
    [2.0, 2.0]
}

/// Whether `s` is a "#rrggbb" colour.
fn is_hex_color(s: &str) -> bool {
    s.len() == 7 && s.starts_with('#') && s[1..].chars().all(|c| c.is_ascii_hexdigit())
}

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
    /// Position of a place given as a point or as an object or area name.
    pub(crate) fn place_of(&self, at: Option<[f64; 3]>, object: Option<&str>) -> Option<[f64; 3]> {
        at.or_else(|| {
            let name = object?;
            self.objects
                .iter()
                .find(|o| o.name == name)
                .map(|o| o.position)
                .or_else(|| {
                    self.areas
                        .iter()
                        .find(|a| a.name == name)
                        .map(|a| a.position)
                })
        })
    }

    /// Areas, and the home and break places of the transporters.
    pub(crate) fn validate_places(&self, errors: &mut Vec<String>) {
        let mut names: HashSet<&str> = self.objects.iter().map(|o| o.name.as_str()).collect();
        for a in &self.areas {
            if a.name.trim().is_empty() {
                errors.push("há uma área sem nome".to_string());
            } else if !names.insert(a.name.as_str()) {
                errors.push(format!(
                    "área '{}': o nome já é usado por um objeto ou outra área",
                    a.name
                ));
            }
            if !a.position.iter().all(|v| v.is_finite()) {
                errors.push(format!("área '{}': position deve ser finita", a.name));
            }
            if !a.size.iter().all(|v| v.is_finite() && *v > 0.0) {
                errors.push(format!("área '{}': size deve ser finito e > 0", a.name));
            }
        }
        for obj in &self.objects {
            let ObjectKind::Transporter {
                home_at,
                home_object,
                break_at,
                break_object,
                ..
            } = &obj.kind
            else {
                continue;
            };
            for (what, at, object) in [
                ("home", home_at, home_object),
                ("break", break_at, break_object),
            ] {
                if at.is_some() && object.is_some() {
                    errors.push(format!(
                        "{}: use {what}_at (um ponto) ou {what}_object (um objeto ou área), não os dois",
                        obj.name
                    ));
                }
                if at.is_some_and(|p| !p.iter().all(|v| v.is_finite())) {
                    errors.push(format!("{}: {what}_at deve ser finito", obj.name));
                }
                if let Some(o) = object {
                    if o == &obj.name {
                        errors.push(format!(
                            "{}: {what}_object não pode ser o próprio transportador",
                            obj.name
                        ));
                    } else if self.place_of(None, Some(o)).is_none() {
                        errors.push(format!(
                            "{}: {what}_object '{o}' não é um objeto nem uma área do modelo",
                            obj.name
                        ));
                    }
                }
            }
        }
    }

    /// The item types as the sources apply them.
    pub(crate) fn item_type_specs(&self) -> Vec<ItemTypeSpec> {
        self.item_types
            .iter()
            .map(|t| ItemTypeSpec {
                name: t.name.clone(),
                labels: t.labels.iter().map(|(k, v)| (k.clone(), *v)).collect(),
            })
            .collect()
    }

    /// `item_types` and every reference to them (sources, table rows).
    pub(crate) fn validate_item_types(&self, errors: &mut Vec<String>) {
        let mut names = HashSet::new();
        for (k, t) in self.item_types.iter().enumerate() {
            let n = k + 1;
            if t.name.trim().is_empty() {
                errors.push(format!("item_types[{n}]: o tipo precisa de um nome"));
            } else if !names.insert(t.name.as_str()) {
                errors.push(format!("item_types: tipo '{}' repetido", t.name));
            }
            if t.color.as_deref().is_some_and(|c| !is_hex_color(c)) {
                errors.push(format!(
                    "item_types '{}': color deve ser \"#rrggbb\"",
                    t.name
                ));
            }
            if t.labels.contains_key(ITEM_TYPE_LABEL) {
                errors.push(format!(
                    "item_types '{}': o rótulo '{ITEM_TYPE_LABEL}' é reservado ao tipo",
                    t.name
                ));
            }
            if t.labels.values().any(|v| !v.is_finite()) {
                errors.push(format!(
                    "item_types '{}': os rótulos devem ser finitos",
                    t.name
                ));
            }
        }
        let known = |name: &str| self.item_types.iter().any(|t| t.name == name);
        let unknown = |what: String, name: &str| {
            let list: Vec<&str> = self.item_types.iter().map(|t| t.name.as_str()).collect();
            format!(
                "{what}: o tipo '{name}' não existe ({})",
                if list.is_empty() {
                    "o modelo não tem item_types".to_string()
                } else {
                    format!("tipos: {}", list.join(", "))
                }
            )
        };
        for obj in &self.objects {
            let ObjectKind::Source {
                item_type,
                item_type_mix,
                table,
                labels,
                ..
            } = &obj.kind
            else {
                continue;
            };
            let name = &obj.name;
            if let Some(t) = item_type.as_deref().filter(|t| !known(t)) {
                errors.push(unknown(format!("{name}.item_type"), t));
            }
            for (t, w) in item_type_mix {
                if !known(t) {
                    errors.push(unknown(format!("{name}.item_type_mix"), t));
                }
                if !(w.is_finite() && *w >= 0.0) {
                    errors.push(format!(
                        "{name}.item_type_mix: o peso de '{t}' deve ser finito e >= 0"
                    ));
                }
            }
            if !item_type_mix.is_empty() && !item_type_mix.iter().any(|(_, w)| *w > 0.0) {
                errors.push(format!(
                    "{name}.item_type_mix: ao menos um peso precisa ser > 0"
                ));
            }
            if item_type.is_some() && !item_type_mix.is_empty() {
                errors.push(format!(
                    "{name}: use item_type (um tipo) ou item_type_mix (sorteio), não os dois"
                ));
            }
            let rows = table.as_ref().map_or(&[][..], |t| t.rows.as_slice());
            for (k, row) in rows.iter().enumerate() {
                if let Some(t) = row.item_type.as_deref().filter(|t| !known(t)) {
                    errors.push(unknown(
                        format!("{name}: table.rows[{}].item_type", k + 1),
                        t,
                    ));
                }
                if row.labels.contains_key(ITEM_TYPE_LABEL) {
                    errors.push(format!(
                        "{name}: table.rows[{}]: o rótulo '{ITEM_TYPE_LABEL}' é reservado; use item_type",
                        k + 1
                    ));
                }
            }
            let typed = item_type.is_some()
                || !item_type_mix.is_empty()
                || rows.iter().any(|r| r.item_type.is_some());
            if typed && labels.contains_key(ITEM_TYPE_LABEL) {
                errors.push(format!(
                    "{name}: o rótulo '{ITEM_TYPE_LABEL}' é reservado ao tipo do item; tire-o de labels"
                ));
            }
        }
    }

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
        if !self.item_types.is_empty() {
            let types: Vec<String> = self
                .item_types
                .iter()
                .enumerate()
                .map(|(k, t)| {
                    let look: Vec<&str> = [t.color.as_deref(), t.asset.as_deref()]
                        .into_iter()
                        .flatten()
                        .collect();
                    if look.is_empty() {
                        format!("{} = {}", t.name, k + 1)
                    } else {
                        format!("{} = {} ({})", t.name, k + 1, look.join(", "))
                    }
                })
                .collect();
            out.push(format!(
                "tipos de item (rótulo '{ITEM_TYPE_LABEL}'): {}",
                types.join("; ")
            ));
        }
        out
    }
}
