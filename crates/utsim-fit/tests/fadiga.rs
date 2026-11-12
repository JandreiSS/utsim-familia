//! Fatigue curve from a time study (`utsim_fit::fatigue`): known values, exact recovery
//! of the parameters from three hours, recovery from noisy synthetic data, the recovery
//! rate across a pause and the distribution of the rested times.

use utsim_core::rng::stream;
use utsim_core::{Dist, Fatigue, Person, Reading};
use utsim_fit::auto::AutoConfig;
use utsim_fit::fatigue::{DEFAULT_RECOVERY_PER_HOUR, fit_curve, fit_person, level_at};

fn reading(hour: f64, times: Vec<f64>) -> Reading {
    Reading {
        task: "torno".into(),
        method: String::new(),
        hour,
        times,
    }
}

fn close(what: &str, got: f64, want: f64, rel: f64) {
    assert!(
        (got - want).abs() <= rel * want.abs(),
        "{what}: obtido {got}, esperado {want} (±{:.0} %)",
        rel * 100.0
    );
}

/// Mean time at `hour` of a person with these parameters.
fn curve(t0: f64, beta: f64, work: f64, recovery: f64, pauses: &[(f64, f64)], hour: f64) -> f64 {
    t0 * (1.0 + beta * level_at(hour, work, recovery, pauses))
}

#[test]
fn the_level_follows_the_closed_formulas() {
    let e = |x: f64| x.exp();
    assert!((level_at(1.0, 1.0, 2.0, &[]) - (1.0 - e(-1.0))).abs() < 1e-12);
    let pauses = [(1.0, 1.5)];
    let after_pause = (1.0 - e(-1.0)) * e(-1.0);
    assert!((level_at(1.5, 1.0, 2.0, &pauses) - after_pause).abs() < 1e-12);
    let later = 1.0 - (1.0 - after_pause) * e(-0.5);
    assert!((level_at(2.0, 1.0, 2.0, &pauses) - later).abs() < 1e-12);
    // In the middle of the pause, and a pause that has not started yet.
    assert!((level_at(1.25, 1.0, 2.0, &pauses) - (1.0 - e(-1.0)) * e(-0.5)).abs() < 1e-12);
    assert!((level_at(0.5, 1.0, 2.0, &pauses) - (1.0 - e(-0.5))).abs() < 1e-12);
    assert_eq!(level_at(0.0, 1.0, 2.0, &pauses), 0.0);
}

#[test]
fn three_hours_give_the_curve_back_exactly() {
    let (t0, beta, work) = (40.0, 0.25, 0.3);
    let hours = [0.5, 4.0, 7.5];
    let rs: Vec<Reading> = hours
        .iter()
        .map(|h| reading(*h, vec![curve(t0, beta, work, 1.0, &[], *h)]))
        .collect();
    let refs: Vec<&Reading> = rs.iter().collect();
    let f = fit_curve(&refs, &[], 0.95).unwrap();
    close("t0", f.t0, t0, 1e-4);
    close("beta", f.beta, beta, 1e-3);
    close("lambda", f.work_per_hour, work, 1e-3);
    assert!(f.rms < 1e-6, "{}", f.rms);
    assert!(f.intervals.is_none());
    assert!(!f.recovery_estimated);
    assert_eq!(f.recovery_per_hour, DEFAULT_RECOVERY_PER_HOUR);
    assert!(
        f.warnings.iter().any(|w| w.contains("só 3 horários")),
        "{:?}",
        f.warnings
    );
    // In the kernel's units: per second.
    match f.fatigue(3600.0) {
        Fatigue::Dynamic {
            work: w,
            recovery,
            time_factor,
            ..
        } => {
            close("work/s", w, work / 3600.0, 1e-3);
            close("recovery/s", recovery, 1.0e-3, 1e-9);
            close("time_factor", time_factor, beta, 1e-3);
        }
        other => panic!("{other:?}"),
    }
}

