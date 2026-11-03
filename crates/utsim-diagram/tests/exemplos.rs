//! The models of `examples/` and the handmade ones produce flowcharts without error.

use std::path::PathBuf;

use utsim_diagram::{
    Direction, Options, Symbol, carta, carta_csv, flowchart, html, mermaid, svg, svg_inline,
};
use utsim_model::Model;

fn examples_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../examples")
}

/// Every model of the folder (experiment and optimisation files are not models).
fn models() -> Vec<(String, Model)> {
    let mut v = Vec::new();
    for entry in std::fs::read_dir(examples_dir()).unwrap() {
        let path = entry.unwrap().path();
        let name = path.file_name().unwrap().to_string_lossy().to_string();
        if path.extension().is_none_or(|e| e != "ron")
            || name.starts_with("experimento_")
            || name.starts_with("otimizacao_")
        {
            continue;
        }
        let model = Model::load(&path).unwrap_or_else(|e| panic!("{name}: {e}"));
        v.push((name, model));
    }
    assert!(v.len() >= 20, "poucos exemplos: {}", v.len());
    v
}

fn example(name: &str) -> Model {
    Model::load(examples_dir().join(name)).unwrap()
}

/// Shape check of Mermaid text: header, balanced quotes and brackets, declared ids.
pub fn check_mermaid(text: &str) {
    let mut lines = text.lines().filter(|l| !l.trim().starts_with("%%"));
    assert_eq!(lines.next(), Some("flowchart LR"));
    let mut declared: Vec<String> = Vec::new();
    for line in lines {
        assert!(line.starts_with("    "), "linha sem recuo: {line}");
        let body = line.trim();
        if body.starts_with("classDef ") || body.starts_with("class ") {
            assert!(body.ends_with(';'), "{body}");
            continue;
        }
        assert_eq!(body.matches('"').count() % 2, 0, "aspas: {body}");
        // Brackets outside quotes must balance.
        // `id>"texto"]` (asymmetric shape) opens with `>`.
        let mut depth = i32::from(body.contains(">\""));
        let mut inside = false;
        for c in body.chars() {
            match c {
                '"' => inside = !inside,
                '(' | '[' | '{' if !inside => depth += 1,
                ')' | ']' | '}' if !inside => depth -= 1,
                _ => {}
            }
        }
        assert_eq!(depth, 0, "colchetes: {body}");
        let id: String = body
            .chars()
            .take_while(|c| c.is_ascii_alphanumeric())
            .collect();
        assert!(id.starts_with('n') && id.len() > 1, "id: {body}");
        let rest = body[id.len()..].trim_start();
        if rest.starts_with("-->") || rest.starts_with("-.->") {
            assert!(declared.contains(&id), "origem não declarada: {body}");
            let to = body.rsplit(['|', ' ']).next().unwrap();
            assert!(declared.iter().any(|d| d == to), "destino: {body}");
        } else {
            assert!(
                rest.starts_with("([")
                    || rest.starts_with("((")
                    || rest.starts_with(">\"")
                    || rest.starts_with("[\"")
                    || rest.starts_with("(\"")
                    || rest.starts_with("{{"),
                "forma: {body}"
            );
            assert!(!declared.contains(&id), "id repetido: {id}");
            declared.push(id);
        }
    }
}

#[test]
fn every_example_generates_all_outputs() {
    for (name, model) in models() {
        let g = flowchart(&model, &Options::default());
        let steps = model
            .objects
            .iter()
            .filter(|o| !utsim_model::is_mobile(&o.kind))
            .count();
        assert_eq!(g.nodes.len(), steps, "{name}");
        assert!(g.edges.len() <= model.connections.len(), "{name}");
        let svg_text = svg(&g);
        assert!(svg_text.starts_with("<?xml"), "{name}");
        assert!(svg_text.ends_with("</svg>"), "{name}");
        assert_eq!(svg_text.matches("<g data-etapa").count(), steps, "{name}");
        let page = html(&g);
        assert!(page.contains("<h2>Legenda</h2>") && page.contains("Carta de processo"));
        assert!(page.contains(&svg_inline(&g)), "{name}");
        check_mermaid(&mermaid(&g));
        let csv = carta_csv(&g);
        assert!(csv.starts_with("etapa,simbolo,"), "{name}");
        assert!(
            g.width.is_finite() && g.height.is_finite() && g.width > 0.0,
            "{name}"
        );
        for n in &g.nodes {
            assert!(n.x >= 0.0 && n.y >= 0.0 && n.w > 0.0 && n.h > 0.0, "{name}");
        }
        // Two layouts of the same model are the same text.
        let again = flowchart(&model, &Options::default());
        assert_eq!(
            svg_text,
            svg(&again),
            "{name}: disposição não determinística"
        );
    }
}

