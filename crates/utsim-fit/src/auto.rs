//! Automatic choice of a distribution from real data, for any time field of a model.
//!
//! This is the pure API behind "valores reais" in the model (`utsim-model`, `data: [..]`),
//! the `utsim dados` command and the interface: it receives a vector of observations and
//! returns the ranking of every candidate family, the chosen distribution and a report.
//!
//! Steps (all deterministic, the bootstrap uses a fixed seed):
//! 1. the support of the data (integer, non-negative, positive, range) decides which
//!    families apply: discrete families need integers, the others refuse what they
//!    cannot model (negative values, for instance) and say why;
//! 2. every family that applies is fitted and tested ([`crate::report::analyze`]'s
//!    pipeline), and the published rule of [`crate::rank`] ranks them;
//! 3. the candidates are ordered by the chosen [`Criterion`] (accepted ones first);
//! 4. [`Choice`] picks the distribution: the best accepted family, a family fixed by the
//!    user, or the empirical distribution. When no family is accepted the empirical
//!    distribution is used, with a warning;
//! 5. problems with the data are reported in plain language: few points, trend and
//!    autocorrelation (from [`crate::diagnostics`]), mixture, outliers, rounding ties.

use serde::{Deserialize, Serialize};
use utsim_core::Dist;

use crate::rank::{CandidateResult, num};
use crate::report::{FitConfig, FitReport, HTML_CSS, empirical, esc};

/// How the distribution of a field is chosen from its data.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Choice {
    /// The best accepted family by the criterion (empirical when none is accepted).
    #[default]
    Best,
    /// This family, by name ("Lognormal", "Gama", "Weibull"...), even when rejected.
    Family(String),
    /// The empirical distribution of the observations themselves.
    Empirical,
}

/// What orders the accepted candidates.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Criterion {
    /// Akaike information criterion, with the parsimony rule of [`crate::rank::RULE`].
    #[default]
    Aic,
    /// Bayesian information criterion (penalises parameters more).
    Bic,
    /// Smallest Kolmogorov-Smirnov distance.
    Ks,
    /// Smallest Anderson-Darling statistic (more weight on the tails).
    Ad,
}

impl Criterion {
    pub fn label(self) -> &'static str {
        match self {
            Criterion::Aic => "AIC",
            Criterion::Bic => "BIC",
            Criterion::Ks => "Kolmogorov-Smirnov",
            Criterion::Ad => "Anderson-Darling",
        }
    }

    /// Value of the criterion for a candidate (lower is better, ∞ when it does not apply).
    pub fn value(self, c: &CandidateResult) -> f64 {
        let stat =
            |t: &Option<crate::gof::TestStat>| t.as_ref().map_or(f64::INFINITY, |t| t.statistic);
        match self {
            Criterion::Aic => c.aic,
            Criterion::Bic => c.bic,
            Criterion::Ks => stat(&c.gof.ks),
            Criterion::Ad => stat(&c.gof.anderson_darling),
        }
    }
}

/// Settings of one automatic fit.
#[derive(Clone, Debug, Default)]
pub struct AutoConfig {
    pub choose: Choice,
    pub criterion: Criterion,
    pub fit: FitConfig,
}

/// What the values look like, which decides the candidate families.
#[derive(Clone, Debug, Serialize)]
pub struct Support {
    pub n: usize,
    pub integer: bool,
    pub nonnegative: bool,
    pub positive: bool,
    pub min: f64,
    pub max: f64,
    /// One sentence for people.
    pub description: String,
}

impl Support {
    pub fn of(data: &[f64]) -> Support {
        let n = data.len();
        let min = data.iter().copied().fold(f64::INFINITY, f64::min);
        let max = data.iter().copied().fold(f64::NEG_INFINITY, f64::max);
        let integer = n > 0 && data.iter().all(|x| x.fract() == 0.0);
        let nonnegative = n > 0 && min >= 0.0;
        let positive = n > 0 && min > 0.0;
        let kind = if integer {
            "inteiros (famílias discretas e contínuas são candidatas)"
        } else {
            "contínuos (famílias discretas não se aplicam)"
        };
        let sign = if positive {
            "todos positivos"
        } else if nonnegative {
            "não negativos, com zeros (famílias que exigem valores > 0 ficam de fora)"
        } else {
            "com valores negativos (famílias de suporte positivo ficam de fora)"
        };
        let description = if n == 0 {
            "sem dados".to_string()
        } else {
            format!(
                "{n} valores {kind}, {sign}, entre {} e {} (as famílias limitadas, uniforme, \
                 triangular e beta, usam esse intervalo).",
                num(min),
                num(max)
            )
        };
        Support {
            n,
            integer,
            nonnegative,
            positive,
            min,
            max,
            description,
        }
    }
}

