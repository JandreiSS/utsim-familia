//! Fluid networks in the 3D view (phase 7): layout, shapes and sizes of the nodes.
//!
//! Pure data in Bevy space (Y up), no Bevy types. The layout comes from the replay's
//! [`FluidDef`]; the levels over time come from [`crate::timeline::Timeline::fluid_at`].
//! Fluid nodes are not editable and are drawn in `Simular` only.

use utsim_fluid::{FluidDef, FluidKind};
use utsim_model::replay::ReplayBlock;

use crate::layout;

/// How a node is drawn.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NodeShape {
    /// Cylinder whose fill height shows the level.
    Tank,
    Source,
    Sink,
    Pipe,
    /// Converter between discrete items and fluid (either direction).
    Converter,
}

/// One node of the network, placed in the scene.
#[derive(Clone, Debug, PartialEq)]
pub struct FluidNodeView {
    pub name: String,
    pub shape: NodeShape,
    /// Bevy-space base position (center of the footprint on the ground).
    pub pos: [f32; 3],
    /// Tank capacity (0 for the other nodes).
    pub capacity: f64,
    /// Tank level at the start of the run.
    pub initial: f64,
    /// Human-readable kind (Portuguese).
    pub kind_label: &'static str,
}

/// A converter's dashed line to its discrete object.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ConverterLink {
    pub node: usize,
    /// Replay block index of the discrete object.
    pub block: usize,
    /// `true` for `FromItems` (items become fluid: the line points to the node).
    pub into_fluid: bool,
}

/// The layout of a fluid network, ready to draw.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct FluidView {
    /// Replay block index of the network (the `net` field of the trace events).
    pub net: Option<usize>,
    pub nodes: Vec<FluidNodeView>,
    /// `(from, to)` node indices.
    pub links: Vec<(usize, usize)>,
    pub converters: Vec<ConverterLink>,
}

/// Radius and height of a tank of the given capacity (grows with the cube root, so a
/// tank 8 times larger is only twice as tall).
pub fn tank_dims(capacity: f64) -> (f32, f32) {
    let c = (capacity.max(0.0) as f32).cbrt();
    (0.8 + 0.2 * c, 1.0 + 0.7 * c)
}

/// Height above the ground where the links meet the node.
pub fn port_height(node: &FluidNodeView) -> f32 {
    match node.shape {
        NodeShape::Tank => 0.45,
        _ => 0.35,
    }
}

/// Half extents of the node's bounding box (centered `half[1]` above the base).
pub fn node_half(node: &FluidNodeView) -> [f32; 3] {
    match node.shape {
        NodeShape::Tank => {
            let (r, h) = tank_dims(node.capacity);
            [r, h / 2.0, r]
        }
        _ => [0.35, 0.35, 0.35],
    }
}

/// Fraction `0..=1` of the tank that is filled.
pub fn fill_fraction(level: f64, capacity: f64) -> f32 {
    if capacity > 0.0 {
        (level / capacity).clamp(0.0, 1.0) as f32
    } else {
        0.0
    }
}

fn shape_of(kind: &FluidKind) -> (NodeShape, &'static str) {
    match kind {
        FluidKind::Source { .. } => (NodeShape::Source, "fonte"),
        FluidKind::Tank { .. } => (NodeShape::Tank, "tanque"),
        FluidKind::Pipe { .. } => (NodeShape::Pipe, "tubulação"),
        FluidKind::Sink { .. } => (NodeShape::Sink, "sumidouro"),
        FluidKind::FromItems { .. } => (NodeShape::Converter, "conversor de itens em fluido"),
        FluidKind::ToItems { .. } => (NodeShape::Converter, "conversor de fluido em itens"),
    }
}

impl FluidView {
    /// Lays out `def` (the network whose block is `net`). Positions written in the model
    /// are used as they are; when every node sits at the origin the nodes are arranged
    /// in layers along the links, in a strip below the discrete blocks (`block_pos` are
    /// the Bevy base positions of the blocks that are drawn).
    pub fn build(
        def: &FluidDef,
        net: usize,
        blocks: &[ReplayBlock],
        block_pos: &[[f32; 3]],
    ) -> Self {
        let index = |name: &str| def.nodes.iter().position(|n| n.name == name);
        let links: Vec<(usize, usize)> = def
            .links
            .iter()
            .filter_map(|(a, b)| Some((index(a)?, index(b)?)))
            .collect();
        let raw: Vec<[f64; 3]> = def.nodes.iter().map(|n| n.position).collect();
        let pos: Vec<[f32; 3]> = if layout::all_at_origin(&raw) {
            let (mut x0, mut z1) = (f32::MAX, f32::MIN);
            for p in block_pos {
                x0 = x0.min(p[0]);
                z1 = z1.max(p[2]);
            }
            if block_pos.is_empty() {
                (x0, z1) = (0.0, -9.0);
            }
            layout::auto_layout(def.nodes.len(), &links)
                .into_iter()
                .map(|p| [x0 + p[0] * 1.4, 0.0, z1 + 9.0 + p[2] * 1.6])
                .collect()
        } else {
            raw.iter().map(|p| layout::to_bevy(*p)).collect()
        };
        let nodes = def
            .nodes
            .iter()
            .zip(pos)
            .map(|(n, pos)| {
                let (shape, kind_label) = shape_of(&n.kind);
                let (capacity, initial) = match n.kind {
                    FluidKind::Tank {
                        capacity, initial, ..
                    } => (capacity, initial),
                    _ => (0.0, 0.0),
                };
                FluidNodeView {
                    name: n.name.clone(),
                    shape,
                    pos,
                    capacity,
                    initial,
                    kind_label,
                }
            })
            .collect();
        let converters = def
            .nodes
            .iter()
            .enumerate()
            .filter_map(|(node, n)| {
                let (object, into_fluid) = match &n.kind {
                    FluidKind::FromItems { object, .. } => (object, true),
                    FluidKind::ToItems { object, .. } => (object, false),
                    _ => return None,
                };
                let block = blocks
                    .iter()
                    .position(|b| b.kind != "Fluid" && b.kind != "Flow" && &b.name == object)?;
                Some(ConverterLink {
                    node,
                    block,
                    into_fluid,
                })
            })
            .collect();
        FluidView {
            net: Some(net),
            nodes,
            links,
            converters,
        }
    }

