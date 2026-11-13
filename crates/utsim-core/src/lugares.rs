//! The places of a waiting area (a table, a pallet, a shelf, a marked floor): which item
//! is where, and which items can be taken now. Pure and deterministic: the queue in the
//! kernel and the 3D view replay the same entries and exits and get the same places.
//!
//! The rule:
//! - an item that arrives takes the **first free place with support**, in the order of
//!   the list (the model lists them by shelf, layer, column from the exit and row from the
//!   access side); a place on top counts only when the one under it is taken;
//! - an item that leaves leaves its place **empty**: nothing is rearranged; the next one
//!   that arrives takes the first free place, which may be that one;
//! - an item is **free** when nothing is on top of it: only free items can be taken;
//! - with every place taken, an item waits **beside** the area (no place, [`Lugares::fora`]);
//!   when a place frees, the oldest of those goes into it, before any new arrival.

use std::collections::HashMap;
use std::hash::Hash;

use crate::item::ItemId;

/// One place: where the carrier stands to take an item from it (model coordinates), and
/// the place it rests on (`None`: on the surface).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Lugar {
    pub acesso: [f64; 3],
    pub abaixo: Option<usize>,
}

/// Who is in each place. `K` names the items: [`ItemId`] in the kernel, the trace's item
/// key in the 3D view.
#[derive(Clone, Debug)]
pub struct Lugares<K = ItemId> {
    lugares: Vec<Lugar>,
    /// The place on top of each one.
    acima: Vec<Option<usize>>,
    ocupante: Vec<Option<K>>,
    onde: HashMap<K, usize>,
    /// Items beside the area, the oldest first.
    fora: Vec<K>,
}

impl<K: Copy + Eq + Hash> Lugares<K> {
    pub fn new(lugares: Vec<Lugar>) -> Lugares<K> {
        let mut acima = vec![None; lugares.len()];
        for (k, l) in lugares.iter().enumerate() {
            if let Some(b) = l.abaixo.filter(|b| *b < acima.len()) {
                acima[b] = Some(k);
            }
        }
        let n = lugares.len();
        Lugares {
            lugares,
            acima,
            ocupante: vec![None; n],
            onde: HashMap::new(),
            fora: Vec::new(),
        }
    }

    /// Empties every place.
    pub fn clear(&mut self) {
        self.ocupante.iter_mut().for_each(|o| *o = None);
        self.onde.clear();
        self.fora.clear();
    }

    pub fn len(&self) -> usize {
        self.lugares.len()
    }

    pub fn is_empty(&self) -> bool {
        self.lugares.is_empty()
    }

    /// The places, in the order they are taken.
    pub fn lugares(&self) -> &[Lugar] {
        &self.lugares
    }

    /// The first free place with support whose supporting item may be covered.
    fn primeiro_livre(&self, pode_cobrir: &impl Fn(K) -> bool) -> Option<usize> {
        (0..self.lugares.len()).find(|&k| {
            self.ocupante[k].is_none()
                && match self.lugares[k].abaixo {
                    None => true,
                    Some(b) => self
                        .ocupante
                        .get(b)
                        .copied()
                        .flatten()
                        .is_some_and(pode_cobrir),
                }
        })
    }

    /// `item` arrives: its place, or `None` when it waits beside the area.
    pub fn entrar(&mut self, item: K) -> Option<usize> {
        self.entrar_com(item, |_| true)
    }

    /// As [`Lugares::entrar`], but never on top of an item `pode_cobrir` refuses (one
    /// already promised to a carrier, that must stay reachable).
    pub fn entrar_com(&mut self, item: K, pode_cobrir: impl Fn(K) -> bool) -> Option<usize> {
        match self.primeiro_livre(&pode_cobrir) {
            Some(k) => {
                self.ocupante[k] = Some(item);
                self.onde.insert(item, k);
                Some(k)
            }
            None => {
                self.fora.push(item);
                None
            }
        }
    }

    /// `item` left (taken, gave up, destroyed). Returns the item that moved from beside
    /// the area into a freed place, if any, with its place.
    pub fn sair(&mut self, item: K) -> Option<(K, usize)> {
        self.sair_com(item, |_| true)
    }