/// Everything about the automatic choice for one sample.
#[derive(Serialize)]
pub struct AutoFit {
    /// Name of the sample (the model field, or the file and column).
    pub name: String,
    /// Hash of the values, in order (see [`data_hash`]).
    pub hash: String,
    pub support: Support,
    pub choose: Choice,
    pub criterion: Criterion,
    /// Indices into `report.ranking.candidates`, in the order of the criterion
    /// (accepted first, then rejected, then the ones that do not apply).
    pub order: Vec<usize>,
    /// Index of the chosen family in `report.ranking.candidates`; `None` = empirical.
    pub chosen: Option<usize>,
    /// Name of the chosen family ("Empírica" for the empirical distribution).
    pub family: String,
    pub dist: Dist,
    /// No family was accepted and the empirical distribution was used instead.
    pub empirical_fallback: bool,
    /// Why this distribution, in plain language, one sentence per line.
    pub justification: Vec<String>,
    /// Problems with the data or with the choice.
    pub warnings: Vec<String>,
    pub report: FitReport,
}

/// Name used for the empirical distribution.
pub const EMPIRICAL: &str = "Empírica";

/// Stable hash of the values, in order: FNV-1a (64 bits) over the IEEE bits of each
/// value. Formatting does not matter ("1,50" and "1.5" hash the same); any change in a
/// value, in the order or in the number of values changes it.
pub fn data_hash(data: &[f64]) -> String {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for x in data {
        // -0.0 and 0.0 are the same observation.
        let bits = if *x == 0.0 { 0u64 } else { x.to_bits() };
        for b in bits.to_le_bytes() {
            h ^= u64::from(b);
            h = h.wrapping_mul(0x0000_0100_0000_01b3);
        }
    }
    format!("fnv1a64:{h:016x}")
}

/// Lower case, without accents, spaces or punctuation, with English aliases, so that
/// "LogNormal", "lognormal" and "Lognormal" name the same family.
fn normalize(name: &str) -> String {
    let plain: String = name
        .to_lowercase()
        .chars()
        .map(|c| match c {
            'á' | 'à' | 'â' | 'ã' => 'a',
            'é' | 'ê' => 'e',
            'í' => 'i',
            'ó' | 'ô' | 'õ' => 'o',
            'ú' | 'ü' => 'u',
            'ç' => 'c',
            c => c,
        })
        .filter(|c| c.is_alphanumeric())
        .collect();
    let alias = match plain.as_str() {
        "exp" | "exponential" => "exponencial",
        "gamma" => "gama",
        "uniform" => "uniforme",
        "loglogistic" => "loglogistica",
        "pearson5" | "inversegamma" => "pearsonv",
        "pearson6" | "betaprime" => "pearsonvi",
        "geometric" => "geometrica",
        "negbinomial" | "negativebinomial" => "binomialnegativa",
        "shiftedexponential" | "shiftedexp" => "exponencialdeslocada",
        "shiftedweibull" => "weibulldeslocada",
        "shiftedgamma" => "gamadeslocada",
        "shiftedlognormal" => "lognormaldeslocada",
        "empirical" => "empirica",
        other => return other.to_string(),
    };
    alias.to_string()
}

/// Every candidate family, with the names to show in error messages.
pub fn family_names() -> Vec<&'static str> {
    crate::families::all().iter().map(|f| f.name()).collect()
}

/// Fits every family that applies to `data` and ranks them (no choice yet).
fn evaluate_all(data: &[f64], support: &Support, config: &FitConfig) -> Vec<CandidateResult> {
    crate::families::all()
        .iter()
        .map(|family| {
            if family.is_discrete() && !support.integer {
                CandidateResult::failed(
                    family.name(),
                    family.n_params(),
                    "família discreta e os dados não são todos inteiros".into(),
                )
            } else {
                CandidateResult::evaluate(family.as_ref(), data, &config.gof)
            }
        })
        .collect()
}

fn group(c: &CandidateResult) -> u8 {
    if c.fit_error.is_some() {
        2
    } else if c.accepted() {
        0
    } else {
        1
    }
}

