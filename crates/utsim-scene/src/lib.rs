//! Engine-independent 3D scene of a UTSim run (contract).
//!
//! The simulation produces a trace; this crate turns a [`Replay`] into *what a
//! renderer must draw at time `t`*: the static layout once ([`StaticScene`]) and one
//! [`Frame`] per instant, with the pose of every block, item and mobile resource and a
//! semantic [`Motion`] (walking, lifting forks, loading...) that each engine maps to its
//! own animation clips. The Bevy viewer, the browser page, a future VR mode and external
//! engines (Unreal, Unity, Godot) all consume the same frames, so they never disagree on
//! where things are.
//!
//! # Conventions (fixed by protocol version [`PROTOCOL`])
//! - **Units:** metres, seconds of simulated time (1 model time unit = 1 s for speeds).
//! - **Axes:** the model's own plan coordinates. Right-handed, `x` right, `y` along
//!   the floor, **`z` up**; the floor is `z = 0`. Bevy (Y-up) maps `(x, y, z)` to
//!   `(x, z, -y)`; Unreal (Z-up, left-handed, cm) maps it to `(100x, -100y, 100z)`.
//! - **Yaw:** radians, counter-clockwise seen from above, `0` = facing `+x`.
//! - **Human scale:** assets are authored at real size, so a person is ~1.75 m tall and a
//!   VR camera at eye height (~1.6 m) sees the plant at the right scale.
//!
//! # Stream (`utsim cena`, and later a live socket)
//! JSON lines: the first line is a [`Header`] (protocol, conventions, static scene), every
//! following line is a [`Frame`]. A client needs nothing else to animate a run.

use serde::{Deserialize, Serialize};

pub use utsim_model::replay::Replay;

/// Stream/protocol identifier. Bump on any incompatible change to the types below.
pub const PROTOCOL: &str = "utsim-scene/1";

/// Stable id of an entity across frames (blocks keep their index; items their id).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub enum EntityId {
    Block(u32),
    Item(u64),
    FluidNode(u32),
}

/// What an entity is, for choosing a default asset.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum EntityKind {
    /// A model object; `kind` is the object kind name ("Processor", "Rack"...).
    Block { name: String, kind: String },
    /// A mobile resource (transporter, operator, AGV): moves and has a [`Motion`].
    Mobile {
        name: String,
        kind: String,
        role: MobileRole,
    },
    /// A flow item. `kind` is the item type label when the model sets one.
    Item { kind: Option<String> },
    /// A node of the fluid network ("Tank", "Pipe"...).
    FluidNode { name: String, kind: String },
}

/// Which animation family a mobile resource uses.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum MobileRole {
    /// Walks; carries items in its hands.
    Person,
    /// Drives; lifts items with forks (mast height = [`Pose::lift`]).
    Forklift,
    /// Drives along the network; carries items on top.
    Agv,
    /// Any other vehicle (generic transporter).
    Vehicle,
}

/// What an entity is doing now, independent of any engine's clip names.
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub enum Motion {
    #[default]
    Idle,
    /// Travelling at `speed` m/s (walk/drive cycle; the clip rate follows the speed).
    Moving { speed: f32 },
    /// Taking an item; `progress` goes 0 → 1 over the load time.
    Loading { progress: f32 },
    /// Releasing an item; `progress` goes 0 → 1 over the unload time.
    Unloading { progress: f32 },
    /// Busy at a station (operator utilised, processor processing).
    Working,
    /// Waiting for room or for a shared path segment.
    Blocked,
    /// Broken down, off shift or stopped by an external command.
    Stopped,
}

/// Where an entity is and what it is doing at one instant.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Pose {
    pub id: EntityId,
    /// World position of the entity's base (floor contact point), metres, Z-up.
    pub position: [f32; 3],
    /// Heading, radians (see the module conventions).
    pub yaw: f32,
    pub motion: Motion,
    /// Height of the carried load above the floor for forklifts (fork carriage), and of
    /// the hands for people; 0 when not applicable.
    pub lift: f32,
    /// Entity this one rides on or sits in (an item on a forklift, in a rack cell...).
    /// `position` is already in world coordinates; the parent only helps engines that
    /// want to attach meshes (e.g. VR grabbing, smooth interpolation).
    pub parent: Option<EntityId>,
    /// Block state name ("idle", "processing", "blocked"...), for colouring/labels.
    pub state: Option<String>,
    /// Fill fraction 0..1 for tanks; `None` otherwise.
    pub fill: Option<f32>,
}

/// Everything that moves or changes, at time `time`. Entities absent from a frame
/// do not exist at that instant (items not yet created or already destroyed).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Frame {
    pub time: f64,
    pub poses: Vec<Pose>,
}

/// One entity of the static layout.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct StaticEntity {
    pub id: EntityId,
    pub kind: EntityKind,
    /// Base position (metres, Z-up); mobile resources start here.
    pub position: [f32; 3],
    pub yaw: f32,
    /// Footprint/size hint `(length x, width y, height z)` in metres.
    pub size: [f32; 3],
    /// The object's `visual` from the model, when set.
    pub visual: Option<utsim_model::VisualDef>,
}

/// A segment of the path network or of a conveyor, for drawing floors and tracks.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Segment {
    pub from: [f32; 3],
    pub to: [f32; 3],
    /// Shared-segment capacity for AGV traffic, when controlled.
    pub capacity: Option<usize>,
}

/// The layout, sent once.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct StaticScene {
    pub entities: Vec<StaticEntity>,
    pub network: Vec<Segment>,
    /// Axis-aligned bounds `(min, max)` of everything, metres.
    pub bounds: ([f32; 3], [f32; 3]),
}

/// First line of a scene stream.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Header {
    pub protocol: String,
    pub model: String,
    pub units: String,
    pub up: String,
    pub start: f64,
    pub end: f64,
    pub scene: StaticScene,
}

/// Something that consumes a scene: a file, a socket, a VR runtime, a test.
pub trait SceneSink {
    fn header(&mut self, header: &Header) -> std::io::Result<()>;
    fn frame(&mut self, frame: &Frame) -> std::io::Result<()>;
}

/// Writes the JSON-lines stream described in the module docs.
pub struct JsonLines<W: std::io::Write>(pub W);

impl<W: std::io::Write> SceneSink for JsonLines<W> {
    fn header(&mut self, header: &Header) -> std::io::Result<()> {
        serde_json::to_writer(&mut self.0, header)?;
        self.0.write_all(b"\n")
    }

    fn frame(&mut self, frame: &Frame) -> std::io::Result<()> {
        serde_json::to_writer(&mut self.0, frame)?;
        self.0.write_all(b"\n")
    }
}
