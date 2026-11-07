//! Rank and median tests of several groups (`docs/plano_lean_seis_sigma.md`, 2.3, P2):
//! Kruskal-Wallis (the one-way ANOVA of ranks, with the correction for ties), Mood's median
//! test (counts above and not above the grand median, χ²) and Friedman's test (treatments
//! ranked within each block, with the correction for ties). All by the χ² approximation.

use crate::special::chi2_sf;

/// The result of a test by χ².
#[derive(Debug, Clone, PartialEq)]
pub struct ChiTest {
    pub statistic: f64,
    pub df: f64,
    pub p_value: f64,
    /// Kruskal-Wallis and Friedman: mean rank of each group; Mood: median of each group.
    pub per_group: Vec<f64>,
}

/// Mid-ranks (1-based) of `v`, and `Σ(t³ − t)` of the ties.
fn midranks(v: &[f64]) -> (Vec<f64>, f64) {
    let mut idx: Vec<usize> = (0..v.len()).collect();
    idx.sort_by(|a, b| v[*a].total_cmp(&v[*b]));
    let mut r = vec![0.0; v.len()];
    let mut ties = 0.0;
    let mut i = 0;
    while i < idx.len() {
        let mut j = i;
        while j + 1 < idx.len() && v[idx[j + 1]] == v[idx[i]] {
            j += 1;
        }
        let avg = (i + j) as f64 / 2.0 + 1.0;
        for k in &idx[i..=j] {
            r[*k] = avg;
        }
        let t = (j - i + 1) as f64;
        ties += t * t * t - t;
        i = j + 1;
    }
    (r, ties)
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

/// Kruskal-Wallis: `H = 12/(N(N+1)) Σ Rᵢ²/nᵢ − 3(N+1)`, divided by
/// `1 − Σ(t³−t)/(N³−N)`. `None` with fewer than two non-empty groups or all values equal.
pub fn kruskal_wallis(groups: &[&[f64]]) -> Option<ChiTest> {
    let groups: Vec<&[f64]> = groups.iter().copied().filter(|g| !g.is_empty()).collect();
    if groups.len() < 2 {
        return None;
    }
    let all: Vec<f64> = groups.iter().flat_map(|g| g.iter().copied()).collect();
    let n = all.len() as f64;
    let (r, ties) = midranks(&all);
    let c = 1.0 - ties / (n * n * n - n);
    if c <= 0.0 {
        return None;
    }
    let mut pos = 0;
    let mut sum = 0.0;
    let mut means = Vec::new();
    for g in &groups {
        let rs: f64 = r[pos..pos + g.len()].iter().sum();
        pos += g.len();
        sum += rs * rs / g.len() as f64;
        means.push(rs / g.len() as f64);
    }
    let h = (12.0 / (n * (n + 1.0)) * sum - 3.0 * (n + 1.0)) / c;
    let df = groups.len() as f64 - 1.0;
    Some(ChiTest {
        statistic: h,
        df,
        p_value: chi2_sf(h, df),
        per_group: means,
    })
}

/// Mood's median test: in each group, how many values are above the grand median and how
/// many are not; the χ² of that `2 × k` table, `k − 1` degrees of freedom. `None` with
/// fewer than two non-empty groups or when every value is on one side.
pub fn mood_median(groups: &[&[f64]]) -> Option<ChiTest> {
    let groups: Vec<&[f64]> = groups.iter().copied().filter(|g| !g.is_empty()).collect();
    if groups.len() < 2 {
        return None;
    }
    let all: Vec<f64> = groups.iter().flat_map(|g| g.iter().copied()).collect();
    let m = median(&all);
    let above: Vec<f64> = groups
        .iter()
        .map(|g| g.iter().filter(|x| **x > m).count() as f64)
        .collect();
    let sizes: Vec<f64> = groups.iter().map(|g| g.len() as f64).collect();
    let n: f64 = sizes.iter().sum();
    let ta: f64 = above.iter().sum();
    let tb = n - ta;
    if ta == 0.0 || tb == 0.0 {
        return None;
    }
    let mut chi = 0.0;
    for (a, s) in above.iter().zip(&sizes) {
        let ea = s * ta / n;
        let eb = s * tb / n;
        chi += (a - ea).powi(2) / ea + ((s - a) - eb).powi(2) / eb;
    }
    let df = groups.len() as f64 - 1.0;
    Some(ChiTest {
        statistic: chi,
        df,
        p_value: chi2_sf(chi, df),
        per_group: groups.iter().map(|g| median(g)).collect(),
    })
}

/// Friedman's test of `k` treatments in `b` blocks (`data[block][treatment]`): ranks within
/// each block, `Q = 12/(b k (k+1)) Σ Rⱼ² − 3 b (k+1)`, divided by
/// `1 − Σ(t³−t)/(b (k³−k))`. `None` with fewer than two blocks or treatments, a ragged
/// table, or every block tied.
pub fn friedman(data: &[Vec<f64>]) -> Option<ChiTest> {
    let b = data.len();
    let k = data.first().map_or(0, Vec::len);
    if b < 2 || k < 2 || data.iter().any(|r| r.len() != k) {
        return None;
    }
    let mut sums = vec![0.0; k];
    let mut ties = 0.0;
    for row in data {
        let (r, t) = midranks(row);
        ties += t;
        for (s, x) in sums.iter_mut().zip(r) {
            *s += x;
        }
    }
    let (bf, kf) = (b as f64, k as f64);
    let c = 1.0 - ties / (bf * (kf * kf * kf - kf));
    if c <= 0.0 {
        return None;
    }
    let q = (12.0 / (bf * kf * (kf + 1.0)) * sums.iter().map(|s| s * s).sum::<f64>()
        - 3.0 * bf * (kf + 1.0))
        / c;
    Some(ChiTest {
        statistic: q,
        df: kf - 1.0,
        p_value: chi2_sf(q, kf - 1.0),
        per_group: sums.iter().map(|s| s / bf).collect(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kruskal_without_and_with_ties() {
        // Ranks 1..9 in three groups of three: R = 6, 15, 24;
        // H = 12/90 · (36+225+576)/3 − 30 = 12/90 · 279 − 30 = 7,2.
        let a = [1.0, 2.0, 3.0];
        let b = [4.0, 5.0, 6.0];
        let c = [7.0, 8.0, 9.0];
        let r = kruskal_wallis(&[&a, &b, &c]).unwrap();
        assert!((r.statistic - 7.2).abs() < 1e-12);
        assert_eq!(r.df, 2.0);
        assert_eq!(r.per_group, vec![2.0, 5.0, 8.0]);
        assert!((r.p_value - chi2_sf(7.2, 2.0)).abs() < 1e-15);
        // A tie of two (ranks 3,5 → 3,5 each): C = 1 − 6/(720) .
        let d = [1.0, 2.0, 5.0];
        let e = [5.0, 6.0, 7.0];
        let t = kruskal_wallis(&[&d, &e]).unwrap();
        // ranks: 1,2,3.5 | 3.5,5,6 → R = 6.5, 14.5; n = 6.
        let h = (12.0 / 42.0 * (6.5f64.powi(2) / 3.0 + 14.5f64.powi(2) / 3.0) - 21.0)
            / (1.0 - 6.0 / 210.0);
        assert!((t.statistic - h).abs() < 1e-12);
        assert!(kruskal_wallis(&[&a]).is_none());
        assert!(kruskal_wallis(&[&[2.0, 2.0], &[2.0]]).is_none());
    }

    #[test]
    fn mood_median_by_hand() {
        // Grand median 4,5: above = 0, 3 of 3 each; expected 1,5 each cell;
        // χ² = 4 · (1,5²/1,5) = 6.
        let a = [1.0, 2.0, 3.0];
        let b = [6.0, 7.0, 8.0];
        let r = mood_median(&[&a, &b]).unwrap();
        assert!((r.statistic - 6.0).abs() < 1e-12);
        assert_eq!(r.per_group, vec![2.0, 7.0]);
        assert!(mood_median(&[&[1.0], &[1.0]]).is_none());
    }

    #[test]
    fn friedman_by_hand() {
        // Treatment 3 best in every one of 4 blocks, 1 worst: R = 4, 8, 12;
        // Q = 12/(4·3·4) · (16+64+144) − 3·4·4 = 0,25 · 224 − 48 = 8.
        let data = vec![
            vec![1.0, 2.0, 3.0],
            vec![2.0, 4.0, 9.0],
            vec![0.0, 1.0, 5.0],
            vec![3.0, 3.5, 4.0],
        ];
        let r = friedman(&data).unwrap();
        assert!((r.statistic - 8.0).abs() < 1e-12);
        assert_eq!(r.per_group, vec![1.0, 2.0, 3.0]);
        assert!(friedman(&[vec![1.0, 2.0]]).is_none());
        assert!(friedman(&[vec![1.0, 1.0], vec![2.0, 2.0]]).is_none());
    }
}
