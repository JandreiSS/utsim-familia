//! Full analysis and its presentation (contract). Implemented by the "relatório" front.

use serde::Serialize;

use crate::diagnostics::Diagnostics;
use crate::gof::{GofConfig, TestStat};
use crate::plots;
use crate::rank::{CandidateResult, Ranking, num};

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
    /// The analysed sample, kept for the plots; not serialised.
    #[serde(skip)]
    pub data: Vec<f64>,
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
        data: data.to_vec(),
    }
}

fn p_text(t: &Option<TestStat>) -> String {
    t.as_ref().map_or("-".to_string(), |t| num(t.p_value))
}

fn params_text(c: &CandidateResult) -> String {
    if c.params.is_empty() {
        return "-".to_string();
    }
    c.params
        .iter()
        .map(|(k, v)| format!("{k} = {}", num(*v)))
        .collect::<Vec<_>>()
        .join(", ")
}

fn is_discrete(family: &str) -> bool {
    crate::families::all()
        .iter()
        .any(|f| f.name() == family && f.is_discrete())
}

fn esc(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

fn pad(s: &str, width: usize) -> String {
    let n = s.chars().count();
    format!("{s}{}", " ".repeat(width.saturating_sub(n)))
}

impl FitReport {
    /// Status of the candidate at `index` in the ranking.
    fn status(&self, index: usize) -> &'static str {
        let c = &self.ranking.candidates[index];
        if c.fit_error.is_some() {
            "não se aplica"
        } else if self.ranking.best == Some(index) {
            "escolhida"
        } else if c.accepted() {
            "aceita"
        } else {
            "rejeitada"
        }
    }

    fn best_aic(&self) -> f64 {
        self.ranking
            .candidates
            .iter()
            .filter(|c| c.fit_error.is_none())
            .map(|c| c.aic)
            .fold(f64::INFINITY, f64::min)
    }

    /// The winner, or the least bad fitted candidate when everything was rejected.
    fn plot_target(&self) -> Option<&CandidateResult> {
        self.ranking
            .best
            .map(|i| &self.ranking.candidates[i])
            .or_else(|| self.ranking.candidates.iter().find(|c| c.fitted.is_some()))
    }

    /// The `Dist` of the winner as one line of RON, ready to paste into a model.
    pub fn ron_line(&self) -> Option<String> {
        let dist = self
            .ranking
            .best
            .and_then(|i| self.ranking.candidates[i].dist.as_ref())?;
        ron::ser::to_string(dist).ok()
    }

    /// One row per candidate: position, family, parameters, p-values, AIC, ΔAIC, weight, status.
    fn table_rows(&self) -> Vec<Vec<String>> {
        let best = self.best_aic();
        self.ranking
            .candidates
            .iter()
            .enumerate()
            .map(|(i, c)| {
                let fitted = c.fit_error.is_none();
                vec![
                    (i + 1).to_string(),
                    c.family.clone(),
                    params_text(c),
                    p_text(&c.gof.ks),
                    p_text(&c.gof.anderson_darling),
                    p_text(&c.gof.chi_square),
                    if fitted { num(c.aic) } else { "-".into() },
                    if fitted {
                        num(c.aic - best)
                    } else {
                        "-".into()
                    },
                    if fitted {
                        format!("{}%", num(100.0 * c.akaike_weight))
                    } else {
                        "-".into()
                    },
                    self.status(i).to_string(),
                ]
            })
            .collect()
    }

    const HEADERS: [&'static str; 10] = [
        "pos.",
        "família",
        "parâmetros",
        "KS p",
        "AD p",
        "χ² p",
        "AIC",
        "ΔAIC",
        "peso Akaike",
        "situação",
    ];

    fn descriptive_lines(&self) -> Vec<(&'static str, String)> {
        let d = &self.diagnostics.descriptive;
        vec![
            ("n", d.n.to_string()),
            ("média", num(d.mean)),
            ("mediana", num(d.median)),
            ("desvio padrão", num(d.std_dev)),
            ("CV", num(d.cv)),
            ("assimetria", num(d.skewness)),
            ("curtose (excesso)", num(d.excess_kurtosis)),
            ("mínimo", num(d.min)),
            ("Q1", num(d.q1)),
            ("Q3", num(d.q3)),
            ("máximo", num(d.max)),
        ]
    }

    fn equivalence_note(&self) -> Option<String> {
        let r = &self.ranking;
        if r.equivalent.is_empty() {
            return None;
        }
        let winner = r.best.map(|i| r.candidates[i].family.as_str())?;
        Some(format!(
            "Atenção: {} {} praticamente equivalente(s) a {winner} (ΔAIC < 2). Os dados não permitem \
             distinguir entre elas; qualquer uma é defensável, e vale preferir a mais simples ou a que \
             tenha melhor interpretação para o processo.",
            r.equivalent.join(", "),
            if r.equivalent.len() == 1 {
                "é"
            } else {
                "são"
            },
        ))
    }

    fn empirical_note(&self) -> Option<&'static str> {
        self.ranking.recommend_empirical.then_some(
            "Nenhuma distribuição paramétrica serviu: use a distribuição empírica (por exemplo \
             `Discrete([...])` com os próprios valores observados, ou a série medida diretamente).",
        )
    }

    /// Plain-text report for the terminal.
    pub fn to_text(&self) -> String {
        let mut out = String::new();
        let line = "─".repeat(78);
        out.push_str(&format!(
            "Ajuste de distribuições: {}\n{line}\n\n",
            self.dataset
        ));

        out.push_str("DADOS\n");
        let desc = self.descriptive_lines();
        for chunk in desc.chunks(3) {
            let cells: Vec<String> = chunk
                .iter()
                .map(|(k, v)| pad(&format!("{k}: {v}"), 28))
                .collect();
            out.push_str(&format!("  {}\n", cells.join("").trim_end()));
        }
        out.push('\n');

        let dg = &self.diagnostics;
        out.push_str("DIAGNÓSTICO\n");
        if dg.warnings.is_empty() {
            out.push_str("  nenhum aviso: não há sinal de violação das hipóteses i.i.d.\n");
        }
        for w in &dg.warnings {
            out.push_str(&format!("  ! {w}\n"));
        }
        for c in &dg.checks {
            out.push_str(&format!(
                "  - {}: estatística = {}, p = {}. {}\n",
                c.name,
                num(c.statistic),
                num(c.p_value),
                c.verdict
            ));
        }
        if !dg.autocorrelation.is_empty() {
            let ac: Vec<String> = dg
                .autocorrelation
                .iter()
                .take(5)
                .enumerate()
                .map(|(i, r)| format!("lag {} = {}", i + 1, num(*r)))
                .collect();
            out.push_str(&format!("  - autocorrelação: {}\n", ac.join(", ")));
        }
        if !dg.outliers.is_empty() {
            out.push_str(&format!(
                "  - valores atípicos (IQR): {}\n",
                dg.outliers.len()
            ));
        }
        if dg.ties_fraction > 0.0 {
            out.push_str(&format!(
                "  - valores repetidos: {}%\n",
                num(100.0 * dg.ties_fraction)
            ));
        }
        out.push('\n');

        out.push_str(&format!(
            "REGRA DE ESCOLHA (α = {})\n",
            num(self.ranking.alpha)
        ));
        // Verbatim, on one line, so it can be searched for and quoted.
        out.push_str(&format!("  {}\n\n", self.ranking.rule));

        out.push_str("CANDIDATAS\n");
        let rows = self.table_rows();
        let widths: Vec<usize> = (0..Self::HEADERS.len())
            .map(|j| {
                rows.iter()
                    .map(|r| r[j].chars().count())
                    .chain([Self::HEADERS[j].chars().count()])
                    .max()
                    .unwrap_or(0)
            })
            .collect();
        let fmt_row = |cells: Vec<&str>| {
            let s: Vec<String> = cells
                .iter()
                .enumerate()
                .map(|(j, c)| pad(c, widths[j]))
                .collect();
            format!("  {}\n", s.join("  ").trim_end())
        };
        out.push_str(&fmt_row(Self::HEADERS.to_vec()));
        for r in &rows {
            out.push_str(&fmt_row(r.iter().map(String::as_str).collect()));
        }
        out.push('\n');

        out.push_str("POR QUE\n");
        for e in &self.ranking.explanation {
            out.push_str(&format!("  - {e}\n"));
        }
        if let Some(n) = self.equivalence_note() {
            out.push_str(&format!("\n  {n}\n"));
        }
        out.push('\n');

        out.push_str("PARA O MODELO (.ron)\n");
        match self.ron_line() {
            Some(l) => out.push_str(&format!("  {l}\n")),
            None => match self.empirical_note() {
                Some(n) => out.push_str(&format!("  {n}\n")),
                None => out.push_str("  a escolhida não tem equivalente em Dist.\n"),
            },
        }
        out
    }

    pub fn to_json(&self) -> String {
        serde_json::to_string_pretty(self).expect("report serializes")
    }

    /// Single self-contained HTML page (inline CSS and SVG, no external resources).
    pub fn to_html(&self) -> String {
        let mut h = String::new();
        h.push_str(
            "<!doctype html>\n<html lang=\"pt-BR\"><head><meta charset=\"utf-8\">\
             <meta name=\"viewport\" content=\"width=device-width, initial-scale=1\">\n",
        );
        h.push_str(&format!(
            "<title>Ajuste de distribuições: {}</title>\n<style>{HTML_CSS}</style></head><body><main>\n",
            esc(&self.dataset)
        ));
        h.push_str(&format!(
            "<h1>Ajuste de distribuições</h1><p class=\"sub\">{}</p>\n",
            esc(&self.dataset)
        ));

        h.push_str("<h2>Dados</h2><dl class=\"stats\">");
        for (k, v) in self.descriptive_lines() {
            h.push_str(&format!(
                "<div><dt>{}</dt><dd>{}</dd></div>",
                esc(k),
                esc(&v)
            ));
        }
        h.push_str("</dl>\n");

        let dg = &self.diagnostics;
        h.push_str("<h2>Diagnóstico</h2>");
        if dg.warnings.is_empty() {
            h.push_str(
                "<p class=\"ok\">Nenhum aviso: não há sinal de violação das hipóteses i.i.d.</p>",
            );
        }
        for w in &dg.warnings {
            h.push_str(&format!("<p class=\"warn\">{}</p>", esc(w)));
        }
        if !dg.checks.is_empty() {
            h.push_str("<ul>");
            for c in &dg.checks {
                h.push_str(&format!(
                    "<li><b>{}</b>: estatística = {}, p = {}. {}</li>",
                    esc(&c.name),
                    num(c.statistic),
                    num(c.p_value),
                    esc(&c.verdict)
                ));
            }
            h.push_str("</ul>");
        }
        if !dg.autocorrelation.is_empty() {
            let ac: Vec<String> = dg
                .autocorrelation
                .iter()
                .enumerate()
                .map(|(i, r)| format!("lag {} = {}", i + 1, num(*r)))
                .collect();
            h.push_str(&format!("<p>Autocorrelação: {}</p>", ac.join(", ")));
        }
        if self.data.len() > 2 {
            h.push_str(&format!(
                "<figure>{}</figure>",
                plots::lag_scatter(&self.data)
            ));
        }

        h.push_str(&format!(
            "<h2>Regra de escolha</h2><p>{}</p><p>Nível de significância α = {}.</p>\n",
            esc(self.ranking.rule),
            num(self.ranking.alpha)
        ));

        h.push_str("<h2>Candidatas</h2><div class=\"scroll\"><table><thead><tr>");
        for t in Self::HEADERS {
            h.push_str(&format!("<th>{}</th>", esc(t)));
        }
        h.push_str("</tr></thead><tbody>");
        for (i, row) in self.table_rows().iter().enumerate() {
            h.push_str(&format!(
                "<tr class=\"{}\">",
                match self.status(i) {
                    "escolhida" => "best",
                    "rejeitada" => "rej",
                    "não se aplica" => "na",
                    _ => "acc",
                }
            ));
            for cell in row {
                h.push_str(&format!("<td>{}</td>", esc(cell)));
            }
            h.push_str("</tr>");
        }
        h.push_str("</tbody></table></div>\n");

        h.push_str("<h2>Por que</h2><ul>");
        for e in &self.ranking.explanation {
            h.push_str(&format!("<li>{}</li>", esc(e)));
        }
        h.push_str("</ul>");
        if let Some(n) = self.equivalence_note() {
            h.push_str(&format!("<p class=\"warn\">{}</p>", esc(&n)));
        }

        h.push_str("<h2>Para o modelo</h2>");
        match (self.ron_line(), self.empirical_note()) {
            (Some(l), _) => h.push_str(&format!("<pre>{}</pre>", esc(&l))),
            (None, Some(n)) => h.push_str(&format!("<p class=\"warn\">{}</p>", esc(n))),
            _ => h.push_str("<p>A escolhida não tem equivalente em <code>Dist</code>.</p>"),
        }

        h.push_str("<h2>Gráficos</h2>");
        if let Some(target) = self.plot_target() {
            let discrete = is_discrete(&target.family);
            let top: Vec<&CandidateResult> = self
                .ranking
                .candidates
                .iter()
                .filter(|c| c.fitted.is_some())
                .take(3)
                .collect();
            h.push_str(&format!(
                "<p>Gráficos da {}: <b>{}</b>.</p><div class=\"grid\">",
                if self.ranking.best.is_some() {
                    "escolhida"
                } else {
                    "menos ruim"
                },
                esc(&target.family)
            ));
            for svg in [
                plots::histogram(&self.data, &top, discrete),
                plots::qq(&self.data, target),
                plots::pp(&self.data, target),
                plots::ecdf(&self.data, target),
            ] {
                h.push_str(&format!("<figure>{svg}</figure>"));
            }
            h.push_str("</div>");
            h.push_str("<h2>Cada candidata</h2>");
            for (i, c) in self.ranking.candidates.iter().enumerate() {
                let reason = self
                    .ranking
                    .explanation
                    .iter()
                    .find(|e| {
                        e.starts_with(&format!("{}:", c.family))
                            || e.starts_with(&format!("{} foi", c.family))
                    })
                    .cloned();
                h.push_str(&format!(
                    "<details><summary>{} <span class=\"tag\">{}</span></summary>",
                    esc(&c.family),
                    self.status(i)
                ));
                if let Some(e) = reason {
                    h.push_str(&format!("<p>{}</p>", esc(&e)));
                }
                if let Some(err) = &c.fit_error {
                    h.push_str(&format!("<p>Não se aplica: {}.</p>", esc(err)));
                } else {
                    h.push_str(&format!("<p>Parâmetros: {}.</p>", esc(&params_text(c))));
                    for (name, t) in [
                        ("Kolmogorov-Smirnov", &c.gof.ks),
                        ("Anderson-Darling", &c.gof.anderson_darling),
                        ("Qui-quadrado", &c.gof.chi_square),
                    ] {
                        if let Some(t) = t {
                            h.push_str(&format!(
                                "<p>{name}: estatística = {}, p = {} ({}).</p>",
                                num(t.statistic),
                                num(t.p_value),
                                esc(&t.method)
                            ));
                        }
                    }
                    h.push_str("<div class=\"grid\">");
                    for svg in [
                        plots::qq(&self.data, c),
                        plots::pp(&self.data, c),
                        plots::ecdf(&self.data, c),
                    ] {
                        h.push_str(&format!("<figure>{svg}</figure>"));
                    }
                    h.push_str("</div>");
                }
                h.push_str("</details>");
            }
        } else {
            h.push_str("<p>Nenhuma candidata foi ajustada, não há o que desenhar.</p>");
        }
        h.push_str(
            "\n<footer>Gerado pelo UTSim (<code>utsim fit</code>).</footer></main></body></html>\n",
        );
        h
    }
}

