//! Forests of items linked by a parent. Two people can reparent nodes at the same time and
//! close a cycle (A under B on one computer, B under A on the other); the forest breaks it
//! deterministically, so no node disappears from the screen, and reports it. Generic over
//! the identifier of the app (any `Ord + Copy` id).

use std::collections::{BTreeMap, BTreeSet};

/// A node in depth-first order.
#[derive(Debug, Clone, PartialEq)]
pub struct Node<I> {
    pub id: I,
    pub depth: usize,
    /// The parent as drawn (absent for a root, also for a node whose cycle was broken).
    pub parent: Option<I>,
}

/// Depth-first order of `nodes` given as `(id, parent)` in sibling order. Parents that are
/// not among the nodes make roots. Nodes left unreached (a cycle) become roots too, the
/// smallest id first; their ids are returned in the second vector.
pub fn forest<I: Ord + Copy>(nodes: &[(I, Option<I>)]) -> (Vec<Node<I>>, Vec<I>) {
    let known: BTreeSet<I> = nodes.iter().map(|(id, _)| *id).collect();
    let mut children: BTreeMap<Option<I>, Vec<I>> = BTreeMap::new();
    for (id, parent) in nodes {
        let p = parent.filter(|p| known.contains(p) && p != id);
        children.entry(p).or_default().push(*id);
    }
    let mut out = Vec::new();
    let mut seen = BTreeSet::new();
    let walk = |root: I, out: &mut Vec<Node<I>>, seen: &mut BTreeSet<I>| {
        let mut stack = vec![(root, 0usize, None)];
        while let Some((id, depth, parent)) = stack.pop() {
            if !seen.insert(id) {
                continue;
            }
            out.push(Node { id, depth, parent });
            if let Some(cs) = children.get(&Some(id)) {
                for c in cs.iter().rev() {
                    stack.push((*c, depth + 1, Some(id)));
                }
            }
        }
    };
    for r in children.get(&None).cloned().unwrap_or_default() {
        walk(r, &mut out, &mut seen);
    }
    let mut broken = Vec::new();
    while let Some(next) = known.iter().find(|id| !seen.contains(*id)).copied() {
        broken.push(next);
        walk(next, &mut out, &mut seen);
    }
    (out, broken)
}

/// Paths from each root to each leaf.
pub fn paths<I: Copy>(nodes: &[Node<I>]) -> Vec<Vec<I>> {
    let mut out = Vec::new();
    let mut current: Vec<I> = Vec::new();
    for (i, n) in nodes.iter().enumerate() {
        current.truncate(n.depth);
        current.push(n.id);
        let leaf = nodes.get(i + 1).is_none_or(|next| next.depth <= n.depth);
        if leaf {
            out.push(current.clone());
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn depth_first_with_orphans_as_roots() {
        let nodes = [
            (1u32, None),
            (2, Some(1)),
            (3, Some(2)),
            (4, Some(1)),
            (5, Some(99)),
        ];
        let (f, broken) = forest(&nodes);
        let order: Vec<(u32, usize)> = f.iter().map(|n| (n.id, n.depth)).collect();
        assert_eq!(order, [(1, 0), (2, 1), (3, 2), (4, 1), (5, 0)]);
        assert!(broken.is_empty());
        assert_eq!(paths(&f), [vec![1, 2, 3], vec![1, 4], vec![5]]);
    }

    #[test]
    fn cycles_are_broken_and_nothing_is_lost() {
        let nodes = [(1u32, Some(2)), (2, Some(1)), (3, Some(3))];
        let (f, broken) = forest(&nodes);
        assert_eq!(f.len(), 3);
        assert_eq!(broken, [1], "self parent counts as root");
        assert_eq!(f[0].id, 3);
        assert_eq!((f[1].id, f[1].depth), (1, 0));
        assert_eq!((f[2].id, f[2].depth), (2, 1));
    }
}