    /// As [`Lugares::sair`], with the same rule as [`Lugares::entrar_com`] for the item
    /// that moves in from beside the area.
    pub fn sair_com(&mut self, item: K, pode_cobrir: impl Fn(K) -> bool) -> Option<(K, usize)> {
        if let Some(at) = self.fora.iter().position(|i| *i == item) {
            self.fora.remove(at);
            return None;
        }
        let k = self.onde.remove(&item)?;
        self.ocupante[k] = None;
        if self.fora.is_empty() {
            return None;
        }
        let k = self.primeiro_livre(&pode_cobrir)?;
        let next = self.fora.remove(0);
        self.ocupante[k] = Some(next);
        self.onde.insert(next, k);
        Some((next, k))
    }

    /// The place of `item` (`None`: beside the area, or not here).
    pub fn lugar_de(&self, item: K) -> Option<usize> {
        self.onde.get(&item).copied()
    }

    /// Where the carrier stands to take `item`.
    pub fn acesso(&self, item: K) -> Option<[f64; 3]> {
        self.lugar_de(item).map(|k| self.lugares[k].acesso)
    }

    /// Whether `item` can be taken: nothing on top of it (an item beside the area is
    /// free too).
    pub fn livre(&self, item: K) -> bool {
        match self.lugar_de(item) {
            Some(k) => self.acima[k].is_none_or(|a| self.ocupante[a].is_none()),
            None => self.fora.contains(&item),
        }
    }

    /// The item under `item`, if any (it is covered by `item`).
    pub fn sob(&self, item: K) -> Option<K> {
        let k = self.lugar_de(item)?;
        self.ocupante[self.lugares[k].abaixo?]
    }

    /// Items waiting beside the area, the oldest first.
    pub fn fora(&self) -> &[K] {
        &self.fora
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn id(k: u64) -> u64 {
        k
    }

    /// Two columns, two layers: 0 and 1 on the surface, 2 on 0, 3 on 1.
    fn mesa() -> Lugares<u64> {
        let l = |x: f64, abaixo| Lugar {
            acesso: [x, -1.0, 0.0],
            abaixo,
        };
        Lugares::new(vec![
            l(0.5, None),
            l(-0.5, None),
            l(0.5, Some(0)),
            l(-0.5, Some(1)),
        ])
    }

    #[test]
    fn items_take_the_first_free_place_with_support_and_leave_a_gap() {
        let mut m = mesa();
        assert_eq!(m.entrar(id(1)), Some(0));
        assert_eq!(m.entrar(id(2)), Some(1));
        // The second layer only once the first is full.
        assert_eq!(m.entrar(id(3)), Some(2));
        assert!(!m.livre(id(1)) && m.livre(id(3)) && m.livre(id(2)));
        assert_eq!(m.sob(id(3)), Some(id(1)));
        // 2 leaves: its place stays empty, nothing moves; 4 takes it (the first free).
        assert_eq!(m.sair(id(2)), None);
        assert_eq!(m.lugar_de(id(3)), Some(2));
        assert_eq!(m.entrar(id(4)), Some(1));
        assert_eq!(m.acesso(id(4)), Some([-0.5, -1.0, 0.0]));
    }

    #[test]
    fn a_full_area_keeps_items_beside_it_and_fills_the_freed_place_first() {
        let mut m = mesa();
        for k in 1..=4 {
            m.entrar(id(k));
        }
        assert_eq!(m.entrar(id(5)), None);
        assert_eq!(m.entrar(id(6)), None);
        assert!(m.livre(id(5)) && m.acesso(id(5)).is_none());
        // The top of column 0 leaves: the oldest beside the area goes there.
        assert_eq!(m.sair(id(3)), Some((id(5), 2)));
        assert_eq!(m.fora(), &[id(6)]);
        // One beside the area that leaves just goes.
        assert_eq!(m.sair(id(6)), None);
        assert!(m.fora().is_empty());
        // Emptied, the first place is taken again.
        m.clear();
        assert_eq!(m.entrar(id(7)), Some(0));
        assert_eq!(m.sair(id(7)), None);
        assert_eq!(m.entrar(id(8)), Some(0));
    }

    #[test]
    fn an_item_promised_to_a_carrier_is_never_covered() {
        let mut m = mesa();
        m.entrar(id(1));
        m.entrar(id(2));
        // 1 is promised: 3 goes on 2 instead.
        assert_eq!(m.entrar_com(id(3), |i| i != id(1)), Some(3));
        // Neither may be covered: 4 waits beside the area.
        assert_eq!(m.entrar_com(id(4), |i| i != id(1) && i != id(2)), None);
    }
}
