use super::*;

fn coded(d: &Design) -> Vec<Vec<f64>> {
    d.runs
        .iter()
        .map(|r| r.levels.iter().map(|l| f64::from(*l)).collect())
        .collect()
}

fn col(x: &[Vec<f64>], mask: u32) -> Vec<f64> {
    x.iter()
        .map(|r| {
            (0..r.len())
                .filter(|i| mask & (1 << i) != 0)
                .fold(1.0, |a, i| a * r[i])
        })
        .collect()
}

#[test]
fn full_factorials_are_balanced_and_orthogonal() {
    for k in 2..=6 {
        let d = design(k, &[], 1, 0, 1).unwrap();
        let x = coded(&d);
        assert_eq!(x.len(), 1 << k);
        let terms: Vec<u32> = (1u32..(1 << k)).collect();
        for &a in &terms {
            let ca = col(&x, a);
            assert_eq!(ca.iter().sum::<f64>(), 0.0, "k={k} {}", term_name(a));
            for &b in terms.iter().filter(|b| **b > a) {
                let dot: f64 = ca.iter().zip(col(&x, b)).map(|(p, q)| p * q).sum();
                assert_eq!(dot, 0.0, "k={k} {} {}", term_name(a), term_name(b));
            }
        }
    }
}

#[test]
fn fractions_alias_as_the_defining_relation_says() {
    let expected = [
        ((3, 1), 3),
        ((4, 1), 4),
        ((5, 1), 5),
        ((5, 2), 3),
        ((6, 1), 6),
        ((6, 2), 4),
        ((6, 3), 3),
        ((7, 1), 7),
        ((7, 2), 4),
        ((7, 3), 4),
        ((7, 4), 3),
        ((8, 2), 5),
        ((8, 3), 4),
        ((8, 4), 4),
    ];
    for ((k, p), res) in expected {
        let g = standard_generators(k, p).unwrap();
        let d = design(k, &g, 1, 0, 9).unwrap();
        let x = coded(&d);
        assert_eq!(x.len(), 1 << (k - p));
        let words = d.words();
        assert_eq!(words.len(), (1 << p) - 1);
        assert_eq!(d.resolution(), Some(res), "2^({k}-{p})");
        // The words are exactly the terms whose column is all +1.
        let mut ones: Vec<u32> = (1u32..(1 << k))
            .filter(|m| col(&x, *m).iter().all(|v| *v == 1.0))
            .collect();
        ones.sort_by_key(|w| (order(*w), *w));
        assert_eq!(ones, words, "2^({k}-{p})");
        // Every term has the column of each of its aliases.
        for m in 1u32..(1 << k) {
            let c = col(&x, m);
            for a in aliases(m, &words) {
                if a != 0 {
                    assert_eq!(c, col(&x, a), "{} {}", term_name(m), term_name(a));
                }
            }
        }
    }
    assert!(standard_generators(4, 0).is_none());
    assert_eq!(term_name(0b1011), "ABD");
}

#[test]
fn run_order_comes_from_the_seed() {
    let a = design(3, &[], 2, 3, 77).unwrap();
    let b = design(3, &[], 2, 3, 77).unwrap();
    let c = design(3, &[], 2, 3, 78).unwrap();
    assert_eq!(a, b);
    assert_ne!(a.runs, c.runs);
    assert_eq!(a.runs.len(), 2 * 8 + 3);
    let mut std: Vec<usize> = a.runs.iter().map(|r| r.std_order).collect();
    std.sort();
    assert_eq!(std, (1..=19).collect::<Vec<_>>());
    assert!(a.runs.iter().enumerate().all(|(i, r)| r.run_order == i + 1));
    assert_eq!(a.runs.iter().filter(|r| r.center).count(), 3);
    assert!(design(3, &[Generator { factor: 2, word: 1 }], 1, 0, 1).is_err());
    assert!(design(1, &[], 1, 0, 1).is_err());
}

