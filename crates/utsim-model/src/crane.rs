//! Overhead cranes (`ObjectKind::Crane`, ponte rolante) in the model: the kernel
//! geometry from the object's `size`, position and `visual.yaw`, the checks of the
//! fields and of the points the crane serves, and the model warnings (items heavier
//! than the capacity, cranes whose areas overlap, points of a dispatcher out of reach).
//! The kinematics are in `utsim_objects::crane`.

use std::collections::HashMap;

use utsim_core::BlockId;
use utsim_objects::{CraneLane, CraneOperator, CraneSpec};

use serde::{Deserialize, Serialize};

use crate::{Model, ObjectDef, ObjectKind};

/// Who runs a crane (`operator`, em português `operador`).
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum CraneControl {
    /// From the cabin: the operator is not modelled.
    #[default]
    #[serde(alias = "Cabine")]
    Cabin,
    /// Pendant control (botoeira): for each transport a person of `team` walks to the
    /// load, beside it, and with it to the drop; the crane travels no faster than the
    /// slowest walk of the team.
    #[serde(alias = "Botoeira")]
    Pendant {
        #[serde(alias = "equipe")]
        team: String,
    },
    /// Radio control: a person of `team` meets the crane at the load and runs it from
    /// there; the crane keeps its own speed.
    #[serde(alias = "Controle")]
    Radio {
        #[serde(alias = "equipe")]
        team: String,
    },
}

impl CraneControl {
    pub fn is_cabin(&self) -> bool {
        matches!(self, CraneControl::Cabin)
    }

    /// The team of the operator (`None`: cabin).
    pub fn team(&self) -> Option<&str> {
        match self {
            CraneControl::Cabin => None,
            CraneControl::Pendant { team } | CraneControl::Radio { team } => Some(team),
        }
    }
}

/// Distance (m) between the edge of the area under the load and the person at the
/// pendant or radio control.
pub const OPERATOR_CLEARANCE: f64 = 1.0;

/// The area under a crane's suspended load ([`Model::crane_load_zone`]): a rectangle
/// centred under the hook, of half sizes `half` (m, along the runway and across it),
/// turned by `yaw` (radians, counter-clockwise from the global `x`).
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct LoadZone {
    pub half: [f64; 2],
    pub yaw: f64,
}

impl LoadZone {
    /// Whether floor point `p` is inside the zone when the hook is above `hook`.
    pub fn contains(&self, hook: [f64; 2], p: [f64; 2]) -> bool {
        let (dx, dy) = (p[0] - hook[0], p[1] - hook[1]);
        let (s, c) = self.yaw.sin_cos();
        let u = c * dx + s * dy;
        let v = -s * dx + c * dy;
        u.abs() <= self.half[0] && v.abs() <= self.half[1]
    }
}

/// Corners of the covered rectangle, on the floor.
fn corners(c: &CraneSpec) -> [[f64; 2]; 4] {
    let (l, w) = (c.length / 2.0, c.span / 2.0);
    [(-l, -w), (l, -w), (l, w), (-l, w)].map(|(u, v)| {
        let g = c.global(u, v, 0.0);
        [g[0], g[1]]
    })
}

/// Whether two covered rectangles overlap (touching along an edge does not count):
/// separating axis test on the sides of both.
fn overlap(a: &CraneSpec, b: &CraneSpec) -> bool {
    let (ca, cb) = (corners(a), corners(b));
    for rect in [&ca, &cb] {
        for i in 0..2 {
            let (p, q) = (rect[i], rect[i + 1]);
            let axis = [q[1] - p[1], p[0] - q[0]];
            let project = |r: &[[f64; 2]; 4]| {
                r.iter()
                    .map(|c| c[0] * axis[0] + c[1] * axis[1])
                    .fold((f64::INFINITY, f64::NEG_INFINITY), |(lo, hi), x| {
                        (lo.min(x), hi.max(x))
                    })
            };
            let ((a0, a1), (b0, b1)) = (project(&ca), project(&cb));
            let tol = 1e-6 * (axis[0].abs() + axis[1].abs()).max(1.0);
            if a1 <= b0 + tol || b1 <= a0 + tol {
                return false;
            }
        }
    }
    true
}

/// Whether two runways are parallel (the same direction or opposite ones).
fn parallel(a: &CraneSpec, b: &CraneSpec) -> bool {
    (a.yaw - b.yaw).sin().abs() < 1e-6
}

