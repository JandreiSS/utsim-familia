//! Writes model values as RON text, the format `Command::Set` expects, for the editors
//! that build a `Set` from a typed value (the Bevy viewer's properties panel and the
//! Godot bridge's schema). The (small, closed) set of types is formatted by hand, as
//! the model files write them; the tests here and in both editors check that every
//! output parses back to the same value.

use std::collections::BTreeMap;

use utsim_core::{Dist, Downtime, OutputRule, ShiftTable};
use utsim_model::{
    ArrivalSchedule, ArrivalTable, DowntimeDef, PeriodArrivals, ShiftSpec, TableRepeat,
};

pub fn float(v: f64) -> String {
    format!("{v:?}")
}

fn list<T>(items: &[T], f: impl Fn(&T) -> String) -> String {
    let inner: Vec<String> = items.iter().map(f).collect();
    format!("[{}]", inner.join(", "))
}

pub fn floats(xs: &[f64]) -> String {
    list(xs, |x| float(*x))
}

pub fn usizes(xs: &[usize]) -> String {
    list(xs, |x| x.to_string())
}

pub fn strings(xs: &[String]) -> String {
    list(xs, |x| string(x))
}

/// A processor's `operators` section (`OperatorUse`).
pub fn operators(team: &str, count: usize, setup: bool, process: bool) -> String {
    format!(
        "(team: {}, count: {count}, setup: {setup}, process: {process})",
        string(team)
    )
}

/// Like [`operators`], with the skill the operators must have (`requires`), if any.
pub fn operators_requiring(
    team: &str,
    count: usize,
    setup: bool,
    process: bool,
    requires: Option<&str>,
) -> String {
    let base = operators(team, count, setup, process);
    match requires {
        None => base,
        Some(skill) => format!(
            "{}, requires: {})",
            base.trim_end_matches(')'),
            string(skill)
        ),
    }
}

/// A processor's whole `operators` section, with the optional skill and request
/// priority fields (left out when absent, as the model writes them).
pub fn operator_use(o: &utsim_objects::OperatorUse) -> String {
    let mut text = operators_requiring(&o.team, o.count, o.setup, o.process, o.requires.as_deref());
    text.pop();
    if let Some(p) = o.priority {
        text.push_str(&format!(", priority: {p}"));
    }
    if let Some(label) = &o.priority_label {
        text.push_str(&format!(", priority_label: {}", string(label)));
    }
    text.push(')');
    text
}

