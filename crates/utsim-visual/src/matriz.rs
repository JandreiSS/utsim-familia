//! A matriz severidade × ocorrência da FMEA: a severidade sobe no eixo vertical, a
//! ocorrência cresce para a direita, o fundo de cada célula mostra a faixa de S × O e cada
//! modo de falha é um ponto na cor do risco da linha (NPR ou AP, quem chama escolhe).
//! Clicar numa célula leva à linha; clicar de novo passa para a próxima linha da mesma
//! célula.

use std::hash::Hash;

use egui::{Align2, Color32, FontId, Id, Pos2, Rect, Sense, Stroke, StrokeKind, Ui, pos2, vec2};

use crate::cores::{self, Nivel};
use crate::texto::{fx, tx};

/// Um modo de falha na matriz.
#[derive(Clone, Debug, PartialEq)]
pub struct Ponto {
    /// A linha da tabela (quem chama sabe o que é).
    pub linha: usize,
    pub s: u8,
    pub o: u8,
    pub nivel: Nivel,
    pub rotulo: String,
}

/// O lado de uma célula.
pub const CELULA: f32 = 24.0;
const MARGEM_ESQ: f32 = 34.0;
const MARGEM_BAIXO: f32 = 34.0;

/// A faixa de S × O do fundo: alta a partir de 50, média a partir de 20.
pub fn faixa(s: u8, o: u8) -> Nivel {
    let so = u32::from(s) * u32::from(o);
    if so >= 50 || (s >= 9 && o >= 4) {
        Nivel::Alto
    } else if so >= 20 {
        Nivel::Medio
    } else {
        Nivel::Baixo
    }
}

/// O retângulo da célula (s, o), notas de 1 a 10.
pub fn celula(grade: Rect, s: u8, o: u8) -> Rect {
    let x = grade.left() + (f32::from(o.clamp(1, 10)) - 1.0) * CELULA;
    let y = grade.top() + (10.0 - f32::from(s.clamp(1, 10))) * CELULA;
    Rect::from_min_size(pos2(x, y), vec2(CELULA, CELULA))
}

/// A célula (s, o) sob o ponto.
pub fn celula_em(grade: Rect, p: Pos2) -> Option<(u8, u8)> {
    if !grade.contains(p) {
        return None;
    }
    let o = ((p.x - grade.left()) / CELULA).floor() as i32 + 1;
    let s = 10 - ((p.y - grade.top()) / CELULA).floor() as i32;
    ((1..=10).contains(&s) && (1..=10).contains(&o)).then_some((s as u8, o as u8))
}

/// A linha a mostrar depois de clicar em (s, o): a primeira da célula, ou a seguinte à
/// que está escolhida quando ela já é desta célula.
pub fn proxima(pontos: &[Ponto], s: u8, o: u8, atual: Option<usize>) -> Option<usize> {
    let aqui: Vec<usize> = pontos
        .iter()
        .filter(|p| p.s == s && p.o == o)
        .map(|p| p.linha)
        .collect();
    let i = atual.and_then(|a| aqui.iter().position(|&l| l == a));
    match i {
        Some(i) => aqui.get((i + 1) % aqui.len()).copied(),
        None => aqui.first().copied(),
    }
}

/// O tamanho do desenho.
pub fn tamanho() -> egui::Vec2 {
    vec2(
        MARGEM_ESQ + 10.0 * CELULA + 8.0,
        10.0 * CELULA + MARGEM_BAIXO + 6.0,
    )
}