impl Model {
    /// The runways shared by cranes (areas that overlap, runways parallel), by crane
    /// name: each group gets its own lane, the direction of its first crane and the
    /// largest `bridge_gap` of the group.
    pub fn crane_lanes(&self) -> HashMap<&str, CraneLane> {
        let cranes: Vec<(&ObjectDef, CraneSpec, f64)> = self
            .objects
            .iter()
            .filter_map(|o| {
                let ObjectKind::Crane { bridge_gap, .. } = &o.kind else {
                    return None;
                };
                Some((o, self.crane_spec(o)?, *bridge_gap))
            })
            .collect();
        // Groups by union of the pairs that share a runway.
        let mut group: Vec<usize> = (0..cranes.len()).collect();
        fn root(g: &mut [usize], mut i: usize) -> usize {
            while g[i] != i {
                g[i] = g[g[i]];
                i = g[i];
            }
            i
        }
        let mut shared = vec![false; cranes.len()];
        for i in 0..cranes.len() {
            for j in i + 1..cranes.len() {
                let (a, b) = (&cranes[i].1, &cranes[j].1);
                if parallel(a, b) && overlap(a, b) {
                    let (ri, rj) = (root(&mut group, i), root(&mut group, j));
                    group[ri.max(rj)] = ri.min(rj);
                    shared[i] = true;
                    shared[j] = true;
                }
            }
        }
        let mut lanes: Vec<(usize, CraneLane)> = Vec::new();
        let mut out = HashMap::new();
        for i in (0..cranes.len()).filter(|&i| shared[i]) {
            let r = root(&mut group, i);
            let lane = match lanes.iter_mut().find(|(g, _)| *g == r) {
                Some((_, lane)) => {
                    lane.gap = lane.gap.max(cranes[i].2);
                    *lane
                }
                None => {
                    let (s, c) = cranes[r].1.yaw.sin_cos();
                    let lane = CraneLane {
                        id: lanes.len() as u32,
                        dir: [c, s],
                        gap: cranes[i].2.max(cranes[r].2),
                    };
                    lanes.push((r, lane));
                    lane
                }
            };
            out.insert(cranes[i].0.name.as_str(), lane);
        }
        // The gap of the whole group.
        for lane in out.values_mut() {
            lane.gap = lanes[lane.id as usize].1.gap;
        }
        out
    }

    /// Kernel geometry of a crane (`None`: not a crane, or without `size`).
    pub(crate) fn crane_spec(&self, obj: &ObjectDef) -> Option<CraneSpec> {
        let ObjectKind::Crane {
            bridge,
            trolley,
            hoist,
            hook_height,
            capacity_t,
            motion,
            weight_label,
            ..
        } = &obj.kind
        else {
            return None;
        };
        let size = obj.size?;
        Some(CraneSpec {
            centre: obj.position,
            yaw: obj
                .visual
                .as_ref()
                .and_then(|v| v.yaw)
                .unwrap_or(0.0)
                .to_radians(),
            length: size[0],
            span: size[1],
            bridge: *bridge,
            trolley: *trolley,
            hoist: *hoist,
            hook_height: *hook_height,
            motion: *motion,
            capacity_kg: capacity_t * 1000.0,
            weight_label: weight_label.clone(),
        })
    }

    /// The slowest walking speed of `team` (a person, or a dispatcher of people); `None`
    /// when it is not one.
    fn team_speed(&self, team: &str) -> Option<f64> {
        let speed_of = |name: &str| {
            self.objects
                .iter()
                .find(|o| o.name == name)
                .and_then(|o| match &o.kind {
                    ObjectKind::Transporter { speed, .. } => Some(*speed),
                    _ => None,
                })
        };
        let obj = self.objects.iter().find(|o| o.name == team)?;
        match &obj.kind {
            ObjectKind::Transporter { speed, .. } => Some(*speed),
            ObjectKind::Dispatcher { team, .. } if !team.is_empty() => team
                .iter()
                .map(|m| speed_of(m))
                .collect::<Option<Vec<f64>>>()
                .map(|v| v.into_iter().fold(f64::INFINITY, f64::min)),
            _ => None,
        }
    }

    /// Where the person at a crane's pendant or radio stands, from the hook, on the
    /// floor: across the runway, 1 m outside the area under the load.
    fn operator_offset(&self, obj: &ObjectDef) -> [f64; 2] {
        let zone = self.crane_load_zone(obj).map_or(0.0, |z| z.half[1]);
        let d = zone + OPERATOR_CLEARANCE;
        let yaw = self.crane_spec(obj).map_or(0.0, |c| c.yaw);
        let (s, c) = yaw.sin_cos();
        // Minus the local `y` (the trolley's axis), turned with the runway.
        [s * d, -c * d]
    }

    /// The pendant or radio operator of a crane, for the kernel.
    pub(crate) fn crane_operator(
        &self,
        obj: &ObjectDef,
        ids: &HashMap<&str, BlockId>,
    ) -> Option<CraneOperator> {
        let ObjectKind::Crane { operator, .. } = &obj.kind else {
            return None;
        };
        let team = operator.team()?;
        Some(CraneOperator {
            team: *ids.get(team)?,
            follow: matches!(operator, CraneControl::Pendant { .. }),
            speed: self.team_speed(team).unwrap_or(f64::INFINITY),
            offset: self.operator_offset(obj),
        })
    }

