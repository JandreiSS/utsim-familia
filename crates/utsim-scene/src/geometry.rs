//! Footprint and orientation of every block, including the ones whose size comes from
//! the model (conveyors and racks). Pure data in Bevy space (Y up), no Bevy types.

use crate::handling::{
    BEAM, BELT_THICKNESS, BELT_WIDTH, BeltParams, Handling, RACK_DEPTH, RackParams,
};
use crate::style::{self, Shape};

/// Shape, half extents (of the axis-aligned box that contains the block) and
/// orientation of one block.
#[derive(Clone, Debug, PartialEq)]
pub struct Geom {
    pub shape: Shape,
    /// Half extents of the bounding box, centered `half[1]` above the block base.
    pub half: [f32; 3],
    /// Rotation about the vertical axis (belts follow their outgoing connection).
    pub yaw: f32,
    /// Sanitized parameters of a conveyor or rack.
    pub handling: Option<Handling>,
}

impl Geom {
    /// `dir` is the horizontal direction `(dx, dz)` the block points to, if any.
    pub fn new(kind: &str, handling: Option<Handling>, dir: Option<[f32; 2]>) -> Self {
        let shape = style::shape_of(kind);
        match (shape, handling) {
            (Shape::Belt, Some(Handling::Conveyor(p))) => {
                Self::belt(p.sanitized(), dir.unwrap_or([1.0, 0.0]))
            }
            (Shape::Belt, _) => {
                Self::belt(BeltParams::default().sanitized(), dir.unwrap_or([1.0, 0.0]))
            }
            (Shape::Rack, Some(Handling::Rack(p))) => Self::rack(p.sanitized()),
            (Shape::Rack, _) => Self::rack(RackParams::guess(None).sanitized()),
            (shape, _) => Geom {
                shape,
                half: style::extents(shape).0,
                yaw: 0.0,
                handling: None,
            },
        }
    }

    fn belt(p: BeltParams, dir: [f32; 2]) -> Self {
        // Rotation about Y that takes local +X to the horizontal direction (dx, dz).
        let yaw = if dir[0].abs() + dir[1].abs() < 1e-6 {
            0.0
        } else {
            (-dir[1]).atan2(dir[0])
        };
        let (c, s) = (yaw.cos().abs(), yaw.sin().abs());
        let (hl, hw) = (p.length as f32 / 2.0, BELT_WIDTH / 2.0);
        Geom {
            shape: Shape::Belt,
            half: [c * hl + s * hw, BELT_THICKNESS / 2.0, s * hl + c * hw],
            yaw,
            handling: Some(Handling::Conveyor(p)),
        }
    }

    fn rack(p: RackParams) -> Self {
        Geom {
            shape: Shape::Rack,
            half: [
                p.total_width() / 2.0,
                p.total_height() / 2.0,
                RACK_DEPTH / 2.0,
            ],
            yaw: 0.0,
            handling: Some(Handling::Rack(p)),
        }
    }

    /// Half extents in the block's own frame, before its rotation: a belt's length along
    /// its local `x` and its width along `z` ([`Geom::half`] holds the box of the turned
    /// belt); the same as `half` for the other shapes, which `half` keeps unrotated.
    pub fn local_half(&self) -> [f32; 3] {
        match self.belt_params() {
            Some(p) if self.shape == Shape::Belt => [
                p.length as f32 / 2.0,
                BELT_THICKNESS / 2.0,
                BELT_WIDTH / 2.0,
            ],
            _ => self.half,
        }
    }

    pub fn belt_params(&self) -> Option<BeltParams> {
        match self.handling {
            Some(Handling::Conveyor(p)) => Some(p),
            _ => None,
        }
    }

    pub fn rack_params(&self) -> Option<RackParams> {
        match self.handling {
            Some(Handling::Rack(p)) => Some(p),
            _ => None,
        }
    }

    /// Height above the block base where its in-scene label should float.
    pub fn label_height(&self, drawn: usize) -> f32 {
        match self.shape {
            Shape::Belt => BELT_THICKNESS + 0.9,
            Shape::Rack => self.half[1] * 2.0 + 0.3,
            shape => style::top_of_stack(shape, drawn),
        }
    }

    /// Items that are resting but not drawn (belts and racks draw every item).
    pub fn hidden_items(&self, resting: usize) -> usize {
        match self.shape {
            Shape::Belt | Shape::Rack => 0,
            shape => style::hidden_items(shape, resting),
        }
    }

    /// Center of an item `distance` from the start of the belt (its front edge is at
    /// `distance`, so it is fully on the belt once it has moved one `item_length`),
    /// relative to the block base, and the edge of its cube.
    pub fn belt_item(&self, distance: f64) -> Option<([f32; 3], f32)> {
        let p = self.belt_params()?;
        let size = p.item_size();
        let along = (distance - p.item_length / 2.0 - p.length / 2.0) as f32;
        let (s, c) = self.yaw.sin_cos();
        Some((
            [along * c, BELT_THICKNESS + size / 2.0 + 0.01, -along * s],
            size,
        ))
    }

    /// Center of the item stored in `slot`, relative to the block base, and its cube edge.
    pub fn rack_item(&self, slot: usize) -> Option<([f32; 3], f32)> {
        let p = self.rack_params()?;
        let (x, y) = p.slot_center(slot);
        Some(([x, y, 0.0], p.item_size()))
    }
}