pub fn string(s: &str) -> String {
    let mut out = String::from("\"");
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

pub fn option<T>(v: &Option<T>, f: impl Fn(&T) -> String) -> String {
    v.as_ref().map_or("None".to_string(), f)
}

pub fn dist(d: &Dist) -> String {
    let f = float;
    match d {
        Dist::Const(v) => format!("Const({})", f(*v)),
        Dist::Uniform { min, max } => format!("Uniform(min: {}, max: {})", f(*min), f(*max)),
        Dist::Exp { mean } => format!("Exp(mean: {})", f(*mean)),
        Dist::Triangular { min, mode, max } => format!(
            "Triangular(min: {}, mode: {}, max: {})",
            f(*min),
            f(*mode),
            f(*max)
        ),
        Dist::Normal { mean, sd } => format!("Normal(mean: {}, sd: {})", f(*mean), f(*sd)),
        Dist::LogNormal { mean, sd } => format!("LogNormal(mean: {}, sd: {})", f(*mean), f(*sd)),
        Dist::Gamma { shape, scale } => {
            format!("Gamma(shape: {}, scale: {})", f(*shape), f(*scale))
        }
        Dist::Erlang { k, mean } => format!("Erlang(k: {k}, mean: {})", f(*mean)),
        Dist::Weibull { shape, scale } => {
            format!("Weibull(shape: {}, scale: {})", f(*shape), f(*scale))
        }
        Dist::Discrete(table) => format!(
            "Discrete({})",
            list(table, |(v, w)| format!("({}, {})", f(*v), f(*w)))
        ),
        Dist::Beta {
            alpha,
            beta,
            min,
            max,
        } => format!(
            "Beta(alpha: {}, beta: {}, min: {}, max: {})",
            f(*alpha),
            f(*beta),
            f(*min),
            f(*max)
        ),
        Dist::LogLogistic { shape, scale } => {
            format!("LogLogistic(shape: {}, scale: {})", f(*shape), f(*scale))
        }
        Dist::PearsonV { shape, scale } => {
            format!("PearsonV(shape: {}, scale: {})", f(*shape), f(*scale))
        }
        Dist::PearsonVI {
            shape1,
            shape2,
            scale,
        } => format!(
            "PearsonVI(shape1: {}, shape2: {}, scale: {})",
            f(*shape1),
            f(*shape2),
            f(*scale)
        ),
        Dist::Shifted {
            offset,
            dist: inner,
        } => {
            format!("Shifted(offset: {}, dist: {})", f(*offset), dist(inner))
        }
        Dist::Poisson { mean } => format!("Poisson(mean: {})", f(*mean)),
        Dist::Geometric { p } => format!("Geometric(p: {})", f(*p)),
        Dist::Binomial { n, p } => format!("Binomial(n: {n}, p: {})", f(*p)),
        Dist::NegBinomial { r, p } => format!("NegBinomial(r: {}, p: {})", f(*r), f(*p)),
        Dist::Empirical(xs) => format!("Empirical({})", floats(xs)),
    }
}

pub fn output(rule: &OutputRule) -> String {
    match rule {
        OutputRule::FirstAvailable => "FirstAvailable".into(),
        OutputRule::RoundRobin => "RoundRobin".into(),
        OutputRule::Random => "Random".into(),
        OutputRule::ByLabel(label) => format!("ByLabel({})", string(label)),
        OutputRule::Weighted(w) => format!("Weighted({})", floats(w)),
        OutputRule::Script(src) => format!("Script({})", string(src)),
    }
}

/// Breakdowns as the `downtime` field writes them: the kernel's [`Downtime`] or the
/// model's [`DowntimeDef`] (whose maintenance fields are kept).
pub trait DowntimeText {
    fn downtime_text(&self) -> String;
}

impl DowntimeText for Downtime {
    fn downtime_text(&self) -> String {
        format!("(mtbf: {}, mttr: {})", dist(&self.mtbf), dist(&self.mttr))
    }
}

impl DowntimeText for DowntimeDef {
    fn downtime_text(&self) -> String {
        if self.has_maintenance() {
            ron::to_string(self).expect("downtime serializes")
        } else {
            self.downtime().downtime_text()
        }
    }
}

pub fn downtime<D: DowntimeText>(d: &D) -> String {
    d.downtime_text()
}

pub fn transfer(t: &utsim_core::TransferDef) -> String {
    format!(
        "(speed: {}, delay: {}, path: {:?})",
        float(t.speed),
        float(t.delay),
        t.path
    )
}

pub fn shifts(t: &ShiftTable) -> String {
    let windows = list(&t.off, |w| {
        format!(
            "(start: {}, duration: {})",
            float(w.start),
            float(w.duration)
        )
    });
    let end = if t.on_end.is_pause() {
        String::new()
    } else {
        format!(", on_end: {:?}", t.on_end)
    };
    format!("(period: {}, off: {windows}{end})", float(t.period))
}

/// An object's `shifts`: inline table or `Calendar("name")` (with its own `on_end`).
pub fn shift_spec(s: &ShiftSpec) -> String {
    match s {
        ShiftSpec::Inline(t) => shifts(t),
        ShiftSpec::Calendar { name, on_end: None } => format!("Calendar({})", string(name)),
        ShiftSpec::Calendar {
            name,
            on_end: Some(end),
        } => format!("Calendar({}, {end:?})", string(name)),
    }
}

pub fn period_arrivals(a: &PeriodArrivals) -> String {
    match a {
        PeriodArrivals::Default => "Default".into(),
        PeriodArrivals::Interarrival(d) => format!("Interarrival({})", dist(d)),
        PeriodArrivals::Rate(r) => format!("Rate({})", float(*r)),
        PeriodArrivals::Closed => "Closed".into(),
    }
}

/// A source's `schedule` (defaults left out, as the model writes them).
pub fn schedule(s: &ArrivalSchedule) -> String {
    let periods = list(&s.periods, |p| {
        let mut text = format!("(start: {}, end: {}", float(p.start), float(p.end));
        if !p.arrivals.is_default() {
            text.push_str(&format!(", arrivals: {}", period_arrivals(&p.arrivals)));
        }
        if !p.labels.is_empty() {
            text.push_str(&format!(", labels: {}", labels(&p.labels)));
        }
        text.push(')');
        text
    });
    format!("(cycle: {}, periods: {periods})", float(s.cycle))
}

/// A source's `table` (defaults left out, as the model writes them).
pub fn table(t: &ArrivalTable) -> String {
    let rows = list(&t.rows, |r| {
        let mut parts = Vec::new();
        if let Some(at) = r.at {
            parts.push(format!("at: {}", float(at)));
        }
        parts.push(format!("count: {}", r.count));
        if let Some(ty) = &r.item_type {
            parts.push(format!("item_type: {}", string(ty)));
        }
        if !r.labels.is_empty() {
            let entries: Vec<String> = r
                .labels
                .iter()
                .map(|(k, v)| format!("{}: {}", string(k), float(*v)))
                .collect();
            parts.push(format!("labels: {{{}}}", entries.join(", ")));
        }
        format!("({})", parts.join(", "))
    });
    let repeat = match t.repeat {
        TableRepeat::Once => String::new(),
        TableRepeat::Cycle(d) => format!(", repeat: Cycle({})", float(d)),
        TableRepeat::Loop => ", repeat: Loop".into(),
    };
    format!("(mode: {:?}{repeat}, rows: {rows})", t.mode)
}

pub fn batch(size: usize, max_wait: f64, flush: bool) -> String {
    format!(
        "(size: {size}, max_wait: {}, flush: {flush})",
        float(max_wait)
    )
}

pub fn labels(map: &BTreeMap<String, Dist>) -> String {
    let entries: Vec<String> = map
        .iter()
        .map(|(k, d)| format!("{}: {}", string(k), dist(d)))
        .collect();
    format!("{{{}}}", entries.join(", "))
}

/// Unit-like enums (`Fifo`, `Wait`, `Pack`...) and `Split(n)` are written by their
/// derived `Debug`, which is exactly their RON form.
pub fn debug<T: std::fmt::Debug>(v: &T) -> String {
    format!("{v:?}")
}

/// Reads RON text the way model files are read (`Some` is implicit).
pub fn parse<T: serde::de::DeserializeOwned>(text: &str) -> Result<T, String> {
    ::ron::Options::default()
        .with_default_extension(::ron::extensions::Extensions::IMPLICIT_SOME)
        .from_str(text)
        .map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use utsim_core::{OffWindow, ShiftEnd};

    #[test]
    fn values_read_back_unchanged() {
        let dists = [
            Dist::Const(2.5),
            Dist::Exp { mean: 3.0 },
            Dist::Uniform { min: 1.0, max: 2.0 },
            Dist::Triangular {
                min: 1.0,
                mode: 2.0,
                max: 4.0,
            },
        ];
        for d in dists {
            assert_eq!(parse::<Dist>(&dist(&d)).unwrap(), d);
        }
        let table = ShiftTable {
            period: 1440.0,
            off: vec![OffWindow {
                start: 720.0,
                duration: 60.0,
            }],
            on_end: ShiftEnd::Finish,
        };
        assert_eq!(parse::<ShiftTable>(&shifts(&table)).unwrap(), table);
        let specs = [
            ShiftSpec::Inline(table.clone()),
            ShiftSpec::calendar("turno_a"),
            ShiftSpec::Calendar {
                name: "turno_a".into(),
                on_end: Some(ShiftEnd::Finish),
            },
        ];
        for s in specs {
            assert_eq!(parse::<ShiftSpec>(&shift_spec(&s)).unwrap(), s);
        }
        assert_eq!(parse::<Vec<f64>>(&floats(&[1.0, 0.5])).unwrap(), [1.0, 0.5]);
        assert_eq!(parse::<String>(&string("a \"b\"")).unwrap(), "a \"b\"");
        assert!(parse::<Dist>("Exp(").is_err());
    }

    #[test]
    fn arrival_tables_read_back_unchanged() {
        use utsim_model::{ArrivalRow, TableMode};
        let tables = [
            ArrivalTable {
                mode: TableMode::Times,
                repeat: TableRepeat::Cycle(1440.0),
                rows: vec![
                    ArrivalRow {
                        at: Some(480.0),
                        count: 12,
                        item_type: Some("caixa".into()),
                        labels: BTreeMap::from([("prioridade".to_string(), 2.5)]),
                    },
                    ArrivalRow {
                        at: Some(600.0),
                        count: 0,
                        item_type: None,
                        labels: BTreeMap::new(),
                    },
                ],
            },
            ArrivalTable {
                mode: TableMode::Sequence,
                repeat: TableRepeat::Loop,
                rows: vec![ArrivalRow {
                    at: None,
                    count: 3,
                    item_type: None,
                    labels: BTreeMap::new(),
                }],
            },
            ArrivalTable {
                mode: TableMode::Sequence,
                repeat: TableRepeat::Once,
                rows: Vec::new(),
            },
        ];
        for t in tables {
            assert_eq!(
                parse::<ArrivalTable>(&table(&t)).unwrap(),
                t,
                "{}",
                table(&t)
            );
        }
    }
}
