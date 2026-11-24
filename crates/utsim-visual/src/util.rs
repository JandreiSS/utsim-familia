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

/// A pergunta de confirmação de uma remoção, num quadro junto de `onde` (o cartão ou o nó):
/// o texto, "Remover" em vermelho e "Cancelar". Devolve `Some(true)` ao confirmar,
/// `Some(false)` ao cancelar (ou com Esc) e `None` enquanto espera.
pub fn confirmar_remocao(
    ctx: &egui::Context,
    id: egui::Id,
    onde: egui::Pos2,
    pergunta: &str,
) -> Option<bool> {
    let mut resposta = None;
    egui::Area::new(id)
        .order(egui::Order::Foreground)
        .fixed_pos(onde)
        .show(ctx, |ui| {
            egui::Frame::popup(ui.style()).show(ui, |ui| {
                ui.set_max_width(320.0);
                ui.label(pergunta);
                ui.horizontal(|ui| {
                    let vermelho = ui.visuals().error_fg_color;
                    if ui
                        .button(egui::RichText::new(crate::texto::tx("visual-remover")).color(vermelho))
                        .clicked()
                    {
                        resposta = Some(true);
                    }
                    if ui.button(crate::texto::tx("visual-cancelar")).clicked() {
                        resposta = Some(false);
                    }
                });
            });
        });
    if resposta.is_none() && ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
        resposta = Some(false);
    }
    resposta
}
