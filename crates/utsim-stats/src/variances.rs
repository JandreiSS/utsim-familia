//! Equality of variances of several groups (`docs/plano_lean_seis_sigma.md`, 2.3, P2):
//! Levene's test in the Brown-Forsythe form (absolute deviations from the group median,
//! robust to non-normal data) and Bartlett's test (assumes normal data, more powerful
//! when it holds).

use crate::special::{chi2_sf, f_sf};

/// The result of a test of equal variances.
#[derive(Debug, Clone, PartialEq)]
pub struct Variances {
    pub statistic: f64,
    /// Degrees of freedom (one for χ², two for F).
    pub df: (f64, f64),
    pub p_value: f64,
    /// Standard deviation of each group.
    pub sds: Vec<f64>,
}

fn mean(v: &[f64]) -> f64 {
    v.iter().sum::<f64>() / v.len() as f64
}

fn median(v: &[f64]) -> f64 {
    let mut s = v.to_vec();
    s.sort_by(|a, b| a.total_cmp(b));
    let n = s.len();
    if n % 2 == 1 {
        s[n / 2]
    } else {
        0.5 * (s[n / 2 - 1] + s[n / 2])
    }
}

fn var(v: &[f64]) -> f64 {
    let m = mean(v);
    v.iter().map(|x| (x - m).powi(2)).sum::<f64>() / (v.len() as f64 - 1.0)
}

fn usable(groups: &[&[f64]]) -> bool {
    groups.len() >= 2 && groups.iter().all(|g| g.len() >= 2)
}

/// Levene's test with the group medians (Brown-Forsythe): the one-way ANOVA of
/// `|x − median of its group|`. `None` with fewer than two groups of two values.
pub fn levene(groups: &[&[f64]]) -> Option<Variances> {
    if !usable(groups) {
        return None;
    }
    let z: Vec<Vec<f64>> = groups
        .iter()
        .map(|g| {
            let m = median(g);
            g.iter().map(|x| (x - m).abs()).collect()
        })
        .collect();
    let k = z.len() as f64;
    let n: usize = z.iter().map(Vec::len).sum();
    let grand = z.iter().flatten().sum::<f64>() / n as f64;
    let between: f64 = z
        .iter()
        .map(|g| g.len() as f64 * (mean(g) - grand).powi(2))
        .sum();
    let within: f64 = z
        .iter()
        .map(|g| {
            let m = mean(g);
            g.iter().map(|x| (x - m).powi(2)).sum::<f64>()
        })
        .sum();
    let d1 = k - 1.0;
    let d2 = n as f64 - k;
    let w = if within > 0.0 {
        (between / d1) / (within / d2)
    } else if between > 0.0 {
        f64::INFINITY
    } else {
        0.0
    };
    Some(Variances {
        statistic: w,
        df: (d1, d2),
        p_value: f_sf(w, d1, d2),
        sds: groups.iter().map(|g| var(g).sqrt()).collect(),
    })
}

/// Bartlett's test: `T = ((N−k) ln s²ₚ − Σ(nᵢ−1) ln s²ᵢ) / C`, χ² with `k − 1` degrees of
/// freedom. `None` with fewer than two groups of two values or a group without spread.
pub fn bartlett(groups: &[&[f64]]) -> Option<Variances> {
    if !usable(groups) {
        return None;
    }
    let k = groups.len() as f64;
    let vars: Vec<f64> = groups.iter().map(|g| var(g)).collect();
    if vars.iter().any(|v| v.is_nan() || *v <= 0.0) {
        return None;
    }
    let n: f64 = groups.iter().map(|g| g.len() as f64).sum();
    let sp2 = groups
        .iter()
        .zip(&vars)
        .map(|(g, v)| (g.len() as f64 - 1.0) * v)
        .sum::<f64>()
        / (n - k);
    let num = (n - k) * sp2.ln()
        - groups
            .iter()
            .zip(&vars)
            .map(|(g, v)| (g.len() as f64 - 1.0) * v.ln())
            .sum::<f64>();
    let c = 1.0
        + (groups
            .iter()
            .map(|g| 1.0 / (g.len() as f64 - 1.0))
            .sum::<f64>()
            - 1.0 / (n - k))
            / (3.0 * (k - 1.0));
    let t = num / c;
    Some(Variances {
        statistic: t,
        df: (k - 1.0, 0.0),
        p_value: chi2_sf(t, k - 1.0),
        sds: vars.iter().map(|v| v.sqrt()).collect(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn levene_by_hand() {
        // Medians 2 and 6; |dev|: (1,0,1) and (2,0,2); means 2/3 and 4/3, grand 1;
        // between = 3·(1/9)·2 = 2/3; within = (1/9+4/9+1/9) + (4/9+16/9+4/9) = 6/9 + 24/9 = 30/9.
        let a = [1.0, 2.0, 3.0];
        let b = [4.0, 6.0, 8.0];
        let r = levene(&[&a, &b]).unwrap();
        let w = (2.0 / 3.0) / ((30.0 / 9.0) / 4.0);
        assert!((r.statistic - w).abs() < 1e-12);
        assert_eq!(r.df, (1.0, 4.0));
        assert!((r.p_value - f_sf(w, 1.0, 4.0)).abs() < 1e-15);
        assert!(levene(&[&a]).is_none());
    }

    #[test]
    fn bartlett_by_hand_and_equal_spread() {
        // Variances 1 and 4, n = 3 each: sp² = 2.5; num = 4 ln 2.5 − 2 ln 1 − 2 ln 4;
        // C = 1 + (1/2 + 1/2 − 1/4)/3 = 1.25.
        let a = [1.0, 2.0, 3.0];
        let b = [4.0, 6.0, 8.0];
        let r = bartlett(&[&a, &b]).unwrap();
        let t = (4.0 * 2.5f64.ln() - 2.0 * 4.0f64.ln()) / 1.25;
        assert!((r.statistic - t).abs() < 1e-12);
        assert!((r.p_value - chi2_sf(t, 1.0)).abs() < 1e-15);
        // Same spread: the statistic is zero and p is one.
        let c = [11.0, 12.0, 13.0];
        let same = bartlett(&[&a, &c]).unwrap();
        assert!(same.statistic.abs() < 1e-12 && (same.p_value - 1.0).abs() < 1e-9);
        assert!(bartlett(&[&a, &[5.0, 5.0]]).is_none());
    }
}
