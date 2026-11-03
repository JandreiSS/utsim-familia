//! The ten kinds of diagram: static ones for every example, dynamic ones for a few.

use std::path::PathBuf;

use utsim_diagram::{Options, Symbol, TipoDiagrama, flowchart, gerar, indice, rastro};
use utsim_model::Model;

fn example(name: &str) -> Model {
    Model::load(
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../examples")
            .join(name),
    )
    .unwrap()
}

fn all_examples() -> Vec<(String, Model)> {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../examples");
    let mut v = Vec::new();
    for e in std::fs::read_dir(dir).unwrap() {
        let p = e.unwrap().path();
        let n = p.file_name().unwrap().to_string_lossy().to_string();
        if p.extension().is_none_or(|x| x != "ron")
            || n.starts_with("experimento_")
            || n.starts_with("otimizacao_")
        {
            continue;
        }
        v.push((n, Model::load(&p).unwrap()));
    }
    v
}

fn check_mermaid_loose(m: &str) {
    assert!(
        m.lines()
            .next()
            .is_some_and(|l| l.starts_with("flowchart LR") || l.starts_with("%%"))
    );
    assert!(m.contains("flowchart LR"));
    assert_eq!(
        m.matches("    subgraph ").count(),
        m.matches("    end").count(),
        "subgraph sem end"
    );
    for l in m.lines() {
        assert_eq!(l.matches('"').count() % 2, 0, "aspas: {l}");
    }
}

#[test]
fn ids_round_trip() {
    for t in TipoDiagrama::ALL {
        assert_eq!(TipoDiagrama::from_id(t.id()), Some(t));
        assert!(!t.descricao().is_empty() && !t.nome().is_empty());
    }
    assert_eq!(TipoDiagrama::from_id("De_Para"), Some(TipoDiagrama::DePara));
    assert_eq!(TipoDiagrama::from_id("xis"), None);
    assert_eq!(TipoDiagrama::ALL.iter().filter(|t| t.dinamico()).count(), 5);
}

#[test]
fn static_types_work_for_every_example() {
    let o = Options::default();
    for (name, m) in all_examples() {
        for t in TipoDiagrama::ALL.into_iter().filter(|t| !t.dinamico()) {
            let out = gerar(t, &m, &o).unwrap_or_else(|e| panic!("{name} {}: {e}", t.id()));
            assert!(out.html.starts_with("<!doctype html>"), "{name} {}", t.id());
            assert!(out.html.contains("Origem dos dados"), "{name} {}", t.id());
            if let Some(mm) = &out.mermaid {
                check_mermaid_loose(mm);
            }
            if let Some(s) = &out.svg {
                assert!(
                    s.starts_with("<?xml") && s.ends_with("</svg>"),
                    "{name} {}",
                    t.id()
                );
            }
        }
    }
}

#[test]
fn dynamic_types_work_and_are_deterministic() {
    let o = Options::default();
    for name in [
        "centro_de_distribuicao.ron",
        "tutorial_02_linha.ron",
        "mm1.ron",
        "agvs.ron",
    ] {
        let m = example(name);
        for t in TipoDiagrama::ALL.into_iter().filter(|t| t.dinamico()) {
            let a = gerar(t, &m, &o).unwrap_or_else(|e| panic!("{name} {}: {e}", t.id()));
            let b = gerar(t, &m, &o).unwrap();
            assert_eq!(a.html, b.html, "{name} {} não determinístico", t.id());
            assert!(
                a.html.contains("semente"),
                "{name} {}: origem sem semente",
                t.id()
            );
        }
    }
}

#[test]
fn operation_process_chart_keeps_only_operations_and_inspections() {
    let m = example("centro_de_distribuicao.ron");
    let out = gerar(TipoDiagrama::Operacoes, &m, &Options::default()).unwrap();
    let svg = out.svg.unwrap();
    for n in ["O-1 descarga", "I-1 inspecao", "O-2 separacao"] {
        assert!(svg.contains(n), "{n}");
    }
    assert!(!svg.contains("recebimento") && !svg.contains("porta_paletes"));
    // Start, three steps, end.
    assert_eq!(svg.matches("<g data-etapa").count(), 5);
}

#[test]
fn swimlanes_split_by_resource_and_mark_handovers() {
    let m = example("centro_de_distribuicao.ron");
    let out = gerar(TipoDiagrama::Raias, &m, &Options::default()).unwrap();
    for lane in [
        "Estações",
        "Esteiras",
        "Filas e armazenagem",
        "Frota frota",
        "Equipe equipe_inspecao",
    ] {
        assert!(out.html.contains(lane), "{lane}");
    }
    assert!(out.html.contains("Passagens entre raias"));
    let mm = out.mermaid.unwrap();
    assert!(mm.contains("==>"));
    // Area chosen by the user wins.
    let mut o = Options::default();
    o.areas.insert("descarga".into(), "Doca".into());
    let out = gerar(TipoDiagrama::Raias, &m, &o).unwrap();
    assert!(out.html.contains("Doca"));
}

