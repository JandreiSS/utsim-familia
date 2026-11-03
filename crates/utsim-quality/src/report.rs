//! Text and self-contained HTML reports (tables plus inline SVG, no external
//! resources), in the style of the simulator's results report
//! (`utsim-model/src/html.rs`). Numbers use the decimal comma.

use std::fmt::Write as _;

use crate::attribute::AttributeStudy;
use crate::capability::{BoxCoxCapability, Capability, PercentileIndices, Spec};
use crate::charts::{Chart, Limits, Panel, Phase};
use crate::msa::{Bias, GageRR, Linearity};

/// Escapes text for HTML.
pub fn esc(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

/// Number for Portuguese text: decimal comma, 4 significant digits after the point for
/// small values, fewer for large ones.
pub fn num(x: f64) -> String {
    if x.is_nan() {
        return "—".into();
    }
    if !x.is_finite() {
        return if x > 0.0 {
            "∞".into()
        } else {
            "−∞".into()
        };
    }
    let a = x.abs();
    let s = if a >= 1000.0 {
        format!("{x:.1}")
    } else if a >= 100.0 {
        format!("{x:.2}")
    } else if a >= 1.0 || a == 0.0 {
        format!("{x:.4}")
    } else {
        format!("{x:.5}")
    };
    s.replace('.', ",")
}

/// Short number without trailing zeros (`6`, `5,15`).
fn short(x: f64) -> String {
    format!("{x}").replace('.', ",")
}

fn opt(x: Option<f64>) -> String {
    x.map_or("—".into(), num)
}

fn pct(x: f64) -> String {
    format!("{}%", format!("{x:.2}").replace('.', ","))
}

const CSS: &str = r#"
:root{--bg:#fff;--fg:#1c1f24;--muted:#5d6672;--line:#d8dde3;--accent:#2563eb;--hot:#c2410c;--card:#f5f7fa;--lim:#b91c1c}
@media(prefers-color-scheme:dark){:root{--bg:#14171b;--fg:#e6e9ee;--muted:#9aa4b0;--line:#2d343c;--accent:#6ea0ff;--hot:#fb923c;--card:#1d2228;--lim:#f87171}}
body{font:15px/1.5 system-ui,sans-serif;background:var(--bg);color:var(--fg);max-width:1100px;margin:2rem auto;padding:0 1rem}
h1{font-size:1.5rem}h2{font-size:1.15rem;margin-top:2rem}h3{font-size:1rem;margin-top:1.4rem}
table{border-collapse:collapse;width:100%;font-size:.85rem}
th,td{padding:.3rem .5rem;border-bottom:1px solid var(--line);text-align:left}
th{color:var(--muted);font-weight:600}
td.n{text-align:right;font-variant-numeric:tabular-nums}
tr.hot td{background:var(--card);font-weight:600}
.muted{color:var(--muted)}.hot{color:var(--hot)}
ul.notes li{color:var(--muted)}
svg{max-width:100%;height:auto}
svg text{fill:var(--fg);font-size:11px}
svg .axis{stroke:var(--muted);stroke-width:1}
svg .lim{stroke:var(--lim);stroke-width:1.2;fill:none}
svg .center{stroke:var(--muted);stroke-width:1;stroke-dasharray:5 4;fill:none}
svg .series{stroke:var(--accent);stroke-width:1.4;fill:none}
svg .pt{fill:var(--accent)}svg .pthot{fill:var(--hot)}svg .ptex{fill:none;stroke:var(--muted)}
svg .bar{fill:var(--accent);opacity:.75}svg .bar2{fill:var(--hot);opacity:.75}
svg .curve{stroke:var(--fg);stroke-width:1.4;fill:none}
svg .curve2{stroke:var(--fg);stroke-width:1.2;fill:none;stroke-dasharray:4 3}
svg .spec{stroke:var(--lim);stroke-width:1.6}
svg .target{stroke:var(--muted);stroke-width:1.2;stroke-dasharray:3 3}
"#;

fn page(title: &str, subtitle: &str, body: &str) -> String {
    format!(
        "<!doctype html><html lang=\"pt-BR\"><head><meta charset=\"utf-8\">\
         <meta name=\"viewport\" content=\"width=device-width,initial-scale=1\">\
         <title>{t}</title><style>{CSS}</style></head><body><h1>{t}</h1>\
         <p class=\"muted\">{s}</p>{body}</body></html>",
        t = esc(title),
        s = esc(subtitle)
    )
}

fn notes_html(notes: &[String]) -> String {
    if notes.is_empty() {
        return String::new();
    }
    let mut h = String::from("<ul class=\"notes\">");
    for n in notes {
        let _ = write!(h, "<li>{}</li>", esc(n));
    }
    h.push_str("</ul>");
    h
}

// ---------------------------------------------------------------- control charts

fn limits_line(chart: &Chart) -> String {
    match &chart.limits {
        Limits::Variables { n, mean, sigma, .. } => {
            format!("μ = {}, σ = {}, n = {n}", num(*mean), num(*sigma))
        }
        Limits::Proportion { pbar, n, .. } => {
            if n.is_nan() {
                format!("p̄ = {}", num(*pbar))
            } else {
                format!("p̄ = {}, n = {}", num(*pbar), num(*n))
            }
        }
        Limits::Count { mean, .. } => format!("média = {}", num(*mean)),
        Limits::Fixed { center, lcl, ucl } => format!(
            "LIC = {}, LC = {}, LSC = {} (fixos)",
            num(*lcl),
            num(*center),
            num(*ucl)
        ),
    }
}

fn phase_label(chart: &Chart) -> &'static str {
    match chart.phase {
        Phase::I => "fase I (limites estimados destes dados)",
        Phase::II => "fase II (limites congelados)",
    }
}

fn const_or_range(v: &[f64]) -> String {
    let (lo, hi) = v.iter().fold((f64::INFINITY, f64::NEG_INFINITY), |a, &x| {
        (a.0.min(x), a.1.max(x))
    });
    if (hi - lo).abs() <= 1e-12 * hi.abs().max(1.0) {
        num(lo)
    } else {
        format!("{} a {}", num(lo), num(hi))
    }
}

/// Text summary of a control chart.
pub fn chart_text(title: &str, chart: &Chart) -> String {
    let mut s = String::new();
    let kind = chart.kind.map_or("limites fixos", |k| k.label());
    let _ = writeln!(s, "{title}: carta {kind}, {}", phase_label(chart));
    let _ = writeln!(s, "  {}", limits_line(chart));
    let _ = writeln!(
        s,
        "  regras: {}",
        chart
            .rules
            .0
            .iter()
            .map(|r| r.code())
            .collect::<Vec<_>>()
            .join(", ")
    );
    for p in &chart.panels {
        let _ = writeln!(
            s,
            "\n{}: LC {}, LIC {}, LSC {}",
            p.name,
            const_or_range(&p.center),
            const_or_range(&p.lcl),
            const_or_range(&p.ucl)
        );
        if p.violations.is_empty() {
            let _ = writeln!(s, "  nenhum sinal");
        }
        for v in &p.violations {
            let _ = writeln!(
                s,
                "  subgrupo {:>4}: {} ({})",
                v.index + 1,
                v.rule.code(),
                v.rule.describe()
            );
        }
    }
    for n in &chart.notes {
        let _ = writeln!(s, "nota: {n}");
    }
    let _ = write!(
        s,
        "\n{}",
        if chart.in_control() {
            "Processo sob controle estatístico pelas regras escolhidas."
        } else {
            "Há sinais de causa especial: investigue os subgrupos listados."
        }
    );
    s
}

/// SVG of one panel: values, centre and limits (stepped when they vary), signals in the
/// highlight colour, excluded points hollow.
pub fn panel_svg(panel: &Panel, excluded: &[usize]) -> String {
    let k = panel.values.len().max(1);
    let (w, h, left, right, top, bottom) = (900.0, 240.0, 56.0, 90.0, 12.0, 26.0);
    let mut lo = f64::INFINITY;
    let mut hi = f64::NEG_INFINITY;
    for i in 0..panel.values.len() {
        for v in [Some(panel.lcl[i]), Some(panel.ucl[i]), panel.values[i]]
            .into_iter()
            .flatten()
        {
            if v.is_finite() {
                lo = lo.min(v);
                hi = hi.max(v);
            }
        }
    }
    if !(hi > lo) {
        hi = lo + 1.0;
    }
    let pad = 0.06 * (hi - lo);
    let (lo, hi) = (lo - pad, hi + pad);
    let dx = (w - left - right) / k as f64;
    let x = |i: usize| left + dx * (i as f64 + 0.5);
    let y = |v: f64| top + (h - top - bottom) * (1.0 - (v - lo) / (hi - lo));
    let mut s = format!(
        "<svg viewBox=\"0 0 {w} {h}\" role=\"img\" aria-label=\"{}\">",
        esc(&panel.name)
    );
    let _ = write!(
        s,
        "<line class=\"axis\" x1=\"{left}\" x2=\"{left}\" y1=\"{top}\" y2=\"{:.1}\"/>\
         <line class=\"axis\" x1=\"{left}\" x2=\"{:.1}\" y1=\"{:.1}\" y2=\"{:.1}\"/>",
        h - bottom,
        w - right,
        h - bottom,
        h - bottom
    );
    // Y ticks.
    for t in 0..=4 {
        let v = lo + (hi - lo) * t as f64 / 4.0;
        let _ = write!(
            s,
            "<text x=\"{:.1}\" y=\"{:.1}\" text-anchor=\"end\">{}</text>",
            left - 4.0,
            y(v) + 4.0,
            esc(&num(v))
        );
    }
    let stepped = |vals: &[f64]| -> String {
        let mut d = String::new();
        for (i, &v) in vals.iter().enumerate() {
            let (a, b) = (x(i) - dx / 2.0, x(i) + dx / 2.0);
            let _ = write!(
                d,
                "{}{a:.1},{yv:.1} L{b:.1},{yv:.1} ",
                if i == 0 { "M" } else { "L" },
                yv = y(v)
            );
        }
        d
    };
    let _ = write!(
        s,
        "<path class=\"lim\" d=\"{}\"/><path class=\"lim\" d=\"{}\"/><path class=\"center\" d=\"{}\"/>",
        stepped(&panel.ucl),
        stepped(&panel.lcl),
        stepped(&panel.center)
    );
    let last = panel.values.len().saturating_sub(1);
    for (label, v) in [
        ("LSC", panel.ucl.get(last)),
        ("LC", panel.center.get(last)),
        ("LIC", panel.lcl.get(last)),
    ] {
        if let Some(&v) = v {
            let _ = write!(
                s,
                "<text x=\"{:.1}\" y=\"{:.1}\">{label} {}</text>",
                w - right + 6.0,
                y(v) + 4.0,
                esc(&num(v))
            );
        }
    }
    let mut d = String::new();
    for (i, v) in panel.values.iter().enumerate() {
        if let Some(v) = v {
            let _ = write!(
                d,
                "{}{:.1},{:.1} ",
                if d.is_empty() { "M" } else { "L" },
                x(i),
                y(*v)
            );
        }
    }
    let _ = write!(s, "<path class=\"series\" d=\"{d}\"/>");
    for (i, v) in panel.values.iter().enumerate() {
        let Some(v) = v else { continue };
        let class = if excluded.contains(&i) {
            "ptex"
        } else if panel.violations.iter().any(|q| q.index == i) {
            "pthot"
        } else {
            "pt"
        };
        let tip: Vec<String> = panel
            .violations
            .iter()
            .filter(|q| q.index == i)
            .map(|q| q.rule.code())
            .collect();
        let _ = write!(
            s,
            "<circle class=\"{class}\" cx=\"{:.1}\" cy=\"{:.1}\" r=\"3\"><title>subgrupo {}: {}{}</title></circle>",
            x(i),
            y(*v),
            i + 1,
            esc(&num(*v)),
            if tip.is_empty() {
                String::new()
            } else {
                format!(" ({})", tip.join(", "))
            }
        );
    }
    // X ticks: about 10 labels.
    let step = (k / 10).max(1);
    for i in (0..k).step_by(step) {
        let _ = write!(
            s,
            "<text x=\"{:.1}\" y=\"{:.1}\" text-anchor=\"middle\">{}</text>",
            x(i),
            h - 8.0,
            i + 1
        );
    }
    s.push_str("</svg>");
    s
}

fn violations_table(chart: &Chart) -> String {
    let mut h = String::new();
    let any = chart.panels.iter().any(|p| !p.violations.is_empty());
    if !any {
        h.push_str("<p>Nenhum sinal pelas regras escolhidas.</p>");
        return h;
    }
    h.push_str("<table><tr><th>Carta</th><th>Subgrupo</th><th>Regra</th><th>Descrição</th></tr>");
    for p in &chart.panels {
        for v in &p.violations {
            let _ = write!(
                h,
                "<tr><td>{}</td><td class=\"n\">{}</td><td>{}</td><td>{}</td></tr>",
                esc(&p.name),
                v.index + 1,
                esc(&v.rule.code()),
                esc(&v.rule.describe())
            );
        }
    }
    h.push_str("</table>");
    h
}

/// HTML report of a control chart (or of any list of panels, e.g. EWMA and CUSUM).
pub fn chart_html(title: &str, chart: &Chart) -> String {
    let kind = chart.kind.map_or("limites fixos", |k| k.label());
    let mut b = String::new();
    let _ = write!(
        b,
        "<p>Carta <b>{}</b>, {}. {}.</p>",
        esc(kind),
        esc(phase_label(chart)),
        esc(&limits_line(chart))
    );
    for p in &chart.panels {
        let _ = write!(b, "<h2>{}</h2>", esc(&p.name));
        b.push_str(&panel_svg(p, &chart.excluded));
    }
    b.push_str("<h2>Sinais</h2>");
    let _ = write!(
        b,
        "<p class=\"muted\">Regras: {}. As regras de zona e de sequência valem na carta de \
         posição; na de dispersão, só pontos fora dos limites. Pontos vazados: subgrupos \
         excluídos da estimativa (causa atribuível).</p>",
        esc(&chart
            .rules
            .0
            .iter()
            .map(|r| format!("{} ({})", r.code(), r.describe()))
            .collect::<Vec<_>>()
            .join("; "))
    );
    b.push_str(&violations_table(chart));
    b.push_str(&notes_html(&chart.notes));
    page(
        title,
        &format!(
            "Carta de controle, {} subgrupos.",
            chart.panels[0].values.len()
        ),
        &b,
    )
}

/// HTML report of panels that are not a Shewhart chart (EWMA, CUSUM).
pub fn panels_html(title: &str, subtitle: &str, panels: &[Panel], notes: &[String]) -> String {
    let mut b = String::new();
    for p in panels {
        let _ = write!(b, "<h2>{}</h2>", esc(&p.name));
        b.push_str(&panel_svg(p, &[]));
        if p.violations.is_empty() {
            b.push_str("<p>Nenhum sinal.</p>");
        } else {
            let _ = write!(
                b,
                "<p class=\"hot\">Sinal nos pontos {}.</p>",
                p.violations
                    .iter()
                    .map(|v| (v.index + 1).to_string())
                    .collect::<Vec<_>>()
                    .join(", ")
            );
        }
    }
    b.push_str(&notes_html(notes));
    page(title, subtitle, &b)
}

// ---------------------------------------------------------------- capability

fn spec_line(spec: &Spec) -> String {
    format!(
        "LIE = {}, LSE = {}, alvo = {}",
        opt(spec.lsl),
        opt(spec.usl),
        opt(spec.target_or_mid())
    )
}

fn index_text(i: &crate::capability::Index) -> String {
    format!("{} [{} ; {}]", num(i.value), num(i.lo), num(i.hi))
}

/// Text summary of a capability study.
pub fn capability_text(title: &str, c: &Capability) -> String {
    let mut s = String::new();
    let _ = writeln!(s, "{title}: capacidade do processo, {} valores", c.n);
    let _ = writeln!(s, "  {}", spec_line(&c.spec));
    let _ = writeln!(
        s,
        "  média {}, σ dentro {} ({}), σ global {}",
        num(c.mean),
        num(c.sigma_within),
        c.within_method.label(),
        num(c.sigma_overall)
    );
    let lv = format!("{:.0}%", 100.0 * c.level);
    let _ = writeln!(s, "\nCapacidade potencial (σ dentro), IC de {lv}:");
    let _ = writeln!(s, "  Cp  {}", c.cp.as_ref().map_or("—".into(), index_text));
    let _ = writeln!(s, "  Cpu {}   Cpl {}", opt(c.cpu), opt(c.cpl));
    let _ = writeln!(s, "  Cpk {} (Bissell)", index_text(&c.cpk));
    let _ = writeln!(s, "Desempenho global (σ global):");
    let _ = writeln!(s, "  Pp  {}", c.pp.as_ref().map_or("—".into(), index_text));
    let _ = writeln!(s, "  Ppu {}   Ppl {}", opt(c.ppu), opt(c.ppl));
    let _ = writeln!(s, "  Ppk {} (Bissell)", index_text(&c.ppk));
    let _ = writeln!(s, "  Cpm {}", opt(c.cpm));
    let _ = writeln!(
        s,
        "\nPPM fora da especificação     abaixo       acima       total"
    );
    for (name, p) in [
        ("observado", c.ppm_observed),
        ("esperado (σ dentro)", c.ppm_within),
        ("esperado (σ global)", c.ppm_overall),
    ] {
        let _ = writeln!(
            s,
            "  {name:<27} {:>10} {:>11} {:>11}",
            num(p.below),
            num(p.above),
            num(p.total)
        );
    }
    let _ = writeln!(
        s,
        "\nZ bench: {} (dentro), {} (global)",
        num(c.z_bench_within),
        num(c.z_bench_overall)
    );
    let _ = writeln!(
        s,
        "Nível sigma: {} = Z bench global {} + deslocamento de {}σ",
        num(c.sigma_level),
        num(c.z_bench_overall),
        num(c.sigma_shift)
    );
    if let Some(ad) = &c.normality {
        let _ = writeln!(
            s,
            "Normalidade (Anderson-Darling): A² = {}, A*² = {}, p = {}",
            num(ad.a2),
            num(ad.a2_star),
            num(ad.p_value)
        );
    }
    for n in &c.notes {
        let _ = writeln!(s, "nota: {n}");
    }
    s
}

/// Text of the Box-Cox study.
pub fn box_cox_text(title: &str, r: &BoxCoxCapability) -> String {
    let mut s = format!(
        "Box-Cox: λ̂ = {}, IC [{} ; {}], λ usado = {}{}\n",
        num(r.fit.lambda_hat),
        num(r.fit.ci.0),
        num(r.fit.ci.1),
        num(r.fit.lambda),
        if r.fit.rounded {
            " (arredondado para um valor usual dentro do IC)"
        } else {
            ""
        }
    );
    if let (Some(a), Some(b)) = (&r.before, &r.transformed.normality) {
        let _ = writeln!(
            s,
            "Normalidade antes: p = {}; depois: p = {}",
            num(a.p_value),
            num(b.p_value)
        );
    }
    s.push_str(&capability_text(
        &format!("{title} (escala transformada)"),
        &r.transformed,
    ));
    s
}

/// Text of the percentile method.
pub fn percentile_text(source: &str, p: &PercentileIndices) -> String {
    format!(
        "Método dos percentis ({source}): P0,135 = {}, P50 = {}, P99,865 = {}\n  Pp {}   Ppu {}   Ppl {}   Ppk {}\n",
        num(p.p_low),
        num(p.median),
        num(p.p_high),
        opt(p.pp),
        opt(p.ppu),
        opt(p.ppl),
        num(p.ppk)
    )
}

/// Histogram with the specification limits and the normal curves (σ global solid,
/// σ dentro dashed).
pub fn capability_svg(values: &[f64], c: &Capability) -> String {
    let (w, h, left, right, top, bottom) = (900.0, 280.0, 50.0, 20.0, 18.0, 28.0);
    let n = values.len().max(1);
    let mut lo = values.iter().copied().fold(f64::INFINITY, f64::min);
    let mut hi = values.iter().copied().fold(f64::NEG_INFINITY, f64::max);
    for v in [c.spec.lsl, c.spec.usl].into_iter().flatten() {
        lo = lo.min(v);
        hi = hi.max(v);
    }
    let sd = c.sigma_overall.max(c.sigma_within);
    lo = lo.min(c.mean - 3.5 * sd);
    hi = hi.max(c.mean + 3.5 * sd);
    if !(hi > lo) {
        hi = lo + 1.0;
    }
    // Sturges' rule over the data range.
    let bins = ((n as f64).log2().ceil() as usize + 1).max(5);
    let dmin = values.iter().copied().fold(f64::INFINITY, f64::min);
    let dmax = values.iter().copied().fold(f64::NEG_INFINITY, f64::max);
    let bw = if dmax > dmin {
        (dmax - dmin) / bins as f64
    } else {
        1.0
    };
    let mut counts = vec![0usize; bins];
    for &v in values {
        let b = (((v - dmin) / bw) as usize).min(bins - 1);
        counts[b] += 1;
    }
    let pdf = |x: f64, s: f64| {
        (-(x - c.mean).powi(2) / (2.0 * s * s)).exp() / (s * (2.0 * std::f64::consts::PI).sqrt())
    };
    let peak = (counts.iter().copied().max().unwrap_or(1) as f64)
        .max(n as f64 * bw * pdf(c.mean, c.sigma_within))
        .max(n as f64 * bw * pdf(c.mean, c.sigma_overall))
        * 1.08;
    let x = |v: f64| left + (w - left - right) * (v - lo) / (hi - lo);
    let y = |v: f64| top + (h - top - bottom) * (1.0 - v / peak);
    let mut s = format!(
        "<svg viewBox=\"0 0 {w} {h}\" role=\"img\" aria-label=\"Histograma e especificação\">"
    );
    for (i, &cnt) in counts.iter().enumerate() {
        let a = dmin + bw * i as f64;
        let _ = write!(
            s,
            "<rect class=\"bar\" x=\"{:.1}\" y=\"{:.1}\" width=\"{:.1}\" height=\"{:.1}\"><title>{} a {}: {cnt}</title></rect>",
            x(a),
            y(cnt as f64),
            (x(a + bw) - x(a) - 1.0).max(0.5),
            y(0.0) - y(cnt as f64),
            esc(&num(a)),
            esc(&num(a + bw))
        );
    }
    for (class, sig) in [("curve", c.sigma_overall), ("curve2", c.sigma_within)] {
        let mut d = String::new();
        for k in 0..=200 {
            let v = lo + (hi - lo) * k as f64 / 200.0;
            let _ = write!(
                d,
                "{}{:.1},{:.1} ",
                if k == 0 { "M" } else { "L" },
                x(v),
                y(n as f64 * bw * pdf(v, sig))
            );
        }
        let _ = write!(s, "<path class=\"{class}\" d=\"{d}\"/>");
    }
    let base = h - bottom;
    let _ = write!(
        s,
        "<line class=\"axis\" x1=\"{left}\" x2=\"{:.1}\" y1=\"{base}\" y2=\"{base}\"/>",
        w - right
    );
    for (label, v, class) in [
        ("LIE", c.spec.lsl, "spec"),
        ("LSE", c.spec.usl, "spec"),
        ("alvo", c.spec.target_or_mid(), "target"),
    ] {
        if let Some(v) = v {
            let _ = write!(
                s,
                "<line class=\"{class}\" x1=\"{xv:.1}\" x2=\"{xv:.1}\" y1=\"{top}\" y2=\"{base}\"/>\
                 <text x=\"{xv:.1}\" y=\"{:.1}\" text-anchor=\"middle\">{label} {}</text>",
                top - 4.0,
                esc(&num(v)),
                xv = x(v)
            );
        }
    }
    for t in 0..=6 {
        let v = lo + (hi - lo) * t as f64 / 6.0;
        let _ = write!(
            s,
            "<text x=\"{:.1}\" y=\"{:.1}\" text-anchor=\"middle\">{}</text>",
            x(v),
            h - 8.0,
            esc(&num(v))
        );
    }
    s.push_str("</svg>");
    s
}

fn capability_tables(c: &Capability) -> String {
    let mut b = String::new();
    let lv = format!("{:.0}%", 100.0 * c.level);
    let idx = |i: Option<&crate::capability::Index>| match i {
        Some(i) => format!(
            "<td class=\"n\">{}</td><td class=\"n\">{} a {}</td>",
            esc(&num(i.value)),
            esc(&num(i.lo)),
            esc(&num(i.hi))
        ),
        None => "<td class=\"n\">—</td><td></td>".into(),
    };
    let one = |v: Option<f64>| format!("<td class=\"n\">{}</td><td></td>", esc(&opt(v)));
    let _ = write!(
        b,
        "<p>{} valores, média {}, σ dentro {} por {}, σ global {}. {}.</p>\
         <table><tr><th>Índice</th><th>Valor</th><th>IC de {lv}</th><th>Índice</th><th>Valor</th><th>IC de {lv}</th></tr>\
         <tr><td>Cp</td>{}<td>Pp</td>{}</tr>\
         <tr><td>Cpu</td>{}<td>Ppu</td>{}</tr>\
         <tr><td>Cpl</td>{}<td>Ppl</td>{}</tr>\
         <tr><td>Cpk</td>{}<td>Ppk</td>{}</tr>\
         <tr><td>Cpm</td>{}<td></td><td></td><td></td></tr></table>\
         <p class=\"muted\">IC de Cp e Pp pela distribuição χ²; de Cpk e Ppk pela aproximação de \
         Bissell. σ dentro mede a variação de curto prazo (dentro dos subgrupos); σ global, a de \
         todos os valores juntos.</p>",
        c.n,
        esc(&num(c.mean)),
        esc(&num(c.sigma_within)),
        esc(c.within_method.label()),
        esc(&num(c.sigma_overall)),
        esc(&spec_line(&c.spec)),
        idx(c.cp.as_ref()),
        idx(c.pp.as_ref()),
        one(c.cpu),
        one(c.ppu),
        one(c.cpl),
        one(c.ppl),
        idx(Some(&c.cpk)),
        idx(Some(&c.ppk)),
        one(c.cpm),
    );
    b.push_str(
        "<h3>Fora da especificação (PPM)</h3><table><tr><th></th><th>Abaixo do LIE</th>\
         <th>Acima do LSE</th><th>Total</th></tr>",
    );
    for (name, p) in [
        ("Observado", c.ppm_observed),
        ("Esperado (σ dentro)", c.ppm_within),
        ("Esperado (σ global)", c.ppm_overall),
    ] {
        let _ = write!(
            b,
            "<tr><td>{name}</td><td class=\"n\">{}</td><td class=\"n\">{}</td><td class=\"n\">{}</td></tr>",
            esc(&num(p.below)),
            esc(&num(p.above)),
            esc(&num(p.total))
        );
    }
    b.push_str("</table>");
    let _ = write!(
        b,
        "<p>Z bench: {} (dentro), {} (global). <b>Nível sigma: {}</b> = Z bench global + \
         deslocamento de {}σ (convenção do Seis Sigma para o longo prazo; sem o deslocamento, \
         o nível é o próprio Z).</p>",
        esc(&num(c.z_bench_within)),
        esc(&num(c.z_bench_overall)),
        esc(&num(c.sigma_level)),
        esc(&num(c.sigma_shift))
    );
    if let Some(ad) = &c.normality {
        let _ = write!(
            b,
            "<p>Normalidade (Anderson-Darling, parâmetros estimados): A² = {}, A*² = {}, \
             p = {}{}.</p>",
            esc(&num(ad.a2)),
            esc(&num(ad.a2_star)),
            esc(&num(ad.p_value)),
            if ad.p_value < 0.05 {
                ": <span class=\"hot\">a normalidade é rejeitada a 5%</span>"
            } else {
                ""
            }
        );
    }
    b.push_str(&notes_html(&c.notes));
    b
}

/// HTML report of a capability study, with the optional non-normal analyses.
pub fn capability_html(
    title: &str,
    values: &[f64],
    c: &Capability,
    box_cox: Option<&BoxCoxCapability>,
    percentile: Option<(&str, &PercentileIndices)>,
) -> String {
    let mut b = String::new();
    b.push_str("<h2>Histograma</h2>");
    b.push_str(&capability_svg(values, c));
    b.push_str(
        "<p class=\"muted\">Barras: dados (classes pela regra de Sturges). Curva cheia: normal \
         com σ global; tracejada: com σ dentro.</p><h2>Índices (dados supostos normais)</h2>",
    );
    b.push_str(&capability_tables(c));
    if let Some(r) = box_cox {
        let _ = write!(
            b,
            "<h2>Transformação Box-Cox</h2><p>λ̂ = {}, IC [{} ; {}]; λ usado = {}{}. Dados, \
             limites e alvo transformados por y = (x^λ − 1)/λ (ln x em λ = 0).</p>",
            esc(&num(r.fit.lambda_hat)),
            esc(&num(r.fit.ci.0)),
            esc(&num(r.fit.ci.1)),
            esc(&num(r.fit.lambda)),
            if r.fit.rounded {
                " (valor usual dentro do IC)"
            } else {
                ""
            }
        );
        b.push_str(&capability_tables(&r.transformed));
    }
    if let Some((source, p)) = percentile {
        let _ = write!(
            b,
            "<h2>Método dos percentis</h2><p>Distribuição: {}. P0,135 = {}, P50 = {}, \
             P99,865 = {}.</p><table><tr><th>Pp</th><th>Ppu</th><th>Ppl</th><th>Ppk</th></tr>\
             <tr><td class=\"n\">{}</td><td class=\"n\">{}</td><td class=\"n\">{}</td>\
             <td class=\"n\">{}</td></tr></table><p class=\"muted\">Ppu = (LSE − P50)/(P99,865 − \
             P50), Ppl = (P50 − LIE)/(P50 − P0,135); na normal, iguais aos índices clássicos.</p>",
            esc(source),
            esc(&num(p.p_low)),
            esc(&num(p.median)),
            esc(&num(p.p_high)),
            esc(&opt(p.pp)),
            esc(&opt(p.ppu)),
            esc(&opt(p.ppl)),
            esc(&num(p.ppk))
        );
    }
    page(title, "Capacidade do processo.", &b)
}

// ---------------------------------------------------------------- MSA

/// Text of one or more Gage R&R studies.
pub fn gage_text(title: &str, studies: &[GageRR]) -> String {
    let mut s = String::new();
    for g in studies {
        let _ = writeln!(
            s,
            "{title}: Gage R&R por {} ({} peças × {} operadores × {} repetições)",
            g.method, g.parts, g.operators, g.reps
        );
        if !g.anova.is_empty() {
            let _ = writeln!(
                s,
                "  {:<18} {:>4} {:>12} {:>12} {:>9} {:>8}",
                "Fonte", "gl", "SQ", "QM", "F", "p"
            );
            for r in &g.anova {
                let _ = writeln!(
                    s,
                    "  {:<18} {:>4} {:>12} {:>12} {:>9} {:>8}",
                    r.source,
                    r.df,
                    num(r.ss),
                    num(r.ms),
                    opt(r.f),
                    opt(r.p)
                );
            }
        }
        let _ = writeln!(
            s,
            "  {:<24} {:>12} {:>9} {:>12} {:>8} {:>8}",
            "Componente",
            "Variância",
            "%Contr.",
            format!("VE ({}σ)", short(g.k)),
            "%VE",
            "%Tol."
        );
        for c in &g.components {
            let _ = writeln!(
                s,
                "  {:<24} {:>12} {:>9} {:>12} {:>8} {:>8}",
                c.name,
                num(c.variance),
                pct(c.pct_contribution),
                num(c.study_var),
                pct(c.pct_study_var),
                c.pct_tolerance.map_or("—".into(), pct)
            );
        }
        let _ = writeln!(
            s,
            "  Número de categorias distintas (ndc): {} ({})",
            g.ndc,
            num(g.ndc_raw)
        );
        let _ = writeln!(s, "  Sistema de medição: {}", g.verdict());
        for w in &g.warnings {
            let _ = writeln!(s, "  nota: {w}");
        }
        s.push('\n');
    }
    s
}

fn gage_section(g: &GageRR) -> String {
    let mut b = String::new();
    let _ = write!(
        b,
        "<h2>Gage R&amp;R por {}</h2><p>{} peças × {} operadores × {} repetições. \
         Sistema de medição: <b>{}</b>. ndc = {} ({}).</p>",
        esc(&g.method),
        g.parts,
        g.operators,
        g.reps,
        esc(g.verdict()),
        g.ndc,
        esc(&num(g.ndc_raw))
    );
    if !g.anova.is_empty() {
        b.push_str("<h3>ANOVA</h3><table><tr><th>Fonte</th><th>gl</th><th>SQ</th><th>QM</th><th>F</th><th>p</th></tr>");
        for r in &g.anova {
            let _ = write!(
                b,
                "<tr><td>{}</td><td class=\"n\">{}</td><td class=\"n\">{}</td><td class=\"n\">{}</td><td class=\"n\">{}</td><td class=\"n\">{}</td></tr>",
                esc(&r.source),
                r.df,
                esc(&num(r.ss)),
                esc(&num(r.ms)),
                esc(&opt(r.f)),
                esc(&opt(r.p))
            );
        }
        b.push_str("</table>");
    }
    let _ = write!(
        b,
        "<h3>Componentes</h3><table><tr><th>Componente</th><th>Variância</th><th>%Contribuição</th>\
         <th>Desvio</th><th>VE ({}σ)</th><th>%VE</th><th>%Tolerância</th></tr>",
        esc(&short(g.k))
    );
    for c in &g.components {
        let hot = if c.name == crate::msa::GRR {
            " class=\"hot\""
        } else {
            ""
        };
        let _ = write!(
            b,
            "<tr{hot}><td>{}</td><td class=\"n\">{}</td><td class=\"n\">{}</td><td class=\"n\">{}</td><td class=\"n\">{}</td><td class=\"n\">{}</td><td class=\"n\">{}</td></tr>",
            esc(&c.name),
            esc(&num(c.variance)),
            esc(&pct(c.pct_contribution)),
            esc(&num(c.sd)),
            esc(&num(c.study_var)),
            esc(&pct(c.pct_study_var)),
            esc(&c.pct_tolerance.map_or("—".into(), pct))
        );
    }
    b.push_str("</table>");
    b.push_str(&gage_svg(g));
    b.push_str(&notes_html(&g.warnings));
    b
}

/// Bars of %contribution and %study variation per component (without the total).
pub fn gage_svg(g: &GageRR) -> String {
    let comps: Vec<_> = g
        .components
        .iter()
        .filter(|c| c.name != crate::msa::TOTAL)
        .collect();
    let (w, row, left, right) = (900.0, 30.0, 190.0, 70.0);
    let h = row * comps.len() as f64 + 30.0;
    let x = |v: f64| left + (w - left - right) * (v / 100.0).clamp(0.0, 1.0);
    let mut s = format!(
        "<svg viewBox=\"0 0 {w} {h}\" role=\"img\" aria-label=\"Componentes da variação\">"
    );
    for (k, c) in comps.iter().enumerate() {
        let y = 6.0 + row * k as f64;
        let _ = write!(
            s,
            "<text x=\"{:.1}\" y=\"{:.1}\" text-anchor=\"end\">{}</text>\
             <rect class=\"bar\" x=\"{left}\" y=\"{y:.1}\" width=\"{:.1}\" height=\"10\"/>\
             <rect class=\"bar2\" x=\"{left}\" y=\"{:.1}\" width=\"{:.1}\" height=\"10\"/>\
             <text x=\"{:.1}\" y=\"{:.1}\">{}</text><text x=\"{:.1}\" y=\"{:.1}\">{}</text>",
            left - 8.0,
            y + 14.0,
            esc(&c.name),
            (x(c.pct_contribution) - left).max(0.5),
            y + 11.0,
            (x(c.pct_study_var) - left).max(0.5),
            x(c.pct_contribution) + 4.0,
            y + 9.0,
            esc(&pct(c.pct_contribution)),
            x(c.pct_study_var) + 4.0,
            y + 20.0,
            esc(&pct(c.pct_study_var))
        );
    }
    let _ = write!(
        s,
        "<text x=\"{left}\" y=\"{:.1}\">azul: %contribuição (variâncias); laranja: %VE (desvios)</text></svg>",
        h - 4.0
    );
    s
}

/// HTML report of one or more Gage R&R studies.
pub fn gage_html(title: &str, studies: &[GageRR]) -> String {
    let mut b = String::new();
    for g in studies {
        b.push_str(&gage_section(g));
    }
    b.push_str(
        "<p class=\"muted\">Critério usual: %VE do GRR até 10% aceitável, de 10% a 30% \
         condicional, acima de 30% inaceitável; ndc ≥ 5. Cada empresa pode adotar o seu.</p>",
    );
    page(title, "Análise do sistema de medição (variáveis).", &b)
}

/// Text of an attribute agreement study.
pub fn attribute_text(title: &str, a: &AttributeStudy) -> String {
    let mut s = String::new();
    let lv = "IC exato";
    let _ = writeln!(
        s,
        "{title}: concordância por atributos ({} itens, {} avaliadores, {} avaliações por item; \
         categorias {})",
        a.items,
        a.appraisers.len(),
        a.trials,
        a.categories.join(", ")
    );
    for r in &a.appraisers {
        let _ = writeln!(s, "  {}:", r.appraiser);
        if let Some(w) = &r.within {
            let _ = writeln!(
                s,
                "    dentro do avaliador: {}/{} = {} ({lv} {} a {}), κ = {}",
                w.matched,
                w.inspected,
                pct(w.percent),
                pct(w.ci.0),
                pct(w.ci.1),
                opt(r.kappa_within)
            );
        }
        if let Some(w) = &r.vs_standard {
            let _ = writeln!(
                s,
                "    contra o padrão: {}/{} = {} ({lv} {} a {}), κ = {}",
                w.matched,
                w.inspected,
                pct(w.percent),
                pct(w.ci.0),
                pct(w.ci.1),
                opt(r.kappa_vs_standard)
            );
        }
    }
    let b = &a.between;
    let _ = writeln!(
        s,
        "  entre avaliadores: {}/{} = {} ({lv} {} a {})",
        b.matched,
        b.inspected,
        pct(b.percent),
        pct(b.ci.0),
        pct(b.ci.1)
    );
    if let Some(w) = &a.all_vs_standard {
        let _ = writeln!(
            s,
            "  todos contra o padrão: {}/{} = {} ({lv} {} a {})",
            w.matched,
            w.inspected,
            pct(w.percent),
            pct(w.ci.0),
            pct(w.ci.1)
        );
    }
    if let Some(f) = &a.fleiss_all {
        let _ = writeln!(
            s,
            "  kappa de Fleiss (todas as avaliações): {} (EP {}, z = {}, p = {})",
            num(f.overall.kappa),
            num(f.overall.se0),
            num(f.overall.z),
            num(f.overall.p_value)
        );
        for &(j, p, kj, _, pv) in &f.per_category {
            let _ = writeln!(
                s,
                "    categoria {}: proporção {}, κ = {}, p = {}",
                a.categories[j],
                num(p),
                num(kj),
                num(pv)
            );
        }
    }
    for w in &a.warnings {
        let _ = writeln!(s, "  nota: {w}");
    }
    s
}

/// HTML report of an attribute agreement study.
pub fn attribute_html(title: &str, a: &AttributeStudy) -> String {
    let text = attribute_text(title, a);
    let mut b = String::from(
        "<table><tr><th>Avaliador</th><th>Dentro do avaliador</th><th>κ dentro</th>\
         <th>Contra o padrão</th><th>κ contra o padrão</th></tr>",
    );
    let ag = |x: &Option<crate::attribute::Agreement>| match x {
        Some(w) => format!(
            "{}/{} = {} ({} a {})",
            w.matched,
            w.inspected,
            pct(w.percent),
            pct(w.ci.0),
            pct(w.ci.1)
        ),
        None => "—".into(),
    };
    for r in &a.appraisers {
        let _ = write!(
            b,
            "<tr><td>{}</td><td>{}</td><td class=\"n\">{}</td><td>{}</td><td class=\"n\">{}</td></tr>",
            esc(&r.appraiser),
            esc(&ag(&r.within)),
            esc(&opt(r.kappa_within)),
            esc(&ag(&r.vs_standard)),
            esc(&opt(r.kappa_vs_standard))
        );
    }
    b.push_str("</table>");
    let _ = write!(b, "<h2>Resumo</h2><pre>{}</pre>", esc(&text));
    b.push_str(
        "<p class=\"muted\">Percentuais com intervalo exato de Clopper-Pearson de 95%. Kappa de \
         Cohen contra o padrão; de Fleiss entre as avaliações repetidas e entre todos.</p>",
    );
    page(title, "Análise do sistema de medição por atributos.", &b)
}

/// Text of a bias and linearity study.
pub fn bias_text(title: &str, lin: Option<&Linearity>, single: Option<&Bias>) -> String {
    let mut s = format!("{title}: viés e linearidade\n");
    let line = |b: &Bias| {
        format!(
            "  referência {}: n = {}, média {}, viés {} (IC {} a {}), t = {}, p = {}\n",
            num(b.reference),
            b.n,
            num(b.mean),
            num(b.bias),
            num(b.ci.0),
            num(b.ci.1),
            num(b.t),
            num(b.p_value)
        )
    };
    if let Some(b) = single {
        s.push_str(&line(b));
    }
    if let Some(l) = lin {
        for b in &l.per_reference {
            s.push_str(&line(b));
        }
        let _ = writeln!(
            s,
            "  viés = {} {} {} · referência (p da inclinação {}, p do intercepto {}, R² = {})",
            num(l.intercept),
            if l.slope < 0.0 { "−" } else { "+" },
            num(l.slope.abs()),
            num(l.p_slope),
            num(l.p_intercept),
            num(l.r2)
        );
        let _ = writeln!(
            s,
            "  %linearidade = {}{}",
            pct(l.pct_linearity),
            l.linearity
                .map_or(String::new(), |v| format!(", linearidade = {}", num(v)))
        );
        let _ = writeln!(
            s,
            "  {}",
            if l.p_slope < 0.05 {
                "o viés muda com a referência (inclinação significativa a 5%)"
            } else {
                "sem evidência de que o viés mude com a referência"
            }
        );
    }
    s
}

/// HTML report of a bias and linearity study, with the scatter of the bias.
pub fn bias_html(
    title: &str,
    pairs: &[(f64, f64)],
    lin: Option<&Linearity>,
    single: Option<&Bias>,
) -> String {
    let mut b = String::new();
    if let Some(l) = lin {
        let (w, h, left, right, top, bottom) = (900.0, 280.0, 60.0, 20.0, 14.0, 28.0);
        let xs: Vec<f64> = pairs.iter().map(|p| p.0).collect();
        let ys: Vec<f64> = pairs.iter().map(|p| p.1 - p.0).collect();
        let (x0, x1) = (
            xs.iter().copied().fold(f64::INFINITY, f64::min),
            xs.iter().copied().fold(f64::NEG_INFINITY, f64::max),
        );
        let (mut y0, mut y1) = (
            ys.iter().copied().fold(0.0f64, f64::min),
            ys.iter().copied().fold(0.0f64, f64::max),
        );
        if !(y1 > y0) {
            y1 = y0 + 1.0;
        }
        let pad = 0.08 * (y1 - y0);
        y0 -= pad;
        y1 += pad;
        let xr = if x1 > x0 { x1 - x0 } else { 1.0 };
        let px = |v: f64| left + (w - left - right) * (v - x0 + 0.05 * xr) / (1.1 * xr);
        let py = |v: f64| top + (h - top - bottom) * (1.0 - (v - y0) / (y1 - y0));
        let mut s = format!(
            "<svg viewBox=\"0 0 {w} {h}\" role=\"img\" aria-label=\"Viés por referência\">"
        );
        let _ = write!(
            s,
            "<line class=\"center\" x1=\"{left}\" x2=\"{:.1}\" y1=\"{:.1}\" y2=\"{:.1}\"/>",
            w - right,
            py(0.0),
            py(0.0)
        );
        let (a, z) = (x0 - 0.05 * xr, x1 + 0.05 * xr);
        let _ = write!(
            s,
            "<line class=\"lim\" x1=\"{:.1}\" x2=\"{:.1}\" y1=\"{:.1}\" y2=\"{:.1}\"/>",
            px(a),
            px(z),
            py(l.intercept + l.slope * a),
            py(l.intercept + l.slope * z)
        );
        for (x, y) in xs.iter().zip(&ys) {
            let _ = write!(
                s,
                "<circle class=\"pt\" cx=\"{:.1}\" cy=\"{:.1}\" r=\"3\"/>",
                px(*x),
                py(*y)
            );
        }
        for t in 0..=4 {
            let v = y0 + (y1 - y0) * t as f64 / 4.0;
            let _ = write!(
                s,
                "<text x=\"{:.1}\" y=\"{:.1}\" text-anchor=\"end\">{}</text>",
                left - 4.0,
                py(v) + 4.0,
                esc(&num(v))
            );
        }
        let mut refs = xs.clone();
        refs.sort_by(|a, b| a.total_cmp(b));
        refs.dedup();
        for r in refs {
            let _ = write!(
                s,
                "<text x=\"{:.1}\" y=\"{:.1}\" text-anchor=\"middle\">{}</text>",
                px(r),
                h - 8.0,
                esc(&num(r))
            );
        }
        s.push_str("</svg>");
        b.push_str("<h2>Viés contra a referência</h2>");
        b.push_str(&s);
        b.push_str("<p class=\"muted\">Pontos: viés de cada medição; linha: reta ajustada; tracejado: viés zero.</p>");
    }
    let _ = write!(
        b,
        "<h2>Resultados</h2><pre>{}</pre>",
        esc(&bias_text(title, lin, single))
    );
    page(title, "Viés e linearidade do sistema de medição.", &b)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::charts::{ChartData, ChartKind, phase1};
    use crate::rules::RuleSet;

    #[test]
    fn numbers_use_the_decimal_comma() {
        assert_eq!(num(1.23456), "1,2346");
        assert_eq!(num(0.012346), "0,01235");
        assert_eq!(num(1234.5), "1234,5");
        assert_eq!(num(f64::INFINITY), "∞");
    }

    #[test]
    fn chart_report_is_self_contained() {
        let x = [5.0, 7.0, 6.0, 9.0, 8.0, 30.0, 7.0];
        let data = ChartData::Variables(x.iter().map(|&v| vec![v]).collect());
        let chart = phase1(ChartKind::IMr, &data, &[5], &RuleSet::western_electric()).unwrap();
        let html = chart_html("Teste <a>", &chart);
        assert!(html.starts_with("<!doctype html>"));
        assert!(html.contains("Teste &lt;a&gt;"));
        assert!(html.contains("<svg") && !html.contains("http"));
        assert!(html.contains("ptex"));
        let text = chart_text("Teste", &chart);
        assert!(text.contains("fase I"));
    }
}
