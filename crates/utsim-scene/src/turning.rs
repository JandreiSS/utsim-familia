//! Smooth turning through the corners of a travel path (pure, model plan coordinates).
//!
//! The trace gives a mobile resource a polyline and a duration; driven literally, its
//! heading would jump at every vertex. [`sample`] rounds each corner instead: over the
//! last and the first [`TURN_HALF`] metres of path around a vertex (less on short
//! pieces: never more than half of either piece) the position follows a circular fillet
//! tangent to both pieces and the heading turns at a constant rate, so position and
//! heading are both continuous. The parameter is still the arc length `s` *of the
//! polyline*: the fillet is traversed while `s` crosses the corner zone, so a leg starts
//! and ends exactly where and when the trace says, and straight pieces are unchanged.
//!
//! A corner can be shared with the neighbouring leg (the vehicle goes on without
//! stopping, as on the pieces of an AGV route): the caller passes the adjacent piece of
//! that leg as a [`Neighbour`], and both legs round the same fillet, each its own half.
//! Nearly reversing corners (more than [`REVERSAL`]) have no useful fillet: the vehicle
//! stays on the polyline and turns on the spot across the zone.

use std::f64::consts::PI;

/// Path length on each side of a vertex over which the turn happens, metres.
pub const TURN_HALF: f64 = 0.5;
/// Turn angle above which a corner is treated as a reversal (radians, 170 degrees).
pub const REVERSAL: f64 = 170.0 * PI / 180.0;

/// The piece of a neighbouring leg that meets this leg at a shared corner.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Neighbour {
    /// Unit direction of travel along that piece (model plan, `x`, `y`).
    pub dir: [f64; 2],
    /// Length of that piece, metres.
    pub len: f64,
}

/// A non-degenerate straight piece of a path.
#[derive(Clone, Copy, Debug)]
struct Piece {
    a: [f64; 3],
    b: [f64; 3],
    dir: [f64; 2],
    len: f64,
    /// Arc length at `a`.
    s0: f64,
}

fn pieces(path: &[[f64; 3]], cum: &[f64]) -> Vec<Piece> {
    let mut out = Vec::new();
    for k in 1..path.len().min(cum.len()) {
        let (a, b) = (path[k - 1], path[k]);
        let (dx, dy) = (b[0] - a[0], b[1] - a[1]);
        let flat = (dx * dx + dy * dy).sqrt();
        let len = cum[k] - cum[k - 1];
        if flat <= 1e-9 || len <= 1e-9 {
            continue;
        }
        out.push(Piece {
            a,
            b,
            dir: [dx / flat, dy / flat],
            len,
            s0: cum[k - 1],
        });
    }
    out
}

/// First piece of a path, as the [`Neighbour`] a preceding leg sees.
pub fn first_piece(path: &[[f64; 3]], cum: &[f64]) -> Option<Neighbour> {
    pieces(path, cum).first().map(|p| Neighbour {
        dir: p.dir,
        len: p.len,
    })
}

/// Last piece of a path, as the [`Neighbour`] a following leg sees.
pub fn last_piece(path: &[[f64; 3]], cum: &[f64]) -> Option<Neighbour> {
    pieces(path, cum).last().map(|p| Neighbour {
        dir: p.dir,
        len: p.len,
    })
}

fn heading(dir: [f64; 2]) -> f64 {
    dir[1].atan2(dir[0])
}

/// Angle in `(-pi, pi]`.
fn wrap(angle: f64) -> f64 {
    angle.sin().atan2(angle.cos())
}

/// Position and heading at fraction `u` (0 at the entry tangent point, 1 at the exit)
/// of the rounded corner at `v`, entered along `din` and left along `dout`, with
/// tangent length `d`. `z` is the height to report.
fn corner(v: [f64; 3], din: [f64; 2], dout: [f64; 2], d: f64, u: f64, z: f64) -> ([f64; 3], f64) {
    let cross = din[0] * dout[1] - din[1] * dout[0];
    let dot = din[0] * dout[0] + din[1] * dout[1];
    let theta = cross.atan2(dot);
    let yaw = wrap(heading(din) + u * theta);
    if theta.abs() < 1e-9 {
        let off = (2.0 * u - 1.0) * d;
        return ([v[0] + din[0] * off, v[1] + din[1] * off, z], yaw);
    }
    if theta.abs() > REVERSAL {
        // On the spot: back along the incoming piece, then out along the outgoing one.
        let p = if u < 0.5 {
            let k = d * (1.0 - 2.0 * u);
            [v[0] - din[0] * k, v[1] - din[1] * k]
        } else {
            let k = d * (2.0 * u - 1.0);
            [v[0] + dout[0] * k, v[1] + dout[1] * k]
        };
        return ([p[0], p[1], z], yaw);
    }
    // Fillet tangent to both pieces at distance d from the vertex.
    let r = d / (theta.abs() / 2.0).tan();
    let side = theta.signum();
    let t1 = [v[0] - din[0] * d, v[1] - din[1] * d];
    // Centre: from the entry point, towards the inside of the turn.
    let c = [t1[0] - din[1] * r * side, t1[1] + din[0] * r * side];
    let (rx, ry) = (t1[0] - c[0], t1[1] - c[1]);
    let (s, co) = (u * theta).sin_cos();
    ([c[0] + rx * co - ry * s, c[1] + rx * s + ry * co, z], yaw)
}

