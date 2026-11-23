//! Uma árvore desenhada da esquerda para a direita: o problema na raiz, cada porquê ligado
//! ao de antes e, nas pontas, a ação. Serve aos 5 porquês do Qualidade, do Segurança e do
//! LSS.
//!
//! Na tela: duplo clique edita o nó; o "+" à direita do nó (ou o menu) acrescenta um
//! porquê abaixo dele, abrindo um ramo novo quando ele já tem filhos; o botão direito abre
//! o menu (editar, acrescentar, marcar e remover).

use std::hash::Hash;

use egui::epaint::CubicBezierShape;
use egui::{Align2, Color32, FontId, Id, Pos2, Rect, Sense, Stroke, StrokeKind, Ui, pos2, vec2};

use crate::cores;
use crate::edicao::{self, Edicao, Fim};
use crate::texto::{fx, tx};
use crate::util::cortado;

/// O papel do nó, que dá a cor.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Tipo {
    /// O problema.
    Raiz,
    #[default]
    Porque,
    /// A ação sobre a causa (a ponta do ramo).
    Acao,
}

/// Um nó; `pai` é a posição de outro nó na mesma lista.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct No {
    pub pai: Option<usize>,
    pub texto: String,
    pub tipo: Tipo,
    /// Marcado (a causa raiz do LSS): borda grossa na cor de destaque.
    pub marcado: bool,
    pub editavel: bool,
    pub pode_filho: bool,
    pub removivel: bool,
    pub dica: Option<String>,
}

impl No {
    pub fn novo(pai: Option<usize>, tipo: Tipo, texto: impl Into<String>) -> Self {
        Self {
            pai,
            texto: texto.into(),
            tipo,
            marcado: false,
            editavel: true,
            pode_filho: tipo != Tipo::Acao,
            removivel: tipo != Tipo::Raiz,
            dica: None,
        }
    }
}

/// O que desenhar.
#[derive(Clone, Debug)]
pub struct Arvore<'a> {
    pub nos: &'a [No],
    /// O item de menu que marca e desmarca o nó (por exemplo "Causa raiz"), quando há.
    pub marcar: Option<&'a str>,
}

/// O que a pessoa fez. `no` é a posição em [`Arvore::nos`].
#[derive(Clone, Debug, PartialEq)]
pub enum Acao {
    Editar {
        no: usize,
        texto: String,
    },
    /// Um porquê novo abaixo do nó.
    Filho {
        no: usize,
        texto: String,
    },
    Remover {
        no: usize,
    },
    Marcar {
        no: usize,
    },
}

pub const LARGURA: f32 = 190.0;
pub const ALTURA: f32 = 46.0;
pub const LINHA: f32 = 60.0;
pub const VAO: f32 = 46.0;
const TOPO: f32 = 16.0;

/// Os filhos de cada nó, na ordem da lista (nós com pai fora da lista viram raízes).
pub fn filhos(nos: &[No]) -> (Vec<usize>, Vec<Vec<usize>>) {
    let mut f = vec![Vec::new(); nos.len()];
    let mut raizes = Vec::new();
    for (i, n) in nos.iter().enumerate() {
        match n.pai {
            Some(p) if p < nos.len() && p != i => f[p].push(i),
            _ => raizes.push(i),
        }
    }
    (raizes, f)
}

/// A caixa de cada nó a partir de `origem`, e o tamanho do desenho. As folhas ocupam
/// linhas seguidas; o pai fica no meio dos filhos.
pub fn dispor(origem: Pos2, nos: &[No]) -> (Vec<Rect>, egui::Vec2) {
    let (raizes, f) = filhos(nos);
    let mut y = vec![0.0f32; nos.len()];
    let mut nivel = vec![0usize; nos.len()];
    let mut visto = vec![false; nos.len()];
    let mut linha = 0usize;
    fn visitar(
        i: usize,
        prof: usize,
        f: &[Vec<usize>],
        y: &mut [f32],
        nivel: &mut [usize],
        visto: &mut [bool],
        linha: &mut usize,
    ) {
        if visto[i] {
            return;
        }
        visto[i] = true;
        nivel[i] = prof;
        let fs: Vec<usize> = f[i].iter().copied().filter(|&c| !visto[c]).collect();
        if fs.is_empty() {
            y[i] = *linha as f32 * LINHA;
            *linha += 1;
            return;
        }
        for &c in &fs {
            visitar(c, prof + 1, f, y, nivel, visto, linha);
        }
        y[i] = (y[fs[0]] + y[fs[fs.len() - 1]]) / 2.0;
    }
    for &r in &raizes {
        visitar(r, 0, &f, &mut y, &mut nivel, &mut visto, &mut linha);
    }
    // Um ciclo deixa nós sem visita: cada um vai numa linha própria, na primeira coluna.
    for i in 0..nos.len() {
        if !visto[i] {
            visitar(i, 0, &f, &mut y, &mut nivel, &mut visto, &mut linha);
        }
    }
    let rects = (0..nos.len())
        .map(|i| {
            Rect::from_min_size(
                origem + vec2(nivel[i] as f32 * (LARGURA + VAO), TOPO + y[i]),
                vec2(LARGURA, ALTURA),
            )
        })
        .collect();
    let colunas = nivel.iter().copied().max().map_or(0, |m| m + 1);
    let tamanho = vec2(
        colunas as f32 * (LARGURA + VAO) + LARGURA * 0.5,
        TOPO + linha.max(1) as f32 * LINHA + LINHA,
    );
    (rects, tamanho)
}

