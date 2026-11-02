//! Arrivals by period (`Source.schedule`), the end of a shift with `Finish` and the
//! model's named calendars (`calendars`): statistical checks against `λ·duration` and the
//! mean interarrival of each period, hand-checked timelines, equivalence of calendars and
//! inline shifts, and case study 3 rebuilt with one source.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use utsim_core::stats::Summary;
use utsim_core::{Observer, RunReport, Time, TraceEvent, summarize};
use utsim_model::Model;
use utsim_model::experiment::apply_patches;

fn parse(text: &str) -> Model {
    let m = Model::parse(text).unwrap_or_else(|e| panic!("{e}\n{text}"));
    m.validate().unwrap_or_else(|e| panic!("{e}\n{text}"));
    m
}

/// Records the time of every item created in block `block` and of every move into
/// block `into`.
#[derive(Clone, Default)]
struct Times {
    created: Arc<Mutex<Vec<Time>>>,
    moved: Arc<Mutex<Vec<(Time, usize, usize)>>>,
}

struct Recorder {
    block: usize,
    times: Times,
}

impl Observer for Recorder {
    fn on_event(&mut self, t: Time, event: &TraceEvent) {
        match event {
            TraceEvent::Created { block, .. } if *block == self.block => {
                self.times.created.lock().unwrap().push(t)
            }
            TraceEvent::Moved { from, to, .. } => {
                self.times.moved.lock().unwrap().push((t, *from, *to))
            }
            _ => {}
        }
    }
}

/// Runs replication `r` and returns the report with the creation times of block 0 and
/// every move.
fn observed(model: &Model, r: u32) -> (RunReport, Times) {
    let times = Times::default();
    let mut sim = model.build(r);
    sim.set_observer(Box::new(Recorder {
        block: 0,
        times: times.clone(),
    }));
    let report = sim.run().expect("run");
    (report, times)
}

fn mean_sd(xs: &[f64]) -> (f64, f64) {
    let n = xs.len() as f64;
    let mean = xs.iter().sum::<f64>() / n;
    let var = xs.iter().map(|x| (x - mean).powi(2)).sum::<f64>() / (n - 1.0);
    (mean, var.sqrt())
}

// ---------------------------------------------------------------------------
// Arrivals by period
// ---------------------------------------------------------------------------

/// Rates per minute of 24 hourly periods: low and high hours alternate, so every
/// boundary is a jump.
fn hourly_rates() -> Vec<f64> {
    (0..24)
        .map(|k| 0.05 + 0.85 * ((k * 7) % 24) as f64 / 23.0)
        .collect()
}

#[test]
fn hourly_rates_give_lambda_times_duration_in_every_period_and_near_every_boundary() {
    let rates = hourly_rates();
    let periods: Vec<String> = rates
        .iter()
        .enumerate()
        .map(|(k, r)| {
            format!(
                "(start: {}.0, end: {}.0, arrivals: Rate({r}))",
                60 * k,
                60 * (k + 1)
            )
        })
        .collect();
    const DAYS: f64 = 4.0;
    const REPS: u32 = 100;
    let model = parse(&format!(
        r#"Model(
            name: "taxas por hora",
            run: (stop_time: {stop}, seed: 11),
            objects: [
                (name: "chegada", kind: Source(interarrival: Exp(mean: 1.0), on_blocked: Drop,
                    schedule: (cycle: 1440.0, periods: [{periods}]))),
                (name: "saida", kind: Sink),
            ],
            connections: [("chegada", "saida")],
        )"#,
        stop = DAYS * 1440.0 - 1e-9,
        periods = periods.join(", ")
    ));
    // Per replication: arrivals in each hour, in its first 5 minutes and in its last 5.
    let mut whole = vec![Vec::new(); 24];
    let mut first = vec![Vec::new(); 24];
    let mut last = vec![Vec::new(); 24];
    for r in 0..REPS {
        let (_, times) = observed(&model, r);
        let mut w = [0.0; 24];
        let mut f = [0.0; 24];
        let mut l = [0.0; 24];
        for &t in times.created.lock().unwrap().iter() {
            let phase = t.rem_euclid(1440.0);
            let k = ((phase / 60.0) as usize).min(23);
            let offset = phase - 60.0 * k as f64;
            w[k] += 1.0;
            if offset < 5.0 {
                f[k] += 1.0;
            }
            if offset >= 55.0 {
                l[k] += 1.0;
            }
        }
        for k in 0..24 {
            whole[k].push(w[k]);
            first[k].push(f[k]);
            last[k].push(l[k]);
        }
    }
    // The counts of a Poisson process are Poisson: mean and variance λ·duration. The
    // bound is 4 standard errors (24 x 3 checks).
    let mut worst: f64 = 0.0;
    for k in 0..24 {
        for (what, counts, minutes) in [
            ("hora", &whole[k], 60.0),
            ("primeiros 5 min", &first[k], 5.0),
            ("últimos 5 min", &last[k], 5.0),
        ] {
            let expected = rates[k] * minutes * DAYS;
            let (mean, sd) = mean_sd(counts);
            let z = (mean - expected) / (expected / f64::from(REPS)).sqrt();
            worst = worst.max(z.abs());
            assert!(
                z.abs() < 4.0,
                "período {} ({what}): média {mean:.2}, esperado λ·Δt = {expected:.2} (z = {z:.2})",
                k + 1
            );
            if what == "hora" {
                let dispersion = sd * sd / expected;
                assert!(
                    (0.6..1.5).contains(&dispersion),
                    "período {}: variância/média = {dispersion:.2}, Poisson dá 1",
                    k + 1
                );
            }
        }
    }
    eprintln!("taxas por hora: maior |z| em 72 contagens = {worst:.2}");
}

