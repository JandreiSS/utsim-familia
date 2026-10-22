use serde::{Deserialize, Serialize};
use utsim_core::{Behavior, Ctx, Dist, Item};

/// Which free position a rack gives to an incoming item.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum RackPlacement {
    /// Lowest level first, then lowest bay.
    #[default]
    FirstFree,
    /// Uniformly among free positions (block's random stream).
    Random,
}

/// Addressable storage (phase 5, contract): see `docs/arquitetura.md`, "Porta-paletes".
/// The rack front implements the behaviour; until then a run reports an error.
pub struct Rack {
    pub bays: usize,
    pub levels: usize,
    pub dwell: Option<Dist>,
    pub placement: RackPlacement,
}

impl Rack {
    pub fn new(bays: usize, levels: usize, dwell: Option<Dist>, placement: RackPlacement) -> Self {
        Rack {
            bays,
            levels,
            dwell,
            placement,
        }
    }
}

impl Behavior for Rack {
    fn kind(&self) -> &'static str {
        "rack"
    }

    fn reset(&mut self, cx: &mut Ctx) {
        cx.error("porta-paletes (Rack): em construção na fase 5");
    }

    fn accepts(&self, _item: &Item, content: usize, _port: usize) -> bool {
        content < self.bays * self.levels
    }
}