    /// Points served through `transport: <name>`: each such object (pick-up) and the
    /// objects it connects to (drop).
    fn served_points(&self, name: &str) -> Vec<&ObjectDef> {
        let mut out: Vec<&ObjectDef> = Vec::new();
        for o in self
            .objects
            .iter()
            .filter(|o| o.transport.as_deref() == Some(name))
        {
            out.push(o);
            for (_, to) in self.connections.iter().filter(|(f, _)| *f == o.name) {
                if let Some(t) = self.objects.iter().find(|x| x.name == *to) {
                    if !out.iter().any(|x| x.name == t.name) {
                        out.push(t);
                    }
                }
            }
        }
        out
    }

    /// Errors of the cranes: fields, geometry and points served directly out of reach.
    pub(crate) fn validate_cranes(&self, errors: &mut Vec<String>) {
        for obj in &self.objects {
            let ObjectKind::Crane {
                bridge,
                trolley,
                hoist,
                hook_height,
                capacity_t,
                weight_label,
                load_zone,
                operator,
                bridge_gap,
                ..
            } = &obj.kind
            else {
                continue;
            };
            let name = &obj.name;
            if !(bridge_gap.is_finite() && *bridge_gap >= 0.0) {
                errors.push(format!(
                    "{name}: bridge_gap deve ser finita e >= 0 (distância, em metros, entre os \
                     ganchos de duas pontes no mesmo caminho de rolamento)"
                ));
            }
            if let Some(team) = operator.team() {
                if self.team_speed(team).is_none() {
                    errors.push(format!(
                        "{name}: operator.team '{team}' precisa ser uma pessoa (Transporter) ou \
                         um Dispatcher só de pessoas, que opera a botoeira ou o rádio"
                    ));
                }
            }
            if !(load_zone.is_finite() && *load_zone >= 0.0) {
                errors.push(format!(
                    "{name}: load_zone deve ser finita e >= 0 (metade do lado da área sob a carga, em metros)"
                ));
            }
            for (what, axis) in [("bridge", bridge), ("trolley", trolley), ("hoist", hoist)] {
                if let Err(e) = axis.validate(what) {
                    errors.push(format!("{name}: {e}"));
                }
            }
            if !(capacity_t.is_finite() && *capacity_t > 0.0) {
                errors.push(format!(
                    "{name}: capacity_t deve ser finita e > 0 (toneladas)"
                ));
            }
            if weight_label.trim().is_empty() {
                errors.push(format!("{name}: weight_label não pode ser vazio"));
            }
            let Some(size) = obj.size else {
                errors.push(format!(
                    "{name}: a ponte rolante precisa de size: (curso da ponte, vão, altura útil), em metros"
                ));
                continue;
            };
            if size.iter().any(|v| !(v.is_finite() && *v > 0.0)) {
                errors.push(format!(
                    "{name}: size deve ter as três medidas finitas e > 0"
                ));
                continue;
            }
            if !(hook_height.is_finite() && *hook_height > 0.0 && *hook_height <= size[2]) {
                errors.push(format!(
                    "{name}: hook_height deve ficar entre 0 e a altura útil ({} m)",
                    size[2]
                ));
            }
            let Some(spec) = self.crane_spec(obj) else {
                continue;
            };
            let out: Vec<&str> = self
                .served_points(name)
                .into_iter()
                .filter(|o| !spec.covers(o.position))
                .map(|o| o.name.as_str())
                .collect();
            if !out.is_empty() {
                errors.push(format!(
                    "{name}: fora da área da ponte ({} × {} m em volta de {:?}): {}",
                    size[0],
                    size[1],
                    obj.position,
                    out.join(", ")
                ));
            }
        }
    }

    /// One line per crane, for `utsim check`.
    /// The area under a crane's suspended load: a square centred under the hook, of half
    /// side `half` (m), turned by `yaw` (radians) with the runway. `None` for objects that
    /// are not cranes.
    pub fn crane_load_zone(&self, obj: &ObjectDef) -> Option<LoadZone> {
        let ObjectKind::Crane { load_zone, .. } = &obj.kind else {
            return None;
        };
        let yaw = obj.visual.as_ref().and_then(|v| v.yaw).unwrap_or(0.0);
        Some(LoadZone {
            half: [*load_zone, *load_zone],
            yaw: yaw.to_radians(),
        })
    }

