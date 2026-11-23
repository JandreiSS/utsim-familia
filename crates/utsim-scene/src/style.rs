//! Colors and block geometry shared by the scene and the UI (pure data, no Bevy).

use utsim_core::State;

/// sRGB color of a block in the given state (`None` = never changed state).
pub fn state_color(state: Option<State>) -> [f32; 3] {
    match state {
        Some(State::Processing) => [0.20, 0.72, 0.30],
        Some(State::Setup) => [0.95, 0.85, 0.20],
        // Operators getting in or out: between setup (yellow) and processing (green).
        Some(State::Entering | State::Leaving) => [0.62, 0.80, 0.25],
        Some(State::Blocked) => [0.98, 0.55, 0.10],
        Some(State::Down) => [0.85, 0.15, 0.15],
        Some(State::OffShift) => [0.25, 0.45, 0.90],
        Some(State::WaitingRepair) => [0.93, 0.35, 0.25],
        Some(State::Maintenance) => [0.55, 0.35, 0.80],
        Some(State::WaitingOperator | State::WaitingTransport) => [0.70, 0.45, 0.80],
        Some(State::Idle) => [0.55, 0.55, 0.58],
        Some(State::TravelEmpty | State::TravelLoaded) => [0.20, 0.60, 0.85],
        Some(State::Loading | State::Unloading | State::Utilize) => [0.20, 0.72, 0.30],
        // Empty, generating, releasing, collecting and "no state yet": neutral.
        Some(_) | None => [0.62, 0.66, 0.72],
    }
}

/// Portuguese name of a block state, for the UI.
pub fn state_label(state: Option<State>) -> &'static str {
    match state {
        None => "sem estado",
        Some(State::Idle) => "ocioso",
        Some(State::Empty) => "vazio",
        Some(State::Generating) => "gerando",
        Some(State::Releasing) => "liberando",
        Some(State::Collecting) => "coletando",
        Some(State::Setup) => "preparação",
        Some(State::Processing) => "processando",
        Some(State::Blocked) => "bloqueado",
        Some(State::WaitingOperator) => "aguardando operador",
        Some(State::WaitingTransport) => "aguardando transporte",
        Some(State::Down) => "em falha",
        Some(State::OffShift) => "fora do turno",
        Some(State::TravelEmpty) => "viajando vazio",
        Some(State::TravelLoaded) => "viajando carregado",
        Some(State::Loading) => "carregando",
        Some(State::Unloading) => "descarregando",
        Some(State::Utilize) => "trabalhando no posto",
        Some(State::Stopped) => "parado (comando externo)",
        Some(State::WaitingRepair) => "aguardando técnico",
        Some(State::Maintenance) => "em manutenção preventiva",
        Some(State::Resting) => "em pausa (fadiga)",
        Some(State::Entering) => "operador entrando",
        Some(State::Leaving) => "operador saindo",
    }
}

/// Color of items created by block `origin`: golden-ratio hue steps keep neighbours apart.
pub fn origin_color(origin: usize) -> [f32; 3] {
    let hue = (origin as f32 * 0.618_034 + 0.08).fract();
    hsv_to_rgb(hue, 0.65, 0.95)
}

fn hsv_to_rgb(h: f32, s: f32, v: f32) -> [f32; 3] {
    let i = (h * 6.0).floor();
    let f = h * 6.0 - i;
    let (p, q, t) = (v * (1.0 - s), v * (1.0 - f * s), v * (1.0 - (1.0 - f) * s));
    match (i as i32).rem_euclid(6) {
        0 => [v, t, p],
        1 => [q, v, p],
        2 => [p, v, t],
        3 => [p, q, v],
        4 => [t, p, v],
        _ => [v, p, q],
    }
}

/// Shape of a block, by the object kind written in the model.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Shape {
    Cone,
    LongBox,
    Box,
    Funnel,
    Frustum,
    Cylinder,
    /// Transporter: low flat body with a mast (the mast is added by the scene).
    Vehicle,
    /// Dispatcher: small marker.
    Flag,
    /// Conveyor: a long flat belt (its real size comes from the model, see `geometry`).
    Belt,
    /// Rack: bays x levels frame (real size from the model).
    Rack,
}

