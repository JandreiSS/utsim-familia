//! Composition of the scene: where every block, item, mobile resource and fluid node is,
//! and what it is doing, at any time `t`.
//!
//! [`Scene::build`] indexes a [`Replay`] once ([`Timeline`]), places the blocks
//! ([`layout`]), sizes them ([`geometry`]) and lays out the fluid network ([`fluids`]).
//! Those modules work in *layout space*, the Y-up frame of the Bevy viewer
//! (`layout::to_bevy`), and the viewer reads it directly through [`Scene::base_at`] and
//! [`Scene::items_at`]. The engine-independent output ([`Scene::static_scene`],
//! [`Scene::frame`]) converts every point to model coordinates (Z up) with
//! [`layout::to_model`], so both views come from the same numbers.
//!
//! # Derivation rules
//! - **Role** of a transporter ([`MobileRole`]), first match wins:
//!   1. `visual.asset` names the family: contains "empilhadeira" or "forklift" →
//!      `Forklift`; "agv" → `Agv`; "operador", "pessoa" or "person" → `Person`;
//!   2. `agv: true` → `Agv`;
//!   3. it serves a rack (an object whose `transport` is the transporter, or a
//!      dispatcher whose team includes it, is a `Rack` or is connected to one) →
//!      `Forklift`;
//!   4. it is an operator (its name, or a dispatcher whose team includes it, is the
//!      `operators.team` of a processor) → `Person`;
//!   5. otherwise `Vehicle` (also every transporter of a replay read without its model).
//! - **Position**: the end of the latest `Travel` leg, interpolated linearly in time
//!   along the path by arc length (the trace carries path and duration, not the speed
//!   profile); the layout position before the first leg.
//! - **Yaw**: the direction of the path piece being driven; between legs and while
//!   stopped, the heading at the end of the last leg with length; 0 before any travel.
//! - **Motion**, first match wins: `Stopped` in state `down`, `off_shift` or `stopped`;
//!   `Moving` while a leg with length is being driven; `Loading`/`Unloading` in those
//!   states; `Working` in state `utilize` (an operator at a station); `Blocked` in state
//!   `blocked`; otherwise `Idle`. Fixed blocks: `Working` while `processing` or `setup`,
//!   `Blocked`, `Stopped` as above, else `Idle`.
//! - **Speed** while `Moving`: the leg's length over its duration. When the model gives
//!   the transporter an acceleration or deceleration and the trapezoidal profile of the
//!   leg's own length lasts the leg's duration (within 0.5 %), the instantaneous speed of
//!   that profile instead. AGV pieces are slices of a whole-route profile, so they keep
//!   the mean speed of the piece.
//! - **Load and unload windows**: a `loading` state spans the load times of every item
//!   of the trip. Each pick-up (`Moved` into the carrier) closes one window and opens the
//!   next; `progress` runs 0 → 1 over the window containing `t`. Unloading is the same
//!   with the drop-offs (`Moved` out of the carrier).
//! - **Lift**: forklifts rest their forks at [`FORK_LOWERED`] empty and [`CARRY_HEIGHT`]
//!   loaded. Loading or unloading at a rack, the forks rise to the bottom of the item's
//!   cell, reach it in the middle of the window and return to the travel height. At any
//!   other block they move between the two rest heights. People hold carried items at
//!   [`HAND_HEIGHT`]; other vehicles report 0.
//! - **Parent**: an item resting in a block (queue, rack cell, belt, carrier) has that
//!   block as parent; an item packed in a container has the container; an item in
//!   transit between two blocks has none.
//! - **Fill**: a tank's level over its capacity, linear between the trace samples.

use std::collections::HashMap;
use std::f32::consts::PI;

use utsim_core::State;
use utsim_model::replay::Replay;
use utsim_model::{Model, ObjectKind, VisualDef};

use crate::fluids::{self, FluidView, NodeShape};
use crate::geometry::{self, Geom};
use crate::handling::{BeltParams, Handling, RackParams};
use crate::layout;
use crate::network::NetworkView;
use crate::style::{self, Shape};
use crate::timeline::{FluidSample, ItemPos, Timeline, TransitSetting};
use crate::{
    EntityId, EntityKind, Frame, Header, MobileRole, Motion, PROTOCOL, Pose, Segment, StaticEntity,
    StaticScene,
};

