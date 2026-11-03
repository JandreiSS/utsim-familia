//! Textbook networks with answers worked out by hand: CPM dates and floats, classic PERT,
//! and Monte Carlo (convergence, criticality, determinism).

use utsim_core::Dist;
use utsim_pert::monte_carlo::{self, McOptions};
use utsim_pert::pert::{self, Moments};
use utsim_pert::{ActivityNetwork, schedule_means};

fn c(x: f64) -> Dist {
    Dist::Const(x)
}

fn tri(a: f64, m: f64, b: f64) -> Dist {
    Dist::Triangular {
        min: a,
        mode: m,
        max: b,
    }
}

/// A(3); B(4) after A; C(2) after A; D(5) after B; E(1) after C; F(2) after C;
/// G(4) after D, E; H(3) after F, G.
fn eight() -> ActivityNetwork {
    ActivityNetwork::from_table(&[
        ("A", c(3.0), &[]),
        ("B", c(4.0), &["A"]),
        ("C", c(2.0), &["A"]),
        ("D", c(5.0), &["B"]),
        ("E", c(1.0), &["C"]),
        ("F", c(2.0), &["C"]),
        ("G", c(4.0), &["D", "E"]),
        ("H", c(3.0), &["F", "G"]),
    ])
    .unwrap()
}

#[test]
fn cpm_dates_and_floats_by_hand() {
    let net = eight();
    let s = schedule_means(&net);
    assert_eq!(s.project, 19.0);
    //                 A    B    C    D     E    F    G     H
    let es = [0.0, 3.0, 3.0, 7.0, 5.0, 5.0, 12.0, 16.0];
    let ef = [3.0, 7.0, 5.0, 12.0, 6.0, 7.0, 16.0, 19.0];
    let ls = [0.0, 3.0, 9.0, 7.0, 11.0, 14.0, 12.0, 16.0];
    let lf = [3.0, 7.0, 11.0, 12.0, 12.0, 16.0, 16.0, 19.0];
    let tf = [0.0, 0.0, 6.0, 0.0, 6.0, 9.0, 0.0, 0.0];
    let ff = [0.0, 0.0, 0.0, 0.0, 6.0, 9.0, 0.0, 0.0];
    for v in 0..8 {
        let name = &net.activities[v].name;
        assert_eq!(s.es[v], es[v], "ES de {name}");
        assert_eq!(s.ef[v], ef[v], "EF de {name}");
        assert_eq!(s.ls[v], ls[v], "LS de {name}");
        assert_eq!(s.lf[v], lf[v], "LF de {name}");
        assert_eq!(s.total_float[v], tf[v], "folga total de {name}");
        assert_eq!(s.free_float[v], ff[v], "folga livre de {name}");
    }
    assert_eq!(s.critical_paths.len(), 1);
    let names: Vec<&str> = s.critical_paths[0]
        .iter()
        .filter(|&&v| !net.activities[v].is_milestone())
        .map(|&v| net.activities[v].name.as_str())
        .collect();
    assert_eq!(names, ["A", "B", "D", "G", "H"]);
}

#[test]
fn tied_critical_paths_are_all_listed() {
    let net = ActivityNetwork::from_table(&[
        ("A", c(2.0), &[]),
        ("B", c(3.0), &["A"]),
        ("C", c(3.0), &["A"]),
        ("D", c(1.0), &["B", "C"]),
    ])
    .unwrap();
    let s = schedule_means(&net);
    assert_eq!(s.project, 6.0);
    assert_eq!(s.critical_paths.len(), 2);
    assert!(s.critical.iter().all(|&x| x));
}

#[test]
fn cycles_are_refused_in_hand_built_networks() {
    let r = ActivityNetwork::from_table(&[("A", c(1.0), &["B"]), ("B", c(1.0), &["A"])]);
    assert!(r.is_err());
}

/// A(2,4,6); B(3,5,13) after A; C(1,2,3) after A; D(4,6,8) after B and C.
fn pert_book() -> ActivityNetwork {
    ActivityNetwork::from_table(&[
        ("A", tri(2.0, 4.0, 6.0), &[]),
        ("B", tri(3.0, 5.0, 13.0), &["A"]),
        ("C", tri(1.0, 2.0, 3.0), &["A"]),
        ("D", tri(4.0, 6.0, 8.0), &["B", "C"]),
    ])
    .unwrap()
}