/// Desenha a matriz. `escolhida` é a linha em destaque; devolve a linha clicada.
pub fn mostrar(
    ui: &mut Ui,
    id: impl Hash + std::fmt::Debug,
    pontos: &[Ponto],
    escolhida: Option<usize>,
) -> Option<usize> {
    let id = Id::new(id);
    let (rect, _) = ui.allocate_exact_size(tamanho(), Sense::hover());
    let grade = Rect::from_min_size(
        rect.min + vec2(MARGEM_ESQ, 4.0),
        vec2(10.0 * CELULA, 10.0 * CELULA),
    );
    let p = ui.painter_at(rect);
    let fraco = ui.visuals().weak_text_color();
    let fg = ui.visuals().text_color();
    let pequena = FontId::proportional(10.0);
    for s in 1..=10u8 {
        for o in 1..=10u8 {
            let c = celula(grade, s, o);
            p.rect_filled(
                c.shrink(0.5),
                1.0,
                cores::do_nivel(faixa(s, o)).gamma_multiply(0.18),
            );
        }
    }
    p.rect_stroke(grade, 2.0, Stroke::new(1.0, fraco), StrokeKind::Outside);
    for k in 1..=10u8 {
        let c = celula(grade, k, 1);
        p.text(
            pos2(grade.left() - 4.0, c.center().y),
            Align2::RIGHT_CENTER,
            k.to_string(),
            pequena.clone(),
            fraco,
        );
        let c = celula(grade, 1, k);
        p.text(
            pos2(c.center().x, grade.bottom() + 3.0),
            Align2::CENTER_TOP,
            k.to_string(),
            pequena.clone(),
            fraco,
        );
    }
    p.text(
        pos2(grade.center().x, grade.bottom() + 16.0),
        Align2::CENTER_TOP,
        tx("visual-ocorrencia"),
        pequena.clone(),
        fg,
    );
    p.text(
        pos2(rect.left() + 2.0, grade.top() - 2.0),
        Align2::LEFT_TOP,
        "S",
        FontId::proportional(11.0),
        fg,
    );

    // Os pontos: um círculo por célula, na cor do pior risco, com quantos há.
    let mut clicada = None;
    let mut celulas: Vec<(u8, u8)> = pontos.iter().map(|p| (p.s, p.o)).collect();
    celulas.sort_unstable();
    celulas.dedup();
    for (s, o) in celulas {
        let aqui: Vec<&Ponto> = pontos.iter().filter(|p| (p.s, p.o) == (s, o)).collect();
        let pior = aqui.iter().map(|p| p.nivel).max().unwrap_or_default();
        let c = celula(grade, s, o);
        let r = ui.interact(c, id.with(("celula", s, o)), Sense::click());
        let tem_escolhida = escolhida.is_some_and(|e| aqui.iter().any(|p| p.linha == e));
        p.circle_filled(c.center(), CELULA * 0.36, cores::do_nivel(pior));
        if tem_escolhida || r.hovered() {
            p.circle_stroke(c.center(), CELULA * 0.46, Stroke::new(2.0, fg));
        }
        if aqui.len() > 1 {
            p.text(
                c.center(),
                Align2::CENTER_CENTER,
                aqui.len().to_string(),
                FontId::proportional(10.0),
                Color32::WHITE,
            );
        }
        let dica = aqui
            .iter()
            .map(|p| format!("• {}", p.rotulo))
            .collect::<Vec<_>>()
            .join("\n");
        let r = r.on_hover_text(format!(
            "{}\n{dica}\n\n{}",
            fx(
                "visual-s_o",
                &[("s", i64::from(s).into()), ("o", i64::from(o).into())]
            ),
            tx("visual-clique_leva_a_linha")
        ));
        if r.clicked() {
            clicada = proxima(pontos, s, o, escolhida);
        }
    }
    clicada
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ponto(linha: usize, s: u8, o: u8) -> Ponto {
        Ponto {
            linha,
            s,
            o,
            nivel: Nivel::Medio,
            rotulo: format!("linha {linha}"),
        }
    }

    #[test]
    fn a_celula_sob_o_ponto_e_a_mesma_do_desenho() {
        let g = Rect::from_min_size(pos2(10.0, 10.0), vec2(10.0 * CELULA, 10.0 * CELULA));
        for (s, o) in [(1, 1), (10, 10), (7, 3), (2, 9)] {
            assert_eq!(celula_em(g, celula(g, s, o).center()), Some((s, o)));
        }
        assert!(celula(g, 10, 1).top() < celula(g, 1, 1).top(), "S sobe");
        assert_eq!(celula_em(g, pos2(0.0, 0.0)), None);
    }

    #[test]
    fn clicar_de_novo_na_celula_passa_para_a_proxima_linha() {
        let pts = [ponto(4, 8, 5), ponto(1, 3, 2), ponto(9, 8, 5)];
        assert_eq!(proxima(&pts, 8, 5, None), Some(4));
        assert_eq!(proxima(&pts, 8, 5, Some(4)), Some(9));
        assert_eq!(proxima(&pts, 8, 5, Some(9)), Some(4));
        assert_eq!(proxima(&pts, 8, 5, Some(1)), Some(4));
        assert_eq!(proxima(&pts, 5, 5, None), None);
    }

    #[test]
    fn faixas() {
        assert_eq!(faixa(10, 10), Nivel::Alto);
        assert_eq!(faixa(5, 4), Nivel::Medio);
        assert_eq!(faixa(2, 3), Nivel::Baixo);
        assert_eq!(faixa(9, 4), Nivel::Alto);
    }
}
