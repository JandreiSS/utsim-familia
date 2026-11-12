//! Fatigue curve of one person from a time study: stopwatch readings taken at the start,
//! in the middle and at the end of the shift (or at any hours).
//!
//! The model is the one the kernel runs ([`utsim_core::Fatigue::Dynamic`]): the level
//! `F` grows while working, `F = 1 − (1 − F0)·e^(−λ·t)`, falls during the pauses,
//! `F = F0·e^(−μ·t)`, and a time started at level `F` is `t0·(1 + β·F)`. The fit:
//!
//! 1. **Curve.** For a given `λ` (and `μ`), `t = t0 + (t0·β)·F(h)` is a straight line in
//!    `F`, fitted by least squares over every reading. `λ` (and `μ` when it can be
//!    estimated) is searched on a log grid and refined; the pair with the least squared
//!    error wins. As many distinct hours as parameters (3, or 4 with `μ`) determine the
//!    curve exactly (the report says there is no slack to check it); with more there is,
//!    and the intervals of `t0` and of the increase `t0·β` are given (conditional on the
//!    fitted rates).
//! 2. **Distribution.** The times divided by `1 + β·F(h)` are the times of the rested
//!    person; they go through the usual automatic fit ([`crate::auto::auto_fit`]).
//! 3. **Recovery.** `μ` is estimated only when there are readings before and after a
//!    pause and at least four distinct hours; otherwise it keeps the kernel's default and
//!    the report says so.
//!
//! The rates come out per hour; [`FatigueFit::fatigue`] converts them to the time unit
//! of the model.

use serde::Serialize;
use utsim_core::{Fatigue, Person, Reading, TaskTime};

use crate::auto::{AutoConfig, AutoFit, auto_fit};

/// Kernel default of `μ` (per second), per hour.
pub const DEFAULT_RECOVERY_PER_HOUR: f64 = 1.0e-3 * 3600.0;
/// Search range of `λ` and `μ`, per hour.
const RATE_RANGE: (f64, f64) = (1.0e-3, 50.0);

/// Mean of the readings at one hour, with the fitted curve.
#[derive(Clone, Debug, Serialize)]
pub struct Point {
    pub hour: f64,
    pub n: usize,
    pub mean: f64,
    /// Fatigue level at that hour, by the fitted curve.
    pub level: f64,
    /// Mean time by the fitted curve.
    pub fitted: f64,
}

/// Confidence intervals of the curve (conditional on the fitted rates).
#[derive(Clone, Debug, Serialize)]
pub struct Intervals {
    pub level: f64,
    /// Interval of `t0`, the time of the rested person.
    pub t0: (f64, f64),
    /// Interval of `t0·β`, what fatigue at its maximum adds to the time.
    pub increase: (f64, f64),
}

/// Fatigue curve of one person in one task and method.
#[derive(Clone, Debug, Serialize)]
pub struct FatigueFit {
    pub task: String,
    pub method: String,
    /// Time of the rested person (mean).
    pub t0: f64,
    /// `β`: times × `1 + β·F`.
    pub beta: f64,
    /// `λ` while working, per hour.
    pub work_per_hour: f64,
    /// `μ` while pausing, per hour.
    pub recovery_per_hour: f64,
    /// `μ` came from the data (otherwise it is the kernel default).
    pub recovery_estimated: bool,
    pub points: Vec<Point>,
    /// Number of readings.
    pub n: usize,
    /// Root mean square of the differences between the means and the curve.
    pub rms: f64,
    /// With four or more distinct hours.
    pub intervals: Option<Intervals>,
    /// Problems with the data or with the fit, in plain language.
    pub warnings: Vec<String>,
}

impl FatigueFit {
    /// The kernel model with these parameters, for a model whose time unit is
    /// `units_per_hour` of an hour (3600 in seconds, 60 in minutes). The walking rates,
    /// the speed factor and the pause limit keep their defaults.
    pub fn fatigue(&self, units_per_hour: f64) -> Fatigue {
        match Fatigue::dynamic() {
            Fatigue::Dynamic {
                walk,
                carry,
                speed_factor,
                limit,
                target,
                ..
            } => Fatigue::Dynamic {
                work: self.work_per_hour / units_per_hour,
                walk,
                carry,
                recovery: self.recovery_per_hour / units_per_hour,
                time_factor: self.beta.max(0.0),
                speed_factor,
                limit,
                target,
            },
            other => other,
        }
    }

