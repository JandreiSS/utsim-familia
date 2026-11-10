//! Standardised work and line balancing (`docs/plano_lean_seis_sigma.md`, 2.4): the
//! standard work sheet (the sequence of elements of each operator with manual, automatic
//! and walking times), the combination table against the takt, the standard work in process
//! and the balance of the operators against the takt. Times in seconds.

/// The takt: available time over demand (same period). `None` when either is not positive.
pub fn takt(available_s: f64, demand: f64) -> Option<f64> {
    (available_s > 0.0 && demand > 0.0 && available_s.is_finite() && demand.is_finite())
        .then(|| available_s / demand)
}

/// One element of the work of an operator.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Element {
    pub operator: String,
    pub name: String,
    pub manual: f64,
    /// Machine time after the manual part; the operator is free meanwhile.
    pub auto: f64,
    pub walk: f64,
    /// Standard work in process kept at this element (pieces).
    pub wip: f64,
}

/// One row of the combination table: when the element starts in the operator's cycle.
#[derive(Debug, Clone, PartialEq)]
pub struct Step {
    pub name: String,
    pub start: f64,
    pub manual: f64,
    pub auto: f64,
    pub walk: f64,
    /// The machine has not finished when the operator comes back to it next cycle.
    pub machine_late: bool,
}

/// The work of one operator.
#[derive(Debug, Clone, PartialEq)]
pub struct Operator {
    pub name: String,
    pub steps: Vec<Step>,
    /// Σ (manual + walk): the operator's cycle.
    pub content: f64,
    /// Content over takt, in %.
    pub occupancy: Option<f64>,
    pub over_takt: bool,
    pub wip: f64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Balance {
    pub takt: Option<f64>,
    /// In the order the operators first appear.
    pub operators: Vec<Operator>,
    pub total_content: f64,
    /// ⌈Σ content / takt⌉.
    pub min_operators: Option<usize>,
    /// Σ content / (operators × the longest cycle), in %.
    pub efficiency: Option<f64>,
    /// The longest operator cycle (the line's pace when above the takt).
    pub bottleneck: Option<f64>,
    pub total_wip: f64,
}

fn clean(x: f64) -> f64 {
    if x.is_finite() && x > 0.0 { x } else { 0.0 }
}

/// The combination table and the balance. Elements without operator go to "" (one group).
pub fn balance(elements: &[Element], takt: Option<f64>) -> Balance {
    let takt = takt.filter(|t| t.is_finite() && *t > 0.0);
    let mut ops: Vec<Operator> = Vec::new();
    for e in elements {
        let name = e.operator.trim().to_string();
        let i = match ops.iter().position(|o| o.name == name) {
            Some(i) => i,
            None => {
                ops.push(Operator {
                    name,
                    steps: Vec::new(),
                    content: 0.0,
                    occupancy: None,
                    over_takt: false,
                    wip: 0.0,
                });
                ops.len() - 1
            }
        };
        let o = &mut ops[i];
        let (manual, auto, walk) = (clean(e.manual), clean(e.auto), clean(e.walk));
        o.steps.push(Step {
            name: e.name.clone(),
            start: o.content,
            manual,
            auto,
            walk,
            machine_late: false,
        });
        o.content += manual + walk;
        o.wip += clean(e.wip);
    }
    for o in &mut ops {
        // The operator comes back after the longer of the own cycle and the takt.
        let cycle = takt.map_or(o.content, |t| t.max(o.content));
        for s in &mut o.steps {
            s.machine_late = s.auto > 0.0 && s.manual + s.auto > cycle + 1e-9;
        }
        if let Some(t) = takt {
            o.occupancy = Some(o.content / t * 100.0);
            o.over_takt = o.content > t + 1e-9;
        }
    }
    let total_content: f64 = ops.iter().map(|o| o.content).sum();
    let bottleneck = ops
        .iter()
        .map(|o| o.content)
        .fold(None, |m: Option<f64>, c| Some(m.map_or(c, |m| m.max(c))));
    let efficiency = bottleneck
        .filter(|b| *b > 0.0)
        .map(|b| total_content / (ops.len() as f64 * b) * 100.0);
    Balance {
        min_operators: takt.map(|t| (total_content / t - 1e-9).ceil().max(0.0) as usize),
        total_wip: ops.iter().map(|o| o.wip).sum(),
        takt,
        operators: ops,
        total_content,
        efficiency,
        bottleneck,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn el(op: &str, name: &str, manual: f64, auto: f64, walk: f64) -> Element {
        Element {
            operator: op.into(),
            name: name.into(),
            manual,
            auto,
            walk,
            wip: 1.0,
        }
    }

    #[test]
    fn takt_by_hand() {
        // Two shifts of 7,5 h less 30 min of breaks: 2 × 7 h = 50 400 s; 840 pieces: 60 s.
        assert_eq!(takt(50_400.0, 840.0), Some(60.0));
        assert_eq!(takt(0.0, 10.0), None);
    }

    #[test]
    fn balance_by_hand() {
        let b = balance(
            &[
                el("Ana", "Pegar a peça", 5.0, 0.0, 3.0),
                el("Ana", "Tornear", 8.0, 40.0, 4.0),
                el("Ana", "Furar", 10.0, 30.0, 5.0),
                el("Bia", "Montar", 30.0, 0.0, 2.0),
                el("Bia", "Inspecionar", 20.0, 0.0, 0.0),
                el("Caio", "Embalar", 70.0, 0.0, 5.0),
            ],
            Some(60.0),
        );
        let ana = &b.operators[0];
        // Ana: 8 + 12 + 15 = 35 s.
        assert_eq!(ana.content, 35.0);
        assert_eq!(ana.steps[1].start, 8.0);
        assert_eq!(ana.steps[2].start, 20.0);
        assert!(!ana.steps[1].machine_late, "8 + 40 ≤ 60");
        assert_eq!(ana.wip, 3.0);
        let bia = &b.operators[1];
        assert_eq!(bia.content, 52.0);
        assert!((bia.occupancy.unwrap() - 52.0 / 60.0 * 100.0).abs() < 1e-9);
        assert!(!bia.over_takt);
        let caio = &b.operators[2];
        assert_eq!(caio.content, 75.0);
        assert!(caio.over_takt);
        // Σ = 162 s; ⌈162 / 60⌉ = 3; efficiency 162 / (3 × 75) = 72 %.
        assert_eq!(b.total_content, 162.0);
        assert_eq!(b.min_operators, Some(3));
        assert_eq!(b.bottleneck, Some(75.0));
        assert!((b.efficiency.unwrap() - 72.0).abs() < 1e-9);
        assert_eq!(b.total_wip, 6.0);
    }

    #[test]
    fn machine_late_against_the_takt() {
        let b = balance(&[el("Ana", "Fresar", 10.0, 55.0, 2.0)], Some(60.0));
        assert!(b.operators[0].steps[0].machine_late, "10 + 55 > 60");
        let b = balance(&[el("Ana", "Fresar", 10.0, 55.0, 2.0)], None);
        assert!(
            b.operators[0].steps[0].machine_late,
            "without takt, against 12 s"
        );
        assert_eq!(b.min_operators, None);
        // Exact multiple: 120 s of content with takt 60 needs exactly 2.
        let b = balance(
            &[el("A", "x", 60.0, 0.0, 0.0), el("B", "y", 60.0, 0.0, 0.0)],
            Some(60.0),
        );
        assert_eq!(b.min_operators, Some(2));
        assert_eq!(b.efficiency, Some(100.0));
    }
}
