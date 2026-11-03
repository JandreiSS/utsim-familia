//! Correlation between metrics across replications: Pearson (linear) and Spearman
//! (ranks, monotone), with a two-sided p-value and the matrix of a set of metrics.
//!
//! Questions like "does a long queue at station 2 go with a lower throughput?" are
//! answered by the correlation of the two metrics over the replications (each
//! replication is one independent observation of the pair).

use serde::Serialize;

use crate::special::t_cdf;

/// Pearson's correlation coefficient of `x` and `y` (paired by position; the shorter
/// length is used). NaN with fewer than two pairs or when either side is constant.
pub fn pearson(x: &[f64], y: &[f64]) -> f64 {
    let n = x.len().min(y.len());
    if n < 2 {
        return f64::NAN;
    }
    let (x, y) = (&x[..n], &y[..n]);
    let mx = x.iter().sum::<f64>() / n as f64;
    let my = y.iter().sum::<f64>() / n as f64;
    let (mut sxy, mut sxx, mut syy) = (0.0, 0.0, 0.0);
    for (a, b) in x.iter().zip(y) {
        let (dx, dy) = (a - mx, b - my);
        sxy += dx * dy;
        sxx += dx * dx;
        syy += dy * dy;
    }
    if sxx <= 0.0 || syy <= 0.0 {
        return f64::NAN;
    }
    (sxy / (sxx * syy).sqrt()).clamp(-1.0, 1.0)
}

/// Ranks from 1 to `n`, ties getting the mean of their ranks.
pub fn ranks(x: &[f64]) -> Vec<f64> {
    let mut idx: Vec<usize> = (0..x.len()).collect();
    idx.sort_by(|&a, &b| x[a].total_cmp(&x[b]));
    let mut r = vec![0.0; x.len()];
    let mut i = 0;
    while i < idx.len() {
        let mut j = i;
        while j + 1 < idx.len() && x[idx[j + 1]] == x[idx[i]] {
            j += 1;
        }
        let mean_rank = (i + j) as f64 / 2.0 + 1.0;
        for &k in &idx[i..=j] {
            r[k] = mean_rank;
        }
        i = j + 1;
    }
    r
}

/// Spearman's rank correlation: Pearson's coefficient of the ranks (ties averaged).
pub fn spearman(x: &[f64], y: &[f64]) -> f64 {
    let n = x.len().min(y.len());
    pearson(&ranks(&x[..n]), &ranks(&y[..n]))
}

/// Two-sided p-value of "no correlation" for a coefficient `r` of `n` pairs, by the
/// t statistic `r √((n - 2) / (1 - r²))` with `n - 2` degrees of freedom (exact for
/// Pearson under normality; the usual large-sample approximation for Spearman).
pub fn p_value(r: f64, n: usize) -> f64 {
    if r.is_nan() || n < 3 {
        return f64::NAN;
    }
    if r.abs() >= 1.0 {
        return 0.0;
    }
    let df = (n - 2) as f64;
    let t = r * (df / (1.0 - r * r)).sqrt();
    (2.0 * (1.0 - t_cdf(t.abs(), df))).clamp(0.0, 1.0)
}

/// Correlations of every pair of a set of metrics.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct CorrelationMatrix {
    pub names: Vec<String>,
    /// Pairs used (the shortest series).
    pub n: usize,
    /// `pearson[i][j]`, symmetric with 1 on the diagonal (NaN for a constant metric).
    pub pearson: Vec<Vec<f64>>,
    pub spearman: Vec<Vec<f64>>,
    /// Two-sided p-values of the Pearson and Spearman coefficients.
    pub p_pearson: Vec<Vec<f64>>,
    pub p_spearman: Vec<Vec<f64>>,
}

/// One pair of metrics of a [`CorrelationMatrix`], for ranking the strongest.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct CorrelationPair {
    pub a: String,
    pub b: String,
    pub pearson: f64,
    pub spearman: f64,
    pub p_pearson: f64,
    pub p_spearman: f64,
}

impl CorrelationMatrix {
    /// The matrix of `series` (`name`, values per replication).
    pub fn build(series: &[(String, Vec<f64>)]) -> CorrelationMatrix {
        let k = series.len();
        let n = series.iter().map(|(_, v)| v.len()).min().unwrap_or(0);
        let mut pe = vec![vec![f64::NAN; k]; k];
        let mut sp = pe.clone();
        let mut ppe = pe.clone();
        let mut psp = pe.clone();
        let ranked: Vec<Vec<f64>> = series.iter().map(|(_, v)| ranks(&v[..n])).collect();
        for i in 0..k {
            for j in i..k {
                let (x, y) = (&series[i].1[..n], &series[j].1[..n]);
                let (r, s) = (pearson(x, y), pearson(&ranked[i], &ranked[j]));
                let (r, s) = if i == j && !r.is_nan() {
                    (1.0, 1.0)
                } else {
                    (r, s)
                };
                for (m, v) in [
                    (&mut pe, r),
                    (&mut sp, s),
                    (&mut ppe, p_value(r, n)),
                    (&mut psp, p_value(s, n)),
                ] {
                    m[i][j] = v;
                    m[j][i] = v;
                }
            }
        }
        CorrelationMatrix {
            names: series.iter().map(|(s, _)| s.clone()).collect(),
            n,
            pearson: pe,
            spearman: sp,
            p_pearson: ppe,
            p_spearman: psp,
        }
    }

