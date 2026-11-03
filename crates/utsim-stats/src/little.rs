//! Little's law `L = λ·W` as a consistency check of a set of replications.
//!
//! Over a window of length `T`, with `L` the time-average number present, `λ` the
//! departures per time unit and `W` the mean time spent by those departures, the law
//! holds up to the items present at the two ends of the window. A clear gap usually
//! means a warm-up that is too short (the window starts far from steady state), items
//! stuck at the end of the run, or items counted on one side only (e.g. in transit).

use serde::Serialize;
use utsim_core::stats::Summary;

use crate::summary;

/// Relative gap `|L - λW| / max(L, λW)` below which the law counts as closed even when the
/// interval of the difference excludes zero (many replications make tiny end effects
/// "significant").
pub const DEFAULT_TOLERANCE: f64 = 0.02;

/// Little's law over the replications of one object (or of the whole system).
#[derive(Clone, Debug, Serialize)]
pub struct LittleCheck {
    /// Time-average number present.
    pub l: Summary,
    /// Departures per time unit.
    pub lambda: Summary,
    /// Mean time spent.
    pub w: Summary,
    /// `λ·W`, computed per replication.
    pub lambda_w: Summary,
    /// `L - λ·W`, paired per replication.
    pub difference: Summary,
    /// `|mean L - mean λW| / max(mean L, mean λW)` (0 when both are 0).
    pub relative: f64,
    /// Whether the law closes: the interval of the difference contains zero, or the
    /// relative gap is within the tolerance.
    pub holds: bool,
}

/// Checks `L = λ·W` from one `(L, λ, W)` triple per replication.
pub fn check(l: &[f64], lambda: &[f64], w: &[f64], level: f64, tolerance: f64) -> LittleCheck {
    let lw: Vec<f64> = lambda.iter().zip(w).map(|(a, b)| a * b).collect();
    let d: Vec<f64> = l.iter().zip(&lw).map(|(a, b)| a - b).collect();
    let l_sum = summary(l, level);
    let lw_sum = summary(&lw, level);
    let difference = summary(&d, level);
    let scale = l_sum.mean.abs().max(lw_sum.mean.abs());
    let relative = if scale > 0.0 {
        (l_sum.mean - lw_sum.mean).abs() / scale
    } else {
        0.0
    };
    let inside = !difference.half_width.is_nan() && difference.mean.abs() <= difference.half_width;
    LittleCheck {
        l: l_sum,
        lambda: summary(lambda, level),
        w: summary(w, level),
        lambda_w: lw_sum,
        difference,
        relative,
        holds: inside || relative <= tolerance,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exact_triples_close_the_law() {
        // L = 2, λ = 0.5, W = 4 in every replication.
        let c = check(
            &[2.0, 2.0],
            &[0.5, 0.5],
            &[4.0, 4.0],
            0.95,
            DEFAULT_TOLERANCE,
        );
        assert_eq!(c.relative, 0.0);
        assert_eq!(c.lambda_w.mean, 2.0);
        assert!(c.holds);
    }

    #[test]
    fn a_systematic_gap_is_flagged() {
        // λW is 10% below L in every replication, with little noise.
        let l = [10.0, 10.2, 9.8, 10.1];
        let lambda = [1.0; 4];
        let w = [9.0, 9.2, 8.8, 9.1];
        let c = check(&l, &lambda, &w, 0.95, DEFAULT_TOLERANCE);
        assert!((c.relative - 1.0 / 10.025).abs() < 1e-12);
        assert!((c.difference.mean - 1.0).abs() < 1e-12);
        assert!(!c.holds);
        // A looser tolerance accepts it.
        assert!(check(&l, &lambda, &w, 0.95, 0.15).holds);
    }

    #[test]
    fn noise_alone_does_not_fail_the_check() {
        let l = [10.0, 12.0, 8.0, 11.0];
        let lambda = [1.0; 4];
        let w = [10.5, 11.0, 9.0, 10.0];
        let c = check(&l, &lambda, &w, 0.95, 0.0);
        assert!(c.holds, "{c:?}");
    }
}