/// Edge of the default item cube, metres.
pub const ITEM_SIZE: f32 = 0.3;
/// Fork height of an empty forklift, metres.
pub const FORK_LOWERED: f32 = 0.1;
/// Fork height of a loaded forklift while driving, metres.
pub const CARRY_HEIGHT: f32 = 0.3;
/// Hand height of a person carrying an item, metres.
pub const HAND_HEIGHT: f32 = 1.0;
/// Height of the arc an item follows when it moves between two fixed blocks.
const TRANSIT_HEIGHT: f32 = 1.2;
const TRANSIT_ARC: f32 = 0.7;

/// What the scene is built from, besides the replay.
#[derive(Clone, Copy, Debug)]
pub struct SceneOptions<'a> {
    /// Length of the visual transit of an item between two fixed blocks.
    pub transit: TransitSetting,
    /// The model that produced the replay, when available: conveyor and rack sizes,
    /// the path network, `visual` and the transporter parameters come from it. Objects
    /// are matched by name. Without it (a `.json` replay) belts are 5 units long at
    /// speed 1 with items of length 1, racks are guessed from the highest slot used and
    /// every transporter is a [`MobileRole::Vehicle`].
    pub model: Option<&'a Model>,
}

impl Default for SceneOptions<'_> {
    fn default() -> Self {
        SceneOptions {
            transit: TransitSetting::Auto,
            model: None,
        }
    }
}

/// Blocks of a replay that are not objects of the 3D scene: token flows (drawn in the
/// flows panel) and fluid networks (their nodes are drawn instead).
pub fn hidden_kind(kind: &str) -> bool {
    kind == "Flow" || kind == "Fluid"
}

/// Speed profile of a transporter, from the model.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Profile {
    speed: f64,
    accel: f64,
    decel: f64,
}

impl Profile {
    /// Peak speed, time to reach it and time to stop from it, and total time for a
    /// trip of `length` (the same profile as the engine's).
    fn plan(&self, length: f64) -> (f64, f64, f64, f64) {
        let (a, d) = (self.accel, self.decel);
        let inv = 1.0 / a + 1.0 / d;
        let peak = if self.speed * self.speed / 2.0 * inv <= length {
            self.speed
        } else {
            (2.0 * length / inv).sqrt()
        };
        let (sa, sd) = (peak * peak / (2.0 * a), peak * peak / (2.0 * d));
        let (ta, td) = (peak / a, peak / d);
        (peak, ta, td, ta + (length - sa - sd).max(0.0) / peak + td)
    }

    /// Instantaneous speed `elapsed` into a leg of `length` lasting `duration`, when the
    /// profile explains that duration.
    fn speed_at(&self, length: f64, duration: f64, elapsed: f64) -> Option<f64> {
        if self.speed <= 0.0 || length <= 0.0 {
            return None;
        }
        let (peak, ta, td, total) = self.plan(length);
        if (total - duration).abs() > 0.005 * duration.max(1e-9) {
            return None;
        }
        Some(if elapsed < ta {
            self.accel * elapsed
        } else if elapsed > total - td {
            self.decel * (total - elapsed).max(0.0)
        } else {
            peak
        })
    }
}

/// Per replay block, what the scene knows besides its position and geometry.
#[derive(Clone, Debug, Default)]
struct BlockInfo {
    name: String,
    kind: String,
    visual: Option<VisualDef>,
    /// `Some` for transporters (the moving entities).
    role: Option<MobileRole>,
    profile: Option<Profile>,
    /// Heading held after each travel leg (latest leg with length).
    headings: Vec<Option<f32>>,
}

