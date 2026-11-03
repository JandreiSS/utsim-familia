//! Synthetic models of any size, for benchmarks, scale tests and `utsim gerar-modelo`.
//!
//! The three shapes are deterministic functions of `(shape, objects, items)`: the same
//! arguments always give the same model. Every shape is stable (utilisation about 0.7),
//! so queues stay short and the cost per item does not hide a growing backlog.

use std::collections::BTreeMap;

use utsim_core::{Dist, OutputRule};
use utsim_objects::{Discipline, OnBlocked, RackPlacement};

use crate::{Model, ObjectDef, ObjectKind, RunSettings};

/// Shape of a synthetic model.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Shape {
    /// One source, then `queue -> processor` pairs in series, then a sink.
    Line,
    /// One source dealing round-robin into parallel `queue -> processor` branches
    /// that share one sink.
    Network,
    /// One source, a queue, parallel pallet racks and a sink.
    Warehouse,
}

impl Shape {
    /// Parses the CLI spelling (`linha`, `rede`, `armazem`).
    pub fn parse(text: &str) -> Option<Shape> {
        match text.trim().to_lowercase().as_str() {
            "linha" => Some(Shape::Line),
            "rede" => Some(Shape::Network),
            "armazem" | "armazém" => Some(Shape::Warehouse),
            _ => None,
        }
    }

    /// Fewest objects the shape needs.
    pub fn min_objects(self) -> usize {
        4
    }
}

fn object(name: String, kind: ObjectKind, x: f64, y: f64) -> ObjectDef {
    ObjectDef {
        name,
        kind,
        output: OutputRule::FirstAvailable,
        position: [x, y, 0.0],
        downtime: None,
        shifts: None,
        transport: None,
        transport_priority: None,
        transport_priority_label: None,
        transfer: None,
        on_entry: None,
        visual: None,
        access: None,
        person: None,
    }
}

fn queue() -> ObjectKind {
    ObjectKind::Queue {
        capacity: None,
        discipline: Discipline::Fifo,
        batch: None,
    }
}

fn processor(mean: f64) -> ObjectKind {
    ObjectKind::Processor {
        capacity: 1,
        setup: None,
        process: Dist::Exp { mean },
        operators: None,
    }
}

fn source(mean: f64, items: u64) -> ObjectKind {
    ObjectKind::Source {
        interarrival: Dist::Exp { mean },
        first_at_zero: false,
        max_arrivals: Some(items),
        labels: BTreeMap::new(),
        on_blocked: OnBlocked::default(),
        schedule: None,
    }
}

