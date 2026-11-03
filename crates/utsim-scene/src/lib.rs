//! Engine-independent 3D scene of a UTSim run (contract).
//!
//! The simulation produces a trace; this crate turns a [`Replay`] into *what a
//! renderer must draw at time `t`*: the static layout once ([`StaticScene`]) and one
//! [`Frame`] per instant, with the pose of every block, item and mobile resource and a
//! semantic [`Motion`] (walking, lifting forks, loading...) that each engine maps to its
//! own animation clips. The Bevy viewer, the browser page, a future VR mode and external
//! engines (the Godot front-end for 3D, VR and AR) all consume the same frames, so they never disagree on
//! where things are.
//!
//! # Conventions (fixed by protocol version [`PROTOCOL`])
//! - **Units:** metres, seconds of simulated time (1 model time unit = 1 s for speeds).
//! - **Axes:** the model's own plan coordinates. Right-handed, `x` right, `y` along
//!   the floor, **`z` up**; the floor is `z = 0`. Bevy (Y-up) maps `(x, y, z)` to
//!   `(x, z, -y)`, and so does Godot (also Y-up, right-handed, metres).
//! - **Yaw:** radians, counter-clockwise seen from above, `0` = facing `+x`.
//! - **Human scale:** assets are authored at real size, so a person is ~1.75 m tall and a
//!   VR camera at eye height (~1.6 m) sees the plant at the right scale.
//!
//! # Stream (`utsim cena`, and the live socket)
//! JSON lines: the first line is a [`Header`] (protocol, conventions, static scene), every
//! following line is a [`Frame`]. A client needs nothing else to animate a run.
//!
//! # Extension messages (`"tipo"`)
//! A line that has a `"tipo"` field is an extension message, not a header or a frame:
//! [`ServerMessage`] (server to client: `aviso`, `fim`) and [`ClientMessage`] (client to
//! server: `comando`). Header and frames never carry `"tipo"`, so files written by
//! `utsim cena` are unchanged, and **readers must ignore lines whose `"tipo"` they do not
//! know**. Extension messages exist only on the live socket ([`server`], feature
//! `server`), where the same stream is served over plain TCP (one JSON per line) and over
//! WebSocket (one JSON per text message), see [`live`] for how frames of a running
//! simulation are produced.
//!
//! # Building a scene
//! [`Scene::build`] indexes a replay; [`Scene::header`] and [`Scene::frame`] give the
//! stream. The derivation rules (roles, yaw, motion, lift, load windows) are in
//! [`compose`]. The pure modules below work in the Y-up *layout space* of the Bevy
//! viewer ([`layout::to_bevy`]); the stream is in model coordinates.

use serde::{Deserialize, Serialize};

pub use utsim_model::replay::Replay;

pub mod compose;
pub mod fluids;
pub mod geometry;
pub mod handling;
pub mod heatmap;
pub mod layout;
pub mod links;
pub mod live;
pub mod network;
#[cfg(all(feature = "server", not(target_arch = "wasm32")))]
pub mod server;
pub mod style;
pub mod timeline;
pub mod tokens;
pub mod turning;

pub use compose::{ItemPlacement, MobilePose, Scene, SceneOptions};
pub use links::{LinkKind, LinkLine, team_links};
pub use live::LiveScene;

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
    /// Item type of an item (1-based index into [`StaticScene::item_types`]), when the
    /// model declares types. Absent otherwise.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub item_type: Option<u32>,
    /// Where a moving mobile resource is going: the end of the travel in progress
    /// (metres, Z-up; for an AGV the end of the current network piece). Absent when it
    /// stands still and for every other entity.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub task_target: Option<[f32; 3]>,
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
    /// Item types of the model: the colour and the look of the items whose
    /// [`Pose::item_type`] points here. Empty (and absent from the JSON) without types.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub item_types: Vec<ItemTypeStyle>,
    /// Named floor areas of the model (waiting areas, break rooms), drawn on the floor.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub areas: Vec<AreaShape>,
    /// Team and request links (dispatcher → member, object → carrier, processor →
    /// operators, object → repair crew, member → home and break place), see [`links`].
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub links: Vec<LinkLine>,
}

/// A named floor area (the model's `areas`).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct AreaShape {
    pub name: String,
    /// Centre on the floor, metres, Z-up.
    pub position: [f32; 3],
    /// Length along x and width along y, metres.
    pub size: [f32; 2],
}

/// How the items of one item type look (the model's `item_types`).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ItemTypeStyle {
    /// 1-based index, the value of the item's `type` label and of [`Pose::item_type`].
    pub index: u32,
    pub name: String,
    /// "#rrggbb", when the type sets one.
    pub color: Option<String>,
    /// Asset id of the built-in library or a `.glb` file, when the type sets one.
    pub asset: Option<String>,
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

/// A value written by a client command: the same two kinds the emulator accepts.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum CommandValue {
    Bool(bool),
    Num(f64),
}

/// Client to server extension messages of `utsim-scene/1`.
///
/// `{"tipo":"comando","variavel":"M1.Parar","valor":true}` writes a variable of the
/// emulation, with the names of the OPC UA address space: `<block>.Parar` (boolean:
/// stop or resume a block) and `Velocidade` (number: simulation speed factor). The
/// server applies it at the current simulated time and logs it like a command from an
/// OPC UA client. A command the emulation rejects (unknown block, wrong value kind) is
/// answered with a [`ServerMessage::Aviso`]; there is no acknowledgement of success: the
/// effect shows in the next frames (`state` of the block becomes `stopped`).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "tipo", rename_all = "lowercase")]
pub enum ClientMessage {
    Comando {
        variavel: String,
        valor: CommandValue,
    },
}

/// Server to client extension messages of `utsim-scene/1` (socket only).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "tipo", rename_all = "lowercase")]
pub enum ServerMessage {
    /// Something the client should know: a rejected message, a rejected command.
    Aviso { mensagem: String },
    /// The run ended at simulated time `tempo`; no more frames follow.
    Fim { tempo: f64 },
}
