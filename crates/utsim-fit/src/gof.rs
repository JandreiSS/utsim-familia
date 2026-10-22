//! Goodness-of-fit tests (contract). Implemented by the "testes de aderência" front.

use serde::Serialize;

use crate::family::{Family, Fitted};

#[derive(Clone, Debug, Serialize)]
pub struct GofConfig {
    /// Parametric bootstrap replicates for KS and AD p-values (0 = asymptotic only).
    pub bootstrap: usize,
    pub seed: u64,
    /// Number of equiprobable classes for χ² (`None` = automatic rule).
    pub chi2_bins: Option<usize>,
}

impl Default for GofConfig {
    fn default() -> Self {
        GofConfig {
            bootstrap: 300,
            seed: 1,
            chi2_bins: None,
        }
    }
}

/// One test: statistic, p-value and how the p-value was obtained.
#[derive(Clone, Debug, Serialize)]
pub struct TestStat {
    pub statistic: f64,
    pub p_value: f64,
    /// e.g. "bootstrap paramétrico (300)", "assintótico".
    pub method: String,
}

#[derive(Clone, Debug, Default, Serialize)]
pub struct GofResult {
    pub ks: Option<TestStat>,
    pub anderson_darling: Option<TestStat>,
    pub chi_square: Option<TestStat>,
}

/// Runs all applicable tests for one fitted candidate. `family` is needed to refit
/// each bootstrap sample, since parameters estimated from the data bias plain KS/AD.
pub fn evaluate(
    family: &dyn Family,
    fitted: &Fitted,
    data: &[f64],
    config: &GofConfig,
) -> GofResult {
    let _ = (family, fitted, data, config);
    GofResult::default()
}