#[test]
fn distribution_periods_keep_their_mean_and_start_afresh_at_the_boundary() {
    // [0, 500): Uniform(1, 3), mean 2. [500, 1000): Gamma(4, 1.25), mean 5, sd 2.5.
    // The first arrival of a period comes one fresh draw after the boundary: no residual
    // of the previous period's draw carries over.
    let model = parse(
        r#"Model(
            name: "distribuições por período",
            run: (stop_time: 100000.0, seed: 5),
            objects: [
                (name: "chegada", kind: Source(interarrival: Exp(mean: 1.0), on_blocked: Drop,
                    schedule: (cycle: 1000.0, periods: [
                        (start: 0.0, end: 500.0, arrivals: Interarrival(Uniform(min: 1.0, max: 3.0))),
                        (start: 500.0, end: 1000.0, arrivals: Interarrival(Gamma(shape: 4.0, scale: 1.25))),
                    ]))),
                (name: "saida", kind: Sink),
            ],
            connections: [("chegada", "saida")],
        )"#,
    );
    let mut gaps = [Vec::new(), Vec::new()];
    let mut first_gaps = [Vec::new(), Vec::new()];
    for r in 0..20 {
        let (_, times) = observed(&model, r);
        let created = times.created.lock().unwrap();
        let mut prev: Option<(f64, usize, f64)> = None; // (time, period, cycle start)
        for &t in created.iter() {
            let cycle = (t / 1000.0).floor() * 1000.0;
            let k = usize::from(t - cycle >= 500.0);
            let start = cycle + 500.0 * k as f64;
            match prev {
                Some((p, pk, pc)) if pk == k && pc == cycle => gaps[k].push(t - p),
                _ => first_gaps[k].push(t - start),
            }
            prev = Some((t, k, cycle));
        }
    }
    for (k, (mean, sd, name)) in [
        (2.0, (1.0f64 / 3.0).sqrt(), "Uniform(1, 3)"),
        (5.0, 2.5, "Gamma(4; 1,25)"),
    ]
    .into_iter()
    .enumerate()
    {
        let (m_first, _) = mean_sd(&first_gaps[k]);
        let se_first = sd / (first_gaps[k].len() as f64).sqrt();
        assert!(
            (m_first - mean).abs() < 4.0 * se_first,
            "{name}: primeiro intervalo do período com média {m_first:.4}, esperado {mean} (± {:.4})",
            4.0 * se_first
        );
        let (m_all, _) = mean_sd(&gaps[k]);
        let se = sd / (gaps[k].len() as f64).sqrt();
        // The gap that crosses the end of the period is never seen (censored), which
        // shortens the observed mean by about sd²/(2·duration): 0.03% and 1.25% here.
        let censoring = sd * sd / 2.0 / 500.0;
        assert!(
            (m_all - mean).abs() < 4.0 * se + censoring,
            "{name}: intervalo médio {m_all:.4}, esperado {mean} (± {:.4})",
            4.0 * se + censoring
        );
        eprintln!(
            "{name}: média do 1.º intervalo {m_first:.4} (n = {}), média dos intervalos {m_all:.4} (n = {}), esperado {mean}",
            first_gaps[k].len(),
            gaps[k].len()
        );
    }
}

