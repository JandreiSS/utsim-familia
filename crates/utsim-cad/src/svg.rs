//! Exportação para a interface (arrays planos) e para SVG.

use std::fmt::Write;

use crate::geom::Bbox;
use crate::plan::{Layer, Plan};
use crate::roles::{Role, RoleMap};

/// Linhas de uma camada em arrays planos, prontos para a interface (`PackedFloat32Array`).
#[derive(Debug, Clone, PartialEq, Default)]
pub struct FlatLines {
    /// `x0, y0, x1, y1, ...` de todas as polilinhas, em sequência.
    pub coords: Vec<f32>,
    /// Índice (em pontos, não em floats) onde cada polilinha começa; o último elemento é o
    /// total de pontos, então a polilinha `i` vai de `starts[i]` a `starts[i + 1]`.
    pub starts: Vec<u32>,
    /// Se cada polilinha é fechada.
    pub closed: Vec<bool>,
    /// Pares de pontos `x0, y0, x1, y1` por segmento, para `PRIMITIVE_LINES`.
    pub segments: Vec<f32>,
}

impl FlatLines {
    pub fn from_layer(layer: &Layer) -> Self {
        let mut f = FlatLines::default();
        let mut total = 0u32;
        for pl in &layer.polylines {
            f.starts.push(total);
            f.closed.push(pl.closed);
            for p in &pl.points {
                f.coords.push(p[0] as f32);
                f.coords.push(p[1] as f32);
            }
            total += pl.points.len() as u32;
            for s in pl.segments() {
                f.segments.extend([
                    s[0][0] as f32,
                    s[0][1] as f32,
                    s[1][0] as f32,
                    s[1][1] as f32,
                ]);
            }
        }
        f.starts.push(total);
        f
    }
}

/// Opções do SVG.
#[derive(Debug, Clone)]
pub struct SvgOptions {
    /// Largura da imagem em pixels (a altura segue a proporção).
    pub width_px: f64,
    /// Margem em pixels.
    pub margin_px: f64,
    /// Camadas a desenhar (vazio = todas, exceto as de papel `Ignorar`).
    pub layers: Vec<String>,
    /// Papéis por camada (cores). Se `None`, tudo é desenhado como fundo.
    pub roles: Option<RoleMap>,
    /// Espessura do traço em pixels.
    pub stroke_px: f64,
}

impl Default for SvgOptions {
    fn default() -> Self {
        SvgOptions {
            width_px: 1000.0,
            margin_px: 10.0,
            layers: Vec::new(),
            roles: None,
            stroke_px: 1.0,
        }
    }
}

fn color(role: Role) -> &'static str {
    match role {
        Role::Fundo => "#8a8f98",
        Role::Parede => "#202020",
        Role::Coluna => "#c0392b",
        Role::Porta => "#2e86c1",
        Role::Ignorar => "#cccccc",
    }
}

fn escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

/// Gera o SVG da planta (eixo y para cima, como na planta). Determinístico: três casas
/// decimais, camadas em ordem de nome.
pub fn to_svg(plan: &Plan, opts: &SvgOptions) -> String {
    let bb: Bbox = plan.bbox().unwrap_or(Bbox {
        min: [0.0, 0.0],
        max: [1.0, 1.0],
    });
    let w = bb.width().max(1e-9);
    let h = bb.height().max(1e-9);
    let inner = (opts.width_px - 2.0 * opts.margin_px).max(1.0);
    let k = inner / w;
    let height_px = h * k + 2.0 * opts.margin_px;
    let tx = |x: f64| opts.margin_px + (x - bb.min[0]) * k;
    let ty = |y: f64| opts.margin_px + (bb.max[1] - y) * k;
    let mut s = String::new();
    let _ = writeln!(
        s,
        "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{:.0}\" height=\"{:.0}\" viewBox=\"0 0 {:.3} {:.3}\">",
        opts.width_px, height_px, opts.width_px, height_px
    );
    let _ = writeln!(s, "<rect width=\"100%\" height=\"100%\" fill=\"#ffffff\"/>");
    for layer in &plan.layers {
        let role = opts
            .roles
            .as_ref()
            .map(|r| r.role_of(&layer.name))
            .unwrap_or(Role::Fundo);
        let wanted = if opts.layers.is_empty() {
            role != Role::Ignorar
        } else {
            opts.layers.iter().any(|n| n == &layer.name)
        };
        if !wanted {
            continue;
        }
        let _ = writeln!(
            s,
            "<g id=\"{}\" fill=\"none\" stroke=\"{}\" stroke-width=\"{:.2}\">",
            escape(&layer.name),
            color(role),
            opts.stroke_px
        );
        for pl in &layer.polylines {
            let pts: Vec<String> = pl
                .points
                .iter()
                .map(|p| format!("{:.3},{:.3}", tx(p[0]), ty(p[1])))
                .collect();
            let tag = if pl.closed { "polygon" } else { "polyline" };
            let _ = writeln!(s, "<{tag} points=\"{}\"/>", pts.join(" "));
        }
        let _ = writeln!(s, "</g>");
    }
    let _ = writeln!(s, "</svg>");
    s
}
