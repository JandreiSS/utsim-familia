//! Charts with memory: EWMA and tabular CUSUM. They accumulate small, persistent shifts
//! that a Shewhart chart only sees late (`docs/plano_gemeo_digital_e_painel.md`, E4), and
//! are the ones used on the residual series real − simulated of the digital twin.
//!
//! - EWMA: `zᵢ = λxᵢ + (1 − λ)zᵢ₋₁`, `z₀ = μ0`; limits
//!   `μ0 ± Lσ√(λ/(2 − λ)·[1 − (1 − λ)²ⁱ])` (exact) or without the bracket (asymptotic).
//! - CUSUM: `C⁺ᵢ = max(0, xᵢ − (μ0 + K) + C⁺ᵢ₋₁)`, `C⁻ᵢ = max(0, (μ0 − K) − xᵢ + C⁻ᵢ₋₁)`, with
//!   `K = kσ` and a signal when either passes `H = hσ`.
//!
//! `σ` is the standard deviation of the plotted value (`σ/√n` for subgroup means).

use serde::{Deserialize, Serialize};

use crate::charts::Panel;
use crate::rules::{Rule, Violation};

/// EWMA chart settings.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Ewma {
    pub lambda: f64,
    pub l: f64,
    pub target: f64,
    pub sigma: f64,
    /// Constant (asymptotic) limits instead of the exact ones that widen at the start.
    pub asymptotic: bool,
}

/// Tabular CUSUM settings (`k` and `h` in units of σ).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Cusum {
    pub k: f64,
    pub h: f64,
    pub target: f64,
    pub sigma: f64,
}

/// Result of an EWMA chart.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EwmaResult {
    pub z: Vec<f64>,
    pub lcl: Vec<f64>,
    pub ucl: Vec<f64>,
    /// Points beyond the limits.
    pub signals: Vec<usize>,
}

/// Result of a CUSUM chart.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CusumResult {
    pub upper: Vec<f64>,
    pub lower: Vec<f64>,
    /// `H = hσ`, the decision interval.
    pub h: f64,
    pub signals_up: Vec<usize>,
    pub signals_down: Vec<usize>,
    /// For each upward signal, the estimated new mean `μ0 + K + C⁺/N⁺` (`N⁺`: points
    /// since `C⁺` was last zero).
    pub shift_estimates: Vec<(usize, f64)>,
}

impl Ewma {
    pub fn validate(&self) -> Result<(), String> {
        if !(self.lambda > 0.0 && self.lambda <= 1.0) {
            return Err(format!(
                "λ do EWMA deve estar em (0, 1] (recebido {})",
                self.lambda
            ));
        }
        if !(self.l > 0.0 && self.sigma > 0.0) {
            return Err("L e σ do EWMA devem ser positivos".into());
        }
        Ok(())
    }

    /// Half-width of the limits at point `i` (0-based).
    pub fn half_width(&self, i: usize) -> f64 {
        let lam = self.lambda;
        let f = if self.asymptotic {
            1.0
        } else {
            1.0 - (1.0 - lam).powi(2 * (i as i32 + 1))
        };
        self.l * self.sigma * (lam / (2.0 - lam) * f).sqrt()
    }

    pub fn run(&self, x: &[f64]) -> Result<EwmaResult, String> {
        self.validate()?;
        let mut z = Vec::with_capacity(x.len());
        let (mut lcl, mut ucl, mut signals) = (vec![], vec![], vec![]);
        let mut prev = self.target;
        for (i, &v) in x.iter().enumerate() {
            prev = self.lambda * v + (1.0 - self.lambda) * prev;
            let hw = self.half_width(i);
            z.push(prev);
            lcl.push(self.target - hw);
            ucl.push(self.target + hw);
            if (prev - self.target).abs() > hw {
                signals.push(i);
            }
        }
        Ok(EwmaResult {
            z,
            lcl,
            ucl,
            signals,
        })
    }
}

impl EwmaResult {
    /// As a chart panel (for reports).
    pub fn panel(&self, target: f64) -> Panel {
        Panel {
            name: "EWMA".into(),
            values: self.z.iter().map(|&v| Some(v)).collect(),
            center: vec![target; self.z.len()],
            lcl: self.lcl.clone(),
            ucl: self.ucl.clone(),
            violations: self
                .signals
                .iter()
                .map(|&index| Violation {
                    rule: Rule::Beyond3,
                    index,
                })
                .collect(),
        }
    }
}

impl Cusum {
    pub fn validate(&self) -> Result<(), String> {
        if !(self.k >= 0.0 && self.h > 0.0 && self.sigma > 0.0) {
            return Err("k ≥ 0, h > 0 e σ > 0 no CUSUM".into());
        }
        Ok(())
    }