/// Runs the automatic choice. `Err` only when there is nothing to fit (no values,
/// values that are not numbers) or when the family asked for by name does not exist
/// or does not apply to these data.
pub fn auto_fit(name: &str, data: &[f64], config: &AutoConfig) -> Result<AutoFit, String> {
    if data.is_empty() {
        return Err(format!("{name}: não há valores para ajustar"));
    }
    if data.iter().any(|x| !x.is_finite()) {
        return Err(format!("{name}: há valores que não são números finitos"));
    }
    let support = Support::of(data);
    let candidates = evaluate_all(data, &support, &config.fit);
    let ranking = crate::rank::rank(candidates, config.fit.alpha);
    let report = FitReport {
        dataset: name.to_string(),
        config: config.fit.clone(),
        diagnostics: crate::diagnostics::diagnose(data),
        ranking,
        data: data.to_vec(),
    };
    let cands = &report.ranking.candidates;

    // Order by the criterion. AIC keeps the published rule (with parsimony) as is.
    let mut order: Vec<usize> = (0..cands.len()).collect();
    if config.criterion != Criterion::Aic {
        order.sort_by(|&a, &b| {
            let (ca, cb) = (&cands[a], &cands[b]);
            group(ca)
                .cmp(&group(cb))
                .then(
                    config
                        .criterion
                        .value(ca)
                        .total_cmp(&config.criterion.value(cb)),
                )
                .then(a.cmp(&b))
        });
    }
    let best_accepted = order.iter().copied().find(|&i| cands[i].accepted());

    let mut warnings: Vec<String> = report.diagnostics.warnings.clone();
    warnings.extend(extra_warnings(data, &report));
    let mut justification = Vec::new();
    let alpha = num(config.fit.alpha);

    let (chosen, empirical_fallback) = match &config.choose {
        Choice::Best => match best_accepted {
            Some(i) => {
                justification.extend(best_reason(&report, &order, i, config.criterion));
                (Some(i), false)
            }
            None => {
                let least_bad = order.iter().find(|&&i| cands[i].fit_error.is_none());
                justification.push(match least_bad {
                    Some(&i) => format!(
                        "Nenhuma família passou nos testes de aderência ao nível α = {alpha}; a \
                         menos ruim pelo critério {} foi {}. Foi usada a distribuição empírica, \
                         que reproduz os próprios valores observados.",
                        config.criterion.label(),
                        cands[i].family
                    ),
                    None => "Nenhuma família se aplica a estes dados. Foi usada a distribuição \
                             empírica, que reproduz os próprios valores observados."
                        .to_string(),
                });
                warnings.push(format!(
                    "Nenhuma família foi aceita (α = {alpha}): o campo usa a distribuição \
                     empírica. Ela não gera valores fora do intervalo observado; com mais dados, \
                     ou separando grupos distintos, uma família pode passar a servir."
                ));
                (None, true)
            }
        },
        Choice::Family(wanted) => {
            let key = normalize(wanted);
            let Some(i) = cands.iter().position(|c| normalize(&c.family) == key) else {
                return Err(format!(
                    "{name}: família '{wanted}' desconhecida; as candidatas são: {}",
                    family_names().join(", ")
                ));
            };
            if let Some(reason) = &cands[i].fit_error {
                return Err(format!(
                    "{name}: a família {} não se aplica a estes dados ({reason})",
                    cands[i].family
                ));
            }
            let position = order.iter().position(|&j| j == i).unwrap_or(0) + 1;
            justification.push(format!(
                "Família fixada no modelo: {}. Pelo critério {} ela ficou em {position}º lugar{}.",
                cands[i].family,
                config.criterion.label(),
                match best_accepted {
                    Some(b) if b != i => format!("; a melhor aceita seria {}", cands[b].family),
                    _ => String::new(),
                }
            ));
            if cands[i].accepted() {
                justification.push(format!(
                    "{} não foi rejeitada pelos testes de aderência (α = {alpha}).",
                    cands[i].family
                ));
            } else {
                warnings.push(format!(
                    "A família fixada ({}) foi rejeitada por {} < α = {alpha}: ela não descreve \
                     bem estes dados.",
                    cands[i].family,
                    cands[i].rejected_by.join(", ")
                ));
            }
            (Some(i), false)
        }
        Choice::Empirical => {
            justification.push(format!(
                "Distribuição empírica pedida no modelo: reproduz os próprios valores observados.{}",
                match best_accepted {
                    Some(b) => format!(
                        " A melhor família aceita pelo critério {} seria {}.",
                        config.criterion.label(),
                        cands[b].family
                    ),
                    None => String::new(),
                }
            ));
            (None, false)
        }
    };

    let (family, dist) = match chosen.and_then(|i| cands[i].dist.clone().map(|d| (i, d))) {
        Some((i, d)) => (cands[i].family.clone(), d),
        None => {
            if let Some(i) = chosen {
                warnings.push(format!(
                    "{} não tem equivalente nas distribuições do UTSim: foi usada a empírica.",
                    cands[i].family
                ));
            }
            let d = empirical(data).expect("non-empty, finite data");
            (EMPIRICAL.to_string(), d)
        }
    };
    let chosen = chosen.filter(|_| family != EMPIRICAL);

    Ok(AutoFit {
        name: name.to_string(),
        hash: data_hash(data),
        support,
        choose: config.choose.clone(),
        criterion: config.criterion,
        order,
        chosen,
        family,
        dist,
        empirical_fallback,
        justification,
        warnings,
        report,
    })
}

