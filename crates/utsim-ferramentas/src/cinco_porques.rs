//! Five whys, without any project state: chains of "why?" from a problem, with evidence at
//! each level and branches. A chain may start from a cause of an Ishikawa diagram; the
//! last why of each path is a candidate root cause, unless the team marked the root.

use crate::arvore::{forest, paths};

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

/// A why as saved: its parent is another why or the chain.
#[derive(Debug, Clone, PartialEq)]
pub struct WhyInput<I> {
    pub id: I,
    pub parent: Option<I>,
    pub text: String,
    pub evidence: String,
    pub root: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Why<I> {
    pub id: I,
    pub text: String,
    pub evidence: String,
    pub root: bool,
    /// 1 for the first why.
    pub level: usize,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Chain<I> {
    pub id: I,
    pub problem: String,
    /// The Ishikawa cause the chain starts from, with its text (empty if it was deleted).
    pub cause: Option<(I, String)>,
    /// Every why of the chain in depth-first order.
    pub whys: Vec<Why<I>>,
    /// Each path from the first why to a last one.
    pub paths: Vec<Vec<I>>,
}

impl<I: PartialEq> Chain<I> {
    /// The candidate root causes: the whys marked as root, or else the last why of each
    /// path.
    pub fn root_causes(&self) -> Vec<&Why<I>> {
        let marked: Vec<&Why<I>> = self.whys.iter().filter(|w| w.root).collect();
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

/// The chain `id` from the whys of the document (all chains', in sibling order): only the
/// whys under this chain are taken.
pub fn chain<I: Ord + Copy>(
    id: I,
    problem: &str,
    cause: Option<(I, String)>,
    whys: &[WhyInput<I>],
) -> Chain<I> {
    let mut mine: Vec<(I, Option<I>)> = Vec::new();
    let mut frontier = vec![id];
    while let Some(p) = frontier.pop() {
        for w in whys {
            if w.parent == Some(p) && !mine.iter().any(|(m, _)| *m == w.id) {
                let parent = (p != id).then_some(p);
                mine.push((w.id, parent));
                frontier.push(w.id);
            }
        }
    }
    // Keep sibling order.
    mine.sort_by_key(|(m, _)| whys.iter().position(|w| w.id == *m));
    let (nodes, _) = forest(&mine);
    let read = |wid: I, level: usize| {
        let w = whys
            .iter()
            .find(|w| w.id == wid)
            .expect("why of the inputs");
        Why {
            id: wid,
            text: w.text.clone(),
            evidence: w.evidence.clone(),
            root: w.root,
            level,
        }
    };
    Chain {
        id,
        problem: problem.to_string(),
        cause,
        whys: nodes.iter().map(|n| read(n.id, n.depth + 1)).collect(),
        paths: paths(&nodes),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn why(id: u32, parent: u32) -> WhyInput<u32> {
        WhyInput {
            id,
            parent: Some(parent),
            text: format!("porque {id}"),
            evidence: String::new(),
            root: false,
        }
    }

    #[test]
    fn branches_levels_and_root_causes() {
        // Chain 1: 10 → 11 → 12, and 10 → 13; chain 2 has 20.
        let whys = [
            why(10, 1),
            why(11, 10),
            why(12, 11),
            why(13, 10),
            why(20, 2),
        ];
        let c = chain(1, "Furo fora", None, &whys);
        assert_eq!(c.whys.len(), 4);
        assert_eq!(c.depth(), 3);
        assert_eq!(c.paths, [vec![10, 11, 12], vec![10, 13]]);
        let roots: Vec<u32> = c.root_causes().iter().map(|w| w.id).collect();
        assert_eq!(roots, [12, 13]);
        let mut marked = whys.to_vec();
        marked[1].root = true;
        let c = chain(1, "Furo fora", Some((7, "Gabarito".into())), &marked);
        assert_eq!(c.root_causes().len(), 1);
        assert_eq!(c.root_causes()[0].id, 11);
        assert_eq!(chain(2, "", None, &whys).whys.len(), 1);
    }
}
