//! Certified values of the NIST Statistical Reference Datasets (StRD,
//! <https://www.itl.nist.gov/div898/strd/>), univariate summary statistics and one-way
//! ANOVA. The data files are the originals of NIST, in `tests/data/nist/`.
//!
//! Accuracy is measured by the log relative error `LRE = −log₁₀(|x − c| / |c|)` (the
//! number of correct significant digits; capped at 15 when the value is exact), the
//! measure used by NIST and by McCullough (1998), "Assessing the reliability of
//! statistical software", *The American Statistician* 52, 358-366.

use utsim_stats::descriptive::{QuantileMethod, describe};
use utsim_stats::hypothesis::one_way_anova;
use utsim_stats::lag1_autocorrelation;

fn lre(x: f64, c: f64) -> f64 {
    if x == c {
        return 15.0;
    }
    let rel = if c != 0.0 {
        ((x - c) / c).abs()
    } else {
        x.abs()
    };
    (-rel.log10()).min(15.0)
}

/// Rows of numbers from line 61 on (the data block of every StRD file).
fn rows(text: &str) -> Vec<Vec<f64>> {
    text.lines()
        .skip(60)
        .map(|l| {
            l.split_whitespace()
                .filter_map(|c| c.parse::<f64>().ok())
                .collect::<Vec<f64>>()
        })
        .filter(|r| !r.is_empty())
        .collect()
}

fn univariate(text: &str) -> Vec<f64> {
    rows(text).into_iter().map(|r| r[r.len() - 1]).collect()
}

/// (name, data, certified mean, sd, lag-1 autocorrelation, minimum LRE required).
const UNIVARIATE: [(&str, &str, f64, f64, f64, f64); 8] = [
    // Lower difficulty.
    (
        "Lew",
        include_str!("data/nist/Lew.dat"),
        -177.435000000000,
        277.332168044316,
        -0.307304800605679,
        13.0,
    ),
    (
        "Lottery",
        include_str!("data/nist/Lottery.dat"),
        518.958715596330,
        291.699727470969,
        -0.120948622967393,
        13.0,
    ),
    (
        "Mavro",
        include_str!("data/nist/Mavro.dat"),
        2.00185600000000,
        0.000429123454003053,
        0.937989183438248,
        12.0,
    ),
    (
        "Michelso",
        include_str!("data/nist/Michelso.dat"),
        299.852400000000,
        0.0790105478190518,
        0.535199668621283,
        12.0,
    ),
    (
        "NumAcc1",
        include_str!("data/nist/NumAcc1.dat"),
        10000002.0,
        1.0,
        -0.5,
        14.0,
    ),
    // Average difficulty.
    (
        "NumAcc2",
        include_str!("data/nist/NumAcc2.dat"),
        1.2,
        0.1,
        -0.999,
        12.0,
    ),
    (
        "NumAcc3",
        include_str!("data/nist/NumAcc3.dat"),
        1000000.2,
        0.1,
        -0.999,
        9.0,
    ),
    // Higher difficulty.
    (
        "NumAcc4",
        include_str!("data/nist/NumAcc4.dat"),
        10000000.2,
        0.1,
        -0.999,
        8.0,
    ),
];

#[test]
fn univariate_summary_statistics_reach_the_certified_digits() {
    for (name, text, mean, sd, r1, min_lre) in UNIVARIATE {
        let xs = univariate(text);
        let d = describe(&xs, 0.95, QuantileMethod::Weibull).unwrap();
        let (lm, ls) = (lre(d.mean, mean), lre(d.sd, sd));
        let lr = lre(lag1_autocorrelation(&xs), r1);
        println!(
            "{name}: n {} mean LRE {lm:.1}, sd LRE {ls:.1}, r1 LRE {lr:.1}",
            d.n
        );
        assert!(lm >= min_lre, "{name}: mean {} LRE {lm}", d.mean);
        assert!(ls >= min_lre, "{name}: sd {} LRE {ls}", d.sd);
        // The autocorrelation is a ratio of sums of products: a few digits fewer.
        assert!(lr >= min_lre - 4.0, "{name}: r1 LRE {lr}");
    }
}