/// A source with `Const(3)` by default and a 20-unit cycle; `second` is the process of
/// [10, 20). Returns the arrival times before `stop`.
fn const_source_times(second: &str, stop: f64) -> Vec<f64> {
    let model = parse(&format!(
        r#"Model(
            name: "regra da troca",
            run: (stop_time: {stop}),
            objects: [
                (name: "chegada", kind: Source(interarrival: Const(3.0),
                    schedule: (cycle: 20.0, periods: [
                        (start: 0.0, end: 10.0),
                        (start: 10.0, end: 20.0{second}),
                    ]))),
                (name: "saida", kind: Sink),
            ],
            connections: [("chegada", "saida")],
        )"#
    ));
    let (_, times) = observed(&model, 0);
    times.created.lock().unwrap().clone()
}

#[test]
fn default_mode_uses_the_source_interarrival_and_the_boundary_rule_is_exact() {
    // "Igual ao padrão" on both sides is the same process: the pending arrival is kept
    // (3, 6, 9, 12, ...: the same as without a schedule).
    let kept: Vec<f64> = (1..=13).map(|k| 3.0 * k as f64).collect();
    assert_eq!(const_source_times("", 40.0), kept);
    // An equal distribution written out is the same process too.
    assert_eq!(
        const_source_times(", arrivals: Interarrival(Const(3.0))", 40.0),
        kept
    );
    // A different process starts afresh at each boundary: 3, 6, 9 | 14, 18 | 23, 26, 29 |
    // 34, 38. The arrival due at 12 is dropped at 10 and the one due at 22 at 20.
    assert_eq!(
        const_source_times(", arrivals: Interarrival(Const(4.0))", 40.0),
        vec![3.0, 6.0, 9.0, 14.0, 18.0, 23.0, 26.0, 29.0, 34.0, 38.0]
    );
    // A rate of 1/4 has the same expected count as Const(4), and a closed period none.
    assert_eq!(
        const_source_times(", arrivals: Closed", 40.0),
        vec![3.0, 6.0, 9.0, 23.0, 26.0, 29.0]
    );

    // Default mode draws exactly what the interarrival written in the period would.
    let model = |first: &str| {
        parse(&format!(
            r#"Model(
                name: "padrão",
                run: (stop_time: 20000.0, seed: 3, replications: 4),
                objects: [
                    (name: "chegada", kind: Source(interarrival: Exp(mean: 3.0),
                        schedule: (cycle: 20.0, periods: [(start: 0.0, end: 10.0{first})]))),
                    (name: "saida", kind: Sink),
                ],
                connections: [("chegada", "saida")],
            )"#
        ))
    };
    let default = model("").run_replications().unwrap();
    let written = model(", arrivals: Interarrival(Exp(mean: 3.0))")
        .run_replications()
        .unwrap();
    let metrics = |rs: &[RunReport]| rs.iter().map(RunReport::metrics).collect::<Vec<_>>();
    assert_eq!(metrics(&default), metrics(&written));
    // Half of the cycle is open: 20000 / 2 / 3 arrivals, and the source shows the other
    // half as "fora do horário" (off_shift).
    let s = summarize(&default);
    let n = s["saida.input"].mean;
    assert!(
        (n - 20000.0 / 6.0).abs() < 4.0 * (20000.0f64 / 6.0 / 4.0).sqrt(),
        "{n}"
    );
    let off = s["chegada.state.off_shift"].mean;
    assert!((off - 0.5).abs() < 1e-9, "fora do horário: {off}");
}