/// Where an item is drawn at one instant, in layout space.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ItemPlacement {
    /// Dense item index of the [`Timeline`].
    pub item: usize,
    /// Trace id of the item.
    pub id: u64,
    /// Center of the item cube (layout space, Y up).
    pub center: [f32; 3],
    /// Scale of the [`ITEM_SIZE`] cube (belt and rack items take their slot's size, a
    /// container holding items is drawn larger).
    pub scale: f32,
    /// Heading (riding a carrier: the carrier's; on a belt: the belt's).
    pub yaw: f32,
    pub parent: Option<EntityId>,
    /// `false` for items the viewer does not draw: packed in a container, or beyond the
    /// visible stack of a queue (they still exist and are in the frames).
    pub drawn: bool,
}

/// Pose of a mobile resource at one instant (position in layout space).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MobilePose {
    pub position: [f32; 3],
    pub yaw: f32,
    pub motion: Motion,
    pub lift: f32,
}

/// The composed scene of one replay. See the module docs for the rules.
#[derive(Clone, Debug)]
pub struct Scene {
    pub timeline: Timeline,
    /// Layout-space base position of each replay block (hidden blocks are parked on the
    /// first object).
    pub positions: Vec<[f32; 3]>,
    /// Footprint and orientation of each block (zero size for hidden blocks).
    pub geoms: Vec<Geom>,
    /// Layout of the model's fluid network (empty when it has none).
    pub fluid: FluidView,
    /// Path network of the model (empty without a model or a network).
    pub network: NetworkView,
    /// End of the animation, in simulation time.
    pub end: f64,
    model: String,
    blocks: Vec<BlockInfo>,
}

impl Scene {
    /// Indexes `replay` and places everything (model positions, or an automatic layout
    /// when every object sits at the origin).
    pub fn build(replay: &Replay, opts: &SceneOptions) -> Scene {
        let mut timeline = Timeline::build(replay, opts.transit);
        // Hidden blocks come after the objects; they are left out of the layout and
        // parked on the first object.
        let objects = replay
            .blocks
            .iter()
            .filter(|b| !hidden_kind(&b.kind))
            .count();
        let raw: Vec<[f64; 3]> = replay
            .blocks
            .iter()
            .filter(|b| !hidden_kind(&b.kind))
            .map(|b| b.position)
            .collect();
        let object_links: Vec<(usize, usize)> = replay
            .connections
            .iter()
            .copied()
            .filter(|&(a, b)| a < objects && b < objects)
            .collect();
        let mut positions = layout::place(&raw, &object_links);
        let anchor = positions.first().copied().unwrap_or_default();
        positions.resize(replay.blocks.len(), anchor);
        let end = if replay.stop_time.is_finite() {
            replay.stop_time
        } else {
            timeline.last_event_time()
        }
        .max(timeline.last_event_time());

        let object = |name: &str| {
            opts.model
                .and_then(|m| m.objects.iter().find(|o| o.name == name))
        };
        let handlings: Vec<Option<Handling>> = replay
            .blocks
            .iter()
            .enumerate()
            .map(|(b, block)| {
                let from_model = object(&block.name).and_then(|o| Handling::of(&o.kind));
                if let Some(Handling::Conveyor(p)) = from_model {
                    timeline.set_belt_params(b, p);
                }
                from_model.or(match block.kind.as_str() {
                    "Conveyor" => Some(Handling::Conveyor(BeltParams::default())),
                    "Rack" => Some(Handling::Rack(RackParams::guess(timeline.max_slot(b)))),
                    _ => None,
                })
            })
            .collect();
        let kinds: Vec<&str> = replay.blocks.iter().map(|b| b.kind.as_str()).collect();
        let mut geoms = geometry::build(&kinds, &handlings, &positions, &replay.connections);
        for (geom, kind) in geoms.iter_mut().zip(&kinds) {
            if hidden_kind(kind) {
                geom.half = [0.0; 3];
            }
        }

        let net = replay.blocks.iter().position(|b| b.kind == "Fluid");
        let fluid = match (&replay.fluids, net) {
            (Some(def), Some(net)) => {
                let drawn: Vec<[f32; 3]> = positions
                    .iter()
                    .zip(&replay.blocks)
                    .filter(|(_, b)| !hidden_kind(&b.kind))
                    .map(|(p, _)| *p)
                    .collect();
                FluidView::build(def, net, &replay.blocks, &drawn)
            }
            _ => FluidView::default(),
        };

        let blocks = replay
            .blocks
            .iter()
            .enumerate()
            .map(|(b, block)| {
                let obj = object(&block.name);
                let mobile = block.kind == "Transporter";
                let profile = obj.and_then(|o| match o.kind {
                    ObjectKind::Transporter {
                        speed,
                        acceleration,
                        deceleration,
                        ..
                    } if acceleration.is_some() || deceleration.is_some() => Some(Profile {
                        speed,
                        accel: acceleration.unwrap_or(f64::INFINITY),
                        decel: deceleration.unwrap_or(f64::INFINITY),
                    }),
                    _ => None,
                });
                let mut held = None;
                let headings = timeline
                    .legs(b)
                    .iter()
                    .map(|leg| {
                        held = leg.final_heading().map(|h| h as f32).or(held);
                        held
                    })
                    .collect();
                BlockInfo {
                    name: block.name.clone(),
                    kind: block.kind.clone(),
                    visual: obj.and_then(|o| o.visual.clone()),
                    role: mobile.then(|| role_of(&block.name, opts.model)),
                    profile,
                    headings,
                }
            })
            .collect();

        Scene {
            timeline,
            positions,
            geoms,
            fluid,
            network: opts.model.map(NetworkView::of).unwrap_or_default(),
            end,
            model: replay.model.clone(),
            blocks,
        }
    }

