//! Plan-view diagrams: flow-process map (mapofluxograma) and spaghetti diagram.

use std::collections::BTreeMap;
use std::fmt::Write as _;

use utsim_model::Model;
use utsim_model::csv::{num as csvnum, row};

use crate::desenho::{PALETTE, Parte, Plano, curto, svg_doc, texto};
use crate::graph::FlowGraph;
use crate::rastro::{Rastro, dist3};
use crate::svg::glyph;
use crate::text::{esc, num};

/// Where each object sits on the plan, in model units; `None` for the ones with no place.
/// Models with every position at the origin get the position of the block layout instead
/// (`layout == true`): shapes, not distances.
pub fn pontos(model: &Model, graph: &FlowGraph) -> (Vec<Option<(f64, f64)>>, bool) {
    let all_zero = model
        .objects
        .iter()
        .all(|o| o.position.iter().all(|c| *c == 0.0));
    if !all_zero {
        return (
            model
                .objects
                .iter()
                .map(|o| Some((o.position[0], o.position[1])))
                .collect(),
            false,
        );
    }
    let pts = model
        .objects
        .iter()
        .map(|o| {
            graph
                .node_index(&o.name)
                .map(|i| (graph.nodes[i].x + graph.nodes[i].w / 2.0, graph.nodes[i].y))
        })
        .collect();
    (pts, true)
}

fn canvas_pts(pts: &[Option<(f64, f64)>]) -> Plano {
    let v: Vec<(f64, f64)> = pts.iter().flatten().copied().collect();
    Plano::new(&v)
}

fn simbolo_de(graph: &FlowGraph, name: &str) -> crate::graph::Symbol {
    graph
        .node_index(name)
        .map_or(crate::graph::Symbol::Operacao, |i| graph.nodes[i].symbol)
}

/// Draws the steps of the plan with their symbols.
fn desenhar_passos(
    b: &mut String,
    model: &Model,
    graph: &FlowGraph,
    pts: &[Option<(f64, f64)>],
    plano: &Plano,
) {
    for (i, o) in model.objects.iter().enumerate() {
        let (Some(p), Some(_)) = (pts[i], graph.node_index(&o.name)) else {
            continue;
        };
        let (x, y) = plano.at(p);
        b.push_str(&glyph(simbolo_de(graph, &o.name), x, y, 11.0));
        b.push_str(&texto(x - 40.0, y + 28.0, "etq mut", &curto(&o.name, 18)));
    }
}