    pub fn is_empty(&self) -> bool {
        self.nodes.is_empty()
    }

    /// Bevy-space points that must stay in the frame (base and top of every node).
    pub fn bounds_points(&self) -> Vec<[f32; 3]> {
        let mut out = Vec::new();
        for n in &self.nodes {
            let half = node_half(n);
            out.push([n.pos[0] - half[0], 0.0, n.pos[2] - half[2]]);
            out.push([n.pos[0] + half[0], half[1] * 2.0, n.pos[2] + half[2]]);
        }
        out
    }
}

/// Percentage text of a tank level, e.g. `"25.0%"`.
pub fn percent_text(level: f64, capacity: f64) -> String {
    format!("{:.1}%", fill_fraction(level, capacity) * 100.0)
}

/// What a rate means for a tank, in Portuguese.
pub fn rate_label(rate: f64) -> &'static str {
    const EPS: f64 = 1e-9;
    if rate > EPS {
        "enchendo"
    } else if rate < -EPS {
        "esvaziando"
    } else {
        "parado"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use utsim_fluid::FluidNode;

    fn node(name: &str, kind: FluidKind, position: [f64; 3]) -> FluidNode {
        FluidNode {
            name: name.to_string(),
            kind,
            position,
        }
    }

    fn tank() -> FluidKind {
        FluidKind::Tank {
            capacity: 100.0,
            initial: 25.0,
            max_in: None,
            max_out: None,
        }
    }

    fn def(positions: [[f64; 3]; 2]) -> FluidDef {
        FluidDef {
            name: "f".into(),
            nodes: vec![
                node("a", FluidKind::Source { rate: 1.0 }, positions[0]),
                node("t", tank(), positions[1]),
            ],
            links: vec![("a".into(), "t".into())],
        }
    }

    #[test]
    fn fill_fraction_is_clamped() {
        assert_eq!(fill_fraction(25.0, 100.0), 0.25);
        assert_eq!(fill_fraction(-1.0, 100.0), 0.0);
        assert_eq!(fill_fraction(150.0, 100.0), 1.0);
        assert_eq!(fill_fraction(1.0, 0.0), 0.0);
        assert_eq!(percent_text(25.0, 100.0), "25.0%");
    }

    #[test]
    fn bigger_tanks_are_bigger_but_sublinearly() {
        let (r1, h1) = tank_dims(10.0);
        let (r2, h2) = tank_dims(80.0);
        assert!(r2 > r1 && h2 > h1);
        assert!(h2 < 8.0 * h1);
    }

    #[test]
    fn model_positions_are_kept() {
        let v = FluidView::build(&def([[1.0, 2.0, 0.0], [5.0, 2.0, 0.0]]), 3, &[], &[]);
        assert_eq!(v.net, Some(3));
        assert_eq!(v.nodes[0].pos, [1.0, 0.0, -2.0]);
        assert_eq!(v.nodes[1].pos, [5.0, 0.0, -2.0]);
        assert_eq!(v.links, vec![(0, 1)]);
        assert_eq!(v.nodes[1].capacity, 100.0);
        assert_eq!(v.nodes[1].initial, 25.0);
    }

    #[test]
    fn zero_positions_get_an_automatic_strip_below_the_blocks() {
        let blocks = [[0.0, 0.0, 0.0], [5.0, 0.0, 3.5]];
        let v = FluidView::build(&def([[0.0; 3]; 2]), 2, &[], &blocks);
        // Layered left to right, all on one lane, below the highest-Z block.
        assert!(v.nodes[0].pos[0] < v.nodes[1].pos[0]);
        assert_eq!(v.nodes[0].pos[2], v.nodes[1].pos[2]);
        assert!(v.nodes[0].pos[2] > 3.5);
    }

    #[test]
    fn converters_point_to_their_discrete_object() {
        let blocks = vec![
            ReplayBlock {
                name: "descarga".into(),
                kind: "Sink".into(),
                position: [0.0; 3],
            },
            ReplayBlock {
                name: "fluidos".into(),
                kind: "Fluid".into(),
                position: [0.0; 3],
            },
        ];
        let d = FluidDef {
            name: "fluidos".into(),
            nodes: vec![
                node(
                    "c",
                    FluidKind::FromItems {
                        object: "descarga".into(),
                        volume: 6.0,
                    },
                    [0.0; 3],
                ),
                node("t", tank(), [0.0; 3]),
            ],
            links: vec![("c".into(), "t".into())],
        };
        let v = FluidView::build(&d, 1, &blocks, &[[0.0; 3]]);
        assert_eq!(
            v.converters,
            vec![ConverterLink {
                node: 0,
                block: 0,
                into_fluid: true
            }]
        );
        assert_eq!(v.nodes[0].shape, NodeShape::Converter);
    }

    #[test]
    fn rate_labels() {
        assert_eq!(rate_label(0.5), "enchendo");
        assert_eq!(rate_label(-0.5), "esvaziando");
        assert_eq!(rate_label(0.0), "parado");
    }
}
