//! The campaign map in strips of cards: one strip per act, one card per mission (number,
//! title, stars, best grade, lock), the next mission to do highlighted in the app's colour.
//! Only drawing: which missions are open, the records and the rules come from the progress.

use egui::{
    Align2, Color32, CornerRadius, FontId, Rect, RichText, Sense, Stroke, StrokeKind, Ui, pos2,
    vec2,
};
use utsim_ensino_core::progresso::RegistroMissao;

use crate::{App, ROXO, numero_missao, texto};

/// The colour of the stars earned.
const OURO: Color32 = Color32::from_rgb(240, 190, 80);
const LARGURA: f32 = 232.0;
const ALTURA: f32 = 118.0;

/// What the person clicked on the map.
pub(crate) enum Clique {
    Missao(usize),
    Areia(usize),
}

/// The acts of the open package as strips of cards.
pub(crate) fn atos(app: &App, ui: &mut Ui) -> Option<Clique> {
    let p = app.pacote.clone()?;
    let prova = p.pacote.prova.is_some();
    let resolvida = |i: usize| {
        app.progresso
            .registro(&p.missoes[i].missao.id)
            .is_some_and(|r| r.resolvida)
    };
    // The next mission: the first open one not solved yet.
    let proxima =
        (0..p.missoes.len()).find(|&i| (prova || app.progresso.liberada(&p, i)) && !resolvida(i));
    let mut clique = None;
    for ato in &p.pacote.atos {
        let missoes: Vec<usize> = (0..p.missoes.len())
            .filter(|&i| p.missoes[i].ato == ato.numero)
            .collect();
        let feitas = missoes.iter().filter(|&&i| resolvida(i)).count();
        ui.add_space(14.0);
        ui.horizontal(|ui| {
            ui.label(
                RichText::new(texto::f(texto::ato_titulo(), &[&ato.numero, &ato.titulo]))
                    .size(17.0)
                    .strong(),
            );
            if !prova {
                ui.label(
                    RichText::new(texto::f(
                        texto::mapa_resolvidas(),
                        &[&feitas, &missoes.len()],
                    ))
                    .weak(),
                );
            }
        });
        ui.add_space(4.0);
        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing = vec2(12.0, 12.0);
            for &i in &missoes {
                let m = &p.missoes[i];
                let liberada = prova || app.progresso.liberada(&p, i);
                let bloqueio = (!liberada).then(|| {
                    if i == 0 || p.missoes[i - 1].ato != m.ato {
                        texto::bloqueada_ato()
                    } else {
                        texto::bloqueada_anterior()
                    }
                });
                let c = Cartao {
                    numero: numero_missao(&p, i),
                    titulo: &m.missao.titulo,
                    // An exam shows no grade before the teacher's correction.
                    reg: app.progresso.registro(&m.missao.id).filter(|_| !prova),
                    bloqueio,
                    proxima: proxima == Some(i),
                };
                match cartao(ui, &c) {
                    Some(true) => clique = Some(Clique::Missao(i)),
                    Some(false) => clique = Some(Clique::Areia(i)),
                    None => {}
                }
            }
        });
    }
    clique
}

struct Cartao<'a> {
    numero: String,
    titulo: &'a str,
    reg: Option<&'a RegistroMissao>,
    bloqueio: Option<&'static str>,
    proxima: bool,
}

