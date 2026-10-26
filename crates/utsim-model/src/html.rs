//! Self-contained HTML report of a set of replications (and, optionally, of a scenario
//! comparison): tables plus charts as inline SVG, no external resources. Written by the
//! 3D editor's "Resultados" area; the style follows the optimiser's report.

use std::fmt::Write as _;

use utsim_core::stats::Summary;

use crate::analysis::{ObjectStats, bottleneck, delta_cell, is_significant, mean_hw};
use crate::experiment::ExperimentResult;

/// What the report shows.
pub struct ReportInput<'a> {
    pub title: &'a str,
    /// One line describing the run (replications, times, seed).
    pub settings: String,
    pub stats: &'a [ObjectStats],
    /// sRGB colour (0..1) of a state, by state name (the 3D view's colours).
    pub state_color: &'a dyn Fn(&str) -> [f32; 3],
    /// Portuguese label of a state, by state name.
    pub state_label: &'a dyn Fn(&str) -> String,
    pub experiment: Option<&'a ExperimentResult>,
}

pub fn esc(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

fn hex(c: [f32; 3]) -> String {
    let b = |v: f32| (v.clamp(0.0, 1.0) * 255.0).round() as u8;
    format!("#{:02x}{:02x}{:02x}", b(c[0]), b(c[1]), b(c[2]))
}

const CSS: &str = r#"
:root{--bg:#fff;--fg:#1c1f24;--muted:#5d6672;--line:#d8dde3;--accent:#2563eb;--hot:#c2410c;--card:#f5f7fa}
@media(prefers-color-scheme:dark){:root{--bg:#14171b;--fg:#e6e9ee;--muted:#9aa4b0;--line:#2d343c;--accent:#6ea0ff;--hot:#fb923c;--card:#1d2228}}
body{font:15px/1.5 system-ui,sans-serif;background:var(--bg);color:var(--fg);max-width:1100px;margin:2rem auto;padding:0 1rem}
h1{font-size:1.5rem}h2{font-size:1.15rem;margin-top:2rem}
table{border-collapse:collapse;width:100%;font-size:.85rem}
th,td{padding:.3rem .5rem;border-bottom:1px solid var(--line);text-align:left}
th{color:var(--muted);font-weight:600}
td.n{text-align:right;font-variant-numeric:tabular-nums}
tr.hot td{background:var(--card);font-weight:600}
.muted{color:var(--muted)}.hot{color:var(--hot)}
.legend span{display:inline-block;margin-right:1rem}
.legend i{display:inline-block;width:.8rem;height:.8rem;margin-right:.3rem;vertical-align:-1px}
svg{max-width:100%;height:auto}
svg text{fill:var(--fg);font-size:11px}
svg .axis{stroke:var(--muted);stroke-width:1}
svg .whisker{stroke:var(--fg);stroke-width:1.2}
svg .bar{fill:var(--accent);opacity:.85}svg .barhot{fill:var(--hot)}
"#;

fn cell(s: &Summary) -> String {
    esc(&mean_hw(s))
}

fn pct(s: &Summary) -> String {
    if s.half_width.is_nan() {
        format!("{:.1}%", 100.0 * s.mean)
    } else {
        format!("{:.1}% ± {:.1}", 100.0 * s.mean, 100.0 * s.half_width)
    }
}

/// The whole report as one HTML page.
pub fn report(input: &ReportInput) -> String {
    let mut h = String::new();
    let _ = write!(
        h,
        "<!doctype html><html lang=\"pt-BR\"><head><meta charset=\"utf-8\">\
         <meta name=\"viewport\" content=\"width=device-width,initial-scale=1\">\
         <title>Resultados: {t}</title><style>{CSS}</style></head><body>\
         <h1>Resultados: {t}</h1><p class=\"muted\">{s}. Cada valor é a média entre \
         replicações ± a meia-amplitude do intervalo de confiança de 95%.</p>",
        t = esc(input.title),
        s = esc(&input.settings)
    );
    let hot = bottleneck(input.stats);
    if let Some(i) = hot {
        let s = &input.stats[i];
        let _ = write!(
            h,
            "<p>Gargalo: <b class=\"hot\">{}</b> (ocupação {}).</p>",
            esc(&s.name),
            esc(&pct(&s.utilization.unwrap_or(s.content)))
        );
    }
    h.push_str("<h2>Resumo por objeto</h2>");
    h.push_str(&objects_table(input.stats, hot));
    h.push_str("<h2>Ocupação dos recursos</h2>");
    h.push_str(&utilization_chart(input.stats, hot));
    h.push_str("<h2>Distribuição dos estados</h2>");
    h.push_str(&states_chart(input));
    let metrics: Vec<String> = input
        .stats
        .iter()
        .flat_map(|s| {
            s.extra
                .iter()
                .map(move |(k, v)| (format!("{}.{k}.mean", s.name), *v))
                .chain(
                    s.custom
                        .iter()
                        .map(move |(k, v)| (format!("{}.{k}", s.name), *v)),
                )
        })
        .map(|(k, v)| {
            format!(
                "<tr><td><code>{}</code></td><td class=\"n\">{}</td></tr>",
                esc(&k),
                cell(&v)
            )
        })
        .collect();
    if !metrics.is_empty() {
        h.push_str("<h2>Métricas próprias</h2><table><tr><th>Métrica</th><th>Valor</th></tr>");
        h.push_str(&metrics.concat());
        h.push_str("</table>");
    }
    if let Some(exp) = input.experiment {
        h.push_str(&experiment_table(exp));
    }
    h.push_str("</body></html>");
    h
}

fn objects_table(stats: &[ObjectStats], hot: Option<usize>) -> String {
    let mut h = String::from(
        "<table><tr><th>Objeto</th><th>Tipo</th><th>Entradas</th><th>Saídas</th>\
         <th>Conteúdo médio</th><th>Permanência média</th><th>Ocupação</th></tr>",
    );
    for (i, s) in stats.iter().enumerate() {
        let _ = write!(
            h,
            "<tr{}><td>{}</td><td>{}</td><td class=\"n\">{}</td><td class=\"n\">{}</td>\
             <td class=\"n\">{}</td><td class=\"n\">{}</td><td class=\"n\">{}</td></tr>",
            if hot == Some(i) { " class=\"hot\"" } else { "" },
            esc(&s.name),
            esc(&s.kind),
            cell(&s.input),
            cell(&s.output),
            cell(&s.content),
            cell(&s.staytime),
            s.utilization
                .as_ref()
                .map(pct)
                .map_or(String::new(), |t| esc(&t)),
        );
    }
    h.push_str("</table>");
    h
}

/// Horizontal bars of the utilisation of every resource, 0 to 100%, with the 95% interval.
fn utilization_chart(stats: &[ObjectStats], hot: Option<usize>) -> String {
    let rows: Vec<(usize, &ObjectStats, Summary)> = stats
        .iter()
        .enumerate()
        .filter_map(|(i, s)| s.utilization.map(|u| (i, s, u)))
        .collect();
    if rows.is_empty() {
        return "<p class=\"muted\">O modelo não tem recursos (processos ou recursos móveis).</p>"
            .into();
    }
    let (w, row_h, left, right) = (900.0, 24.0, 180.0, 60.0);
    let hgt = row_h * rows.len() as f64 + 24.0;
    let x = |v: f64| left + (w - left - right) * v.clamp(0.0, 1.0);
    let mut s = format!(
        "<svg viewBox=\"0 0 {w} {hgt}\" role=\"img\" aria-label=\"Ocupação dos recursos\">"
    );
    for (k, (i, st, u)) in rows.iter().enumerate() {
        let y = 4.0 + row_h * k as f64;
        let class = if hot == Some(*i) { "barhot" } else { "bar" };
        let _ = write!(
            s,
            "<text x=\"{}\" y=\"{:.1}\" text-anchor=\"end\">{}</text>\
             <rect class=\"{class}\" x=\"{left}\" y=\"{y:.1}\" width=\"{:.1}\" height=\"{}\"/>\
             <text x=\"{:.1}\" y=\"{:.1}\">{:.1}%</text>",
            left - 8.0,
            y + 13.0,
            esc(&st.name),
            (x(u.mean) - left).max(0.5),
            row_h - 8.0,
            w - right + 6.0,
            y + 13.0,
            100.0 * u.mean
        );
        if !u.half_width.is_nan() {
            let (a, b) = (x(u.mean - u.half_width), x(u.mean + u.half_width));
            let m = y + (row_h - 8.0) / 2.0;
            let _ = write!(
                s,
                "<line class=\"whisker\" x1=\"{a:.1}\" x2=\"{b:.1}\" y1=\"{m:.1}\" y2=\"{m:.1}\"/>\
                 <line class=\"whisker\" x1=\"{a:.1}\" x2=\"{a:.1}\" y1=\"{:.1}\" y2=\"{:.1}\"/>\
                 <line class=\"whisker\" x1=\"{b:.1}\" x2=\"{b:.1}\" y1=\"{:.1}\" y2=\"{:.1}\"/>",
                m - 4.0,
                m + 4.0,
                m - 4.0,
                m + 4.0
            );
        }
    }
    let base = hgt - 6.0;
    let _ = write!(
        s,
        "<text x=\"{left}\" y=\"{base}\">0%</text><text x=\"{}\" y=\"{base}\" text-anchor=\"end\">100%</text></svg>",
        w - right
    );
    s
}

/// One stacked horizontal bar per object: the fraction of time in each state.
fn states_chart(input: &ReportInput) -> String {
    let rows: Vec<&ObjectStats> = input
        .stats
        .iter()
        .filter(|s| !s.states.is_empty())
        .collect();
    if rows.is_empty() {
        return "<p class=\"muted\">Sem estados registrados.</p>".into();
    }
    let (w, row_h, left, right) = (900.0, 22.0, 180.0, 20.0);
    let hgt = row_h * rows.len() as f64 + 8.0;
    let span = w - left - right;
    let mut s = format!(
        "<svg viewBox=\"0 0 {w} {hgt}\" role=\"img\" aria-label=\"Distribuição dos estados\">"
    );
    let mut used: Vec<String> = Vec::new();
    for (k, st) in rows.iter().enumerate() {
        let y = 4.0 + row_h * k as f64;
        let _ = write!(
            s,
            "<text x=\"{}\" y=\"{:.1}\" text-anchor=\"end\">{}</text>",
            left - 8.0,
            y + 12.0,
            esc(&st.name)
        );
        let total: f64 = st.states.iter().map(|(_, v)| v.mean).sum();
        let mut x = left;
        for (state, v) in &st.states {
            let width = span * v.mean / total.max(1e-12);
            let _ = write!(
                s,
                "<rect x=\"{x:.1}\" y=\"{y:.1}\" width=\"{width:.1}\" height=\"{}\" fill=\"{}\">\
                 <title>{}: {}</title></rect>",
                row_h - 6.0,
                hex((input.state_color)(state)),
                esc(&(input.state_label)(state)),
                esc(&pct(v))
            );
            x += width;
            if !used.contains(state) {
                used.push(state.clone());
            }
        }
    }
    s.push_str("</svg><p class=\"legend\">");
    for state in used {
        let _ = write!(
            s,
            "<span><i style=\"background:{}\"></i>{}</span>",
            hex((input.state_color)(&state)),
            esc(&(input.state_label)(&state))
        );
    }
    s.push_str("</p>");
    s
}

/// Scenario comparison: one column per scenario, and the paired difference to the
/// first scenario (`≈`: not significant at 95%).
pub fn experiment_table(exp: &ExperimentResult) -> String {
    let mut h = format!(
        "<h2>Cenários: {}</h2><table><tr><th>Métrica</th>",
        esc(&exp.name)
    );
    for (i, s) in exp.scenarios.iter().enumerate() {
        let _ = write!(h, "<th>{}</th>", esc(&s.name));
        if i > 0 {
            let _ = write!(h, "<th>Δ vs {}</th>", esc(&exp.scenarios[0].name));
        }
    }
    h.push_str("</tr>");
    for m in exp.metric_names() {
        let _ = write!(h, "<tr><td><code>{}</code></td>", esc(&m));
        for (i, s) in exp.scenarios.iter().enumerate() {
            let v = s.summary.get(&m).map_or("-".to_string(), cell);
            let _ = write!(h, "<td class=\"n\">{v}</td>");
            if i > 0 {
                let d = exp.difference_to_first(i, &m);
                let class = if d.as_ref().is_some_and(is_significant) {
                    " hot"
                } else {
                    ""
                };
                let text = d.as_ref().map_or("-".to_string(), delta_cell);
                let _ = write!(h, "<td class=\"n{class}\">{}</td>", esc(&text));
            }
        }
        h.push_str("</tr>");
    }
    h.push_str(
        "</table><p class=\"muted\">Δ: diferença pareada (mesmas sementes) para o primeiro \
         cenário, com o intervalo de 95%. ≈ indica que o intervalo contém zero: sem diferença \
         significativa.</p>",
    );
    h
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Model;
    use crate::analysis::object_stats;

    #[test]
    fn report_is_self_contained_and_marks_the_bottleneck() {
        let m = Model::parse(
            r#"Model(name: "m", run: (stop_time: 500.0, replications: 3),
                objects: [
                    (name: "src", kind: Source(interarrival: Exp(mean: 1.0))),
                    (name: "q", kind: Queue()),
                    (name: "maq", kind: Processor(process: Exp(mean: 0.8))),
                    (name: "out", kind: Sink),
                ],
                connections: [("src", "q"), ("q", "maq"), ("maq", "out")])"#,
        )
        .unwrap();
        let reports = m.run_replications().unwrap();
        let stats = object_stats(&m, &reports);
        let color = |_: &str| [0.2, 0.7, 0.3];
        let label = |s: &str| s.to_string();
        let html = report(&ReportInput {
            title: "m <teste>",
            settings: "3 replicações".into(),
            stats: &stats,
            state_color: &color,
            state_label: &label,
            experiment: None,
        });
        assert!(html.contains("<svg") && html.contains("m &lt;teste&gt;"));
        assert!(!html.contains("http://") && !html.contains("https://"));
        assert!(html.contains("Gargalo: <b class=\"hot\">maq</b>"), "{html}");
        assert!(html.contains("#33b34d"));
        assert!(html.contains("time_in_system"));
    }
}
