//! O Pareto vivo: as barras das categorias (as poucas vitais na cor de destaque), a curva
//! acumulada, a linha de 80 % e o corte das poucas vitais marcado. Clicar numa barra a
//! escolhe (de novo desfaz); quem chama filtra a tabela pela categoria escolhida.

use std::hash::Hash;

use egui::epaint::TextShape;
use egui::{Align2, FontId, Id, Pos2, Rect, Sense, Shape, Stroke, StrokeKind, Ui, pos2, vec2};

use crate::cores;
use crate::texto::{fx, tx};
use crate::util::cortado;

/// Uma barra, já na ordem do Pareto (da maior para a menor).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Barra {
    pub nome: String,
    pub valor: f64,
    /// Percentual do total e percentual acumulado (0 a 100).
    pub pct: f64,
    pub acum: f64,
    pub vital: bool,
}

/// A linha das poucas vitais.
pub const VITAL: f64 = 80.0;

/// O gráfico na tela.
#[derive(Clone, Debug, PartialEq)]
pub struct Grafico {
    /// A área das barras (sem os eixos e os nomes).
    pub plot: Rect,
    pub barras: Vec<Rect>,
    /// As colunas inteiras (barra, vão e nome): clicar nelas escolhe a barra.
    pub colunas: Vec<Rect>,
    pub maximo: f64,
}

impl Grafico {
    /// A altura (y) de um percentual no eixo da direita.
    pub fn y_pct(&self, pct: f64) -> f32 {
        self.plot.bottom() - self.plot.height() * (pct.clamp(0.0, 100.0) / 100.0) as f32
    }

    /// A barra sob o ponto (a coluna inteira conta, nome embaixo incluído).
    pub fn barra_em(&self, p: Pos2) -> Option<usize> {
        self.colunas.iter().position(|c| c.contains(p))
    }
}

/// A posição da última das poucas vitais.
pub fn corte(barras: &[Barra]) -> Option<usize> {
    barras.iter().rposition(|b| b.vital)
}

/// Escolhe a barra, ou desfaz quando já era ela.
pub fn alternar(escolhida: &mut Option<String>, nome: &str) {
    if escolhida.as_deref() == Some(nome) {
        *escolhida = None;
    } else {
        *escolhida = Some(nome.to_string());
    }
}

/// Onde fica cada barra no retângulo.
pub fn dispor(rect: Rect, barras: &[Barra]) -> Grafico {
    let plot = Rect::from_min_max(rect.min + vec2(48.0, 18.0), rect.max - vec2(44.0, 54.0));
    let n = barras.len().max(1);
    let w = plot.width() / n as f32;
    let maximo = barras
        .iter()
        .map(|b| b.valor)
        .fold(0.0f64, f64::max)
        .max(f64::MIN_POSITIVE);
    let mut rs = Vec::with_capacity(barras.len());
    let mut cs = Vec::with_capacity(barras.len());
    for (i, b) in barras.iter().enumerate() {
        let x = plot.left() + i as f32 * w;
        let h = plot.height() * (b.valor.max(0.0) / maximo) as f32;
        rs.push(Rect::from_min_max(
            pos2(x + w * 0.14, plot.bottom() - h),
            pos2(x + w * 0.86, plot.bottom()),
        ));
        cs.push(Rect::from_min_max(
            pos2(x, plot.top()),
            pos2(x + w, rect.bottom()),
        ));
    }
    Grafico {
        plot,
        barras: rs,
        colunas: cs,
        maximo,
    }
}

/// A barra escolhida guardada na memória da tela.
pub fn escolhida(ctx: &egui::Context, id: impl Hash + std::fmt::Debug) -> Option<String> {
    ler(ctx, Id::new(id))
}

fn ler(ctx: &egui::Context, id: Id) -> Option<String> {
    ctx.data(|m| m.get_temp::<Option<String>>(id)).flatten()
}

fn gravar(ctx: &egui::Context, id: Id, nome: Option<String>) {
    ctx.data_mut(|m| m.insert_temp(id, nome));
}

/// Muda a barra escolhida (`None` mostra todas).
pub fn escolher(ctx: &egui::Context, id: impl Hash + std::fmt::Debug, nome: Option<String>) {
    gravar(ctx, Id::new(id), nome);
}