/// Flow-process map: the symbols of the process chart at the place of each step, and the
/// route of the item between them, numbered in order.
pub fn mapofluxograma(
    model: &Model,
    graph: &FlowGraph,
    rastro: &Rastro,
    fundo: Option<&str>,
) -> Parte {
    let (pts, layout) = pontos(model, graph);
    let plano = canvas_pts(&pts);
    let rota: Vec<usize> = rastro
        .rotas
        .first()
        .map(|r| r.0.clone())
        .unwrap_or_else(|| {
            crate::carta::main_path(graph)
                .into_iter()
                .filter_map(|i| {
                    model
                        .objects
                        .iter()
                        .position(|o| o.name == graph.nodes[i].name)
                })
                .collect()
        });
    let mut b = String::new();
    if let Some(f) = fundo {
        // Hook for the CAD floor plan: any SVG fragment in plan coordinates of the canvas.
        let _ = write!(b, "<g class=\"planta\">{f}</g>");
    }
    let mut csv = String::from("ordem,de,para,distancia\n");
    let mut tab = String::from(
        "<table><thead><tr><th>#</th><th>De</th><th>Para</th><th class=\"n\">Distância</th></tr></thead><tbody>",
    );
    for (k, w) in rota.windows(2).enumerate() {
        let (a, c) = (w[0], w[1]);
        if let (Some(pa), Some(pc)) = (pts[a], pts[c]) {
            let (pa, pc) = (plano.at(pa), plano.at(pc));
            let _ = write!(
                b,
                "<path class=\"fino\" style=\"stroke:{};stroke-width:2\" marker-end=\"url(#ponta)\" d=\"M{:.1} {:.1} L{:.1} {:.1}\"/>",
                PALETTE[1], pa.0, pa.1, pc.0, pc.1
            );
            let (mx, my) = ((pa.0 + pc.0) / 2.0, (pa.1 + pc.1) / 2.0);
            let _ = write!(
                b,
                "<circle cx=\"{mx:.1}\" cy=\"{my:.1}\" r=\"9\" style=\"fill:{};stroke:none\"/>\
                 <text x=\"{mx:.1}\" y=\"{:.1}\" text-anchor=\"middle\" style=\"fill:#fff;font-size:10px;font-weight:700\">{}</text>",
                PALETTE[1],
                my + 3.5,
                k + 1
            );
        }
        let d = (!layout).then(|| dist3(rastro.pos[a], rastro.pos[c]));
        let _ = write!(
            tab,
            "<tr><td class=\"n\">{}</td><td>{}</td><td>{}</td><td class=\"n\">{}</td></tr>",
            k + 1,
            esc(&rastro.nomes[a]),
            esc(&rastro.nomes[c]),
            d.map(num).unwrap_or_else(|| "-".into())
        );
        row(
            &mut csv,
            &[
                (k + 1).to_string(),
                rastro.nomes[a].clone(),
                rastro.nomes[c].clone(),
                d.map(csvnum).unwrap_or_default(),
            ],
        );
    }
    tab.push_str("</tbody></table>");
    desenhar_passos(&mut b, model, graph, &pts, &plano);
    let svg = svg_doc(plano.w, plano.h, "Mapofluxograma", &b, true);
    let mut nota = String::new();
    if layout {
        nota.push_str("<p class=\"muted\">O modelo não tem posições (tudo em 0, 0, 0): o desenho usa a disposição do fluxograma de blocos e não há distâncias.</p>");
    }
    let n = rastro.rotas.first().map_or(0, |r| r.1);
    let _ = write!(
        nota,
        "<p class=\"muted\">Percurso mostrado: o mais usado pelos itens ({n} itens fizeram esse caminho).</p>"
    );
    Parte {
        svg: Some(svg),
        tabelas: format!("{nota}{tab}"),
        csv: Some(csv),
    }
}

/// Totals of the distance travelled.
#[derive(Clone, Debug)]
pub struct Totais {
    /// `(resource, trips, total distance)` per mobile resource.
    pub recursos: Vec<(String, usize, f64)>,
    pub itens_viagens: usize,
    pub itens_dist: f64,
    pub itens: usize,
}

pub fn totais(rastro: &Rastro) -> Totais {
    let mut by: BTreeMap<usize, (usize, f64)> = BTreeMap::new();
    for p in &rastro.pernas {
        let e = by.entry(p.carrier).or_default();
        e.0 += 1;
        e.1 += p.dist;
    }
    let mut itens = std::collections::BTreeSet::new();
    for v in &rastro.viagens {
        itens.insert(v.item);
    }
    Totais {
        recursos: by
            .into_iter()
            .map(|(c, (n, d))| (rastro.nomes[c].clone(), n, d))
            .collect(),
        itens_viagens: rastro.viagens.len(),
        itens_dist: rastro.viagens.iter().map(|v| v.dist).sum(),
        itens: itens.len(),
    }
}

fn totais_html(t: &Totais) -> String {
    let mut h = String::from(
        "<table><thead><tr><th>Recurso</th><th class=\"n\">Viagens</th><th class=\"n\">Distância total</th><th class=\"n\">Média por viagem</th></tr></thead><tbody>",
    );
    for (nome, n, d) in &t.recursos {
        let _ = write!(
            h,
            "<tr><td>{}</td><td class=\"n\">{n}</td><td class=\"n\">{}</td><td class=\"n\">{}</td></tr>",
            esc(nome),
            num(*d),
            num(d / (*n).max(1) as f64)
        );
    }
    let _ = write!(
        h,
        "<tr><td>Itens (todos)</td><td class=\"n\">{}</td><td class=\"n\">{}</td><td class=\"n\">{}</td></tr></tbody></table>\
         <p class=\"muted\">{} itens viajaram; cada um percorreu em média {} (soma das ligações entre objetos).</p>",
        t.itens_viagens,
        num(t.itens_dist),
        num(t.itens_dist / t.itens_viagens.max(1) as f64),
        t.itens,
        num(t.itens_dist / t.itens.max(1) as f64)
    );
    h
}