/// Position (model coordinates) and heading (radians from `+x`, counter-clockwise) at
/// arc length `s` of the polyline `path` (`cum` = arc length at each point), with its
/// corners rounded (see the module docs). `before` and `after` are the pieces of the
/// legs that continue this one without a stop, when there are any. The heading is
/// `None` for a path without length (then the position is its last point).
pub fn sample(
    path: &[[f64; 3]],
    cum: &[f64],
    s: f64,
    before: Option<Neighbour>,
    after: Option<Neighbour>,
) -> ([f64; 3], Option<f64>) {
    let list = pieces(path, cum);
    let Some(last) = list.last() else {
        return (path.last().copied().unwrap_or([0.0; 3]), None);
    };
    let total = last.s0 + last.len;
    let s = s.clamp(list[0].s0, total);
    let j = list
        .iter()
        .rposition(|p| p.s0 <= s)
        .unwrap_or(0)
        .min(list.len() - 1);
    let p = list[j];
    let f = ((s - p.s0) / p.len).clamp(0.0, 1.0);
    let z = p.a[2] + (p.b[2] - p.a[2]) * f;
    // Corner at the start of the piece.
    let incoming = if j > 0 {
        Some(Neighbour {
            dir: list[j - 1].dir,
            len: list[j - 1].len,
        })
    } else {
        before
    };
    if let Some(n) = incoming {
        let d = TURN_HALF.min(n.len / 2.0).min(p.len / 2.0);
        let into = s - p.s0;
        if d > 1e-9 && into < d {
            let u = 0.5 + 0.5 * into / d;
            let (pos, yaw) = corner(p.a, n.dir, p.dir, d, u, z);
            return (pos, Some(yaw));
        }
    }
    let outgoing = if j + 1 < list.len() {
        Some(Neighbour {
            dir: list[j + 1].dir,
            len: list[j + 1].len,
        })
    } else {
        after
    };
    if let Some(n) = outgoing {
        let d = TURN_HALF.min(p.len / 2.0).min(n.len / 2.0);
        let left = p.s0 + p.len - s;
        if d > 1e-9 && left < d {
            let u = 0.5 - 0.5 * left / d;
            let (pos, yaw) = corner(p.b, p.dir, n.dir, d, u, z);
            return (pos, Some(yaw));
        }
    }
    let pos = [
        p.a[0] + (p.b[0] - p.a[0]) * f,
        p.a[1] + (p.b[1] - p.a[1]) * f,
        z,
    ];
    (pos, Some(heading(p.dir)))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cum_of(path: &[[f64; 3]]) -> Vec<f64> {
        let mut c = vec![0.0];
        for w in path.windows(2) {
            let d = ((w[1][0] - w[0][0]).powi(2) + (w[1][1] - w[0][1]).powi(2)).sqrt();
            c.push(c.last().unwrap() + d);
        }
        c
    }

    fn close(a: [f64; 3], b: [f64; 3], eps: f64) -> bool {
        (0..3).all(|k| (a[k] - b[k]).abs() < eps)
    }

    fn yaw_gap(a: f64, b: f64) -> f64 {
        wrap(a - b).abs()
    }

    #[test]
    fn straight_pieces_are_unchanged() {
        let path = [[0.0, 0.0, 0.0], [10.0, 0.0, 0.0], [10.0, 10.0, 0.0]];
        let cum = cum_of(&path);
        // Away from the corner the polyline is followed exactly.
        let (p, y) = sample(&path, &cum, 3.0, None, None);
        assert!(close(p, [3.0, 0.0, 0.0], 1e-12) && y.unwrap().abs() < 1e-12);
        let (p, y) = sample(&path, &cum, 15.0, None, None);
        assert!(close(p, [10.0, 5.0, 0.0], 1e-12));
        assert!((y.unwrap() - PI / 2.0).abs() < 1e-12);
        // Ends are exact, and a collinear vertex changes nothing.
        assert!(close(
            sample(&path, &cum, 0.0, None, None).0,
            path[0],
            1e-12
        ));
        assert!(close(
            sample(&path, &cum, 20.0, None, None).0,
            path[2],
            1e-12
        ));
        let line = [[0.0, 0.0, 0.0], [4.0, 0.0, 0.0], [9.0, 0.0, 0.0]];
        let lc = cum_of(&line);
        for k in 0..=90 {
            let s = k as f64 * 0.1;
            let (p, y) = sample(&line, &lc, s, None, None);
            assert!(
                close(p, [s, 0.0, 0.0], 1e-9) && y.unwrap().abs() < 1e-9,
                "{s}"
            );
        }
    }

    #[test]
    fn position_and_heading_are_continuous_through_a_corner() {
        let path = [
            [0.0, 0.0, 0.0],
            [10.0, 0.0, 0.0],
            [10.0, 10.0, 0.0],
            [3.0, 3.0, 0.0],
        ];
        let cum = cum_of(&path);
        let total = *cum.last().unwrap();
        let n = 4000;
        let mut prev = sample(&path, &cum, 0.0, None, None);
        for k in 1..=n {
            let s = total * k as f64 / n as f64;
            let cur = sample(&path, &cum, s, None, None);
            let step = total / n as f64;
            assert!(
                close(cur.0, prev.0, 1.5 * step),
                "jump at {s}: {prev:?} {cur:?}"
            );
            // A quarter turn over 1 m of path: at most pi/2 per metre.
            assert!(
                yaw_gap(cur.1.unwrap(), prev.1.unwrap()) < 2.4 * step + 1e-9,
                "{s}"
            );
            prev = cur;
        }
        // At the vertex itself the heading is halfway and the fillet cuts the corner.
        let (p, y) = sample(&path, &cum, 10.0, None, None);
        assert!((y.unwrap() - PI / 4.0).abs() < 1e-9);
        assert!(
            p[0] < 10.0 && p[1] > 0.0 && p[0] > 9.5 && p[1] < 0.5,
            "{p:?}"
        );
        // On the fillet the distance to the centre is the radius 0.5 / tan(pi / 4).
        let r = 0.5;
        for k in 0..=10 {
            let s = 9.5 + k as f64 * 0.1;
            let (p, _) = sample(&path, &cum, s, None, None);
            let d = ((p[0] - 9.5).powi(2) + (p[1] - 0.5).powi(2)).sqrt();
            assert!((d - r).abs() < 1e-9, "{s}: {d}");
        }
    }

    #[test]
    fn a_corner_shared_with_the_next_leg_is_split_between_them() {
        let a = [[0.0, 0.0, 0.0], [5.0, 0.0, 0.0]];
        let b = [[5.0, 0.0, 0.0], [5.0, 5.0, 0.0]];
        let (ca, cb) = (cum_of(&a), cum_of(&b));
        let next = first_piece(&b, &cb);
        let prev = last_piece(&a, &ca);
        // The end of the first leg meets the start of the second.
        let end = sample(&a, &ca, 5.0, None, next);
        let start = sample(&b, &cb, 0.0, prev, None);
        assert!(close(end.0, start.0, 1e-9), "{end:?} {start:?}");
        assert!(yaw_gap(end.1.unwrap(), start.1.unwrap()) < 1e-9);
        assert!((end.1.unwrap() - PI / 4.0).abs() < 1e-9);
        // Without the neighbour the leg ends square, as before.
        let square = sample(&a, &ca, 5.0, None, None);
        assert!(close(square.0, a[1], 1e-12) && square.1.unwrap().abs() < 1e-12);
    }

    #[test]
    fn short_pieces_and_reversals_stay_continuous() {
        // A 0.4 m jog: the zone shrinks to half of the short piece.
        let path = [
            [0.0, 0.0, 0.0],
            [3.0, 0.0, 0.0],
            [3.0, 0.4, 0.0],
            [6.0, 0.4, 0.0],
        ];
        let cum = cum_of(&path);
        let total = *cum.last().unwrap();
        let mut prev = sample(&path, &cum, 0.0, None, None);
        for k in 1..=2000 {
            let s = total * k as f64 / 2000.0;
            let cur = sample(&path, &cum, s, None, None);
            assert!(close(cur.0, prev.0, 2.0 * total / 2000.0), "{s}");
            prev = cur;
        }
        // Going back the way it came: on the spot, the heading turns half way at the tip.
        let back = [[0.0, 0.0, 0.0], [4.0, 0.0, 0.0], [1.0, 0.0, 0.0]];
        let bc = cum_of(&back);
        let (p, y) = sample(&back, &bc, 4.0, None, None);
        assert!(close(p, [4.0, 0.0, 0.0], 1e-9));
        assert!((y.unwrap().abs() - PI / 2.0).abs() < 1e-9);
        let (p, _) = sample(&back, &bc, 3.75, None, None);
        assert!(close(p, [3.75, 0.0, 0.0], 1e-9));
        // A path without length has no heading.
        let dot = [[2.0, 2.0, 0.0], [2.0, 2.0, 0.0]];
        assert_eq!(sample(&dot, &cum_of(&dot), 0.0, None, None), (dot[1], None));
    }
}
