//! People kept out of the way of an overhead crane's suspended load (crane option
//! `keep_clear`): a board shared by the cranes and by everyone who walks or drives on
//! the floor.
//!
//! Before a loaded move, the crane books the **strip** its load sweeps on the floor: the
//! square under the hook (`load_zone`, turned with the runway) dragged along the move.
//! The booking lasts until the move ends.
//!
//! - A person (or vehicle) about to walk a piece of a path that enters a booked strip
//!   before the booking ends stops at the edge and waits for the load to pass; the piece
//!   is split at the edge. A piece that starts inside a strip goes on (the person leaves).
//! - A crane about to move a load across a person who is walking through the strip waits
//!   until that person is out. Meanwhile its strip is already booked, so nobody else
//!   walks in.
//!
//! People standing still (working at a station) are not moved: the exposure analysis
//! (`utsim exposicao`) still counts them. Coordinates are metres on the floor.

use utsim_core::BlockId;

const EPS: f64 = 1e-9;

/// A convex polygon on the floor, counter-clockwise.
pub type Strip = Vec<[f64; 2]>;

/// The strip a load sweeps from `from` to `to` on the floor: the square of half side
/// `half` turned by `yaw` (radians), dragged along the move (the convex hull of the
/// square at both ends).
pub fn sweep(from: [f64; 2], to: [f64; 2], half: f64, yaw: f64) -> Strip {
    let (s, c) = yaw.sin_cos();
    let mut pts = Vec::with_capacity(8);
    for p in [from, to] {
        for (du, dv) in [(-half, -half), (half, -half), (half, half), (-half, half)] {
            pts.push([p[0] + c * du - s * dv, p[1] + s * du + c * dv]);
        }
    }
    hull(pts)
}

/// Convex hull, counter-clockwise (monotone chain).
fn hull(mut pts: Vec<[f64; 2]>) -> Strip {
    pts.sort_by(|a, b| a[0].total_cmp(&b[0]).then(a[1].total_cmp(&b[1])));
    pts.dedup_by(|a, b| (a[0] - b[0]).abs() < EPS && (a[1] - b[1]).abs() < EPS);
    if pts.len() < 3 {
        return pts;
    }
    let cross = |o: [f64; 2], a: [f64; 2], b: [f64; 2]| {
        (a[0] - o[0]) * (b[1] - o[1]) - (a[1] - o[1]) * (b[0] - o[0])
    };
    let mut out: Vec<[f64; 2]> = Vec::with_capacity(pts.len() * 2);
    for pass in 0..2 {
        let start = out.len();
        let iter: Box<dyn Iterator<Item = &[f64; 2]>> = if pass == 0 {
            Box::new(pts.iter())
        } else {
            Box::new(pts.iter().rev())
        };
        for &p in iter {
            while out.len() >= start + 2 && cross(out[out.len() - 2], out[out.len() - 1], p) <= EPS
            {
                out.pop();
            }
            out.push(p);
        }
        out.pop();
    }
    out
}

/// The part of the segment `a → b` inside the strip, as fractions `(enter, leave)` of the
/// segment (Cyrus–Beck). `None` when it misses the strip.
pub fn clip(strip: &Strip, a: [f64; 2], b: [f64; 2]) -> Option<(f64, f64)> {
    if strip.len() < 3 {
        return None;
    }
    let d = [b[0] - a[0], b[1] - a[1]];
    let (mut lo, mut hi) = (0.0_f64, 1.0_f64);
    for i in 0..strip.len() {
        let p = strip[i];
        let q = strip[(i + 1) % strip.len()];
        // Inward normal of a counter-clockwise edge.
        let n = [-(q[1] - p[1]), q[0] - p[0]];
        let num = n[0] * (a[0] - p[0]) + n[1] * (a[1] - p[1]);
        let den = n[0] * d[0] + n[1] * d[1];
        if den.abs() < EPS {
            if num < -EPS {
                return None; // parallel and outside
            }
            continue;
        }
        let t = -num / den;
        if den > 0.0 {
            lo = lo.max(t);
        } else {
            hi = hi.min(t);
        }
        if lo > hi + EPS {
            return None;
        }
    }
    (hi - lo > EPS).then_some((lo, hi))
}

/// Whether `p` is inside the strip, farther than 1 mm from its edges.
pub fn inside(strip: &Strip, p: [f64; 2]) -> bool {
    if strip.len() < 3 {
        return false;
    }
    (0..strip.len()).all(|i| {
        let a = strip[i];
        let b = strip[(i + 1) % strip.len()];
        let len = (b[0] - a[0]).hypot(b[1] - a[1]);
        len > 0.0 && ((b[0] - a[0]) * (p[1] - a[1]) - (b[1] - a[1]) * (p[0] - a[0])) / len > 1e-3
    })
}

#[derive(Clone, Debug)]
struct Load {
    crane: BlockId,
    strip: Strip,
    until: f64,
}

/// A straight piece someone walks (or drives) from `t0` to `t1`.
#[derive(Clone, Copy, Debug)]
struct Walk {
    who: BlockId,
    a: [f64; 2],
    b: [f64; 2],
    t0: f64,
    t1: f64,
}

/// The board shared by the cranes with `keep_clear` and the mobile resources on the
/// floor. One per simulation.
#[derive(Debug, Default)]
pub struct LoadBoard {
    loads: Vec<Load>,
    walks: Vec<Walk>,
}

