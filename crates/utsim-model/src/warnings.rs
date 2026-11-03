//! Non-fatal model checks: things that are valid but probably not what the author meant,
//! mostly about distances. `utsim check` and `utsim run` print them as "aviso: ...".

use std::collections::{BTreeSet, HashMap};

use utsim_core::task::distance;

use crate::{Model, ObjectKind, is_mobile};

/// A direct connection longer than this (metres) with no timing is reported.
pub const LONG_LINK: f64 = 10.0;

/// An object farther than this (metres) from every network node is reported.
pub const FAR_FROM_NETWORK: f64 = 50.0;

impl Model {
    /// Short, actionable warnings about a model that [`Model::validate`] accepts.
    /// They never stop a run. Meant to be called after `validate`.
    pub fn warnings(&self) -> Vec<String> {
        let pos: HashMap<&str, [f64; 3]> = self
            .objects
            .iter()
            .map(|o| (o.name.as_str(), o.position))
            .collect();
        let obj: HashMap<&str, &crate::ObjectDef> =
            self.objects.iter().map(|o| (o.name.as_str(), o)).collect();
        let all_at_origin = self.objects.iter().all(|o| o.position == [0.0; 3]);
        let mut out = Vec::new();

        // Objects served by each mobile resource (through `transport` or `operators`).
        let mut served: HashMap<&str, BTreeSet<&str>> = HashMap::new();
        for o in &self.objects {
            if let Some(t) = &o.transport {
                served
                    .entry(t.as_str())
                    .or_default()
                    .insert(o.name.as_str());
                for (_, to) in self.connections.iter().filter(|(f, _)| *f == o.name) {
                    served.entry(t.as_str()).or_default().insert(to.as_str());
                }
            }
            if let ObjectKind::Processor {
                operators: Some(op),
                ..
            } = &o.kind
            {
                served
                    .entry(op.team.as_str())
                    .or_default()
                    .insert(o.name.as_str());
            }
        }
        let uses_mobile = !served.is_empty()
            || self
                .objects
                .iter()
                .any(|o| matches!(o.kind, ObjectKind::Transporter { .. }));

        if uses_mobile && all_at_origin {
            out.push(
                "o modelo usa recursos móveis, mas todos os objetos estão na origem (0, 0, 0): \
                 as distâncias serão zero; defina `position` em cada objeto"
                    .to_string(),
            );
        } else {
            // A carrier going between two objects that share a position travels nothing.
            for o in &self.objects {
                let Some(t) = &o.transport else { continue };
                for (_, to) in self.connections.iter().filter(|(f, _)| *f == o.name) {
                    if pos.get(to.as_str()) == Some(&o.position) {
                        out.push(format!(
                            "'{t}' leva {} → {to}, mas os dois estão na mesma posição: a viagem dura zero; \
                             separe os objetos com `position`",
                            o.name
                        ));
                    }
                }
            }
            for (carrier, members) in &served {
                let Some(kind) = obj.get(carrier).map(|c| &c.kind) else {
                    continue;
                };
                if !is_mobile(kind) || members.len() < 2 {
                    continue;
                }
                let first = pos[members.iter().next().expect("two members")];
                if members.iter().all(|m| pos.get(m) == Some(&first)) {
                    out.push(format!(
                        "'{carrier}' atende {} na mesma posição: o deslocamento é zero; \
                         separe os objetos com `position`",
                        members.iter().copied().collect::<Vec<_>>().join(", ")
                    ));
                }
            }
        }

        if let Some(net) = &self.network {
            if !net.nodes.is_empty() {
                let mut far: BTreeSet<&str> = BTreeSet::new();
                for name in served.values().flatten() {
                    far.insert(name);
                }
                for name in far {
                    let p = pos[name];
                    let nearest = net
                        .nodes
                        .iter()
                        .map(|n| distance(p, n.position))
                        .fold(f64::INFINITY, f64::min);
                    if nearest > FAR_FROM_NETWORK {
                        out.push(format!(
                            "'{name}' está a {nearest:.0} m do nó mais próximo da rede: \
                             o trecho até a rede será longo; aproxime o objeto ou acrescente um nó"
                        ));
                    }
                }
            }
        }

        // Direct connections: instantaneous no matter how far apart the ends are.
        let is_belt = |name: &str| {
            obj.get(name)
                .is_some_and(|o| matches!(o.kind, ObjectKind::Conveyor { .. }))
        };
        for (from, to) in &self.connections {
            let Some(f) = obj.get(from.as_str()) else {
                continue;
            };
            if f.transport.is_some()
                || self.effective_transfer(f).is_some()
                || is_belt(from)
                || is_belt(to)
            {
                continue;
            }
            let Some(&p_to) = pos.get(to.as_str()) else {
                continue;
            };
            let d = distance(f.position, p_to);
            if d > LONG_LINK {
                out.push(format!(
                    "a ligação {from} → {to} tem {d:.0} m e é instantânea; \
                     use transporte, esteira ou `transfer`"
                ));
            }
        }
        out.extend(crate::dados::warnings(self));
        // Free and network movement: access points, paths, network edges over obstacles.
        if !(uses_mobile && all_at_origin) {
            self.movement_warnings(&mut out);
            self.aisle_warnings(&mut out);
        }
        out
    }
}