    /// Fatigue level at `hour` by the fitted curve.
    pub fn level(&self, hour: f64, pauses: &[(f64, f64)]) -> f64 {
        level_at(hour, self.work_per_hour, self.recovery_per_hour, pauses)
    }

    /// The readings divided by `1 + β·F(h)`: the times of the rested person.
    pub fn normalized(&self, readings: &[&Reading], pauses: &[(f64, f64)]) -> Vec<f64> {
        readings
            .iter()
            .flat_map(|r| {
                let k = 1.0 + self.beta * self.level(r.hour, pauses);
                r.times.iter().map(move |t| t / k)
            })
            .collect()
    }
}

/// Curve and distribution of one person in one task and method.
pub struct PersonFit {
    pub curve: FatigueFit,
    /// Distribution of the normalized times (`t0`).
    pub distribution: AutoFit,
}

impl PersonFit {
    /// Writes the result into the profile, so the kernel uses it: the rested-time
    /// distribution becomes the person's own time of the task (`times`), and the curve
    /// sets the work rate, the time factor and, when it was estimated, the recovery rate
    /// of the dynamic fatigue (the other fatigue fields, and a recovery that was not
    /// estimated, keep what the profile had). `units_per_hour`: time units of the model
    /// in one hour (3600 for seconds).
    pub fn apply(&self, person: &mut Person, units_per_hour: f64) {
        person.times.insert(
            self.curve.task.clone(),
            TaskTime {
                dist: self.distribution.dist.clone(),
                method: self.curve.method.clone(),
            },
        );
        let fitted = self.curve.fatigue(units_per_hour);
        person.fatigue = Some(match (person.fatigue, fitted) {
            (
                Some(Fatigue::Dynamic {
                    walk,
                    carry,
                    recovery,
                    speed_factor,
                    limit,
                    target,
                    ..
                }),
                Fatigue::Dynamic {
                    work,
                    recovery: estimated,
                    time_factor,
                    ..
                },
            ) => Fatigue::Dynamic {
                work,
                walk,
                carry,
                recovery: if self.curve.recovery_estimated {
                    estimated
                } else {
                    recovery
                },
                time_factor,
                speed_factor,
                limit,
                target,
            },
            _ => fitted,
        });
    }
}

/// Fatigue level at `hour` (hours since the start of the shift, rested at the start),
/// working at rate `work` and resting at rate `recovery` during `pauses`.
pub fn level_at(hour: f64, work: f64, recovery: f64, pauses: &[(f64, f64)]) -> f64 {
    let mut sorted: Vec<(f64, f64)> = pauses.to_vec();
    sorted.sort_by(|a, b| a.0.total_cmp(&b.0));
    let mut f = 0.0;
    let mut t = 0.0;
    for (start, end) in sorted {
        if start >= hour {
            break;
        }
        let start = start.max(t);
        if start > t {
            f = 1.0 - (1.0 - f) * (-work * (start - t)).exp();
        }
        let end = end.min(hour);
        if end > start {
            f *= (-recovery * (end - start)).exp();
        }
        t = t.max(end);
    }
    if hour > t {
        f = 1.0 - (1.0 - f) * (-work * (hour - t)).exp();
    }
    f
}

/// Least squares of `y = a + b·x`: `(a, b, sse, sxx, mean x)`.
fn line(x: &[f64], y: &[f64]) -> (f64, f64, f64, f64, f64) {
    let n = x.len() as f64;
    let mx = x.iter().sum::<f64>() / n;
    let my = y.iter().sum::<f64>() / n;
    let sxx: f64 = x.iter().map(|v| (v - mx) * (v - mx)).sum();
    let sxy: f64 = x.iter().zip(y).map(|(a, b)| (a - mx) * (b - my)).sum();
    let b = if sxx > 0.0 { sxy / sxx } else { 0.0 };
    let a = my - b * mx;
    let sse = x.iter().zip(y).map(|(u, v)| (v - a - b * u).powi(2)).sum();
    (a, b, sse, sxx, mx)
}