impl LoadBoard {
    /// The crane's load will sweep `strip` until `until` (replaces its booking).
    pub fn book(&mut self, crane: BlockId, strip: Strip, until: f64) {
        self.loads.retain(|l| l.crane != crane);
        self.loads.push(Load {
            crane,
            strip,
            until,
        });
    }

    /// `who` walks from `a` to `b` between `t0` and `t1` (replaces its last piece).
    pub fn walk(&mut self, who: BlockId, a: [f64; 2], b: [f64; 2], t0: f64, t1: f64) {
        self.walks.retain(|w| w.who != who);
        if t1 > t0 && (a[0] - b[0]).hypot(a[1] - b[1]) > EPS {
            self.walks.push(Walk { who, a, b, t0, t1 });
        }
    }

    /// Drops what `who` (a crane or a walker) had on the board.
    pub fn forget(&mut self, who: BlockId) {
        self.loads.retain(|l| l.crane != who);
        self.walks.retain(|w| w.who != who);
    }

    /// Someone at `a` is about to walk to `b`, taking `dur`, at `now`: the first booked
    /// strip it would enter while the load is still on it, as (fraction of the piece
    /// where it enters, when the load is gone). Strips it starts inside of are ignored
    /// (it walks out).
    pub fn ahead(&mut self, a: [f64; 2], b: [f64; 2], dur: f64, now: f64) -> Option<(f64, f64)> {
        self.loads.retain(|l| l.until > now + EPS);
        let mut first: Option<(f64, f64)> = None;
        for l in &self.loads {
            if inside(&l.strip, a) {
                continue;
            }
            let Some((enter, _)) = clip(&l.strip, a, b) else {
                continue;
            };
            if now + enter * dur >= l.until - EPS {
                continue; // the load is gone before it gets there
            }
            first = match first {
                Some((f, u)) if f < enter - EPS => Some((f, u)),
                Some((f, u)) if (f - enter).abs() <= EPS => Some((f, u.max(l.until))),
                _ => Some((enter, l.until)),
            };
        }
        first
    }

    /// A crane (`crane`, its operator `except`) is about to sweep `strip` from `now` to
    /// `end`: when the last of the people walking through it during that time is out.
    /// `None`: nobody is in the way.
    pub fn in_way(
        &mut self,
        strip: &Strip,
        now: f64,
        end: f64,
        except: Option<BlockId>,
    ) -> Option<f64> {
        self.walks.retain(|w| w.t1 > now + EPS);
        let mut out: Option<f64> = None;
        for w in &self.walks {
            if Some(w.who) == except {
                continue;
            }
            let Some((enter, leave)) = clip(strip, w.a, w.b) else {
                continue;
            };
            let span = w.t1 - w.t0;
            let (t_in, t_out) = (w.t0 + enter * span, w.t0 + leave * span);
            if t_out > now + EPS && t_in < end - EPS {
                out = Some(out.map_or(t_out, |o: f64| o.max(t_out)));
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_strip_of_a_move_along_x() {
        let s = sweep([0.0, 0.0], [10.0, 0.0], 1.0, 0.0);
        assert_eq!(s.len(), 4, "{s:?}");
        // A person crossing it at x = 5, from y = -5 to 5, is inside from -1 to 1.
        let (a, b) = clip(&s, [5.0, -5.0], [5.0, 5.0]).unwrap();
        assert!((a - 0.4).abs() < 1e-9 && (b - 0.6).abs() < 1e-9, "{a} {b}");
        assert!(clip(&s, [12.0, -5.0], [12.0, 5.0]).is_none());
        assert!(inside(&s, [5.0, 0.0]));
        assert!(!inside(&s, [5.0, 1.0]), "on the edge is outside");
        // Turned by 45°, a diagonal move: a hexagon.
        let s = sweep([0.0, 0.0], [10.0, 0.0], 1.0, std::f64::consts::FRAC_PI_4);
        assert_eq!(s.len(), 6, "{s:?}");
    }

    #[test]
    fn walkers_wait_for_the_load_and_the_crane_for_the_walkers() {
        let mut b = LoadBoard::default();
        let crane = BlockId(0);
        let person = BlockId(1);
        b.book(crane, sweep([0.0, 0.0], [10.0, 0.0], 1.0, 0.0), 20.0);
        // At 5 s, 10 m at 1 m/s across the strip: it would get there at 9 s, before 20 s.
        assert_eq!(
            b.ahead([5.0, -5.0], [5.0, 5.0], 10.0, 5.0),
            Some((0.4, 20.0))
        );
        // Starting inside, it walks out.
        assert_eq!(b.ahead([5.0, 0.0], [5.0, 5.0], 5.0, 5.0), None);
        // After 20 s the load is gone.
        assert_eq!(b.ahead([5.0, -5.0], [5.0, 5.0], 10.0, 20.0), None);
        // A person crossing from 0 s to 10 s is inside from 4 to 6 s.
        b.walk(person, [5.0, -5.0], [5.0, 5.0], 0.0, 10.0);
        let strip = sweep([0.0, 0.0], [10.0, 0.0], 1.0, 0.0);
        assert_eq!(b.in_way(&strip, 3.0, 8.0, None), Some(6.0));
        assert_eq!(b.in_way(&strip, 3.0, 8.0, Some(person)), None);
        assert_eq!(b.in_way(&strip, 6.5, 8.0, None), None);
        b.forget(person);
        assert_eq!(b.in_way(&strip, 3.0, 8.0, None), None);
    }
}