    pub fn crane_lines(&self) -> Vec<String> {
        let lanes = self.crane_lanes();
        self.objects
            .iter()
            .filter_map(|o| {
                let c = self.crane_spec(o)?;
                let order = match c.motion {
                    utsim_objects::CraneMotion::Safe => {
                        "o gancho sobe e desce sozinho; ponte e carro juntos"
                    }
                    utsim_objects::CraneMotion::Sequential => "um eixo de cada vez",
                    utsim_objects::CraneMotion::Parallel => "os três eixos juntos",
                };
                let who = match &o.kind {
                    ObjectKind::Crane {
                        operator: CraneControl::Pendant { team },
                        ..
                    } => format!("; botoeira (equipe '{team}')"),
                    ObjectKind::Crane {
                        operator: CraneControl::Radio { team },
                        ..
                    } => format!("; rádio (equipe '{team}')"),
                    _ => String::new(),
                };
                let shared = lanes.get(o.name.as_str()).map_or(String::new(), |lane| {
                    let mut others: Vec<&str> = lanes
                        .iter()
                        .filter(|(n, l)| l.id == lane.id && **n != o.name.as_str())
                        .map(|(n, _)| *n)
                        .collect();
                    others.sort_unstable();
                    format!(
                        "; mesmo caminho que {} (anticolisão, {} m entre os ganchos)",
                        others.join(", "),
                        lane.gap
                    )
                });
                Some(format!(
                    "{}: área de {} × {} m (caminho × vão), gancho a {} m, {} t; ponte {} m/s, \
                     carro {} m/s, gancho {} m/s; {order}{who}{shared}",
                    o.name,
                    c.length,
                    c.span,
                    c.hook_height,
                    c.capacity_kg / 1000.0,
                    c.bridge.speed,
                    c.trolley.speed,
                    c.hoist.speed
                ))
            })
            .collect()
    }

    /// Warnings of the cranes.
    pub(crate) fn crane_warnings(&self, out: &mut Vec<String>) {
        let cranes: Vec<(&ObjectDef, CraneSpec)> = self
            .objects
            .iter()
            .filter_map(|o| self.crane_spec(o).map(|c| (o, c)))
            .collect();
        for (obj, spec) in &cranes {
            let heaviest = self.max_weight(&spec.weight_label);
            if heaviest > spec.capacity_kg {
                out.push(format!(
                    "{}: há peças de até {} kg (rótulo '{}'), acima da capacidade da ponte ({} t); \
                     ela recusa essas peças, que ficam paradas onde estão",
                    obj.name,
                    heaviest,
                    spec.weight_label,
                    spec.capacity_kg / 1000.0
                ));
            }
            // Served through a dispatcher of its team: points it cannot reach.
            for d in self.objects.iter().filter(|d| {
                matches!(&d.kind, ObjectKind::Dispatcher { team, .. } if team.contains(&obj.name))
            }) {
                let far: Vec<&str> = self
                    .served_points(&d.name)
                    .into_iter()
                    .filter(|o| !spec.covers(o.position))
                    .map(|o| o.name.as_str())
                    .collect();
                if !far.is_empty() {
                    out.push(format!(
                        "{}: a equipe '{}' pode lhe dar pedidos fora da sua área: {} (a ponte \
                         vai até a borda mais próxima)",
                        obj.name,
                        d.name,
                        far.join(", ")
                    ));
                }
            }
        }
        for (i, (a, sa)) in cranes.iter().enumerate() {
            for (b, sb) in &cranes[i + 1..] {
                if overlap(sa, sb) && !parallel(sa, sb) {
                    out.push(format!(
                        "{} e {}: as áreas das duas pontes se cruzam em ângulo; o UTSim só \
                         impede a colisão de pontes no mesmo caminho de rolamento (paralelas)",
                        a.name, b.name
                    ));
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use utsim_objects::{Axis, CraneMotion};

    fn spec(x: f64, yaw: f64) -> CraneSpec {
        let axis = Axis {
            speed: 1.0,
            acceleration: None,
            deceleration: None,
        };
        CraneSpec {
            centre: [x, 0.0, 0.0],
            yaw,
            length: 10.0,
            span: 4.0,
            bridge: axis,
            trolley: axis,
            hoist: axis,
            hook_height: 3.0,
            motion: CraneMotion::Safe,
            capacity_kg: 1000.0,
            weight_label: "weight".into(),
        }
    }

    #[test]
    fn overlap_of_turned_rectangles() {
        assert!(overlap(&spec(0.0, 0.0), &spec(9.0, 0.0)));
        // Side by side along the runway: they touch, no overlap.
        assert!(!overlap(&spec(0.0, 0.0), &spec(10.0, 0.0)));
        // Turned 90°, the second reaches only 2 m along x: from 5 to 9, clear of 0 ± 5.
        assert!(!overlap(
            &spec(0.0, 0.0),
            &spec(7.0, std::f64::consts::FRAC_PI_2)
        ));
        assert!(overlap(
            &spec(0.0, 0.0),
            &spec(6.0, std::f64::consts::FRAC_PI_2)
        ));
    }
}