    pub fn run(&self, x: &[f64]) -> Result<CusumResult, String> {
        self.validate()?;
        let k = self.k * self.sigma;
        let h = self.h * self.sigma;
        let (mut cp, mut cm) = (0.0f64, 0.0f64);
        let mut np = 0usize;
        let mut out = CusumResult {
            upper: vec![],
            lower: vec![],
            h,
            signals_up: vec![],
            signals_down: vec![],
            shift_estimates: vec![],
        };
        for (i, &v) in x.iter().enumerate() {
            cp = (v - (self.target + k) + cp).max(0.0);
            cm = ((self.target - k) - v + cm).max(0.0);
            np = if cp > 0.0 { np + 1 } else { 0 };
            out.upper.push(cp);
            out.lower.push(cm);
            if cp > h {
                out.signals_up.push(i);
                out.shift_estimates
                    .push((i, self.target + k + cp / np.max(1) as f64));
            }
            if cm > h {
                out.signals_down.push(i);
            }
        }
        Ok(out)
    }
}

impl CusumResult {
    /// The two sums as chart panels (`C⁺` and `C⁻`, limit `H`).
    pub fn panels(&self) -> Vec<Panel> {
        let make = |name: &str, v: &[f64], s: &[usize]| Panel {
            name: name.into(),
            values: v.iter().map(|&x| Some(x)).collect(),
            center: vec![0.0; v.len()],
            lcl: vec![0.0; v.len()],
            ucl: vec![self.h; v.len()],
            violations: s
                .iter()
                .map(|&index| Violation {
                    rule: Rule::Beyond3,
                    index,
                })
                .collect(),
        };
        vec![
            make("CUSUM C⁺ (alta)", &self.upper, &self.signals_up),
            make("CUSUM C⁻ (baixa)", &self.lower, &self.signals_down),
        ]
    }
}

/// Residuals real − simulated, standardised by the spread of the simulated KPI across
/// replications: the input of EWMA and CUSUM for the twin's deviation monitor
/// (`docs/plano_gemeo_digital_e_painel.md`, T8). `simulated_mean[i]` and `simulated_sd[i]`
/// are the mean and standard deviation across replications for the same period as
/// `real[i]`. Under a valid model the result has mean 0 and σ ≈ 1.
pub fn standardized_residuals(
    real: &[f64],
    simulated_mean: &[f64],
    simulated_sd: &[f64],
) -> Result<Vec<f64>, String> {
    if real.len() != simulated_mean.len() || real.len() != simulated_sd.len() {
        return Err("séries real e simulada com comprimentos diferentes".into());
    }
    real.iter()
        .zip(simulated_mean)
        .zip(simulated_sd)
        .enumerate()
        .map(|(i, ((r, m), s))| {
            if *s > 0.0 {
                Ok((r - m) / s)
            } else {
                Err(format!("desvio simulado nulo no período {}", i + 1))
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ewma_by_hand() {
        let e = Ewma {
            lambda: 0.2,
            l: 3.0,
            target: 10.0,
            sigma: 1.0,
            asymptotic: false,
        };
        let r = e.run(&[11.0, 9.0, 12.0]).unwrap();
        // z1 = 0.2·11 + 0.8·10 = 10.2; z2 = 1.8 + 8.16 = 9.96; z3 = 2.4 + 7.968 = 10.368.
        assert!((r.z[0] - 10.2).abs() < 1e-12);
        assert!((r.z[1] - 9.96).abs() < 1e-12);
        assert!((r.z[2] - 10.368).abs() < 1e-12);
        // First half-width: 3·√(0.2/1.8·(1 − 0.64)) = 3·0.2 = 0.6.
        assert!((r.ucl[0] - 10.6).abs() < 1e-12);
        // Asymptotic: 3·√(1/9) = 1.
        let a = Ewma {
            asymptotic: true,
            ..e
        };
        assert!((a.half_width(50) - 1.0).abs() < 1e-12);
        assert!((e.half_width(500) - 1.0).abs() < 1e-12);
    }

    #[test]
    fn cusum_by_hand() {
        let c = Cusum {
            k: 0.5,
            h: 4.0,
            target: 10.0,
            sigma: 1.0,
        };
        let r = c.run(&[10.0, 11.0, 12.0, 11.5, 12.0, 11.0, 12.5]).unwrap();
        // C⁺: 0, 0.5, 2.0, 3.0, 4.5 (signal), 5.0, 7.0.
        let want = [0.0, 0.5, 2.0, 3.0, 4.5, 5.0, 7.0];
        for (g, w) in r.upper.iter().zip(want) {
            assert!((g - w).abs() < 1e-12);
        }
        assert_eq!(r.signals_up, vec![4, 5, 6]);
        assert!(r.signals_down.is_empty());
        // New mean at the first signal: 10 + 0.5 + 4.5/4 = 11.625.
        assert!((r.shift_estimates[0].1 - 11.625).abs() < 1e-12);
    }

    #[test]
    fn residuals_are_standardised() {
        let r = standardized_residuals(&[12.0, 8.0], &[10.0, 10.0], &[2.0, 1.0]).unwrap();
        assert_eq!(r, vec![1.0, -2.0]);
        assert!(standardized_residuals(&[1.0], &[1.0], &[0.0]).is_err());
    }
}
