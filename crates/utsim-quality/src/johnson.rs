//! Johnson transformation: turns non-normal data into normal by one of the three
//! Johnson families, chosen as by Chou, Polansky and Mason (1998, *Journal of Quality
//! Technology* 30(2)).
//!
//! For each `z` in 0.25, 0.26, …, 1.25 the four sample quantiles at `Φ(−3z)`, `Φ(−z)`,
//! `Φ(z)` and `Φ(3z)` give `m = x₃z − x_z`, `n = x₋z − x₋₃z` and `p = x_z − x₋z`. The
//! quantile ratio `mn/p²` picks the family (above 1: unbounded SU; below 1: bounded SB;
//! about 1: lognormal SL) and the four quantiles fix its parameters. Every candidate that
//! transforms all values is tested by Anderson-Darling; the one with the largest p-value
//! wins.
//!
//! - SU: `y = γ + η·asinh((x − ε)/λ)`;
//! - SB: `y = γ + η·ln((x − ε)/(λ + ε − x))`, for `ε < x < ε + λ`;
//! - SL: `y = γ + η·ln(x − ε)`, for `x > ε`.

use serde::{Deserialize, Serialize};
use utsim_stats::descriptive::{QuantileMethod, quantile_sorted};
use utsim_stats::special::norm_cdf;

use crate::normality::anderson_darling;

/// The Johnson family.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Family {
    /// Unbounded.
    Su,
    /// Bounded on both sides.
    Sb,
    /// Lognormal (bounded below).
    Sl,
}

/// A fitted Johnson transformation.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct JohnsonFit {
    pub family: Family,
    pub gamma: f64,
    pub eta: f64,
    pub epsilon: f64,
    /// Scale (1 for SL, where it does not exist).
    pub lambda: f64,
    /// The `z` of the quantiles that gave the parameters.
    pub z: f64,
    /// Anderson-Darling p-value of the transformed data.
    pub p_value: f64,
}

impl JohnsonFit {
    /// The transformed value, or `None` outside the support of an SB or SL.
    pub fn transform(&self, x: f64) -> Option<f64> {
        let y = match self.family {
            Family::Su => self.gamma + self.eta * ((x - self.epsilon) / self.lambda).asinh(),
            Family::Sb => {
                let (a, b) = (x - self.epsilon, self.lambda + self.epsilon - x);
                if a <= 0.0 || b <= 0.0 {
                    return None;
                }
                self.gamma + self.eta * (a / b).ln()
            }
            Family::Sl => {
                let a = x - self.epsilon;
                if a <= 0.0 {
                    return None;
                }
                self.gamma + self.eta * a.ln()
            }
        };
        y.is_finite().then_some(y)
    }

    /// The value whose transform is `y` (back to the units of the data).
    pub fn inverse(&self, y: f64) -> f64 {
        let u = (y - self.gamma) / self.eta;
        match self.family {
            Family::Su => self.epsilon + self.lambda * u.sinh(),
            Family::Sb => self.epsilon + self.lambda / (1.0 + (-u).exp()),
            Family::Sl => self.epsilon + u.exp(),
        }
    }

    /// Every value transformed, or `None` when one falls outside the support.
    pub fn transform_all(&self, xs: &[f64]) -> Option<Vec<f64>> {
        xs.iter().map(|&x| self.transform(x)).collect()
    }
}

/// Parameters from the four quantiles at `z` (`None` when they do not give a valid
/// member of the family).
fn candidate(family: Family, z: f64, q: [f64; 4]) -> Option<(f64, f64, f64, f64)> {
    let [x3m, xm, xp, x3p] = q;
    let (m, n, p) = (x3p - xp, xm - x3m, xp - xm);
    if !(m > 0.0 && n > 0.0 && p > 0.0) {
        return None;
    }
    let (mp, np) = (m / p, n / p);
    let mid = (xp + xm) / 2.0;
    let r = match family {
        Family::Su => {
            let qr = mp * np - 1.0;
            if qr <= 0.0 || mp + np <= 2.0 {
                return None;
            }
            let eta = 2.0 * z / (0.5 * (mp + np)).acosh();
            let gamma = eta * ((np - mp) / (2.0 * qr.sqrt())).asinh();
            let lambda = 2.0 * p * qr.sqrt() / ((mp + np - 2.0) * (mp + np + 2.0).sqrt());
            let epsilon = mid + p * (np - mp) / (2.0 * (mp + np - 2.0));
            (gamma, eta, epsilon, lambda)
        }
        Family::Sb => {
            let (pm, pn) = (p / m, p / n);
            let a = (1.0 + pm) * (1.0 + pn);
            let qr = pm * pn - 1.0;
            if qr <= 0.0 || a <= 4.0 {
                return None;
            }
            let eta = z / (0.5 * a.sqrt()).acosh();
            let gamma = eta * ((pn - pm) * (a - 4.0).sqrt() / (2.0 * qr)).asinh();
            let lambda = p * ((a - 2.0).powi(2) - 4.0).sqrt() / qr;
            let epsilon = mid - lambda / 2.0 + p * (pn - pm) / (2.0 * qr);
            (gamma, eta, epsilon, lambda)
        }
        Family::Sl => {
            if mp <= 1.0 {
                return None;
            }
            let eta = 2.0 * z / mp.ln();
            let gamma = eta * ((mp - 1.0) / (p * mp.sqrt())).ln();
            let epsilon = mid - (p / 2.0) * (mp + 1.0) / (mp - 1.0);
            (gamma, eta, epsilon, 1.0)
        }
    };
    (r.0.is_finite() && r.1.is_finite() && r.1 > 0.0 && r.2.is_finite() && r.3 > 0.0).then_some(r)
}

