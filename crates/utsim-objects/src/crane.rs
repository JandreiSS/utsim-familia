//! Overhead crane (ponte rolante): the kinematics a [`crate::Transporter`] uses in crane
//! mode ([`crate::Transporter::crane`]).
//!
//! The crane covers a rectangle on the floor: `length` along its runway (the local `x`
//! of the object, where the bridge moves) and `span` across it (the local `y`, where the
//! trolley moves on the girder), centred on the object's position and turned by its yaw.
//! Each served point is projected into that rectangle. Three independent axes, each
//! with its own speed, acceleration and deceleration (the trapezoidal profile of
//! [`travel_time`]): the bridge, the trolley and the hoist (vertical).
//!
//! **Order of the axes** ([`CraneMotion`]):
//! - `Safe` (default): the hook travels at `hook_height`; the bridge and the trolley move
//!   together (the time is the longer of the two) only with the hook up, and the hook
//!   goes down and up on its own at each end (lower, hook on or off, raise).
//! - `Sequential`: like `Safe`, with the bridge and then the trolley (the times add up).
//! - `Parallel`: the three axes together; the hook goes straight from its height to the
//!   height of the next point while the bridge and the trolley move, and stays down
//!   after hooking or unhooking.
//!
//! **Load.** The weight of an item is its `weight_label` (kg); an item heavier than the
//! capacity is refused: the transporter counts it and does not carry it.

use serde::{Deserialize, Serialize};
use utsim_core::task::travel_time;

/// Speed (m per time unit) and optional acceleration and deceleration of one axis.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Axis {
    #[serde(alias = "velocidade")]
    pub speed: f64,
    #[serde(default, skip_serializing_if = "Option::is_none", alias = "aceleracao")]
    pub acceleration: Option<f64>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        alias = "desaceleracao"
    )]
    pub deceleration: Option<f64>,
}

impl Axis {
    /// Time to move `distance` on this axis, from rest to rest.
    pub fn time(&self, distance: f64) -> f64 {
        travel_time(
            distance.abs(),
            self.speed,
            self.acceleration,
            self.deceleration,
        )
    }

    pub fn validate(&self, what: &str) -> Result<(), String> {
        if !(self.speed.is_finite() && self.speed > 0.0) {
            return Err(format!("{what}.speed deve ser finito e > 0"));
        }
        for (name, v) in [
            ("acceleration", self.acceleration),
            ("deceleration", self.deceleration),
        ] {
            if v.is_some_and(|v| !(v.is_finite() && v > 0.0)) {
                return Err(format!("{what}.{name} deve ser finito e > 0"));
            }
        }
        Ok(())
    }
}

/// Order of the axes (see the module docs).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum CraneMotion {
    #[default]
    #[serde(alias = "Segura")]
    Safe,
    #[serde(alias = "Sequencial")]
    Sequential,
    #[serde(alias = "Paralela")]
    Parallel,
}

/// Geometry and axes of one crane.
#[derive(Clone, Debug, PartialEq)]
pub struct CraneSpec {
    /// Centre of the covered rectangle (the object's position).
    pub centre: [f64; 3],
    /// Turn of the runway, radians, counter-clockwise from the global `x`.
    pub yaw: f64,
    /// Length of the runway (bridge travel) and span (trolley travel), m.
    pub length: f64,
    pub span: f64,
    pub bridge: Axis,
    pub trolley: Axis,
    pub hoist: Axis,
    /// Height the hook travels at (above the floor of the crane's position).
    pub hook_height: f64,
    pub motion: CraneMotion,
    /// Capacity in kg.
    pub capacity_kg: f64,
    pub weight_label: String,
}

/// Where the hook is: bridge, trolley (local coordinates) and height.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Hook {
    pub u: f64,
    pub v: f64,
    pub z: f64,
}

/// Time each axis moved in one motion (statistics).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct AxisTimes {
    pub bridge: f64,
    pub trolley: f64,
    pub hoist: f64,
}

impl std::ops::AddAssign for AxisTimes {
    fn add_assign(&mut self, o: Self) {
        self.bridge += o.bridge;
        self.trolley += o.trolley;
        self.hoist += o.hoist;
    }
}

impl CraneSpec {
    /// Local coordinates of a floor point, clamped to the covered rectangle.
    pub fn local(&self, p: [f64; 3]) -> (f64, f64) {
        let (dx, dy) = (p[0] - self.centre[0], p[1] - self.centre[1]);
        let (s, c) = self.yaw.sin_cos();
        let u = c * dx + s * dy;
        let v = -s * dx + c * dy;
        (
            u.clamp(-self.length / 2.0, self.length / 2.0),
            v.clamp(-self.span / 2.0, self.span / 2.0),
        )
    }

    /// Whether a floor point is inside the covered rectangle (with a 1 mm tolerance).
    pub fn covers(&self, p: [f64; 3]) -> bool {
        let (dx, dy) = (p[0] - self.centre[0], p[1] - self.centre[1]);
        let (s, c) = self.yaw.sin_cos();
        let (u, v) = (c * dx + s * dy, -s * dx + c * dy);
        u.abs() <= self.length / 2.0 + 1e-3 && v.abs() <= self.span / 2.0 + 1e-3
    }