#[test]
fn labels_follow_the_period_in_force_when_the_item_is_created() {
    // Arrivals every 10 min. Night [0, 420): "classe" 1 and "turno" 1; day: the default
    // "classe" 2 and "turno" 2 (a label the source does not have by default).
    // Arrivals at 10, 20, ..., 2880 (288). Night: 10..410 (41), 1440..1850 (42), 2880 (1):
    // 84. Day: 204. The arrival due at 420 comes after the boundary: day.
    let model = parse(
        r#"Model(
            name: "rótulos por período",
            run: (stop_time: 2880.0),
            objects: [
                (name: "chegada", output: ByLabel("classe"), kind: Source(interarrival: Const(10.0),
                    labels: { "classe": Const(2.0) },
                    schedule: (cycle: 1440.0, periods: [
                        (start: 0.0, end: 420.0, labels: { "classe": Const(1.0), "turno": Const(1.0) }),
                        (start: 420.0, end: 1440.0, labels: { "turno": Const(2.0) }),
                    ]))),
                (name: "q", output: ByLabel("turno"), kind: Queue()),
                (name: "urgentes", kind: Sink),
                (name: "noite", kind: Sink),
                (name: "dia", kind: Sink),
            ],
            connections: [("chegada", "urgentes"), ("chegada", "q"), ("q", "noite"), ("q", "dia")],
        )"#,
    );
    let r = model.run_replications().unwrap().remove(0);
    assert_eq!(r.block("urgentes").unwrap().input, 84);
    assert_eq!(r.block("q").unwrap().input, 204);
    assert_eq!(r.block("noite").unwrap().input, 0);
    assert_eq!(r.block("dia").unwrap().input, 204);
}

#[test]
fn schedule_errors_are_clear() {
    let base = |schedule: &str| {
        Model::parse(&format!(
            r#"Model(name: "x", run: (stop_time: 10.0), objects: [
                (name: "c", kind: Source(interarrival: Exp(mean: 1.0), schedule: {schedule})),
                (name: "s", kind: Sink)], connections: [("c", "s")])"#
        ))
        .unwrap()
        .validate()
        .unwrap_err()
        .to_string()
    };
    let e = base("(cycle: 100.0, periods: [(start: 0.0, end: 50.0), (start: 40.0, end: 60.0)])");
    assert!(
        e.contains("c: schedule[2]: começa em 40 antes do fim do período anterior (50)"),
        "{e}"
    );
    let e = base("(cycle: 100.0, periods: [(start: 90.0, end: 120.0)])");
    assert!(
        e.contains("schedule[1]: o período [90, 120) precisa estar dentro do ciclo [0, 100]"),
        "{e}"
    );
    let e = base("(cycle: 100.0, periods: [(start: 10.0, end: 10.0)])");
    assert!(
        e.contains("schedule[1]: o fim (10) precisa ser maior que o início (10)"),
        "{e}"
    );
    let e = base("(cycle: 100.0, periods: [(start: 0.0, end: 10.0, arrivals: Rate(0.0))])");
    assert!(
        e.contains("schedule[1]: a taxa deve ser finita e > 0"),
        "{e}"
    );
    let e = base("(cycle: 0.0, periods: [])");
    assert!(e.contains("schedule.cycle deve ser finito e > 0"), "{e}");
    let e = base("(cycle: 10.0, periods: [])");
    assert!(e.contains("schedule precisa de ao menos um período"), "{e}");
}

