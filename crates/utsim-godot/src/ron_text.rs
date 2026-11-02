//! Writes model values as RON text, the format `Command::Set` expects. A copy of
//! `utsim_viewer::properties::ron_text` (the viewer pulls in Bevy, so the bridge cannot
//! depend on it); both go to a crate without an engine after the viewer's polish round.
//! The `schema` tests check that every value written here reads back unchanged.

use std::collections::BTreeMap;

use utsim_core::{Dist, Downtime, OutputRule, ShiftTable};

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

pub fn downtime(d: &Downtime) -> String {
    format!("(mtbf: {}, mttr: {})", dist(&d.mtbf), dist(&d.mttr))
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
    format!("(period: {}, off: {windows})", float(t.period))
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
