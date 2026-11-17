//! O desenho de uma carta de controle ([`utsim_quality::Chart`]) com o egui_plot, comum
//! às telas de CEP do UTSim Qualidade e à carta de incidentes do UTSim Segurança do
//! Trabalho: um gráfico por painel com os valores, a linha central, os limites e, se
//! pedido, os pontos com sinal.

use egui_plot::{Line, Plot, PlotPoints, Points};
use utsim_quality::Chart;

/// Como a carta é desenhada em cada tela. Os nomes das séries vêm do app (textos dele).
#[derive(Clone, Copy, Debug)]
pub struct Estilo<'a> {
    /// A altura de cada painel.
    pub altura: f32,
    /// O nome da série dos valores.
    pub valores: &'a str,
    /// Os nomes do limite superior e do inferior.
    pub lsc: &'a str,
    pub lic: &'a str,
    /// Escreve o nome do painel acima do gráfico.
    pub nome_do_painel: bool,
    /// Marca os pontos com sinal (as violações das regras).
    pub sinais: bool,
}

/// A série `v` nas posições 1, 2, 3…
fn serie(v: &[f64]) -> PlotPoints<'static> {
    v.iter()
        .enumerate()
        .map(|(i, y)| [(i + 1) as f64, *y])
        .collect()
}

/// Um gráfico por painel da carta (ids `"{id}_{k}"`): valores, linha central, limites e,
/// com [`Estilo::sinais`], os pontos com sinal.
pub fn desenhar(ui: &mut egui::Ui, id: &str, ch: &Chart, e: &Estilo<'_>) {
    for (k, p) in ch.panels.iter().enumerate() {
        if e.nome_do_painel {
            ui.label(&p.name);
        }
        let valores: Vec<[f64; 2]> = p
            .values
            .iter()
            .enumerate()
            .filter_map(|(i, v)| v.map(|v| [(i + 1) as f64, v]))
            .collect();
        let sinais: Vec<[f64; 2]> = if e.sinais {
            p.violations
                .iter()
                .filter_map(|v| {
                    p.values
                        .get(v.index)
                        .copied()
                        .flatten()
                        .map(|y| [(v.index + 1) as f64, y])
                })
                .collect()
        } else {
            Vec::new()
        };
        Plot::new(format!("{id}_{k}"))
            .height(e.altura)
            .allow_scroll(false)
            .show(ui, |pl| {
                pl.line(Line::new(e.valores, PlotPoints::new(valores)));
                pl.line(Line::new("central", serie(&p.center)));
                pl.line(Line::new(e.lsc, serie(&p.ucl)));
                pl.line(Line::new(e.lic, serie(&p.lcl)));
                if !sinais.is_empty() {
                    pl.points(Points::new("sinais", PlotPoints::new(sinais)).radius(5.0));
                }
            });
    }
}
