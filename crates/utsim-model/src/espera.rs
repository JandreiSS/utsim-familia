//! The kind of a real waiting area (a table, a pallet, a shelf, a marked floor) and where
//! its places are: the grid each kind lays the items in, the same for the simulation (the
//! place each item takes and where the carrier stands to take it,
//! [`utsim_core::lugares`]) and for the 3D view (where each item is drawn).
//!
//! The numbers of each kind are those of its 3D element in `assets/modelos3d/catalogo.json`
//! (`espera`, `palete`, `estante_espera`, `espera_chao`): the free rectangle of the surface
//! (`dados.area`), its height (`dados.tampo`, the shelves' `altura`) and the element's
//! size. A `size` on the object stretches the free rectangle by the same amount the object
//! grows, and its surfaces in proportion to its height (the 3D view stretches the element
//! to the `size`).

use serde::{Deserialize, Serialize};

/// Edge of the item drawn on a waiting area (the scene's item cube), metres.
pub const ITEM: f64 = 0.3;
/// Gap between two items of a grid, as a fraction of the item.
const GAP: f64 = 0.1;

/// The kind of waiting area.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum WaitingKind {
    /// A table: the items on its top, two layers.
    #[default]
    Table,
    /// A pallet on the floor: the items stacked on it, three layers.
    Pallet,
    /// A shelf: one layer on each of its three shelves, the lowest first.
    Shelf,
    /// A marked area on the floor: the items on the floor, two layers.
    Floor,
}

/// A real waiting area on a `Queue` (`waiting_area: Some((kind: Pallet))`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct WaitingArea {
    pub kind: WaitingKind,
    /// Layers on each surface (absent: the kind's).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub layers: Option<u32>,
    /// Distance between the centres of two neighbouring items, metres (absent: the item
    /// plus 10%).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pitch: Option<f64>,
}

/// One place, in the object's own frame (metres, `x` towards the exit, `-y` the access
/// side): the centre of the item's base, and the place it rests on.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Place {
    pub center: [f64; 3],
    pub below: Option<usize>,
}

struct Spec {
    /// Size of the element (x, y, z).
    size: [f64; 3],
    /// Free rectangle of each surface (x, y), centred.
    area: [f64; 2],
    /// Height of each surface, the lowest first.
    surfaces: &'static [f64],
    layers: u32,
    element: &'static str,
}

fn spec(kind: WaitingKind) -> Spec {
    match kind {
        WaitingKind::Table => Spec {
            size: [1.4, 1.0, 0.82],
            area: [1.1, 0.66],
            surfaces: &[0.76],
            layers: 2,
            element: "espera",
        },
        WaitingKind::Pallet => Spec {
            size: [1.2, 1.0, 0.144],
            area: [1.16, 0.96],
            surfaces: &[0.144],
            layers: 3,
            element: "palete",
        },
        WaitingKind::Shelf => Spec {
            size: [1.2, 0.6, 1.65],
            area: [1.1, 0.52],
            surfaces: &[0.30, 0.75, 1.20],
            layers: 1,
            element: "estante_espera",
        },
        WaitingKind::Floor => Spec {
            size: [1.3, 1.1, 0.01],
            area: [1.2, 1.0],
            surfaces: &[0.0],
            layers: 2,
            element: "espera_chao",
        },
    }
}

impl WaitingKind {
    pub const ALL: [WaitingKind; 4] = [
        WaitingKind::Table,
        WaitingKind::Pallet,
        WaitingKind::Shelf,
        WaitingKind::Floor,
    ];

    /// The name in the model file.
    pub fn name(self) -> &'static str {
        match self {
            WaitingKind::Table => "Table",
            WaitingKind::Pallet => "Pallet",
            WaitingKind::Shelf => "Shelf",
            WaitingKind::Floor => "Floor",
        }
    }

    pub fn by_name(name: &str) -> Option<WaitingKind> {
        WaitingKind::ALL.into_iter().find(|k| k.name() == name)
    }
}

