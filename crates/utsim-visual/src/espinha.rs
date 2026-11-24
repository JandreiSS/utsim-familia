//! O Ishikawa como espinha de peixe editável: o efeito na cabeça, um osso por categoria
//! (metade em cima, metade embaixo) e as causas em cartões ao longo do osso.
//!
//! Na tela: clicar no osso (fora dos cartões) acrescenta uma causa; duplo clique num cartão
//! edita o texto (vazio remove); arrastar um cartão para outro osso muda a categoria; duplo
//! clique no nome do osso o renomeia; o botão direito abre o menu (editar, mover para,
//! começar um 5 porquês, remover). Quem chama aplica as [`Acao`]s nos seus dados.

use std::hash::Hash;

use egui::{
    Align2, Color32, CursorIcon, FontId, Id, LayerId, Order, Pos2, Rect, Sense, Shape, Stroke,
    StrokeKind, Ui, pos2, vec2,
};

use crate::cores;
use crate::edicao::{self, Edicao, Fim};
use crate::texto::tx;
use crate::util::cortado;

/// Uma causa no desenho.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Cartao {
    pub texto: String,
    /// A cor da borda (o estado da causa, quando o app tem); `None` usa a do tema.
    pub cor: Option<Color32>,
    /// Um selo curto no canto (por exemplo "5P" quando a causa já tem porquês).
    pub marca: Option<String>,
    /// O texto ao passar o mouse (o texto inteiro quando vazio).
    pub dica: Option<String>,
}

impl Cartao {
    pub fn novo(texto: impl Into<String>) -> Self {
        Self {
            texto: texto.into(),
            ..Self::default()
        }
    }
}

/// Uma categoria (osso) com as causas.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Osso {
    pub nome: String,
    pub cartoes: Vec<Cartao>,
}

/// O que desenhar e o que a pessoa pode fazer.
#[derive(Clone, Debug)]
pub struct Espinha<'a> {
    pub efeito: &'a str,
    pub ossos: &'a [Osso],
    /// Acrescentar, editar, arrastar e remover causas.
    pub editavel: bool,
    /// Renomear e remover ossos (as categorias).
    pub ossos_editaveis: bool,
    /// O item "Começar um 5 porquês" no menu do cartão.
    pub porques: bool,
    /// Pequena, só para ver (a miniatura do A3).
    pub compacta: bool,
}

impl<'a> Espinha<'a> {
    /// Uma espinha editável por inteiro.
    pub fn editavel(efeito: &'a str, ossos: &'a [Osso]) -> Self {
        Self {
            efeito,
            ossos,
            editavel: true,
            ossos_editaveis: true,
            porques: true,
            compacta: false,
        }
    }

    /// Uma miniatura só para ver.
    pub fn miniatura(efeito: &'a str, ossos: &'a [Osso]) -> Self {
        Self {
            efeito,
            ossos,
            editavel: false,
            ossos_editaveis: false,
            porques: false,
            compacta: true,
        }
    }
}

/// O que a pessoa fez no desenho. `osso` e `cartao` são as posições em [`Espinha::ossos`].
#[derive(Clone, Debug, PartialEq)]
pub enum Acao {
    /// Arrastou a causa para outro osso.
    Mover {
        osso: usize,
        cartao: usize,
        para: usize,
    },
    /// Uma causa nova no fim do osso.
    Acrescentar {
        osso: usize,
        texto: String,
    },
    Editar {
        osso: usize,
        cartao: usize,
        texto: String,
    },
    Remover {
        osso: usize,
        cartao: usize,
    },
    /// Começar um 5 porquês a partir desta causa.
    PorQues {
        osso: usize,
        cartao: usize,
    },
    RenomearOsso {
        osso: usize,
        nome: String,
    },
    RemoverOsso {
        osso: usize,
    },
}

// ---- disposição ----------------------------------------------------------------------

struct Medidas {
    cartao: f32,
    folga: f32,
    rotulo: f32,
    cabeca: f32,
    minima: f32,
    fonte: f32,
}

