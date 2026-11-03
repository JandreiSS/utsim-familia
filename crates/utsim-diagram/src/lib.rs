//! Process flowchart of a UTSim model.
//!
//! Every object becomes a step with the standard symbol of the process flowchart of
//! methods engineering (operation, transport, inspection, wait, storage, start/end);
//! every connection becomes an arrow with its routing; teams and fleets appear as side
//! notes. Outputs: the flowchart as SVG and HTML, the process chart ("carta de processo")
//! as HTML and CSV, and Mermaid text. The crate is pure: it reads a [`Model`] and, when
//! asked, figures of a run, and writes text. See `docs/manual/src/fluxograma.md`.

pub mod carta;
mod depara;
mod desenho;
pub mod graph;
pub mod homem_maquina;
pub mod html;
pub mod layout;
mod mapa;
pub mod mermaid;
mod operacoes;
mod raias;
pub mod rastro;
mod sipoc;
pub mod svg;
pub mod tempos;
mod text;
pub mod tipos;
mod vsm;

use std::collections::BTreeMap;

pub use carta::{Carta, Row, Totals};
pub use graph::{
    Direction, Edge, EdgeTransport, FlowGraph, Node, NodeKind, NodeStats, Options, Resource,
    ResourceKind, Symbol, default_symbol,
};
pub use html::{carta_html, html};
pub use mapa::caixas_dos_rotulos;
pub use mermaid::mermaid;
pub use svg::{svg, svg_inline};
pub use tipos::{Saida, TipoDiagrama, gerar, indice};
use utsim_model::Model;
use utsim_model::analysis::ObjectStats;

/// Builds the graph of `model` and lays it out.
pub fn flowchart(model: &Model, options: &Options) -> FlowGraph {
    let mut g = graph::build(model, options);
    layout::apply(&mut g);
    g
}

/// The process chart of the main path, as CSV.
pub fn carta_csv(graph: &FlowGraph) -> String {
    carta::csv(&carta::build(graph))
}

/// Figures for the boxes from the statistics of a run
/// (`utsim_model::analysis::object_stats`).
pub fn stats_from_run(stats: &[ObjectStats]) -> BTreeMap<String, NodeStats> {
    stats
        .iter()
        .map(|s| {
            let queue = s.kind == "queue";
            (
                s.name.clone(),
                NodeStats {
                    utilization: s.utilization.as_ref().map(|u| u.mean),
                    mean_content: (queue || s.kind == "rack").then_some(s.content.mean),
                    mean_stay: queue.then_some(s.staytime.mean),
                    items: Some(if s.kind == "sink" {
                        s.input.mean
                    } else {
                        s.output.mean
                    }),
                },
            )
        })
        .collect()
}