impl WaitingArea {
    /// The 3D element it is drawn with (`assets/modelos3d`).
    pub fn element(&self) -> &'static str {
        spec(self.kind).element
    }

    /// Size of the object when it has no `size`.
    pub fn default_size(&self) -> [f64; 3] {
        spec(self.kind).size
    }

    /// Layers on each surface.
    pub fn layers(&self) -> u32 {
        self.layers.unwrap_or(spec(self.kind).layers).max(1)
    }

    /// Distance between two neighbouring items.
    pub fn pitch(&self) -> f64 {
        self.pitch
            .filter(|p| p.is_finite() && *p > 0.0)
            .unwrap_or(ITEM * (1.0 + GAP))
    }

    /// The free rectangle of each surface for an object of `size` (absent: the kind's).
    pub fn area(&self, size: Option<[f64; 3]>) -> [f64; 2] {
        let s = spec(self.kind);
        let Some(size) = size else {
            return s.area;
        };
        std::array::from_fn(|k| (size[k] - (s.size[k] - s.area[k])).max(ITEM))
    }

    /// The places, in the order they are taken: surface by surface from the lowest, layer
    /// by layer, column by column from the exit (`+x`) and row by row from the access side
    /// (`-y`). A place above another rests on it.
    pub fn places(&self, size: Option<[f64; 3]>) -> Vec<Place> {
        let s = spec(self.kind);
        let area = self.area(size);
        let pitch = self.pitch();
        let n = |l: f64| ((l / pitch + 1e-9).floor() as usize).max(1);
        let (cols, rows) = (n(area[0]), n(area[1]));
        let per_layer = cols * rows;
        // A taller or shorter object (its `size` z) carries its surfaces with it.
        let k = match size {
            Some(d) if s.size[2] > 0.05 && d[2].is_finite() && d[2] > 0.0 => d[2] / s.size[2],
            _ => 1.0,
        };
        let mut out = Vec::new();
        for &h in s.surfaces {
            let h = h * k;
            let first = out.len();
            for layer in 0..self.layers() as usize {
                for col in 0..cols {
                    for row in 0..rows {
                        let x = area[0] / 2.0 - area[0] / cols as f64 * (col as f64 + 0.5);
                        let y = -area[1] / 2.0 + area[1] / rows as f64 * (row as f64 + 0.5);
                        let below =
                            (layer > 0).then(|| first + (layer - 1) * per_layer + col * rows + row);
                        out.push(Place {
                            center: [x, y, h + layer as f64 * ITEM],
                            below,
                        });
                    }
                }
            }
        }
        out
    }

    /// What is wrong with it, user-facing text.
    pub fn problem(&self) -> Option<String> {
        if let Some(p) = self.pitch
            && !(p.is_finite() && p >= ITEM)
        {
            return Some(format!(
                "waiting_area.pitch: precisa ser pelo menos {ITEM} m (o tamanho da peça)"
            ));
        }
        if self.layers.is_some_and(|l| !(1..=20).contains(&l)) {
            return Some("waiting_area.layers: de 1 a 20 camadas".into());
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn area(kind: WaitingKind) -> WaitingArea {
        WaitingArea {
            kind,
            ..Default::default()
        }
    }

    #[test]
    fn each_kind_lays_its_grid() {
        // Table: 3 along, 2 across, two layers on its top.
        let t = area(WaitingKind::Table).places(None);
        assert_eq!(t.len(), 12);
        assert!((t[0].center[2] - 0.76).abs() < 1e-12 && t[0].below.is_none());
        // The oldest at the exit (+x), the access side (-y) first.
        assert!(t[0].center[0] > 0.3 && t[0].center[1] < 0.0);
        assert!(t[1].center[1] > 0.0 && t[2].center[0].abs() < 1e-12);
        // The second layer rests on the first.
        assert_eq!(t[6].below, Some(0));
        assert!((t[6].center[2] - 1.06).abs() < 1e-12);
        // Pallet: 3 × 2, three layers. Shelf: 3 × 1 on each of three shelves.
        assert_eq!(area(WaitingKind::Pallet).places(None).len(), 18);
        let s = area(WaitingKind::Shelf).places(None);
        assert_eq!(s.len(), 9);
        assert!(s.iter().all(|p| p.below.is_none()));
        assert!((s[8].center[2] - 1.2).abs() < 1e-12);
        // Floor: 3 × 3, two layers, on the floor.
        let f = area(WaitingKind::Floor).places(None);
        assert_eq!((f.len(), f[0].center[2]), (18, 0.0));
        // Every item inside its free rectangle.
        for k in WaitingKind::ALL {
            let a = area(k);
            let r = a.area(None);
            for p in a.places(None) {
                assert!(
                    p.center[0].abs() + ITEM / 2.0 <= r[0] / 2.0 + 1e-9,
                    "{k:?} {p:?}"
                );
                assert!(
                    p.center[1].abs() + ITEM / 2.0 <= r[1] / 2.0 + 1e-9,
                    "{k:?} {p:?}"
                );
            }
        }
    }

    #[test]
    fn a_bigger_object_has_more_places_and_its_own_options() {
        let t = area(WaitingKind::Table);
        // 2.4 × 1.0: the free rectangle grows by 1 m along x.
        assert_eq!(t.area(Some([2.4, 1.0, 0.82])), [2.1, 0.66]);
        assert_eq!(t.places(Some([2.4, 1.0, 0.82])).len(), 6 * 2 * 2);
        let one = WaitingArea {
            layers: Some(1),
            pitch: Some(0.5),
            ..t
        };
        assert_eq!(one.places(None).len(), 2);
        assert!(one.problem().is_none());
        assert!(
            WaitingArea {
                pitch: Some(0.1),
                ..t
            }
            .problem()
            .is_some()
        );
        assert!(
            WaitingArea {
                layers: Some(0),
                ..t
            }
            .problem()
            .is_some()
        );
    }
}