#[test]
fn experiments_patch_the_periods() {
    let base = parse(
        r#"Model(name: "x", run: (stop_time: 1000.0), objects: [
            (name: "chegada", kind: Source(interarrival: Exp(mean: 1.0), schedule: (cycle: 100.0, periods: [
                (start: 0.0, end: 50.0, arrivals: Rate(0.5)), (start: 50.0, end: 100.0)]))),
            (name: "s", kind: Sink)], connections: [("chegada", "s")])"#,
    );
    let set = |pairs: &[(&str, &str)]| -> Result<Model, String> {
        let set: BTreeMap<String, String> = pairs
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect();
        apply_patches(&base, &set).map_err(|e| e.to_string())
    };
    let m = set(&[
        ("chegada.schedule[1].rate", "0.75"),
        ("chegada.schedule[2].interarrival", "Const(2.0)"),
        ("chegada.schedule.cycle", "200.0"),
        ("chegada.schedule[2].end", "150.0"),
    ])
    .unwrap();
    let utsim_model::ObjectKind::Source {
        schedule: Some(s), ..
    } = &m.objects[0].kind
    else {
        panic!("source with schedule");
    };
    assert_eq!(s.cycle, 200.0);
    assert_eq!(
        s.periods[0].arrivals,
        utsim_model::PeriodArrivals::Rate(0.75)
    );
    assert_eq!(
        s.periods[1].arrivals,
        utsim_model::PeriodArrivals::Interarrival(utsim_core::Dist::Const(2.0))
    );
    assert_eq!(s.periods[1].end, 150.0);
    let e = set(&[("chegada.schedule[3].rate", "1.0")]).unwrap_err();
    assert!(
        e.contains("período 3 não existe; a tabela tem 2 período(s)"),
        "{e}"
    );
    let e = set(&[("chegada.schedule[0].rate", "1.0")]).unwrap_err();
    assert!(e.contains("período 0 não existe"), "{e}");
    let e = set(&[("chegada.schedule[1].rate", "-1.0")]).unwrap_err();
    assert!(e.contains("a taxa deve ser finita e > 0"), "{e}");
}

// ---------------------------------------------------------------------------
// End of shift: Pause or Finish
// ---------------------------------------------------------------------------

/// Always-busy machine with `Const(4)` and a 10-unit cycle off in [6, 9).
fn machine(on_end: &str, stop: f64) -> Model {
    parse(&format!(
        r#"Model(
            name: "fim de turno",
            run: (stop_time: {stop}),
            objects: [
                (name: "src", kind: Source(interarrival: Const(1.0), first_at_zero: true)),
                (name: "q", kind: Queue()),
                (name: "p", kind: Processor(process: Const(4.0)),
                    shifts: (period: 10.0, off: [(start: 6.0, duration: 3.0)]{on_end})),
                (name: "out", kind: Sink),
            ],
            connections: [("src", "q"), ("q", "p"), ("p", "out")],
        )"#
    ))
}

fn exits(model: &Model, from: usize) -> Vec<f64> {
    let (_, times) = observed(model, 0);
    times
        .moved
        .lock()
        .unwrap()
        .iter()
        .filter(|(_, f, _)| *f == from)
        .map(|(t, ..)| *t)
        .collect()
}

#[test]
fn finish_completes_the_job_straddling_the_shift_end_and_pause_freezes_it() {
    // Pause: A 0-4. B starts at 4, runs 4-6 (2 done), frozen 6-9, runs 9-11. C 11-15.
    //   D starts at 15, runs 15-16, frozen 16-19, still running at 20. Exits: 4, 11, 15.
    //   Processing [0,6] + [9,16] + [19,20] = 14; off shift 6.
    // Finish: A 0-4. B 4-8 (overtime 6-8, no new item accepted), off shift 8-9. C enters
    //   at 9, the usual start of the shift: 9-13. D 13-17 (overtime 16-17), off 17-19.
    //   E 19-23. Exits: 4, 8, 13, 17. Processing 17, off shift 3.
    let pause = machine("", 20.0);
    let finish = machine(", on_end: Finish", 20.0);
    assert_eq!(exits(&pause, 2), vec![4.0, 11.0, 15.0]);
    assert_eq!(exits(&finish, 2), vec![4.0, 8.0, 13.0, 17.0]);
    // Portuguese spelling, same result.
    let terminar = machine(", ao_fim_do_turno: Terminar", 20.0);
    assert_eq!(exits(&terminar, 2), vec![4.0, 8.0, 13.0, 17.0]);

    let state = |m: &Model, s: &str| m.run_replications().unwrap()[0].block("p").unwrap().states[s];
    assert!((state(&pause, "processing") - 0.7).abs() < 1e-9);
    assert!((state(&pause, "off_shift") - 0.3).abs() < 1e-9);
    assert!((state(&finish, "processing") - 0.85).abs() < 1e-9);
    assert!((state(&finish, "off_shift") - 0.15).abs() < 1e-9);
}