/// Golden-section search of the minimum of `f` over `ln r` in `[lo, hi]`.
fn refine(f: &dyn Fn(f64) -> f64, lo: f64, hi: f64) -> f64 {
    let g = (5f64.sqrt() - 1.0) / 2.0;
    let (mut a, mut b) = (lo.ln(), hi.ln());
    let mut c = b - g * (b - a);
    let mut d = a + g * (b - a);
    let (mut fc, mut fd) = (f(c.exp()), f(d.exp()));
    for _ in 0..80 {
        if fc < fd {
            b = d;
            d = c;
            fd = fc;
            c = b - g * (b - a);
            fc = f(c.exp());
        } else {
            a = c;
            c = d;
            fc = fd;
            d = a + g * (b - a);
            fd = f(d.exp());
        }
    }
    ((a + b) / 2.0).exp()
}

/// Best rate on the log grid of `RATE_RANGE`, refined between its neighbours.
fn search(f: &dyn Fn(f64) -> f64, steps: usize) -> f64 {
    let (lo, hi) = RATE_RANGE;
    let step = (hi / lo).ln() / steps as f64;
    let rate = |i: usize| lo * (step * i as f64).exp();
    let best = (0..=steps)
        .min_by(|i, j| f(rate(*i)).total_cmp(&f(rate(*j))))
        .unwrap_or(0);
    refine(f, rate(best.saturating_sub(1)), rate((best + 1).min(steps)))
}

