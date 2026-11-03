//! Five whys (`docs/plano_lean_seis_sigma.md`, 2.3): chains of "why?" from a problem, with
//! evidence at each level and branches. A chain may start from a cause of an Ishikawa
//! diagram of the project; the last why of each path is a candidate root cause.

use super::tree::{forest, paths};
use super::{ItemRef, ishikawa, items};
use crate::id::Id;
use crate::state::State;

/// The start of a chain (the problem, or an Ishikawa cause).
pub const CHAIN: &str = "cadeia";
/// One answer to "why?"; its parent is the previous why or the chain.
pub const WHY: &str = "porque";

pub mod fields {
    pub const TEXT: &str = "texto";
    pub const EVIDENCE: &str = "evidencia";
    /// Chain: the Ishikawa cause it starts from.
    pub const CAUSE: &str = "causa_ishikawa";
    /// Why: marked by the team as the root cause.
    pub const ROOT: &str = "causa_raiz";
}

#[derive(Debug, Clone, PartialEq)]
pub struct Why {
    pub id: Id,
    pub text: String,
    pub evidence: String,
    pub root: bool,
    /// 1 for the first why.
    pub level: usize,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Chain {
    pub id: Id,
    pub problem: String,
    /// The Ishikawa cause the chain starts from, with its text (empty if it was deleted).
    pub cause: Option<(Id, String)>,
    /// Every why of the chain in depth-first order.
    pub whys: Vec<Why>,
    /// Each path from the first why to a last one.
    pub paths: Vec<Vec<Id>>,
}

impl Chain {
    /// The candidate root causes: the whys marked as root, or else the last why of each
    /// path.
    pub fn root_causes(&self) -> Vec<&Why> {
        let marked: Vec<&Why> = self.whys.iter().filter(|w| w.root).collect();
        if !marked.is_empty() {
            return marked;
        }
        self.paths
            .iter()
            .filter_map(|p| p.last())
            .filter_map(|id| self.whys.iter().find(|w| w.id == *id))
            .collect()
    }

    /// Deepest level reached (5 is the usual target, not a rule).
    pub fn depth(&self) -> usize {
        self.whys.iter().map(|w| w.level).max().unwrap_or(0)
    }
}

/// The chains of a deliverable.
pub fn chains(state: &State, deliverable: Id) -> Vec<Chain> {
    let whys = items(state, deliverable, WHY);
    let causes = super::ishikawa::project_causes_by_id(state);
    items(state, deliverable, CHAIN)
        .iter()
        .map(|c| chain(c, &whys, &causes))
        .collect()
}

fn chain(c: &ItemRef, whys: &[ItemRef], causes: &[(Id, String)]) -> Chain {
    // Whys under this chain: walk down from the chain.
    let mut mine: Vec<(Id, Option<Id>)> = Vec::new();
    let mut frontier = vec![c.id];
    while let Some(p) = frontier.pop() {
        for w in whys {
            if w.parent() == Some(p) && !mine.iter().any(|(id, _)| *id == w.id) {
                let parent = (p != c.id).then_some(p);
                mine.push((w.id, parent));
                frontier.push(w.id);
            }
        }
    }
    // Keep sibling order (items are sorted by `ordem`).
    mine.sort_by_key(|(id, _)| whys.iter().position(|w| w.id == *id));
    let (nodes, _) = forest(&mine);
    let read = |id: Id, level: usize| {
        let w = whys.iter().find(|w| w.id == id).expect("why of the items");
        Why {
            id,
            text: w.text(fields::TEXT),
            evidence: w.text(fields::EVIDENCE),
            root: w.entity.boolean(fields::ROOT),
            level,
        }
    };
    let cause = c.entity.id(fields::CAUSE).map(|id| {
        let text = causes
            .iter()
            .find(|(c, _)| *c == id)
            .map(|(_, t)| t.clone())
            .unwrap_or_default();
        (id, text)
    });
    Chain {
        id: c.id,
        problem: c.text(fields::TEXT),
        cause,
        whys: nodes.iter().map(|n| read(n.id, n.depth + 1)).collect(),
        paths: paths(&nodes),
    }
}

impl ishikawa::Cause {
    /// Text shown for the cause in pickers.
    pub fn label(&self) -> String {
        format!("{}: {}", ishikawa::category_name(&self.category), self.text)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tools::Tool;
    use crate::tools::testing::Fixture;

    #[test]
    fn chains_with_branches_and_ishikawa_link() {
        let mut f = Fixture::new();
        let ish = f.deliverable(Tool::Ishikawa);
        let cause = f.item(
            ish,
            ishikawa::ITEM,
            None,
            vec![(ishikawa::fields::TEXT, "Ferramenta gasta".into())],
        );
        let d = f.deliverable(Tool::FiveWhys);
        let c = f.item(
            d,
            CHAIN,
            None,
            vec![
                (fields::TEXT, "Rebarba na peça".into()),
                (fields::CAUSE, cause.to_string().into()),
            ],
        );
        let w1 = f.item(
            d,
            WHY,
            Some(c),
            vec![(fields::TEXT, "Ferramenta gasta".into())],
        );
        let w2 = f.item(
            d,
            WHY,
            Some(w1),
            vec![(fields::TEXT, "Troca só quando quebra".into())],
        );
        let w3a = f.item(
            d,
            WHY,
            Some(w2),
            vec![(fields::TEXT, "Sem plano de troca".into())],
        );
        let w3b = f.item(
            d,
            WHY,
            Some(w2),
            vec![(fields::TEXT, "Sem contador de peças".into())],
        );
        let other = f.item(d, CHAIN, None, vec![(fields::TEXT, "Atraso".into())]);
        let ch = chains(&f.state, d);
        assert_eq!(ch.len(), 2);
        let first = &ch[0];
        assert_eq!(first.cause, Some((cause, "Ferramenta gasta".into())));
        let levels: Vec<usize> = first.whys.iter().map(|w| w.level).collect();
        assert_eq!(levels, [1, 2, 3, 3]);
        assert_eq!(first.paths, [vec![w1, w2, w3a], vec![w1, w2, w3b]]);
        let roots: Vec<Id> = first.root_causes().iter().map(|w| w.id).collect();
        assert_eq!(roots, [w3a, w3b]);
        assert_eq!(first.depth(), 3);
        f.set(w3b, fields::ROOT, true);
        let roots: Vec<Id> = chains(&f.state, d)[0]
            .root_causes()
            .iter()
            .map(|w| w.id)
            .collect();
        assert_eq!(roots, [w3b], "marked root wins");
        assert_eq!(ch[1].id, other);
        assert!(ch[1].whys.is_empty() && ch[1].root_causes().is_empty());
        // The cause deleted: the link stays, without text.
        f.set(cause, crate::op::FIELD_DELETED, true);
        assert_eq!(chains(&f.state, d)[0].cause, Some((cause, String::new())));
    }
}