#[test]
fn finish_raises_throughput_without_delaying_the_next_shift() {
    // 7 of every 10 units on shift. Pause: the machine works exactly the shift time,
    // 7 / 4 = 1.75 items per cycle; by 9995 it has 999 * 7 + 5 = 6998 units of work,
    // floor(6998 / 4) = 1749 items. Finish: A and B as in the timeline above, then every
    // cycle k an item enters at 9 + 10k and another at 13 + 10k (the second one ends at
    // 17 + 10k, in overtime), 2 per cycle: 2 + 999 + 998 = 1999 by 9995.
    let out = |m: Model| m.run_replications().unwrap()[0].block("p").unwrap().output;
    let pause = out(machine("", 9995.0));
    let finish = out(machine(", on_end: Finish", 9995.0));
    assert_eq!(pause, 1749);
    assert_eq!(finish, 1999);
    eprintln!("vazão em 9995: Pausar {pause}, Terminar {finish}");
    // Every shift starts at 9 + 10k: an item enters p at each shift start.
    let finish = machine(", on_end: Finish", 100.0);
    let (_, times) = observed(&finish, 0);
    let entries: Vec<f64> = times
        .moved
        .lock()
        .unwrap()
        .iter()
        .filter(|(t, f, to)| *f == 1 && *to == 2 && (t - 9.0).rem_euclid(10.0) < 1e-9)
        .map(|(t, ..)| *t)
        .collect();
    assert_eq!(entries.len(), 10, "{entries:?}");
}

#[test]
fn an_idle_object_with_finish_goes_off_shift_at_once() {
    // Arrivals every 10 at 0, 10, ...; process 2. At 5 (window [5, 8)) the machine is
    // idle: off shift at once, as with Pause. Same output and states.
    let model = |on_end: &str| {
        parse(&format!(
            r#"Model(name: "ociosa", run: (stop_time: 100.0), objects: [
                (name: "src", kind: Source(interarrival: Const(10.0), first_at_zero: true)),
                (name: "p", kind: Processor(process: Const(2.0)),
                    shifts: (period: 10.0, off: [(start: 5.0, duration: 3.0)]{on_end})),
                (name: "out", kind: Sink)],
                connections: [("src", "p"), ("p", "out")])"#
        ))
        .run_replications()
        .unwrap()
        .remove(0)
        .metrics()
    };
    assert_eq!(model(""), model(", on_end: Finish"));
}

/// One transporter carrying from `q` (x = 0) to `fim` (x = 10) at speed 1, off shift
/// in [15, 40) of every 100.
fn carrier(on_end: &str) -> Model {
    parse(&format!(
        r#"Model(name: "carro", run: (stop_time: 100.0), objects: [
            (name: "src", kind: Source(interarrival: Const(1.0), first_at_zero: true)),
            (name: "q", transport: "carro", kind: Queue(), position: (0.0, 0.0, 0.0)),
            (name: "fim", kind: Sink, position: (10.0, 0.0, 0.0)),
            (name: "carro", kind: Transporter(speed: 1.0), position: (0.0, 0.0, 0.0),
                shifts: (period: 100.0, off: [(start: 15.0, duration: 25.0)]{on_end}))],
            connections: [("src", "q"), ("q", "fim")])"#
    ))
}

#[test]
fn a_transporter_with_finish_completes_its_trip_before_leaving() {
    // Trip 1: load at 0, deliver at 10. Trip 2 drives back (10-20) and is underway at
    // 15. Pause: frozen at x = 5 until 40, back at q at 45, delivers at 55, then 75, 95.
    // Finish: back at q at 20, delivers at 30, off shift 30-40, then 60, 80, 100.
    let deliveries = |m: &Model| -> Vec<f64> {
        let (_, times) = observed(m, 0);
        times
            .moved
            .lock()
            .unwrap()
            .iter()
            .filter(|(_, from, to)| *from == 3 && *to == 2)
            .map(|(t, ..)| *t)
            .collect()
    };
    assert_eq!(deliveries(&carrier("")), vec![10.0, 55.0, 75.0, 95.0]);
    assert_eq!(
        deliveries(&carrier(", on_end: Finish")),
        vec![10.0, 30.0, 60.0, 80.0, 100.0]
    );
}