    /// Whether replay block `b` is not drawn as an object (token flow or fluid network).
    pub fn is_hidden(&self, b: usize) -> bool {
        self.blocks.get(b).is_some_and(|x| hidden_kind(&x.kind))
    }

    /// Replay block index of the token flow called `name`.
    pub fn flow_block(&self, name: &str) -> Option<usize> {
        self.blocks
            .iter()
            .position(|b| b.kind == "Flow" && b.name == name)
    }

    /// Role of replay block `b` when it is a transporter.
    pub fn role(&self, b: usize) -> Option<MobileRole> {
        self.blocks.get(b).and_then(|x| x.role)
    }

    /// Level and rate of fluid node `node` at `t`; before the first sample a tank is at
    /// its initial level and idle.
    pub fn fluid_sample(&self, node: usize, t: f64) -> FluidSample {
        let n = &self.fluid.nodes[node];
        let capacity = (n.capacity > 0.0).then_some(n.capacity);
        self.fluid
            .net
            .and_then(|net| self.timeline.fluid_at(net, node, t, capacity))
            .unwrap_or(FluidSample {
                level: n.initial,
                rate: 0.0,
            })
    }

    /// Layout-space base of block `b` at `t`: its layout position or, for a mobile
    /// resource that has started travelling, where the trace says it is.
    pub fn base_at(&self, b: usize, t: f64) -> [f32; 3] {
        self.timeline
            .carrier_pos(b, t)
            .map_or_else(|| self.positions[b], layout::to_bevy)
    }

    /// Heading of block `b` at `t` (see the module docs).
    fn yaw_at(&self, b: usize, t: f64) -> f32 {
        let legs = self.timeline.legs(b);
        let Some(k) = legs.partition_point(|l| l.t0 <= t).checked_sub(1) else {
            return self.geoms[b].yaw;
        };
        let leg = &legs[k];
        if leg.is_moving(t)
            && let Some(h) = leg.heading_at(t)
        {
            return h as f32;
        }
        self.blocks[b]
            .headings
            .get(k)
            .copied()
            .flatten()
            .unwrap_or(self.geoms[b].yaw)
    }

