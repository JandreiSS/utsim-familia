//! Linear regression, simple and multiple, by least squares (`docs/plano_lean_seis_sigma.md`,
//! 2.3, P2): coefficients with standard errors, t and p-values and intervals, R², adjusted
//! R², the F test of the model, the residual standard error, the variance inflation factor
//! of each predictor, the fitted values and the residuals. The normal equations are solved
//! by Gauss-Jordan with partial pivoting on centred and scaled predictors (stable for the
//! small models of a project).

use crate::special::{f_sf, t_cdf, t_ppf};

#[derive(Debug, Clone, PartialEq)]
pub struct Coef {
    /// `constante` or the predictor's name.
    pub name: String,
    pub estimate: f64,
    pub se: f64,
    pub t: f64,
    pub p_value: f64,
    pub ci: (f64, f64),
    /// Variance inflation factor (none for the constant).
    pub vif: Option<f64>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Regression {
    pub n: usize,
    pub coefs: Vec<Coef>,
    pub r2: f64,
    pub r2_adj: f64,
    /// Residual standard error.
    pub s: f64,
    pub f: f64,
    pub f_p_value: f64,
    pub df_model: f64,
    pub df_error: f64,
    pub fitted: Vec<f64>,
    pub residuals: Vec<f64>,
}

/// Inverse of a square matrix by Gauss-Jordan with partial pivoting; `None` when singular.
pub(crate) fn inverse(m: &[Vec<f64>]) -> Option<Vec<Vec<f64>>> {
    let n = m.len();
    let mut a: Vec<Vec<f64>> = m
        .iter()
        .enumerate()
        .map(|(i, r)| {
            let mut row = r.clone();
            row.extend((0..n).map(|j| if i == j { 1.0 } else { 0.0 }));
            row
        })
        .collect();
    let scale = m
        .iter()
        .flatten()
        .fold(0.0f64, |s, x| s.max(x.abs()))
        .max(1e-300);
    for c in 0..n {
        let p = (c..n).max_by(|i, j| a[*i][c].abs().total_cmp(&a[*j][c].abs()))?;
        if a[p][c].abs() < 1e-12 * scale {
            return None;
        }
        a.swap(c, p);
        let d = a[c][c];
        for x in a[c].iter_mut() {
            *x /= d;
        }
        for r in 0..n {
            if r != c {
                let f = a[r][c];
                if f != 0.0 {
                    let pivot = a[c].clone();
                    for (x, q) in a[r].iter_mut().zip(&pivot) {
                        *x -= f * q;
                    }
                }
            }
        }
    }
    Some(a.into_iter().map(|r| r[n..].to_vec()).collect())
}

/// The regression of `y` on the predictors `x[j][i]` (predictor `j`, observation `i`)
/// named `names`, at the confidence `level` for the intervals.
pub fn fit(names: &[String], x: &[Vec<f64>], y: &[f64], level: f64) -> Result<Regression, String> {
    let n = y.len();
    let p = x.len();
    if p == 0 || x.iter().any(|c| c.len() != n) {
        return Err(
            "as colunas dos preditores e a resposta precisam do mesmo número de linhas".into(),
        );
    }
    if n < p + 2 {
        return Err(format!(
            "a regressão com {p} preditor(es) precisa de ao menos {} observações",
            p + 2
        ));
    }
    let nf = n as f64;
    let mean = |v: &[f64]| v.iter().sum::<f64>() / v.len() as f64;
    let mx: Vec<f64> = x.iter().map(|c| mean(c)).collect();
    let sx: Vec<f64> = x
        .iter()
        .zip(&mx)
        .map(|(c, m)| (c.iter().map(|v| (v - m).powi(2)).sum::<f64>()).sqrt())
        .collect();
    if let Some(j) = sx.iter().position(|s| s.is_nan() || *s <= 0.0) {
        return Err(format!(
            "o preditor {} não varia",
            names.get(j).map_or("?", |s| s)
        ));
    }
    // Standardized predictors z = (x − m)/s: z'z is the correlation matrix.
    let z: Vec<Vec<f64>> = x
        .iter()
        .zip(mx.iter().zip(&sx))
        .map(|(c, (m, s))| c.iter().map(|v| (v - m) / s).collect())
        .collect();
    let corr: Vec<Vec<f64>> = (0..p)
        .map(|a| {
            (0..p)
                .map(|b| z[a].iter().zip(&z[b]).map(|(u, v)| u * v).sum())
                .collect()
        })
        .collect();
    let inv = inverse(&corr).ok_or("preditores colineares: um deles é combinação dos outros")?;
    let my = mean(y);
    let zy: Vec<f64> = (0..p)
        .map(|a| z[a].iter().zip(y).map(|(u, v)| u * (v - my)).sum())
        .collect();
    let bz: Vec<f64> = (0..p)
        .map(|a| (0..p).map(|b| inv[a][b] * zy[b]).sum())
        .collect();
    let b: Vec<f64> = bz.iter().zip(&sx).map(|(c, s)| c / s).collect();
    let b0 = my - b.iter().zip(&mx).map(|(c, m)| c * m).sum::<f64>();
    let fitted: Vec<f64> = (0..n)
        .map(|i| b0 + (0..p).map(|j| b[j] * x[j][i]).sum::<f64>())
        .collect();
    let residuals: Vec<f64> = y.iter().zip(&fitted).map(|(a, f)| a - f).collect();
    let sse: f64 = residuals.iter().map(|r| r * r).sum();
    let sst: f64 = y.iter().map(|v| (v - my).powi(2)).sum();
    let df_e = nf - p as f64 - 1.0;
    let mse = sse / df_e;
    let s = mse.sqrt();
    let tq = t_ppf(1.0 - (1.0 - level) / 2.0, df_e);
    let pv = |t: f64| {
        if t.is_finite() {
            2.0 * (1.0 - t_cdf(t.abs(), df_e))
        } else {
            0.0
        }
    };
    let mut coefs = Vec::new();
    // Var(b0) = mse (1/n + m' (X_c'X_c)⁻¹ m), with (X_c'X_c)⁻¹ = S⁻¹ inv S⁻¹.
    let mut q = 0.0;
    for a in 0..p {
        for c in 0..p {
            q += mx[a] * inv[a][c] / (sx[a] * sx[c]) * mx[c];
        }
    }
    let se0 = (mse * (1.0 / nf + q)).sqrt();
    let t0 = b0 / se0;
    coefs.push(Coef {
        name: "constante".into(),
        estimate: b0,
        se: se0,
        t: t0,
        p_value: pv(t0),
        ci: (b0 - tq * se0, b0 + tq * se0),
        vif: None,
    });
    for j in 0..p {
        let se = (mse * inv[j][j]).sqrt() / sx[j];
        let t = b[j] / se;
        coefs.push(Coef {
            name: names
                .get(j)
                .cloned()
                .unwrap_or_else(|| format!("x{}", j + 1)),
            estimate: b[j],
            se,
            t,
            p_value: pv(t),
            ci: (b[j] - tq * se, b[j] + tq * se),
            vif: Some(inv[j][j]),
        });
    }
    let r2 = if sst > 0.0 { 1.0 - sse / sst } else { 0.0 };
    let df_m = p as f64;
    let f = if mse > 0.0 {
        ((sst - sse) / df_m) / mse
    } else {
        f64::INFINITY
    };
    Ok(Regression {
        n,
        coefs,
        r2,
        r2_adj: 1.0 - (1.0 - r2) * (nf - 1.0) / df_e,
        s,
        f,
        f_p_value: f_sf(f, df_m, df_e),
        df_model: df_m,
        df_error: df_e,
        fitted,
        residuals,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn simple_regression_by_hand() {
        // x 1..5, y 2,4,5,4,5: b = 0,6, b0 = 2,2; SSE = 2,4; SST = 6; R² = 0,6.
        let x = vec![vec![1.0, 2.0, 3.0, 4.0, 5.0]];
        let y = [2.0, 4.0, 5.0, 4.0, 5.0];
        let r = fit(&["x".into()], &x, &y, 0.95).unwrap();
        assert!((r.coefs[1].estimate - 0.6).abs() < 1e-12);
        assert!((r.coefs[0].estimate - 2.2).abs() < 1e-12);
        assert!((r.r2 - 0.6).abs() < 1e-12);
        // se(b) = sqrt(MSE / Sxx) = sqrt(0,8 / 10).
        assert!((r.coefs[1].se - (0.8f64 / 10.0).sqrt()).abs() < 1e-12);
        // se(b0) = sqrt(MSE (1/n + x̄²/Sxx)) = sqrt(0,8 (0,2 + 0,9)).
        assert!((r.coefs[0].se - (0.8f64 * 1.1).sqrt()).abs() < 1e-12);
        assert!((r.f - r.coefs[1].t.powi(2)).abs() < 1e-9);
        assert_eq!(r.coefs[1].vif, Some(1.0));
    }

    #[test]
    fn multiple_regression_recovers_the_plane() {
        // y = 3 + 2 x1 − 0,5 x2 + small noise.
        let x1: Vec<f64> = (0..20).map(|i| i as f64).collect();
        let x2: Vec<f64> = (0..20).map(|i| ((i * 7) % 11) as f64).collect();
        let y: Vec<f64> = (0..20)
            .map(|i| 3.0 + 2.0 * x1[i] - 0.5 * x2[i] + if i % 2 == 0 { 0.01 } else { -0.01 })
            .collect();
        let r = fit(&["x1".into(), "x2".into()], &[x1.clone(), x2], &y, 0.95).unwrap();
        assert!((r.coefs[1].estimate - 2.0).abs() < 1e-2);
        assert!((r.coefs[2].estimate + 0.5).abs() < 1e-2);
        assert!(r.r2 > 0.999 && r.coefs[1].p_value < 1e-10);
        let sum: f64 = r.residuals.iter().sum();
        assert!(sum.abs() < 1e-9);
        // Collinear predictors and too few rows are refused.
        let dup = x1.iter().map(|v| 2.0 * v).collect();
        assert!(fit(&["a".into(), "b".into()], &[x1.clone(), dup], &y, 0.95).is_err());
        assert!(fit(&["a".into()], &[vec![1.0, 2.0]], &[1.0, 2.0], 0.95).is_err());
        assert!(
            fit(
                &["a".into()],
                &[vec![1.0; 5]],
                &[1.0, 2.0, 3.0, 4.0, 5.0],
                0.95
            )
            .is_err()
        );
    }
}
