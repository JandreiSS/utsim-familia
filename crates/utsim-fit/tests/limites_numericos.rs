//! Data near the largest f64: the fits refuse values whose mean or variance overflows, and
//! the quantile of a model whose CDF never reaches `p` ends with an infinite value instead
//! of doubling for ever (a mutated model once spent tens of minutes in the bootstrap).

use std::sync::atomic::{AtomicUsize, Ordering};

use utsim_core::rng::SimRng;
use utsim_fit::special::gamma_p;
use utsim_fit::{Family, FittedModel};

#[test]
fn fits_refuse_values_that_overflow() {
    let mut data: Vec<f64> = (1..30).map(|i| 0.2 + 0.01 * i as f64).collect();
    data.push(f64::MAX);
    for f in utsim_fit::families::all() {
        if f.is_discrete() {
            continue;
        }
        if let Ok(fit) = f.fit(&data) {
            for (name, v) in &fit.params {
                assert!(v.is_finite(), "{}: {name} = {v}", f.name());
            }
        }
    }
    let erlang = utsim_fit::families::Erlang.fit(&data);
    assert!(erlang.err().is_some_and(|e| e.contains("grandes demais")));
}

/// A model whose CDF is always 0, counting the calls.
struct Never(AtomicUsize);

impl FittedModel for Never {
    fn cdf(&self, _x: f64) -> f64 {
        self.0.fetch_add(1, Ordering::SeqCst);
        0.0
    }
    fn pdf(&self, _x: f64) -> f64 {
        0.0
    }
    fn sample(&self, _rng: &mut SimRng) -> f64 {
        0.0
    }
}

#[test]
fn the_quantile_bracket_stops_at_infinity() {
    let m = Never(AtomicUsize::new(0));
    assert_eq!(m.quantile(0.5), f64::INFINITY);
    assert!(m.0.load(Ordering::SeqCst) < 1100, "{:?}", m.0);
}

#[test]
fn incomplete_gamma_at_infinity() {
    assert_eq!(gamma_p(3.0, f64::INFINITY), 1.0);
    assert!(gamma_p(3.0, f64::NAN).is_nan());
    assert_eq!(utsim_fit::special::gamma_q(3.0, f64::INFINITY), 0.0);
}