fn numero(v: f64) -> String {
    if (v - v.round()).abs() < 1e-9 {
        format!("{}", v.round() as i64)
    } else {
        format!("{v:.1}").replace('.', ",")
    }
}

/// Desenha o Pareto e devolve a barra escolhida depois deste quadro (a da memória, mudada
/// pelo clique).
pub fn mostrar(
    ui: &mut Ui,
    id: impl Hash + std::fmt::Debug,
    barras: &[Barra],
    altura: f32,
) -> Option<String> {
    let id = Id::new(id);
    let mut sel = ler(ui.ctx(), id);
    if sel
        .as_ref()
        .is_some_and(|s| !barras.iter().any(|b| &b.nome == s))
    {
        sel = None;
    }
    let largura = ui.available_width().max(280.0);
    let (rect, _) = ui.allocate_exact_size(vec2(largura, altura.max(180.0)), Sense::hover());
    let fg = ui.visuals().text_color();
    let fraco = ui.visuals().weak_text_color();
    let acento = cores::destaque(ui);
    let p = ui.painter_at(rect);
    if barras.is_empty() || barras.iter().all(|b| b.valor <= 0.0) {
        p.text(
            rect.center(),
            Align2::CENTER_CENTER,
            tx("visual-sem_dados_pareto"),
            FontId::proportional(13.0),
            fraco,
        );
        gravar(ui.ctx(), id, None);
        return None;
    }
    let g = dispor(rect, barras);
    let pequena = FontId::proportional(10.5);

    // Grade e eixos.
    for q in [0.0, 25.0, 50.0, 75.0, 100.0] {
        let y = g.y_pct(q);
        p.line_segment(
            [pos2(g.plot.left(), y), pos2(g.plot.right(), y)],
            Stroke::new(0.5, fraco.gamma_multiply(0.4)),
        );
        p.text(
            pos2(g.plot.right() + 4.0, y),
            Align2::LEFT_CENTER,
            format!("{q:.0} %"),
            pequena.clone(),
            fraco,
        );
        p.text(
            pos2(g.plot.left() - 4.0, y),
            Align2::RIGHT_CENTER,
            numero(g.maximo * q / 100.0),
            pequena.clone(),
            fraco,
        );
    }

    // As barras.
    let w = g.colunas.first().map_or(40.0, |c| c.width());
    let mut clicada = None;
    for (i, (b, r)) in barras.iter().zip(&g.barras).enumerate() {
        let resp = ui.interact(g.colunas[i], id.with(("barra", i)), Sense::click());
        let escolhida = sel.as_deref() == Some(b.nome.as_str());
        let base = if b.vital { acento } else { cores::CINZA };
        let cor = if sel.is_some() && !escolhida {
            base.gamma_multiply(0.3)
        } else {
            base
        };
        if resp.hovered() {
            p.rect_filled(g.colunas[i], 2.0, fraco.gamma_multiply(0.08));
        }
        p.rect_filled(*r, 2.0, cor);
        if escolhida {
            p.rect_stroke(
                r.expand(2.0),
                3.0,
                Stroke::new(2.0, fg),
                StrokeKind::Outside,
            );
        }
        // O nome embaixo: deitado quando a coluna é estreita.
        let nome_rect = Rect::from_min_max(
            pos2(g.colunas[i].left(), g.plot.bottom() + 4.0),
            g.colunas[i].max,
        );
        if w >= 44.0 {
            let gal = cortado(&p, &b.nome, pequena.clone(), fg, w - 4.0, if w >= 70.0 { 2 } else { 1 });
            p.galley(
                pos2(nome_rect.center().x - gal.size().x / 2.0, nome_rect.top()),
                gal,
                fg,
            );
        } else {
            let gal = cortado(&p, &b.nome, pequena.clone(), fg, 66.0, 1);
            let ancora = pos2(nome_rect.center().x + 4.0, nome_rect.top() + 2.0);
            p.add(TextShape::new(ancora, gal, fg).with_angle_and_anchor(-0.6, Align2::RIGHT_TOP));
        }
        let resp = resp.on_hover_text(fx(
            "visual-dica_da_barra",
            &[
                ("nome", b.nome.clone().into()),
                ("valor", numero(b.valor).into()),
                ("pct", numero(b.pct).into()),
                ("acum", numero(b.acum).into()),
            ],
        ));
        if resp.clicked() {
            clicada = Some(i);
        }
    }

    // A curva acumulada.
    let pontos: Vec<Pos2> = barras
        .iter()
        .zip(&g.colunas)
        .map(|(b, c)| pos2(c.center().x, g.y_pct(b.acum)))
        .collect();
    p.add(Shape::line(pontos.clone(), Stroke::new(2.0, cores::AMBAR)));
    for q in &pontos {
        p.circle_filled(*q, 3.0, cores::AMBAR);
    }

    // A linha de 80 % e o corte das poucas vitais.
    let y80 = g.y_pct(VITAL);
    p.extend(Shape::dashed_line(
        &[pos2(g.plot.left(), y80), pos2(g.plot.right(), y80)],
        Stroke::new(1.3, cores::VERMELHO),
        6.0,
        4.0,
    ));
    if let Some(k) = corte(barras) {
        let x = g.colunas[k].right();
        p.extend(Shape::dashed_line(
            &[pos2(x, g.plot.top() - 6.0), pos2(x, g.plot.bottom())],
            Stroke::new(1.3, cores::VERMELHO),
            6.0,
            4.0,
        ));
        p.text(
            pos2(x - 4.0, g.plot.top() - 6.0),
            Align2::RIGHT_BOTTOM,
            fx("visual-poucas_vitais", &[("n", ((k + 1) as i64).into())]),
            pequena.clone(),
            cores::VERMELHO,
        );
    }

    if let Some(i) = clicada {
        alternar(&mut sel, &barras[i].nome);
    }
    gravar(ui.ctx(), id, sel.clone());
    sel
}