#[test]
fn horizontal_layout_works_too() {
    for (name, model) in models() {
        let o = Options {
            direction: Direction::Horizontal,
            ..Options::default()
        };
        let g = flowchart(&model, &o);
        assert!(g.width > 0.0 && g.height > 0.0, "{name}");
        check_mermaid(&mermaid(&g));
    }
}

#[test]
fn distribution_center_counts_and_symbols() {
    let g = flowchart(&example("centro_de_distribuicao.ron"), &Options::default());
    assert_eq!(g.nodes.len(), 10);
    assert_eq!(g.edges.len(), 9);
    let sym = |n: &str| g.nodes[g.node_index(n).unwrap()].symbol;
    assert_eq!(sym("chegada_caminhoes"), Symbol::Inicio);
    assert_eq!(sym("descarga"), Symbol::Operacao);
    assert_eq!(sym("recebimento"), Symbol::Espera);
    assert_eq!(sym("esteira_entrada"), Symbol::Transporte);
    assert_eq!(sym("inspecao"), Symbol::Inspecao);
    assert_eq!(sym("porta_paletes"), Symbol::Armazenagem);
    assert_eq!(sym("separacao"), Symbol::Operacao);
    assert_eq!(sym("expedicao"), Symbol::Fim);
    // Resources: the inspection team and the fleet (their members are listed inside).
    let names: Vec<&str> = g.resources.iter().map(|r| r.name.as_str()).collect();
    assert_eq!(names, ["equipe_inspecao", "frota"]);
    assert_eq!(g.resources[1].members.len(), 2);
    // The forklift transport sits on the arrow leaving the inspection.
    let i = g.node_index("inspecao").unwrap();
    let out = g.outgoing(i);
    let t = out[0].transport.as_ref().expect("transporte");
    assert_eq!(t.by, "frota");
    assert!(t.distance.unwrap() >= 40.0);
    // The main path crosses the whole line and has a transport row.
    let c = carta::build(&g);
    assert_eq!(c.rows.first().unwrap().symbol, Symbol::Inicio);
    assert_eq!(c.rows.last().unwrap().symbol, Symbol::Fim);
    assert_eq!(c.rows.len(), 11);
    assert!(
        c.rows
            .iter()
            .any(|r| r.symbol == Symbol::Transporte && r.distancia.is_some())
    );
    assert!(c.totals.tempo > 0.0 && c.totals.distancia >= 40.0);
}

#[test]
fn routing_by_weights_shows_percentages() {
    let g = flowchart(&example("roteamento_por_tipo.ron"), &Options::default());
    assert!(g.nodes.iter().any(|n| n.out_degree > 1));
    assert!(g.edges.iter().any(|e| e.label.is_some()));
    let g = flowchart(&example("atendimento_com_perda.ron"), &Options::default());
    assert!(
        g.nodes
            .iter()
            .any(|n| n.notes.iter().any(|t| t.starts_with("perda")))
    );
}

#[test]
fn override_changes_the_symbol() {
    let m = example("mm1.ron");
    let mut o = Options::default();
    let name = m
        .objects
        .iter()
        .find(|x| !x.name.is_empty())
        .unwrap()
        .name
        .clone();
    o.overrides.insert(name.clone(), Symbol::Inspecao);
    let g = flowchart(&m, &o);
    assert_eq!(
        g.nodes[g.node_index(&name).unwrap()].symbol,
        Symbol::Inspecao
    );
}
