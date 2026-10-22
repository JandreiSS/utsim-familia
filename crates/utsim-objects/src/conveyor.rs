use utsim_core::{Behavior, Ctx, Item};

/// Kinematic conveyor (phase 5, contract): see `docs/arquitetura.md`, "Esteira".
/// The conveyor front implements the behaviour; until then a run reports an error.
pub struct Conveyor {
    pub length: f64,
    pub speed: f64,
    pub item_length: f64,
    pub accumulating: bool,
}

impl Conveyor {
    pub fn new(length: f64, speed: f64, item_length: f64, accumulating: bool) -> Self {
        Conveyor {
            length,
            speed,
            item_length,
            accumulating,
        }
    }

    /// Items that fit on the belt at once.
    pub fn capacity(&self) -> usize {
        (self.length / self.item_length).floor() as usize
    }
}

impl Behavior for Conveyor {
    fn kind(&self) -> &'static str {
        "conveyor"
    }

    fn reset(&mut self, cx: &mut Ctx) {
        cx.error("esteira (Conveyor): em construção na fase 5");
    }

    fn accepts(&self, _item: &Item, content: usize, _port: usize) -> bool {
        content < self.capacity()
    }
}