    /// Pose of mobile block `b` at `t` (`None` when `b` is not a transporter).
    pub fn mobile_at(&self, b: usize, t: f64) -> Option<MobilePose> {
        let info = self.blocks.get(b)?;
        let role = info.role?;
        let tl = &self.timeline;
        let content = tl.block_content(b, t);
        let span = tl.state_span(b, t);
        let state = span.map(|s| s.0);
        let legs = tl.legs(b);
        let leg = legs
            .partition_point(|l| l.t0 <= t)
            .checked_sub(1)
            .map(|k| &legs[k])
            .filter(|l| l.is_moving(t));

        let rest = match role {
            MobileRole::Forklift if content > 0 => CARRY_HEIGHT,
            MobileRole::Forklift => FORK_LOWERED,
            MobileRole::Person if content > 0 => HAND_HEIGHT,
            _ => 0.0,
        };
        let mut lift = rest;
        let motion = match (state, leg) {
            (Some(State::Down | State::OffShift | State::Stopped), _) => Motion::Stopped,
            (_, Some(leg)) => {
                let length = leg.length();
                let mean = length / leg.duration;
                let speed = info
                    .profile
                    .and_then(|p| p.speed_at(length, leg.duration, t - leg.t0))
                    .unwrap_or(mean);
                Motion::Moving {
                    speed: speed as f32,
                }
            }
            (Some(s @ (State::Loading | State::Unloading)), _) => {
                let (_, start, end) = span.unwrap_or((s, t, t));
                let pickup = s == State::Loading;
                let (progress, handoff) = self.window(b, t, start, end, pickup);
                if role == MobileRole::Forklift {
                    let (from, to) = if pickup {
                        (rest, CARRY_HEIGHT)
                    } else {
                        let after = if content > 1 {
                            CARRY_HEIGHT
                        } else {
                            FORK_LOWERED
                        };
                        (CARRY_HEIGHT, after)
                    };
                    lift = match handoff.and_then(|h| self.cell_height(h, pickup, t)) {
                        Some(cell) => {
                            let u = ((progress.min(1.0 - progress)) / 0.4).clamp(0.0, 1.0);
                            let base = if progress < 0.5 { from } else { to };
                            base + (cell - base) * u
                        }
                        None => from + (to - from) * progress,
                    };
                }
                if pickup {
                    Motion::Loading { progress }
                } else {
                    Motion::Unloading { progress }
                }
            }
            (Some(State::Utilize), _) => Motion::Working,
            (Some(State::Blocked), _) => Motion::Blocked,
            _ => Motion::Idle,
        };
        Some(MobilePose {
            position: self.base_at(b, t),
            yaw: self.yaw_at(b, t),
            motion,
            lift,
        })
    }

    /// Progress through the load (`pickup`) or unload window of carrier `b` that
    /// contains `t`, inside a state span `start..end`, and the hand-off closing it.
    fn window(
        &self,
        b: usize,
        t: f64,
        start: f64,
        end: f64,
        pickup: bool,
    ) -> (f32, Option<crate::timeline::Handoff>) {
        let list = self.timeline.handoffs(b);
        let mine = || list.iter().filter(|h| h.pickup == pickup);
        let next = mine().find(|h| h.t > t && h.t <= end).copied();
        let w0 = mine()
            .filter(|h| h.t <= t)
            .map(|h| h.t)
            .fold(start, f64::max);
        let w1 = next.map_or(end, |h| h.t);
        let progress = if w1.is_finite() && w1 > w0 {
            ((t - w0) / (w1 - w0)).clamp(0.0, 1.0) as f32
        } else {
            0.0
        };
        (progress, next)
    }

    /// Height of the bottom of the rack cell involved in hand-off `h`, when the other
    /// block is a rack.
    fn cell_height(&self, h: crate::timeline::Handoff, pickup: bool, t: f64) -> Option<f32> {
        let rack = h.other;
        if self.blocks.get(rack)?.kind != "Rack" {
            return None;
        }
        // Picking: the item is still in its cell. Dropping: its cell is known from the
        // `Slot` event of the drop-off.
        let at = if pickup { t } else { h.t };
        let slot = self.timeline.rack_slot(rack, h.item, at)?;
        let (center, size) = self.geoms[rack].rack_item(slot)?;
        Some(self.positions[rack][1] + center[1] - size / 2.0)
    }

