//! Block placement in the 3D scene.
//!
//! Coordinate convention. UTSim models are written on a plan view: `x` to the right,
//! `y` along the floor, `z` up. Bevy is Y-up, so a UTSim position `(x, y, z)` becomes
//! the Bevy translation `(x, z, -y)`: model `x` stays on Bevy X, model `z` (height) is
//! Bevy Y, and model `y` runs along Bevy Z with its sign flipped, so a model drawn
//! with `y` growing "upwards on the paper" also looks that way from the default
//! camera (which sits on +Z looking towards -Z) and the mapping is not a mirror image.
//!
//! When every object sits at the origin (a model written without positions) an
//! automatic layout is used instead: blocks are layered by the longest path from the
//! sources (layer index on X) and spread side by side inside a layer (Z).

/// Distance between layers (Bevy X) and between blocks of a layer (Bevy Z).
pub const LAYER_SPACING: f32 = 5.0;
pub const LANE_SPACING: f32 = 3.5;

/// Model position (x, y, z) to Bevy translation.
pub fn to_bevy(p: [f64; 3]) -> [f32; 3] {
    [p[0] as f32, p[2] as f32, -p[1] as f32]
}

/// Layout-space (Bevy, Y up) point back to model coordinates `(x, y, z)`, Z up: the
/// inverse of [`to_bevy`] (exact, it only swaps and negates).
pub fn to_model(p: [f32; 3]) -> [f32; 3] {
    [p[0], -p[2], p[1]]
}

pub fn all_at_origin(positions: &[[f64; 3]]) -> bool {
    positions.iter().all(|p| p.iter().all(|c| *c == 0.0))
}

/// Final Bevy positions of every block: the model's own, or an automatic layout.
pub fn place(positions: &[[f64; 3]], connections: &[(usize, usize)]) -> Vec<[f32; 3]> {
    if all_at_origin(positions) {
        auto_layout(positions.len(), connections)
    } else {
        positions.iter().map(|p| to_bevy(*p)).collect()
    }
}

/// Layer (column) of each block: longest path from the sources, ignoring the edges
/// that close a cycle (rework loops).
pub fn layers(n: usize, connections: &[(usize, usize)]) -> Vec<usize> {
    let edges: Vec<(usize, usize)> = connections
        .iter()
        .copied()
        .filter(|&(a, b)| a < n && b < n && a != b)
        .collect();
    let mut out: Vec<Vec<usize>> = vec![Vec::new(); n];
    let mut indeg = vec![0usize; n];
    for &(a, b) in &edges {
        out[a].push(b);
        indeg[b] += 1;
    }

    // Iterative DFS from the sources (then from anything left) to find back edges.
    #[derive(Clone, Copy, PartialEq)]
    enum Mark {
        New,
        OnStack,
        Done,
    }
    let mut mark = vec![Mark::New; n];
    let mut back: std::collections::HashSet<(usize, usize)> = std::collections::HashSet::new();
    let roots: Vec<usize> = (0..n)
        .filter(|&v| indeg[v] == 0)
        .chain(0..n)
        .collect::<Vec<_>>();
    for root in roots {
        if mark[root] != Mark::New {
            continue;
        }
        let mut stack = vec![(root, 0usize)];
        mark[root] = Mark::OnStack;
        while let Some(top) = stack.last_mut() {
            let v = top.0;
            if top.1 < out[v].len() {
                let w = out[v][top.1];
                top.1 += 1;
                match mark[w] {
                    Mark::New => {
                        mark[w] = Mark::OnStack;
                        stack.push((w, 0));
                    }
                    Mark::OnStack => {
                        back.insert((v, w));
                    }
                    Mark::Done => {}
                }
            } else {
                mark[v] = Mark::Done;
                stack.pop();
            }
        }
    }

    // Longest path on the remaining DAG (Kahn order).
    let mut dag_out: Vec<Vec<usize>> = vec![Vec::new(); n];
    let mut dag_in = vec![0usize; n];
    for &(a, b) in &edges {
        if !back.contains(&(a, b)) {
            dag_out[a].push(b);
            dag_in[b] += 1;
        }
    }
    let mut layer = vec![0usize; n];
    let mut ready: Vec<usize> = (0..n).filter(|&v| dag_in[v] == 0).collect();
    while let Some(v) = ready.pop() {
        for &w in &dag_out[v] {
            layer[w] = layer[w].max(layer[v] + 1);
            dag_in[w] -= 1;
            if dag_in[w] == 0 {
                ready.push(w);
            }
        }
    }
    layer
}

/// Layered layout in Bevy coordinates (ground plane at y = 0).
pub fn auto_layout(n: usize, connections: &[(usize, usize)]) -> Vec<[f32; 3]> {
    let layer = layers(n, connections);
    let n_layers = layer.iter().copied().max().map_or(0, |m| m + 1);
    let mut width = vec![0usize; n_layers];
    for &l in &layer {
        width[l] += 1;
    }
    let mut seen = vec![0usize; n_layers];
    layer
        .iter()
        .map(|&l| {
            let i = seen[l];
            seen[l] += 1;
            let z = (i as f32 - (width[l] as f32 - 1.0) / 2.0) * LANE_SPACING;
            [l as f32 * LAYER_SPACING, 0.0, z]
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn convention_maps_model_z_to_height() {
        assert_eq!(to_bevy([1.0, 2.0, 3.0]), [1.0, 3.0, -2.0]);
        assert_eq!(to_model(to_bevy([1.0, 2.0, 3.0])), [1.0, 2.0, 3.0]);
    }

    #[test]
    fn model_positions_are_used_when_any_is_set() {
        let pos = [[0.0; 3], [4.0, 1.0, 0.0]];
        assert!(!all_at_origin(&pos));
        assert_eq!(
            place(&pos, &[(0, 1)]),
            vec![[0.0, 0.0, 0.0], [4.0, 0.0, -1.0]]
        );
    }

    #[test]
    fn chain_goes_left_to_right_on_one_lane() {
        let p = auto_layout(4, &[(0, 1), (1, 2), (2, 3)]);
        for (i, q) in p.iter().enumerate() {
            assert_eq!(*q, [i as f32 * LAYER_SPACING, 0.0, 0.0]);
        }
    }

    #[test]
    fn longest_path_decides_the_layer() {
        // 0 -> 1 -> 3 and 0 -> 2 -> 3 and 0 -> 3 directly: 3 is in layer 2.
        let l = layers(4, &[(0, 1), (0, 2), (1, 3), (2, 3), (0, 3)]);
        assert_eq!(l, vec![0, 1, 1, 2]);
        let p = auto_layout(4, &[(0, 1), (0, 2), (1, 3), (2, 3), (0, 3)]);
        // Blocks 1 and 2 share layer 1 and are spread symmetrically on Z.
        assert_eq!(p[1][0], p[2][0]);
        assert_eq!(p[1][2], -p[2][2]);
        assert_ne!(p[1][2], p[2][2]);
    }

    #[test]
    fn cycles_do_not_hang_and_keep_forward_order() {
        // 0 -> 1 -> 2 -> 1 (rework loop) and 2 -> 3.
        let l = layers(4, &[(0, 1), (1, 2), (2, 1), (2, 3)]);
        assert_eq!(l, vec![0, 1, 2, 3]);
        // Pure cycle with no source still terminates.
        let l = layers(2, &[(0, 1), (1, 0)]);
        assert_eq!(l.len(), 2);
    }

    #[test]
    fn disconnected_blocks_get_layer_zero() {
        assert_eq!(layers(3, &[(0, 1)]), vec![0, 1, 0]);
    }
}
