//! Type I error rates by simulation (plan, section 5.4): under `H₀`, each test rejects
//! at 5% in about 5% of the samples. With 4000 samples the standard error of the rate
//! is 0.34 percentage points, so the band ±1.4 p.p. holds with overwhelming probability
//! for a correct test (and the seeds are fixed, so the run is reproducible).

use rand::{Rng, SeedableRng};
use rand_pcg::Pcg64;
use utsim_stats::hypothesis::{
    Alternative, ProportionMethod, chi_square_independence, one_proportion, t_one_sample,
    t_two_sample,
};
use utsim_stats::nonparametric::{RankMethod, mann_whitney, wilcoxon_signed_rank};

const REPS: usize = 4000;

fn normal(rng: &mut Pcg64) -> f64 {
    // Box-Muller.
    let u: f64 = rng.random::<f64>().max(1e-300);
    let v: f64 = rng.random();
    (-2.0 * u.ln()).sqrt() * (2.0 * std::f64::consts::PI * v).cos()
}

fn sample(rng: &mut Pcg64, n: usize, mu: f64, sd: f64) -> Vec<f64> {
    (0..n).map(|_| mu + sd * normal(rng)).collect()
}

fn rate(mut reject: impl FnMut(&mut Pcg64) -> bool, seed: u64) -> f64 {
    let mut rng = Pcg64::seed_from_u64(seed);
    (0..REPS).filter(|_| reject(&mut rng)).count() as f64 / REPS as f64
}

fn near_alpha(r: f64, what: &str) {
    assert!((r - 0.05).abs() < 0.014, "{what}: rejection rate {r}");
}

#[test]
fn t_tests_hold_their_level() {
    let r = rate(
        |g| {
            let x = sample(g, 8, 10.0, 2.0);
            t_one_sample(&x, 10.0, 0.95, Alternative::TwoSided)
                .unwrap()
                .p_value
                < 0.05
        },
        1,
    );
    near_alpha(r, "one-sample t");
    // Welch with very different variances and sizes keeps the level.
    let r = rate(
        |g| {
            let x = sample(g, 6, 0.0, 1.0);
            let y = sample(g, 20, 0.0, 4.0);
            t_two_sample(&x, &y, 0.0, false, 0.95, Alternative::TwoSided)
                .unwrap()
                .p_value
                < 0.05
        },
        2,
    );
    near_alpha(r, "Welch t");
    let r = rate(
        |g| {
            let x = sample(g, 10, 5.0, 1.0);
            let y = sample(g, 12, 5.0, 1.0);
            t_two_sample(&x, &y, 0.0, true, 0.95, Alternative::Greater)
                .unwrap()
                .p_value
                < 0.05
        },
        3,
    );
    near_alpha(r, "pooled t, one-sided");
}

#[test]
fn rank_tests_hold_their_level() {
    // Exact tests are conservative (discrete distribution): the rate is at most 5%.
    let r = rate(
        |g| {
            let x = sample(g, 9, 0.0, 1.0);
            let y = sample(g, 11, 0.0, 1.0);
            mann_whitney(&x, &y, RankMethod::Auto, Alternative::TwoSided)
                .unwrap()
                .p_value
                < 0.05
        },
        4,
    );
    assert!(r <= 0.05 + 0.014 && r > 0.03, "Mann-Whitney exact: {r}");
    let r = rate(
        |g| {
            let x = sample(g, 15, 3.0, 1.0);
            wilcoxon_signed_rank(&x, 3.0, RankMethod::Auto, Alternative::TwoSided)
                .unwrap()
                .p_value
                < 0.05
        },
        5,
    );
    assert!(r <= 0.05 + 0.014 && r > 0.03, "Wilcoxon exact: {r}");
    let r = rate(
        |g| {
            let x = sample(g, 30, 0.0, 1.0);
            let y = sample(g, 30, 0.0, 1.0);
            mann_whitney(&x, &y, RankMethod::Normal, Alternative::TwoSided)
                .unwrap()
                .p_value
                < 0.05
        },
        6,
    );
    near_alpha(r, "Mann-Whitney normal");
}

#[test]
fn categorical_tests_hold_their_level() {
    // Exact binomial: conservative.
    let r = rate(
        |g| {
            let x = (0..40).filter(|_| g.random::<f64>() < 0.3).count() as u64;
            one_proportion(
                x,
                40,
                0.3,
                ProportionMethod::Exact,
                0.95,
                Alternative::TwoSided,
            )
            .unwrap()
            .p_value
                < 0.05
        },
        7,
    );
    assert!(r <= 0.05 + 0.014, "exact binomial: {r}");
    // χ² of independence in a 2 × 3 table with 300 observations.
    let r = rate(
        |g| {
            let mut t = vec![vec![0.0; 3]; 2];
            for _ in 0..300 {
                let i = usize::from(g.random::<f64>() < 0.4);
                let u: f64 = g.random();
                let j = if u < 0.2 {
                    0
                } else if u < 0.6 {
                    1
                } else {
                    2
                };
                t[i][j] += 1.0;
            }
            chi_square_independence(&t).unwrap().p_value < 0.05
        },
        8,
    );
    near_alpha(r, "χ² independence");
}