fn totais_csv(t: &Totais) -> String {
    let mut s = String::from("recurso,viagens,distancia_total,distancia_media\n");
    for (nome, n, d) in &t.recursos {
        row(
            &mut s,
            &[
                nome.clone(),
                n.to_string(),
                csvnum(*d),
                csvnum(d / (*n).max(1) as f64),
            ],
        );
    }
    row(
        &mut s,
        &[
            "itens".into(),
            t.itens_viagens.to_string(),
            csvnum(t.itens_dist),
            csvnum(t.itens_dist / t.itens_viagens.max(1) as f64),
        ],
    );
    s
}

/// Spaghetti SVG of one run.
pub fn espaguete_svg(
    model: &Model,
    graph: &FlowGraph,
    rastro: &Rastro,
    titulo: &str,
    standalone: bool,
) -> String {
    let (pts, layout) = pontos(model, graph);
    let mut all: Vec<(f64, f64)> = pts.iter().flatten().copied().collect();
    if !layout {
        all.extend(
            rastro
                .pernas
                .iter()
                .flat_map(|p| p.path.iter().map(|q| (q[0], q[1]))),
        );
    }
    let plano = Plano::new(&all);
    let mut b = String::new();
    // Items: one stroke per pair of objects, thicker the more trips.
    let mut pairs: BTreeMap<(usize, usize), usize> = BTreeMap::new();
    for v in &rastro.viagens {
        *pairs.entry((v.de, v.para)).or_default() += 1;
    }
    let max_pair = pairs.values().copied().max().unwrap_or(1) as f64;
    for (&(a, c), &n) in &pairs {
        if let (Some(pa), Some(pc)) = (pts[a], pts[c]) {
            let (pa, pc) = (plano.at(pa), plano.at(pc));
            let _ = write!(
                b,
                "<path style=\"fill:none;stroke:{};stroke-opacity:.55;stroke-width:{:.1}\" d=\"M{:.1} {:.1} L{:.1} {:.1}\"/>",
                PALETTE[0],
                1.0 + 7.0 * n as f64 / max_pair,
                pa.0,
                pa.1,
                pc.0,
                pc.1
            );
        }
    }
    // Mobile resources: identical segments add up.
    if !layout {
        let mut segs: BTreeMap<(usize, [i64; 4]), usize> = BTreeMap::new();
        for p in &rastro.pernas {
            for w in p.path.windows(2) {
                let k =
                    |q: [f64; 3]| ((q[0] * 100.0).round() as i64, (q[1] * 100.0).round() as i64);
                let (a, c) = (k(w[0]), k(w[1]));
                let (a, c) = if a <= c { (a, c) } else { (c, a) };
                *segs.entry((p.carrier, [a.0, a.1, c.0, c.1])).or_default() += 1;
            }
        }
        let max_seg = segs.values().copied().max().unwrap_or(1) as f64;
        for (&(carrier, s), &n) in &segs {
            let pa = plano.at((s[0] as f64 / 100.0, s[1] as f64 / 100.0));
            let pc = plano.at((s[2] as f64 / 100.0, s[3] as f64 / 100.0));
            let _ = write!(
                b,
                "<path style=\"fill:none;stroke:{};stroke-opacity:.7;stroke-width:{:.1}\" d=\"M{:.1} {:.1} L{:.1} {:.1}\"/>",
                PALETTE[1 + carrier % 7],
                1.0 + 7.0 * n as f64 / max_seg,
                pa.0,
                pa.1,
                pc.0,
                pc.1
            );
        }
    }
    desenhar_passos(&mut b, model, graph, &pts, &plano);
    // Legend: one colour per resource.
    let mut ly = 18.0;
    b.push_str(&format!(
        "<line x1=\"14\" y1=\"{ly}\" x2=\"38\" y2=\"{ly}\" style=\"stroke:{};stroke-width:3\"/>{}",
        PALETTE[0],
        texto(44.0, ly + 4.0, "etq", "itens")
    ));
    let mut ids: Vec<usize> = rastro.pernas.iter().map(|p| p.carrier).collect();
    ids.sort_unstable();
    ids.dedup();
    for c in ids {
        ly += 16.0;
        b.push_str(&format!(
            "<line x1=\"14\" y1=\"{ly}\" x2=\"38\" y2=\"{ly}\" style=\"stroke:{};stroke-width:3\"/>{}",
            PALETTE[1 + c % 7],
            texto(44.0, ly + 4.0, "etq", &rastro.nomes[c])
        ));
    }
    svg_doc(plano.w, plano.h, titulo, &b, standalone)
}

