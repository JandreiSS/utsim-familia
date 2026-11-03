//! Small models written inline: each checks one rule of the graph.

use utsim_diagram::{NodeStats, Options, Symbol, carta, flowchart, mermaid, svg};
use utsim_model::Model;

fn model(objects: &str, connections: &str) -> Model {
    let text = format!(
        "Model(name: \"t\", run: (stop_time: 100.0), objects: [{objects}], connections: [{connections}])"
    );
    Model::parse(&text).unwrap()
}

const LINE: &str = r#"
    (name: "chegada", kind: Source(interarrival: Const(10.0))),
    (name: "fila", kind: Queue()),
    (name: "torno", kind: Processor(process: Const(5.0), setup: Some(Const(1.0)))),
    (name: "conferencia_final", kind: Processor(process: Const(2.0))),
    (name: "prateleira", kind: Rack(bays: 2, levels: 3, dwell: Some(Const(30.0)))),
    (name: "saida", kind: Sink),
"#;
const LINE_CONN: &str = r#"("chegada","fila"),("fila","torno"),("torno","conferencia_final"),
    ("conferencia_final","prateleira"),("prateleira","saida")"#;

#[test]
fn kinds_map_to_the_standard_symbols() {
    let g = flowchart(&model(LINE, LINE_CONN), &Options::default());
    let got: Vec<Symbol> = g.nodes.iter().map(|n| n.symbol).collect();
    assert_eq!(
        got,
        [
            Symbol::Inicio,
            Symbol::Espera,
            Symbol::Operacao,
            Symbol::Inspecao,
            Symbol::Armazenagem,
            Symbol::Fim
        ]
    );
    assert_eq!(g.nodes.len(), 6);
    assert_eq!(g.edges.len(), 5);
    // Times: setup + process, shelf dwell.
    assert_eq!(g.nodes[2].time, Some(6.0));
    assert_eq!(g.nodes[4].time, Some(30.0));
}

#[test]
fn keywords_are_configurable_and_overrides_win() {
    let mut o = Options::default();
    o.inspection_keywords.clear();
    o.overrides.insert("torno".into(), Symbol::Inspecao);
    let g = flowchart(&model(LINE, LINE_CONN), &o);
    assert_eq!(g.nodes[2].symbol, Symbol::Inspecao);
    assert_eq!(g.nodes[3].symbol, Symbol::Operacao);
}

#[test]
fn symbols_are_read_from_text() {
    assert_eq!(Symbol::from_id("Inspeção"), Some(Symbol::Inspecao));
    assert_eq!(Symbol::from_id("D"), Some(Symbol::Espera));
    assert_eq!(Symbol::from_id("armazenagem"), Some(Symbol::Armazenagem));
    assert_eq!(Symbol::from_id("xis"), None);
}

#[test]
fn weighted_branches_show_percentages_and_junction_is_noted() {
    let g = flowchart(
        &model(
            r#"
            (name: "a", kind: Source(interarrival: Const(1.0)), output: Weighted([3.0, 1.0])),
            (name: "b", kind: Processor(process: Const(1.0))),
            (name: "c", kind: Processor(process: Const(2.0))),
            (name: "d", kind: Sink),"#,
            r#"("a","b"),("a","c"),("b","d"),("c","d")"#,
        ),
        &Options::default(),
    );
    assert_eq!(g.edges[0].label.as_deref(), Some("75%"));
    assert_eq!(g.edges[1].label.as_deref(), Some("25%"));
    assert_eq!(g.edges[0].probability, Some(0.75));
    assert!(g.nodes[3].notes.iter().any(|n| n.contains("junção")));
    // Same layer for the two branches, the join one layer below.
    assert_eq!(g.nodes[1].layer, g.nodes[2].layer);
    assert_eq!(g.nodes[3].layer, 2);
    // The main path follows the 75% branch.
    let c = carta::build(&g);
    let names: Vec<&str> = c.rows.iter().filter_map(|r| r.objeto.as_deref()).collect();
    assert_eq!(names, ["a", "b", "d"]);
    assert!(c.rows[0].observacao.contains("ramifica"));
    let m = mermaid(&g);
    assert!(m.contains("|\"75%\"|"));
}

#[test]
fn a_loop_is_rework() {
    let g = flowchart(
        &model(
            r#"
            (name: "a", kind: Source(interarrival: Const(1.0))),
            (name: "p", kind: Processor(process: Const(1.0))),
            (name: "q", kind: Processor(process: Const(1.0)), output: Weighted([9.0, 1.0])),
            (name: "z", kind: Sink),"#,
            r#"("a","p"),("p","q"),("q","z"),("q","p")"#,
        ),
        &Options::default(),
    );
    let rework: Vec<_> = g.edges.iter().filter(|e| e.rework).collect();
    assert_eq!(rework.len(), 1);
    assert_eq!((rework[0].from, rework[0].to), (2, 1));
    assert!(mermaid(&g).contains("-.->|\"retrabalho"));
    assert!(svg(&g).contains("retrabalho"));
    // The loop does not push the step back: layers still grow forward.
    assert!(g.nodes[1].layer < g.nodes[2].layer);
}

