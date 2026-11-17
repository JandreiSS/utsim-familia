//! The drawn statement at the top of a mission: the 3D picture of the process with the cards
//! of the problem pinned to its objects (`utsim_ensino_core::cena`). The values are live:
//! the loads follow the tasks the student drags, the measures show the result after the run.

use egui::{
    Color32, CornerRadius, FontId, Galley, Id, Pos2, Rect, Sense, Stroke, StrokeKind,
    TextureHandle, TextureOptions, Ui, pos2, vec2,
};
use std::sync::Arc;
use utsim_ensino_core::cena::{Carga, Conteudo, conteudo};
use utsim_ensino_core::missao::{Decisoes, Objetivo};
use utsim_ensino_core::nota::{Resultado, fmt_num};
use utsim_ensino_core::pacote::MissaoCarregada;

use crate::texto;

/// The colours of the models of a mix, in their order (the same of the products in the
/// picture would be better, but the picture is fixed: the colour is in the chips).
const MODELOS: [Color32; 4] = [
    Color32::from_rgb(26, 107, 217),
    Color32::from_rgb(232, 126, 20),
    Color32::from_rgb(46, 160, 90),
    Color32::from_rgb(140, 82, 200),
];
const ALERTA: Color32 = Color32::from_rgb(214, 48, 49);
const OK: Color32 = Color32::from_rgb(46, 160, 90);
const TINTA: Color32 = Color32::from_rgb(28, 34, 44);
const APAGADO: Color32 = Color32::from_rgb(96, 104, 116);
const PAPEL: Color32 = Color32::from_rgba_premultiplied(250, 251, 252, 245);
const PERGUNTA: Color32 = Color32::from_rgba_premultiplied(255, 249, 219, 245);
const NEUTRO: Color32 = Color32::from_rgb(52, 60, 72);

fn textura(ui: &Ui, chave: &str, png: &[u8]) -> Option<TextureHandle> {
    let id = Id::new(("cena", chave));
    if let Some(t) = ui.ctx().data(|d| d.get_temp::<TextureHandle>(id)) {
        return Some(t);
    }
    let img = utsim_ui_tema::familia::imagem_png(png)?;
    let lado = ui.ctx().input(|i| i.max_texture_side);
    if img.size[0] > lado || img.size[1] > lado {
        return None;
    }
    let t = ui
        .ctx()
        .load_texture(format!("cena-{chave}"), img, TextureOptions::LINEAR);
    ui.ctx().data_mut(|d| d.insert_temp(id, t.clone()));
    Some(t)
}

/// One line of a card: a piece of text, or a coloured chip.
enum Linha {
    Fraca(String),
    Forte(String),
    Chip {
        texto: String,
        cor: Color32,
        alerta: bool,
    },
}

/// A laid-out line of a card, with the colour and the alert of its chip (when it is one).
type LinhaPronta = (Arc<Galley>, Option<(Color32, bool)>);

struct Bloco {
    ancora: Pos2,
    abaixo: bool,
    /// A label on the floor (a machine of a layout): right over its anchor, without a stem.
    rente: bool,
    fundo: Color32,
    borda: Option<Color32>,
    linhas: Vec<LinhaPronta>,
    tamanho: egui::Vec2,
}

fn numero(v: f64, casas: u32) -> String {
    fmt_num(v, casas)
}