    /// Global point of local `(u, v)` at height `z` (above the crane's floor).
    pub fn global(&self, u: f64, v: f64, z: f64) -> [f64; 3] {
        let (s, c) = self.yaw.sin_cos();
        [
            self.centre[0] + c * u - s * v,
            self.centre[1] + s * u + c * v,
            self.centre[2] + z,
        ]
    }

    /// Height of the hook at a served point: the point's own height above the crane's
    /// floor, never below it nor above the travel height.
    pub fn work_height(&self, p: [f64; 3]) -> f64 {
        (p[2] - self.centre[2]).clamp(0.0, self.hook_height)
    }

    /// The hook at rest at the crane's position, up.
    pub fn start(&self) -> Hook {
        Hook {
            u: 0.0,
            v: 0.0,
            z: self.hook_height,
        }
    }

    /// Horizontal move (and, in `Parallel`, the vertical one) from `hook` to above
    /// `target`: duration, axis times and the hook at the end.
    pub fn travel(&self, hook: Hook, target: [f64; 3]) -> (f64, AxisTimes, Hook) {
        let (u, v) = self.local(target);
        let tb = self.bridge.time(u - hook.u);
        let tt = self.trolley.time(v - hook.v);
        let (z, th) = match self.motion {
            CraneMotion::Parallel => {
                let z = self.work_height(target);
                (z, self.hoist.time(z - hook.z))
            }
            _ => (hook.z, 0.0),
        };
        let t = match self.motion {
            CraneMotion::Sequential => tb + tt,
            CraneMotion::Safe => tb.max(tt),
            CraneMotion::Parallel => tb.max(tt).max(th),
        };
        (
            t,
            AxisTimes {
                bridge: tb,
                trolley: tt,
                hoist: th,
            },
            Hook { u, v, z },
        )
    }

    /// Hoist time around hooking or unhooking at `at` with the hook at `hook`: lowering
    /// to the work height and raising back to the travel height (`Safe` and
    /// `Sequential`); nothing in `Parallel`, where the hook already came down while
    /// travelling. Returns (time before the handling, time after it, hook at the end).
    pub fn hoist_around(&self, hook: Hook, at: [f64; 3]) -> (f64, f64, Hook) {
        match self.motion {
            CraneMotion::Parallel => (0.0, 0.0, hook),
            _ => {
                let z = self.work_height(at);
                let down = self.hoist.time(hook.z - z);
                let up = self.hoist.time(self.hook_height - z);
                (
                    down,
                    up,
                    Hook {
                        z: self.hook_height,
                        ..hook
                    },
                )
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn axis(speed: f64) -> Axis {
        Axis {
            speed,
            acceleration: None,
            deceleration: None,
        }
    }

    fn spec(motion: CraneMotion) -> CraneSpec {
        CraneSpec {
            centre: [10.0, 5.0, 0.0],
            yaw: 0.0,
            length: 20.0,
            span: 10.0,
            bridge: axis(0.5),
            trolley: axis(0.25),
            hoist: axis(0.1),
            hook_height: 5.0,
            motion,
            capacity_kg: 1000.0,
            weight_label: "weight".into(),
        }
    }

    #[test]
    fn safe_sequential_and_parallel_by_hand() {
        // From the centre to (18, 8): the bridge moves 8 m at 0.5 (16 s), the trolley
        // 3 m at 0.25 (12 s).
        let target = [18.0, 8.0, 1.0];
        let s = spec(CraneMotion::Safe);
        let (t, axes, hook) = s.travel(s.start(), target);
        assert_eq!(
            (t, axes.bridge, axes.trolley, axes.hoist),
            (16.0, 16.0, 12.0, 0.0)
        );
        assert_eq!((hook.u, hook.v, hook.z), (8.0, 3.0, 5.0));
        // Lower from 5 to 1 m and raise back: 40 s each way at 0.1 m/s.
        let (down, up, after) = s.hoist_around(hook, target);
        assert_eq!((down, up, after.z), (40.0, 40.0, 5.0));

        let q = spec(CraneMotion::Sequential);
        assert_eq!(q.travel(q.start(), target).0, 28.0);

        // Parallel: the hook comes down 4 m (40 s) while travelling, and stays down.
        let p = spec(CraneMotion::Parallel);
        let (t, _, hook) = p.travel(p.start(), target);
        assert_eq!((t, hook.z), (40.0, 1.0));
        assert_eq!(p.hoist_around(hook, target), (0.0, 0.0, hook));
    }

    #[test]
    fn the_covered_rectangle_turns_with_the_object() {
        let mut s = spec(CraneMotion::Safe);
        s.yaw = std::f64::consts::FRAC_PI_2;
        // Turned 90°: the runway runs along the global y.
        assert!(s.covers([10.0, 14.0, 0.0]));
        assert!(!s.covers([19.0, 5.0, 0.0]));
        let (u, v) = s.local([12.0, 14.0, 0.0]);
        assert!(
            (u - 9.0).abs() < 1e-12 && (v + 2.0).abs() < 1e-12,
            "{u} {v}"
        );
        let g = s.global(u, v, 2.0);
        assert!((g[0] - 12.0).abs() < 1e-12 && (g[1] - 14.0).abs() < 1e-12 && g[2] == 2.0);
        // A point outside is clamped to the edge.
        assert_eq!(s.local([10.0, 40.0, 0.0]).0, 10.0);
    }
}
