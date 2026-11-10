//! A barra lateral de navegação dos apps da família: o ícone e o nome do app no topo, umas
//! linhas sobre o que está aberto (a unidade, o autor) e as telas em grupos com título.
//! Vai num `egui::Panel::left`; o app guarda qual tela está aberta.

use egui::{Align2, Color32, CornerRadius, FontId, Rect, RichText, Sense, Ui, Vec2, pos2, vec2};

/// Uma tela da barra.
#[derive(Debug, Clone, Copy)]
pub struct Item<'a, T> {
    pub tela: T,
    pub titulo: &'a str,
    /// Sem `None`, o item fica apagado e o texto diz por quê.
    pub bloqueio: Option<&'a str>,
}

impl<'a, T> Item<'a, T> {
    pub fn novo(tela: T, titulo: &'a str) -> Self {
        Item {
            tela,
            titulo,
            bloqueio: None,
        }
    }

    pub fn bloqueado_se(mut self, bloquear: bool, motivo: &'a str) -> Self {
        if bloquear {
            self.bloqueio = Some(motivo);
        }
        self
    }
}

/// Um grupo de telas, com título (ou sem, para o primeiro, "Início").
#[derive(Debug, Clone)]
pub struct Grupo<'a, T> {
    pub titulo: Option<&'a str>,
    pub itens: Vec<Item<'a, T>>,
}

/// A barra inteira.
pub struct Barra<'a, T> {
    pub nome: &'a str,
    pub cor: Color32,
    pub icone: Option<&'static [u8]>,
    /// Linhas curtas sob o nome (a unidade aberta, o autor).
    pub linhas: &'a [String],
    pub grupos: &'a [Grupo<'a, T>],
}

/// A largura padrão da barra, em pontos.
pub const LARGURA: f32 = 228.0;

impl<T: Copy + PartialEq> Barra<'_, T> {
    /// Desenha a barra; `atual` muda quando a pessoa escolhe outra tela. Devolve `true`
    /// nesse quadro.
    pub fn mostrar(&self, ui: &mut Ui, atual: &mut T) -> bool {
        let mut mudou = false;
        ui.add_space(12.0);
        ui.horizontal(|ui| {
            let lado = 32.0;
            let (r, _) = ui.allocate_exact_size(Vec2::splat(lado), Sense::hover());
            match self
                .icone
                .and_then(|b| crate::abertura::textura(ui, self.nome, b))
            {
                Some(t) => {
                    ui.painter().image(
                        t.id(),
                        r,
                        Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)),
                        Color32::WHITE,
                    );
                }
                None => {
                    ui.painter().rect_filled(r, CornerRadius::same(8), self.cor);
                    ui.painter().text(
                        r.center(),
                        Align2::CENTER_CENTER,
                        crate::abertura::iniciais(self.nome),
                        FontId::proportional(14.0),
                        Color32::WHITE,
                    );
                }
            }
            ui.add(egui::Label::new(RichText::new(self.nome).strong().size(15.0)).truncate());
        });
        if !self.linhas.is_empty() {
            ui.add_space(6.0);
            for l in self.linhas {
                ui.add(
                    egui::Label::new(
                        RichText::new(l)
                            .size(12.0)
                            .color(ui.visuals().weak_text_color()),
                    )
                    .truncate(),
                );
            }
        }
        ui.add_space(10.0);
        egui::ScrollArea::vertical()
            .auto_shrink([false, false])
            .show(ui, |ui| {
                for g in self.grupos {
                    if let Some(t) = g.titulo {
                        ui.add_space(10.0);
                        ui.label(
                            RichText::new(t.to_uppercase())
                                .size(11.0)
                                .strong()
                                .color(ui.visuals().weak_text_color()),
                        );
                        ui.add_space(2.0);
                    }
                    for it in &g.itens {
                        if item(ui, it, *atual == it.tela, self.cor) {
                            *atual = it.tela;
                            mudou = true;
                        }
                    }
                }
            });
        mudou
    }
}

/// Uma linha da barra. Devolve `true` no clique.
fn item<T>(ui: &mut Ui, it: &Item<'_, T>, selecionado: bool, cor: Color32) -> bool {
    let ativo = it.bloqueio.is_none();
    let (r, resp) = ui.allocate_exact_size(
        vec2(ui.available_width(), 30.0),
        if ativo {
            Sense::click()
        } else {
            Sense::hover()
        },
    );
    let resp = match it.bloqueio {
        Some(m) => resp.on_hover_text(m),
        None => resp.on_hover_cursor(egui::CursorIcon::PointingHand),
    };
    if ui.is_rect_visible(r) {
        let v = ui.visuals();
        let p = ui.painter();
        if selecionado {
            p.rect_filled(r, CornerRadius::same(6), cor.gamma_multiply(0.22));
            p.rect_filled(
                Rect::from_min_size(r.min + vec2(0.0, 6.0), vec2(3.0, r.height() - 12.0)),
                CornerRadius::same(2),
                cor,
            );
        } else if ativo && resp.hovered() {
            p.rect_filled(r, CornerRadius::same(6), v.widgets.hovered.weak_bg_fill);
        }
        let tinta = if !ativo {
            v.weak_text_color().gamma_multiply(0.7)
        } else if selecionado {
            v.strong_text_color()
        } else {
            v.text_color()
        };
        let mut job = egui::text::LayoutJob::simple_singleline(
            it.titulo.to_string(),
            FontId::proportional(14.0),
            tinta,
        );
        job.wrap = egui::text::TextWrapping::truncate_at_width(r.width() - 24.0);
        let g = p.layout_job(job);
        p.galley(
            pos2(r.left() + 14.0, r.center().y - g.size().y / 2.0),
            g,
            tinta,
        );
    }
    ativo && resp.clicked()
}
