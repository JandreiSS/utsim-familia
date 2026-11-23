//! A casa da qualidade do QFD: as necessidades nas linhas (com a importância), os requisitos
//! técnicos nas colunas, a matriz de relações (círculo cheio forte 9, círculo vazio média 3,
//! triângulo fraca 1), o telhado
//! com as correlações entre os requisitos (++, +, −, −−) e o peso de cada requisito embaixo.
//!
//! Clicar numa célula da matriz troca a relação (vazia → 9 → 3 → 1 → vazia); clicar num
//! losango do telhado troca a correlação (vazia → ++ → + → − → −− → vazia).

use std::hash::Hash;

use egui::epaint::TextShape;
use egui::{
    Align2, Color32, FontId, Id, Pos2, Rect, Sense, Shape, Stroke, StrokeKind, Ui, pos2, vec2,
};

use crate::cores;
use crate::texto::tx;
use crate::util::cortado;

/// O que desenhar.
#[derive(Clone, Debug)]
pub struct Casa<'a> {
    /// Nome e importância de cada necessidade.
    pub necessidades: &'a [(String, f64)],
    pub requisitos: &'a [String],
    /// A relação de cada necessidade com cada requisito: 0, 1, 3 ou 9.
    pub relacoes: &'a [Vec<u8>],
    /// As correlações do telhado: (requisito a, requisito b, -2 a 2).
    pub telhado: &'a [(usize, usize, i8)],
    /// O peso de cada requisito em % (vazio não mostra).
    pub pesos: &'a [f64],
    pub editavel: bool,
}

/// O que a pessoa fez.
#[derive(Clone, Debug, PartialEq)]
pub enum Acao {
    Relacao {
        necessidade: usize,
        requisito: usize,
        valor: u8,
    },
    Correlacao {
        a: usize,
        b: usize,
        valor: i8,
    },
}

/// A relação depois de um clique: vazia → 9 → 3 → 1 → vazia.
pub fn proxima_relacao(v: u8) -> u8 {
    match v {
        0 => 9,
        9 => 3,
        3 => 1,
        _ => 0,
    }
}

/// A correlação depois de um clique: vazia → ++ → + → − → −− → vazia.
pub fn proxima_correlacao(v: i8) -> i8 {
    match v {
        0 => 2,
        2 => 1,
        1 => -1,
        -1 => -2,
        _ => 0,
    }
}

/// O símbolo de uma relação desenhado em `c` (a fonte padrão não tem ● e △): círculo
/// cheio para 9, vazio para 3 e triângulo para 1.
pub fn desenhar_relacao(p: &egui::Painter, c: Pos2, v: u8, cor: Color32) {
    let r = 6.5;
    match v {
        9 => {
            p.circle_filled(c, r, cor);
        }
        3 => {
            p.circle_stroke(c, r - 0.5, Stroke::new(1.8, cor));
        }
        1 => {
            p.add(Shape::closed_line(
                vec![
                    c + vec2(0.0, -r),
                    c + vec2(r * 0.9, r * 0.65),
                    c + vec2(-r * 0.9, r * 0.65),
                ],
                Stroke::new(1.6, cor),
            ));
        }
        _ => {}
    }
}

/// A correlação escrita como na tabela do telhado.
pub fn texto_correlacao(v: i8) -> &'static str {
    match v {
        2 => "++",
        1 => "+",
        -1 => "-",
        -2 => "--",
        _ => "",
    }
}

/// A correlação lida da tabela do telhado.
pub fn ler_correlacao(t: &str) -> i8 {
    match t.trim() {
        "++" => 2,
        "+" => 1,
        "-" | "−" => -1,
        "--" | "−−" => -2,
        _ => 0,
    }
}

/// A relação lida de uma célula (9, 3 ou 1; o resto é vazio).
pub fn ler_relacao(t: &str) -> u8 {
    match t.trim() {
        "9" => 9,
        "3" => 3,
        "1" => 1,
        _ => 0,
    }
}