#[test]
fn sipoc_has_five_columns_from_sources_and_sinks() {
    let m = example("centro_de_distribuicao.ron");
    let out = gerar(TipoDiagrama::Sipoc, &m, &Options::default()).unwrap();
    assert!(out.html.contains("chegada_caminhoes") && out.html.contains("expedicao"));
    let csv = out.csv.unwrap();
    assert!(csv.starts_with("fornecedores,entradas,processo,saidas,clientes"));
    // descarga, inspecao, porta_paletes, separacao.
    assert_eq!(csv.lines().count(), 5);
}

#[test]
fn from_to_matrix_adds_up_to_the_trips() {
    let m = example("centro_de_distribuicao.ron");
    let r = rastro::capturar(&m, 0).unwrap();
    assert!(!r.viagens.is_empty() && !r.rotas.is_empty());
    let out = gerar(TipoDiagrama::DePara, &m, &Options::default()).unwrap();
    let csv = out.csv.unwrap();
    let total: usize = csv
        .lines()
        .skip(1)
        .map(|l| l.split(',').nth(2).unwrap().parse::<usize>().unwrap())
        .sum();
    assert_eq!(total, r.viagens.len());
    // The most used route goes from the source to the sink.
    let route = &r.rotas[0].0;
    assert_eq!(r.nomes[route[0]], "chegada_caminhoes");
    assert_eq!(r.nomes[*route.last().unwrap()], "expedicao");
}

#[test]
fn man_machine_stretches_cover_the_window() {
    let m = example("centro_de_distribuicao.ron");
    let r = rastro::capturar(&m, 0).unwrap();
    let (w0, w1) = (4000.0, 5000.0);
    for states in &r.estados {
        let segs = utsim_diagram::homem_maquina::segmentos(states, w0, w1);
        let sum: f64 = segs.iter().map(|s| s.1 - s.0).sum();
        assert!((sum - (w1 - w0)).abs() < 1e-6);
    }
    let mut o = Options {
        window: Some((4000.0, 5000.0)),
        ..Options::default()
    };
    let out = gerar(TipoDiagrama::HomemMaquina, &m, &o).unwrap();
    assert!(out.html.contains("Ocupação") && out.html.contains("inspecao"));
    o.window = Some((5000.0, 4000.0));
    assert!(gerar(TipoDiagrama::HomemMaquina, &m, &o).is_err());
}

#[test]
fn value_stream_map_numbers() {
    let m = example("centro_de_distribuicao.ron");
    let out = gerar(TipoDiagrama::Vsm, &m, &Options::default()).unwrap();
    // Value-added time = 240 (descarga) + 80 (separacao); inspection does not add value.
    assert!(out.html.contains("Tempo de agregação de valor: 320"));
    assert!(out.html.contains("Índice de agregação de valor"));
    assert!(out.csv.unwrap().contains("inspecao,60,20"));
}

#[test]
fn spaghetti_compares_two_models() {
    let a = example("centro_de_distribuicao.ron");
    let mut b = a.clone();
    // The "after": the forklifts' corridor is shorter.
    for o in &mut b.objects {
        if o.name == "porta_paletes" {
            o.position[0] = 30.0;
        }
    }
    let o = Options {
        compare: Some(Box::new(b)),
        ..Options::default()
    };
    let out = gerar(TipoDiagrama::Espaguete, &a, &o).unwrap();
    assert!(out.html.contains("Antes") && out.html.contains("Depois"));
    let csv = out.csv.unwrap();
    assert!(csv.starts_with("recurso,antes,depois,variacao_pct"));
    assert!(csv.contains("Itens (todos)"));
    let plain = gerar(TipoDiagrama::Espaguete, &a, &Options::default()).unwrap();
    assert!(plain.csv.unwrap().contains("itens,"));
}

#[test]
fn plan_hook_draws_under_the_map() {
    let m = example("centro_de_distribuicao.ron");
    let o = Options {
        plant_background: Some("<rect id=\"planta-cad\" width=\"5\" height=\"5\"/>".into()),
        ..Options::default()
    };
    let out = gerar(TipoDiagrama::Mapofluxograma, &m, &o).unwrap();
    assert!(out.svg.unwrap().contains("planta-cad"));
}

#[test]
fn model_without_positions_falls_back_to_the_block_layout() {
    let m = example("linha_com_bloqueio.ron");
    let out = gerar(TipoDiagrama::Mapofluxograma, &m, &Options::default()).unwrap();
    assert!(out.html.contains("não tem posições"));
}