pub fn shape_of(kind: &str) -> Shape {
    match kind {
        "Source" => Shape::Cone,
        "Queue" => Shape::LongBox,
        "Combiner" => Shape::Funnel,
        "Separator" => Shape::Frustum,
        "Sink" => Shape::Cylinder,
        "Transporter" => Shape::Vehicle,
        "Dispatcher" => Shape::Flag,
        "Conveyor" => Shape::Belt,
        "Rack" => Shape::Rack,
        _ => Shape::Box,
    }
}

/// Half extents of the block's bounding box and the height of its item surface.
pub fn extents(shape: Shape) -> ([f32; 3], f32) {
    match shape {
        Shape::LongBox => ([2.0, 0.15, 0.7], 0.3),
        Shape::Cone => ([0.8, 0.5, 0.8], 1.0),
        Shape::Box => ([0.9, 0.5, 0.9], 1.0),
        Shape::Funnel => ([1.0, 0.5, 1.0], 1.0),
        Shape::Frustum => ([1.0, 0.5, 1.0], 1.0),
        Shape::Cylinder => ([0.9, 0.5, 0.9], 1.0),
        Shape::Vehicle => ([0.55, 0.2, 0.4], 0.4),
        Shape::Flag => ([0.2, 0.4, 0.2], 0.8),
        // Defaults for a conveyor/rack of unknown size; `geometry::Geom` has the real ones.
        Shape::Belt => ([2.5, 0.125, 0.55], 0.25),
        Shape::Rack => ([2.0, 1.5, 0.5], 3.0),
    }
}

/// Offset of item slot `i` (0 = oldest) from the block's base center, at the shape's
/// default size.
pub fn slot_offset(shape: Shape, i: usize) -> [f32; 3] {
    slot_offset_in(shape, i, extents(shape).0)
}

/// Distance between two resting items on a block, metres.
const SLOT_PITCH: f32 = 0.4;

/// Columns (along `x`) and lanes (across) of resting items on a block of half extents
/// `half`: the default grid (10 × 3 on a queue, 3 × 3 elsewhere) grown or shrunk with the
/// block, at least one of each.
pub fn slot_grid(shape: Shape, half: [f32; 3]) -> (usize, usize) {
    let (d, _) = extents(shape);
    let (cols, lanes) = if shape == Shape::LongBox {
        (10, 3)
    } else {
        (3, 3)
    };
    let n = |k: usize, h: f32, dh: f32| {
        if h.is_finite() && h > 0.0 {
            ((k as f32 * h / dh).round() as usize).max(1)
        } else {
            k
        }
    };
    (n(cols, half[0], d[0]), n(lanes, half[2], d[2]))
}

/// Offset of item slot `i` (0 = oldest) from the base center of a block of half extents
/// `half` (its size in the model): the items lie on its top in [`slot_grid`], the oldest
/// of a queue at its exit (`+X`), then stack.
pub fn slot_offset_in(shape: Shape, i: usize, half: [f32; 3]) -> [f32; 3] {
    let (cols, lanes) = slot_grid(shape, half);
    let per = cols * lanes;
    let (k, layer) = (i % per, i / per);
    let (col, lane) = (k % cols, k / cols);
    let mid = |n: usize| (n as f32 - 1.0) / 2.0;
    let x = if shape == Shape::LongBox {
        mid(cols) - col as f32
    } else {
        col as f32 - mid(cols)
    };
    [
        x * SLOT_PITCH,
        2.0 * half[1] + 0.17 + layer as f32 * 0.32,
        (lane as f32 - mid(lanes)) * SLOT_PITCH,
    ]
}