    /// Where every live item is at `t` (layout space), sorted by item index.
    pub fn items_at(&self, t: f64) -> Vec<ItemPlacement> {
        let tl = &self.timeline;
        let n = self.positions.len();
        let mut live = Vec::new();
        tl.live_items(t, &mut live);
        let id = |i: usize| tl.item_id(i).unwrap_or(i as u64);

        // Resting items per block (ordered by arrival), transiting items, container fill.
        let mut resting: HashMap<usize, Vec<(f64, usize)>> = HashMap::new();
        let mut placed: Vec<ItemPlacement> = Vec::new();
        let mut fill: HashMap<usize, usize> = HashMap::new();
        let mut packed: Vec<(usize, usize)> = Vec::new();
        for &i in &live {
            let (pos, since) = tl.item_pos_since(i, t);
            match pos {
                ItemPos::At(b) if b < n => {
                    resting.entry(b).or_default().push((since, i));
                }
                ItemPos::Transit { from, to, progress } if from < n && to < n => {
                    let s = progress as f32;
                    let s = s * s * (3.0 - 2.0 * s);
                    let (a, b) = (self.base_at(from, t), self.base_at(to, t));
                    let arc = (PI * s).sin() * TRANSIT_ARC;
                    placed.push(ItemPlacement {
                        item: i,
                        id: id(i),
                        center: [
                            a[0] + (b[0] - a[0]) * s,
                            a[1] + TRANSIT_HEIGHT + (b[1] - a[1]) * s + arc,
                            a[2] + (b[2] - a[2]) * s,
                        ],
                        scale: 1.0,
                        yaw: 0.0,
                        parent: None,
                        drawn: true,
                    });
                }
                ItemPos::Inside(c) => {
                    *fill.entry(c).or_default() += 1;
                    packed.push((i, c));
                }
                _ => {}
            }
        }
        for (&b, list) in &mut resting {
            list.sort_by(|x, y| x.0.total_cmp(&y.0).then(x.1.cmp(&y.1)));
            let geom = &self.geoms[b];
            // Items inside a transporter ride on it (stacked on its body).
            let base = self.base_at(b, t);
            let parent = Some(EntityId::Block(b as u32));
            let yaw = if self.blocks[b].role.is_some() {
                self.yaw_at(b, t)
            } else if geom.shape == Shape::Belt {
                geom.yaw
            } else {
                0.0
            };
            let hidden = |i: usize| ItemPlacement {
                item: i,
                id: id(i),
                center: [base[0], base[1] + ITEM_SIZE / 2.0, base[2]],
                scale: 1.0,
                yaw,
                parent,
                drawn: false,
            };
            match geom.shape {
                // Belts and racks put every item where the trace says it is.
                Shape::Belt | Shape::Rack => {
                    for &(_, i) in list.iter() {
                        let spot = if geom.shape == Shape::Belt {
                            tl.belt_position(b, i, t)
                                .and_then(|distance| geom.belt_item(distance))
                        } else {
                            tl.rack_slot(b, i, t).and_then(|slot| geom.rack_item(slot))
                        };
                        placed.push(match spot {
                            Some((offset, size)) => ItemPlacement {
                                center: add(base, offset),
                                scale: size / ITEM_SIZE,
                                drawn: true,
                                ..hidden(i)
                            },
                            None => hidden(i),
                        });
                    }
                }
                shape => {
                    for (slot, &(_, i)) in list.iter().enumerate() {
                        // The stack is capped; the label shows how many are not drawn.
                        if slot >= style::visible_slots(shape) {
                            placed.push(hidden(i));
                            continue;
                        }
                        placed.push(ItemPlacement {
                            center: add(base, style::slot_offset(shape, slot)),
                            drawn: true,
                            ..hidden(i)
                        });
                    }
                }
            }
        }
        for entry in &mut placed {
            if let Some(&k) = fill.get(&entry.item) {
                // A container that holds items is drawn slightly larger.
                entry.scale = 1.0 + 0.18 * k.min(5) as f32 + 0.2;
            }
        }
        // Packed items ride inside their container, not drawn.
        let containers: HashMap<usize, ItemPlacement> =
            placed.iter().map(|p| (p.item, *p)).collect();
        for (i, c) in packed {
            if let Some(container) = containers.get(&c) {
                placed.push(ItemPlacement {
                    item: i,
                    id: id(i),
                    scale: 1.0,
                    parent: Some(EntityId::Item(container.id)),
                    drawn: false,
                    ..*container
                });
            }
        }
        placed.sort_by_key(|p| p.item);
        placed
    }

