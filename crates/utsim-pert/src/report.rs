//! Reports of a PERT/CPM analysis: text for the terminal, CSV files, a self-contained HTML
//! page (inline SVG, no external resources, in the style of `utsim-model::html`), and the
//! coordinates of the network diagram for an interface to draw.

use std::fmt::Write as _;
use std::io;
use std::path::Path;

use serde::Serialize;
use utsim_core::stats::{Histogram, Summary};
use utsim_model::csv::{num, row};
use utsim_model::html::esc;

use crate::Mode;
use crate::cpm::Schedule;
use crate::monte_carlo::McResult;
use crate::network::ActivityNetwork;
use crate::observed::ObservedResult;
use crate::pert::{Moments, PertResult};

/// Everything a report shows. `classic` is always there: it holds the CPM dates on the
/// mean durations that every mode lists as reference.
pub struct Analysis<'a> {
    pub title: &'a str,
    /// "de A até B" or "objetos a, b, c".
    pub selection: String,
    pub mode: Mode,
    pub network: &'a ActivityNetwork,
    pub classic: &'a PertResult,
    pub monte_carlo: Option<&'a McResult>,
    pub observed: Option<&'a ObservedResult>,
}

impl Analysis<'_> {
    fn schedule(&self) -> &Schedule {
        &self.classic.schedule
    }

    /// Criticality shown on the diagram: the mode's own, or 1/0 from the CPM.
    fn criticality(&self) -> Vec<f64> {
        let net = self.network;
        match (self.mode, self.monte_carlo, self.observed) {
            (Mode::MonteCarlo, Some(mc), _) => mc.criticality.clone(),
            (Mode::Simulated, _, Some(obs)) => net
                .activities
                .iter()
                .map(|a| {
                    if a.is_milestone() {
                        1.0
                    } else {
                        obs.activities
                            .iter()
                            .find(|o| o.name == a.name)
                            .map_or(0.0, |o| o.pooled_criticality)
                    }
                })
                .collect(),
            _ => self
                .schedule()
                .critical
                .iter()
                .map(|&c| if c { 1.0 } else { 0.0 })
                .collect(),
        }
    }
}

fn f(x: f64) -> String {
    if x.is_nan() {
        "-".into()
    } else {
        format!("{x:.3}")
    }
}

fn mean_hw(s: &Summary) -> String {
    if s.half_width.is_nan() {
        f(s.mean)
    } else {
        format!("{} ± {}", f(s.mean), f(s.half_width))
    }
}

fn pct(x: f64) -> String {
    if x.is_nan() {
        "-".into()
    } else {
        format!("{:.1}%", 100.0 * x)
    }
}

fn pct_hw(s: &Summary) -> String {
    if s.half_width.is_nan() {
        pct(s.mean)
    } else {
        format!("{} ± {:.1}", pct(s.mean), 100.0 * s.half_width)
    }
}

/// Names along a path, without the milestones.
pub fn path_names(net: &ActivityNetwork, path: &[usize]) -> Vec<String> {
    path.iter()
        .filter(|&&v| !net.activities[v].is_milestone())
        .map(|&v| net.activities[v].name.clone())
        .collect()
}

// ---------------------------------------------------------------------------------------
// Text
// ---------------------------------------------------------------------------------------

/// Report for the terminal.
pub fn to_text(a: &Analysis) -> String {
    let net = a.network;
    let mut t = String::new();
    let _ = writeln!(t, "PERT/CPM de {}: {}", a.title, a.selection);
    let _ = writeln!(t, "modo: {}", a.mode.label());
    if let Some(s) = &net.scenario {
        let _ = writeln!(
            t,
            "cenário: {} (probabilidade {})",
            s.label,
            pct(s.probability)
        );
    }
    let _ = writeln!(
        t,
        "rede: {} atividades, {} precedências",
        net.activities.iter().filter(|x| !x.is_milestone()).count(),
        net.precedences.len()
    );
    t.push_str("\nhipóteses:\n");
    for h in &net.assumptions {
        let _ = writeln!(t, "  - {h}");
    }
    let warnings: Vec<&String> = net
        .warnings
        .iter()
        .chain(&a.classic.warnings)
        .chain(a.observed.iter().flat_map(|o| &o.warnings))
        .collect();
    if !warnings.is_empty() {
        t.push_str("avisos:\n");
        for w in warnings {
            let _ = writeln!(t, "  - {w}");
        }
    }
    match a.mode {
        Mode::Simulated => {
            if let Some(o) = a.observed {
                t.push_str(&observed_text(net, o));
            }
        }
        Mode::MonteCarlo => {
            if let Some(mc) = a.monte_carlo {
                t.push_str(&monte_carlo_text(net, mc));
            }
        }
        Mode::Classic => t.push_str(&classic_text(a.classic)),
    }
    t.push_str(&schedule_text(a));
    t
}