fn medidas(compacta: bool) -> Medidas {
    if compacta {
        Medidas {
            cartao: 15.0,
            folga: 3.0,
            rotulo: 16.0,
            cabeca: 110.0,
            minima: 150.0,
            fonte: 10.0,
        }
    } else {
        Medidas {
            cartao: 24.0,
            folga: 5.0,
            rotulo: 24.0,
            cabeca: 170.0,
            minima: 280.0,
            fonte: 13.0,
        }
    }
}

/// Um osso na tela.
#[derive(Clone, Debug, PartialEq)]
pub struct OssoNaTela {
    /// A faixa do osso: clicar nela acrescenta, soltar um cartão nela muda a categoria.
    pub area: Rect,
    /// A ponta solta (perto do nome) e a ponta na espinha.
    pub inicio: Pos2,
    pub fim: Pos2,
    pub rotulo: Rect,
    pub cima: bool,
    pub cartoes: Vec<Rect>,
    /// Onde entra a próxima causa.
    pub novo: Rect,
}

/// A espinha na tela.
#[derive(Clone, Debug, PartialEq)]
pub struct Disposicao {
    pub rect: Rect,
    /// A altura da espinha central.
    pub meio: f32,
    pub cabeca: Rect,
    pub ossos: Vec<OssoNaTela>,
}

impl Disposicao {
    /// O osso cuja faixa contém o ponto.
    pub fn osso_em(&self, p: Pos2) -> Option<usize> {
        self.ossos.iter().position(|o| o.area.contains(p))
    }

    /// O cartão (osso, posição) sob o ponto.
    pub fn cartao_em(&self, p: Pos2) -> Option<(usize, usize)> {
        self.ossos
            .iter()
            .enumerate()
            .find_map(|(k, o)| o.cartoes.iter().position(|r| r.contains(p)).map(|j| (k, j)))
    }
}

fn lados(n: usize) -> usize {
    n.div_ceil(2)
}

fn altura_do_lado(vagas: usize, m: &Medidas) -> f32 {
    m.rotulo + 8.0 + vagas as f32 * (m.cartao + m.folga) + 12.0
}

fn vagas(contagens: &[usize], editavel: bool) -> (usize, usize) {
    let cima = lados(contagens.len());
    let extra = usize::from(editavel);
    let max = |s: &[usize]| s.iter().copied().max().unwrap_or(0) + extra;
    (
        max(&contagens[..cima]).max(1),
        max(&contagens[cima..]).max(1),
    )
}

/// A altura do desenho para estas quantidades de causas por osso.
pub fn altura(contagens: &[usize], editavel: bool, compacta: bool) -> f32 {
    let m = medidas(compacta);
    let (c, b) = vagas(contagens, editavel);
    (altura_do_lado(c, &m) + altura_do_lado(b, &m)).max(m.minima)
}