#[test]
fn replicated_two_by_two_by_hand() {
    // (−,−): 10, 12; (+,−): 20, 22; (−,+): 14, 16; (+,+): 30, 28.
    // A = 25 − 13 = 12; B = 22 − 16 = 6; AB = 20 − 18 = 2; grand mean 19.
    // SS total 376; SS A = 8·6² = 288, B = 72, AB = 8; error 8 on 4 df.
    let x = vec![
        vec![-1.0, -1.0],
        vec![-1.0, -1.0],
        vec![1.0, -1.0],
        vec![1.0, -1.0],
        vec![-1.0, 1.0],
        vec![-1.0, 1.0],
        vec![1.0, 1.0],
        vec![1.0, 1.0],
    ];
    let y = [10.0, 12.0, 20.0, 22.0, 14.0, 16.0, 30.0, 28.0];
    let a = analyze(&x, &y, 2, 0.95).unwrap();
    assert_eq!(a.error, ErrorKind::Residual);
    let e = |m: u32| a.effect(m).unwrap();
    assert!((e(1).effect - 12.0).abs() < 1e-12);
    assert!((e(2).effect - 6.0).abs() < 1e-12);
    assert!((e(3).effect - 2.0).abs() < 1e-12);
    assert!((e(1).ss - 288.0).abs() < 1e-9);
    let err = a.anova.iter().find(|l| l.source == "erro").unwrap();
    assert!((err.ss - 8.0).abs() < 1e-9 && err.df == 4.0);
    let la = a.anova.iter().find(|l| l.source == "A").unwrap();
    assert!((la.f.unwrap() - 144.0).abs() < 1e-9);
    assert!((la.p_value.unwrap() - f_sf(144.0, 1.0, 4.0)).abs() < 1e-15);
    // t² = F for a one-degree term.
    assert!((e(1).t.unwrap().powi(2) - 144.0).abs() < 1e-9);
    assert!((a.r2 - 368.0 / 376.0).abs() < 1e-12);
    assert_eq!(main_effect_means(&x, &y, 0), (13.0, 25.0));
    assert_eq!(
        interaction_means(&x, &y, 0, 1),
        [[11.0, 21.0], [15.0, 29.0]]
    );
}

#[test]
fn lenth_by_hand_and_unreplicated_fraction() {
    // |e| sorted 0,3 0,5 0,8 1 1,2 2 10: s0 = 1,5; kept below 3,75: median 0,9; PSE 1,35.
    let l = lenth(&[10.0, -1.0, 0.5, 2.0, -0.3, 0.8, 1.2], 0.95).unwrap();
    assert!((l.pse - 1.35).abs() < 1e-12);
    assert!((l.df - 7.0 / 3.0).abs() < 1e-12);
    // Half of a 2³ with C = AB: AB has the column of C and is left out.
    let g = standard_generators(3, 1).unwrap();
    let d = design(3, &g, 1, 0, 5).unwrap();
    let x = coded(&d);
    let y: Vec<f64> = x.iter().map(|r| 50.0 + 4.0 * r[0] - 1.0 * r[2]).collect();
    let a = analyze(&x, &y, 2, 0.95).unwrap();
    assert_eq!(a.effects.len(), 3);
    assert!(a.confounded.contains(&(0b011, 0b100)));
    assert!((a.effect(1).unwrap().effect - 8.0).abs() < 1e-12);
    assert!(a.effect(4).unwrap().aliases.contains(&0b011));
    assert_eq!(a.error, ErrorKind::Lenth);
}

#[test]
fn center_points_give_the_curvature() {
    let d = design(2, &[], 1, 4, 3).unwrap();
    let x = coded(&d);
    // A plane at the corners (mean 10) and 13 at the center: curvature −3.
    let y: Vec<f64> = d
        .runs
        .iter()
        .enumerate()
        .map(|(i, r)| {
            if r.center {
                13.0 + 0.1 * (i % 2) as f64
            } else {
                10.0 + f64::from(r.levels[0])
            }
        })
        .collect();
    let a = analyze(&x, &y, 2, 0.95).unwrap();
    assert_eq!((a.factorial_runs, a.center_runs), (4, 4));
    let centers: Vec<f64> = d
        .runs
        .iter()
        .zip(&y)
        .filter(|(r, _)| r.center)
        .map(|(_, v)| *v)
        .collect();
    let expected = 10.0 - centers.iter().sum::<f64>() / 4.0;
    assert!((a.curvature.unwrap() - expected).abs() < 1e-9 && expected < -2.9);
    let c = a.anova.iter().find(|l| l.source == "curvatura").unwrap();
    assert!(c.p_value.unwrap() < 0.001);
    assert!(analyze(&[vec![0.5, 1.0]], &[1.0], 2, 0.95).is_err());
    assert_eq!(code(&[2.0, 4.0, 3.0]), Some(vec![-1.0, 1.0, 0.0]));
    assert_eq!(code(&[2.0, 2.0]), None);
}