/// Places of the items resting on a block with `n` places, oldest first: each item has
/// its own place while it rests, its arrival number (`seq`) modulo `n`, so the others do
/// not move when one leaves. Two items wanting the same place (a stack, or more than `n`
/// waiting) leave the newer to the lowest free place; past `n` items, `None` (not drawn).
pub fn pick_slots(seqs: &[Option<u32>], n: usize) -> Vec<Option<usize>> {
    let mut out = vec![None; seqs.len()];
    if n == 0 {
        return out;
    }
    let mut used = vec![false; n];
    let shown = seqs.len().min(n);
    for (k, seq) in seqs.iter().take(shown).enumerate() {
        if let Some(s) = seq {
            let want = *s as usize % n;
            if !used[want] {
                used[want] = true;
                out[k] = Some(want);
            }
        }
    }
    let mut free = 0;
    for slot in out.iter_mut().take(shown) {
        if slot.is_none() {
            while used[free] {
                free += 1;
            }
            used[free] = true;
            *slot = Some(free);
        }
    }
    out
}

/// Layers of items drawn on a block; items beyond that are not drawn (the label shows "+N").
pub const MAX_LAYERS: usize = 2;

/// How many resting items a block of this shape draws at most.
pub fn visible_slots(shape: Shape) -> usize {
    visible_slots_in(shape, extents(shape).0)
}

/// How many resting items a block of half extents `half` draws at most.
pub fn visible_slots_in(shape: Shape, half: [f32; 3]) -> usize {
    let (cols, lanes) = slot_grid(shape, half);
    cols * lanes * MAX_LAYERS
}

/// Items of a block that are not drawn because the stack is capped.
pub fn hidden_items(shape: Shape, resting: usize) -> usize {
    resting.saturating_sub(visible_slots(shape))
}

/// Height above the block base of the top of what is drawn on it (`drawn` items).
pub fn top_of_stack(shape: Shape, drawn: usize) -> f32 {
    top_of_stack_in(shape, drawn, extents(shape).0)
}

/// [`top_of_stack`] on a block of half extents `half`.
pub fn top_of_stack_in(shape: Shape, drawn: usize, half: [f32; 3]) -> f32 {
    if drawn == 0 {
        half[1] * 2.0
    } else {
        slot_offset_in(shape, drawn.min(visible_slots_in(shape, half)) - 1, half)[1] + 0.2
    }
}

