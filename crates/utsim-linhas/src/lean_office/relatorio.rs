//! The report of the Lean Office line: one self-contained HTML page (tables and inline
//! SVG charts, no external resources) and a text summary for the terminal.

use std::fmt::Write as _;

use utsim_core::stats::Summary;

use super::{CAUSES, DAY, Indicadores, mhw, num, num1, pct_hw};
use crate::pacote::Vocabulary;

pub(crate) const CSS: &str = r#"
:root{--bg:#fff;--fg:#1c1f24;--muted:#5d6672;--line:#d8dde3;--accent:#2563eb;--hot:#c2410c;--ok:#16a34a;--card:#f5f7fa}
@media(prefers-color-scheme:dark){:root{--bg:#14171b;--fg:#e6e9ee;--muted:#9aa4b0;--line:#2d343c;--accent:#6ea0ff;--hot:#fb923c;--ok:#4ade80;--card:#1d2228}}
body{font:15px/1.5 system-ui,sans-serif;background:var(--bg);color:var(--fg);max-width:1100px;margin:2rem auto;padding:0 1rem}
h1{font-size:1.5rem}h2{font-size:1.15rem;margin-top:2rem}
table{border-collapse:collapse;width:100%;font-size:.85rem}
th,td{padding:.3rem .5rem;border-bottom:1px solid var(--line);text-align:left}
th{color:var(--muted);font-weight:600}
td.n{text-align:right;font-variant-numeric:tabular-nums}
.muted{color:var(--muted)}.hot{color:var(--hot)}
.cards{display:grid;grid-template-columns:repeat(auto-fit,minmax(170px,1fr));gap:.6rem}
.card{background:var(--card);border-radius:8px;padding:.6rem .8rem}
.card b{display:block;font-size:1.2rem}
.rolagem{overflow-x:auto}
svg{max-width:100%;height:auto}
svg text{fill:var(--fg);font-size:11px}
svg .eixo{stroke:var(--muted);stroke-width:1}
svg .barra{fill:var(--accent);opacity:.85}
svg .linha{fill:none;stroke:var(--accent);stroke-width:1.6}
svg .acum{fill:none;stroke:var(--hot);stroke-width:1.6}
svg .sla{stroke:var(--hot);stroke-width:2;stroke-dasharray:5 3}
td.calor{text-align:center;font-size:.75rem}
"#;

pub(crate) fn esc(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

fn pct(x: f64) -> String {
    format!("{}%", num1(100.0 * x))
}

/// Line chart of the cases in the model over time.
fn grafico_wip(k: &Indicadores) -> String {
    let (w, h, m) = (760.0, 220.0, 40.0);
    let pts = &k.serie_wip;
    if pts.len() < 2 {
        return String::new();
    }
    let (t0, t1) = (pts[0].0, pts[pts.len() - 1].0);
    let ymax = pts.iter().map(|p| p.1).fold(1.0, f64::max);
    let x = |t: f64| m + (w - 2.0 * m) * (t - t0) / (t1 - t0).max(1e-9);
    let y = |v: f64| h - m + (m * 2.0 - h) * v / ymax;
    let mut path = String::new();
    for (i, (t, v)) in pts.iter().enumerate() {
        let _ = write!(
            path,
            "{}{:.1} {:.1}",
            if i == 0 { "M" } else { " L" },
            x(*t),
            y(*v)
        );
    }
    let mut s = format!(
        "<svg viewBox=\"0 0 {w} {h}\" role=\"img\" aria-label=\"WIP de casos no tempo\">\
         <line class=\"eixo\" x1=\"{m}\" y1=\"{b}\" x2=\"{r}\" y2=\"{b}\"/>\
         <line class=\"eixo\" x1=\"{m}\" y1=\"{m}\" x2=\"{m}\" y2=\"{b}\"/>\
         <path class=\"linha\" d=\"{path}\"/>",
        b = h - m,
        r = w - m
    );
    let _ = write!(
        s,
        "<text x=\"4\" y=\"{:.1}\">{}</text><text x=\"4\" y=\"{:.1}\">0</text>",
        y(ymax) + 4.0,
        num(ymax),
        y(0.0) + 4.0
    );
    for d in 0..=((t1 - t0) / DAY).floor() as usize {
        let t = t0 + d as f64 * DAY;
        if d % 7 == 0 || (t1 - t0) < 15.0 * DAY {
            let _ = write!(
                s,
                "<text x=\"{:.1}\" y=\"{:.1}\">dia {}</text>",
                x(t) - 10.0,
                h - m + 16.0,
                (t / DAY).floor() as i64
            );
        }
    }
    s.push_str("</svg>");
    s
}

/// Histogram of the lead time of every finished case, with the SLA line.
fn grafico_lead(k: &Indicadores) -> String {
    let leads: Vec<f64> = k
        .reps
        .iter()
        .flat_map(|r| r.leads.iter().copied())
        .collect();
    if leads.is_empty() {
        return String::new();
    }
    let mut slas: Vec<f64> = k.reps.iter().flat_map(|r| r.slas.iter().copied()).collect();
    slas.sort_by(f64::total_cmp);
    let sla = slas.get(slas.len() / 2).copied();
    let max = leads
        .iter()
        .copied()
        .fold(sla.unwrap_or(0.0), f64::max)
        .max(1e-9);
    let bins = 24;
    let mut counts = vec![0usize; bins];
    for l in &leads {
        counts[((l / max * bins as f64) as usize).min(bins - 1)] += 1;
    }
    let (w, h, m) = (760.0, 240.0, 40.0);
    let cmax = *counts.iter().max().unwrap_or(&1) as f64;
    let bw = (w - 2.0 * m) / bins as f64;
    let mut s = format!(
        "<svg viewBox=\"0 0 {w} {h}\" role=\"img\" aria-label=\"Histograma do lead time\">\
         <line class=\"eixo\" x1=\"{m}\" y1=\"{b}\" x2=\"{r}\" y2=\"{b}\"/>",
        b = h - m,
        r = w - m
    );
    for (i, c) in counts.iter().enumerate() {
        let bh = (h - 2.0 * m) * *c as f64 / cmax;
        let _ = write!(
            s,
            "<rect class=\"barra\" x=\"{:.1}\" y=\"{:.1}\" width=\"{:.1}\" height=\"{bh:.1}\"/>",
            m + i as f64 * bw + 1.0,
            h - m - bh,
            bw - 2.0
        );
    }
    for k in 0..=4 {
        let v = max * k as f64 / 4.0;
        let _ = write!(
            s,
            "<text x=\"{:.1}\" y=\"{:.1}\">{} h</text>",
            m + (w - 2.0 * m) * k as f64 / 4.0 - 8.0,
            h - m + 16.0,
            num1(v / 60.0)
        );
    }
    if let Some(sla) = sla {
        let x = m + (w - 2.0 * m) * sla / max;
        let _ = write!(
            s,
            "<line class=\"sla\" x1=\"{x:.1}\" y1=\"{m}\" x2=\"{x:.1}\" y2=\"{:.1}\"/>\
             <text x=\"{:.1}\" y=\"{:.1}\">SLA {} h</text>",
            h - m,
            x + 4.0,
            m + 10.0,
            num1(sla / 60.0)
        );
    }
    s.push_str("</svg>");
    s
}

/// Pareto chart of the mean wait per case by cause.
fn grafico_pareto(k: &Indicadores) -> (String, Vec<(usize, Summary)>) {
    let mut causes: Vec<(usize, Summary)> = (0..CAUSES.len())
        .map(|c| (c, k.resumo(|r| r.espera[c])))
        .filter(|(_, s)| s.mean > 1e-9)
        .collect();
    causes.sort_by(|a, b| b.1.mean.total_cmp(&a.1.mean));
    let total: f64 = causes.iter().map(|c| c.1.mean).sum();
    if causes.is_empty() || total <= 0.0 {
        return (String::new(), causes);
    }
    let (w, bar_h, m) = (760.0, 26.0, 8.0);
    let label_w = 330.0;
    let h = m * 2.0 + bar_h * causes.len() as f64 + 20.0;
    let max = causes[0].1.mean;
    let mut s = format!(
        "<svg viewBox=\"0 0 {w} {h:.0}\" role=\"img\" aria-label=\"Pareto das esperas por causa\">"
    );
    let mut acc = 0.0;
    for (i, (c, sm)) in causes.iter().enumerate() {
        acc += sm.mean;
        let y = m + i as f64 * bar_h;
        let bw = (w - label_w - 120.0) * sm.mean / max;
        let _ = write!(
            s,
            "<text x=\"0\" y=\"{:.1}\">{}</text>\
             <rect class=\"barra\" x=\"{label_w}\" y=\"{:.1}\" width=\"{bw:.1}\" height=\"{:.1}\"/>\
             <text x=\"{:.1}\" y=\"{:.1}\">{} h · acum. {}</text>",
            y + 17.0,
            esc(&curto(CAUSES[*c], 52)),
            y + 4.0,
            bar_h - 8.0,
            label_w + bw + 6.0,
            y + 17.0,
            num1(sm.mean / 60.0),
            pct(acc / total)
        );
    }
    s.push_str("</svg>");
    (s, causes)
}

/// `t` cut to `max` characters with an ellipsis.
pub fn curto(t: &str, max: usize) -> String {
    if t.chars().count() <= max {
        t.to_string()
    } else {
        let mut s: String = t.chars().take(max - 1).collect();
        s.push('…');
        s
    }
}

/// Table of the occupancy of each person by hour of the day (only the hours on shift).
fn tabela_horas(k: &Indicadores) -> String {
    let hours: Vec<usize> = (0..24)
        .filter(|h| k.ocupacao_hora.values().any(|row| row[*h].is_some()))
        .collect();
    if hours.is_empty() {
        return String::new();
    }
    let mut s = String::from("<div class=\"rolagem\"><table><thead><tr><th>Pessoa</th>");
    for h in &hours {
        let _ = write!(s, "<th class=\"n\">{h}h</th>");
    }
    s.push_str("</tr></thead><tbody>");
    for (name, row) in &k.ocupacao_hora {
        let _ = write!(s, "<tr><td>{}</td>", esc(name));
        for h in &hours {
            match row[*h] {
                Some(v) => {
                    let _ = write!(
                        s,
                        "<td class=\"calor\" style=\"background:rgba(194,65,12,{:.2})\">{}</td>",
                        v.clamp(0.0, 1.0) * 0.85,
                        pct(v)
                    );
                }
                None => s.push_str("<td class=\"calor muted\">-</td>"),
            }
        }
        s.push_str("</tr>");
    }
    s.push_str("</tbody></table></div>");
    s
}

/// The whole report as one page. `diagramas` is extra HTML (office VSM and swimlanes)
/// put after the indicators.
pub fn html(k: &Indicadores, voc: &Vocabulary, diagramas: &str) -> String {
    let reps = k.reps.len();
    let mut h = String::new();
    let _ = write!(
        h,
        "<!doctype html><html lang=\"pt-BR\"><head><meta charset=\"utf-8\">\
         <meta name=\"viewport\" content=\"width=device-width,initial-scale=1\">\
         <title>{linha}: {t}</title><style>{CSS}</style></head><body>\
         <h1>{linha}: {t}</h1><p class=\"muted\">{reps} replicação(ões), janela de estatísticas \
         de t = {de} a t = {ate} min ({dias} dias corridos). Cada valor é a média entre \
         replicações ± a meia-amplitude do intervalo de confiança de 95%. Tempos em minutos \
         (e horas), em tempo corrido: noites, fins de semana e esperas contam.</p>",
        linha = esc(&k.linha),
        t = esc(&k.modelo),
        de = num(k.stats_from),
        ate = num(k.end_time),
        dias = num1((k.end_time - k.stats_from) / DAY),
    );
    // Cards with the headline figures.
    let card = |nome: &str, valor: String| {
        format!(
            "<div class=\"card\"><span class=\"muted\">{}</span><b>{}</b></div>",
            esc(nome),
            esc(&valor)
        )
    };
    h.push_str("<div class=\"cards\">");
    let lead = k.resumo(|r| r.lead_time);
    let touch = k.resumo(|r| r.touch);
    h.push_str(&card(
        voc.term("time_in_system"),
        format!("{} h", num1(lead.mean / 60.0)),
    ));
    h.push_str(&card(
        voc.term("process_time"),
        format!("{} min", num(touch.mean)),
    ));
    h.push_str(&card(
        "Razão de atividade",
        pct(k.resumo(|r| r.razao_atividade).mean),
    ));
    h.push_str(&card("FPY", pct(k.resumo(|r| r.fpy).mean)));
    if let Some(s) = k.resumo_opt(|r| r.sla) {
        h.push_str(&card("Dentro do SLA", pct(s.mean)));
    }
    h.push_str(&card(
        &format!("WIP de {}", voc.term("itens")),
        num(k.resumo(|r| r.wip).mean),
    ));
    h.push_str("</div>");

    h.push_str("<h2>Indicadores</h2><table><thead><tr><th>Indicador</th><th class=\"n\">Valor</th><th>Definição</th></tr></thead><tbody>");
    for (nome, valor, def) in k.tabela() {
        let _ = write!(
            h,
            "<tr><td>{}</td><td class=\"n\">{}</td><td class=\"muted\">{}</td></tr>",
            esc(&nome),
            esc(&valor),
            esc(def)
        );
    }
    h.push_str("</tbody></table>");

    let _ = write!(
        h,
        "<h2>WIP de {} no tempo</h2><p class=\"muted\">Casos em aberto no modelo, replicação 0.</p>{}",
        esc(voc.term("itens")),
        grafico_wip(k)
    );
    let _ = write!(
        h,
        "<h2>Distribuição do {}</h2><p class=\"muted\">Todos os casos concluídos, de todas as replicações; a linha tracejada é o SLA.</p>{}",
        esc(voc.term("time_in_system")),
        grafico_lead(k)
    );

    let (pareto, causes) = grafico_pareto(k);
    h.push_str("<h2>Esperas por causa (Pareto)</h2><p class=\"muted\">Tempo médio por caso em cada tipo de espera. Lead time = tempo de processamento + soma das esperas.</p>");
    h.push_str(&pareto);
    h.push_str("<table><thead><tr><th>Causa</th><th class=\"n\">Por caso (min)</th><th class=\"n\">Por caso (h)</th></tr></thead><tbody>");
    for (c, s) in &causes {
        let _ = write!(
            h,
            "<tr><td>{}</td><td class=\"n\">{}</td><td class=\"n\">{}</td></tr>",
            esc(CAUSES[*c]),
            mhw(s, 1.0),
            mhw(s, 1.0 / 60.0)
        );
    }
    h.push_str("</tbody></table>");

    // Steps.
    h.push_str("<h2>Etapas</h2><table><thead><tr><th>Etapa</th><th class=\"n\">P/T por visita (min)</th><th class=\"n\">Touch por visita (min)</th><th class=\"n\">%C&amp;A</th></tr></thead><tbody>");
    let mut steps: Vec<String> = Vec::new();
    for r in &k.reps {
        for name in r.pt_etapa.keys() {
            if !steps.contains(name) {
                steps.push(name.clone());
            }
        }
    }
    for name in &steps {
        let pt = k.resumo_opt(|r| r.pt_etapa.get(name).map(|p| p.0));
        let tt = k.resumo_opt(|r| r.pt_etapa.get(name).map(|p| p.1));
        let ca = k.resumo_opt(|r| r.ca.get(name).copied());
        let _ = write!(
            h,
            "<tr><td>{}</td><td class=\"n\">{}</td><td class=\"n\">{}</td><td class=\"n\">{}</td></tr>",
            esc(name),
            pt.map_or("-".into(), |s| mhw(&s, 1.0)),
            tt.map_or("-".into(), |s| mhw(&s, 1.0)),
            ca.map_or("-".into(), |s| pct_hw(&s))
        );
    }
    h.push_str("</tbody></table>");
    let _ = write!(
        h,
        "<h2>{} por objeto</h2><table><thead><tr><th>Objeto</th><th class=\"n\">Média no tempo</th></tr></thead><tbody>",
        esc(&capital(voc.term("content")))
    );
    for (name, s) in &k.parados {
        let _ = write!(
            h,
            "<tr><td>{}</td><td class=\"n\">{}</td></tr>",
            esc(name),
            mhw(s, 1.0)
        );
    }
    h.push_str("</tbody></table>");

    // People.
    let _ = write!(
        h,
        "<h2>Ocupação das {}</h2><table><thead><tr><th>Pessoa</th><th class=\"n\">{}</th><th class=\"n\">Trocando de caso</th><th class=\"n\">{}</th><th class=\"n\">{}</th><th class=\"n\">{}</th><th class=\"n\">Ocupação no expediente</th></tr></thead><tbody>",
        esc(voc.term("operadores")),
        esc(&capital(voc.term("utilize"))),
        esc(&capital(voc.term("travel_empty"))),
        esc(&capital(voc.term("idle"))),
        esc(&capital(voc.term("off_shift")))
    );
    let mut people: Vec<String> = Vec::new();
    for r in &k.reps {
        for n in r.ocupacao.keys() {
            if !people.contains(n) {
                people.push(n.clone());
            }
        }
    }
    for name in &people {
        let get = |f: fn(&super::Ocupacao) -> f64| {
            k.resumo_opt(|r| r.ocupacao.get(name).map(f))
                .map_or("-".into(), |s| pct_hw(&s))
        };
        let on = k
            .resumo_opt(|r| r.ocupacao.get(name).and_then(|o| o.no_expediente))
            .map_or("-".into(), |s| pct_hw(&s));
        let _ = write!(
            h,
            "<tr><td>{}</td><td class=\"n\">{}</td><td class=\"n\">{}</td><td class=\"n\">{}</td><td class=\"n\">{}</td><td class=\"n\">{}</td><td class=\"n\">{on}</td></tr>",
            esc(name),
            get(|o| o.trabalhando),
            get(|o| o.trocando),
            get(|o| o.andando),
            get(|o| o.ocioso),
            get(|o| o.fora),
        );
    }
    h.push_str("</tbody></table>");
    let horas = tabela_horas(k);
    if !horas.is_empty() {
        h.push_str("<h3>Ocupação por hora do dia</h3><p class=\"muted\">Tempo trabalhando no caso ÷ tempo no expediente em cada hora, replicação 0.</p>");
        h.push_str(&horas);
    }

    // Rework loops.
    h.push_str("<h2>Voltas de retrabalho</h2><table><thead><tr><th>Voltas por caso</th><th class=\"n\">Casos (média por replicação)</th><th class=\"n\">Fração</th></tr></thead><tbody>");
    let total: f64 = k
        .reps
        .iter()
        .map(|r| r.concluidos as f64)
        .sum::<f64>()
        .max(1.0);
    for (i, label) in ["0", "1", "2", "3 ou mais"].iter().enumerate() {
        let c: f64 = k.reps.iter().map(|r| r.voltas_dist[i] as f64).sum();
        let _ = write!(
            h,
            "<tr><td>{label}</td><td class=\"n\">{}</td><td class=\"n\">{}</td></tr>",
            num(c / reps.max(1) as f64),
            pct(c / total)
        );
    }
    h.push_str("</tbody></table>");
    h.push_str(diagramas);
    h.push_str("<p class=\"muted\">Gerado pelo UTSim, linha Lean Office. Os indicadores contam os casos concluídos depois do aquecimento; o WIP é a média no tempo da mesma janela.</p></body></html>");
    h
}

fn capital(s: &str) -> String {
    let mut c = s.chars();
    match c.next() {
        Some(f) => f.to_uppercase().chain(c).collect(),
        None => String::new(),
    }
}

/// Text summary for the terminal.
pub fn texto(k: &Indicadores) -> String {
    let mut s = format!(
        "{}: {} ({} replicação(ões), média ± IC 95%)\n\n",
        k.linha,
        k.modelo,
        k.reps.len()
    );
    let w = k
        .tabela()
        .iter()
        .map(|r| r.0.chars().count())
        .max()
        .unwrap_or(0);
    for (nome, valor, _) in k.tabela() {
        let pad = w - nome.chars().count();
        let _ = writeln!(s, "  {nome}{}  {valor}", " ".repeat(pad));
    }
    let etapas = k.etapas_ca();
    if !etapas.is_empty() {
        s.push_str("\n%C&A por etapa:\n");
        for e in etapas {
            if let Some(v) = k.resumo_opt(|r| r.ca.get(&e).copied()) {
                let _ = writeln!(s, "  {e}: {}", pct_hw(&v));
            }
        }
    }
    let (_, causes) = grafico_pareto(k);
    if !causes.is_empty() {
        s.push_str("\nEsperas por causa (por caso):\n");
        for (c, v) in causes {
            let _ = writeln!(s, "  {}: {} h", CAUSES[c], mhw(&v, 1.0 / 60.0));
        }
    }
    s
}
