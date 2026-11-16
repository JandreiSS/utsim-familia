//! Overhead cranes (`ObjectKind::Crane`, ponte rolante) in the model: the kernel
//! geometry from the object's `size`, position and `visual.yaw`, the checks of the
//! fields and of the points the crane serves, and the model warnings (items heavier
//! than the capacity, cranes whose areas overlap, points of a dispatcher out of reach).
//! The kinematics are in `utsim_objects::crane`.

use utsim_objects::CraneSpec;

use serde::{Deserialize, Serialize};

use crate::{Model, ObjectDef, ObjectKind};

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

impl Model {
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
                ..
            } = &obj.kind
            else {
                continue;
            };
            let name = &obj.name;
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
                Some(format!(
                    "{}: área de {} × {} m (caminho × vão), gancho a {} m, {} t; ponte {} m/s, \
                     carro {} m/s, gancho {} m/s; {order}",
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
                if overlap(sa, sb) {
                    out.push(format!(
                        "{} e {}: as áreas das duas pontes se sobrepõem; o UTSim ainda não \
                         impede que uma passe pela outra (sem anticolisão): use uma ponte por \
                         caminho de rolamento",
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