#[test]
fn a_team_member_with_finish_leaves_the_work_to_the_others() {
    // Team of two; "a" is off in [15, 40) with Finish, "b" is always on shift. Once "a"
    // has finished its trip it takes no new task until the shift comes back, so every
    // delivery in [31, 40) is made by "b".
    let model = parse(
        r#"Model(name: "equipe", run: (stop_time: 100.0), objects: [
            (name: "src", kind: Source(interarrival: Const(4.0), first_at_zero: true)),
            (name: "q", transport: "equipe", kind: Queue(), position: (0.0, 0.0, 0.0)),
            (name: "fim", kind: Sink, position: (10.0, 0.0, 0.0)),
            (name: "a", kind: Transporter(speed: 2.0, home: true), position: (0.0, 0.0, 0.0),
                shifts: (period: 100.0, off: [(start: 15.0, duration: 25.0)], on_end: Finish)),
            (name: "b", kind: Transporter(speed: 2.0, home: true), position: (0.0, 0.0, 0.0)),
            (name: "equipe", kind: Dispatcher(team: ["a", "b"]))],
            connections: [("src", "q"), ("q", "fim")])"#,
    );
    let (report, times) = observed(&model, 0);
    let moved = times.moved.lock().unwrap();
    let by = |who: usize, lo: f64, hi: f64| {
        moved
            .iter()
            .filter(|(t, from, to)| *from == who && *to == 2 && (lo..hi).contains(t))
            .count()
    };
    assert!(by(3, 0.0, 15.0) > 0, "a trabalha antes do fim do turno");
    assert_eq!(by(3, 31.0, 40.0), 0, "a não pega tarefa fora do turno");
    assert!(by(4, 31.0, 40.0) > 0, "b atende enquanto a está fora");
    assert!(
        by(3, 40.0, 100.0) > 0,
        "a volta a trabalhar depois do turno"
    );
    assert!(report.block("a").unwrap().states["off_shift"] > 0.0);
}

// ---------------------------------------------------------------------------
// Calendars
// ---------------------------------------------------------------------------

#[test]
fn calendars_give_exactly_the_results_of_inline_shifts() {
    let objects = |shifts_a: &str, shifts_b: &str| {
        format!(
            r#"objects: [
                (name: "src", kind: Source(interarrival: Exp(mean: 2.0))),
                (name: "q", kind: Queue()),
                (name: "a", kind: Processor(process: Exp(mean: 3.0)), shifts: {shifts_a}),
                (name: "b", kind: Processor(process: Exp(mean: 3.0)), shifts: {shifts_b},
                    downtime: (mtbf: Exp(mean: 200.0), mttr: Exp(mean: 10.0))),
                (name: "out", kind: Sink),
            ],
            connections: [("src", "q"), ("q", "a"), ("q", "b"), ("a", "out"), ("b", "out")],"#
        )
    };
    let day =
        "(period: 1440.0, off: [(start: 300.0, duration: 60.0), (start: 900.0, duration: 540.0)])";
    let night = "(period: 1440.0, off: [(start: 0.0, duration: 900.0)], on_end: Finish)";
    let inline = parse(&format!(
        r#"Model(name: "turnos", run: (stop_time: 14400.0, warmup: 1440.0, seed: 9, replications: 3),
            {})"#,
        objects(day, night)
    ));
    let with_calendars = parse(&format!(
        r#"Model(name: "turnos", run: (stop_time: 14400.0, warmup: 1440.0, seed: 9, replications: 3),
            calendars: {{ "dia": {day}, "noite": {} }},
            {})"#,
        night.replace(", on_end: Finish", ""),
        objects("Calendar(\"dia\")", "Calendar(\"noite\", Finish)")
    ));
    let metrics = |m: &Model| -> Vec<BTreeMap<String, f64>> {
        m.run_replications()
            .unwrap()
            .iter()
            .map(RunReport::metrics)
            .collect()
    };
    assert_eq!(metrics(&inline), metrics(&with_calendars));

    // A missing calendar is a clear error.
    let e = Model::parse(&format!(
        r#"Model(name: "x", run: (stop_time: 10.0), calendars: {{ "dia": {day} }}, {})"#,
        objects("Calendario(\"diaa\")", "None")
    ))
    .unwrap()
    .validate()
    .unwrap_err()
    .to_string();
    assert!(
        e.contains("a: o calendário 'diaa' não existe (calendários: dia)"),
        "{e}"
    );
}

// ---------------------------------------------------------------------------
// Case study 3 with one source
// ---------------------------------------------------------------------------

