//! Binary logistic regression (`docs/plano_estatistica.md`, E4): `P(evento) = 1/(1 + e^−η)`
//! with `η = b₀ + Σ bⱼ·xⱼ`, fitted by maximum likelihood with Newton-Raphson (with step
//! halving) on centred and scaled predictors. It gives the Wald test and the odds ratio
//! with its interval for each coefficient, the deviance and the likelihood-ratio test of the
//! model (G), McFadden's R², the AIC, the Hosmer-Lemeshow test and the fitted
//! probabilities. Complete or quasi-complete separation (no finite estimate) is refused.

use crate::regression::inverse;
use crate::special::{chi2_sf, norm_cdf, norm_ppf};

/// One coefficient of the model.
#[derive(Debug, Clone, PartialEq)]
pub struct LogitCoef {
    pub name: String,
    pub estimate: f64,
    pub se: f64,
    /// Wald `z = b/se`.
    pub z: f64,
    pub p_value: f64,
    /// `e^b` and its interval (`None` for the intercept).
    pub odds_ratio: Option<(f64, f64, f64)>,
}

/// The Hosmer-Lemeshow goodness-of-fit test.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct HosmerLemeshow {
    pub chi2: f64,
    pub df: f64,
    pub p_value: f64,
    pub groups: usize,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Logistic {
    pub n: usize,
    pub events: usize,
    /// Intercept first, then the predictors in order.
    pub coefs: Vec<LogitCoef>,
    pub deviance: f64,
    pub null_deviance: f64,
    /// Likelihood-ratio statistic `G = null − deviance`, its degrees of freedom and p.
    pub g: f64,
    pub g_df: f64,
    pub g_p_value: f64,
    pub mcfadden_r2: f64,
    pub aic: f64,
    pub iterations: usize,
    /// The fitted probability of the event for each observation.
    pub fitted: Vec<f64>,
    pub hosmer_lemeshow: Option<HosmerLemeshow>,
}

impl Logistic {
    /// The probability of the event at the predictor values `x` (one per predictor).
    pub fn predict(&self, x: &[f64]) -> f64 {
        let eta = self.coefs[0].estimate
            + self.coefs[1..]
                .iter()
                .zip(x)
                .map(|(c, v)| c.estimate * v)
                .sum::<f64>();
        sigmoid(eta)
    }
}

fn sigmoid(eta: f64) -> f64 {
    if eta >= 0.0 {
        1.0 / (1.0 + (-eta).exp())
    } else {
        let e = eta.exp();
        e / (1.0 + e)
    }
}

fn deviance(y: &[bool], p: &[f64]) -> f64 {
    -2.0 * y
        .iter()
        .zip(p)
        .map(|(&e, &q)| {
            let q = q.clamp(1e-300, 1.0 - 1e-16);
            if e { q.ln() } else { (1.0 - q).ln() }
        })
        .sum::<f64>()
}

/// Hosmer-Lemeshow with the observations sorted by fitted probability and split into `g`
/// groups of (nearly) equal size, `g = min(10, n/5)`; `None` below 3 groups.
pub fn hosmer_lemeshow(y: &[bool], p: &[f64]) -> Option<HosmerLemeshow> {
    let n = y.len();
    let g = (n / 5).min(10);
    if g < 3 {
        return None;
    }
    let mut ord: Vec<usize> = (0..n).collect();
    ord.sort_by(|&a, &b| p[a].total_cmp(&p[b]));
    let mut chi2 = 0.0;
    for k in 0..g {
        let idx = &ord[k * n / g..(k + 1) * n / g];
        let ng = idx.len() as f64;
        let obs = idx.iter().filter(|&&i| y[i]).count() as f64;
        let exp: f64 = idx.iter().map(|&i| p[i]).sum();
        let pbar = exp / ng;
        let v = ng * pbar * (1.0 - pbar);
        if v > 0.0 {
            chi2 += (obs - exp).powi(2) / v;
        }
    }
    let df = (g - 2) as f64;
    Some(HosmerLemeshow {
        chi2,
        df,
        p_value: chi2_sf(chi2, df),
        groups: g,
    })
}