pub const COLUNA: f32 = 34.0;
pub const LINHA: f32 = 26.0;
pub const NOMES: f32 = 200.0;
pub const CABECALHO: f32 = 120.0;

/// A casa na tela.
#[derive(Clone, Debug, PartialEq)]
pub struct Plano {
    /// A matriz de relações.
    pub matriz: Rect,
    /// A base do telhado (o alto do cabeçalho).
    pub base_telhado: f32,
    pub n: usize,
    pub m: usize,
    pub tamanho: egui::Vec2,
}

impl Plano {
    pub fn celula(&self, i: usize, j: usize) -> Rect {
        Rect::from_min_size(
            self.matriz.min + vec2(j as f32 * COLUNA, i as f32 * LINHA),
            vec2(COLUNA, LINHA),
        )
    }

    /// O centro do losango do telhado entre os requisitos a e b (a < b).
    pub fn losango(&self, a: usize, b: usize) -> Pos2 {
        let (a, b) = (a.min(b), a.max(b));
        let xa = self.matriz.left() + (a as f32 + 0.5) * COLUNA;
        let xb = self.matriz.left() + (b as f32 + 0.5) * COLUNA;
        pos2(
            (xa + xb) / 2.0,
            self.base_telhado - (b - a) as f32 * COLUNA / 2.0,
        )
    }

    pub fn celula_em(&self, p: Pos2) -> Option<(usize, usize)> {
        if !self.matriz.contains(p) {
            return None;
        }
        let j = ((p.x - self.matriz.left()) / COLUNA) as usize;
        let i = ((p.y - self.matriz.top()) / LINHA) as usize;
        (i < self.n && j < self.m).then_some((i, j))
    }

    /// O losango (a, b) sob o ponto.
    pub fn losango_em(&self, p: Pos2) -> Option<(usize, usize)> {
        for a in 0..self.m {
            for b in a + 1..self.m {
                let c = self.losango(a, b);
                if (p.x - c.x).abs() + (p.y - c.y).abs() <= COLUNA / 2.0 {
                    return Some((a, b));
                }
            }
        }
        None
    }
}

/// Onde fica cada parte da casa, para `n` necessidades e `m` requisitos.
pub fn dispor(origem: Pos2, n: usize, m: usize) -> Plano {
    let telhado = m.saturating_sub(1) as f32 * COLUNA / 2.0 + COLUNA / 2.0 + 4.0;
    let base_telhado = origem.y + telhado;
    let matriz = Rect::from_min_size(
        pos2(origem.x + NOMES, base_telhado + CABECALHO),
        vec2(m as f32 * COLUNA, n as f32 * LINHA),
    );
    let tamanho = vec2(
        NOMES + m as f32 * COLUNA + 8.0,
        telhado + CABECALHO + n as f32 * LINHA + 2.0 * LINHA + 8.0,
    );
    Plano {
        matriz,
        base_telhado,
        n,
        m,
        tamanho,
    }
}

fn losango_shape(c: Pos2, r: f32, fill: Color32, stroke: Stroke) -> Shape {
    Shape::convex_polygon(
        vec![
            c + vec2(0.0, -r),
            c + vec2(r, 0.0),
            c + vec2(0.0, r),
            c + vec2(-r, 0.0),
        ],
        fill,
        stroke,
    )
}