/// Why `winner` is the best accepted candidate, then one line per other candidate.
fn best_reason(report: &FitReport, order: &[usize], winner: usize, crit: Criterion) -> Vec<String> {
    let cands = &report.ranking.candidates;
    if crit == Criterion::Aic {
        // The published rule already explains the winner and every other candidate.
        return report.ranking.explanation.clone();
    }
    let w = &cands[winner];
    let accepted = order.iter().filter(|&&i| cands[i].accepted()).count();
    let mut out = vec![format!(
        "{} foi a escolhida: entre as {accepted} famílias não rejeitadas pelos testes de \
         aderência (α = {}), tem o menor valor de {} ({}); AIC = {}.",
        w.family,
        num(report.ranking.alpha),
        crit.label(),
        num(crit.value(w)),
        num(w.aic)
    )];
    if crit == Criterion::Ad && !w.gof.anderson_darling.is_some() {
        out.push(
            "O teste de Anderson-Darling não se aplica a famílias discretas; entre elas valeu a \
             ordem do AIC."
                .into(),
        );
    }
    for &i in order.iter().filter(|&&i| i != winner) {
        let c = &cands[i];
        if c.accepted() {
            out.push(format!(
                "{}: também aceita, com {} = {}.",
                c.family,
                crit.label(),
                num(crit.value(c))
            ));
        }
    }
    out.extend(
        report
            .ranking
            .explanation
            .iter()
            .filter(|e| e.contains(": rejeitada por") || e.contains(": não se aplica"))
            .cloned(),
    );
    out
}

/// Checks not covered by [`crate::diagnostics`]: very few points, mixture, extremes.
fn extra_warnings(data: &[f64], report: &FitReport) -> Vec<String> {
    let mut out = Vec::new();
    let n = data.len();
    if n < 10 {
        out.push(format!(
            "Muito poucos pontos (n = {n}): nenhum teste consegue distinguir as famílias. \
             Colete pelo menos 30 valores antes de confiar na escolha."
        ));
    }
    if let Some(modes) = modes(data).filter(|m| *m >= 2) {
        out.push(format!(
            "A densidade dos dados tem {modes} picos separados: parece uma mistura de grupos \
             (por exemplo, produtos, turnos ou operadores diferentes). Considere separar os \
             grupos e ajustar cada um."
        ));
    }
    let d = &report.diagnostics.descriptive;
    let iqr = d.q3 - d.q1;
    if n >= 4 && iqr > 0.0 {
        let extreme = data
            .iter()
            .filter(|&&x| x < d.q1 - 3.0 * iqr || x > d.q3 + 3.0 * iqr)
            .count();
        if extreme > 0 {
            out.push(format!(
                "{extreme} valor(es) muito afastado(s) do resto (além de 3·IQR dos quartis): \
                 confira se não são erros de anotação ou eventos de outra natureza."
            ));
        }
    }
    out
}

/// Number of clear peaks of a kernel density estimate (Silverman's bandwidth, which
/// oversmooths and so only shows well separated groups). `None` below 30 points.
fn modes(data: &[f64]) -> Option<usize> {
    let n = data.len();
    if n < 30 {
        return None;
    }
    let mut v = data.to_vec();
    v.sort_by(f64::total_cmp);
    let nf = n as f64;
    let mean = v.iter().sum::<f64>() / nf;
    let sd = (v.iter().map(|x| (x - mean).powi(2)).sum::<f64>() / (nf - 1.0)).sqrt();
    let q = |p: f64| v[((nf - 1.0) * p).round() as usize];
    let spread = sd.min((q(0.75) - q(0.25)) / 1.34);
    if spread.is_nan() || spread <= 0.0 {
        return None;
    }
    let h = 0.9 * spread * nf.powf(-0.2);
    const GRID: usize = 256;
    let (lo, hi) = (v[0] - h, v[n - 1] + h);
    let step = (hi - lo) / (GRID - 1) as f64;
    let dens: Vec<f64> = (0..GRID)
        .map(|g| {
            let x = lo + step * g as f64;
            v.iter()
                .map(|xi| (-0.5 * ((x - xi) / h).powi(2)).exp())
                .sum::<f64>()
        })
        .collect();
    let top = dens.iter().copied().fold(0.0, f64::max);
    let peaks: Vec<usize> = (1..GRID - 1)
        .filter(|&g| dens[g] > dens[g - 1] && dens[g] >= dens[g + 1] && dens[g] >= 0.25 * top)
        .collect();
    // Count peaks separated from the previous kept one by a deep valley.
    let mut kept = vec![*peaks.first()?];
    for &p in &peaks[1..] {
        let last = *kept.last().expect("not empty");
        let valley = dens[last..=p].iter().copied().fold(f64::INFINITY, f64::min);
        if valley < 0.6 * dens[last].min(dens[p]) {
            kept.push(p);
        } else if dens[p] > dens[last] {
            *kept.last_mut().expect("not empty") = p;
        }
    }
    Some(kept.len())
}