/// The logistic regression of the event `y` on the predictors `x[j][i]` (predictor `j`,
/// observation `i`) named `names`, with odds-ratio intervals at the confidence `level`.
pub fn fit(names: &[String], x: &[Vec<f64>], y: &[bool], level: f64) -> Result<Logistic, String> {
    let n = y.len();
    let k = x.len();
    if k == 0 || x.iter().any(|c| c.len() != n) {
        return Err(
            "as colunas dos preditores e a resposta precisam do mesmo número de linhas".into(),
        );
    }
    if !(level > 0.0 && level < 1.0) {
        return Err("o nível de confiança precisa estar entre 0 e 1".into());
    }
    let events = y.iter().filter(|&&e| e).count();
    if events == 0 || events == n {
        return Err("a resposta precisa ter os dois valores (evento e não evento)".into());
    }
    if n < k + 2 {
        return Err(format!(
            "a logística com {k} preditor(es) precisa de ao menos {} observações",
            k + 2
        ));
    }
    let mean = |v: &[f64]| v.iter().sum::<f64>() / v.len() as f64;
    let mx: Vec<f64> = x.iter().map(|c| mean(c)).collect();
    let sx: Vec<f64> = x
        .iter()
        .zip(&mx)
        .map(|(c, m)| (c.iter().map(|v| (v - m).powi(2)).sum::<f64>() / n as f64).sqrt())
        .collect();
    if let Some(j) = sx.iter().position(|s| s.is_nan() || *s <= 0.0) {
        return Err(format!(
            "o preditor {} não varia",
            names.get(j).map_or("?", |s| s)
        ));
    }
    // Design matrix on the standardized scale, intercept first.
    let row = |i: usize| -> Vec<f64> {
        std::iter::once(1.0)
            .chain((0..k).map(|j| (x[j][i] - mx[j]) / sx[j]))
            .collect()
    };
    let rows: Vec<Vec<f64>> = (0..n).map(row).collect();
    let m = k + 1;
    let ybar = events as f64 / n as f64;
    let mut b = vec![0.0; m];
    b[0] = (ybar / (1.0 - ybar)).ln();
    let probs = |b: &[f64]| -> Vec<f64> {
        rows.iter()
            .map(|r| sigmoid(r.iter().zip(b).map(|(u, v)| u * v).sum()))
            .collect()
    };
    let mut p = probs(&b);
    let mut dev = deviance(y, &p);
    let mut iterations = 0;
    let mut converged = false;
    let mut cov = Vec::new();
    for it in 1..=100 {
        iterations = it;
        let mut info = vec![vec![0.0; m]; m];
        let mut score = vec![0.0; m];
        for ((r, &e), &q) in rows.iter().zip(y).zip(&p) {
            let w = q * (1.0 - q);
            let res = if e { 1.0 } else { 0.0 } - q;
            for a in 0..m {
                score[a] += r[a] * res;
                for c in 0..m {
                    info[a][c] += w * r[a] * r[c];
                }
            }
        }
        let inv = inverse(&info)
            .ok_or("preditores colineares ou separação: a informação ficou singular")?;
        let step: Vec<f64> = (0..m)
            .map(|a| (0..m).map(|c| inv[a][c] * score[c]).sum())
            .collect();
        cov = inv;
        let mut t = 1.0;
        let (mut nb, mut np, mut nd);
        loop {
            nb = b
                .iter()
                .zip(&step)
                .map(|(u, s)| u + t * s)
                .collect::<Vec<_>>();
            np = probs(&nb);
            nd = deviance(y, &np);
            if nd <= dev + 1e-12 || t < 1e-6 {
                break;
            }
            t /= 2.0;
        }
        let change = step.iter().map(|s| (t * s).abs()).fold(0.0, f64::max);
        b = nb;
        p = np;
        let dd = dev - nd;
        dev = nd;
        if b.iter().skip(1).any(|v| v.abs() > 30.0) {
            break;
        }
        if change < 1e-10 || (dd.abs() < 1e-12 && change < 1e-6) {
            converged = true;
            break;
        }
    }
    let separated = b.iter().skip(1).any(|v| v.abs() > 30.0)
        || p.iter().all(|&q| !(1e-8..=1.0 - 1e-8).contains(&q));
    if !converged || separated {
        return Err("separação: os preditores separam (quase) por completo os eventos dos não eventos, e a logística não tem estimativa finita".into());
    }
    // Information at the final estimate for the covariance.
    let mut info = vec![vec![0.0; m]; m];
    for (r, &q) in rows.iter().zip(&p) {
        let w = q * (1.0 - q);
        for a in 0..m {
            for c in 0..m {
                info[a][c] += w * r[a] * r[c];
            }
        }
    }
    if let Some(inv) = inverse(&info) {
        cov = inv;
    }
    // Back to the original scale: β = T·βz, Cov(β) = T·Cov(βz)·Tᵀ.
    let mut tm = vec![vec![0.0; m]; m];
    tm[0][0] = 1.0;
    for j in 0..k {
        tm[0][j + 1] = -mx[j] / sx[j];
        tm[j + 1][j + 1] = 1.0 / sx[j];
    }
    let beta: Vec<f64> = (0..m)
        .map(|a| (0..m).map(|c| tm[a][c] * b[c]).sum())
        .collect();
    let var: Vec<f64> = (0..m)
        .map(|a| {
            (0..m)
                .map(|c| (0..m).map(|d| tm[a][c] * cov[c][d] * tm[a][d]).sum::<f64>())
                .sum()
        })
        .collect();
    let zq = norm_ppf(0.5 + level / 2.0);
    let coefs = (0..m)
        .map(|a| {
            let se = var[a].max(0.0).sqrt();
            let z = beta[a] / se;
            LogitCoef {
                name: if a == 0 {
                    "constante".into()
                } else {
                    names.get(a - 1).cloned().unwrap_or_else(|| format!("x{a}"))
                },
                estimate: beta[a],
                se,
                z,
                p_value: 2.0 * (1.0 - norm_cdf(z.abs())),
                odds_ratio: (a > 0).then(|| {
                    (
                        beta[a].exp(),
                        (beta[a] - zq * se).exp(),
                        (beta[a] + zq * se).exp(),
                    )
                }),
            }
        })
        .collect();
    let null_deviance = deviance(y, &vec![ybar; n]);
    let g = (null_deviance - dev).max(0.0);
    Ok(Logistic {
        n,
        events,
        coefs,
        deviance: dev,
        null_deviance,
        g,
        g_df: k as f64,
        g_p_value: chi2_sf(g, k as f64),
        mcfadden_r2: 1.0 - dev / null_deviance,
        aic: dev + 2.0 * m as f64,
        iterations,
        hosmer_lemeshow: hosmer_lemeshow(y, &p),
        fitted: p,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Launch temperatures (°F) and whether an O-ring was damaged: a classic data set with
    /// published estimates b₀ = 15,0429 (EP 7,3786) and b₁ = −0,2322 (EP 0,1082).
    fn lancamentos() -> (Vec<f64>, Vec<bool>) {
        let t = [
            53, 57, 58, 63, 66, 67, 67, 67, 68, 69, 70, 70, 70, 70, 72, 73, 75, 75, 76, 76, 78, 79,
            81,
        ];
        let y = [
            1, 1, 1, 1, 0, 0, 0, 0, 0, 0, 1, 0, 1, 0, 0, 0, 0, 1, 0, 0, 0, 0, 0,
        ];
        (
            t.iter().map(|&v| v as f64).collect(),
            y.iter().map(|&v| v == 1).collect(),
        )
    }

    #[test]
    fn matches_the_published_fit() {
        let (t, y) = lancamentos();
        let r = fit(&["temperatura".into()], &[t], &y, 0.95).unwrap();
        assert_eq!((r.n, r.events), (23, 7));
        let (b0, b1) = (&r.coefs[0], &r.coefs[1]);
        assert!((b0.estimate - 15.042902).abs() < 1e-5, "{b0:?}");
        assert!((b1.estimate + 0.232163).abs() < 1e-6, "{b1:?}");
        assert!((b0.se - 7.378636).abs() < 1e-5 && (b1.se - 0.108237).abs() < 1e-6);
        assert!((r.deviance - 20.315193).abs() < 1e-5);
        assert!((r.null_deviance - 28.267153).abs() < 1e-5);
        assert!((r.g - 7.951960).abs() < 1e-5 && r.g_p_value < 0.01);
        assert!((r.aic - 24.315193).abs() < 1e-5);
        let (or, lo, hi) = b1.odds_ratio.unwrap();
        assert!((or - (-0.232163f64).exp()).abs() < 1e-6 && lo < or && or < hi && hi < 1.0);
        assert!(b0.odds_ratio.is_none());
        // At 31 °F the damage is almost certain.
        assert!(r.predict(&[31.0]) > 0.99);
        let hl = r.hosmer_lemeshow.unwrap();
        assert_eq!((hl.groups, hl.df), (4, 2.0));
        assert!((hl.chi2 - 2.270836).abs() < 1e-4, "{hl:?}");
        assert!(hl.p_value > 0.05 && hl.p_value <= 1.0);
    }

    #[test]
    fn the_scale_of_a_predictor_does_not_change_the_fit() {
        let (t, y) = lancamentos();
        let a = fit(&["t".into()], std::slice::from_ref(&t), &y, 0.95).unwrap();
        let big: Vec<f64> = t.iter().map(|v| v * 1000.0 + 1e6).collect();
        let b = fit(&["t".into()], &[big], &y, 0.95).unwrap();
        assert!((a.deviance - b.deviance).abs() < 1e-8);
        assert!((a.coefs[1].z - b.coefs[1].z).abs() < 1e-6);
    }

    #[test]
    fn separation_and_bad_input_are_refused() {
        let x: Vec<f64> = (0..20).map(f64::from).collect();
        let y: Vec<bool> = (0..20).map(|i| i >= 10).collect();
        let e = fit(&["x".into()], std::slice::from_ref(&x), &y, 0.95).unwrap_err();
        assert!(e.contains("separação"), "{e}");
        assert!(fit(&["x".into()], std::slice::from_ref(&x), &[false; 20], 0.95).is_err());
        assert!(fit(&["x".into()], &[vec![1.0; 20]], &y, 0.95).is_err());
    }
}