/// Desenha a casa e devolve o que a pessoa fez.
pub fn mostrar(ui: &mut Ui, id: impl Hash + std::fmt::Debug, c: &Casa) -> Vec<Acao> {
    let id = Id::new(id);
    let (n, m) = (c.necessidades.len(), c.requisitos.len());
    let plano0 = dispor(Pos2::ZERO, n, m);
    let (rect, _) = ui.allocate_exact_size(plano0.tamanho, Sense::hover());
    let pl = dispor(rect.min, n, m);
    let p = ui.painter_at(rect);
    let fg = ui.visuals().text_color();
    let fraco = ui.visuals().weak_text_color();
    let acento = cores::destaque(ui);
    let mut acoes = Vec::new();
    let sentido = if c.editavel {
        Sense::click()
    } else {
        Sense::hover()
    };

    // O telhado.
    if m >= 2 {
        let esq = pos2(pl.matriz.left(), pl.base_telhado);
        let dir = pos2(pl.matriz.right(), pl.base_telhado);
        let topo = pl.losango(0, m - 1) - vec2(0.0, COLUNA / 2.0);
        p.add(Shape::line(vec![esq, topo, dir], Stroke::new(1.2, fraco)));
        let rel: std::collections::HashMap<(usize, usize), i8> = c
            .telhado
            .iter()
            .map(|&(a, b, v)| ((a.min(b), a.max(b)), v))
            .collect();
        for a in 0..m {
            for b in a + 1..m {
                let ctr = pl.losango(a, b);
                let hit = Rect::from_center_size(ctr, vec2(COLUNA * 0.7, COLUNA * 0.7));
                let r = ui.interact(hit, id.with(("telhado", a, b)), sentido);
                let v = rel.get(&(a, b)).copied().unwrap_or(0);
                let cor = if v > 0 {
                    cores::VERDE
                } else if v < 0 {
                    cores::VERMELHO
                } else {
                    fraco
                };
                p.add(losango_shape(
                    ctr,
                    COLUNA / 2.0 - 1.0,
                    if r.hovered() {
                        acento.gamma_multiply(0.2)
                    } else {
                        Color32::TRANSPARENT
                    },
                    Stroke::new(0.6, fraco.gamma_multiply(0.6)),
                ));
                if v != 0 {
                    p.text(
                        ctr,
                        Align2::CENTER_CENTER,
                        texto_correlacao(v).replace('-', "−"),
                        FontId::proportional(11.0),
                        cor,
                    );
                }
                let r = r.on_hover_text(format!("{} × {}", c.requisitos[a], c.requisitos[b]));
                if r.clicked() {
                    acoes.push(Acao::Correlacao {
                        a,
                        b,
                        valor: proxima_correlacao(v),
                    });
                }
            }
        }
    }

    // O cabeçalho: os requisitos deitados.
    for (j, nome) in c.requisitos.iter().enumerate() {
        let col = pl.celula(0, j);
        let g = cortado(&p, nome, FontId::proportional(11.5), fg, CABECALHO - 8.0, 1);
        let ancora = pos2(col.center().x - g.size().y / 2.0, pl.matriz.top() - 4.0);
        p.add(TextShape::new(ancora, g, fg).with_angle(-std::f32::consts::FRAC_PI_2));
        p.line_segment(
            [
                pos2(col.left(), pl.base_telhado),
                pos2(col.left(), pl.matriz.top()),
            ],
            Stroke::new(0.5, fraco.gamma_multiply(0.5)),
        );
    }
    p.text(
        pos2(pl.matriz.left() - 6.0, pl.matriz.top() - 4.0),
        Align2::RIGHT_BOTTOM,
        tx("visual-necessidade_importancia"),
        FontId::proportional(10.5),
        fraco,
    );

    // As linhas.
    for (i, (nome, imp)) in c.necessidades.iter().enumerate() {
        let y = pl.matriz.top() + i as f32 * LINHA;
        let faixa =
            Rect::from_min_size(pos2(rect.left(), y), vec2(NOMES + m as f32 * COLUNA, LINHA));
        if i % 2 == 1 {
            p.rect_filled(faixa, 0.0, fraco.gamma_multiply(0.06));
        }
        let g = cortado(&p, nome, FontId::proportional(12.0), fg, NOMES - 44.0, 1);
        p.galley(
            pos2(rect.left() + 4.0, y + (LINHA - g.size().y) / 2.0),
            g,
            fg,
        );
        p.text(
            pos2(pl.matriz.left() - 6.0, y + LINHA / 2.0),
            Align2::RIGHT_CENTER,
            if imp.fract() == 0.0 {
                format!("{imp:.0}")
            } else {
                format!("{imp:.1}").replace('.', ",")
            },
            FontId::proportional(11.5),
            acento,
        );
        for j in 0..m {
            let cel = pl.celula(i, j);
            let v = c
                .relacoes
                .get(i)
                .and_then(|l| l.get(j))
                .copied()
                .unwrap_or(0);
            let r = ui.interact(cel, id.with(("rel", i, j)), sentido);
            p.rect_stroke(
                cel,
                0.0,
                Stroke::new(0.5, fraco.gamma_multiply(0.6)),
                StrokeKind::Inside,
            );
            if r.hovered() {
                p.rect_filled(cel.shrink(1.0), 2.0, acento.gamma_multiply(0.18));
            }
            desenhar_relacao(&p, cel.center(), v, fg);
            let r = r.on_hover_text(format!("{nome} × {}", c.requisitos[j]));
            if r.clicked() {
                acoes.push(Acao::Relacao {
                    necessidade: i,
                    requisito: j,
                    valor: proxima_relacao(v),
                });
            }
        }
    }

    // O peso de cada requisito: uma barrinha e o número.
    if !c.pesos.is_empty() && m > 0 {
        let y = pl.matriz.bottom() + 6.0;
        let maior = c.pesos.iter().copied().fold(0.0f64, f64::max).max(1e-9);
        p.text(
            pos2(pl.matriz.left() - 6.0, y + LINHA / 2.0),
            Align2::RIGHT_CENTER,
            tx("visual-peso"),
            FontId::proportional(10.5),
            fraco,
        );
        for (j, w) in c.pesos.iter().enumerate().take(m) {
            let col = pl.celula(0, j);
            let h = (LINHA - 4.0) * (*w / maior) as f32;
            let barra = Rect::from_min_max(
                pos2(col.left() + 6.0, y + LINHA - h),
                pos2(col.right() - 6.0, y + LINHA),
            );
            p.rect_filled(barra, 2.0, acento);
            p.text(
                pos2(col.center().x, y + LINHA + 2.0),
                Align2::CENTER_TOP,
                format!("{w:.0}%"),
                FontId::proportional(10.0),
                fg,
            );
        }
    }
    acoes
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn os_cliques_trocam_a_relacao_e_a_correlacao_em_ciclo() {
        let mut v = 0;
        let mut visto = vec![v];
        for _ in 0..4 {
            v = proxima_relacao(v);
            visto.push(v);
        }
        assert_eq!(visto, [0, 9, 3, 1, 0]);
        let mut c = 0;
        let mut visto = vec![c];
        for _ in 0..5 {
            c = proxima_correlacao(c);
            visto.push(c);
        }
        assert_eq!(visto, [0, 2, 1, -1, -2, 0]);
        for x in [-2, -1, 0, 1, 2] {
            assert_eq!(ler_correlacao(texto_correlacao(x)), x);
        }
    }

    #[test]
    fn o_telhado_fica_acima_da_matriz_e_o_losango_e_achado() {
        let pl = dispor(Pos2::ZERO, 3, 4);
        assert_eq!(pl.celula_em(pl.celula(2, 3).center()), Some((2, 3)));
        assert_eq!(pl.celula_em(pos2(0.0, 0.0)), None);
        for (a, b) in [(0, 1), (0, 3), (1, 2), (2, 3)] {
            let c = pl.losango(a, b);
            assert!(c.y < pl.matriz.top());
            assert_eq!(pl.losango_em(c), Some((a, b)));
        }
        assert!(
            pl.losango(0, 3).y < pl.losango(0, 1).y,
            "mais longe, mais alto"
        );
        assert!(pl.losango(0, 3).y >= 0.0);
    }
}
