//! Networks derived from models (the repository's examples and small inline models) and
//! the critical path observed in the simulation.

use std::path::PathBuf;

use utsim_model::Model;
use utsim_pert::observed::{self, ObservedOptions};
use utsim_pert::pert::{self, Moments};
use utsim_pert::report::{self, Analysis};
use utsim_pert::{
    ActivityKind, DeriveOptions, Mode, PertError, Selection, WeightedBranch, derive,
    derive_scenarios, schedule_means,
};

fn example(name: &str) -> Model {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../examples")
        .join(name);
    Model::load(path).unwrap()
}

fn between(from: &str, to: &str) -> Selection {
    Selection::Between {
        from: from.into(),
        to: to.into(),
    }
}

fn objects(names: &[&str]) -> Selection {
    Selection::Objects(names.iter().map(|s| s.to_string()).collect())
}

fn opts() -> DeriveOptions {
    DeriveOptions::default()
}

#[test]
fn pallet_assembly_between_two_objects() {
    let m = example("montagem_paletes.ron");
    let net = derive(&m, &between("paletes", "saida_caixas"), &opts()).unwrap();
    // The boxes' source and queue do not come from "paletes": out of the stretch.
    assert_eq!(
        net.objects,
        [
            "paletes",
            "fila_paletes",
            "montagem",
            "transporte",
            "desmontagem",
            "saida_caixas"
        ]
    );
    let s = schedule_means(&net);
    // montagem: Uniform(0.2, 0.5) + Triangular(1, 1.5, 2.5) = 0.35 + 5/3; transporte 3;
    // desmontagem 1.
    let expected = 0.35 + 5.0 / 3.0 + 3.0 + 1.0;
    assert!((s.project - expected).abs() < 1e-12, "{}", s.project);
    assert_eq!(s.critical_paths.len(), 1);
}

#[test]
fn pallet_assembly_whole_model_has_and_join_and_split() {
    let m = example("montagem_paletes.ron");
    let all: Vec<&str> = m.objects.iter().map(|o| o.name.as_str()).collect();
    let net = derive(&m, &objects(&all), &opts()).unwrap();
    assert!(net.warnings.is_empty(), "{:?}", net.warnings);
    let (preds, succs) = net.adjacency();
    let comb = net.index("montagem").unwrap();
    let sep = net.index("desmontagem").unwrap();
    assert_eq!(preds[comb].len(), 2, "junção E");
    assert_eq!(succs[sep].len(), 2, "divisão E");
    assert!(
        net.activities[comb]
            .note
            .as_deref()
            .unwrap()
            .contains("junção E")
    );
}

#[test]
fn distribution_centre_gets_a_transport_activity() {
    let m = example("centro_de_distribuicao.ron");
    let net = derive(&m, &between("chegada_caminhoes", "expedicao"), &opts()).unwrap();
    let k = net.index("transporte inspecao→porta_paletes").unwrap();
    let t = &net.activities[k];
    assert_eq!(t.kind, ActivityKind::Transport);
    // Empty trip garagem (40, 20) -> insp (20, 0) and loaded trip insp -> rack (60, 0):
    // 40 m each over the network, 1.5 m/s with 0.5 m/s² ramps: 40/1.5 + 3 s.
    let trip = 40.0 / 1.5 + 3.0;
    assert!(
        (t.mean() - (2.0 * trip + 20.0)).abs() < 1e-9,
        "{}",
        t.mean()
    );
    // The transport sits between inspection and the rack.
    let (i, r) = (
        net.index("inspecao").unwrap(),
        net.index("porta_paletes").unwrap(),
    );
    assert!(net.precedences.contains(&(i, k)) && net.precedences.contains(&(k, r)));
    // The conveyor is its minimum travel time.
    let belt = &net.activities[net.index("esteira_entrada").unwrap()];
    assert_eq!(belt.mean(), 16.0);
    assert!(
        net.assumptions
            .iter()
            .any(|a| a.contains("transporte: viagem vazia"))
    );
    let q = &net.activities[net.index("recebimento").unwrap()];
    assert_eq!(q.mean(), 0.0);
}

#[test]
fn rework_loop_is_cut_with_the_expected_passes() {
    let m = Model::parse(
        r#"Model(name: "retrabalho", run: (stop_time: 1000.0),
            objects: [
                (name: "src", kind: Source(interarrival: Exp(mean: 10.0))),
                (name: "q", kind: Queue()),
                (name: "maq", kind: Processor(process: Const(2.0))),
                (name: "insp", kind: Processor(process: Const(1.0)), output: Weighted([0.8, 0.2])),
                (name: "fim", kind: Sink),
            ],
            connections: [("src", "q"), ("q", "maq"), ("maq", "insp"), ("insp", "fim"), ("insp", "q")])"#,
    )
    .unwrap();
    let net = derive(&m, &between("src", "fim"), &opts()).unwrap();
    let w = net.warnings.join("\n");
    assert!(
        w.contains("ciclo de retrabalho insp → q") && w.contains("1.2500"),
        "{w}"
    );
    let maq = &net.activities[net.index("maq").unwrap()];
    assert_eq!(maq.factor, 1.25);
    assert_eq!(net.activities[net.index("src").unwrap()].factor, 1.0);
    // (2 + 1) × 1.25.
    assert!((schedule_means(&net).project - 3.75).abs() < 1e-12);
}

