//! Pequenas ajudas: data de hoje, cor `#rrggbb` e leitura de modelo.

use std::path::Path;

use egui::Color32;
use utsim_model::Model;
use utsim_posto::Unidade;

/// Data de hoje (UTC) como `AAAA-MM-DD`.
pub fn hoje() -> String {
    let dias = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| (d.as_secs() / 86_400) as i64);
    data_de_dias(dias)
}

/// Dias desde 1970-01-01 para `AAAA-MM-DD` (calendário civil).
pub fn data_de_dias(dias: i64) -> String {
    let z = dias + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    format!("{y:04}-{m:02}-{d:02}")
}

/// Cor `#rrggbb`; cinza se não ler.
pub fn cor(hex: &str) -> Color32 {
    let h = hex.trim().trim_start_matches('#');
    if h.len() == 6
        && let Ok(v) = u32::from_str_radix(h, 16)
    {
        return Color32::from_rgb((v >> 16) as u8, (v >> 8) as u8, v as u8);
    }
    Color32::GRAY
}

/// Texto preto ou branco, conforme o fundo.
pub fn texto_sobre(fundo: Color32) -> Color32 {
    let l =
        0.299 * f32::from(fundo.r()) + 0.587 * f32::from(fundo.g()) + 0.114 * f32::from(fundo.b());
    if l > 140.0 {
        Color32::BLACK
    } else {
        Color32::WHITE
    }
}

/// Lê um modelo `.ron` pelo caminho dado (relativo à pasta da unidade ou absoluto).
pub fn carregar_modelo(u: &Unidade, caminho: &str) -> Result<Model, String> {
    carregar_arquivo(&u.caminho_modelo(caminho.trim()))
}

pub fn carregar_arquivo(c: &Path) -> Result<Model, String> {
    Model::load(c).map_err(|e| format!("{}: {e}", c.display()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn datas() {
        assert_eq!(data_de_dias(0), "1970-01-01");
        assert_eq!(data_de_dias(19_723), "2024-01-01");
    }

    #[test]
    fn cores() {
        assert_eq!(cor("#ff0000"), Color32::from_rgb(255, 0, 0));
        assert_eq!(cor("zz"), Color32::GRAY);
    }
}