    /// The layout, in model coordinates (see the crate conventions).
    pub fn static_scene(&self) -> StaticScene {
        let mut entities = Vec::new();
        let (mut min, mut max) = ([f32::MAX; 3], [f32::MIN; 3]);
        let mut grow = |p: [f32; 3]| {
            for k in 0..3 {
                min[k] = min[k].min(p[k]);
                max[k] = max[k].max(p[k]);
            }
        };
        for (b, info) in self.blocks.iter().enumerate() {
            if hidden_kind(&info.kind) {
                continue;
            }
            let (pos, h) = (self.positions[b], self.geoms[b].half);
            grow(layout::to_model([pos[0] - h[0], pos[1], pos[2] - h[2]]));
            grow(layout::to_model([
                pos[0] + h[0],
                pos[1] + 2.0 * h[1],
                pos[2] + h[2],
            ]));
            let kind = match info.role {
                Some(role) => EntityKind::Mobile {
                    name: info.name.clone(),
                    kind: info.kind.clone(),
                    role,
                },
                None => EntityKind::Block {
                    name: info.name.clone(),
                    kind: info.kind.clone(),
                },
            };
            entities.push(StaticEntity {
                id: EntityId::Block(b as u32),
                kind,
                position: layout::to_model(pos),
                yaw: self.geoms[b].yaw,
                size: [2.0 * h[0], 2.0 * h[2], 2.0 * h[1]],
                visual: info.visual.clone(),
            });
        }
        for (i, node) in self.fluid.nodes.iter().enumerate() {
            let h = fluids::node_half(node);
            let pos = node.pos;
            grow(layout::to_model([pos[0] - h[0], pos[1], pos[2] - h[2]]));
            grow(layout::to_model([
                pos[0] + h[0],
                pos[1] + 2.0 * h[1],
                pos[2] + h[2],
            ]));
            entities.push(StaticEntity {
                id: EntityId::FluidNode(i as u32),
                kind: EntityKind::FluidNode {
                    name: node.name.clone(),
                    kind: node_kind(node.shape).to_string(),
                },
                position: layout::to_model(pos),
                yaw: 0.0,
                size: [2.0 * h[0], 2.0 * h[2], 2.0 * h[1]],
                visual: None,
            });
        }
        let network: Vec<Segment> = self
            .network
            .segments
            .iter()
            .map(|s| Segment {
                from: layout::to_model(s.a),
                to: layout::to_model(s.b),
                capacity: s.capacity,
            })
            .collect();
        for s in &network {
            grow(s.from);
            grow(s.to);
        }
        if entities.is_empty() && network.is_empty() {
            (min, max) = ([0.0; 3], [0.0; 3]);
        }
        StaticScene {
            entities,
            network,
            bounds: (min, max),
        }
    }

    /// First line of a scene stream.
    pub fn header(&self) -> Header {
        Header {
            protocol: PROTOCOL.to_string(),
            model: self.model.clone(),
            units: "m".to_string(),
            up: "z".to_string(),
            start: 0.0,
            end: self.end,
            scene: self.static_scene(),
        }
    }