#[test]
fn or_outputs_keep_one_branch_or_make_scenarios() {
    let text = |rule: &str| {
        format!(
            r#"Model(name: "ou", run: (stop_time: 100.0),
            objects: [
                (name: "src", kind: Source(interarrival: Exp(mean: 10.0))),
                (name: "p", kind: Processor(process: Const(1.0)), output: {rule}),
                (name: "a", kind: Processor(process: Const(5.0))),
                (name: "b", kind: Processor(process: Const(9.0))),
                (name: "fim", kind: Sink),
            ],
            connections: [("src", "p"), ("p", "a"), ("p", "b"), ("a", "fim"), ("b", "fim")])"#
        )
    };
    let m = Model::parse(&text("FirstAvailable")).unwrap();
    let net = derive(&m, &between("src", "fim"), &opts()).unwrap();
    assert!(net.index("a").is_some() && net.index("b").is_none());
    let w = net.warnings.join("\n");
    assert!(w.contains("OU, não E") && w.contains("fora da rede"), "{w}");

    let m = Model::parse(&text("Weighted([0.3, 0.7])")).unwrap();
    let net = derive(&m, &between("src", "fim"), &opts()).unwrap();
    assert!(
        net.index("b").is_some() && net.index("a").is_none(),
        "mais provável"
    );
    let all = derive_scenarios(
        &m,
        &between("src", "fim"),
        &DeriveOptions {
            weighted: WeightedBranch::Scenarios,
        },
    )
    .unwrap();
    assert_eq!(all.len(), 2);
    let p: Vec<f64> = all
        .iter()
        .map(|n| n.scenario.as_ref().unwrap().probability)
        .collect();
    assert!((p[0] - 0.7).abs() < 1e-12 && (p[1] - 0.3).abs() < 1e-12);
    assert_eq!(all[0].scenario.as_ref().unwrap().label, "p→b");
}

#[test]
fn bad_selections_are_reported() {
    let m = example("montagem_paletes.ron");
    assert!(matches!(
        derive(&m, &objects(&["nada"]), &opts()),
        Err(PertError::UnknownObject(_))
    ));
    assert!(matches!(
        derive(&m, &between("saida_caixas", "paletes"), &opts()),
        Err(PertError::NoPath { .. })
    ));
    // Two parts with nothing between them.
    let net = derive(
        &m,
        &objects(&["paletes", "fila_paletes", "caixas"]),
        &opts(),
    )
    .unwrap();
    assert!(
        net.warnings.iter().any(|w| w.contains("desconectadas")),
        "{:?}",
        net.warnings
    );
}

/// Line without queues: classic PERT and the simulation must agree within the interval.
#[test]
fn simulated_mode_matches_classic_without_queues() {
    let m = Model::parse(
        r#"Model(name: "linha", run: (stop_time: 100000.0, seed: 5, replications: 5),
            objects: [
                (name: "src", kind: Source(interarrival: Const(100.0))),
                (name: "a", kind: Processor(process: Uniform(min: 10.0, max: 20.0))),
                (name: "b", kind: Processor(process: Triangular(min: 5.0, mode: 10.0, max: 30.0))),
                (name: "fim", kind: Sink),
            ],
            connections: [("src", "a"), ("a", "b"), ("b", "fim")])"#,
    )
    .unwrap();
    let net = derive(&m, &between("src", "fim"), &opts()).unwrap();
    let classic = pert::analyze(&net, Moments::Exact, None);
    assert!((classic.mean - 30.0).abs() < 1e-12);
    let obs = observed::run(&m, &net, &ObservedOptions::default()).unwrap();
    assert!(obs.units > 4000, "{}", obs.units);
    assert!(
        (obs.total.mean - classic.mean).abs() <= obs.total.half_width,
        "simulado {:?} vs clássico {}",
        obs.total,
        classic.mean
    );
    // No queues: no waiting, the work is the whole time.
    let a = obs.activities.iter().find(|x| x.name == "a").unwrap();
    assert!(a.wait.abs() < 1e-9 && (a.work - 15.0).abs() < 0.5, "{a:?}");
    assert_eq!(a.pooled_criticality, 1.0);
    // Same seed, same result.
    let again = observed::run(&m, &net, &ObservedOptions::default()).unwrap();
    assert_eq!(format!("{:?}", obs.total), format!("{:?}", again.total));
}

