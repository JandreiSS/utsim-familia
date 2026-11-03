//! SIPOC diagram: suppliers, inputs, process, outputs, customers, derived from the sources,
//! the main steps and the sinks of the model.

use std::fmt::Write as _;

use utsim_model::csv::row;

use crate::carta::main_path;
use crate::desenho::{curto, svg_doc, texto};
use crate::graph::{FlowGraph, NodeKind, Symbol};
use crate::text::esc;

pub struct Sipoc {
    pub fornecedores: Vec<String>,
    pub entradas: Vec<String>,
    pub processo: Vec<String>,
    pub saidas: Vec<String>,
    pub clientes: Vec<String>,
}

pub const TITULOS: [&str; 5] = [
    "Fornecedores (S)",
    "Entradas (I)",
    "Processo (P)",
    "Saídas (O)",
    "Clientes (C)",
];

pub fn montar(graph: &FlowGraph) -> Sipoc {
    let mut s = Sipoc {
        fornecedores: Vec::new(),
        entradas: Vec::new(),
        processo: Vec::new(),
        saidas: Vec::new(),
        clientes: Vec::new(),
    };
    for n in &graph.nodes {
        match n.kind {
            NodeKind::Source => {
                s.fornecedores.push(n.name.clone());
                let how = n.details.first().cloned().unwrap_or_default();
                s.entradas.push(if how.is_empty() {
                    format!("itens de {}", n.name)
                } else {
                    format!("itens de {}: {how}", n.name)
                });
            }
            NodeKind::Sink => {
                s.clientes.push(n.name.clone());
                s.saidas.push(format!("itens entregues em {}", n.name));
            }
            _ => {}
        }
    }
    let mut k = 0;
    for i in main_path(graph) {
        let n = &graph.nodes[i];
        if matches!(
            n.symbol,
            Symbol::Operacao | Symbol::Inspecao | Symbol::Armazenagem
        ) {
            k += 1;
            s.processo
                .push(format!("{k}. {} ({})", n.name, n.symbol.nome()));
        }
    }
    s
}

impl Sipoc {
    fn colunas(&self) -> [&Vec<String>; 5] {
        [
            &self.fornecedores,
            &self.entradas,
            &self.processo,
            &self.saidas,
            &self.clientes,
        ]
    }

    pub fn csv(&self) -> String {
        let mut out = String::from("fornecedores,entradas,processo,saidas,clientes\n");
        let rows = self.colunas().iter().map(|c| c.len()).max().unwrap_or(0);
        for r in 0..rows {
            let cells: Vec<String> = self
                .colunas()
                .iter()
                .map(|c| c.get(r).cloned().unwrap_or_default())
                .collect();
            row(&mut out, &cells);
        }
        out
    }

    pub fn html_table(&self) -> String {
        let mut h = String::from("<table><thead><tr>");
        for t in TITULOS {
            let _ = write!(h, "<th>{}</th>", esc(t));
        }
        h.push_str("</tr></thead><tbody><tr>");
        for c in self.colunas() {
            h.push_str("<td><ul>");
            for x in c {
                let _ = write!(h, "<li>{}</li>", esc(x));
            }
            h.push_str("</ul></td>");
        }
        h.push_str("</tr></tbody></table>");
        h
    }

    pub fn svg(&self, titulo: &str, standalone: bool) -> String {
        let (cw, gap, top) = (200.0, 34.0, 44.0);
        let rows = self
            .colunas()
            .iter()
            .map(|c| c.len())
            .max()
            .unwrap_or(1)
            .max(1);
        let h = top + rows as f64 * 26.0 + 30.0;
        let w = 5.0 * cw + 4.0 * gap + 40.0;
        let mut b = String::new();
        for (k, col) in self.colunas().iter().enumerate() {
            let x = 20.0 + k as f64 * (cw + gap);
            let _ = write!(
                b,
                "<rect class=\"caixa2\" x=\"{x:.1}\" y=\"10\" width=\"{cw}\" height=\"{:.1}\" rx=\"6\"/>",
                h - 20.0
            );
            b.push_str(&texto(x + 10.0, 32.0, "b", TITULOS[k]));
            for (r, t) in col.iter().enumerate() {
                b.push_str(&texto(
                    x + 10.0,
                    top + 16.0 + r as f64 * 26.0,
                    "",
                    &curto(t, 30),
                ));
            }
            if k < 4 {
                let ax = x + cw;
                let _ = write!(
                    b,
                    "<path class=\"seta\" marker-end=\"url(#ponta)\" d=\"M{:.1} {:.1} H{:.1}\"/>",
                    ax + 2.0,
                    h / 2.0,
                    ax + gap - 2.0
                );
            }
        }
        svg_doc(w, h, titulo, &b, standalone)
    }
}