pub fn espaguete(model: &Model, graph: &FlowGraph, rastro: &Rastro) -> Parte {
    let t = totais(rastro);
    let (_, layout) = pontos(model, graph);
    let mut nota = String::new();
    if layout {
        nota.push_str("<p class=\"muted\">O modelo não tem posições: o desenho usa a disposição dos blocos e mostra só as viagens dos itens; os percursos das empilhadeiras e pessoas dependem das posições.</p>");
    }
    Parte {
        svg: Some(espaguete_svg(
            model,
            graph,
            rastro,
            "Diagrama de espaguete",
            true,
        )),
        tabelas: format!("{nota}{}", totais_html(&t)),
        csv: Some(totais_csv(&t)),
    }
}

/// Before and after: two models side by side, with the distances.
pub fn comparar(
    antes: (&Model, &FlowGraph, &Rastro),
    depois: (&Model, &FlowGraph, &Rastro),
) -> Parte {
    let (ta, tb) = (totais(antes.2), totais(depois.2));
    let mut names: Vec<String> = ta.recursos.iter().map(|r| r.0.clone()).collect();
    for r in &tb.recursos {
        if !names.contains(&r.0) {
            names.push(r.0.clone());
        }
    }
    let dist = |t: &Totais, n: &str| t.recursos.iter().find(|r| r.0 == n).map_or(0.0, |r| r.2);
    let mut rows: Vec<(String, f64, f64)> = names
        .iter()
        .map(|n| (n.clone(), dist(&ta, n), dist(&tb, n)))
        .collect();
    rows.push(("Itens (todos)".into(), ta.itens_dist, tb.itens_dist));
    rows.push((
        "Itens (média por item)".into(),
        ta.itens_dist / ta.itens.max(1) as f64,
        tb.itens_dist / tb.itens.max(1) as f64,
    ));
    let mut tab = String::from(
        "<table><thead><tr><th>Distância percorrida</th><th class=\"n\">Antes</th><th class=\"n\">Depois</th><th class=\"n\">Variação</th></tr></thead><tbody>",
    );
    let mut csv = String::from("recurso,antes,depois,variacao_pct\n");
    for (n, a, d) in &rows {
        let var = if *a > 0.0 {
            Some(100.0 * (d - a) / a)
        } else {
            None
        };
        let _ = write!(
            tab,
            "<tr><td>{}</td><td class=\"n\">{}</td><td class=\"n\">{}</td><td class=\"n\">{}</td></tr>",
            esc(n),
            num(*a),
            num(*d),
            var.map_or("-".into(), |v| format!("{}%", num(v)))
        );
        row(
            &mut csv,
            &[
                n.clone(),
                csvnum(*a),
                csvnum(*d),
                var.map(csvnum).unwrap_or_default(),
            ],
        );
    }
    tab.push_str("</tbody></table>");
    let sa = espaguete_svg(antes.0, antes.1, antes.2, "Antes", false);
    let sb = espaguete_svg(depois.0, depois.1, depois.2, "Depois", false);
    let side = format!(
        "<div style=\"display:flex;flex-wrap:wrap;gap:1rem\"><div class=\"rolagem\" style=\"flex:1 1 480px\"><h3>Antes: {}</h3>{sa}</div>\
         <div class=\"rolagem\" style=\"flex:1 1 480px\"><h3>Depois: {}</h3>{sb}</div></div>",
        esc(&antes.0.name),
        esc(&depois.0.name)
    );
    Parte {
        svg: None,
        tabelas: format!("{side}{tab}"),
        csv: Some(csv),
    }
}
