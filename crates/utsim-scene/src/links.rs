//! Team and request links of a model (`docs/plano_chegadas_e_equipes_3d.md`, E1a): the
//! lines an editor draws besides the item connections. They are derived from fields the
//! model already has (`team`, `transport`, `operators`, `downtime.repair_crew`, the
//! home and break places), so every model gets them without migration.

use serde::{Deserialize, Serialize};
use utsim_model::{Model, ObjectKind};

/// What a link line means.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum LinkKind {
    /// Dispatcher → member of its team.
    Team,
    /// Object → the transporter or dispatcher that carries its items out (`transport`).
    Transport,
    /// Processor → where its operators come from (`operators.team`).
    Operators,
    /// Object → its repair crew (`downtime.repair_crew`).
    Repair,
    /// Transporter → its home place (`home_at`, `home_object`).
    Home,
    /// Transporter → its break place (`break_at`, `break_object`).
    Break,
}

/// One line from block `from` to block `to` or, for places, to the point `to_point`.
/// Block indices are the model's object order (the scene's `EntityId::Block`).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct LinkLine {
    pub kind: LinkKind,
    pub from: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub to: Option<u32>,
    /// End point (metres, Z-up) of a home or break place; also set for block ends.
    pub to_point: [f32; 3],
}

/// Every team and request link of `model`, in object order (team lines in team order).
/// References to missing objects are skipped.
pub fn team_links(model: &Model) -> Vec<LinkLine> {
    let index = |name: &str| model.objects.iter().position(|o| o.name == name);
    let point = |p: [f64; 3]| p.map(|v| v as f32);
    let to_block = |kind: LinkKind, from: usize, name: &str| {
        index(name).map(|to| LinkLine {
            kind,
            from: from as u32,
            to: Some(to as u32),
            to_point: point(model.objects[to].position),
        })
    };
    let mut out = Vec::new();
    for (i, obj) in model.objects.iter().enumerate() {
        if let ObjectKind::Dispatcher { team, .. } = &obj.kind {
            out.extend(team.iter().filter_map(|m| to_block(LinkKind::Team, i, m)));
        }
        if let Some(t) = &obj.transport {
            out.extend(to_block(LinkKind::Transport, i, t));
        }
        if let ObjectKind::Processor {
            operators: Some(op),
            ..
        } = &obj.kind
        {
            out.extend(to_block(LinkKind::Operators, i, &op.team));
        }
        if let Some(crew) = obj.downtime.as_ref().and_then(|d| d.repair_crew.as_ref()) {
            out.extend(to_block(LinkKind::Repair, i, crew));
        }
        if let ObjectKind::Transporter {
            home_at,
            home_object,
            break_at,
            break_object,
            ..
        } = &obj.kind
        {
            for (kind, at, object) in [
                (LinkKind::Home, home_at, home_object),
                (LinkKind::Break, break_at, break_object),
            ] {
                let to = object.as_deref().and_then(index);
                if let Some(p) = model.place_of(*at, object.as_deref()) {
                    out.push(LinkLine {
                        kind,
                        from: i as u32,
                        to: to.map(|t| t as u32),
                        to_point: point(p),
                    });
                }
            }
        }
    }
    out
}
