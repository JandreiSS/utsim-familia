//! As cores de risco e de apoio dos desenhos. As de risco são as mesmas nos temas claro e
//! escuro (o fundo das células é a mesma cor, mais transparente).

use egui::Color32;

/// O nível de risco de uma linha, de uma célula ou de um ponto.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub enum Nivel {
    #[default]
    Baixo,
    Medio,
    Alto,
}

pub const VERDE: Color32 = Color32::from_rgb(76, 175, 96);
pub const AMBAR: Color32 = Color32::from_rgb(224, 150, 40);
pub const VERMELHO: Color32 = Color32::from_rgb(214, 76, 68);
pub const AZUL: Color32 = Color32::from_rgb(70, 140, 230);
pub const CINZA: Color32 = Color32::from_rgb(140, 144, 152);

/// A cor de um nível de risco.
pub fn do_nivel(n: Nivel) -> Color32 {
    match n {
        Nivel::Baixo => VERDE,
        Nivel::Medio => AMBAR,
        Nivel::Alto => VERMELHO,
    }
}

/// A cor de destaque do tema (a da seleção).
pub fn destaque(ui: &egui::Ui) -> Color32 {
    ui.visuals().selection.stroke.color
}

/// O fundo de um cartão ou de uma caixa, um pouco acima do painel.
pub fn cartao(ui: &egui::Ui) -> Color32 {
    ui.visuals().widgets.inactive.weak_bg_fill
}