fn classic_text(p: &PertResult) -> String {
    let mut t = String::new();
    let _ = writeln!(
        t,
        "\nPERT clássico (momentos {}):",
        match p.moments {
            Moments::Exact => "exatos das distribuições",
            Moments::Classic => "beta: te = (a + 4m + b) / 6",
        }
    );
    let _ = writeln!(
        t,
        "  tempo total esperado {}  desvio padrão {}",
        f(p.mean),
        f(p.sd())
    );
    if let (Some(d), Some(prob)) = (p.deadline, p.probability) {
        let _ = writeln!(t, "  P(T ≤ {}) = {}", f(d), pct(prob));
    }
    t
}

fn monte_carlo_text(net: &ActivityNetwork, mc: &McResult) -> String {
    let mut t = String::new();
    let _ = writeln!(
        t,
        "\nMonte Carlo: {} sorteios (semente {})",
        mc.samples, mc.seed
    );
    let _ = writeln!(
        t,
        "  tempo total {}  desvio {}  mín {}  máx {}",
        mean_hw(&mc.total),
        f(mc.total.std_dev),
        f(mc.min),
        f(mc.max)
    );
    let q: Vec<String> = mc
        .percentiles
        .iter()
        .map(|(p, v)| format!("P{:.0} {}", 100.0 * p, f(*v)))
        .collect();
    let _ = writeln!(t, "  percentis: {}", q.join("  "));
    if let Some(d) = &mc.deadline {
        let _ = writeln!(
            t,
            "  P(T ≤ {}) = {} (IC 95%: {} a {})",
            f(d.deadline),
            pct(d.probability),
            pct(d.low),
            pct(d.high)
        );
    }
    t.push_str("  caminhos mais frequentes:\n");
    for p in mc.paths.iter().take(5) {
        let _ = writeln!(
            t,
            "    {:>6}  {}",
            pct(p.index),
            path_names(net, &p.path).join(" → ")
        );
    }
    if !mc.sensitivity.is_empty() {
        t.push_str("  sensibilidade (correlação de postos com T, crucialidade):\n");
        for s in mc.sensitivity.iter().take(10) {
            let _ = writeln!(
                t,
                "    {:<28} {:>7.3} {:>7.3}",
                net.activities[s.activity].name, s.rank_correlation, s.cruciality
            );
        }
    }
    t
}

fn observed_text(net: &ActivityNetwork, o: &ObservedResult) -> String {
    let _ = net;
    let mut t = String::new();
    let _ = writeln!(
        t,
        "\ncaminho crítico observado: {} unidades em {} replicações",
        o.units,
        o.replications.len()
    );
    let _ = writeln!(
        t,
        "  tempo total médio {} (IC 95% entre replicações)  desvio entre unidades {}",
        mean_hw(&o.total),
        f(o.pooled.std_dev)
    );
    let q: Vec<String> = o
        .percentiles
        .iter()
        .map(|(p, v)| format!("P{:.0} {}", 100.0 * p, f(*v)))
        .collect();
    let _ = writeln!(t, "  percentis: {}", q.join("  "));
    if let Some(d) = &o.deadline {
        let _ = writeln!(
            t,
            "  fração dentro do prazo {}: {}",
            f(d.deadline),
            pct_hw(&d.fraction)
        );
    }
    let _ = writeln!(
        t,
        "  {:<34} {:>16} {:>10} {:>10} {:>10}",
        "atividade", "criticidade", "espera", "trabalho", "bloqueio"
    );
    for a in &o.activities {
        let _ = writeln!(
            t,
            "  {:<34} {:>16} {:>10} {:>10} {:>10}",
            a.name,
            pct_hw(&a.criticality),
            f(a.wait),
            f(a.work),
            f(a.blocked)
        );
    }
    t.push_str("  (espera, trabalho e bloqueio: tempo médio por unidade no caminho crítico)\n");
    t.push_str("  caminhos observados mais frequentes:\n");
    for (p, x) in o.paths.iter().take(5) {
        let _ = writeln!(t, "    {:>6}  {}", pct(*x), p.join(" → "));
    }
    t
}

fn schedule_text(a: &Analysis) -> String {
    let net = a.network;
    let s = a.schedule();
    let p = a.classic;
    let mut t = String::new();
    let _ = writeln!(
        t,
        "\nCPM com as durações médias (tempo total {}):",
        f(s.project)
    );
    let _ = writeln!(
        t,
        "  {:<34} {:>9} {:>9} {:>9} {:>9} {:>9} {:>9} {:>9} {:>9} {:>9}  crítica",
        "atividade", "a", "m", "b", "média", "ES", "EF", "LS", "LF", "folga"
    );
    for v in net.order() {
        let act = &net.activities[v];
        if act.is_milestone() {
            continue;
        }
        let e = &p.estimates[v];
        let _ = writeln!(
            t,
            "  {:<34} {:>9} {:>9} {:>9} {:>9} {:>9} {:>9} {:>9} {:>9} {:>9}  {}",
            act.name,
            f(e.a),
            f(e.m),
            f(e.b),
            f(p.means[v]),
            f(s.es[v]),
            f(s.ef[v]),
            f(s.ls[v]),
            f(s.lf[v]),
            f(s.total_float[v]),
            if s.critical[v] { "sim" } else { "" }
        );
    }
    for path in &s.critical_paths {
        let _ = writeln!(
            t,
            "  caminho crítico: {}",
            path_names(net, path).join(" → ")
        );
    }
    if s.paths_truncated {
        t.push_str("  (há mais caminhos críticos que os listados)\n");
    }
    t
}

