//! Work for mobile resources (phase 4): transport requests, operator requests,
//! messages between blocks and routing in space.
//!
//! A block whose output uses a transporter does not hand its items over directly:
//! when a destination is found, the kernel reserves room there and sends a
//! [`TaskKind::Transport`] to the carrier. The carrier travels, calls
//! [`Ctx::pick_up`](crate::Ctx::pick_up), travels again and calls
//! [`Ctx::drop_off`](crate::Ctx::drop_off). Dispatchers just forward tasks.

use serde::{Deserialize, Serialize};

use crate::block::BlockId;
use crate::item::ItemId;

/// Identifies one task request for its whole life.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct RequestId(pub u64);

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum TaskKind {
    /// Carry `item` from block `from` to input `port` of block `to`. Room in `to`
    /// is already reserved; the item waits in `from` until it is picked up.
    Transport {
        item: ItemId,
        from: BlockId,
        to: BlockId,
        port: usize,
    },
    /// Go to `station` and stay there, working for it, until it releases you with
    /// [`MSG_RELEASE`]. On arrival send [`MSG_ARRIVED`] to the station with the
    /// request id as payload. `tag` is the station's own label for the request.
    Operate { station: BlockId, tag: u32 },
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TaskRequest {
    pub id: RequestId,
    pub kind: TaskKind,
    /// Block that asked for the work (the origin of a transport, or the station).
    pub requester: BlockId,
    /// Higher runs first.
    pub priority: i32,
}

/// Message tags understood by every block. Block-specific tags start at [`MSG_USER`].
pub const MSG_TRANSPORT_RESERVED: u32 = 1;
/// An operator reached the station (payload: request id).
pub const MSG_ARRIVED: u32 = 2;
/// The station no longer needs the operator (payload: request id).
pub const MSG_RELEASE: u32 = 3;
/// A team member became free and can take work (sent to its dispatcher).
pub const MSG_IDLE: u32 = 4;
/// An item entered a block this block subscribed to (payload: item key; `from` = that block).
pub const MSG_ITEM_ENTERED: u32 = 5;
/// An item left a block this block subscribed to (payload: item key; `from` = that block).
pub const MSG_ITEM_EXITED: u32 = 6;
/// The external agent answered a decision this block requested (payload: option index).
pub const MSG_DECISION: u32 = 7;
/// A `try_inject_item` was refused (target full or stopped); payload: the target block index.
pub const MSG_INJECT_REFUSED: u32 = 8;
pub const MSG_USER: u32 = 100;

/// Path between two points in the layout. The default is a straight line; a path
/// network (phase 4) replaces it.
pub trait Router: Send + Sync {
    /// Waypoints from `from` to `to`, including both ends.
    fn route(&self, from: [f64; 3], to: [f64; 3]) -> Vec<[f64; 3]>;
}

pub struct StraightLine;

impl Router for StraightLine {
    fn route(&self, from: [f64; 3], to: [f64; 3]) -> Vec<[f64; 3]> {
        vec![from, to]
    }
}

pub fn distance(a: [f64; 3], b: [f64; 3]) -> f64 {
    ((a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2) + (a[2] - b[2]).powi(2)).sqrt()
}

pub fn path_length(path: &[[f64; 3]]) -> f64 {
    path.windows(2).map(|w| distance(w[0], w[1])).sum()
}

/// Time to cover `length` starting and ending at rest, with maximum `speed` and the
/// given acceleration/deceleration (`None` = instantaneous). Trapezoidal profile,
/// triangular when the distance is too short to reach `speed`.
pub fn travel_time(length: f64, speed: f64, accel: Option<f64>, decel: Option<f64>) -> f64 {
    if length <= 0.0 {
        return 0.0;
    }
    let (a, d) = (
        accel.unwrap_or(f64::INFINITY),
        decel.unwrap_or(f64::INFINITY),
    );
    let ramp = speed * speed / 2.0 * (1.0 / a + 1.0 / d);
    if ramp <= length {
        length / speed + speed / 2.0 * (1.0 / a + 1.0 / d)
    } else {
        // Peak speed v with v²/2·(1/a + 1/d) = length.
        let v = (2.0 * length / (1.0 / a + 1.0 / d)).sqrt();
        v / a + v / d
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn travel_time_profiles() {
        assert_eq!(travel_time(10.0, 2.0, None, None), 5.0);
        // a = d = 1, v = 2: ramps cover 4 m in 4 s, cruise 6 m in 3 s.
        assert!((travel_time(10.0, 2.0, Some(1.0), Some(1.0)) - 7.0).abs() < 1e-12);
        // 2 m with a = d = 1 never reaches v = 2: peak √2, time 2√2.
        assert!((travel_time(2.0, 2.0, Some(1.0), Some(1.0)) - 2.0 * 2f64.sqrt()).abs() < 1e-12);
        assert_eq!(travel_time(0.0, 1.0, None, None), 0.0);
    }

    #[test]
    fn straight_line_path() {
        let p = StraightLine.route([0.0; 3], [3.0, 4.0, 0.0]);
        assert_eq!(path_length(&p), 5.0);
    }
}
