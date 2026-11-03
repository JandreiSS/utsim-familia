//! The `downtime` field of an object: breakdowns (`mtbf`, `mttr`) and, optionally, the
//! machine maintenance of `utsim_core::maintenance` (repair crew, wear, preventive plans,
//! costs and OEE).
//!
//! ```text
//! downtime: (
//!     mtbf: Exp(mean: 400.0), mttr: Triangular(min: 10.0, mode: 15.0, max: 30.0),
//!     repair_crew: Some("manutencao"),            // technicians who come to the machine
//!     wear: Some(Weibull(shape: 2.0, scale: 500.0)),
//!     basis: Busy,                                  // Clock (default) | Busy | Cycles
//!     repair: AsBadAsOld,                           // AsBadAsOld (default) | AsGoodAsNew | Partial(q)
//!     preventive: [(every: Hours(300.0), duration: Const(20.0))],
//!     cost: Some((per_time_down: 2.0, per_repair: 150.0, per_preventive: 60.0)),
//!     oee: Some((ideal_cycle: Some(1.0), reject_to: ["refugo"], reject_label: Some("defeito"))),
//! )
//! ```
//!
//! Only `mtbf` and `mttr` are required. Without any of the other fields the object keeps
//! the plain breakdown kernel and its results do not change at all.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use utsim_core::{
    AgeBasis, BlockId, Dist, Downtime, Maintenance, MaintenanceCost, Preventive, RepairEffect,
    Trigger, Wear,
};

use crate::{Model, ObjectDef, ObjectKind, is_mobile};

/// Breakdowns and maintenance of one object.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct DowntimeDef {
    /// Time between failures (in the units of `basis`); not used when `wear` is set.
    pub mtbf: Dist,
    /// Repair time (from the technician's arrival when there is a `repair_crew`).
    pub mttr: Dist,
    /// Team (Transporter or Dispatcher) whose technician comes to repair the machine.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub repair_crew: Option<String>,
    /// Failure law by age (replaces `mtbf`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub wear: Option<Wear>,
    /// What the age counts.
    #[serde(default, skip_serializing_if = "AgeBasis::is_clock")]
    pub basis: AgeBasis,
    /// How much of the age a corrective repair takes away.
    #[serde(default, skip_serializing_if = "RepairEffect::is_default")]
    pub repair: RepairEffect,
    /// Preventive maintenance plans.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub preventive: Vec<PreventiveDef>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cost: Option<Box<MaintenanceCost>>,
    /// OEE options (the OEE is measured for every object with maintenance).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub oee: Option<Box<OeeDef>>,
}

impl DowntimeDef {
    /// Plain breakdowns (`mtbf`, `mttr`), nothing else.
    pub fn new(mtbf: Dist, mttr: Dist) -> Self {
        DowntimeDef {
            mtbf,
            mttr,
            repair_crew: None,
            wear: None,
            basis: AgeBasis::Clock,
            repair: RepairEffect::AsBadAsOld,
            preventive: Vec::new(),
            cost: None,
            oee: None,
        }
    }

    /// The kernel's breakdown part.
    pub fn downtime(&self) -> Downtime {
        Downtime {
            mtbf: self.mtbf.clone(),
            mttr: self.mttr.clone(),
        }
    }

    /// Whether any maintenance field is set (else: plain breakdowns).
    pub fn has_maintenance(&self) -> bool {
        *self != DowntimeDef::new(self.mtbf.clone(), self.mttr.clone())
    }
}

impl From<Downtime> for DowntimeDef {
    fn from(d: Downtime) -> Self {
        DowntimeDef::new(d.mtbf, d.mttr)
    }
}

impl PartialEq<Downtime> for DowntimeDef {
    fn eq(&self, other: &Downtime) -> bool {
        !self.has_maintenance() && self.mtbf == other.mtbf && self.mttr == other.mttr
    }
}

/// One preventive maintenance plan.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct PreventiveDef {
    pub every: Every,
    pub duration: Dist,
    /// Team that does it (absent: the `repair_crew`; none: the machine stops alone).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub crew: Option<String>,
}

/// When a preventive is due.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum Every {
    /// This much working time since the last one, in the model's time unit (the name
    /// follows the hour meter of machines).
    #[serde(alias = "Horas", alias = "Usage", alias = "Uso")]
    Hours(f64),
    /// This many items left the machine since the last one.
    #[serde(alias = "Ciclos")]
    Cycles(u64),
    /// This much clock time after the end of the last one.
    #[serde(alias = "Relogio")]
    Clock(f64),
    /// At the start of every off window of this calendar of the model.
    #[serde(alias = "Calendario")]
    Calendar(String),
}