// ---------------------------------------------------------------------------------------
// CSV
// ---------------------------------------------------------------------------------------

/// Writes the CSV files into `dir` (created if needed) and returns their names.
pub fn write_csv(a: &Analysis, dir: &Path) -> io::Result<Vec<String>> {
    std::fs::create_dir_all(dir)?;
    let mut files = vec![
        ("atividades.csv", activities_csv(a)),
        ("caminhos.csv", paths_csv(a)),
        ("tempo_total.csv", total_csv(a)),
        (
            "diagrama.csv",
            diagram_csv(&diagram(a.network, a.schedule(), &a.criticality())),
        ),
    ];
    if let Some(mc) = a.monte_carlo {
        files.push(("sensibilidade.csv", sensitivity_csv(a.network, mc)));
    }
    for (name, text) in &files {
        std::fs::write(dir.join(name), text)?;
    }
    Ok(files.into_iter().map(|(n, _)| n.to_string()).collect())
}

pub fn activities_csv(a: &Analysis) -> String {
    let net = a.network;
    let s = a.schedule();
    let p = a.classic;
    let mut out = String::from(
        "atividade,tipo,objeto,a,m,b,media,variancia,es,ef,ls,lf,folga_total,folga_livre,\
         critica,criticidade_montecarlo,criticidade_observada,espera_observada,\
         trabalho_observado,bloqueio_observado,observacao\n",
    );
    for v in net.order() {
        let act = &net.activities[v];
        let e = &p.estimates[v];
        let obs = a
            .observed
            .and_then(|o| o.activities.iter().find(|x| x.name == act.name));
        row(
            &mut out,
            &[
                act.name.clone(),
                act.label.clone(),
                act.object.clone().unwrap_or_default(),
                num(e.a),
                num(e.m),
                num(e.b),
                num(p.means[v]),
                num(p.variances[v]),
                num(s.es[v]),
                num(s.ef[v]),
                num(s.ls[v]),
                num(s.lf[v]),
                num(s.total_float[v]),
                num(s.free_float[v]),
                u8::from(s.critical[v]).to_string(),
                a.monte_carlo
                    .map_or(String::new(), |mc| num(mc.criticality[v])),
                obs.map_or(String::new(), |o| num(o.pooled_criticality)),
                obs.map_or(String::new(), |o| num(o.wait)),
                obs.map_or(String::new(), |o| num(o.work)),
                obs.map_or(String::new(), |o| num(o.blocked)),
                act.note.clone().unwrap_or_default(),
            ],
        );
    }
    out
}

pub fn paths_csv(a: &Analysis) -> String {
    let net = a.network;
    let mut out = String::from("modo,ordem,indice,caminho\n");
    for (k, path) in a.schedule().critical_paths.iter().enumerate() {
        row(
            &mut out,
            &[
                "cpm".into(),
                (k + 1).to_string(),
                "1".into(),
                path_names(net, path).join(" > "),
            ],
        );
    }
    if let Some(mc) = a.monte_carlo {
        for (k, p) in mc.paths.iter().enumerate() {
            row(
                &mut out,
                &[
                    "montecarlo".into(),
                    (k + 1).to_string(),
                    num(p.index),
                    path_names(net, &p.path).join(" > "),
                ],
            );
        }
    }
    if let Some(o) = a.observed {
        for (k, (p, x)) in o.paths.iter().enumerate() {
            row(
                &mut out,
                &[
                    "simulado".into(),
                    (k + 1).to_string(),
                    num(*x),
                    p.join(" > "),
                ],
            );
        }
    }
    out
}

pub fn total_csv(a: &Analysis) -> String {
    let mut out = String::from("modo,estatistica,valor\n");
    let mut put = |mode: &str, k: &str, v: f64| row(&mut out, &[mode.into(), k.into(), num(v)]);
    let p = a.classic;
    put("classico", "media", p.mean);
    put("classico", "desvio_padrao", p.sd());
    if let Some(prob) = p.probability {
        put("classico", "p_prazo", prob);
    }
    if let Some(mc) = a.monte_carlo {
        put("montecarlo", "media", mc.total.mean);
        put("montecarlo", "meia_largura_95", mc.total.half_width);
        put("montecarlo", "desvio_padrao", mc.total.std_dev);
        for (q, v) in &mc.percentiles {
            put("montecarlo", &format!("p{:.0}", 100.0 * q), *v);
        }
        if let Some(d) = &mc.deadline {
            put("montecarlo", "p_prazo", d.probability);
            put("montecarlo", "p_prazo_min95", d.low);
            put("montecarlo", "p_prazo_max95", d.high);
        }
    }
    if let Some(o) = a.observed {
        put("simulado", "unidades", o.units as f64);
        put("simulado", "media", o.total.mean);
        put("simulado", "meia_largura_95", o.total.half_width);
        put("simulado", "desvio_padrao_unidades", o.pooled.std_dev);
        for (q, v) in &o.percentiles {
            put("simulado", &format!("p{:.0}", 100.0 * q), *v);
        }
        if let Some(d) = &o.deadline {
            put("simulado", "fracao_prazo", d.fraction.mean);
            put(
                "simulado",
                "fracao_prazo_meia_largura_95",
                d.fraction.half_width,
            );
        }
    }
    out
}

