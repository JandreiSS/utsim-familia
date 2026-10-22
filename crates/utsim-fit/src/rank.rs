//! The published selection rule and the explanation of its outcome.
//!
//! Rule (shown verbatim in every report, see [`RULE`]):
//! 1. a family that cannot be fitted is listed with the reason and ranked last;
//! 2. a fitted family is **rejected** if any available goodness-of-fit test has p < α;
//! 3. accepted families come first, then rejected ones; within each group the lowest
//!    AIC wins, and ties are broken by the smaller Anderson-Darling statistic;
//! 4. if no family is accepted there is no winner and the empirical distribution is
//!    recommended;
//! 5. accepted families within ΔAIC < 2 of the lowest AIC are statistically
//!    equivalent; among them the one with fewer parameters wins (parsimony), and
//!    the lowest AIC breaks ties.

use serde::Serialize;
use utsim_core::Dist;

use crate::family::{Family, Fitted};
use crate::gof::{self, GofConfig, GofResult, TestStat};

pub const RULE: &str = "1) famílias que não se aplicam aos dados ficam por último; \
2) uma família é rejeitada se algum teste de aderência (KS, Anderson-Darling, qui-quadrado) tiver p < α; \
3) entre as não rejeitadas vence o menor AIC (empate: menor estatística de Anderson-Darling); \
4) se todas forem rejeitadas, recomenda-se a distribuição empírica; \
5) candidatas aceitas com ΔAIC < 2 em relação ao menor AIC são equivalentes; entre elas vence a de menos parâmetros (parcimônia).";

/// Outcome of one candidate, everything the report needs.
#[derive(Serialize)]
pub struct CandidateResult {
    pub family: String,
    pub n_params: usize,
    /// Why the family does not apply, when fitting failed.
    pub fit_error: Option<String>,
    pub params: Vec<(String, f64)>,
    pub dist: Option<Dist>,
    pub log_likelihood: f64,
    pub aic: f64,
    pub bic: f64,
    /// Relative likelihood among fitted candidates (sums to 1).
    pub akaike_weight: f64,
    pub gof: GofResult,
    /// Tests that rejected the family, e.g. "Anderson-Darling (p = 0,012)".
    pub rejected_by: Vec<String>,
    /// The fitted model, kept for plots; not serialised.
    #[serde(skip)]
    pub fitted: Option<Fitted>,
}

impl CandidateResult {
    /// Fits `family` and runs the goodness-of-fit tests.
    pub fn evaluate(family: &dyn Family, data: &[f64], config: &GofConfig) -> Self {
        match family.fit(data) {
            Ok(fitted) => {
                let gof = gof::evaluate(family, &fitted, data, config);
                CandidateResult::from_fit(fitted, gof, data)
            }
            Err(reason) => CandidateResult::failed(family.name(), family.n_params(), reason),
        }
    }

    pub fn from_fit(fitted: Fitted, gof: GofResult, data: &[f64]) -> Self {
        CandidateResult {
            family: fitted.family.to_string(),
            n_params: fitted.n_params,
            fit_error: None,
            params: fitted
                .params
                .iter()
                .map(|(k, v)| (k.to_string(), *v))
                .collect(),
            dist: fitted.dist.clone(),
            log_likelihood: fitted.log_likelihood(data),
            aic: fitted.aic(data),
            bic: fitted.bic(data),
            akaike_weight: 0.0,
            gof,
            rejected_by: Vec::new(),
            fitted: Some(fitted),
        }
    }

    pub fn failed(family: &str, n_params: usize, reason: String) -> Self {
        CandidateResult {
            family: family.to_string(),
            n_params,
            fit_error: Some(reason),
            params: Vec::new(),
            dist: None,
            log_likelihood: f64::NEG_INFINITY,
            aic: f64::INFINITY,
            bic: f64::INFINITY,
            akaike_weight: 0.0,
            gof: GofResult::default(),
            rejected_by: Vec::new(),
            fitted: None,
        }
    }

    pub fn accepted(&self) -> bool {
        self.fit_error.is_none() && self.rejected_by.is_empty()
    }