#[test]
fn classic_pert_by_hand() {
    let net = pert_book();
    let r = pert::analyze(&net, Moments::Classic, Some(18.0));
    // te: A 4, B 6, C 2, D 6. Critical path A-B-D, 16.
    assert!((r.means[0] - 4.0).abs() < 1e-12);
    assert!((r.means[1] - 6.0).abs() < 1e-12);
    assert!((r.mean - 16.0).abs() < 1e-12);
    // σ² = (4/6)² + (10/6)² + (4/6)² = 132/36.
    assert!((r.variance - 132.0 / 36.0).abs() < 1e-12);
    // P(T ≤ 18) = Φ(2 / 1.914854) = Φ(1.04447) ≈ 0.8519.
    let p = r.probability.unwrap();
    assert!((p - 0.8519).abs() < 5e-4, "{p}");
    // Exact moments of the triangulars: means 4, 7, 2, 6 -> 17.
    let e = pert::analyze(&net, Moments::Exact, None);
    assert!((e.mean - 17.0).abs() < 1e-12);
}

#[test]
fn monte_carlo_series_converges_to_the_sum_of_means() {
    let net = ActivityNetwork::from_table(&[
        ("A", Dist::Exp { mean: 2.0 }, &[]),
        ("B", Dist::Uniform { min: 1.0, max: 3.0 }, &["A"]),
        ("C", tri(1.0, 2.0, 6.0), &["B"]),
    ])
    .unwrap();
    let mc = monte_carlo::run(&net, &McOptions::default());
    assert_eq!(mc.samples, 10_000);
    assert!(
        (mc.total.mean - 7.0).abs() < 3.0 * mc.total.half_width,
        "{:?}",
        mc.total
    );
    // A series: every activity is always critical, one path.
    assert!(mc.criticality.iter().all(|&x| x == 1.0));
    assert_eq!(mc.paths.len(), 1);
}

#[test]
fn equal_parallel_branches_split_the_criticality() {
    let net = ActivityNetwork::from_table(&[
        ("X", Dist::Exp { mean: 1.0 }, &[]),
        ("Y", Dist::Exp { mean: 1.0 }, &[]),
    ])
    .unwrap();
    let mc = monte_carlo::run(
        &net,
        &McOptions {
            deadline: Some(1.0),
            ..McOptions::default()
        },
    );
    let (x, y) = (mc.criticality[0], mc.criticality[1]);
    assert!((x - 0.5).abs() < 3.0 * mc.criticality_half_width[0], "{x}");
    assert!((x + y - 1.0).abs() < 1e-12);
    // T = max of two Exp(1): P(T ≤ 1) = (1 - e^-1)² ≈ 0.3996.
    let d = mc.deadline.unwrap();
    assert!(d.low <= 0.3996 && 0.3996 <= d.high, "{d:?}");
    // Classic PERT ignores the merge: its mean is below the Monte Carlo one.
    let classic = pert::analyze(&net, Moments::Exact, None);
    assert!(classic.mean < mc.total.mean);
    assert!(!classic.warnings.is_empty());
    // The sensitivity sees both branches.
    assert_eq!(mc.sensitivity.len(), 2);
}

#[test]
fn monte_carlo_is_deterministic_on_any_number_of_threads() {
    let net = pert_book();
    let opts = McOptions {
        samples: 4321,
        seed: 77,
        deadline: Some(17.0),
        bins: 10,
    };
    let run_on = |threads: usize| {
        let pool = rayon::ThreadPoolBuilder::new()
            .num_threads(threads)
            .build()
            .unwrap();
        pool.install(|| format!("{:?}", monte_carlo::run(&net, &opts)))
    };
    let one = run_on(1);
    assert_eq!(one, run_on(4));
    assert_eq!(one, run_on(3));
    let other = monte_carlo::run(
        &net,
        &McOptions {
            seed: 78,
            ..opts.clone()
        },
    );
    assert_ne!(one, format!("{other:?}"));
}