/// OEE options.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct OeeDef {
    /// Ideal time per item (absent: the minimum of the process time, or its mean when
    /// the minimum is 0 or unbounded below).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ideal_cycle: Option<f64>,
    /// Objects (usually Sinks) that receive rejects from this object.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub reject_to: Vec<String>,
    /// Items leaving with this label different from 0 or "" are rejects.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reject_label: Option<String>,
}

/// Smallest value of a distribution, 0 when unbounded below.
fn support_min(d: &Dist) -> f64 {
    match d {
        Dist::Const(c) => *c,
        Dist::Uniform { min, .. } | Dist::Triangular { min, .. } | Dist::Beta { min, .. } => *min,
        Dist::Discrete(table) => table
            .iter()
            .filter(|(_, w)| *w > 0.0)
            .map(|(v, _)| *v)
            .fold(f64::INFINITY, f64::min),
        Dist::Empirical(xs) => xs.iter().copied().fold(f64::INFINITY, f64::min),
        Dist::Shifted { offset, dist } => offset + support_min(dist),
        _ => 0.0,
    }
}

/// Ideal time per item of an object: `ideal_cycle`, else the minimum of the process time
/// (its mean when the minimum is not positive), over the number of servers.
pub fn ideal_cycle(obj: &ObjectDef) -> Option<f64> {
    let given = obj
        .downtime
        .as_ref()
        .and_then(|d| d.oee.as_ref())
        .and_then(|o| o.ideal_cycle);
    let (process, servers) = match &obj.kind {
        ObjectKind::Processor {
            process, capacity, ..
        } => (process, *capacity),
        ObjectKind::Combiner { process, .. } | ObjectKind::Separator { process, .. } => {
            (process, 1)
        }
        _ => return given,
    };
    let per_item = given.unwrap_or_else(|| {
        let min = support_min(process);
        if min.is_finite() && min > 0.0 {
            min
        } else {
            process.mean()
        }
    });
    Some(per_item / servers.max(1) as f64)
}

impl Model {
    /// The kernel maintenance of `obj`, `None` for plain breakdowns or none.
    pub(crate) fn maintenance_of(
        &self,
        obj: &ObjectDef,
        ids: &HashMap<&str, BlockId>,
    ) -> Option<Maintenance> {
        let d = obj.downtime.as_ref().filter(|d| d.has_maintenance())?;
        let id = |name: &Option<String>| name.as_deref().and_then(|n| ids.get(n).copied());
        let preventive = d
            .preventive
            .iter()
            .filter_map(|p| {
                let trigger = match &p.every {
                    Every::Hours(h) => Trigger::Hours(*h),
                    Every::Cycles(n) => Trigger::Cycles(*n),
                    Every::Clock(t) => Trigger::Clock(*t),
                    Every::Calendar(name) => Trigger::Calendar(self.calendars.get(name)?.clone()),
                };
                Some(Preventive {
                    trigger,
                    duration: p.duration.clone(),
                    crew: id(&p.crew),
                })
            })
            .collect();
        let oee = d.oee.as_deref().cloned().unwrap_or_default();
        Some(Maintenance {
            crew: id(&d.repair_crew),
            wear: d.wear,
            basis: d.basis,
            repair: d.repair,
            preventive,
            cost: d.cost.as_deref().copied().unwrap_or_default(),
            ideal_cycle: ideal_cycle(obj),
            reject_to: oee
                .reject_to
                .iter()
                .filter_map(|n| ids.get(n.as_str()).copied())
                .collect(),
            reject_label: oee.reject_label,
        })
    }