pub fn sensitivity_csv(net: &ActivityNetwork, mc: &McResult) -> String {
    let mut out = String::from("atividade,correlacao_postos,crucialidade,criticidade\n");
    for s in &mc.sensitivity {
        row(
            &mut out,
            &[
                net.activities[s.activity].name.clone(),
                num(s.rank_correlation),
                num(s.cruciality),
                num(mc.criticality[s.activity]),
            ],
        );
    }
    out
}

pub fn diagram_csv(d: &Diagram) -> String {
    let mut out = String::from("atividade,camada,raia,x,y,largura,altura,critica,criticidade\n");
    for b in &d.boxes {
        row(
            &mut out,
            &[
                b.name.clone(),
                b.layer.to_string(),
                b.lane.to_string(),
                num(b.x),
                num(b.y),
                num(b.width),
                num(b.height),
                u8::from(b.critical).to_string(),
                num(b.criticality),
            ],
        );
    }
    out
}

// ---------------------------------------------------------------------------------------
// Diagram
// ---------------------------------------------------------------------------------------

/// Box size and spacing of the diagram (any unit; the HTML uses pixels).
pub const BOX_WIDTH: f64 = 150.0;
pub const BOX_HEIGHT: f64 = 64.0;
pub const LAYER_GAP: f64 = 60.0;
pub const LANE_GAP: f64 = 26.0;
const MARGIN: f64 = 10.0;

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct DiagramBox {
    pub activity: usize,
    pub name: String,
    /// Column (topological layer: longest path from the start).
    pub layer: usize,
    /// Row inside the column.
    pub lane: usize,
    /// Top-left corner, y down.
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
    pub milestone: bool,
    /// On a critical path of the CPM on the means.
    pub critical: bool,
    /// Criticality index of the mode (0 to 1).
    pub criticality: f64,
    pub es: f64,
    pub ef: f64,
    pub ls: f64,
    pub lf: f64,
    pub total_float: f64,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct DiagramArrow {
    pub from: usize,
    pub to: usize,
    /// From the right side of `from` to the left side of `to`.
    pub points: Vec<[f64; 2]>,
    pub critical: bool,
    /// Criticality of the pair (the smaller of the two), for the arrow's thickness.
    pub weight: f64,
}

/// Activity-on-node diagram: boxes in topological columns, arrows between them.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Diagram {
    pub boxes: Vec<DiagramBox>,
    pub arrows: Vec<DiagramArrow>,
    pub width: f64,
    pub height: f64,
}

/// Lays out the network with `utsim-scene::layout::layers` (the same layering as the
/// automatic 3D layout). `criticality` has one value per activity.
pub fn diagram(net: &ActivityNetwork, s: &Schedule, criticality: &[f64]) -> Diagram {
    let n = net.len();
    let layer = utsim_scene::layout::layers(n, &net.precedences);
    let n_layers = layer.iter().copied().max().map_or(1, |m| m + 1);
    let mut lane = vec![0usize; n];
    let mut width = vec![0usize; n_layers];
    for v in net.order() {
        lane[v] = width[layer[v]];
        width[layer[v]] += 1;
    }
    let tallest = width.iter().copied().max().unwrap_or(1).max(1);
    let col = BOX_WIDTH + LAYER_GAP;
    let row_h = BOX_HEIGHT + LANE_GAP;
    let total_h = tallest as f64 * row_h - LANE_GAP + 2.0 * MARGIN;
    let boxes: Vec<DiagramBox> = (0..n)
        .map(|v| {
            let l = layer[v];
            let offset = (tallest - width[l]) as f64 * row_h / 2.0;
            DiagramBox {
                activity: v,
                name: net.activities[v].name.clone(),
                layer: l,
                lane: lane[v],
                x: MARGIN + l as f64 * col,
                y: MARGIN + offset + lane[v] as f64 * row_h,
                width: BOX_WIDTH,
                height: BOX_HEIGHT,
                milestone: net.activities[v].is_milestone(),
                critical: s.critical[v],
                criticality: criticality.get(v).copied().unwrap_or(0.0),
                es: s.es[v],
                ef: s.ef[v],
                ls: s.ls[v],
                lf: s.lf[v],
                total_float: s.total_float[v],
            }
        })
        .collect();
    let tol = crate::cpm::Cpm::tolerance(s.project);
    let arrows = net
        .precedences
        .iter()
        .map(|&(a, b)| {
            let (p, q) = (&boxes[a], &boxes[b]);
            DiagramArrow {
                from: a,
                to: b,
                points: vec![
                    [p.x + p.width, p.y + p.height / 2.0],
                    [q.x, q.y + q.height / 2.0],
                ],
                critical: s.critical[a] && s.critical[b] && (s.ef[a] - s.es[b]).abs() <= tol,
                weight: p.criticality.min(q.criticality),
            }
        })
        .collect();
    Diagram {
        boxes,
        arrows,
        width: n_layers as f64 * col - LAYER_GAP + 2.0 * MARGIN,
        height: total_h,
    }
}