const HTML_CSS: &str = "\
:root{--bg:#fff;--fg:#1d2330;--muted:#5d6779;--line:#d9dee7;--card:#f5f7fa;--ok:#1d7a4f;--warn:#a65b00;--best:#e4f3ea}\
@media (prefers-color-scheme:dark){:root{--bg:#14171d;--fg:#e4e8ef;--muted:#98a1b3;--line:#2d3340;--card:#1c2029;--ok:#5fcf9a;--warn:#f0b35c;--best:#17352a}}\
body{margin:0;background:var(--bg);color:var(--fg);font:15px/1.5 system-ui,sans-serif}\
main{max-width:1000px;margin:0 auto;padding:24px 16px 48px}\
h1{margin:0}h2{margin:32px 0 8px;font-size:18px;border-bottom:1px solid var(--line);padding-bottom:4px}\
.sub{color:var(--muted);margin-top:0}\
.stats{display:flex;flex-wrap:wrap;gap:8px;margin:0}.stats div{background:var(--card);border:1px solid var(--line);border-radius:6px;padding:6px 12px}\
.stats dt{font-size:12px;color:var(--muted)}.stats dd{margin:0;font-weight:600}\
.ok{color:var(--ok)}.warn{color:var(--warn)}\
.scroll{overflow-x:auto}table{border-collapse:collapse;width:100%;font-size:14px}\
th,td{padding:6px 10px;border-bottom:1px solid var(--line);text-align:left;white-space:nowrap}th{color:var(--muted);font-weight:600}\
tr.best{background:var(--best);font-weight:600}tr.rej,tr.na{color:var(--muted)}\
pre{background:var(--card);border:1px solid var(--line);border-radius:6px;padding:12px;overflow-x:auto;font-size:15px}\
.grid{display:grid;grid-template-columns:repeat(auto-fit,minmax(300px,1fr));gap:12px}figure{margin:0}\
details{border:1px solid var(--line);border-radius:6px;padding:8px 12px;margin:8px 0}summary{cursor:pointer;font-weight:600}\
.tag{font-weight:400;color:var(--muted);font-size:13px;margin-left:8px}\
footer{margin-top:40px;color:var(--muted);font-size:13px}";

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> Vec<f64> {
        (1..=80)
            .map(|i| -(1.0 - i as f64 / 81.0).ln() * 3.0)
            .collect()
    }

    #[test]
    fn text_html_json_have_the_expected_structure() {
        let data = sample();
        let report = analyze("amostra.csv (coluna 1)", &data, &FitConfig::default());
        let text = report.to_text();
        assert!(text.contains("amostra.csv"));
        assert!(text.contains(crate::rank::RULE));
        for c in &report.ranking.candidates {
            assert!(text.contains(&c.family), "falta {} em:\n{text}", c.family);
        }
        for header in FitReport::HEADERS {
            assert!(text.contains(header), "falta cabeçalho {header}");
        }
        if report.ranking.best.is_some() {
            assert!(text.contains("escolhida"));
            assert!(text.contains("PARA O MODELO"));
            assert!(report.ron_line().is_some());
        }
        let html = report.to_html();
        assert!(html.starts_with("<!doctype html>"));
        assert!(html.contains("<svg") && html.contains("</html>"));
        assert!(!html.contains("https://"));
        assert!(!html.contains("<link") && !html.contains("<script"));
        for c in &report.ranking.candidates {
            assert!(html.contains(&c.family));
        }
        let json: serde_json::Value = serde_json::from_str(&report.to_json()).unwrap();
        assert!(json["ranking"]["candidates"].is_array());
    }

    #[test]
    fn empty_data_still_renders() {
        let report = analyze("vazio", &[], &FitConfig::default());
        assert!(report.to_text().contains("CANDIDATAS"));
        assert!(report.to_html().contains("</html>"));
    }

    #[test]
    fn ron_line_uses_ron_syntax() {
        let report = analyze("x", &sample(), &FitConfig::default());
        if let Some(line) = report.ron_line() {
            assert!(line.contains('(') && !line.contains('\n'), "{line}");
        }
    }
}
