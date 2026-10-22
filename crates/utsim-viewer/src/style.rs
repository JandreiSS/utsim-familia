//! Colors and block geometry shared by the scene and the UI (pure data, no Bevy).

use utsim_core::State;

/// sRGB color of a block in the given state (`None` = never changed state).
pub fn state_color(state: Option<State>) -> [f32; 3] {
    match state {
        Some(State::Processing) => [0.20, 0.72, 0.30],
        Some(State::Setup) => [0.95, 0.85, 0.20],
        Some(State::Blocked) => [0.98, 0.55, 0.10],
        Some(State::Down) => [0.85, 0.15, 0.15],
        Some(State::OffShift) => [0.25, 0.45, 0.90],
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
}

pub fn shape_of(kind: &str) -> Shape {
    match kind {
        "Source" => Shape::Cone,
        "Queue" => Shape::LongBox,
        "Combiner" => Shape::Funnel,
        "Separator" => Shape::Frustum,
        "Sink" => Shape::Cylinder,
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
    }
}

/// Offset of item slot `i` (0 = oldest) from the block's base center.
pub fn slot_offset(shape: Shape, i: usize) -> [f32; 3] {
    let (_, top) = extents(shape);
    const S: f32 = 0.4;
    if shape == Shape::LongBox {
        // 10 along the queue (oldest at the exit, +X), 3 lanes, then stacked.
        let col = i % 10;
        let lane = (i / 10) % 3;
        let layer = i / 30;
        [
            1.8 - col as f32 * S,
            top + 0.17 + layer as f32 * 0.32,
            (lane as f32 - 1.0) * S,
        ]
    } else {
        let cell = i % 9;
        let layer = i / 9;
        [
            ((cell % 3) as f32 - 1.0) * S,
            top + 0.17 + layer as f32 * 0.32,
            ((cell / 3) as f32 - 1.0) * S,
        ]
    }
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
}