/// Onde fica cada osso e cada cartão no retângulo.
pub fn dispor(rect: Rect, contagens: &[usize], editavel: bool, compacta: bool) -> Disposicao {
    let m = medidas(compacta);
    let (vc, vb) = vagas(contagens, editavel);
    let (hc, hb) = (altura_do_lado(vc, &m), altura_do_lado(vb, &m));
    let meio = rect.top() + rect.height() * hc / (hc + hb);
    let x0 = rect.left() + 10.0;
    let x1 = rect.right() - m.cabeca - 10.0;
    let cabeca = Rect::from_center_size(
        pos2(x1 + 6.0 + m.cabeca / 2.0, meio),
        vec2(m.cabeca, (m.cabeca * 0.45).max(40.0)),
    );
    let n = contagens.len();
    let cima = lados(n);
    let mut ossos = Vec::with_capacity(n);
    for (i, &qtd) in contagens.iter().enumerate() {
        let em_cima = i < cima;
        let (j, lado) = if em_cima {
            (i, cima)
        } else {
            (i - cima, n - cima)
        };
        let w = (x1 - x0) / lado.max(1) as f32;
        let (topo, base) = if em_cima {
            (rect.top(), meio)
        } else {
            (meio, rect.bottom())
        };
        let area = Rect::from_min_max(
            pos2(x0 + j as f32 * w, topo),
            pos2(x0 + (j + 1) as f32 * w, base),
        );
        let fim = pos2(area.right() - 4.0, meio);
        let inclinacao = (w * 0.28).min(area.height() * 0.6);
        let (inicio, rotulo) = if em_cima {
            (
                pos2(fim.x - inclinacao, area.top() + m.rotulo),
                Rect::from_min_max(
                    pos2(area.left() + 4.0, area.top() + 2.0),
                    pos2(area.right() - 4.0, area.top() + m.rotulo),
                ),
            )
        } else {
            (
                pos2(fim.x - inclinacao, area.bottom() - m.rotulo),
                Rect::from_min_max(
                    pos2(area.left() + 4.0, area.bottom() - m.rotulo),
                    pos2(area.right() - 4.0, area.bottom() - 2.0),
                ),
            )
        };
        let primeiro = if em_cima {
            area.top() + m.rotulo + 6.0
        } else {
            meio + 8.0
        };
        let vaga = |k: usize| {
            let y = primeiro + k as f32 * (m.cartao + m.folga);
            let centro = y + m.cartao / 2.0;
            let t = ((centro - inicio.y) / (fim.y - inicio.y)).clamp(0.0, 1.0);
            let x_osso = inicio.x + (fim.x - inicio.x) * t;
            let direita = x_osso - 6.0;
            let esquerda = (area.left() + 6.0).min(direita - 48.0);
            Rect::from_min_max(pos2(esquerda, y), pos2(direita, y + m.cartao))
        };
        ossos.push(OssoNaTela {
            area,
            inicio,
            fim,
            rotulo,
            cima: em_cima,
            cartoes: (0..qtd).map(vaga).collect(),
            novo: vaga(qtd),
        });
    }
    Disposicao {
        rect,
        meio,
        cabeca,
        ossos,
    }
}

// ---- desenho e interação ---------------------------------------------------------------

#[derive(Clone, Copy, Debug, PartialEq)]
enum Alvo {
    Cartao(usize, usize),
    Novo(usize),
    Osso(usize),
}

#[derive(Clone, Debug, Default)]
struct Estado {
    edicao: Option<Edicao<Alvo>>,
    arrasto: Option<(usize, usize)>,
    /// A remoção pedida pelo menu, esperando a confirmação.
    remover: Option<Alvo>,
}