/// The candidate's parameters as text.
fn params_text(c: &CandidateResult) -> String {
    c.params
        .iter()
        .map(|(k, v)| format!("{k} = {}", num(*v)))
        .collect::<Vec<_>>()
        .join(", ")
}

/// Situation of a candidate in the automatic choice.
fn situation(fit: &AutoFit, i: usize) -> &'static str {
    let c = &fit.report.ranking.candidates[i];
    if c.fit_error.is_some() {
        "não se aplica"
    } else if fit.chosen == Some(i) {
        "escolhida"
    } else if c.accepted() {
        "aceita"
    } else {
        "rejeitada"
    }
}

/// CSV cell: quoted when needed, numbers with a decimal point.
fn cell(s: &str) -> String {
    if s.contains([',', '"', '\n', '\r', ';']) {
        format!("\"{}\"", s.replace('"', "\"\""))
    } else {
        s.to_string()
    }
}

fn opt_num(x: Option<f64>) -> String {
    x.filter(|v| v.is_finite())
        .map_or(String::new(), |v| v.to_string())
}

impl AutoFit {
    /// The chosen distribution as one line of RON, ready for a model.
    pub fn ron(&self) -> String {
        ron::ser::to_string(&self.dist).unwrap_or_default()
    }

    /// One row per candidate in the order of the criterion (comma separated, decimal point).
    pub fn to_csv(&self) -> String {
        let mut out = String::from(
            "posicao,familia,parametros,n_parametros,log_verossimilhanca,aic,bic,\
             ks_estatistica,ks_p,ad_estatistica,ad_p,qui2_estatistica,qui2_p,situacao,motivo\n",
        );
        for (pos, &i) in self.order.iter().enumerate() {
            let c = &self.report.ranking.candidates[i];
            let fitted = c.fit_error.is_none();
            let t = |t: &Option<crate::gof::TestStat>| {
                (
                    opt_num(t.as_ref().map(|t| t.statistic)),
                    opt_num(t.as_ref().map(|t| t.p_value)),
                )
            };
            let (ks, ks_p) = t(&c.gof.ks);
            let (ad, ad_p) = t(&c.gof.anderson_darling);
            let (chi, chi_p) = t(&c.gof.chi_square);
            let reason = c
                .fit_error
                .clone()
                .unwrap_or_else(|| c.rejected_by.join("; "));
            let row = [
                (pos + 1).to_string(),
                c.family.clone(),
                c.params
                    .iter()
                    .map(|(k, v)| format!("{k}={v}"))
                    .collect::<Vec<_>>()
                    .join(" "),
                c.n_params.to_string(),
                opt_num(fitted.then_some(c.log_likelihood)),
                opt_num(fitted.then_some(c.aic)),
                opt_num(fitted.then_some(c.bic)),
                ks,
                ks_p,
                ad,
                ad_p,
                chi,
                chi_p,
                situation(self, i).to_string(),
                reason,
            ];
            let cells: Vec<String> = row.iter().map(|s| cell(s)).collect();
            out.push_str(&cells.join(","));
            out.push('\n');
        }
        if self.chosen.is_none() {
            out.push_str(&format!(
                ",{EMPIRICAL},{},{},,,,,,,,,,escolhida,\n",
                cell(&format!("{} valores observados", self.support.n)),
                self.support.n
            ));
        }
        out
    }