// ---------------------------------------------------------------------------------------
// HTML
// ---------------------------------------------------------------------------------------

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
.scroll{overflow-x:auto}
svg{max-width:100%;height:auto}
svg text{fill:var(--fg);font-size:11px}
svg .axis{stroke:var(--muted);stroke-width:1}
svg .box{fill:var(--card);stroke:var(--muted);stroke-width:1}
svg .boxhot{fill:var(--card);stroke:var(--hot);stroke-width:2.5}
svg .arrow{stroke:var(--muted);fill:none}
svg .arrowhot{stroke:var(--hot);fill:none}
svg .bar{fill:var(--accent);opacity:.85}svg .barhot{fill:var(--hot)}
svg .float{fill:var(--accent);opacity:.25}
svg .deadline{stroke:var(--hot);stroke-width:1.5;stroke-dasharray:4 3}
"#;

/// The whole report as one HTML page.
pub fn to_html(a: &Analysis) -> String {
    let net = a.network;
    let mut h = String::new();
    let _ = write!(
        h,
        "<!doctype html><html lang=\"pt-BR\"><head><meta charset=\"utf-8\">\
         <meta name=\"viewport\" content=\"width=device-width,initial-scale=1\">\
         <title>PERT/CPM: {t}</title><style>{CSS}</style></head><body>\
         <h1>PERT/CPM: {t}</h1><p class=\"muted\">Trecho: {s}. Modo: {m}.</p>",
        t = esc(a.title),
        s = esc(&a.selection),
        m = esc(a.mode.label())
    );
    if let Some(sc) = &net.scenario {
        let _ = write!(
            h,
            "<p>Cenário: {} (probabilidade {}).</p>",
            esc(&sc.label),
            pct(sc.probability)
        );
    }
    h.push_str("<h2>Hipóteses</h2><ul>");
    for x in &net.assumptions {
        let _ = write!(h, "<li>{}</li>", esc(x));
    }
    h.push_str("</ul>");
    let warnings: Vec<&String> = net
        .warnings
        .iter()
        .chain(&a.classic.warnings)
        .chain(a.observed.iter().flat_map(|o| &o.warnings))
        .collect();
    if !warnings.is_empty() {
        h.push_str("<h2>Avisos</h2><ul>");
        for w in warnings {
            let _ = write!(h, "<li class=\"hot\">{}</li>", esc(w));
        }
        h.push_str("</ul>");
    }
    let crit = a.criticality();
    h.push_str(
        "<h2>Rede</h2><p class=\"muted\">Atividade no nó; caixas e setas do caminho \
                crítico em destaque; a espessura da seta segue o índice de criticidade.</p>",
    );
    h.push_str("<div class=\"scroll\">");
    h.push_str(&diagram_svg(&diagram(net, a.schedule(), &crit)));
    h.push_str("</div>");

    match a.mode {
        Mode::Simulated => {
            if let Some(o) = a.observed {
                h.push_str(&observed_html(o));
            }
        }
        Mode::MonteCarlo => {
            if let Some(mc) = a.monte_carlo {
                h.push_str(&monte_carlo_html(net, mc));
            }
        }
        Mode::Classic => {
            let p = a.classic;
            let _ = write!(
                h,
                "<h2>PERT clássico</h2><p>Tempo total esperado <b>{}</b>, desvio padrão {}.",
                f(p.mean),
                f(p.sd())
            );
            if let (Some(d), Some(prob)) = (p.deadline, p.probability) {
                let _ = write!(h, " P(T ≤ {}) = <b>{}</b>.", f(d), pct(prob));
            }
            h.push_str("</p>");
        }
    }

    let s = a.schedule();
    let _ = write!(
        h,
        "<h2>Gantt (CPM com as médias)</h2><p class=\"muted\">Barras de ES a EF; a parte clara \
         é a folga total. Tempo total {}.</p>",
        f(s.project)
    );
    h.push_str(&gantt_svg(net, s));
    h.push_str(&activities_table(a));
    h.push_str("</body></html>");
    h
}

