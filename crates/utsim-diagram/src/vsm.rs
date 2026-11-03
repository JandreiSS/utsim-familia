//! Current-state value stream map: processes with cycle time, setup, availability and
//! operators; inventories between them (mean queue); timeline of value-added time against
//! the lead time measured in the simulation.

use std::fmt::Write as _;

use utsim_model::csv::{num as csvnum, row};
use utsim_model::{Model, ObjectKind};

use crate::carta::main_path;
use crate::desenho::{PALETTE, Parte, curto, svg_doc, texto};
use crate::graph::{FlowGraph, Symbol};
use crate::rastro::Rastro;
use crate::text::{esc, num, pct};

pub struct Processo {
    pub nome: String,
    pub ciclo: f64,
    pub setup: f64,
    pub disponibilidade: f64,
    pub operadores: usize,
    pub servidores: usize,
    /// Value-added: operations (inspections are not).
    pub agrega_valor: bool,
    /// Mean content of the queue before it.
    pub estoque: Option<f64>,
}

/// One stretch of the timeline.
pub struct Trecho {
    pub tempo: f64,
    pub valor: bool,
}

pub struct Vsm {
    pub processos: Vec<Processo>,
    pub trechos: Vec<Trecho>,
    pub lead_time: Option<f64>,
}

fn media(d: &utsim_core::Dist) -> f64 {
    let m = d.mean();
    if m.is_finite() { m } else { 0.0 }
}

pub fn montar(model: &Model, graph: &FlowGraph, rastro: &Rastro) -> Vsm {
    let mut processos = Vec::new();
    let mut trechos = Vec::new();
    let mut estoque: Option<f64> = None;
    for i in main_path(graph) {
        let node = &graph.nodes[i];
        let Some(obj) = model.objects.iter().find(|o| o.name == node.name) else {
            continue;
        };
        let rep = rastro.bloco(&node.name);
        match node.symbol {
            Symbol::Operacao | Symbol::Inspecao => {
                let (ciclo, setup, operadores, servidores) = match &obj.kind {
                    ObjectKind::Processor {
                        process,
                        setup,
                        operators,
                        capacity,
                    } => (
                        media(process),
                        setup.as_ref().map_or(0.0, media),
                        operators.as_ref().map_or(0, |o| o.count),
                        *capacity,
                    ),
                    ObjectKind::Combiner { process, setup, .. } => {
                        (media(process), setup.as_ref().map_or(0.0, media), 0, 1)
                    }
                    ObjectKind::Separator { process, .. } => (media(process), 0.0, 0, 1),
                    _ => (0.0, 0.0, 0, 1),
                };
                let disponibilidade = obj.downtime.as_ref().map_or(1.0, |d| {
                    let (up, down) = (media(&d.mtbf), media(&d.mttr));
                    if up + down > 0.0 {
                        up / (up + down)
                    } else {
                        1.0
                    }
                });
                let agrega_valor = node.symbol == Symbol::Operacao;
                processos.push(Processo {
                    nome: node.name.clone(),
                    ciclo,
                    setup,
                    disponibilidade,
                    operadores,
                    servidores,
                    agrega_valor,
                    estoque: estoque.take(),
                });
                trechos.push(Trecho {
                    tempo: ciclo + setup,
                    valor: agrega_valor,
                });
            }
            Symbol::Espera | Symbol::Armazenagem => {
                if node.symbol == Symbol::Espera {
                    estoque = Some(estoque.unwrap_or(0.0) + rep.map_or(0.0, |r| r.content_mean));
                }
                trechos.push(Trecho {
                    tempo: rep.map_or(0.0, |r| r.staytime.mean),
                    valor: false,
                });
            }
            Symbol::Transporte => trechos.push(Trecho {
                tempo: node.time.unwrap_or(0.0),
                valor: false,
            }),
            _ => {}
        }
    }
    let lead_time = graph
        .nodes
        .iter()
        .filter(|n| n.symbol == Symbol::Fim)
        .filter_map(|n| rastro.bloco(&n.name))
        .filter_map(|b| b.extra.get("time_in_system"))
        .map(|t| t.mean)
        .next();
    Vsm {
        processos,
        trechos,
        lead_time,
    }
}

impl Vsm {
    pub fn valor(&self) -> f64 {
        self.trechos
            .iter()
            .filter(|t| t.valor)
            .map(|t| t.tempo)
            .sum()
    }

    pub fn soma(&self) -> f64 {
        self.trechos.iter().map(|t| t.tempo).sum()
    }

    /// Value-added index: value-added time over the measured lead time.
    pub fn indice(&self) -> Option<f64> {
        self.lead_time
            .filter(|l| *l > 0.0)
            .map(|l| self.valor() / l)
    }
}