/// Fits the fatigue curve to `readings` (one task and method) with the `pauses` of the
/// shift. `level` is the confidence level of the intervals (0.95).
pub fn fit_curve(
    readings: &[&Reading],
    pauses: &[(f64, f64)],
    level: f64,
) -> Result<FatigueFit, String> {
    let first = readings.first().ok_or("não há leituras para ajustar")?;
    let (task, method) = (first.task.clone(), first.method.clone());
    let mut x_hour = Vec::new();
    let mut y = Vec::new();
    for r in readings {
        if !(r.hour.is_finite() && r.hour >= 0.0) {
            return Err(format!("{task}: hora {} inválida", r.hour));
        }
        for t in &r.times {
            if !(t.is_finite() && *t > 0.0) {
                return Err(format!("{task}: tempo {t} inválido (precisa ser > 0)"));
            }
            x_hour.push(r.hour);
            y.push(*t);
        }
    }
    let mut hours: Vec<f64> = x_hour.clone();
    hours.sort_by(f64::total_cmp);
    hours.dedup_by(|a, b| (*a - *b).abs() < 1e-9);
    if hours.len() < 3 {
        return Err(format!(
            "{task}: a curva precisa de leituras em ao menos 3 horários do turno (início, meio e fim); há {}",
            hours.len()
        ));
    }
    let sse = |work: f64, recovery: f64| {
        let x: Vec<f64> = x_hour
            .iter()
            .map(|h| level_at(*h, work, recovery, pauses))
            .collect();
        line(&x, &y).2
    };
    // μ can be told apart only with readings on both sides of a pause and enough hours.
    let across_pause = pauses
        .iter()
        .any(|(a, b)| hours.iter().any(|h| *h <= *a) && hours.iter().any(|h| *h >= *b));
    let estimate_recovery = across_pause && hours.len() >= 4;
    let mut warnings = Vec::new();
    let (work, recovery) = if estimate_recovery {
        let best_work = |mu: f64| search(&|w| sse(w, mu), 120);
        let mu = search(&|mu| sse(best_work(mu), mu), 40);
        (best_work(mu), mu)
    } else {
        if !pauses.is_empty() {
            warnings.push(
                "a recuperação nas pausas (μ) ficou no padrão do UTSim: para estimá-la, meça antes \
                 e depois de uma pausa, com ao menos 4 horários"
                    .to_string(),
            );
        }
        let mu = DEFAULT_RECOVERY_PER_HOUR;
        (search(&|w| sse(w, mu), 200), mu)
    };
    let x: Vec<f64> = x_hour
        .iter()
        .map(|h| level_at(*h, work, recovery, pauses))
        .collect();
    let (t0, increase, sse_all, sxx, mx) = line(&x, &y);
    if t0.is_nan() || t0 <= 0.0 {
        return Err(format!(
            "{task}: a curva não fecha (tempo descansado {t0:.3} <= 0); confira as leituras"
        ));
    }
    let beta = increase / t0;
    let (lo, hi) = RATE_RANGE;
    if work <= lo * 1.05 || work >= hi * 0.95 {
        warnings.push(format!(
            "a curva é quase reta (λ no limite da busca, {work:.3}/h): λ e β não se separam; \
             meça mais perto do fim do turno ou em mais horários"
        ));
    }
    if beta < 0.0 {
        warnings.push(
            "os tempos caem ao longo do turno: é aprendizado ou aquecimento, não fadiga; β fica 0 \
             no modelo"
                .to_string(),
        );
    }
    // Parameters of the curve: t0, β, λ and, when estimated, μ.
    let free = if estimate_recovery { 4 } else { 3 };
    if hours.len() == free {
        warnings.push(format!(
            "só {free} horários: a curva passa exatamente pelas médias e não há folga para \
             conferi-la; com {} ou mais o relatório dá os intervalos",
            free + 1
        ));
    }
    let points: Vec<Point> = hours
        .iter()
        .map(|h| {
            let ts: Vec<f64> = x_hour
                .iter()
                .zip(&y)
                .filter(|(a, _)| (**a - *h).abs() < 1e-9)
                .map(|(_, t)| *t)
                .collect();
            let lv = level_at(*h, work, recovery, pauses);
            Point {
                hour: *h,
                n: ts.len(),
                mean: ts.iter().sum::<f64>() / ts.len() as f64,
                level: lv,
                fitted: t0 + increase * lv,
            }
        })
        .collect();
    let rms = (points
        .iter()
        .map(|p| (p.mean - p.fitted).powi(2))
        .sum::<f64>()
        / points.len() as f64)
        .sqrt();
    let n = y.len();
    let intervals = (hours.len() > free && n > 2 && sxx > 0.0).then(|| {
        let s2 = sse_all / (n - 2) as f64;
        let tq = utsim_stats::t_quantile(level, n - 2);
        let se_b = (s2 / sxx).sqrt();
        let se_a = (s2 * (1.0 / n as f64 + mx * mx / sxx)).sqrt();
        Intervals {
            level,
            t0: (t0 - tq * se_a, t0 + tq * se_a),
            increase: (increase - tq * se_b, increase + tq * se_b),
        }
    });
    Ok(FatigueFit {
        task,
        method,
        t0,
        beta,
        work_per_hour: work,
        recovery_per_hour: recovery,
        recovery_estimated: estimate_recovery,
        points,
        n,
        rms,
        intervals,
        warnings,
    })
}

/// Fits the curve and the distribution of the rested times of `person` in `task` and
/// `method` (empty: the readings without a method).
pub fn fit_person(
    person: &Person,
    task: &str,
    method: &str,
    config: &AutoConfig,
) -> Result<PersonFit, String> {
    let study = person
        .study
        .as_ref()
        .ok_or("a pessoa não tem estudo de tempos (study)")?;
    let readings: Vec<&Reading> = study
        .readings
        .iter()
        .filter(|r| r.task == task && r.method == method)
        .collect();
    if readings.is_empty() {
        return Err(if method.is_empty() {
            format!("não há leituras da tarefa '{task}'")
        } else {
            format!("não há leituras da tarefa '{task}' pelo método '{method}'")
        });
    }
    let curve = fit_curve(&readings, &study.pauses, 0.95)?;
    let rested = curve.normalized(&readings, &study.pauses);
    let name = if method.is_empty() {
        task.to_string()
    } else {
        format!("{task} ({method})")
    };
    let distribution = auto_fit(&name, &rested, config)?;
    Ok(PersonFit {
        curve,
        distribution,
    })
}