/// Desenha a espinha e devolve o que a pessoa fez neste quadro.
pub fn mostrar(ui: &mut Ui, id: impl Hash + std::fmt::Debug, e: &Espinha) -> Vec<Acao> {
    let id = Id::new(id);
    let m = medidas(e.compacta);
    let contagens: Vec<usize> = e.ossos.iter().map(|o| o.cartoes.len()).collect();
    let minima = if e.compacta { 320.0 } else { 600.0 };
    let largura = ui.available_width().max(minima);
    let h = altura(&contagens, e.editavel, e.compacta);
    let (rect, _) = ui.allocate_exact_size(vec2(largura, h), Sense::hover());
    let d = dispor(rect, &contagens, e.editavel, e.compacta);
    let mut est: Estado = ui.data(|mem| mem.get_temp(id)).unwrap_or_default();
    let mut acoes = Vec::new();
    // Um estado velho (o osso ou o cartão sumiu) não vale mais.
    if est
        .arrasto
        .is_some_and(|(k, j)| contagens.get(k).is_none_or(|&n| j >= n))
    {
        est.arrasto = None;
    }

    let fg = ui.visuals().text_color();
    let fraco = ui.visuals().weak_text_color();
    let acento = cores::destaque(ui);
    let fundo = cores::cartao(ui);
    let fonte = FontId::proportional(m.fonte);
    let ponteiro = ui.input(|i| i.pointer.latest_pos());
    let alvo_do_arrasto = est.arrasto.and(ponteiro).and_then(|p| d.osso_em(p));

    // As faixas dos ossos (embaixo de tudo): clicar acrescenta.
    for (k, o) in d.ossos.iter().enumerate() {
        let sentido = if e.editavel {
            Sense::click()
        } else {
            Sense::hover()
        };
        let r = ui.interact(o.area, id.with(("faixa", k)), sentido);
        let destino = est.arrasto.is_some_and(|(a, _)| a != k) && alvo_do_arrasto == Some(k);
        if destino {
            ui.painter()
                .rect_filled(o.area, 6.0, acento.gamma_multiply(0.12));
            ui.painter().rect_stroke(
                o.area.shrink(1.0),
                6.0,
                Stroke::new(1.5, acento),
                StrokeKind::Inside,
            );
        } else if e.editavel && r.hovered() && est.arrasto.is_none() {
            ui.painter()
                .rect_filled(o.area, 6.0, acento.gamma_multiply(0.05));
            let n = o.novo;
            ui.painter()
                .rect_stroke(n, 4.0, Stroke::new(1.0, fraco), StrokeKind::Inside);
            ui.painter().text(
                n.left_center() + vec2(6.0, 0.0),
                Align2::LEFT_CENTER,
                tx("visual-mais_causa"),
                fonte.clone(),
                fraco,
            );
        }
        if e.editavel && r.clicked() && est.edicao.is_none() {
            est.edicao = Some(Edicao::nova(Alvo::Novo(k), ""));
        }
        if e.editavel {
            r.on_hover_text(tx("visual-clique_para_acrescentar"));
        }
    }

    // A espinha, a cabeça e os ossos.
    let p = ui.painter_at(rect.expand(2.0));
    let cauda = d.rect.left() + 10.0;
    p.line_segment(
        [pos2(cauda, d.meio), pos2(d.cabeca.left(), d.meio)],
        Stroke::new(if e.compacta { 2.0 } else { 3.0 }, fg),
    );
    let aba = if e.compacta { 8.0 } else { 14.0 };
    p.line_segment(
        [pos2(cauda, d.meio), pos2(cauda - aba * 0.6, d.meio - aba)],
        Stroke::new(2.0, fg),
    );
    p.line_segment(
        [pos2(cauda, d.meio), pos2(cauda - aba * 0.6, d.meio + aba)],
        Stroke::new(2.0, fg),
    );
    p.rect_filled(d.cabeca, 10.0, acento.gamma_multiply(0.18));
    p.rect_stroke(d.cabeca, 10.0, Stroke::new(2.0, acento), StrokeKind::Inside);
    let efeito = if e.efeito.trim().is_empty() {
        tx("visual-efeito_sem_nome")
    } else {
        e.efeito
    };
    let g = cortado(
        &p,
        efeito,
        FontId::proportional(m.fonte + 1.0),
        fg,
        d.cabeca.width() - 14.0,
        if e.compacta { 3 } else { 4 },
    );
    p.galley(d.cabeca.center() - g.size() / 2.0, g, fg);

    for (k, (o, osso)) in d.ossos.iter().zip(e.ossos).enumerate() {
        p.line_segment([o.inicio, o.fim], Stroke::new(1.8, fg.gamma_multiply(0.85)));
        // Os ganchos dos cartões no osso.
        for r in &o.cartoes {
            p.line_segment(
                [r.right_center(), r.right_center() + vec2(6.0, 0.0)],
                Stroke::new(1.0, fraco),
            );
        }
        let editando_osso = matches!(
            est.edicao.as_ref().map(|x| x.alvo),
            Some(Alvo::Osso(x)) if x == k
        );
        if !editando_osso {
            let nome = format!("{} ({})", osso.nome, osso.cartoes.len());
            let g = cortado(
                &p,
                &nome,
                FontId::proportional(m.fonte + 1.0),
                acento,
                o.rotulo.width(),
                1,
            );
            let pos = Align2::CENTER_CENTER
                .align_size_within_rect(g.size(), o.rotulo)
                .min;
            p.galley(pos, g, acento);
        }
        if e.ossos_editaveis {
            let r = ui.interact(o.rotulo, id.with(("rotulo", k)), Sense::click());
            if r.double_clicked() && est.edicao.is_none() {
                est.edicao = Some(Edicao::nova(Alvo::Osso(k), &osso.nome));
            }
            r.on_hover_text(tx("visual-duplo_clique_renomeia"))
                .context_menu(|ui| {
                    if ui.button(tx("visual-renomear")).clicked() {
                        est.edicao = Some(Edicao::nova(Alvo::Osso(k), &osso.nome));
                        ui.close();
                    }
                    if ui.button(tx("visual-remover_categoria")).clicked() {
                        est.remover = Some(Alvo::Osso(k));
                        ui.close();
                    }
                });
        }
    }

    // Os cartões.
    let mut soltou: Option<((usize, usize), Option<usize>)> = None;
    for (k, (o, osso)) in d.ossos.iter().zip(e.ossos).enumerate() {
        for (j, (r, c)) in o.cartoes.iter().zip(&osso.cartoes).enumerate() {
            let editando = matches!(
                est.edicao.as_ref().map(|x| x.alvo),
                Some(Alvo::Cartao(a, b)) if (a, b) == (k, j)
            );
            let sentido = if e.editavel {
                Sense::click_and_drag()
            } else {
                Sense::hover()
            };
            let resp = ui.interact(*r, id.with(("cartao", k, j)), sentido);
            let arrastando = est.arrasto == Some((k, j));
            if !editando {
                let borda = c.cor.unwrap_or(fraco);
                let alfa = if arrastando { 0.35 } else { 1.0 };
                p.rect_filled(*r, 4.0, fundo.gamma_multiply(alfa));
                p.rect_stroke(
                    *r,
                    4.0,
                    Stroke::new(
                        if resp.hovered() { 1.8 } else { 1.2 },
                        borda.gamma_multiply(alfa),
                    ),
                    StrokeKind::Inside,
                );
                let selo = c.marca.as_deref().unwrap_or("");
                let reserva = if selo.is_empty() { 0.0 } else { 26.0 };
                let g = cortado(
                    &p,
                    if c.texto.trim().is_empty() {
                        "…"
                    } else {
                        &c.texto
                    },
                    FontId::proportional(m.fonte),
                    fg.gamma_multiply(alfa),
                    r.width() - 10.0 - reserva,
                    1,
                );
                p.galley(pos2(r.left() + 5.0, r.center().y - g.size().y / 2.0), g, fg);
                if !selo.is_empty() {
                    let s = Rect::from_min_max(
                        pos2(r.right() - reserva, r.top() + 3.0),
                        pos2(r.right() - 3.0, r.bottom() - 3.0),
                    );
                    p.rect_filled(s, 3.0, acento.gamma_multiply(0.85));
                    p.text(
                        s.center(),
                        Align2::CENTER_CENTER,
                        selo,
                        FontId::proportional((m.fonte - 3.0).max(8.0)),
                        Color32::WHITE,
                    );
                }
            }
            if !e.editavel {
                resp.on_hover_text(c.dica.clone().unwrap_or_else(|| c.texto.clone()));
                continue;
            }
            if resp.drag_started() && est.edicao.is_none() {
                est.arrasto = Some((k, j));
            }
            if resp.dragged() && arrastando {
                ui.ctx().set_cursor_icon(CursorIcon::Grabbing);
            }
            if resp.drag_stopped() && est.arrasto == Some((k, j)) {
                soltou = Some(((k, j), ponteiro.and_then(|p| d.osso_em(p))));
            }
            if resp.double_clicked() && est.edicao.is_none() {
                est.edicao = Some(Edicao::nova(Alvo::Cartao(k, j), &c.texto));
            }
            let resp =
                if resp.dragged() {
                    resp
                } else {
                    resp.on_hover_text(c.dica.clone().unwrap_or_else(|| {
                        format!("{}\n\n{}", c.texto, tx("visual-dica_do_cartao"))
                    }))
                };
            resp.context_menu(|ui| {
                if ui.button(tx("visual-editar")).clicked() {
                    est.edicao = Some(Edicao::nova(Alvo::Cartao(k, j), &c.texto));
                    ui.close();
                }
                if e.porques && ui.button(tx("visual-comecar_5_porques")).clicked() {
                    acoes.push(Acao::PorQues { osso: k, cartao: j });
                    ui.close();
                }
                ui.menu_button(tx("visual-mover_para"), |ui| {
                    for (para, outro) in e.ossos.iter().enumerate() {
                        if para != k && ui.button(&outro.nome).clicked() {
                            acoes.push(Acao::Mover {
                                osso: k,
                                cartao: j,
                                para,
                            });
                            ui.close();
                        }
                    }
                });
                ui.separator();
                if ui.button(tx("visual-remover")).clicked() {
                    est.remover = Some(Alvo::Cartao(k, j));
                    ui.close();
                }
            });
        }
    }

    // O cartão que vai com o ponteiro.
    if let (Some((k, j)), Some(pt)) = (est.arrasto, ponteiro)
        && soltou.is_none()
        && let (Some(o), Some(c)) = (
            d.ossos.get(k),
            e.ossos.get(k).and_then(|x| x.cartoes.get(j)),
        )
    {
        let r = Rect::from_center_size(pt, o.cartoes[j].size());
        let topo = ui
            .ctx()
            .layer_painter(LayerId::new(Order::Tooltip, id.with("arrasto")));
        topo.add(Shape::rect_filled(r, 4.0, fundo));
        topo.rect_stroke(r, 4.0, Stroke::new(2.0, acento), StrokeKind::Inside);
        let g = cortado(&topo, &c.texto, fonte.clone(), fg, r.width() - 10.0, 1);
        topo.galley(pos2(r.left() + 5.0, r.center().y - g.size().y / 2.0), g, fg);
    }
    if let Some(((k, j), destino)) = soltou {
        est.arrasto = None;
        if let Some(para) = destino
            && para != k
        {
            acoes.push(Acao::Mover {
                osso: k,
                cartao: j,
                para,
            });
        }
    }
    if est.arrasto.is_some() && !ui.input(|i| i.pointer.any_down()) {
        est.arrasto = None;
    }

    // A confirmação da remoção pedida pelo menu, junto do cartão ou do nome do osso.
    if let Some(alvo) = est.remover {
        let lugar = match alvo {
            Alvo::Cartao(k, j) => d
                .ossos
                .get(k)
                .and_then(|o| o.cartoes.get(j))
                .zip(e.ossos.get(k).and_then(|o| o.cartoes.get(j)))
                .map(|(r, c)| {
                    (
                        r.left_bottom(),
                        crate::texto::fx(
                            "visual-confirmar_remocao",
                            &[("nome", c.texto.clone().into())],
                        ),
                    )
                }),
            Alvo::Osso(k) => d.ossos.get(k).zip(e.ossos.get(k)).map(|(o, osso)| {
                (
                    o.rotulo.left_bottom(),
                    crate::texto::fx(
                        "visual-confirmar_remocao_categoria",
                        &[
                            ("nome", osso.nome.clone().into()),
                            ("n", (osso.cartoes.len() as i64).into()),
                        ],
                    ),
                )
            }),
            Alvo::Novo(_) => None,
        };
        match lugar {
            None => est.remover = None,
            Some((pos, pergunta)) => {
                match crate::util::confirmar_remocao(ui.ctx(), id.with("confirmar"), pos, &pergunta)
                {
                    Some(true) => {
                        est.remover = None;
                        match alvo {
                            Alvo::Cartao(k, j) => acoes.push(Acao::Remover { osso: k, cartao: j }),
                            Alvo::Osso(k) => acoes.push(Acao::RemoverOsso { osso: k }),
                            Alvo::Novo(_) => {}
                        }
                    }
                    Some(false) => est.remover = None,
                    None => {}
                }
            }
        }
    }

    // O campo de edição em cima do desenho.
    if let Some(mut ed) = est.edicao.take() {
        let (r, dica) = match ed.alvo {
            Alvo::Cartao(k, j) => (
                d.ossos.get(k).and_then(|o| o.cartoes.get(j)).copied(),
                tx("visual-causa"),
            ),
            Alvo::Novo(k) => (d.ossos.get(k).map(|o| o.novo), tx("visual-nova_causa")),
            Alvo::Osso(k) => (d.ossos.get(k).map(|o| o.rotulo), tx("visual-categoria")),
        };
        if let Some(r) = r {
            let r = Rect::from_min_size(r.min, vec2(r.width().max(180.0), r.height().max(22.0)));
            match edicao::campo(ui, id.with("campo"), r, &mut ed, dica) {
                Fim::Nada => est.edicao = Some(ed),
                Fim::Cancelar => {}
                Fim::Gravar { texto, enter } => match ed.alvo {
                    Alvo::Cartao(k, j) => {
                        let antes = &e.ossos[k].cartoes[j].texto;
                        if texto.is_empty() {
                            acoes.push(Acao::Remover { osso: k, cartao: j });
                        } else if texto != antes.trim() {
                            acoes.push(Acao::Editar {
                                osso: k,
                                cartao: j,
                                texto,
                            });
                        }
                    }
                    Alvo::Novo(k) => {
                        if !texto.is_empty() {
                            acoes.push(Acao::Acrescentar { osso: k, texto });
                            if enter {
                                // Enter segue com a próxima causa no mesmo osso.
                                est.edicao = Some(Edicao::nova(Alvo::Novo(k), ""));
                            }
                        }
                    }
                    Alvo::Osso(k) => {
                        if !texto.is_empty() && texto != e.ossos[k].nome.trim() {
                            acoes.push(Acao::RenomearOsso {
                                osso: k,
                                nome: texto,
                            });
                        }
                    }
                },
            }
        }
    }
    ui.data_mut(|mem| mem.insert_temp(id, est));
    acoes
}

