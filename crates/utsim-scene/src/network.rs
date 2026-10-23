//! The model's path network in layout space (Y up), for drawing on the ground.

use utsim_model::Model;

use crate::layout;

/// One edge of the path network, in Bevy space.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct NetSeg {
    pub a: [f32; 3],
    pub b: [f32; 3],
    pub two_way: bool,
    /// Vehicles allowed on the edge at once (AGV traffic control); `None` = unlimited.
    pub capacity: Option<usize>,
}

/// The model's path network (nodes and edges) ready to draw on the ground.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct NetworkView {
    pub nodes: Vec<[f32; 3]>,
    pub segments: Vec<NetSeg>,
}

impl NetworkView {
    /// Edges naming a missing node are skipped.
    pub fn of(model: &Model) -> Self {
        let Some(net) = &model.network else {
            return Self::default();
        };
        let at = |name: &str| {
            net.nodes
                .iter()
                .find(|n| n.name == name)
                .map(|n| layout::to_bevy(n.position))
        };
        NetworkView {
            nodes: net
                .nodes
                .iter()
                .map(|n| layout::to_bevy(n.position))
                .collect(),
            segments: net
                .edges
                .iter()
                .filter_map(|e| {
                    Some(NetSeg {
                        a: at(&e.from)?,
                        b: at(&e.to)?,
                        two_way: e.two_way,
                        capacity: e.capacity,
                    })
                })
                .collect(),
        }
    }
}
