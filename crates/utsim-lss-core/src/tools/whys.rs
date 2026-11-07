//! Five whys (`docs/plano_lean_seis_sigma.md`, 2.3): chains of "why?" from a problem, with
//! evidence at each level and branches. A chain may start from a cause of an Ishikawa
//! diagram of the project; the last why of each path is a candidate root cause. The rules
//! are in `utsim_ferramentas::cinco_porques` (decision G3); this module reads the journal.

use super::{ItemRef, items};
use crate::id::Id;
use crate::state::State;

pub use utsim_ferramentas::cinco_porques::{CHAIN, WHY, fields};

pub type Why = utsim_ferramentas::cinco_porques::Why<Id>;
pub type Chain = utsim_ferramentas::cinco_porques::Chain<Id>;

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
    let inputs: Vec<utsim_ferramentas::cinco_porques::WhyInput<Id>> = whys
        .iter()
        .map(|w| utsim_ferramentas::cinco_porques::WhyInput {
            id: w.id,
            parent: w.parent(),
            text: w.text(fields::TEXT),
            evidence: w.text(fields::EVIDENCE),
            root: w.entity.boolean(fields::ROOT),
        })
        .collect();
    let cause = c.entity.id(fields::CAUSE).map(|id| {
        let text = causes
            .iter()
            .find(|(c, _)| *c == id)
            .map(|(_, t)| t.clone())
            .unwrap_or_default();
        (id, text)
    });
    utsim_ferramentas::cinco_porques::chain(c.id, &c.text(fields::TEXT), cause, &inputs)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tools::Tool;
    use crate::tools::ishikawa;
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