#[test]
fn noisy_readings_give_the_parameters_back() {
    // Eight hours, 30 readings each, ±3 % uniform noise around the curve.
    let (t0, beta, work) = (60.0, 0.3, 0.4);
    let mut rng = stream(42, 0, 0);
    let noise = Dist::Uniform {
        min: 0.97,
        max: 1.03,
    };
    let rs: Vec<Reading> = (0..8)
        .map(|i| {
            let h = 0.5 + i as f64;
            let m = curve(t0, beta, work, 1.0, &[], h);
            reading(h, (0..30).map(|_| m * noise.sample(&mut rng)).collect())
        })
        .collect();
    let refs: Vec<&Reading> = rs.iter().collect();
    let f = fit_curve(&refs, &[], 0.95).unwrap();
    close("t0", f.t0, t0, 0.02);
    close("beta", f.beta, beta, 0.2);
    close("lambda", f.work_per_hour, work, 0.35);
    assert_eq!(f.n, 240);
    let ci = f.intervals.as_ref().expect("intervals with 8 hours");
    assert!(ci.t0.0 < t0 && t0 < ci.t0.1, "{:?}", ci.t0);
    assert!(f.warnings.is_empty(), "{:?}", f.warnings);
}

#[test]
fn the_recovery_rate_comes_from_readings_around_a_pause() {
    let (t0, beta, work, recovery) = (30.0, 0.4, 0.5, 3.0);
    let pauses = [(4.0, 4.5)];
    let rs: Vec<Reading> = [0.5, 2.0, 3.5, 4.6, 6.0, 7.5]
        .iter()
        .map(|h| reading(*h, vec![curve(t0, beta, work, recovery, &pauses, *h)]))
        .collect();
    let refs: Vec<&Reading> = rs.iter().collect();
    let f = fit_curve(&refs, &pauses, 0.95).unwrap();
    assert!(f.recovery_estimated);
    close("t0", f.t0, t0, 1e-3);
    close("beta", f.beta, beta, 0.01);
    close("lambda", f.work_per_hour, work, 0.01);
    close("mu", f.recovery_per_hour, recovery, 0.05);

    // Without readings after the pause, μ keeps the default and the report says so.
    let before: Vec<&Reading> = refs.iter().copied().filter(|r| r.hour < 4.0).collect();
    let f = fit_curve(&before, &pauses, 0.95).unwrap();
    assert!(!f.recovery_estimated);
    assert!(
        f.warnings.iter().any(|w| w.contains("recuperação")),
        "{:?}",
        f.warnings
    );
}

#[test]
fn bad_or_too_few_readings_are_explained() {
    let two = [reading(0.5, vec![40.0]), reading(7.5, vec![48.0])];
    let refs: Vec<&Reading> = two.iter().collect();
    let e = fit_curve(&refs, &[], 0.95).unwrap_err();
    assert!(e.contains("3 horários"), "{e}");
    let bad = [
        reading(0.5, vec![40.0]),
        reading(4.0, vec![-1.0]),
        reading(7.5, vec![48.0]),
    ];
    let refs: Vec<&Reading> = bad.iter().collect();
    assert!(
        fit_curve(&refs, &[], 0.95)
            .unwrap_err()
            .contains("inválido")
    );
    // Times that fall through the shift: learning, not fatigue.
    let falling: Vec<Reading> = [(0.5, 50.0), (4.0, 45.0), (7.5, 43.0)]
        .iter()
        .map(|(h, t)| reading(*h, vec![*t]))
        .collect();
    let refs: Vec<&Reading> = falling.iter().collect();
    let f = fit_curve(&refs, &[], 0.95).unwrap();
    assert!(
        f.warnings.iter().any(|w| w.contains("aprendizado")),
        "{:?}",
        f.warnings
    );
    match f.fatigue(60.0) {
        Fatigue::Dynamic { time_factor, .. } => assert_eq!(time_factor, 0.0),
        other => panic!("{other:?}"),
    }
}