#[test]
fn univariate_sizes_match_the_headers() {
    let sizes: Vec<usize> = UNIVARIATE
        .iter()
        .map(|(_, t, ..)| univariate(t).len())
        .collect();
    assert_eq!(sizes, [200, 218, 50, 100, 3, 1001, 1001, 1001]);
}

/// (name, data, df between, SS between, MS between, df within, SS within, MS within, F,
/// R², residual sd, minimum LRE).
type AnovaCase = (
    &'static str,
    &'static str,
    usize,
    f64,
    f64,
    usize,
    f64,
    f64,
    f64,
    f64,
    f64,
    f64,
);

const ANOVA: [AnovaCase; 5] = [
    // Lower difficulty.
    (
        "SiRstv",
        include_str!("data/nist/SiRstv.dat"),
        4,
        5.11462616000000E-02,
        1.27865654000000E-02,
        20,
        2.16636560000000E-01,
        1.08318280000000E-02,
        1.18046237440255E+00,
        1.90999039051129E-01,
        1.04076068334656E-01,
        12.0,
    ),
    (
        "SmLs01",
        include_str!("data/nist/SmLs01.dat"),
        8,
        1.68,
        0.21,
        180,
        1.8,
        0.01,
        21.0,
        4.82758620689655E-01,
        0.1,
        12.0,
    ),
    // Average difficulty.
    (
        "SmLs04",
        include_str!("data/nist/SmLs04.dat"),
        8,
        1.68,
        0.21,
        180,
        1.8,
        0.01,
        21.0,
        4.82758620689655E-01,
        0.1,
        9.0,
    ),
    (
        "AtmWtAg",
        include_str!("data/nist/AtmWtAg.dat"),
        1,
        3.63834187500000E-09,
        3.63834187500000E-09,
        46,
        1.04951729166667E-08,
        2.28155932971014E-10,
        1.59467335677930E+01,
        2.57426544538321E-01,
        1.51048314446410E-05,
        10.0,
    ),
    // Higher difficulty: 13 constant leading digits (1000000000000.x). The spacing of
    // doubles near 1e12 is 2^-13 ≈ 1.2e-4, so reading `1000000000000.4` already moves
    // the deviation 0.1 by up to 6e-5 (a relative 6e-4) before any arithmetic: about
    // 3.5 digits is all a double can hold, whatever the algorithm (the 6 digits of
    // the plan assume data that a double represents).
    (
        "SmLs07",
        include_str!("data/nist/SmLs07.dat"),
        8,
        1.68,
        0.21,
        180,
        1.8,
        0.01,
        21.0,
        4.82758620689655E-01,
        0.1,
        3.0,
    ),
];

#[test]
fn one_way_anova_reaches_the_certified_digits() {
    for (name, text, dfb, ssb, msb, dfw, ssw, msw, f, r2, sd, min_lre) in ANOVA {
        let data = rows(text);
        let k = data.iter().map(|r| r[0] as usize).max().unwrap();
        let groups: Vec<Vec<f64>> = (1..=k)
            .map(|g| {
                data.iter()
                    .filter(|r| r[0] as usize == g)
                    .map(|r| r[1])
                    .collect()
            })
            .collect();
        let refs: Vec<&[f64]> = groups.iter().map(|g| g.as_slice()).collect();
        let o = one_way_anova(&refs, 0.95).unwrap();
        let a = &o.anova;
        assert_eq!((a.df_between, a.df_within), (dfb, dfw), "{name}");
        let checks = [
            ("SS between", a.ss_between, ssb),
            ("MS between", a.ms_between, msb),
            ("SS within", a.ss_within, ssw),
            ("MS within", a.ms_within, msw),
            ("F", a.f, f),
            ("R²", o.r_squared, r2),
            ("residual sd", o.residual_sd, sd),
        ];
        for (what, x, c) in checks {
            let l = lre(x, c);
            println!("{name}: {what} {x:e} LRE {l:.1}");
            assert!(l >= min_lre, "{name}: {what} {x} against {c}, LRE {l}");
        }
    }
}