    /// Every pair `i < j` with a defined coefficient, strongest (largest |Pearson|,
    /// then |Spearman|) first.
    pub fn strongest(&self) -> Vec<CorrelationPair> {
        let k = self.names.len();
        let mut out: Vec<CorrelationPair> = (0..k)
            .flat_map(|i| (i + 1..k).map(move |j| (i, j)))
            .filter(|&(i, j)| !self.pearson[i][j].is_nan() || !self.spearman[i][j].is_nan())
            .map(|(i, j)| CorrelationPair {
                a: self.names[i].clone(),
                b: self.names[j].clone(),
                pearson: self.pearson[i][j],
                spearman: self.spearman[i][j],
                p_pearson: self.p_pearson[i][j],
                p_spearman: self.p_spearman[i][j],
            })
            .collect();
        let key = |p: &CorrelationPair| {
            let a = if p.pearson.is_nan() {
                0.0
            } else {
                p.pearson.abs()
            };
            let b = if p.spearman.is_nan() {
                0.0
            } else {
                p.spearman.abs()
            };
            a.max(b)
        };
        out.sort_by(|x, y| key(y).total_cmp(&key(x)));
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(a: f64, b: f64, tol: f64) {
        assert!((a - b).abs() <= tol, "{a} vs {b}");
    }

    /// Anscombe (1973), "Graphs in statistical analysis", data set I: the published
    /// correlation is 0.816 (0.81642), the same for the four sets. [conferir]
    #[test]
    fn pearson_of_anscombe_quartet() {
        let x = [10.0, 8.0, 13.0, 9.0, 11.0, 14.0, 6.0, 4.0, 12.0, 7.0, 5.0];
        let y1 = [
            8.04, 6.95, 7.58, 8.81, 8.33, 9.96, 7.24, 4.26, 10.84, 4.82, 5.68,
        ];
        let y2 = [
            9.14, 8.14, 8.74, 8.77, 9.26, 8.10, 6.13, 3.10, 9.13, 7.26, 4.74,
        ];
        let r = pearson(&x, &y1);
        close(r, 0.81642, 5e-5);
        close(pearson(&x, &y2), 0.81624, 5e-5);
        // t = r √(9 / (1 - r²)) = 4.24 with 9 df: p ≈ 0.0022.
        close(p_value(r, 11), 0.00217, 5e-5);
    }

    /// The textbook example of Spearman's coefficient (IQ against hours of television
    /// per week, ten people): ρ = -29/165 = -0.17576, p ≈ 0.627. [conferir]
    #[test]
    fn spearman_of_the_textbook_example() {
        let iq = [
            106.0, 100.0, 86.0, 101.0, 99.0, 103.0, 97.0, 113.0, 112.0, 110.0,
        ];
        let tv = [7.0, 27.0, 2.0, 50.0, 28.0, 29.0, 20.0, 12.0, 6.0, 17.0];
        let rho = spearman(&iq, &tv);
        close(rho, -29.0 / 165.0, 1e-12);
        close(p_value(rho, 10), 0.6272, 5e-4);
    }

    #[test]
    fn ranks_average_ties() {
        assert_eq!(ranks(&[3.0, 1.0, 2.0]), [3.0, 1.0, 2.0]);
        assert_eq!(ranks(&[5.0, 1.0, 5.0, 2.0]), [3.5, 1.0, 3.5, 2.0]);
        // Monotone but not linear: Spearman 1, Pearson below 1.
        let x = [1.0, 2.0, 3.0, 4.0, 5.0];
        let y: Vec<f64> = x.iter().map(|v: &f64| v.powi(3)).collect();
        close(spearman(&x, &y), 1.0, 1e-15);
        assert!(pearson(&x, &y) < 0.99);
        assert!(pearson(&[1.0, 1.0, 1.0], &[1.0, 2.0, 3.0]).is_nan());
        assert!(pearson(&[1.0], &[2.0]).is_nan());
    }

    #[test]
    fn matrix_is_symmetric_and_ranks_pairs() {
        let s = vec![
            ("a".to_string(), vec![1.0, 2.0, 3.0, 4.0, 5.0]),
            ("b".to_string(), vec![2.1, 3.9, 6.2, 8.1, 9.8]),
            ("c".to_string(), vec![5.0, 3.0, 4.0, 1.0, 2.0]),
            ("d".to_string(), vec![7.0, 7.0, 7.0, 7.0, 7.0]),
        ];
        let m = CorrelationMatrix::build(&s);
        assert_eq!(m.n, 5);
        for i in 0..3 {
            assert_eq!(m.pearson[i][i], 1.0);
            for j in 0..4 {
                assert_eq!(m.pearson[i][j].to_bits(), m.pearson[j][i].to_bits());
            }
        }
        assert!(m.pearson[3][3].is_nan() && m.pearson[0][3].is_nan());
        let top = m.strongest();
        assert_eq!(top.len(), 3, "constant metric left out");
        assert_eq!((top[0].a.as_str(), top[0].b.as_str()), ("a", "b"));
        close(top[0].spearman, 1.0, 1e-15);
        assert!(m.pearson[0][2] < -0.7);
    }
}