fn linhas_de(c: &Conteudo, unidade: &str) -> (Vec<Linha>, Color32, Option<Color32>) {
    let alerta = c.alerta().then_some(ALERTA);
    match c {
        Conteudo::Estacao { n, tarefas, cargas } => {
            let mut v = vec![Linha::Forte(texto::f(texto::cena_estacao(), &[n]))];
            v.push(Linha::Fraca(if tarefas.is_empty() {
                texto::cena_sem_tarefas().to_string()
            } else {
                tarefas.join(" ")
            }));
            for Carga {
                modelo,
                nome,
                valor,
                passa,
            } in cargas
            {
                let t = format!("{} {}", numero(*valor, 0), unidade);
                v.push(Linha::Chip {
                    texto: if nome.is_empty() {
                        texto::f(texto::cena_carga(), &[&t])
                    } else {
                        format!("{nome} {t}")
                    },
                    cor: modelo.map_or(NEUTRO, |k| MODELOS[k % MODELOS.len()]),
                    alerta: *passa,
                });
            }
            (v, PAPEL, alerta)
        }
        Conteudo::Alavanca { rotulo, valor } => (
            vec![Linha::Fraca(rotulo.clone()), Linha::Forte(valor.clone())],
            PAPEL,
            None,
        ),
        Conteudo::Metrica {
            rotulo,
            unidade,
            casas,
            objetivo,
            meta,
            valor,
            ok,
        } => {
            let mut v = vec![Linha::Fraca(rotulo.clone())];
            let meta_txt = meta.map(|m| {
                let n = format!("{} {}", numero(m, *casas), unidade);
                match objetivo {
                    Objetivo::Maior => texto::f(texto::cena_meta_maior(), &[&n.trim()]),
                    Objetivo::Menor => texto::f(texto::cena_meta_menor(), &[&n.trim()]),
                }
            });
            match valor {
                Some(x) => {
                    v.push(Linha::Forte(
                        format!("{} {}", numero(*x, *casas), unidade)
                            .trim()
                            .to_string(),
                    ));
                    if let Some(t) = meta_txt {
                        v.push(Linha::Fraca(t));
                    }
                }
                None => v.push(Linha::Forte(
                    meta_txt.unwrap_or_else(|| texto::cena_simule().to_string()),
                )),
            }
            let borda = match ok {
                Some(true) => Some(OK),
                Some(false) => Some(ALERTA),
                None => None,
            };
            (v, PAPEL, borda)
        }
        Conteudo::Previsao { pergunta } => (
            vec![
                Linha::Fraca(texto::cena_previsao().to_string()),
                Linha::Forte(pergunta.clone()),
            ],
            PERGUNTA,
            Some(Color32::from_rgb(214, 158, 46)),
        ),
        Conteudo::Valor {
            rotulo,
            valor,
            unidade,
            casas,
        } => (
            vec![
                Linha::Fraca(rotulo.clone()),
                Linha::Forte(
                    format!("{} {}", numero(*valor, *casas), unidade)
                        .trim()
                        .to_string(),
                ),
            ],
            PAPEL,
            None,
        ),
        Conteudo::Texto(t) => (vec![Linha::Forte(t.clone())], PAPEL, None),
        Conteudo::Maquina { nome, fixa } => (
            vec![if *fixa {
                Linha::Fraca(nome.clone())
            } else {
                Linha::Forte(nome.clone())
            }],
            PAPEL,
            None,
        ),
    }
}

