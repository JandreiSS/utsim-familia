//! Full analysis and its presentation (contract). Implemented by the "relatório" front.

use serde::Serialize;

use crate::diagnostics::Diagnostics;
use crate::gof::GofConfig;
use crate::rank::Ranking;

#[derive(Clone, Debug, Serialize)]
pub struct FitConfig {
    /// Significance level for rejecting a candidate.
    pub alpha: f64,
    pub gof: GofConfig,
}

impl Default for FitConfig {
    fn default() -> Self {
        FitConfig {
            alpha: 0.05,
            gof: GofConfig::default(),
        }
    }
}

/// Everything `utsim fit` shows: diagnostics, every candidate, the ranking and why.
#[derive(Serialize)]
pub struct FitReport {
    pub dataset: String,
    pub config: FitConfig,
    pub diagnostics: Diagnostics,
    pub ranking: Ranking,
}

/// Runs the whole pipeline on one sample.
pub fn analyze(dataset: &str, data: &[f64], config: &FitConfig) -> FitReport {
    let diagnostics = crate::diagnostics::diagnose(data);
    let candidates = crate::families::all()
        .iter()
        .map(|family| crate::rank::CandidateResult::evaluate(family.as_ref(), data, &config.gof))
        .collect();
    let ranking = crate::rank::rank(candidates, config.alpha);
    FitReport {
        dataset: dataset.to_string(),
        config: config.clone(),
        diagnostics,
        ranking,
    }
}