/// Text of the in-scene label of a block. `received` is `Some` for sinks (items that
/// arrived so far, which a sink never holds); `hidden` counts items not drawn.
pub fn label_text(name: &str, content: usize, hidden: usize, received: Option<usize>) -> String {
    let mut text = match received {
        Some(n) => format!("{name} (recebidos: {n})"),
        None if content > 0 => format!("{name} ({content})"),
        None => name.to_string(),
    };
    if hidden > 0 {
        text.push_str(&format!(" +{hidden}"));
    }
    text
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_state_has_a_color_and_label() {
        for s in [
            State::Idle,
            State::Empty,
            State::Generating,
            State::Releasing,
            State::Collecting,
            State::Setup,
            State::Processing,
            State::Blocked,
            State::WaitingOperator,
            State::WaitingTransport,
            State::Down,
            State::OffShift,
            State::TravelEmpty,
            State::TravelLoaded,
            State::Loading,
            State::Unloading,
            State::Utilize,
            State::Entering,
            State::Leaving,
        ] {
            let c = state_color(Some(s));
            assert!(c.iter().all(|v| (0.0..=1.0).contains(v)));
            assert!(!state_label(Some(s)).is_empty());
        }
        assert_ne!(
            state_color(Some(State::Processing)),
            state_color(Some(State::Down))
        );
    }

    #[test]
    fn origin_colors_are_valid_and_distinct_for_neighbours() {
        for i in 0..20 {
            let c = origin_color(i);
            assert!(c.iter().all(|v| (0.0..=1.0).contains(v)));
            assert_ne!(c, origin_color(i + 1));
        }
    }

    #[test]
    fn queue_slots_line_up_and_stack() {
        let a = slot_offset(Shape::LongBox, 0);
        let b = slot_offset(Shape::LongBox, 1);
        assert!(a[0] > b[0]);
        assert_eq!(a[1], b[1]);
        assert!(slot_offset(Shape::LongBox, 30)[1] > a[1]);
        assert_eq!(shape_of("Queue"), Shape::LongBox);
        assert_eq!(shape_of("Processor"), Shape::Box);
    }

    #[test]
    fn mobile_resources_have_their_own_shapes() {
        assert_eq!(shape_of("Transporter"), Shape::Vehicle);
        assert_eq!(shape_of("Dispatcher"), Shape::Flag);
        // Items carried by a vehicle sit on top of it.
        assert!(slot_offset(Shape::Vehicle, 0)[1] > 2.0 * extents(Shape::Vehicle).0[1]);
        assert_ne!(
            state_color(Some(State::TravelEmpty)),
            state_color(Some(State::Loading))
        );
    }

    #[test]
    fn resting_items_keep_their_places_when_one_leaves() {
        // Items 0..5 arrived in order; with 8 places each has its own.
        let seqs: Vec<Option<u32>> = (0..5).map(Some).collect();
        assert_eq!(
            pick_slots(&seqs, 8),
            vec![Some(0), Some(1), Some(2), Some(3), Some(4)]
        );
        // The oldest left (FIFO): the others stay where they were.
        assert_eq!(
            pick_slots(&seqs[1..], 8),
            vec![Some(1), Some(2), Some(3), Some(4)]
        );
        // The next arrivals wrap around into the places freed at the front.
        let seqs: Vec<Option<u32>> = (6..10).map(Some).collect();
        assert_eq!(
            pick_slots(&seqs, 8),
            vec![Some(6), Some(7), Some(0), Some(1)]
        );
        // A clash (a stack keeps an old item under new ones) takes the lowest free place;
        // no arrival number takes one too; past the places, not drawn.
        assert_eq!(
            pick_slots(&[Some(0), Some(3), None, Some(1)], 3),
            vec![Some(0), Some(1), Some(2), None]
        );
    }

    #[test]
    fn slots_follow_the_size_of_the_block() {
        // The default size keeps the old grid.
        let near = |a: [f32; 3], b: [f32; 3]| a.iter().zip(b).all(|(x, y)| (x - y).abs() < 1e-5);
        assert!(near(slot_offset(Shape::LongBox, 0), [1.8, 0.47, -0.4]));
        assert!(near(
            slot_offset(Shape::LongBox, 13),
            [1.8 - 1.2, 0.47, 0.0]
        ));
        assert!(near(slot_offset(Shape::Box, 4), [0.0, 1.17, 0.0]));
        // A queue twice as long has twice the columns, and its oldest item at its exit.
        let (d, _) = extents(Shape::LongBox);
        let long = [2.0 * d[0], d[1], d[2]];
        assert_eq!(slot_grid(Shape::LongBox, long), (20, 3));
        assert!((slot_offset_in(Shape::LongBox, 0, long)[0] - 3.8).abs() < 1e-5);
        assert_eq!(visible_slots_in(Shape::LongBox, long), 120);
        // A taller block lifts its items with its top.
        let tall = [d[0], 1.0, d[2]];
        assert!((slot_offset_in(Shape::LongBox, 0, tall)[1] - 2.17).abs() < 1e-5);
        // A small one keeps one slot.
        assert_eq!(slot_grid(Shape::Box, [0.1, 0.5, 0.1]), (1, 1));
    }

    #[test]
    fn stack_is_capped_and_label_counts_the_rest() {
        assert_eq!(visible_slots(Shape::LongBox), 60);
        assert_eq!(visible_slots(Shape::Box), 18);
        assert_eq!(hidden_items(Shape::LongBox, 23), 0);
        assert_eq!(hidden_items(Shape::LongBox, 80), 20);
        // The top of the stack never grows past the cap, however many items rest there.
        assert_eq!(
            top_of_stack(Shape::LongBox, 500),
            top_of_stack(Shape::LongBox, 60)
        );
        assert!(top_of_stack(Shape::LongBox, 60) > top_of_stack(Shape::LongBox, 1));
        assert!(top_of_stack(Shape::LongBox, 0) < top_of_stack(Shape::LongBox, 1));
        assert_eq!(label_text("fila", 80, 20, None), "fila (80) +20");
        assert_eq!(label_text("fila", 0, 0, None), "fila");
        assert_eq!(label_text("saida", 0, 0, Some(12)), "saida (recebidos: 12)");
    }
}