    /// The summary shown on top of the full fit report: choice, justification, warnings.
    fn summary_html(&self) -> String {
        let mut h = String::new();
        h.push_str("<h2>Escolha para o campo</h2>");
        h.push_str(&format!(
            "<p>Distribuição usada: <b>{}</b> (escolha: {}; critério: {}).</p><pre>{}</pre>",
            esc(&self.family),
            esc(match &self.choose {
                Choice::Best => "a melhor família".to_string(),
                Choice::Family(f) => format!("família fixada, {f}"),
                Choice::Empirical => "empírica".to_string(),
            }
            .as_str()),
            esc(self.criterion.label()),
            esc(&self.ron())
        ));
        h.push_str("<h3>Justificativa</h3><ul>");
        for j in &self.justification {
            h.push_str(&format!("<li>{}</li>", esc(j)));
        }
        h.push_str("</ul><h3>Avisos</h3>");
        if self.warnings.is_empty() {
            h.push_str("<p class=\"ok\">Nenhum aviso sobre os dados.</p>");
        }
        for w in &self.warnings {
            h.push_str(&format!("<p class=\"warn\">{}</p>", esc(w)));
        }
        h.push_str(&format!(
            "<p>Suporte: {}</p><p class=\"sub\">Hash dos dados: <code>{}</code>. O ajuste é \
             determinístico: os mesmos dados e as mesmas opções dão sempre a mesma escolha.</p>",
            esc(&self.support.description),
            esc(&self.hash)
        ));
        h.push_str("<h3>Candidatas pelo critério</h3><div class=\"scroll\"><table><thead><tr>");
        for t in [
            "pos.",
            "família",
            "parâmetros",
            self.criterion.label(),
            "AIC",
            "BIC",
            "KS p",
            "AD p",
            "χ² p",
            "situação",
        ] {
            h.push_str(&format!("<th>{}</th>", esc(t)));
        }
        h.push_str("</tr></thead><tbody>");
        let p =
            |t: &Option<crate::gof::TestStat>| t.as_ref().map_or("-".into(), |t| num(t.p_value));
        for (pos, &i) in self.order.iter().enumerate() {
            let c = &self.report.ranking.candidates[i];
            let s = situation(self, i);
            let fitted = c.fit_error.is_none();
            let f = |x: f64| if fitted { num(x) } else { "-".into() };
            let cells = [
                (pos + 1).to_string(),
                c.family.clone(),
                if fitted { params_text(c) } else { "-".into() },
                f(self.criterion.value(c)),
                f(c.aic),
                f(c.bic),
                p(&c.gof.ks),
                p(&c.gof.anderson_darling),
                p(&c.gof.chi_square),
                s.to_string(),
            ];
            let class = match s {
                "escolhida" => "best",
                "rejeitada" => "rej",
                "não se aplica" => "na",
                _ => "acc",
            };
            h.push_str(&format!("<tr class=\"{class}\">"));
            for c in cells {
                h.push_str(&format!("<td>{}</td>", esc(&c)));
            }
            h.push_str("</tr>");
        }
        h.push_str("</tbody></table></div>");
        h.push_str(&crate::compare::comparison_html(self));
        h
    }

    /// Self-contained HTML report: the choice and its justification on top, then the
    /// full analysis (descriptive statistics, diagnostics, every candidate with its
    /// parameters, criteria and p-values, histogram with the candidate densities, Q-Q,
    /// P-P and ECDF plots).
    pub fn to_html(&self, command: &str) -> String {
        self.report.to_html_with(&self.summary_html(), command)
    }

    pub fn to_json(&self) -> String {
        serde_json::to_string_pretty(self).expect("auto fit serializes")
    }
}

/// One line of the index of a model's fits.
pub struct IndexEntry {
    /// The field, e.g. "Torno.process".
    pub field: String,
    /// Where the data came from ("dados/tempos.csv (processo)", "valores no modelo").
    pub source: String,
    /// The chosen family, or the error that prevented the fit.
    pub result: Result<(String, String), String>,
    pub warnings: usize,
    /// Relative link to the field's report.
    pub href: Option<String>,
}

