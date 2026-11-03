//! The flowchart as Mermaid text (`flowchart LR`), to paste into documents.

use std::fmt::Write as _;

use crate::graph::{FlowGraph, Symbol};
use crate::text::{num, pct};

/// Escapes one piece of text for a quoted Mermaid label.
fn q(s: &str) -> String {
    s.replace('&', "#amp;")
        .replace('"', "#quot;")
        .replace('<', "#lt;")
        .replace('>', "#gt;")
        .replace(['\n', '\r'], " ")
}

fn id(i: usize) -> String {
    format!("n{i}")
}

fn colors(s: Symbol) -> &'static str {
    match s {
        Symbol::Inicio | Symbol::Fim => "fill:#e5e7eb,stroke:#475569,color:#1c1f24",
        Symbol::Operacao => "fill:#dbeafe,stroke:#475569,color:#1c1f24",
        Symbol::Transporte => "fill:#dcfce7,stroke:#475569,color:#1c1f24",
        Symbol::Inspecao => "fill:#fef3c7,stroke:#475569,color:#1c1f24",
        Symbol::Espera => "fill:#fee2e2,stroke:#475569,color:#1c1f24",
        Symbol::Armazenagem => "fill:#ede9fe,stroke:#475569,color:#1c1f24",
    }
}

/// The Mermaid source. Steps are `n0`, `n1`... in the order of the model.
pub fn mermaid(graph: &FlowGraph) -> String {
    let mut s = String::new();
    let _ = writeln!(s, "%% Fluxograma de {}", q(&graph.name).replace('\n', " "));
    s.push_str("flowchart LR\n");
    for (i, n) in graph.nodes.iter().enumerate() {
        let mut label = q(&n.name);
        if let Some(t) = n.time.filter(|_| graph.show_times) {
            let _ = write!(label, "<br/>tempo {}", num(t));
        }
        if let Some(u) = n.stats.as_ref().and_then(|s| s.utilization) {
            let _ = write!(label, "<br/>ocupação {}", pct(u));
        }
        let l = format!("\"{label}\"");
        let shape = match n.symbol {
            Symbol::Inicio | Symbol::Fim => format!("([{l}])"),
            Symbol::Operacao => format!("(({l}))"),
            Symbol::Transporte => format!(">{l}]"),
            Symbol::Inspecao => format!("[{l}]"),
            Symbol::Espera => format!("({l})"),
            Symbol::Armazenagem => format!("{{{{{l}}}}}"),
        };
        let _ = writeln!(s, "    {}{shape}", id(i));
    }
    for e in &graph.edges {
        let mut parts: Vec<String> = Vec::new();
        if let Some(l) = &e.label {
            parts.push(q(l));
        }
        if let Some(t) = &e.transport {
            parts.push(q(&t.text()));
        }
        let arrow = if e.rework {
            parts.insert(0, "retrabalho".to_string());
            "-.->"
        } else {
            "-->"
        };
        if parts.is_empty() {
            let _ = writeln!(s, "    {} {arrow} {}", id(e.from), id(e.to));
        } else {
            let _ = writeln!(
                s,
                "    {} {arrow}|\"{}\"| {}",
                id(e.from),
                parts.join("<br/>"),
                id(e.to)
            );
        }
    }
    for sym in Symbol::ALL {
        let _ = writeln!(s, "    classDef {} {};", sym.id(), colors(sym));
    }
    for sym in Symbol::ALL {
        let ids: Vec<String> = graph
            .nodes
            .iter()
            .enumerate()
            .filter(|(_, n)| n.symbol == sym)
            .map(|(i, _)| id(i))
            .collect();
        if !ids.is_empty() {
            let _ = writeln!(s, "    class {} {};", ids.join(","), sym.id());
        }
    }
    s
}