    /// Errors in the maintenance fields of every object.
    pub(crate) fn validate_maintenance(&self, errors: &mut Vec<String>) {
        let kind = |name: &str| {
            self.objects
                .iter()
                .find(|o| o.name == name)
                .map(|o| &o.kind)
        };
        for obj in &self.objects {
            let Some(d) = &obj.downtime else { continue };
            let n = &obj.name;
            let crew_ok = |what: &str, team: &str, errors: &mut Vec<String>| {
                if !kind(team).is_some_and(is_mobile) {
                    errors.push(format!(
                        "{n}: {what} '{team}' precisa ser um Transporter ou Dispatcher"
                    ));
                }
                if team == n {
                    errors.push(format!("{n}: {what} não pode ser o próprio objeto"));
                }
            };
            if let Some(team) = &d.repair_crew {
                crew_ok("repair_crew", team, errors);
            }
            if let Some(w) = &d.wear {
                if let Err(e) = w.validate() {
                    errors.push(format!("{n}.downtime.{e}"));
                }
            }
            if let RepairEffect::Partial(q) = d.repair {
                if !(0.0..=1.0).contains(&q) {
                    errors.push(format!("{n}.downtime.repair: Partial(q) exige q em [0, 1]"));
                }
            }
            if let Some(c) = &d.cost {
                if let Err(e) = c.validate() {
                    errors.push(format!("{n}.downtime.{e}"));
                }
            }
            for (k, p) in d.preventive.iter().enumerate() {
                let at = format!("{n}.downtime.preventive[{k}]");
                match &p.every {
                    Every::Hours(t) | Every::Clock(t) if !(t.is_finite() && *t > 0.0) => {
                        errors.push(format!("{at}: o intervalo deve ser finito e > 0"));
                    }
                    Every::Cycles(0) => errors.push(format!("{at}: Cycles(n) exige n >= 1")),
                    Every::Calendar(c) if !self.calendars.contains_key(c) => {
                        errors.push(format!("{at}: o calendário '{c}' não existe"));
                    }
                    _ => {}
                }
                if let Err(e) = p.duration.validate() {
                    errors.push(format!("{at}.duration: {e}"));
                }
                if let Some(team) = &p.crew {
                    crew_ok(&format!("preventive[{k}].crew"), team, errors);
                }
            }
            if let Some(o) = &d.oee {
                if o.ideal_cycle.is_some_and(|c| !(c.is_finite() && c > 0.0)) {
                    errors.push(format!(
                        "{n}.downtime.oee.ideal_cycle deve ser finito e > 0"
                    ));
                }
                for r in &o.reject_to {
                    if kind(r).is_none() {
                        errors.push(format!("{n}.downtime.oee.reject_to: '{r}' não existe"));
                    } else if !self.connections.iter().any(|(f, t)| f == n && t == r) {
                        errors.push(format!(
                            "{n}.downtime.oee.reject_to: '{r}' não está ligado à saída de {n}"
                        ));
                    }
                }
            }
        }
    }

    /// Warnings about maintenance (shared team, mtbf unused with wear).
    pub(crate) fn maintenance_warnings(&self, out: &mut Vec<String>) {
        for obj in &self.objects {
            let Some(d) = obj.downtime.as_ref().filter(|d| d.has_maintenance()) else {
                continue;
            };
            if let Some(team) = &d.repair_crew {
                let operates = self.objects.iter().any(|o| {
                    matches!(&o.kind, ObjectKind::Processor { operators: Some(op), .. } if op.team == *team)
                        || o.transport.as_deref() == Some(team)
                });
                if operates {
                    out.push(format!(
                        "{}: a equipe de manutenção '{team}' também opera ou transporta; \
                         um técnico ocupado atrasa o reparo e pode haver espera circular",
                        obj.name
                    ));
                }
            }
            if d.repair_crew.is_none() && d.preventive.iter().any(|p| p.crew.is_some()) {
                // Fine: the preventive has its own team.
            } else if d.repair_crew.is_none()
                && self
                    .objects
                    .iter()
                    .any(|o| matches!(o.kind, ObjectKind::Transporter { .. }))
                && !d.preventive.is_empty()
            {
                out.push(format!(
                    "{}: preventivas e reparos sem equipe (repair_crew): a máquina para sozinha, sem técnico",
                    obj.name
                ));
            }
        }
    }

    /// One line per object with maintenance, for `utsim check`.
    pub fn maintenance_lines(&self) -> Vec<String> {
        self.objects
            .iter()
            .filter_map(|o| {
                let d = o.downtime.as_ref().filter(|d| d.has_maintenance())?;
                let mut parts = Vec::new();
                parts.push(match &d.wear {
                    Some(Wear::Weibull { shape, scale }) => format!(
                        "desgaste Weibull(forma {shape}, escala {scale}) por {}",
                        d.basis.label()
                    ),
                    None => format!("falhas pelo mtbf, por {}", d.basis.label()),
                });
                parts.push(match &d.repair_crew {
                    Some(t) => format!("reparo pela equipe '{t}'"),
                    None => "reparo sem equipe".to_string(),
                });
                parts.push(
                    match d.repair {
                        RepairEffect::AsBadAsOld => "corretiva mantém a idade",
                        RepairEffect::AsGoodAsNew => "corretiva zera a idade",
                        RepairEffect::Partial(_) => "corretiva reduz a idade",
                    }
                    .to_string(),
                );
                for p in &d.preventive {
                    let when = match &p.every {
                        Every::Hours(h) => format!("a cada {h} de uso"),
                        Every::Cycles(n) => format!("a cada {n} ciclos"),
                        Every::Clock(t) => format!("{t} depois da anterior"),
                        Every::Calendar(c) => format!("no calendário '{c}'"),
                    };
                    parts.push(format!("preventiva {when}"));
                }
                if let Some(c) = ideal_cycle(o) {
                    parts.push(format!("OEE com ciclo ideal {c:.4}"));
                }
                Some(format!("{}: {}", o.name, parts.join(", ")))
            })
            .collect()
    }
}
