//! Voice of the customer and CTQ tree (`docs/plano_lean_seis_sigma.md`, 2.1): what the
//! customers said (source, segment), grouped into needs (with the optional Kano class),
//! broken down into drivers and these into measurable CTQs (operational definition, unit,
//! lower and upper specification limits, target).

use serde::{Deserialize, Serialize};

use super::{ItemRef, items};
use crate::id::Id;
use crate::state::State;

/// A statement of a customer; its parent is the need it was grouped into.
pub const STATEMENT: &str = "fala";
pub const NEED: &str = "necessidade";
/// Parent: a need.
pub const DRIVER: &str = "direcionador";
/// Parent: a driver.
pub const CTQ: &str = "ctq";

pub mod fields {
    pub const TEXT: &str = "texto";
    pub const SOURCE: &str = "fonte";
    pub const SEGMENT: &str = "segmento";
    pub const KANO: &str = "kano";
    pub const DEFINITION: &str = "definicao_operacional";
    pub const UNIT: &str = "unidade";
    pub const LSL: &str = "lie";
    pub const USL: &str = "lse";
    pub const TARGET: &str = "meta";
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Kano {
    #[serde(rename = "obrigatorio")]
    MustBe,
    #[serde(rename = "desempenho")]
    Performance,
    #[serde(rename = "atrativo")]
    Attractive,
}

impl Kano {
    pub const ALL: [Kano; 3] = [Kano::MustBe, Kano::Performance, Kano::Attractive];
}

#[derive(Debug, Clone, PartialEq)]
pub struct Ctq {
    pub id: Id,
    pub name: String,
    pub definition: String,
    pub unit: String,
    pub lsl: Option<f64>,
    pub usl: Option<f64>,
    pub target: Option<f64>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Driver {
    pub id: Id,
    pub text: String,
    pub ctqs: Vec<Ctq>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Need {
    pub id: Id,
    pub text: String,
    pub kano: Option<Kano>,
    pub statements: Vec<Id>,
    pub drivers: Vec<Driver>,
}

/// A problem of the tree, shown next to the item.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Issue {
    /// A CTQ without unit.
    NoUnit(Id),
    /// A CTQ without any specification limit.
    NoLimit(Id),
    /// LSL at or above USL.
    LimitsReversed(Id),
    /// The target outside the limits.
    TargetOutside(Id),
    /// A need not broken down into drivers.
    NeedWithoutDriver(Id),
    /// A driver without CTQ.
    DriverWithoutCtq(Id),
    /// A statement not grouped into a need.
    Ungrouped(Id),
}

#[derive(Debug, Clone, PartialEq)]
pub struct Tree {
    pub needs: Vec<Need>,
    /// Statements not grouped yet.
    pub ungrouped: Vec<Id>,
    /// Drivers and CTQs whose parent is missing (deleted, or of the wrong level).
    pub orphans: Vec<Id>,
}

impl Ctq {
    fn read(i: &ItemRef) -> Self {
        use fields as f;
        Self {
            id: i.id,
            name: i.text(f::TEXT),
            definition: i.text(f::DEFINITION),
            unit: i.text(f::UNIT),
            lsl: i.entity.number(f::LSL),
            usl: i.entity.number(f::USL),
            target: i.entity.number(f::TARGET),
        }
    }

    pub fn issues(&self) -> Vec<Issue> {
        let mut out = Vec::new();
        if self.unit.trim().is_empty() {
            out.push(Issue::NoUnit(self.id));
        }
        if self.lsl.is_none() && self.usl.is_none() {
            out.push(Issue::NoLimit(self.id));
        }
        if let (Some(l), Some(u)) = (self.lsl, self.usl) {
            if l >= u {
                out.push(Issue::LimitsReversed(self.id));
            }
        }
        if let Some(t) = self.target {
            if self.lsl.is_some_and(|l| t < l) || self.usl.is_some_and(|u| t > u) {
                out.push(Issue::TargetOutside(self.id));
            }
        }
        out
    }
}

/// The tree of a deliverable. The levels are fixed (need → driver → CTQ), so a parent link
/// of the wrong level cannot make a cycle: the item becomes an orphan.
pub fn tree(state: &State, deliverable: Id) -> Tree {
    let statements = items(state, deliverable, STATEMENT);
    let needs_i = items(state, deliverable, NEED);
    let drivers_i = items(state, deliverable, DRIVER);
    let ctqs_i = items(state, deliverable, CTQ);
    let need_ids: Vec<Id> = needs_i.iter().map(|n| n.id).collect();
    let driver_ids: Vec<Id> = drivers_i.iter().map(|n| n.id).collect();
    let needs = needs_i
        .iter()
        .map(|n| Need {
            id: n.id,
            text: n.text(fields::TEXT),
            kano: n.entity.typed(fields::KANO),
            statements: statements
                .iter()
                .filter(|s| s.parent() == Some(n.id))
                .map(|s| s.id)
                .collect(),
            drivers: drivers_i
                .iter()
                .filter(|d| d.parent() == Some(n.id))
                .map(|d| Driver {
                    id: d.id,
                    text: d.text(fields::TEXT),
                    ctqs: ctqs_i
                        .iter()
                        .filter(|c| c.parent() == Some(d.id))
                        .map(Ctq::read)
                        .collect(),
                })
                .collect(),
        })
        .collect();
    let ungrouped = statements
        .iter()
        .filter(|s| s.parent().is_none_or(|p| !need_ids.contains(&p)))
        .map(|s| s.id)
        .collect();
    let orphans = drivers_i
        .iter()
        .filter(|d| d.parent().is_none_or(|p| !need_ids.contains(&p)))
        .chain(
            ctqs_i
                .iter()
                .filter(|c| c.parent().is_none_or(|p| !driver_ids.contains(&p))),
        )
        .map(|i| i.id)
        .collect();
    Tree {
        needs,
        ungrouped,
        orphans,
    }
}

impl Tree {
    pub fn ctqs(&self) -> impl Iterator<Item = &Ctq> {
        self.needs
            .iter()
            .flat_map(|n| n.drivers.iter().flat_map(|d| d.ctqs.iter()))
    }