fn activities_table(a: &Analysis) -> String {
    let net = a.network;
    let s = a.schedule();
    let p = a.classic;
    let mut h = String::from(
        "<h2>Tabela</h2><table><tr><th>Atividade</th><th>Tipo</th><th>a</th><th>m</th>\
         <th>b</th><th>Média</th><th>ES</th><th>EF</th><th>LS</th><th>LF</th>\
         <th>Folga total</th><th>Folga livre</th><th>Observação</th></tr>",
    );
    for v in net.order() {
        let act = &net.activities[v];
        if act.is_milestone() {
            continue;
        }
        let e = &p.estimates[v];
        let _ = write!(
            h,
            "<tr{}><td>{}</td><td>{}</td>{}<td class=\"muted\">{}</td></tr>",
            if s.critical[v] { " class=\"hot\"" } else { "" },
            esc(&act.name),
            esc(&act.label),
            [
                e.a,
                e.m,
                e.b,
                p.means[v],
                s.es[v],
                s.ef[v],
                s.ls[v],
                s.lf[v],
                s.total_float[v],
                s.free_float[v]
            ]
            .iter()
            .map(|x| format!("<td class=\"n\">{}</td>", f(*x)))
            .collect::<String>(),
            esc(act.note.as_deref().unwrap_or(""))
        );
    }
    h.push_str("</table>");
    for path in &s.critical_paths {
        let _ = write!(
            h,
            "<p>Caminho crítico: <b class=\"hot\">{}</b></p>",
            esc(&path_names(net, path).join(" → "))
        );
    }
    h
}

fn monte_carlo_html(net: &ActivityNetwork, mc: &McResult) -> String {
    let mut h = format!(
        "<h2>Monte Carlo</h2><p>{} sorteios (semente {}). Tempo total <b>{}</b>, desvio {}, \
         de {} a {}.</p>",
        mc.samples,
        mc.seed,
        esc(&mean_hw(&mc.total)),
        f(mc.total.std_dev),
        f(mc.min),
        f(mc.max)
    );
    h.push_str(&percentiles_line(&mc.percentiles));
    if let Some(d) = &mc.deadline {
        let _ = write!(
            h,
            "<p>P(T ≤ {}) = <b>{}</b> (IC 95%: {} a {}).</p>",
            f(d.deadline),
            pct(d.probability),
            pct(d.low),
            pct(d.high)
        );
    }
    if let Some(hist) = &mc.histogram {
        h.push_str(&histogram_svg(hist, mc.deadline.map(|d| d.deadline)));
    }
    h.push_str("<h3>Criticidade dos caminhos</h3><table><tr><th>Índice</th><th>Caminho</th></tr>");
    for p in mc.paths.iter().take(10) {
        let _ = write!(
            h,
            "<tr><td class=\"n\">{}</td><td>{}</td></tr>",
            pct(p.index),
            esc(&path_names(net, &p.path).join(" → "))
        );
    }
    h.push_str("</table>");
    if !mc.sensitivity.is_empty() {
        h.push_str(
            "<h3>Sensibilidade</h3><p class=\"muted\">Tornado da crucialidade: criticidade × \
             correlação de postos entre a duração da atividade e o tempo total.</p>",
        );
        h.push_str(&tornado_svg(net, mc));
    }
    h
}

fn observed_html(o: &ObservedResult) -> String {
    let mut h = format!(
        "<h2>Caminho crítico observado</h2><p>{} unidades em {} replicações. Tempo total médio \
         <b>{}</b> (IC 95% entre replicações), desvio entre unidades {}.</p>\
         <p class=\"muted\">O tempo vai da entrada no primeiro objeto do trecho à saída do último \
         e inclui filas, transportes, operadores, quebras e turnos. Em cada montagem, a entrada \
         crítica é a que chegou por último.</p>",
        o.units,
        o.replications.len(),
        esc(&mean_hw(&o.total)),
        f(o.pooled.std_dev)
    );
    h.push_str(&percentiles_line(&o.percentiles));
    if let Some(d) = &o.deadline {
        let _ = write!(
            h,
            "<p>Fração das unidades dentro do prazo {}: <b>{}</b>.</p>",
            f(d.deadline),
            esc(&pct_hw(&d.fraction))
        );
    }
    if let Some(hist) = &o.histogram {
        h.push_str(&histogram_svg(
            hist,
            o.deadline.as_ref().map(|d| d.deadline),
        ));
    }
    h.push_str(
        "<table><tr><th>Atividade</th><th>Criticidade</th><th>Espera</th><th>Trabalho</th>\
         <th>Bloqueio</th></tr>",
    );
    for a in &o.activities {
        let _ = write!(
            h,
            "<tr{}><td>{}</td><td class=\"n\">{}</td><td class=\"n\">{}</td>\
             <td class=\"n\">{}</td><td class=\"n\">{}</td></tr>",
            if a.pooled_criticality >= 0.5 {
                " class=\"hot\""
            } else {
                ""
            },
            esc(&a.name),
            esc(&pct_hw(&a.criticality)),
            f(a.wait),
            f(a.work),
            f(a.blocked)
        );
    }
    h.push_str(
        "</table><p class=\"muted\">Espera, trabalho e bloqueio: tempo médio por unidade passado \
         na atividade enquanto ela estava no caminho crítico.</p>",
    );
    h.push_str("<h3>Caminhos observados</h3><table><tr><th>Unidades</th><th>Caminho</th></tr>");
    for (p, x) in o.paths.iter().take(10) {
        let _ = write!(
            h,
            "<tr><td class=\"n\">{}</td><td>{}</td></tr>",
            pct(*x),
            esc(&p.join(" → "))
        );
    }
    h.push_str("</table>");
    h
}