/// Geometry of every block. Belts point to the block their first outgoing connection
/// reaches; `positions` are Bevy-space base positions.
pub fn build(
    kinds: &[&str],
    handlings: &[Option<Handling>],
    positions: &[[f32; 3]],
    connections: &[(usize, usize)],
) -> Vec<Geom> {
    kinds
        .iter()
        .enumerate()
        .map(|(i, kind)| {
            let dir = connections
                .iter()
                .find(|&&(a, b)| a == i && b < positions.len() && i < positions.len())
                .map(|&(_, b)| {
                    [
                        positions[b][0] - positions[i][0],
                        positions[b][2] - positions[i][2],
                    ]
                });
            Geom::new(kind, handlings.get(i).copied().flatten(), dir)
        })
        .collect()
}

/// Local layout of a rack, for drawing: centers of the uprights `(x, z)` and the
/// heights of the beam centers.
pub fn rack_frame(p: &RackParams) -> (Vec<(f32, f32)>, Vec<f32>) {
    let half_w = p.total_width() / 2.0;
    let mut uprights = Vec::new();
    for k in 0..=p.bays {
        let x = (k as f64 * p.bay_width) as f32 - half_w;
        uprights.push((x, -RACK_DEPTH / 2.0 + BEAM / 2.0));
        uprights.push((x, RACK_DEPTH / 2.0 - BEAM / 2.0));
    }
    let beams = (0..=p.levels)
        .map(|k| (k as f64 * p.level_height) as f32 + BEAM / 2.0)
        .collect();
    (uprights, beams)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn belt(length: f64) -> Option<Handling> {
        Some(Handling::Conveyor(BeltParams {
            length,
            speed: 1.0,
            item_length: 1.0,
            accumulating: true,
        }))
    }

    #[test]
    fn a_belt_follows_its_outgoing_connection() {
        // Block 0 at the origin, block 1 straight to +X, block 2 to -Z (model +y).
        let pos = [[0.0; 3], [10.0, 0.0, 0.0], [0.0, 0.0, -10.0]];
        let kinds = ["Conveyor", "Sink", "Sink"];
        let g = build(&kinds, &[belt(6.0), None, None], &pos, &[(0, 1)]);
        assert!(g[0].yaw.abs() < 1e-6);
        assert!((g[0].half[0] - 3.0).abs() < 1e-5 && g[0].half[2] < 1.0);
        let g = build(&kinds, &[belt(6.0), None, None], &pos, &[(0, 2)]);
        // The belt runs along Z now: the box is long on Z.
        assert!((g[0].half[2] - 3.0).abs() < 1e-4 && g[0].half[0] < 1.0);
        // The front end of the belt (distance L) lies toward the target.
        let (end, _) = g[0].belt_item(6.0 + 0.5).unwrap();
        assert!(end[2] < -2.9 && end[0].abs() < 1e-4, "{end:?}");
        // No connection: along +X.
        let g = build(&["Conveyor"], &[belt(6.0)], &[[0.0; 3]], &[]);
        assert_eq!(g[0].yaw, 0.0);
    }

    #[test]
    fn belt_items_ride_between_the_two_ends() {
        let g = Geom::new("Conveyor", belt(10.0), None);
        let (start, size) = g.belt_item(1.0).unwrap();
        let (end, _) = g.belt_item(10.0).unwrap();
        // Front edge at the distance: the item center is half an item length behind it.
        assert!((start[0] + 4.5).abs() < 1e-5 && (end[0] - 4.5).abs() < 1e-5);
        assert!(start[1] > BELT_THICKNESS && size > 0.5);
        // Neighbours one item length apart are one item length apart in the scene.
        let (b, _) = g.belt_item(8.0).unwrap();
        assert!((end[0] - b[0] - 2.0).abs() < 1e-5);
    }

    #[test]
    fn rack_slots_sit_inside_the_frame() {
        let rack = Some(Handling::Rack(RackParams {
            bays: 6,
            levels: 4,
            bay_width: 1.2,
            level_height: 1.5,
        }));
        let g = Geom::new("Rack", rack, None);
        assert!((g.half[0] - 3.6).abs() < 1e-5);
        for slot in 0..24 {
            let (c, size) = g.rack_item(slot).unwrap();
            assert!(c[0].abs() + size / 2.0 <= g.half[0] + 1e-5, "slot {slot}");
            assert!(c[1] + size / 2.0 <= 2.0 * g.half[1] + 1e-5, "slot {slot}");
        }
        // Level k+1 is one level height above level k, bay b+1 one bay width beside bay b.
        let (a, _) = g.rack_item(0).unwrap();
        let (up, _) = g.rack_item(6).unwrap();
        let (next, _) = g.rack_item(1).unwrap();
        assert!((up[1] - a[1] - 1.5).abs() < 1e-5 && (next[0] - a[0] - 1.2).abs() < 1e-5);
        let (uprights, beams) = rack_frame(&g.rack_params().unwrap());
        assert_eq!((uprights.len(), beams.len()), (14, 5));
    }

    #[test]
    fn other_blocks_keep_their_fixed_footprint() {
        let g = Geom::new("Queue", None, Some([1.0, 0.0]));
        assert_eq!(g.half, style::extents(Shape::LongBox).0);
        assert_eq!(g.hidden_items(80), 20);
        assert_eq!(Geom::new("Conveyor", belt(5.0), None).hidden_items(500), 0);
        // A conveyor or a rack without model parameters still gets a drawable size.
        assert!(Geom::new("Conveyor", None, None).half[0] > 1.0);
        assert!(Geom::new("Rack", None, None).half[1] > 0.5);
    }
}