#[test]
fn index_lists_every_type() {
    let m = example("mm1.ron");
    let items: Vec<_> = TipoDiagrama::ALL
        .iter()
        .map(|t| (*t, Ok(format!("x-{}.html", t.id()))))
        .collect();
    let page = indice(&m, &items);
    for t in TipoDiagrama::ALL {
        assert!(page.contains(&format!("x-{}.html", t.id())));
    }
    let g = flowchart(&m, &Options::default());
    assert!(g.nodes.iter().any(|n| n.symbol == Symbol::Inicio));
}

#[test]
fn items_carried_by_forklifts_follow_the_path_network() {
    let m = example("centro_de_distribuicao.ron");
    let r = rastro::capturar(&m, 0).unwrap();
    let idx = |n: &str| m.objects.iter().position(|o| o.name == n).unwrap();
    let (insp, rack) = (idx("inspecao"), idx("porta_paletes"));
    let trips: Vec<_> = r
        .viagens
        .iter()
        .filter(|v| v.de == insp && v.para == rack)
        .collect();
    assert!(
        !trips.is_empty(),
        "nenhuma viagem inspecao -> porta_paletes"
    );
    for v in &trips {
        // insp (20,0) -> meio (40,0) -> rack (60,0): two corridors of 20.
        assert!((v.dist - 40.0).abs() < 1e-9, "{}", v.dist);
        assert_eq!(v.path.len(), 3);
        assert_eq!(v.path[1], [40.0, 0.0, 0.0]);
    }
    // No trip ends at a forklift: the ride is one trip from the boarding to the exit.
    assert!(r.viagens.iter().all(|v| {
        !r.nomes[v.de].starts_with("empilhadeira") && !r.nomes[v.para].starts_with("empilhadeira")
    }));
    // Loaded legs of the forklifts have the same length as the item's trip.
    assert!(r.pernas.iter().any(|p| (p.dist - 40.0).abs() < 1e-9));
    // The route of the items goes straight from the inspection to the rack.
    assert!(r.rotas[0].0.windows(2).any(|w| w == [insp, rack]));
    // Nothing is drawn as a diagonal towards the garage (40, 20): every stroke of the
    // spaghetti and of the flow-process map is horizontal or vertical.
    for tipo in [TipoDiagrama::Espaguete, TipoDiagrama::Mapofluxograma] {
        let svg = gerar(tipo, &m, &Options::default()).unwrap().svg.unwrap();
        let mut tracos = 0;
        for d in svg.split(" d=\"M").skip(1) {
            let d = d.split('"').next().unwrap();
            if d.contains('C') || d.contains('z') {
                continue;
            }
            let pts: Vec<(f64, f64)> = format!("M{d}")
                .split(['M', 'L'])
                .filter_map(|p| {
                    let mut it = p.split_whitespace().map(|n| n.parse::<f64>().ok());
                    Some((it.next()??, it.next()??))
                })
                .collect();
            for w in pts.windows(2) {
                tracos += 1;
                assert!(
                    (w[0].0 - w[1].0).abs() < 0.2 || (w[0].1 - w[1].1).abs() < 0.2,
                    "diagonal: {d}"
                );
            }
        }
        assert!(tracos > 5);
    }
}

#[test]
fn plan_labels_do_not_overlap() {
    let m = example("centro_de_distribuicao.ron");
    let r = rastro::capturar(&m, 0).unwrap();
    let g = flowchart(&m, &Options::default());
    let caixas = utsim_diagram::caixas_dos_rotulos(&m, &g, &r);
    assert!(caixas.len() >= 10);
    for (i, (na, a)) in caixas.iter().enumerate() {
        for (nb, b) in &caixas[i + 1..] {
            let cruzam = a.0 < b.0 + b.2 && b.0 < a.0 + a.2 && a.1 < b.1 + b.3 && b.1 < a.1 + a.3;
            assert!(!cruzam, "{na} x {nb}");
        }
    }
}

#[test]
fn value_stream_availability_discounts_shifts() {
    let m = example("centro_de_distribuicao.ron");
    let out = gerar(TipoDiagrama::Vsm, &m, &Options::default()).unwrap();
    let csv = out.csv.unwrap();
    let disp = |nome: &str| -> f64 {
        csv.lines()
            .find(|l| l.starts_with(&format!("{nome},")))
            .unwrap()
            .split(',')
            .nth(3)
            .unwrap()
            .parse()
            .unwrap()
    };
    let sep = disp("separacao");
    assert!(sep < 1.0 && sep > 0.9, "{sep}");
    assert!(out.html.contains("fora de turno"));
    // A process with no shifts or breakdowns stays at 100%.
    assert!((disp("inspecao") - 1.0).abs() < 1e-9);
}

#[test]
fn swimlane_label_column_grows_with_the_name() {
    let m = example("centro_de_distribuicao.ron");
    let out = gerar(TipoDiagrama::Raias, &m, &Options::default()).unwrap();
    // The whole name is drawn, not cut with an ellipsis.
    assert!(out.html.contains(">Equipe equipe_inspecao<"));
    assert!(!out.html.contains("Equipe equipe_inspe…"));
}