pub fn gerar(model: &Model, graph: &FlowGraph, rastro: &Rastro) -> Parte {
    let v = montar(model, graph, rastro);
    let (bw, bh, gap) = (170.0, 120.0, 70.0);
    let n = v.processos.len();
    let width = (120.0 + n as f64 * (bw + gap) + 120.0).max(900.0);
    let height = 330.0;
    let mut b = String::new();
    // Supplier and customer.
    b.push_str(&texto(16.0, 40.0, "b", "Fornecedor"));
    let _ = write!(
        b,
        "<rect class=\"caixa2\" x=\"10\" y=\"50\" width=\"90\" height=\"60\"/>{}",
        texto(18.0, 84.0, "etq", "chegadas")
    );
    let cx = 120.0 + n as f64 * (bw + gap);
    let _ = write!(
        b,
        "<rect class=\"caixa2\" x=\"{cx:.1}\" y=\"50\" width=\"90\" height=\"60\"/>{}{}",
        texto(cx + 8.0, 40.0, "b", "Cliente"),
        texto(cx + 18.0, 84.0, "etq", "saídas")
    );
    for (k, p) in v.processos.iter().enumerate() {
        let x = 120.0 + k as f64 * (bw + gap);
        let _ = write!(
            b,
            "<rect class=\"caixa2\" x=\"{x:.1}\" y=\"50\" width=\"{bw}\" height=\"{bh}\"/>\
             <rect x=\"{x:.1}\" y=\"50\" width=\"{bw}\" height=\"22\" style=\"fill:{};opacity:.85\"/>{}",
            PALETTE[0],
            texto(x + 8.0, 66.0, "b", &curto(&p.nome, 20))
        );
        let lines = [
            format!("TC = {}", num(p.ciclo)),
            format!("TR (setup) = {}", num(p.setup)),
            format!("Disponibilidade = {}", pct(p.disponibilidade)),
            format!("Operadores = {}", p.operadores),
            format!("Servidores = {}", p.servidores),
        ];
        for (j, l) in lines.iter().enumerate() {
            b.push_str(&texto(x + 8.0, 92.0 + 16.0 * j as f64, "etq", l));
        }
        // Inventory triangle between processes.
        let ix = x - gap / 2.0 - 6.0;
        if p.estoque.is_some() {
            let _ = write!(
                b,
                "<polygon class=\"simbolo\" points=\"{:.1},{:.1} {:.1},{:.1} {:.1},{:.1}\"/>{}{}",
                ix,
                64.0,
                ix + 24.0,
                64.0,
                ix + 12.0,
                38.0,
                texto(ix + 8.0, 60.0, "etq b", "I"),
                texto(
                    ix - 8.0,
                    84.0,
                    "etq mut",
                    &p.estoque.map_or("-".into(), num)
                )
            );
        }
        let ax = if k == 0 { 100.0 } else { x - gap + 20.0 };
        let _ = write!(
            b,
            "<path class=\"seta\" marker-end=\"url(#ponta)\" d=\"M{:.1} 110 H{:.1}\"/>",
            ax,
            x - 2.0
        );
    }
    if n > 0 {
        let _ = write!(
            b,
            "<path class=\"seta\" marker-end=\"url(#ponta)\" d=\"M{:.1} 110 H{cx:.1}\"/>",
            120.0 + (n - 1) as f64 * (bw + gap) + bw
        );
    }
    // Timeline: stretches in order, wide in proportion to the time.
    let total = v.soma().max(1e-9);
    let (tx, tw, ty) = (120.0, width - 160.0, 230.0);
    b.push_str(&texto(16.0, ty - 14.0, "b", "Linha do tempo"));
    let mut x = tx;
    for t in &v.trechos {
        let w = (tw * t.tempo / total).max(1.0);
        let _ = write!(
            b,
            "<rect x=\"{x:.1}\" y=\"{ty}\" width=\"{w:.1}\" height=\"26\" style=\"fill:{};opacity:.85\"/>",
            if t.valor { "#16a34a" } else { "#c2410c" }
        );
        if w > 44.0 {
            b.push_str(&texto(x + 3.0, ty + 17.0, "etq", &num(t.tempo)));
        }
        x += w;
    }
    b.push_str(&texto(
        16.0,
        ty + 52.0,
        "",
        &format!(
            "Tempo de agregação de valor: {}   |   Soma das etapas: {}   |   Lead time medido: {}   |   Índice de agregação de valor: {}",
            num(v.valor()),
            num(v.soma()),
            v.lead_time.map_or("-".into(), num),
            v.indice().map_or("-".into(), pct)
        ),
    ));
    b.push_str(&texto(
        16.0,
        ty + 72.0,
        "etq mut",
        "Verde: operações (agregam valor). Laranja: esperas, transportes, inspeções e armazenagem.",
    ));
    let mut tab = String::from(
        "<table><thead><tr><th>Processo</th><th class=\"n\">Ciclo</th><th class=\"n\">Setup</th><th class=\"n\">Disponibilidade</th><th class=\"n\">Operadores</th><th class=\"n\">Estoque antes (média)</th><th>Agrega valor</th></tr></thead><tbody>",
    );
    let mut csv = String::from(
        "processo,ciclo,setup,disponibilidade,operadores,estoque_anterior,agrega_valor\n",
    );
    for p in &v.processos {
        let _ = write!(
            tab,
            "<tr><td>{}</td><td class=\"n\">{}</td><td class=\"n\">{}</td><td class=\"n\">{}</td><td class=\"n\">{}</td><td class=\"n\">{}</td><td>{}</td></tr>",
            esc(&p.nome),
            num(p.ciclo),
            num(p.setup),
            pct(p.disponibilidade),
            p.operadores,
            p.estoque.map_or("-".into(), num),
            if p.agrega_valor {
                "sim"
            } else {
                "não (inspeção)"
            }
        );
        row(
            &mut csv,
            &[
                p.nome.clone(),
                csvnum(p.ciclo),
                csvnum(p.setup),
                csvnum(p.disponibilidade),
                p.operadores.to_string(),
                p.estoque.map(csvnum).unwrap_or_default(),
                p.agrega_valor.to_string(),
            ],
        );
    }
    tab.push_str("</tbody></table>");
    Parte {
        svg: Some(svg_doc(width, height, "Mapa do fluxo de valor", &b, true)),
        tabelas: tab,
        csv: Some(csv),
    }
}