fn percentiles_line(q: &[(f64, f64)]) -> String {
    let parts: Vec<String> = q
        .iter()
        .map(|(p, v)| format!("P{:.0} = {}", 100.0 * p, f(*v)))
        .collect();
    format!(
        "<p>Percentis do tempo total: {}.</p>",
        esc(&parts.join(", "))
    )
}

fn diagram_svg(d: &Diagram) -> String {
    let mut s = format!(
        "<svg viewBox=\"0 0 {:.0} {:.0}\" width=\"{:.0}\" role=\"img\" aria-label=\"Rede de \
         atividades\"><defs><marker id=\"seta\" viewBox=\"0 0 10 10\" refX=\"10\" refY=\"5\" \
         markerWidth=\"6\" markerHeight=\"6\" orient=\"auto-start-reverse\">\
         <path d=\"M0,0L10,5L0,10z\" fill=\"currentColor\"/></marker></defs>",
        d.width, d.height, d.width
    );
    for a in &d.arrows {
        let (p, q) = (a.points[0], a.points[a.points.len() - 1]);
        let mid = (p[0] + q[0]) / 2.0;
        let _ = write!(
            s,
            "<path class=\"{}\" stroke-width=\"{:.1}\" marker-end=\"url(#seta)\" \
             d=\"M{:.1},{:.1}C{mid:.1},{:.1} {mid:.1},{:.1} {:.1},{:.1}\"/>",
            if a.critical { "arrowhot" } else { "arrow" },
            1.0 + 3.0 * a.weight.clamp(0.0, 1.0),
            p[0],
            p[1],
            p[1],
            q[1],
            q[0],
            q[1]
        );
    }
    for b in &d.boxes {
        let class = if b.critical { "boxhot" } else { "box" };
        let _ = write!(
            s,
            "<rect class=\"{class}\" x=\"{:.1}\" y=\"{:.1}\" width=\"{:.1}\" height=\"{:.1}\" \
             rx=\"{}\"><title>{}</title></rect>\
             <text x=\"{:.1}\" y=\"{:.1}\" text-anchor=\"middle\" font-weight=\"600\">{}</text>",
            b.x,
            b.y,
            b.width,
            b.height,
            if b.milestone { 30 } else { 4 },
            esc(&b.name),
            b.x + b.width / 2.0,
            b.y + 16.0,
            esc(&short(&b.name, 22))
        );
        if !b.milestone {
            let _ = write!(
                s,
                "<text x=\"{:.1}\" y=\"{:.1}\">ES {} EF {}</text>\
                 <text x=\"{:.1}\" y=\"{:.1}\">LS {} LF {}</text>\
                 <text x=\"{:.1}\" y=\"{:.1}\">folga {} · crit. {}</text>",
                b.x + 6.0,
                b.y + 31.0,
                short_num(b.es),
                short_num(b.ef),
                b.x + 6.0,
                b.y + 44.0,
                short_num(b.ls),
                short_num(b.lf),
                b.x + 6.0,
                b.y + 57.0,
                short_num(b.total_float),
                pct(b.criticality)
            );
        }
    }
    s.push_str("</svg>");
    s
}

fn short(s: &str, n: usize) -> String {
    if s.chars().count() <= n {
        s.to_string()
    } else {
        format!("{}…", s.chars().take(n - 1).collect::<String>())
    }
}

fn short_num(x: f64) -> String {
    if x.abs() >= 1000.0 {
        format!("{x:.0}")
    } else {
        format!("{x:.1}")
    }
}