#[test]
fn deterministic_line_reproduces_cpm_exactly() {
    let m = Model::parse(
        r#"Model(name: "fixa", run: (stop_time: 1000.0, replications: 2),
            objects: [
                (name: "src", kind: Source(interarrival: Const(50.0))),
                (name: "a", kind: Processor(setup: Some(Const(1.5)), process: Const(4.0))),
                (name: "esteira", kind: Conveyor(length: 6.0, speed: 2.0, item_length: 1.0)),
                (name: "b", kind: Processor(process: Const(7.0))),
                (name: "fim", kind: Sink),
            ],
            connections: [("src", "a"), ("a", "esteira"), ("esteira", "b"), ("b", "fim")])"#,
    )
    .unwrap();
    let net = derive(&m, &between("src", "fim"), &opts()).unwrap();
    let cpm = schedule_means(&net).project;
    assert_eq!(cpm, 1.5 + 4.0 + 3.0 + 7.0);
    let obs = observed::run(&m, &net, &ObservedOptions::default()).unwrap();
    assert!(obs.units >= 30);
    for r in &obs.replications {
        for t in &r.totals {
            assert!((t - cpm).abs() < 1e-9, "{t} vs {cpm}");
        }
    }
}

#[test]
fn slow_component_is_critical_in_every_assembly() {
    let m = Model::parse(
        r#"Model(name: "montagem", run: (stop_time: 2000.0, replications: 3),
            objects: [
                (name: "caixas", kind: Source(interarrival: Const(100.0), first_at_zero: true)),
                (name: "pecas", kind: Source(interarrival: Const(100.0), first_at_zero: true)),
                (name: "rapido", kind: Processor(process: Const(1.0))),
                (name: "lento", kind: Processor(process: Const(5.0))),
                (name: "montagem", kind: Combiner(components: [1], process: Const(1.0))),
                (name: "fim", kind: Sink),
            ],
            connections: [("caixas", "rapido"), ("pecas", "lento"), ("rapido", "montagem"),
                          ("lento", "montagem"), ("montagem", "fim")])"#,
    )
    .unwrap();
    let all: Vec<&str> = m.objects.iter().map(|o| o.name.as_str()).collect();
    let net = derive(&m, &objects(&all), &opts()).unwrap();
    assert_eq!(schedule_means(&net).project, 6.0);
    let obs = observed::run(&m, &net, &ObservedOptions::default()).unwrap();
    assert_eq!(obs.units, 60);
    let crit = |n: &str| {
        obs.activities
            .iter()
            .find(|a| a.name == n)
            .unwrap()
            .pooled_criticality
    };
    assert_eq!(crit("lento"), 1.0);
    assert_eq!(crit("rapido"), 0.0);
    assert_eq!(crit("montagem"), 1.0);
    assert!((obs.total.mean - 6.0).abs() < 1e-9);
    assert_eq!(obs.paths.len(), 1);
    assert_eq!(obs.paths[0].0, ["pecas", "lento", "montagem", "fim"]);
}

#[test]
fn simulated_mode_on_the_examples() {
    let m = example("centro_de_distribuicao.ron");
    let net = derive(&m, &between("chegada_caminhoes", "expedicao"), &opts()).unwrap();
    let o = ObservedOptions {
        replications: Some(2),
        deadline: Some(8000.0),
        ..ObservedOptions::default()
    };
    let obs = observed::run(&m, &net, &o).unwrap();
    assert!(obs.units > 0);
    let t = obs
        .activities
        .iter()
        .find(|a| a.name == "transporte inspecao→porta_paletes")
        .unwrap();
    assert_eq!(
        t.pooled_criticality, 1.0,
        "toda unidade passa pela empilhadeira"
    );
    assert!(t.work > 0.0);
    // Queues and the rack's dwell make the observed time longer than the CPM.
    assert!(obs.total.mean >= schedule_means(&net).project);

    let m = example("montagem_paletes.ron");
    let net = derive(&m, &between("paletes", "saida_caixas"), &opts()).unwrap();
    let obs = observed::run(
        &m,
        &net,
        &ObservedOptions {
            replications: Some(2),
            ..ObservedOptions::default()
        },
    )
    .unwrap();
    assert!(obs.units > 0);
    assert!(obs.total.mean >= 4.0, "{:?}", obs.total);

    // The reports render every mode.
    let classic = pert::analyze(&net, Moments::Exact, Some(10.0));
    let mc = utsim_pert::monte_carlo::run(&net, &Default::default());
    for mode in [Mode::Simulated, Mode::Classic, Mode::MonteCarlo] {
        let a = Analysis {
            title: &m.name,
            selection: "de paletes até saida_caixas".into(),
            mode,
            network: &net,
            classic: &classic,
            monte_carlo: Some(&mc),
            observed: Some(&obs),
        };
        let text = report::to_text(&a);
        assert!(text.contains("caminho crítico"), "{text}");
        let html = report::to_html(&a);
        assert!(html.contains("<svg") && !html.contains("http://") && !html.contains("https://"));
        let csv = report::activities_csv(&a);
        assert_eq!(csv.lines().count(), net.len() + 1);
    }
    let d = report::diagram(&net, &classic.schedule, &mc.criticality);
    let start = &d.boxes[net.start];
    let end = &d.boxes[net.end];
    assert!(start.x < end.x && start.layer == 0);
    assert_eq!(d.arrows.len(), net.precedences.len());
}