    /// Every problem of the tree.
    pub fn issues(&self) -> Vec<Issue> {
        let mut out: Vec<Issue> = self
            .ungrouped
            .iter()
            .map(|s| Issue::Ungrouped(*s))
            .collect();
        for n in &self.needs {
            if n.drivers.is_empty() {
                out.push(Issue::NeedWithoutDriver(n.id));
            }
            for d in &n.drivers {
                if d.ctqs.is_empty() {
                    out.push(Issue::DriverWithoutCtq(d.id));
                }
                for c in &d.ctqs {
                    out.extend(c.issues());
                }
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::enum_value;
    use crate::tools::Tool;
    use crate::tools::testing::Fixture;

    #[test]
    fn tree_and_checks() {
        let mut f = Fixture::new();
        let d = f.deliverable(Tool::Voc);
        let s1 = f.item(
            d,
            STATEMENT,
            None,
            vec![
                (fields::TEXT, "O pedido sempre chega atrasado".into()),
                (fields::SOURCE, "SAC".into()),
            ],
        );
        let need = f.item(
            d,
            NEED,
            None,
            vec![
                (fields::TEXT, "Entrega no prazo".into()),
                (fields::KANO, enum_value(Kano::MustBe)),
            ],
        );
        f.set(s1, crate::tools::fields::item::PARENT, need.to_string());
        let loose = f.item(
            d,
            STATEMENT,
            None,
            vec![(fields::TEXT, "Embalagem frágil".into())],
        );
        let drv = f.item(
            d,
            DRIVER,
            Some(need),
            vec![(fields::TEXT, "Tempo de separação".into())],
        );
        let ok = f.item(
            d,
            CTQ,
            Some(drv),
            vec![
                (fields::TEXT, "Lead time do pedido".into()),
                (fields::UNIT, "h".into()),
                (fields::USL, 48.into()),
                (fields::TARGET, 24.into()),
            ],
        );
        let bad = f.item(
            d,
            CTQ,
            Some(drv),
            vec![
                (fields::TEXT, "Peso".into()),
                (fields::LSL, 5.into()),
                (fields::USL, 2.into()),
                (fields::TARGET, 9.into()),
            ],
        );
        let empty_need = f.item(d, NEED, None, vec![]);
        let orphan = f.item(d, CTQ, Some(need), vec![]);
        let t = tree(&f.state, d);
        assert_eq!(t.needs.len(), 2);
        assert_eq!(t.needs[0].statements, [s1]);
        assert_eq!(t.needs[0].kano, Some(Kano::MustBe));
        assert_eq!(t.ctqs().count(), 2);
        assert_eq!(t.ungrouped, [loose]);
        assert_eq!(
            t.orphans,
            [orphan],
            "CTQ under a need is of the wrong level"
        );
        let issues = t.issues();
        assert!(t.ctqs().find(|c| c.id == ok).unwrap().issues().is_empty());
        for i in [
            Issue::Ungrouped(loose),
            Issue::NoUnit(bad),
            Issue::LimitsReversed(bad),
            Issue::TargetOutside(bad),
            Issue::NeedWithoutDriver(empty_need),
        ] {
            assert!(issues.contains(&i), "{i:?}");
        }
        assert!(!issues.contains(&Issue::NoLimit(bad)));
        let no_limit = Ctq {
            id: ok,
            name: String::new(),
            definition: String::new(),
            unit: "mm".into(),
            lsl: None,
            usl: None,
            target: None,
        };
        assert_eq!(no_limit.issues(), [Issue::NoLimit(ok)]);
    }
}