/// Self-contained index page linking the report of every field with data.
pub fn index_html(title: &str, entries: &[IndexEntry]) -> String {
    let mut h = String::new();
    h.push_str(
        "<!doctype html>\n<html lang=\"pt-BR\"><head><meta charset=\"utf-8\">\
         <meta name=\"viewport\" content=\"width=device-width, initial-scale=1\">\n",
    );
    h.push_str(&format!(
        "<title>Dados reais: {}</title>\n<style>{HTML_CSS}</style></head><body><main>\n",
        esc(title)
    ));
    h.push_str(&format!(
        "<h1>Dados reais e ajuste automático</h1><p class=\"sub\">{}</p>\n",
        esc(title)
    ));
    h.push_str(&format!(
        "<p>{} campo(s) do modelo usam valores observados. Cada linha leva ao relatório \
         completo do ajuste daquele campo.</p>",
        entries.len()
    ));
    h.push_str(
        "<div class=\"scroll\"><table><thead><tr><th>campo</th><th>dados</th>\
         <th>distribuição</th><th>RON</th><th>avisos</th></tr></thead><tbody>",
    );
    for e in entries {
        let field = match &e.href {
            Some(href) => format!("<a href=\"{}\">{}</a>", esc(href), esc(&e.field)),
            None => esc(&e.field),
        };
        match &e.result {
            Ok((family, ron)) => h.push_str(&format!(
                "<tr class=\"acc\"><td>{field}</td><td>{}</td><td>{}</td><td><code>{}</code></td><td>{}</td></tr>",
                esc(&e.source),
                esc(family),
                esc(ron),
                e.warnings
            )),
            Err(err) => h.push_str(&format!(
                "<tr class=\"rej\"><td>{field}</td><td>{}</td><td colspan=\"3\" class=\"warn\">{}</td></tr>",
                esc(&e.source),
                esc(err)
            )),
        }
    }
    h.push_str("</tbody></table></div>\n<footer>Gerado pelo UTSim (<code>utsim dados</code>).</footer></main></body></html>\n");
    h
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gof::GofConfig;
    use utsim_core::rng::stream;

    fn quick(choose: Choice, criterion: Criterion) -> AutoConfig {
        AutoConfig {
            choose,
            criterion,
            fit: FitConfig {
                alpha: 0.05,
                gof: GofConfig {
                    bootstrap: 49,
                    seed: 7,
                    chi2_bins: None,
                },
            },
        }
    }

    fn sample(dist: &Dist, n: usize, seed: u64) -> Vec<f64> {
        let mut rng = stream(seed, 0, 0);
        (0..n).map(|_| dist.sample(&mut rng)).collect()
    }

    #[test]
    fn hash_ignores_formatting_but_not_values_or_order() {
        assert_eq!(data_hash(&[1.5, 2.0]), data_hash(&[1.50, 2.0]));
        assert_eq!(data_hash(&[0.0]), data_hash(&[-0.0]));
        assert_ne!(data_hash(&[1.5, 2.0]), data_hash(&[2.0, 1.5]));
        assert_ne!(data_hash(&[1.5, 2.0]), data_hash(&[1.5, 2.0, 2.0]));
        assert!(data_hash(&[]).starts_with("fnv1a64:"));
    }

    #[test]
    fn family_names_are_normalized() {
        assert_eq!(normalize("LogNormal"), normalize("Lognormal"));
        assert_eq!(normalize("Gamma"), normalize("Gama"));
        assert_eq!(normalize("geométrica"), normalize("Geometric"));
        assert_eq!(normalize("Log-logística"), normalize("LogLogistic"));
    }

    #[test]
    fn discrete_families_do_not_apply_to_continuous_data() {
        let data = sample(&Dist::Exp { mean: 2.0 }, 200, 3);
        let fit = auto_fit("x", &data, &quick(Choice::Best, Criterion::Aic)).unwrap();
        let poisson = fit
            .report
            .ranking
            .candidates
            .iter()
            .find(|c| c.family == "Poisson")
            .unwrap();
        assert!(poisson.fit_error.as_ref().unwrap().contains("discreta"));
        assert!(!fit.support.integer && fit.support.positive);
    }

    #[test]
    fn criteria_order_accepted_candidates_by_their_statistic() {
        let data = sample(
            &Dist::Gamma {
                shape: 3.0,
                scale: 1.0,
            },
            300,
            5,
        );
        for crit in [Criterion::Bic, Criterion::Ks, Criterion::Ad] {
            let fit = auto_fit("x", &data, &quick(Choice::Best, crit)).unwrap();
            let cands = &fit.report.ranking.candidates;
            let accepted: Vec<f64> = fit
                .order
                .iter()
                .filter(|&&i| cands[i].accepted())
                .map(|&i| crit.value(&cands[i]))
                .collect();
            assert!(accepted.windows(2).all(|w| w[0] <= w[1]), "{crit:?}");
            assert_eq!(fit.chosen, Some(fit.order[0]));
            assert!(fit.justification[0].contains(crit.label()));
        }
    }

    #[test]
    fn forced_family_and_empirical_choices() {
        let data = sample(&Dist::Exp { mean: 2.0 }, 200, 9);
        let fit = auto_fit(
            "x",
            &data,
            &quick(Choice::Family("normal".into()), Criterion::Aic),
        )
        .unwrap();
        assert_eq!(fit.family, "Normal");
        assert!(matches!(fit.dist, Dist::Normal { .. }));
        assert!(
            fit.warnings.iter().any(|w| w.contains("fixada")),
            "{:?}",
            fit.warnings
        );

        let fit = auto_fit("x", &data, &quick(Choice::Empirical, Criterion::Aic)).unwrap();
        assert_eq!(fit.family, EMPIRICAL);
        assert!(matches!(fit.dist, Dist::Empirical(_)));
        assert!(!fit.empirical_fallback);

        let err = auto_fit(
            "x",
            &data,
            &quick(Choice::Family("Cauchy".into()), Criterion::Aic),
        );
        assert!(err.err().unwrap().contains("desconhecida"));
        let err = auto_fit(
            "x",
            &data,
            &quick(Choice::Family("Poisson".into()), Criterion::Aic),
        );
        assert!(err.err().unwrap().contains("não se aplica"));
        assert!(auto_fit("x", &[], &AutoConfig::default()).is_err());
    }

    #[test]
    fn mixture_is_flagged_and_falls_back_to_empirical() {
        let mut data = sample(
            &Dist::Normal {
                mean: 10.0,
                sd: 1.0,
            },
            300,
            11,
        );
        data.extend(sample(
            &Dist::Normal {
                mean: 30.0,
                sd: 1.0,
            },
            300,
            12,
        ));
        // Interleave so the order does not look like a trend.
        let n = data.len() / 2;
        let data: Vec<f64> = (0..n).flat_map(|i| [data[i], data[n + i]]).collect();
        let fit = auto_fit("mistura", &data, &quick(Choice::Best, Criterion::Aic)).unwrap();
        assert!(
            fit.warnings.iter().any(|w| w.contains("mistura")),
            "{:?}",
            fit.warnings
        );
        assert!(fit.empirical_fallback);
        assert_eq!(fit.family, EMPIRICAL);
    }

    #[test]
    fn unimodal_samples_are_not_called_mixtures() {
        for (i, d) in [
            Dist::Exp { mean: 2.0 },
            Dist::Gamma {
                shape: 2.0,
                scale: 1.0,
            },
            Dist::LogNormal { mean: 4.0, sd: 2.0 },
            Dist::Normal { mean: 5.0, sd: 1.0 },
            Dist::Uniform { min: 0.0, max: 1.0 },
            Dist::Weibull {
                shape: 1.5,
                scale: 3.0,
            },
        ]
        .iter()
        .enumerate()
        {
            for n in [40, 100, 500] {
                let false_alarms = (0..20)
                    .filter(|s| {
                        let data = sample(d, n, 100 + 31 * i as u64 + s);
                        modes(&data).unwrap_or(1) > 1
                    })
                    .count();
                assert!(false_alarms <= 1, "{d:?} n = {n}: {false_alarms} em 20");
            }
        }
    }

    #[test]
    fn small_sample_and_outliers_are_warned() {
        let fit = auto_fit(
            "poucos",
            &[3.1, 2.4, 5.0, 4.2, 3.3],
            &quick(Choice::Best, Criterion::Aic),
        )
        .unwrap();
        assert!(fit.warnings.iter().any(|w| w.contains("Muito poucos")));
        let mut data = sample(
            &Dist::Normal {
                mean: 10.0,
                sd: 1.0,
            },
            100,
            13,
        );
        data[50] = 100.0;
        let fit = auto_fit("atípico", &data, &quick(Choice::Best, Criterion::Aic)).unwrap();
        assert!(
            fit.warnings.iter().any(|w| w.contains("3·IQR")),
            "{:?}",
            fit.warnings
        );
    }

    #[test]
    fn report_csv_and_index_render() {
        let data = sample(&Dist::LogNormal { mean: 4.0, sd: 1.5 }, 150, 17);
        let fit = auto_fit("Torno.process", &data, &quick(Choice::Best, Criterion::Ks)).unwrap();
        let html = fit.to_html("utsim dados");
        assert!(html.starts_with("<!doctype html>") && html.ends_with("</html>\n"));
        assert!(html.contains("Escolha para o campo") && html.contains("Justificativa"));
        assert!(html.contains("<svg") && !html.contains("<script") && !html.contains("https://"));
        let csv = fit.to_csv();
        assert_eq!(csv.lines().count(), 1 + fit.report.ranking.candidates.len());
        assert!(csv.contains("escolhida"));
        let index = index_html(
            "modelo.ron",
            &[IndexEntry {
                field: "Torno.process".into(),
                source: "valores no modelo".into(),
                result: Ok((fit.family.clone(), fit.ron())),
                warnings: fit.warnings.len(),
                href: Some("Torno.process.html".into()),
            }],
        );
        assert!(index.contains("href=\"Torno.process.html\""));
        serde_json::from_str::<serde_json::Value>(&fit.to_json()).unwrap();
    }
}