#[test]
fn combiner_inputs_are_labelled() {
    let g = flowchart(
        &model(
            r#"
            (name: "c1", kind: Source(interarrival: Const(1.0))),
            (name: "p1", kind: Source(interarrival: Const(1.0))),
            (name: "mont", kind: Combiner(components: [2], process: Const(3.0))),
            (name: "z", kind: Sink),"#,
            r#"("c1","mont"),("p1","mont"),("mont","z")"#,
        ),
        &Options::default(),
    );
    assert_eq!(g.edges[0].label.as_deref(), Some("contêiner"));
    assert_eq!(g.edges[1].label.as_deref(), Some("2× componente"));
}

#[test]
fn timed_transfer_puts_distance_and_time_on_the_arrow() {
    let text = r#"Model(name: "t", run: (stop_time: 100.0, transfer: Some((speed: 2.0, delay: 1.0))),
        objects: [
        (name: "a", kind: Source(interarrival: Const(1.0)), position: (0.0, 0.0, 0.0)),
        (name: "b", kind: Sink, position: (10.0, 0.0, 0.0)),
        ], connections: [("a","b")])"#;
    let g = flowchart(&Model::parse(text).unwrap(), &Options::default());
    let t = g.edges[0].transport.as_ref().unwrap();
    assert_eq!(t.distance, Some(10.0));
    assert_eq!(t.time, Some(6.0));
    let c = carta::build(&g);
    assert_eq!(c.rows.len(), 3);
    assert_eq!(c.rows[1].symbol, Symbol::Transporte);
    assert_eq!(c.rows[1].distancia, Some(10.0));
    // Switching them off removes the numbers but keeps the arrow marked.
    let o = Options {
        show_distances: false,
        show_times: false,
        ..Options::default()
    };
    let g = flowchart(&Model::parse(text).unwrap(), &o);
    let t = g.edges[0].transport.as_ref().unwrap();
    assert_eq!((t.distance, t.time), (None, None));
}

#[test]
fn results_go_into_the_boxes_and_the_wait_into_the_chart() {
    let mut stats = std::collections::BTreeMap::new();
    stats.insert(
        "fila".to_string(),
        NodeStats {
            mean_content: Some(1.5),
            mean_stay: Some(12.0),
            ..NodeStats::default()
        },
    );
    stats.insert(
        "torno".to_string(),
        NodeStats {
            utilization: Some(0.632),
            ..NodeStats::default()
        },
    );
    let o = Options {
        stats: Some(stats),
        ..Options::default()
    };
    let g = flowchart(&model(LINE, LINE_CONN), &o);
    let s = svg(&g);
    assert!(s.contains("ocupação 63,2%"));
    assert!(s.contains("fila média 1,5 · espera média 12"));
    let c = carta::build(&g);
    assert_eq!(c.rows[1].tempo, Some(12.0));
}

#[test]
fn csv_has_one_line_per_row() {
    let g = flowchart(&model(LINE, LINE_CONN), &Options::default());
    let csv = utsim_diagram::carta_csv(&g);
    assert_eq!(csv.lines().count(), 7);
    assert!(csv.lines().nth(3).unwrap().contains("Operação"));
}

#[test]
fn mobile_resources_are_notes_not_steps() {
    let g = flowchart(
        &model(
            r#"
            (name: "a", kind: Source(interarrival: Const(1.0))),
            (name: "p", kind: Processor(process: Const(1.0),
                operators: Some(OperatorUse(team: "eq", count: 1))), transport: "f1"),
            (name: "z", kind: Sink),
            (name: "f1", kind: Transporter(speed: 2.0), position: (0.0, 0.0, 0.0)),
            (name: "f2", kind: Transporter(speed: 2.0, agv: true)),
            (name: "eq", kind: Dispatcher(team: ["f1"])),"#,
            r#"("a","p"),("p","z")"#,
        ),
        &Options::default(),
    );
    assert_eq!(g.nodes.len(), 3);
    // f1 is in the team `eq`; f2 stands alone (an AGV); `eq` is a team.
    let names: Vec<(&str, &str)> = g
        .resources
        .iter()
        .map(|r| (r.name.as_str(), r.kind.nome()))
        .collect();
    assert_eq!(names, [("f2", "AGV"), ("eq", "Equipe")]);
    assert!(
        g.nodes[1]
            .resources
            .iter()
            .any(|r| r.starts_with("operadores: eq"))
    );
    assert!(
        g.nodes[1]
            .resources
            .iter()
            .any(|r| r.starts_with("transporte: f1"))
    );
}