#[test]
fn the_rested_times_are_fitted_from_the_study_in_the_model() {
    // The study as it is written in the .ron, in Portuguese.
    let (t0, beta, work) = (40.0, 0.25, 0.3);
    let mut rng = stream(7, 0, 0);
    let base = Dist::Normal { mean: t0, sd: 2.0 };
    let block = |h: f64, rng: &mut _| {
        let k = 1.0 + beta * level_at(h, work, DEFAULT_RECOVERY_PER_HOUR, &[]);
        let ts: Vec<String> = (0..40)
            .map(|_| format!("{:.3}", base.sample(rng) * k))
            .collect();
        format!(
            "(tarefa: \"torno\", metodo: \"A\", hora: {h:?}, tempos: [{}])",
            ts.join(", ")
        )
    };
    let text = format!(
        "(estudo: Some((leituras: [{}, {}, {}, {}])))",
        block(0.5, &mut rng),
        block(2.5, &mut rng),
        block(5.0, &mut rng),
        block(7.5, &mut rng)
    );
    let person: Person = ron::from_str(&text).unwrap();
    assert!(person.validate().is_empty(), "{:?}", person.validate());
    let fit = fit_person(&person, "torno", "A", &AutoConfig::default()).unwrap();
    close("t0", fit.curve.t0, t0, 0.03);
    assert_eq!(fit.distribution.support.n, 160);
    let mean = match fit.distribution.dist {
        Dist::Normal { mean, .. } => mean,
        ref d => d.mean(),
    };
    close("média descansada", mean, t0, 0.03);
    // Another method, or a task without readings, is an error that says which.
    let Err(e) = fit_person(&person, "torno", "B", &AutoConfig::default()) else {
        panic!("método B sem leituras");
    };
    assert!(e.contains("método 'B'"), "{e}");
}

#[test]
fn the_manual_example_reads_and_fits() {
    let doc = std::fs::read_to_string(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../docs/manual/src/pessoas.md"),
    )
    .unwrap();
    let start = doc
        .find("person: Some((\n    study:")
        .expect("exemplo no manual")
        + "person: Some(".len();
    let end = start + doc[start..].find("\n)),\n```").expect("fim do exemplo") + 2;
    let person: Person = ron::from_str(&doc[start..end]).unwrap();
    assert!(person.validate().is_empty(), "{:?}", person.validate());
    let fit = fit_person(&person, "torno", "A", &AutoConfig::default()).unwrap();
    assert!(
        fit.curve.beta > 0.0 && fit.curve.t0 > 35.0 && fit.curve.t0 < 41.0,
        "{:?}",
        fit.curve
    );
    assert!(
        fit.curve
            .warnings
            .iter()
            .any(|w| w.contains("só 3 horários"))
    );
}

#[test]
fn apply_writes_the_own_time_and_keeps_the_rest_of_the_fatigue() {
    let (t0, beta, work) = (40.0, 0.25, 0.3);
    let readings: Vec<Reading> = [0.5, 2.5, 5.0, 7.5]
        .iter()
        .map(|h| {
            let m = curve(t0, beta, work, DEFAULT_RECOVERY_PER_HOUR, &[], *h);
            Reading {
                method: "A".into(),
                ..reading(*h, vec![m * 0.98, m, m * 1.02])
            }
        })
        .collect();
    let mut person = Person {
        fatigue: Some(Fatigue::Dynamic {
            work: 1.0,
            walk: 0.5,
            carry: 0.1,
            recovery: 0.002,
            time_factor: 9.0,
            speed_factor: 0.3,
            limit: Some(0.6),
            target: 0.2,
        }),
        study: Some(utsim_core::TimeStudy {
            readings,
            pauses: Vec::new(),
        }),
        ..Person::default()
    };
    let fit = fit_person(&person, "torno", "A", &AutoConfig::default()).unwrap();
    fit.apply(&mut person, 3600.0);
    let own = &person.times["torno"];
    assert_eq!(own.method, "A");
    close("média do tempo próprio", own.dist.mean(), t0, 0.02);
    match person.fatigue {
        Some(Fatigue::Dynamic {
            work: w,
            walk,
            recovery,
            time_factor,
            speed_factor,
            limit,
            ..
        }) => {
            close("λ", w * 3600.0, work, 0.05);
            close("β", time_factor, beta, 0.05);
            // Not fitted: what the profile had.
            assert_eq!((walk, speed_factor, limit), (0.5, 0.3, Some(0.6)));
            assert_eq!(recovery, 0.002, "μ não estimado fica o do perfil");
        }
        other => panic!("{other:?}"),
    }
    assert!(person.validate().is_empty(), "{:?}", person.validate());
}
