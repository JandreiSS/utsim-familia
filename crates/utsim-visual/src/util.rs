//! Pedaços comuns aos desenhos: texto cortado com reticências e o quadro do cartão.

use std::sync::Arc;

use egui::text::{LayoutJob, TextWrapping};
use egui::{Color32, FontId, Galley, Painter};

/// O texto numa largura, em até `linhas` linhas, terminando com "…" quando não cabe.
pub fn cortado(
    p: &Painter,
    texto: &str,
    fonte: FontId,
    cor: Color32,
    largura: f32,
    linhas: usize,
) -> Arc<Galley> {
    let mut job = LayoutJob::simple(texto.to_string(), fonte, cor, largura.max(8.0));
    job.wrap = TextWrapping {
        max_width: largura.max(8.0),
        max_rows: linhas.max(1),
        break_anywhere: linhas == 1,
        overflow_character: Some('…'),
    };
    p.layout_job(job)
}