/// Builds a model with exactly `objects` objects that creates exactly `items` items.
/// `stop_time` is generous: the run ends when the last item leaves.
pub fn generate(shape: Shape, objects: usize, items: u64) -> Result<Model, String> {
    if objects < shape.min_objects() {
        return Err(format!(
            "são necessários ao menos {} objetos para este tipo (recebido {objects})",
            shape.min_objects()
        ));
    }
    if items == 0 {
        return Err("a quantidade de itens deve ser >= 1".into());
    }
    let mut objs: Vec<ObjectDef> = Vec::with_capacity(objects);
    let mut connections: Vec<(String, String)> = Vec::new();
    // Mean time between arrivals; the offered load per server is 0.7 in every shape.
    let interarrival;
    match shape {
        Shape::Line => {
            interarrival = 1.0;
            objs.push(object("entrada".into(), source(1.0, items), 0.0, 0.0));
            // Middle objects alternate queue / processor; an odd count ends with a queue.
            let middle = objects - 2;
            for k in 0..middle {
                let (name, kind) = if k % 2 == 0 {
                    (format!("fila{}", k / 2 + 1), queue())
                } else {
                    (format!("maq{}", k / 2 + 1), processor(0.7))
                };
                objs.push(object(name, kind, 3.0 * (k + 1) as f64, 0.0));
            }
            objs.push(object(
                "saida".into(),
                ObjectKind::Sink,
                3.0 * (middle + 1) as f64,
                0.0,
            ));
            for pair in objs.windows(2) {
                connections.push((pair[0].name.clone(), pair[1].name.clone()));
            }
        }
        Shape::Network => {
            // `branches` queue/processor pairs; leftover objects become extra buffers.
            let branches = (objects - 2) / 2;
            interarrival = 1.0;
            let mut entry = object("entrada".into(), source(1.0, items), 0.0, 0.0);
            entry.output = OutputRule::RoundRobin;
            objs.push(entry);
            for b in 0..branches {
                let y = 3.0 * b as f64;
                objs.push(object(format!("fila{}", b + 1), queue(), 4.0, y));
                // Each branch sees 1 / branches of the arrivals.
                objs.push(object(
                    format!("maq{}", b + 1),
                    processor(0.7 * branches as f64),
                    8.0,
                    y,
                ));
            }
            let extra = objects - 2 - 2 * branches;
            for k in 0..extra {
                objs.push(object(format!("extra{}", k + 1), queue(), 12.0, 0.0));
            }
            objs.push(object("saida".into(), ObjectKind::Sink, 16.0, 0.0));
            let after_branch = if extra > 0 { "extra1" } else { "saida" };
            for b in 0..branches {
                connections.push(("entrada".into(), format!("fila{}", b + 1)));
                connections.push((format!("fila{}", b + 1), format!("maq{}", b + 1)));
                connections.push((format!("maq{}", b + 1), after_branch.to_string()));
            }
            for k in 0..extra {
                let to = if k + 1 < extra {
                    format!("extra{}", k + 2)
                } else {
                    "saida".to_string()
                };
                connections.push((format!("extra{}", k + 1), to));
            }
        }
        Shape::Warehouse => {
            let racks = objects - 3;
            // 24 positions per rack and a mean stay of 40: a load of 0.7 needs
            // 0.7 * 24 * racks / 40 arrivals per time unit.
            interarrival = 40.0 / (0.7 * 24.0 * racks as f64);
            objs.push(object(
                "recepcao".into(),
                source(interarrival, items),
                0.0,
                0.0,
            ));
            objs.push(object("fila".into(), queue(), 4.0, 0.0));
            for r in 0..racks {
                objs.push(object(
                    format!("rack{}", r + 1),
                    ObjectKind::Rack {
                        bays: 6,
                        levels: 4,
                        dwell: Some(Dist::Uniform {
                            min: 20.0,
                            max: 60.0,
                        }),
                        placement: RackPlacement::FirstFree,
                        bay_width: 1.2,
                        level_height: 1.5,
                    },
                    10.0,
                    4.0 * r as f64,
                ));
            }
            objs.push(object("expedicao".into(), ObjectKind::Sink, 18.0, 0.0));
            connections.push(("recepcao".into(), "fila".into()));
            for r in 0..racks {
                connections.push(("fila".into(), format!("rack{}", r + 1)));
                connections.push((format!("rack{}", r + 1), "expedicao".into()));
            }
        }
    }
    debug_assert_eq!(objs.len(), objects);
    // Enough time for every arrival plus a long drain; the run ends earlier anyway.
    let stop_time = items as f64 * interarrival * 3.0 + 1000.0;
    Ok(Model {
        format: Some(crate::FORMAT_VERSION),
        name: format!("Sintético ({shape:?}, {objects} objetos, {items} itens)"),
        description: "Modelo gerado por `utsim gerar-modelo` para testes de desempenho.".into(),
        run: RunSettings {
            stop_time,
            warmup: 0.0,
            seed: 1,
            replications: 1,
            sample_interval: None,
            histogram_bins: None,
            default_decision: None,
            transfer: None,
            movement: None,
        },
        calendars: BTreeMap::new(),
        objects: objs,
        connections,
        network: None,
        flows: Vec::new(),
        fluids: None,
        spans: Vec::new(),
        data: Vec::new(),
        floor_plan: None,
        obstacles: Vec::new(),
        aisles: Vec::new(),
        floor_marking: None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_shape_is_valid_and_exact() {
        for shape in [Shape::Line, Shape::Network, Shape::Warehouse] {
            for n in [4, 5, 10, 33] {
                let m = generate(shape, n, 200).unwrap();
                assert_eq!(m.objects.len(), n, "{shape:?} {n}");
                m.validate()
                    .unwrap_or_else(|e| panic!("{shape:?} {n}: {e}"));
                let r = m.run_replications().unwrap();
                assert_eq!(r[0].blocks[0].output, 200, "{shape:?} {n}: items created");
                let sink = r[0].blocks.last().unwrap();
                assert_eq!(sink.input, 200, "{shape:?} {n}: items delivered");
            }
        }
    }

    #[test]
    fn rejects_tiny_and_empty() {
        assert!(generate(Shape::Line, 2, 10).is_err());
        assert!(generate(Shape::Line, 10, 0).is_err());
    }

    #[test]
    fn round_trips_through_ron() {
        let m = generate(Shape::Network, 12, 50).unwrap();
        let text = ron::ser::to_string_pretty(&m, ron::ser::PrettyConfig::new()).unwrap();
        assert_eq!(Model::parse(&text).unwrap(), m);
    }
}
