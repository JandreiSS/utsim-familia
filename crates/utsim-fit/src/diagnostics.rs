//! Data diagnostics (contract). Implemented by the "diagnóstico dos dados" front.

use serde::Serialize;

#[derive(Clone, Debug, Default, Serialize)]
pub struct Descriptive {
    pub n: usize,
    pub mean: f64,
    pub median: f64,
    pub std_dev: f64,
    /// Coefficient of variation (std_dev / mean); ≈ 1 hints at an exponential.
    pub cv: f64,
    pub skewness: f64,
    pub excess_kurtosis: f64,
    pub min: f64,
    pub max: f64,
    pub q1: f64,
    pub q3: f64,
}

/// A hypothesis test on the data itself.
#[derive(Clone, Debug, Serialize)]
pub struct Check {
    pub name: String,
    pub statistic: f64,
    pub p_value: f64,
    /// Plain-language reading of the result, in Portuguese.
    pub verdict: String,
}

#[derive(Clone, Debug, Default, Serialize)]
pub struct Diagnostics {
    pub descriptive: Descriptive,
    /// Sample autocorrelation for lags 1..=10.
    pub autocorrelation: Vec<f64>,
    /// Runs test (independence) and Mann-Kendall (trend).
    pub checks: Vec<Check>,
    /// Indices of values outside the 1.5·IQR fences.
    pub outliers: Vec<usize>,
    /// Fraction of repeated values (rounded/discretised data).
    pub ties_fraction: f64,
    /// Warnings the report must show before the fitting results.
    pub warnings: Vec<String>,
}

pub fn diagnose(data: &[f64]) -> Diagnostics {
    Diagnostics {
        descriptive: Descriptive {
            n: data.len(),
            ..Default::default()
        },
        ..Default::default()
    }
}