/// A profundidade de cada nó (a raiz é 0).
fn profundidade(nos: &[No], i: usize) -> usize {
    let mut d = 0;
    let mut a = nos[i].pai;
    while let Some(p) = a {
        if p >= nos.len() || d > nos.len() {
            break;
        }
        d += 1;
        a = nos[p].pai;
    }
    d
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum Alvo {
    No(usize),
    Filho(usize),
}

#[derive(Clone, Debug, Default)]
struct Estado {
    edicao: Option<Edicao<Alvo>>,
}

/// O botão "+" do nó.
fn mais(r: Rect) -> Rect {
    Rect::from_center_size(r.right_center() + vec2(12.0, 0.0), vec2(18.0, 18.0))
}

/// Desenha a árvore e devolve o que a pessoa fez neste quadro.
pub fn mostrar(ui: &mut Ui, id: impl Hash + std::fmt::Debug, a: &Arvore) -> Vec<Acao> {
    let id = Id::new(id);
    let origem0 = ui.cursor().min;
    let (_, tamanho) = dispor(origem0, a.nos);
    let (area, _) = ui.allocate_exact_size(
        vec2(tamanho.x.max(ui.available_width()), tamanho.y),
        Sense::hover(),
    );
    let (rects, _) = dispor(area.min, a.nos);
    let (_, f) = filhos(a.nos);
    let mut est: Estado = ui.data(|m| m.get_temp(id)).unwrap_or_default();
    let mut acoes = Vec::new();
    let p = ui.painter_at(area);
    let fg = ui.visuals().text_color();
    let fraco = ui.visuals().weak_text_color();
    let acento = cores::destaque(ui);
    let fundo = cores::cartao(ui);

    // As ligações.
    for (i, n) in a.nos.iter().enumerate() {
        if let Some(pai) = n.pai.filter(|&x| x < a.nos.len() && x != i) {
            let (de, para) = (rects[pai].right_center(), rects[i].left_center());
            let meio = (para.x - de.x) / 2.0;
            p.add(CubicBezierShape::from_points_stroke(
                [de, de + vec2(meio, 0.0), para - vec2(meio, 0.0), para],
                false,
                Color32::TRANSPARENT,
                Stroke::new(1.5, fraco),
            ));
        }
    }

    let editando = est.edicao.as_ref().map(|e| e.alvo);
    for (i, n) in a.nos.iter().enumerate() {
        let r = rects[i];
        let (cor, preenchimento) = match n.tipo {
            Tipo::Raiz => (cores::VERMELHO, cores::VERMELHO.gamma_multiply(0.16)),
            Tipo::Acao => (cores::VERDE, cores::VERDE.gamma_multiply(0.16)),
            Tipo::Porque => (if n.marcado { acento } else { fraco }, fundo),
        };
        let resp = ui.interact(r, id.with(("no", i)), Sense::click());
        if editando != Some(Alvo::No(i)) {
            p.rect_filled(r, 6.0, preenchimento);
            p.rect_stroke(
                r,
                6.0,
                Stroke::new(
                    if n.marcado {
                        2.6
                    } else if resp.hovered() {
                        1.8
                    } else {
                        1.2
                    },
                    cor,
                ),
                StrokeKind::Inside,
            );
            let vazio = n.texto.trim().is_empty();
            let texto = if vazio {
                tx("visual-escreva_aqui")
            } else {
                &n.texto
            };
            let g = cortado(
                &p,
                texto,
                FontId::proportional(12.5),
                if vazio { fraco } else { fg },
                r.width() - 12.0,
                2,
            );
            p.galley(pos2(r.left() + 6.0, r.center().y - g.size().y / 2.0), g, fg);
        }
        let rotulo = match n.tipo {
            Tipo::Raiz => tx("visual-problema").to_string(),
            Tipo::Acao => tx("visual-acao").to_string(),
            Tipo::Porque => fx(
                "visual-porque_n",
                &[("n", (profundidade(a.nos, i) as i64).into())],
            ),
        };
        p.text(
            r.left_top() + vec2(2.0, -2.0),
            Align2::LEFT_BOTTOM,
            rotulo,
            FontId::proportional(10.5),
            if n.marcado { acento } else { fraco },
        );
        if n.editavel && resp.double_clicked() && est.edicao.is_none() {
            est.edicao = Some(Edicao::nova(Alvo::No(i), &n.texto));
        }
        let resp = resp.on_hover_text(n.dica.clone().unwrap_or_else(|| {
            if n.texto.trim().is_empty() {
                tx("visual-dica_do_no").to_string()
            } else {
                format!("{}\n\n{}", n.texto, tx("visual-dica_do_no"))
            }
        }));
        let mostrar_mais = n.pode_filho
            && (resp.hovered()
                || ui
                    .input(|x| x.pointer.latest_pos())
                    .is_some_and(|q| mais(r).contains(q))
                || (f[i].is_empty() && n.tipo == Tipo::Raiz));
        if mostrar_mais {
            let m = mais(r);
            let rm = ui.interact(m, id.with(("mais", i)), Sense::click());
            p.circle_filled(
                m.center(),
                8.0,
                if rm.hovered() {
                    acento
                } else {
                    acento.gamma_multiply(0.7)
                },
            );
            p.text(
                m.center(),
                Align2::CENTER_CENTER,
                "+",
                FontId::proportional(14.0),
                Color32::WHITE,
            );
            if rm.on_hover_text(tx("visual-acrescentar_porque")).clicked() && est.edicao.is_none() {
                est.edicao = Some(Edicao::nova(Alvo::Filho(i), ""));
            }
        }
        resp.context_menu(|ui| {
            if n.editavel && ui.button(tx("visual-editar")).clicked() {
                est.edicao = Some(Edicao::nova(Alvo::No(i), &n.texto));
                ui.close();
            }
            if n.pode_filho && ui.button(tx("visual-acrescentar_porque")).clicked() {
                est.edicao = Some(Edicao::nova(Alvo::Filho(i), ""));
                ui.close();
            }
            if let Some(m) = a.marcar
                && n.tipo == Tipo::Porque
                && ui.checkbox(&mut n.marcado.clone(), m).clicked()
            {
                acoes.push(Acao::Marcar { no: i });
                ui.close();
            }
            if n.removivel {
                ui.separator();
                if ui.button(tx("visual-remover")).clicked() {
                    acoes.push(Acao::Remover { no: i });
                    ui.close();
                }
            }
        });
    }

    if let Some(mut ed) = est.edicao.take()
        && let Some(base) = match ed.alvo {
            Alvo::No(i) | Alvo::Filho(i) => rects.get(i).copied(),
        }
    {
        let r = match ed.alvo {
            Alvo::No(_) => {
                Rect::from_min_size(base.left_center() - vec2(0.0, 12.0), vec2(LARGURA, 24.0))
            }
            Alvo::Filho(i) => {
                // Abaixo do último filho, ou à direita do nó quando ele não tem.
                let y = f[i]
                    .last()
                    .map_or(base.center().y, |&c| rects[c].center().y + LINHA);
                Rect::from_min_size(pos2(base.right() + VAO, y - 12.0), vec2(LARGURA, 24.0))
            }
        };
        match edicao::campo(ui, id.with("campo"), r, &mut ed, tx("visual-por_que")) {
            Fim::Nada => est.edicao = Some(ed),
            Fim::Cancelar => {}
            Fim::Gravar { texto, .. } => match ed.alvo {
                Alvo::No(i) => {
                    if texto != a.nos[i].texto.trim() {
                        acoes.push(Acao::Editar { no: i, texto });
                    }
                }
                Alvo::Filho(i) => {
                    if !texto.is_empty() {
                        acoes.push(Acao::Filho { no: i, texto });
                    }
                }
            },
        }
    }
    ui.data_mut(|m| m.insert_temp(id, est));
    acoes
}

#[cfg(test)]
mod tests {
    use super::*;

    fn no(pai: Option<usize>, t: &str) -> No {
        No::novo(
            pai,
            if pai.is_none() {
                Tipo::Raiz
            } else {
                Tipo::Porque
            },
            t,
        )
    }

    #[test]
    fn o_pai_fica_no_meio_dos_filhos_e_cada_folha_tem_a_sua_linha() {
        // problema → a → (b, c), problema → d
        let nos = [
            no(None, "problema"),
            no(Some(0), "a"),
            no(Some(1), "b"),
            no(Some(1), "c"),
            no(Some(0), "d"),
        ];
        let (r, tam) = dispor(Pos2::ZERO, &nos);
        assert_eq!(r[2].top() + LINHA, r[3].top(), "folhas seguidas");
        assert!((r[1].center().y - (r[2].center().y + r[3].center().y) / 2.0).abs() < 0.1);
        assert!(r[4].top() > r[3].top());
        assert!(r[2].left() > r[1].left() && r[1].left() > r[0].left());
        assert_eq!(r[2].left(), r[3].left());
        assert!(tam.x >= r[2].right() && tam.y >= r[4].bottom());
        assert_eq!(profundidade(&nos, 3), 2);
    }

    #[test]
    fn um_ciclo_nao_trava() {
        let mut a = no(Some(1), "a");
        a.tipo = Tipo::Porque;
        let nos = [a, no(Some(0), "b")];
        let (r, _) = dispor(Pos2::ZERO, &nos);
        assert_eq!(r.len(), 2);
        assert!(profundidade(&nos, 0) <= nos.len() + 1);
    }
}