fn caso3(file: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../docs/estudos_de_caso/03-clinica-com-turnos/gabarito")
        .join(file)
}

fn rodar(file: &str, patches: &[(&str, &str)]) -> BTreeMap<String, Summary> {
    let mut model = Model::load(caso3(file)).unwrap_or_else(|e| panic!("{file}: {e}"));
    model.run.replications = 12;
    model.run.stop_time = 11.0 * 1440.0;
    model.run.warmup = 1440.0;
    let set: BTreeMap<String, String> = patches
        .iter()
        .map(|(k, v)| (k.to_string(), v.to_string()))
        .collect();
    let model = apply_patches(&model, &set).unwrap_or_else(|e| panic!("{file}: {e}"));
    summarize(&model.run_replications().unwrap())
}

/// Means of two independent runs agree within 4 combined standard errors.
fn consistent(what: &str, a: &Summary, b: &Summary) {
    let se = (a.std_dev.powi(2) / a.n as f64 + b.std_dev.powi(2) / b.n as f64).sqrt();
    let z = (a.mean - b.mean) / se.max(1e-12);
    eprintln!(
        "{what}: seis fontes {:.3} ± {:.3}, uma fonte {:.3} ± {:.3} (z = {z:.2})",
        a.mean, a.half_width, b.mean, b.half_width
    );
    assert!(
        z.abs() < 4.0,
        "{what}: {} contra {} (z = {z:.2})",
        a.mean,
        b.mean
    );
}

#[test]
fn case_3_with_one_source_per_stream_matches_the_six_source_model() {
    // Same end-of-shift rule as modelo.ron (Pause) for a like-for-like comparison.
    let pause = [
        ("calendars.S1.on_end", "Pause"),
        ("calendars.S2.on_end", "Pause"),
        ("calendars.S3.on_end", "Pause"),
    ];
    let seis = rodar("modelo.ron", &[]);
    let uma = rodar("modelo_por_periodo.ron", &pause);
    let soma = |s: &BTreeMap<String, Summary>, prefix: &str| -> f64 {
        s.iter()
            .filter(|(k, _)| k.starts_with(prefix) && k.ends_with(".output"))
            .map(|(_, v)| v.mean)
            .sum()
    };
    // Arrivals per day: 220 in the table; 6 sources against 1.
    let por_dia_seis = soma(&seis, "chegada_B") / 10.0;
    let por_dia_uma = uma["chegada.output"].mean / 10.0;
    eprintln!("pacientes por dia: seis fontes {por_dia_seis:.1}, uma fonte {por_dia_uma:.1}");
    assert!((200.0..240.0).contains(&por_dia_uma), "{por_dia_uma}");
    assert!((por_dia_seis - por_dia_uma).abs() < 8.0);
    for key in [
        "saida.input",
        "fila_urgente.input",
        "fila_normal.input",
        "sala_espera.staytime.mean",
        "fila_urgente.staytime.mean",
        "fila_normal.staytime.mean",
        "medicos_S1.state.processing",
        "medicos_S2.state.processing",
        "medicos_S3.state.processing",
        "saida.time_in_system.mean",
    ] {
        consistent(key, &seis[key], &uma[key]);
    }
    // One source, outside 07h-19h "fora do horário": 12 of 24 hours.
    let off = uma["chegada.state.off_shift"].mean;
    assert!((off - 0.5).abs() < 1e-9, "{off}");

    // With Finish (the file as it is): the consultation in progress at the change of
    // shift ends in overtime instead of waiting for the next morning, so the time in
    // the system falls a lot and the waits do not grow.
    let terminar = rodar("modelo_por_periodo.ron", &[]);
    let t_pause = uma["saida.time_in_system.mean"].mean;
    let t_finish = terminar["saida.time_in_system.mean"].mean;
    eprintln!("tempo no sistema: Pausar {t_pause:.1} min, Terminar {t_finish:.1} min");
    assert!(
        t_finish < 0.6 * t_pause,
        "Terminar {t_finish} contra Pausar {t_pause}"
    );
    assert!(
        terminar["fila_normal.staytime.mean"].mean
            < uma["fila_normal.staytime.mean"].mean
                + 2.0 * uma["fila_normal.staytime.mean"].half_width
    );
}
