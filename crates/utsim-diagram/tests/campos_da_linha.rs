//! The model fields `symbol` and `department` feed the flowchart: the symbol of a step (the
//! command-line choice still wins) and the lane of the swimlane chart.

use std::collections::BTreeMap;

use utsim_diagram::graph::{Options, Symbol};
use utsim_diagram::{TipoDiagrama, gerar};
use utsim_model::Model;

const MODEL: &str = r#"Model(name: "m", run: (stop_time: 100.0), objects: [
    (name: "pedidos", kind: Source(interarrival: Const(1.0)), department: "Cliente"),
    (name: "analise", kind: Processor(process: Const(0.5)), department: "Compras", symbol: "inspecao"),
    (name: "fim", kind: Sink, department: "Compras"),
], connections: [("pedidos", "analise"), ("analise", "fim")])"#;

#[test]
fn symbol_of_the_model_unless_the_option_says_otherwise() {
    let m = Model::parse(MODEL).unwrap();
    let g = utsim_diagram::flowchart(&m, &Options::default());
    assert_eq!(g.nodes[1].symbol, Symbol::Inspecao);
    let mut o = Options::default();
    o.overrides.insert("analise".into(), Symbol::Operacao);
    let g = utsim_diagram::flowchart(&m, &o);
    assert_eq!(g.nodes[1].symbol, Symbol::Operacao);
}

#[test]
fn lanes_come_from_the_area_field() {
    let m = Model::parse(MODEL).unwrap();
    let areas = utsim_diagram::areas(&m, &BTreeMap::new());
    assert_eq!(areas["analise"], "Compras");
    let mut chosen = BTreeMap::new();
    chosen.insert("analise".to_string(), "Diretoria".to_string());
    assert_eq!(utsim_diagram::areas(&m, &chosen)["analise"], "Diretoria");
    let out = gerar(TipoDiagrama::Raias, &m, &Options::default()).unwrap();
    let svg = out.svg.unwrap();
    assert!(svg.contains("Cliente") && svg.contains("Compras"));
    assert!(out.html.contains("<td>Cliente</td><td>Compras</td>"));
}