/// Fits the Johnson transformation of `xs` (at least 8 values, not all equal).
///
/// `sl_band` is how close to 1 the quantile ratio must be for the lognormal SL to be
/// tried as well (0.05 is a usual choice).
pub fn johnson_fit(xs: &[f64], sl_band: f64) -> Result<JohnsonFit, String> {
    let mut v: Vec<f64> = xs.iter().copied().filter(|x| x.is_finite()).collect();
    if v.len() < 8 {
        return Err("Johnson exige ao menos 8 valores".into());
    }
    v.sort_by(f64::total_cmp);
    if v[0] == v[v.len() - 1] {
        return Err("Johnson com dados constantes".into());
    }
    let mut best: Option<JohnsonFit> = None;
    for i in 0..=100 {
        let z = 0.25 + 0.01 * i as f64;
        let q = [-3.0 * z, -z, z, 3.0 * z]
            .map(|t| quantile_sorted(&v, norm_cdf(t), QuantileMethod::default()));
        let (m, n, p) = (q[3] - q[2], q[1] - q[0], q[2] - q[1]);
        if p <= 0.0 {
            continue;
        }
        let qr = m * n / (p * p);
        let mut familias = vec![if qr > 1.0 { Family::Su } else { Family::Sb }];
        if (qr - 1.0).abs() <= sl_band {
            familias.push(Family::Sl);
        }
        for family in familias {
            let Some((gamma, eta, epsilon, lambda)) = candidate(family, z, q) else {
                continue;
            };
            let mut fit = JohnsonFit {
                family,
                gamma,
                eta,
                epsilon,
                lambda,
                z,
                p_value: 0.0,
            };
            let Some(y) = fit.transform_all(&v) else {
                continue;
            };
            let Some(ad) = anderson_darling(&y) else {
                continue;
            };
            fit.p_value = ad.p_value;
            if best.is_none_or(|b| fit.p_value > b.p_value) {
                best = Some(fit);
            }
        }
    }
    best.ok_or_else(|| "nenhuma transformação de Johnson serve para estes dados".into())
}

#[cfg(test)]
mod tests {
    use super::*;
    use rand::SeedableRng;
    use rand_distr::{Distribution, StandardNormal};
    use rand_pcg::Pcg64;

    fn normais(n: usize, semente: u64) -> Vec<f64> {
        let mut r = Pcg64::seed_from_u64(semente);
        (0..n).map(|_| StandardNormal.sample(&mut r)).collect()
    }

    #[test]
    fn unbounded_data_get_an_su_that_makes_them_normal() {
        // x = ε + λ·sinh((y − γ)/η), y normal: an SU by construction.
        let (g, e, eps, l) = (-1.0, 1.2, 10.0, 2.0);
        let xs: Vec<f64> = normais(300, 1)
            .into_iter()
            .map(|y| eps + l * ((y - g) / e).sinh())
            .collect();
        assert!(anderson_darling(&xs).unwrap().p_value < 0.01);
        let f = johnson_fit(&xs, 0.05).unwrap();
        assert_eq!(f.family, Family::Su);
        assert!(f.p_value > 0.1, "{f:?}");
        // The inverse undoes the transform.
        for x in [8.0, 10.0, 25.0] {
            let y = f.transform(x).unwrap();
            assert!((f.inverse(y) - x).abs() < 1e-9);
        }
    }

    #[test]
    fn bounded_data_get_an_sb() {
        // x = ε + λ/(1 + e^{−(y − γ)/η}): an SB on (2, 7).
        let (g, e, eps, l) = (0.5, 0.8, 2.0, 5.0);
        let xs: Vec<f64> = normais(300, 2)
            .into_iter()
            .map(|y| eps + l / (1.0 + (-(y - g) / e).exp()))
            .collect();
        let f = johnson_fit(&xs, 0.05).unwrap();
        assert_eq!(f.family, Family::Sb, "{f:?}");
        assert!(f.p_value > 0.1, "{f:?}");
        assert!(
            f.epsilon < 2.0 + 0.5 && f.epsilon + f.lambda > 7.0 - 0.5,
            "{f:?}"
        );
        let y = f.transform(4.5).unwrap();
        assert!((f.inverse(y) - 4.5).abs() < 1e-9);
    }

    #[test]
    fn skewed_data_become_normal() {
        // Exponential and lognormal samples: clearly non-normal before, normal after.
        let mut r = Pcg64::seed_from_u64(3);
        let exp: Vec<f64> = (0..200).map(|_| rand_distr::Exp1.sample(&mut r)).collect();
        let lognormal: Vec<f64> = normais(200, 4)
            .into_iter()
            .map(|y| (0.6 * y).exp())
            .collect();
        for xs in [exp, lognormal] {
            assert!(anderson_darling(&xs).unwrap().p_value < 0.01);
            let f = johnson_fit(&xs, 0.05).unwrap();
            assert!(f.p_value > 0.1, "{f:?}");
            let y = f.transform_all(&xs).unwrap();
            assert_eq!(y.len(), xs.len());
        }
    }

    #[test]
    fn too_few_or_constant_values_are_refused() {
        assert!(johnson_fit(&[1.0, 2.0, 3.0], 0.05).is_err());
        assert!(johnson_fit(&[2.0; 20], 0.05).is_err());
    }
}