/// As barras de `(nome, valor)` já somadas, com percentuais e as poucas vitais até 80 %
/// (a que cruza os 80 % incluída). Para quem não tem o cálculo pronto.
pub fn barras_de(valores: &[(String, f64)]) -> Vec<Barra> {
    let mut v: Vec<(String, f64)> = valores
        .iter()
        .map(|(n, x)| (n.clone(), if x.is_finite() { x.max(0.0) } else { 0.0 }))
        .collect();
    v.sort_by(|a, b| b.1.total_cmp(&a.1).then(a.0.cmp(&b.0)));
    let total: f64 = v.iter().map(|x| x.1).sum();
    let mut acum = 0.0;
    let mut antes = 0.0;
    v.into_iter()
        .map(|(nome, valor)| {
            let pct = if total > 0.0 {
                valor / total * 100.0
            } else {
                0.0
            };
            acum += pct;
            let vital = total > 0.0 && antes < VITAL;
            antes = acum;
            Barra {
                nome,
                valor,
                pct,
                acum,
                vital,
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn barras() -> Vec<Barra> {
        barras_de(&[
            ("Risco".into(), 50.0),
            ("Mancha".into(), 30.0),
            ("Bolha".into(), 15.0),
            ("Outros".into(), 5.0),
        ])
    }

    #[test]
    fn as_poucas_vitais_vao_ate_cruzar_os_80() {
        let b = barras();
        let v: Vec<bool> = b.iter().map(|x| x.vital).collect();
        assert_eq!(v, [true, true, false, false]);
        assert_eq!(corte(&b), Some(1));
        assert!((b[3].acum - 100.0).abs() < 1e-9);
    }

    #[test]
    fn clicar_numa_barra_escolhe_e_clicar_de_novo_desfaz() {
        let b = barras();
        let r = Rect::from_min_size(Pos2::ZERO, vec2(600.0, 260.0));
        let g = dispor(r, &b);
        let ponto = g.barras[1].center();
        let i = g.barra_em(ponto).expect("a barra");
        assert_eq!(i, 1);
        // O nome embaixo da barra também escolhe.
        assert_eq!(g.barra_em(pos2(ponto.x, r.bottom() - 4.0)), Some(1));
        let mut sel = None;
        alternar(&mut sel, &b[i].nome);
        assert_eq!(sel.as_deref(), Some("Mancha"));
        alternar(&mut sel, "Mancha");
        assert_eq!(sel, None);
        assert!(g.barras[0].height() > g.barras[1].height());
        assert!(g.y_pct(80.0) < g.y_pct(20.0));
    }
}