#[cfg(test)]
mod tests {
    use super::*;

    fn caixa() -> Rect {
        Rect::from_min_size(pos2(0.0, 0.0), vec2(900.0, 400.0))
    }

    #[test]
    fn os_ossos_se_dividem_em_cima_e_embaixo_e_os_cartoes_ficam_no_seu_osso() {
        let cont = [2, 0, 3, 1, 0, 4];
        let h = altura(&cont, true, false);
        let r = Rect::from_min_size(pos2(0.0, 0.0), vec2(900.0, h));
        let d = dispor(r, &cont, true, false);
        assert_eq!(d.ossos.len(), 6);
        assert_eq!(
            d.ossos.iter().filter(|o| o.cima).count(),
            3,
            "metade em cima"
        );
        for (k, o) in d.ossos.iter().enumerate() {
            assert_eq!(o.cartoes.len(), cont[k]);
            for c in o.cartoes.iter().chain([&o.novo]) {
                assert!(o.area.contains(c.center()), "cartão dentro do osso {k}");
                assert!(c.right() < o.fim.x, "à esquerda do osso");
            }
            assert_eq!(d.osso_em(o.area.center()), Some(k));
            if o.cima {
                assert!(o.area.bottom() <= d.meio + 0.1);
            } else {
                assert!(o.area.top() >= d.meio - 0.1);
            }
        }
        assert_eq!(d.cartao_em(d.ossos[2].cartoes[1].center()), Some((2, 1)));
        assert_eq!(d.cartao_em(d.cabeca.center()), None);
        assert!(d.cabeca.right() <= r.right());
    }

    #[test]
    fn a_altura_cresce_com_as_causas() {
        let pouco = altura(&[1, 1, 1, 1], true, false);
        let muito = altura(&[9, 1, 1, 1], true, false);
        assert!(muito > pouco);
        assert!(altura(&[], true, false) >= 280.0);
        let d = dispor(caixa(), &[], true, false);
        assert!(d.ossos.is_empty());
        assert_eq!(d.osso_em(pos2(10.0, 10.0)), None);
    }
}