/// Draws the scene of the mission, when it has one, at the width of `ui`.
pub fn desenhar(ui: &mut Ui, mc: &MissaoCarregada, d: &Decisoes, resultado: Option<&Resultado>) {
    let Some(cc) = &mc.cena else {
        return;
    };
    let Some(tex) = textura(ui, &format!("{}:{}", mc.missao.id, cc.cena.imagem), &cc.png) else {
        return;
    };
    let missao = &mc.missao;
    // The numbers of the problem that are not on an object: the takt and the mix.
    if let Some(b) = &missao.balanceamento {
        ui.horizontal_wrapped(|ui| {
            chip_ui(
                ui,
                &texto::f(
                    texto::cena_takt(),
                    &[&format!("{} {}", numero(b.takt, 0), cc.cena.unidade)],
                ),
                NEUTRO,
            );
            let total: f64 = b.mix.iter().map(|m| m.parcela).sum();
            for (k, m) in b.mix.iter().enumerate() {
                chip_ui(
                    ui,
                    &texto::f(
                        texto::cena_parcela(),
                        &[&numero(100.0 * m.parcela / total, 0), &m.nome],
                    ),
                    MODELOS[k % MODELOS.len()],
                );
            }
            ui.colored_label(ALERTA, texto::cena_passa_takt());
        });
    }
    let a = &cc.ancoras;
    // At most 80 % of the size of the picture in pixels: a narrow scene is not blown up.
    let largura = ui.available_width().min(a.largura as f32 * 0.8);
    let s = largura / a.largura as f32;
    // The cards are laid out first, in the coordinates of the picture (0, 0 at its corner);
    // the drawing then grows above and below the picture as far as they go.
    let r = Rect::from_min_size(pos2(0.0, 0.0), vec2(largura, a.altura as f32 * s));
    let medidor = ui.painter().clone();
    let fonte = FontId::proportional(13.5);
    let fonte_forte = FontId::proportional(14.5);
    let largura_max = 260.0;
    let mut blocos: Vec<Bloco> = Vec::new();
    for c in &cc.cena.cartoes {
        let no = utsim_ensino_core::cena::Cena::ancora(missao, c, d);
        let Some(p) = a.nos.get(&no) else { continue };
        let Some(ct) = conteudo(missao, c, d, resultado) else {
            continue;
        };
        let (linhas, fundo, borda) = linhas_de(&ct, &cc.cena.unidade);
        let mut gs = Vec::new();
        let mut w: f32 = 0.0;
        let mut h: f32 = 0.0;
        for l in linhas {
            let (txt, cor, f, chip) = match l {
                Linha::Fraca(t) => (t, APAGADO, fonte.clone(), None),
                Linha::Forte(t) => (t, TINTA, fonte_forte.clone(), None),
                Linha::Chip { texto, cor, alerta } => (
                    texto,
                    Color32::WHITE,
                    fonte_forte.clone(),
                    Some((cor, alerta)),
                ),
            };
            let g = medidor.layout(txt, f, cor, largura_max);
            let extra = if chip.is_some() { 12.0 } else { 0.0 };
            w = w.max(g.size().x + extra);
            h += g.size().y + if chip.is_some() { 8.0 } else { 2.0 };
            gs.push((g, chip));
        }
        blocos.push(Bloco {
            ancora: r.min + vec2(p[0] as f32 * s, p[1] as f32 * s),
            abaixo: c.abaixo() && !matches!(ct, Conteudo::Maquina { .. }),
            rente: matches!(ct, Conteudo::Maquina { .. }),
            fundo,
            borda,
            linhas: gs,
            tamanho: vec2(w + 16.0, h + 10.0),
        });
    }
    // Left to right, each card goes over (or under) its anchor and climbs until it does
    // not cover a card already placed.
    let mut ordem: Vec<usize> = (0..blocos.len()).collect();
    ordem.sort_by(|i, j| blocos[*i].ancora.x.total_cmp(&blocos[*j].ancora.x));
    let mut postos: Vec<Rect> = Vec::new();
    let mut lugares = vec![Rect::NOTHING; blocos.len()];
    for i in ordem {
        let b = &blocos[i];
        let x = (b.ancora.x - b.tamanho.x / 2.0).clamp(
            r.left() + 4.0,
            (r.right() - b.tamanho.x - 4.0).max(r.left()),
        );
        let mut y = if b.abaixo {
            b.ancora.y + if b.rente { 2.0 } else { 16.0 }
        } else {
            b.ancora.y - if b.rente { 2.0 } else { 16.0 } - b.tamanho.y
        };
        let mut caixa = Rect::from_min_size(pos2(x, y), b.tamanho);
        for _ in 0..blocos.len() {
            let Some(o) = postos.iter().find(|o| o.expand(3.0).intersects(caixa)) else {
                break;
            };
            y = if b.abaixo {
                o.bottom() + 6.0
            } else {
                o.top() - 6.0 - b.tamanho.y
            };
            caixa = Rect::from_min_size(pos2(x, y), b.tamanho);
        }
        postos.push(caixa);
        lugares[i] = caixa;
    }
    let limites = lugares.iter().fold(r, |acc, c| acc.union(*c)).expand(4.0);
    let (resp, painter) = ui.allocate_painter(limites.size(), Sense::hover());
    let desloca = resp.rect.min - limites.min;
    let uv = Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0));
    painter.image(tex.id(), r.translate(desloca), uv, Color32::WHITE);
    // The machines of a layout, each moved from the reference cell to its cell, from the back
    // of the grid to the front (the cells higher in the picture are farther).
    if let Some(ar) = &missao.arranjo {
        let pos = ar.posicoes(&missao.completar(d));
        let ref_ = a.nos.get(&utsim_ensino_core::cena::ancora_celula(0));
        let mut pecas: Vec<(f32, Rect, TextureHandle)> = Vec::new();
        for (id, png) in &cc.maquinas {
            let (Some(cel), Some(r0)) = (pos.get(id), ref_) else {
                continue;
            };
            let Some(pc) = a.nos.get(&utsim_ensino_core::cena::ancora_celula(*cel)) else {
                continue;
            };
            let Some(t) = textura(ui, &format!("{}:{id}", mc.missao.id), png) else {
                continue;
            };
            let delta = vec2((pc[0] - r0[0]) as f32 * s, (pc[1] - r0[1]) as f32 * s);
            pecas.push((pc[1] as f32, r.translate(desloca + delta), t));
        }
        pecas.sort_by(|x, y| x.0.total_cmp(&y.0));
        let recorte = painter.with_clip_rect(r.translate(desloca));
        for (_, rect, t) in pecas {
            recorte.image(t.id(), rect, uv, Color32::WHITE);
        }
    }
    for (b, caixa) in blocos.iter().zip(&lugares) {
        let caixa = &caixa.translate(desloca);
        let ancora = b.ancora + desloca;
        let ponta = if b.abaixo {
            pos2(ancora.x, caixa.top())
        } else {
            pos2(ancora.x, caixa.bottom())
        }
        .clamp(caixa.min, caixa.max);
        if !b.rente {
            painter.line_segment([ponta, ancora], Stroke::new(2.0, Color32::WHITE));
            painter.circle_filled(ancora, 4.0, Color32::WHITE);
        }
        painter.rect(
            *caixa,
            CornerRadius::same(7),
            b.fundo,
            b.borda.map_or(Stroke::NONE, |c| Stroke::new(2.5, c)),
            StrokeKind::Inside,
        );
        let mut y = caixa.top() + 5.0;
        for (g, chip) in &b.linhas {
            match chip {
                Some((cor, alerta)) => {
                    let fundo = Rect::from_min_size(
                        pos2(caixa.left() + 6.0, y),
                        vec2(g.size().x + 12.0, g.size().y + 4.0),
                    );
                    painter.rect_filled(fundo, CornerRadius::same(5), *cor);
                    if *alerta {
                        painter.rect_stroke(
                            fundo.expand(2.0),
                            CornerRadius::same(6),
                            Stroke::new(2.5, ALERTA),
                            StrokeKind::Outside,
                        );
                    }
                    painter.galley(fundo.min + vec2(6.0, 2.0), g.clone(), Color32::WHITE);
                    y += g.size().y + 8.0;
                }
                None => {
                    painter.galley(pos2(caixa.left() + 8.0, y), g.clone(), TINTA);
                    y += g.size().y + 2.0;
                }
            }
        }
    }
    ui.add_space(4.0);
}

fn chip_ui(ui: &mut Ui, t: &str, cor: Color32) {
    egui::Frame::new()
        .fill(cor)
        .corner_radius(CornerRadius::same(6))
        .inner_margin(egui::Margin::symmetric(8, 3))
        .show(ui, |ui| {
            ui.label(egui::RichText::new(t).color(Color32::WHITE).strong());
        });
}
