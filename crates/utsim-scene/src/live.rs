//! The scene of a run that is still going: records arrive while the simulation advances.
//!
//! [`LiveScene`] owns the static part of a [`Replay`] (blocks, connections) and appends
//! the trace records it receives with [`LiveScene::push`]. [`LiveScene::frame`] answers
//! "what does the world look like at `t`?" from the records seen so far, using exactly
//! the code of the batch path ([`Scene::build`]): the index is rebuilt lazily, only when
//! records arrived since the last query, so a frame of a live run is by construction the
//! frame the batch path gives for a replay that holds the same records.
//!
//! # What the live path cannot know
//! A frame at `t` is computed from the records up to the latest `push`. Two quantities
//! of the batch derivation look *ahead* of `t`:
//! - the **visual transit** of an item between two fixed blocks lasts `min(transit, gap
//!   to the item's next event)`; the live default is a fixed [`LIVE_TRANSIT`], so an
//!   item whose next move is closer than that can show a slightly different progress
//!   until the move is recorded;
//! - the **progress of a load/unload window** of a carrier is measured against the end
//!   of its state, which is unknown while the state lasts (the progress stays 0 until a
//!   hand-off or the next state change closes the window), and with it the instant at
//!   which the item changes hands visually (the middle of the window);
//! - the **corner between two legs** driven without a stop is rounded only once the
//!   second leg is recorded, so the last half metre of a leg can turn a little later
//!   than in the batch frame.
//!
//! Both vanish once the records past `t` are in: after the whole run is pushed (or when
//! `t` is a little behind the newest record) the frame equals the batch one. Clients that
//! need exact agreement can render slightly in the past; the socket server renders at the
//! simulation clock instead, which is what keeps a VR view in step with the OPC UA
//! emulation.
//!
//! # Cost
//! Each rebuild is linear in the records so far (the same cost as loading a finished
//! run), and it happens at most once per frame period, off the simulation thread.

use utsim_core::TraceRecord;
use utsim_model::Model;
use utsim_model::replay::Replay;

use crate::compose::{Scene, SceneOptions};
use crate::timeline::TransitSetting;
use crate::{Frame, Header};

/// Length of the visual transit between fixed blocks in live mode, in time units
/// (1 unit = 1 s for speeds). Fixed, because the automatic length is a statistic of the
/// whole run.
pub const LIVE_TRANSIT: f64 = 0.4;

/// Incremental scene. See the module docs.
pub struct LiveScene {
    model: Option<Model>,
    transit: TransitSetting,
    /// Blocks and connections from the start; `records` grows with `push`.
    replay: Replay,
    scene: Option<Scene>,
    /// Records arrived since `scene` was built.
    dirty: bool,
    /// Time of the newest record.
    horizon: f64,
}

impl LiveScene {
    /// A live scene for `model` (validated), with no records yet.
    pub fn new(model: &Model, replication: u32) -> LiveScene {
        LiveScene::from_replay(model.empty_replay(replication), Some(model.clone()))
    }

    /// From the static part of a replay (its `records` are kept as the first ones) and,
    /// when available, the model it came from.
    pub fn from_replay(replay: Replay, model: Option<Model>) -> LiveScene {
        let horizon = replay.records.last().map_or(0.0, |r| r.t);
        LiveScene {
            model,
            transit: TransitSetting::Fixed(LIVE_TRANSIT),
            replay,
            scene: None,
            dirty: true,
            horizon,
        }
    }

    /// Changes the visual transit rule (the default is [`LIVE_TRANSIT`]).
    pub fn with_transit(mut self, transit: TransitSetting) -> LiveScene {
        self.transit = transit;
        self.dirty = true;
        self
    }

    /// Appends trace records, in the order the simulation produced them.
    pub fn push(&mut self, records: impl IntoIterator<Item = TraceRecord>) {
        let before = self.replay.records.len();
        self.replay.records.extend(records);
        if self.replay.records.len() != before {
            self.horizon = self.replay.records.last().map_or(0.0, |r| r.t);
            self.dirty = true;
        }
    }

    /// Number of records received.
    pub fn records(&self) -> usize {
        self.replay.records.len()
    }

    /// Time of the newest record (0 before any).
    pub fn horizon(&self) -> f64 {
        self.horizon
    }

    /// The batch scene of the records so far (rebuilt when records arrived).
    pub fn scene(&mut self) -> &Scene {
        if self.dirty || self.scene.is_none() {
            let opts = SceneOptions {
                transit: self.transit,
                model: self.model.as_ref(),
            };
            self.scene = Some(Scene::build(&self.replay, &opts));
            self.dirty = false;
        }
        self.scene.as_ref().expect("built above")
    }

    /// First line of the stream: the static layout, `end` = the model's stop time.
    pub fn header(&mut self) -> Header {
        self.scene().header()
    }

    /// Everything that moves or changes at `t`.
    pub fn frame(&mut self, t: f64) -> Frame {
        self.scene().frame(t)
    }
}