fn gantt_svg(net: &ActivityNetwork, s: &Schedule) -> String {
    let rows: Vec<usize> = net
        .order()
        .into_iter()
        .filter(|&v| !net.activities[v].is_milestone())
        .collect();
    let (w, row_h, left, right) = (900.0, 20.0, 200.0, 20.0);
    let hgt = row_h * rows.len() as f64 + 24.0;
    let span = s.project.max(1e-12);
    let x = |t: f64| left + (w - left - right) * (t / span).clamp(0.0, 1.0);
    let mut out = format!("<svg viewBox=\"0 0 {w} {hgt}\" role=\"img\" aria-label=\"Gantt\">");
    for (k, &v) in rows.iter().enumerate() {
        let y = 4.0 + row_h * k as f64;
        let _ = write!(
            out,
            "<text x=\"{}\" y=\"{:.1}\" text-anchor=\"end\">{}</text>\
             <rect class=\"float\" x=\"{:.1}\" y=\"{y:.1}\" width=\"{:.1}\" height=\"{}\"/>\
             <rect class=\"{}\" x=\"{:.1}\" y=\"{y:.1}\" width=\"{:.1}\" height=\"{}\"/>",
            left - 8.0,
            y + 11.0,
            esc(&short(&net.activities[v].name, 30)),
            x(s.ef[v]),
            (x(s.lf[v]) - x(s.ef[v])).max(0.0),
            row_h - 6.0,
            if s.critical[v] { "barhot" } else { "bar" },
            x(s.es[v]),
            (x(s.ef[v]) - x(s.es[v])).max(0.5),
            row_h - 6.0
        );
    }
    let base = hgt - 6.0;
    let _ = write!(
        out,
        "<text x=\"{left}\" y=\"{base}\">0</text><text x=\"{}\" y=\"{base}\" \
         text-anchor=\"end\">{}</text></svg>",
        w - right,
        f(s.project)
    );
    out
}

fn histogram_svg(hist: &Histogram, deadline: Option<f64>) -> String {
    let (w, hgt, left, bottom) = (900.0, 220.0, 40.0, 24.0);
    let top = 10.0;
    let max = hist.counts.iter().copied().max().unwrap_or(1).max(1) as f64;
    let bins = hist.counts.len().max(1) as f64;
    let range = (hist.max - hist.min).max(1e-12);
    let bw = (w - left - 10.0) / bins;
    let mut s = format!(
        "<svg viewBox=\"0 0 {w} {hgt}\" role=\"img\" aria-label=\"Histograma do tempo total\">"
    );
    for (k, &c) in hist.counts.iter().enumerate() {
        let bh = (hgt - top - bottom) * c as f64 / max;
        let _ = write!(
            s,
            "<rect class=\"bar\" x=\"{:.1}\" y=\"{:.1}\" width=\"{:.1}\" height=\"{bh:.1}\">\
             <title>{}</title></rect>",
            left + bw * k as f64 + 1.0,
            hgt - bottom - bh,
            (bw - 2.0).max(0.5),
            c
        );
    }
    let _ = write!(
        s,
        "<line class=\"axis\" x1=\"{left}\" x2=\"{}\" y1=\"{y}\" y2=\"{y}\"/>\
         <text x=\"{left}\" y=\"{}\">{}</text><text x=\"{}\" y=\"{}\" text-anchor=\"end\">{}</text>",
        w - 10.0,
        hgt - 6.0,
        f(hist.min),
        w - 10.0,
        hgt - 6.0,
        f(hist.max),
        y = hgt - bottom
    );
    if let Some(d) = deadline {
        let xd = left + (w - left - 10.0) * ((d - hist.min) / range).clamp(0.0, 1.0);
        let _ = write!(
            s,
            "<line class=\"deadline\" x1=\"{xd:.1}\" x2=\"{xd:.1}\" y1=\"{top}\" y2=\"{:.1}\"/>\
             <text x=\"{:.1}\" y=\"{}\">prazo {}</text>",
            hgt - bottom,
            xd + 4.0,
            top + 10.0,
            f(d)
        );
    }
    s.push_str("</svg>");
    s
}

fn tornado_svg(net: &ActivityNetwork, mc: &McResult) -> String {
    let rows: Vec<_> = mc.sensitivity.iter().take(15).collect();
    let (w, row_h, left, right) = (900.0, 22.0, 220.0, 60.0);
    let hgt = row_h * rows.len() as f64 + 10.0;
    let mid = left + (w - left - right) / 2.0;
    let half = (w - left - right) / 2.0;
    let mut s = format!("<svg viewBox=\"0 0 {w} {hgt}\" role=\"img\" aria-label=\"Tornado\">");
    for (k, r) in rows.iter().enumerate() {
        let y = 4.0 + row_h * k as f64;
        let len = half * r.cruciality.clamp(-1.0, 1.0);
        let (x0, width) = if len >= 0.0 {
            (mid, len)
        } else {
            (mid + len, -len)
        };
        let _ = write!(
            s,
            "<text x=\"{}\" y=\"{:.1}\" text-anchor=\"end\">{}</text>\
             <rect class=\"barhot\" x=\"{x0:.1}\" y=\"{y:.1}\" width=\"{:.1}\" height=\"{}\"/>\
             <text x=\"{:.1}\" y=\"{:.1}\">{:.3}</text>",
            left - 8.0,
            y + 13.0,
            esc(&short(&net.activities[r.activity].name, 32)),
            width.max(0.5),
            row_h - 8.0,
            w - right + 6.0,
            y + 13.0,
            r.cruciality
        );
    }
    let _ = write!(
        s,
        "<line class=\"axis\" x1=\"{mid:.1}\" x2=\"{mid:.1}\" y1=\"0\" y2=\"{hgt}\"/></svg>"
    );
    s
}