/// One card. `Some(true)`: open the mission; `Some(false)`: open the sandbox.
fn cartao(ui: &mut Ui, c: &Cartao<'_>) -> Option<bool> {
    let ativo = c.bloqueio.is_none();
    let (r, resp) = ui.allocate_exact_size(
        vec2(LARGURA, ALTURA),
        if ativo {
            Sense::click()
        } else {
            Sense::hover()
        },
    );
    let resp = match c.bloqueio {
        Some(m) => resp.on_hover_text(m),
        None => resp.on_hover_cursor(egui::CursorIcon::PointingHand),
    };
    let mut saida = (ativo && resp.clicked()).then_some(true);
    if !ui.is_rect_visible(r) {
        return saida;
    }
    let v = ui.visuals().clone();
    let sobre = ativo && resp.hovered();
    let fundo = if c.proxima {
        ROXO.gamma_multiply(0.16)
    } else if sobre {
        v.widgets.hovered.weak_bg_fill
    } else {
        v.faint_bg_color
    };
    let borda = if c.proxima {
        Stroke::new(1.5, ROXO)
    } else if sobre {
        Stroke::new(1.0, ROXO.gamma_multiply(0.8))
    } else {
        v.widgets.noninteractive.bg_stroke
    };
    let fraco = v.weak_text_color();
    let tinta = if ativo { v.strong_text_color() } else { fraco };
    let x = r.left() + 14.0;
    let direita = pos2(r.right() - 14.0, r.top() + 12.0);
    let baixo = r.bottom() - 26.0;
    {
        let p = ui.painter();
        p.rect(r, CornerRadius::same(8), fundo, borda, StrokeKind::Inside);
        // Top line: the number, and the stars, the lock or the "next" mark on the right.
        p.text(
            pos2(x, r.top() + 12.0),
            Align2::LEFT_TOP,
            &c.numero,
            FontId::monospace(13.0),
            if c.proxima { ROXO } else { fraco },
        );
        if !ativo {
            cadeado(
                p,
                Rect::from_min_size(direita - vec2(12.0, 0.0), vec2(12.0, 15.0)),
                fraco,
            );
        } else if c.proxima {
            p.text(
                direita,
                Align2::RIGHT_TOP,
                texto::mapa_proxima(),
                FontId::proportional(11.0),
                ROXO,
            );
        } else if let Some(reg) = c.reg {
            let n = reg.estrelas.min(3) as usize;
            let marca = format!("{}{}", "★".repeat(n), "☆".repeat(3 - n));
            p.text(
                direita,
                Align2::RIGHT_TOP,
                marca,
                FontId::proportional(14.0),
                if n > 0 { OURO } else { fraco },
            );
        }
        // The title, up to two lines.
        let mut job = egui::text::LayoutJob::simple(
            c.titulo.to_string(),
            FontId::proportional(15.0),
            tinta,
            r.width() - 28.0,
        );
        job.wrap.max_rows = 2;
        let g = p.layout_job(job);
        p.galley(pos2(x, r.top() + 34.0), g, tinta);
        // The grade, above the bottom line.
        let tentou = c.reg.is_some_and(|reg| reg.tentativas > 0);
        if let Some(reg) = c.reg.filter(|_| tentou) {
            p.text(
                pos2(x, r.bottom() - 46.0),
                Align2::LEFT_TOP,
                texto::f(
                    texto::mapa_nota(),
                    &[&format!("{:.1}", reg.melhor_nota), &reg.tentativas],
                ),
                FontId::proportional(12.0),
                fraco,
            );
        }
        // Bottom line: what to do next.
        if c.proxima {
            let txt = if tentou {
                texto::mapa_tentar_de_novo()
            } else {
                texto::mapa_comecar()
            };
            p.text(
                pos2(x, baixo + 4.0),
                Align2::LEFT_TOP,
                txt,
                FontId::proportional(13.0),
                ROXO,
            );
        }
    }
    // The sandbox of a solved mission: a link in the corner of the card (painted, so the
    // strip of cards keeps its layout).
    if c.reg.is_some_and(|reg| reg.resolvida) {
        let g = ui.painter().layout_no_wrap(
            texto::caixa_de_areia().to_string(),
            FontId::proportional(13.0),
            ROXO,
        );
        let b = Rect::from_min_size(
            pos2(r.right() - 14.0 - g.size().x, baixo + 2.0),
            g.size() + vec2(0.0, 4.0),
        );
        let rb = ui
            .interact(b, ui.id().with(("areia", &c.numero)), Sense::click())
            .on_hover_cursor(egui::CursorIcon::PointingHand);
        let cor = if rb.hovered() {
            ui.visuals().strong_text_color()
        } else {
            ROXO
        };
        ui.painter().galley(b.min + vec2(0.0, 2.0), g, cor);
        if rb.hovered() {
            ui.painter().line_segment(
                [pos2(b.left(), b.bottom()), pos2(b.right(), b.bottom())],
                Stroke::new(1.0, cor),
            );
        }
        if rb.clicked() {
            saida = Some(false);
        }
    }
    saida
}

/// A small padlock drawn with lines.
fn cadeado(p: &egui::Painter, r: Rect, cor: Color32) {
    let st = Stroke::new(1.5, cor);
    let corpo = Rect::from_min_max(pos2(r.left(), r.top() + r.height() * 0.45), r.max);
    p.rect_stroke(corpo, CornerRadius::same(2), st, StrokeKind::Middle);
    let c = pos2(r.center().x, r.top() + r.height() * 0.45);
    let raio = r.width() * 0.32;
    let pts: Vec<_> = (0..=12)
        .map(|k| {
            let a = std::f32::consts::PI * (k as f32 / 12.0);
            pos2(c.x - raio * a.cos(), c.y - raio * 1.3 * a.sin())
        })
        .collect();
    p.add(egui::Shape::line(pts, st));
}