    fn tests(&self) -> [(&'static str, Option<&TestStat>); 3] {
        [
            ("Kolmogorov-Smirnov", self.gof.ks.as_ref()),
            ("Anderson-Darling", self.gof.anderson_darling.as_ref()),
            ("qui-quadrado", self.gof.chi_square.as_ref()),
        ]
    }

    fn group(&self) -> u8 {
        match (self.fit_error.is_some(), self.rejected_by.is_empty()) {
            (true, _) => 2,
            (false, false) => 1,
            (false, true) => 0,
        }
    }
}

#[derive(Serialize)]
pub struct Ranking {
    pub alpha: f64,
    pub rule: &'static str,
    /// All candidates, best first.
    pub candidates: Vec<CandidateResult>,
    /// Index of the winner in `candidates` (always 0 when present).
    pub best: Option<usize>,
    pub recommend_empirical: bool,
    /// Why the winner won, then one line per other candidate, in ranking order.
    pub explanation: Vec<String>,
    /// Candidates statistically equivalent to the winner (ΔAIC < 2).
    pub equivalent: Vec<String>,
}

pub fn rank(mut candidates: Vec<CandidateResult>, alpha: f64) -> Ranking {
    for c in &mut candidates {
        let rejected: Vec<String> = c
            .tests()
            .iter()
            .filter_map(|(name, t)| {
                t.filter(|t| t.p_value < alpha)
                    .map(|t| format!("{name} (p = {})", num(t.p_value)))
            })
            .collect();
        c.rejected_by = rejected;
    }

    let best_aic = candidates
        .iter()
        .filter(|c| c.fit_error.is_none())
        .map(|c| c.aic)
        .fold(f64::INFINITY, f64::min);
    let total: f64 = candidates
        .iter()
        .filter(|c| c.fit_error.is_none())
        .map(|c| (-(c.aic - best_aic) / 2.0).exp())
        .sum();
    for c in candidates.iter_mut().filter(|c| c.fit_error.is_none()) {
        c.akaike_weight = (-(c.aic - best_aic) / 2.0).exp() / total;
    }

    candidates.sort_by(|a, b| {
        a.group()
            .cmp(&b.group())
            .then(a.aic.total_cmp(&b.aic))
            .then_with(|| ad_stat(a).total_cmp(&ad_stat(b)))
            .then_with(|| a.family.cmp(&b.family))
    });

    // Rule 5: among the accepted candidates equivalent to the lowest AIC, the most
    // parsimonious wins. Accepted candidates are sorted by AIC, so the block is a prefix.
    let min_aic = candidates.first().filter(|c| c.accepted()).map(|c| c.aic);
    let mut parsimony = false;
    if let Some(min_aic) = min_aic {
        let block = candidates
            .iter()
            .take_while(|c| c.accepted() && c.aic - min_aic < 2.0)
            .count();
        let pick = (0..block)
            .min_by_key(|&i| candidates[i].n_params)
            .unwrap_or(0);
        parsimony = candidates[pick].n_params < candidates[0].n_params;
        let chosen = candidates.remove(pick);
        candidates.insert(0, chosen);
    }

    let best = min_aic.map(|_| 0);
    let mut explanation = Vec::new();
    let mut equivalent = Vec::new();
    match (best, min_aic) {
        (Some(_), Some(min_aic)) => {
            let w = &candidates[0];
            let why = if parsimony {
                format!(
                    "é estatisticamente equivalente à de menor AIC (ΔAIC = {}) e tem menos parâmetros ({}), então foi preferida por parcimônia",
                    num(w.aic - min_aic),
                    w.n_params
                )
            } else {
                format!("tem o menor AIC entre as aceitas (AIC = {})", num(w.aic))
            };
            explanation.push(format!(
                "{} foi a escolhida: {}; {} (peso de Akaike = {}%).",
                w.family,
                passed_tests(w, alpha),
                why,
                num(100.0 * w.akaike_weight)
            ));
            for c in candidates.iter().skip(1).filter(|c| c.accepted()) {
                let delta = c.aic - min_aic;
                if delta < 2.0 {
                    equivalent.push(c.family.clone());
                    explanation.push(format!(
                        "{}: também aceita e equivalente (ΔAIC = {}), mas com {} parâmetros contra {} da escolhida.",
                        c.family,
                        num(delta),
                        c.n_params,
                        w.n_params
                    ));
                } else {
                    explanation.push(format!(
                        "{}: também aceita, mas com AIC maior (ΔAIC = {}).",
                        c.family,
                        num(delta)
                    ));
                }
            }
        }
        _ => {
            let least_bad = candidates.iter().find(|c| c.fit_error.is_none());
            explanation.push(match least_bad {
                Some(c) => format!(
                    "Nenhuma família passou nos testes ao nível α = {}. Recomenda-se a distribuição empírica; \
                     a menos ruim foi {} (AIC = {}).",
                    num(alpha),
                    c.family,
                    num(c.aic)
                ),
                None => "Nenhuma família se aplica a estes dados. Recomenda-se a distribuição empírica.".to_string(),
            });
        }
    }
    for c in &candidates {
        if let Some(reason) = &c.fit_error {
            explanation.push(format!("{}: não se aplica ({reason}).", c.family));
        } else if !c.rejected_by.is_empty() {
            explanation.push(format!(
                "{}: rejeitada por {} < α = {}.",
                c.family,
                c.rejected_by.join(", "),
                num(alpha)
            ));
        }
    }

    Ranking {
        alpha,
        rule: RULE,
        recommend_empirical: best.is_none(),
        candidates,
        best,
        explanation,
        equivalent,
    }
}

fn ad_stat(c: &CandidateResult) -> f64 {
    c.gof
        .anderson_darling
        .as_ref()
        .map_or(f64::INFINITY, |t| t.statistic)
}

fn passed_tests(c: &CandidateResult, alpha: f64) -> String {
    let passed: Vec<String> = c
        .tests()
        .iter()
        .filter_map(|(name, t)| t.map(|t| format!("{name} p = {}", num(t.p_value))))
        .collect();
    if passed.is_empty() {
        "nenhum teste de aderência foi executado".to_string()
    } else {
        format!(
            "não foi rejeitada ao nível α = {} ({})",
            num(alpha),
            passed.join(", ")
        )
    }
}

/// Number formatted for Portuguese text (decimal comma, 3 significant decimals).
pub fn num(x: f64) -> String {
    if !x.is_finite() {
        return if x > 0.0 { "∞".into() } else { "-∞".into() };
    }
    let s = if x.abs() >= 100.0 {
        format!("{x:.1}")
    } else {
        format!("{x:.3}")
    };
    s.replace('.', ",")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cand(name: &str, aic: f64, ks_p: f64, ad: (f64, f64)) -> CandidateResult {
        let stat = |statistic, p_value| TestStat {
            statistic,
            p_value,
            method: "teste".into(),
        };
        CandidateResult {
            gof: GofResult {
                ks: Some(stat(0.1, ks_p)),
                anderson_darling: Some(stat(ad.0, ad.1)),
                chi_square: None,
            },
            aic,
            bic: aic,
            log_likelihood: -aic / 2.0,
            ..CandidateResult::failed(name, 2, String::new())
        }
        .with_fit_ok()
    }

    impl CandidateResult {
        fn with_fit_ok(mut self) -> Self {
            self.fit_error = None;
            self
        }
    }

    #[test]
    fn rejected_families_lose_even_with_lower_aic() {
        let r = rank(
            vec![
                cand("Weibull", 100.0, 0.30, (0.4, 0.01)),
                cand("Gama", 104.0, 0.50, (0.5, 0.40)),
                CandidateResult::failed("Beta", 4, "precisa de mínimo e máximo".into()),
            ],
            0.05,
        );
        assert_eq!(r.best, Some(0));
        assert_eq!(r.candidates[0].family, "Gama");
        assert_eq!(r.candidates[1].family, "Weibull");
        assert_eq!(r.candidates[2].family, "Beta");
        assert!(r.explanation[0].starts_with("Gama foi a escolhida"));
        assert!(
            r.explanation
                .iter()
                .any(|e| e.contains("Weibull: rejeitada por Anderson-Darling (p = 0,010)"))
        );
        assert!(
            r.explanation
                .iter()
                .any(|e| e.contains("Beta: não se aplica"))
        );
    }

    #[test]
    fn near_ties_are_flagged_and_weights_sum_to_one() {
        let r = rank(
            vec![
                cand("Lognormal", 50.0, 0.4, (0.3, 0.5)),
                cand("Gama", 51.0, 0.4, (0.2, 0.6)),
            ],
            0.05,
        );
        assert_eq!(r.candidates[0].family, "Lognormal");
        assert_eq!(r.equivalent, vec!["Gama".to_string()]);
        let w: f64 = r.candidates.iter().map(|c| c.akaike_weight).sum();
        assert!((w - 1.0).abs() < 1e-12);
    }

    #[test]
    fn parsimony_picks_fewer_parameters_among_equivalents() {
        let mut three = cand("Weibull deslocada", 100.0, 0.5, (0.3, 0.5));
        three.n_params = 3;
        let mut two = cand("Gama", 100.4, 0.6, (0.2, 0.6));
        two.n_params = 2;
        let r = rank(vec![three, two], 0.05);
        assert_eq!(r.candidates[0].family, "Gama");
        assert!(
            r.explanation[0].contains("parcimônia"),
            "{}",
            r.explanation[0]
        );
        assert_eq!(r.equivalent, vec!["Weibull deslocada".to_string()]);
        // Outside the ΔAIC < 2 band, AIC wins even with more parameters.
        let mut three = cand("Weibull deslocada", 100.0, 0.5, (0.3, 0.5));
        three.n_params = 3;
        let r = rank(vec![three, cand("Gama", 103.0, 0.6, (0.2, 0.6))], 0.05);
        assert_eq!(r.candidates[0].family, "Weibull deslocada");
    }

    #[test]
    fn all_rejected_recommends_empirical() {
        let r = rank(vec![cand("Normal", 10.0, 0.001, (2.0, 0.001))], 0.05);
        assert_eq!(r.best, None);
        assert!(r.recommend_empirical);
        assert!(r.explanation[0].contains("distribuição empírica"));
    }

    #[test]
    fn aic_tie_is_broken_by_anderson_darling() {
        let r = rank(
            vec![
                cand("A", 10.0, 0.5, (0.9, 0.5)),
                cand("B", 10.0, 0.5, (0.2, 0.5)),
            ],
            0.05,
        );
        assert_eq!(r.candidates[0].family, "B");
    }
}
