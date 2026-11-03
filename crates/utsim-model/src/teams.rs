//! Results per team (`docs/plano_chegadas_e_equipes_3d.md`, E8a): what each dispatcher
//! and its members did in one replication, read from the report.

use serde::{Deserialize, Serialize};
use utsim_core::RunReport;

use crate::{Model, ObjectKind};

/// One member of a team in one replication.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct MemberSummary {
    pub name: String,
    /// Tasks the dispatcher handed to it (needs `metrics: true`; else 0).
    pub requests: f64,
    /// Fraction of the window between an assignment and its idle report, as the
    /// dispatcher sees it (needs `metrics: true`; else 0).
    pub busy: f64,
    /// Utilisation from the member's own states (travelling, loading, operating...).
    pub utilization: f64,
    /// Fractions of the window travelling empty and loaded.
    pub travel_empty: f64,
    pub travel_loaded: f64,
}

/// A dispatcher and its members in one replication.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct TeamSummary {
    pub dispatcher: String,
    /// Requests received (needs `metrics: true`; else 0).
    pub requests: f64,
    /// Mean and largest wait for a member (request to assignment; `metrics: true`).
    pub wait_mean: Option<f64>,
    pub wait_max: Option<f64>,
    /// Members that gave their work back (with `preempt: Empty`; else 0).
    pub preemptions: f64,
    pub members: Vec<MemberSummary>,
}

impl Model {
    /// One summary per dispatcher, from a replication's report.
    pub fn team_summaries(&self, report: &RunReport) -> Vec<TeamSummary> {
        self.objects
            .iter()
            .filter_map(|o| match &o.kind {
                ObjectKind::Dispatcher { team, .. } => Some((o.name.as_str(), team)),
                _ => None,
            })
            .map(|(name, team)| {
                let d = report.block(name);
                let custom = |key: &str| {
                    d.and_then(|b| b.custom.get(key).copied())
                        .unwrap_or_default()
                };
                let wait = d.and_then(|b| b.extra.get("request_wait"));
                let members = team
                    .iter()
                    .map(|m| {
                        let b = report.block(m);
                        let state =
                            |s: &str| b.and_then(|b| b.states.get(s).copied()).unwrap_or_default();
                        MemberSummary {
                            name: m.clone(),
                            requests: custom(&format!("member.{m}.requests")),
                            busy: custom(&format!("member.{m}.busy")),
                            utilization: b.and_then(|b| b.utilization()).map_or(0.0, |u| u.work),
                            travel_empty: state("travel_empty"),
                            travel_loaded: state("travel_loaded"),
                        }
                    })
                    .collect();
                TeamSummary {
                    dispatcher: name.to_string(),
                    requests: custom("requests.count"),
                    wait_mean: wait.filter(|w| w.count > 0).map(|w| w.mean),
                    wait_max: wait.filter(|w| w.count > 0).map(|w| w.max),
                    preemptions: custom("preemptions.count"),
                    members,
                }
            })
            .collect()
    }
}