    /// Everything that moves or changes at `t`, in model coordinates: blocks (in index
    /// order), fluid nodes, then the live items (by item index).
    pub fn frame(&self, t: f64) -> Frame {
        let mut poses = Vec::new();
        for (b, info) in self.blocks.iter().enumerate() {
            if hidden_kind(&info.kind) {
                continue;
            }
            let state = self.timeline.block_state(b, t);
            let pose = match self.mobile_at(b, t) {
                Some(m) => Pose {
                    id: EntityId::Block(b as u32),
                    position: layout::to_model(m.position),
                    yaw: m.yaw,
                    motion: m.motion,
                    lift: m.lift,
                    parent: None,
                    state: state.map(|s| s.name().to_string()),
                    fill: None,
                },
                None => Pose {
                    id: EntityId::Block(b as u32),
                    position: layout::to_model(self.base_at(b, t)),
                    yaw: self.geoms[b].yaw,
                    motion: block_motion(state),
                    lift: 0.0,
                    parent: None,
                    state: state.map(|s| s.name().to_string()),
                    fill: None,
                },
            };
            poses.push(pose);
        }
        for (i, node) in self.fluid.nodes.iter().enumerate() {
            let sample = self.fluid_sample(i, t);
            let tank = node.shape == NodeShape::Tank;
            let active = sample.rate.abs() > 1e-9;
            poses.push(Pose {
                id: EntityId::FluidNode(i as u32),
                position: layout::to_model(node.pos),
                yaw: 0.0,
                motion: if active {
                    Motion::Working
                } else {
                    Motion::Idle
                },
                lift: 0.0,
                parent: None,
                state: None,
                fill: tank.then(|| fluids::fill_fraction(sample.level, node.capacity)),
            });
        }
        for p in self.items_at(t) {
            let mut position = layout::to_model(p.center);
            position[2] -= p.scale * ITEM_SIZE / 2.0;
            poses.push(Pose {
                id: EntityId::Item(p.id),
                position,
                yaw: p.yaw,
                motion: Motion::Idle,
                lift: 0.0,
                parent: p.parent,
                state: None,
                fill: None,
            });
        }
        Frame { time: t, poses }
    }
}

fn add(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [a[0] + b[0], a[1] + b[1], a[2] + b[2]]
}

/// Motion of a fixed block from its state.
fn block_motion(state: Option<State>) -> Motion {
    match state {
        Some(State::Processing | State::Setup) => Motion::Working,
        Some(State::Blocked) => Motion::Blocked,
        Some(State::Down | State::OffShift | State::Stopped) => Motion::Stopped,
        _ => Motion::Idle,
    }
}

/// Kind name of a fluid node for [`EntityKind::FluidNode`].
fn node_kind(shape: NodeShape) -> &'static str {
    match shape {
        NodeShape::Tank => "Tank",
        NodeShape::Source => "Source",
        NodeShape::Sink => "Sink",
        NodeShape::Pipe => "Pipe",
        NodeShape::Converter => "Converter",
    }
}

/// Role of the transporter called `name` (see the module docs).
pub fn role_of(name: &str, model: Option<&Model>) -> MobileRole {
    let Some(model) = model else {
        return MobileRole::Vehicle;
    };
    let Some(me) = model.objects.iter().find(|o| o.name == name) else {
        return MobileRole::Vehicle;
    };
    let asset = me
        .visual
        .as_ref()
        .and_then(|v| v.asset.as_deref())
        .unwrap_or("")
        .to_lowercase();
    if asset.contains("empilhadeira") || asset.contains("forklift") {
        return MobileRole::Forklift;
    }
    if asset.contains("agv") {
        return MobileRole::Agv;
    }
    if ["operador", "pessoa", "person"]
        .iter()
        .any(|w| asset.contains(w))
    {
        return MobileRole::Person;
    }
    if matches!(me.kind, ObjectKind::Transporter { agv: true, .. }) {
        return MobileRole::Agv;
    }
    // The transporter itself and the dispatchers whose team includes it.
    let mut names = vec![name];
    for o in &model.objects {
        if let ObjectKind::Dispatcher { team, .. } = &o.kind
            && team.iter().any(|m| m == name)
        {
            names.push(&o.name);
        }
    }
    let is_rack = |n: &str| {
        model
            .objects
            .iter()
            .any(|o| o.name == n && matches!(o.kind, ObjectKind::Rack { .. }))
    };
    let serves_rack = model.objects.iter().any(|o| {
        o.transport.as_deref().is_some_and(|t| names.contains(&t))
            && (is_rack(&o.name)
                || model
                    .connections
                    .iter()
                    .any(|(a, b)| a == &o.name && is_rack(b)))
    });
    if serves_rack {
        return MobileRole::Forklift;
    }
    let operates = model.objects.iter().any(|o| match &o.kind {
        ObjectKind::Processor {
            operators: Some(op),
            ..
        } => names.contains(&op.team.as_str()),
        _ => false,
    });
    if operates {
        MobileRole::Person
    } else {
        MobileRole::Vehicle
    }
}
