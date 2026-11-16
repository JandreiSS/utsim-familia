//! Pequenas ajudas: data de hoje, cor `#rrggbb` e leitura de modelo.

use crate::texto::tx;
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

/// Data e hora de agora (UTC) em ISO 8601: `AAAA-MM-DDTHH:MM:SSZ`.
pub fn agora_iso() -> String {
    let s = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs() as i64);
    let (dias, resto) = (s.div_euclid(86_400), s.rem_euclid(86_400));
    format!(
        "{}T{:02}:{:02}:{:02}Z",
        data_de_dias(dias),
        resto / 3600,
        resto % 3600 / 60,
        resto % 60
    )
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

/// Um caminho escolhido no seletor, como a unidade guarda: relativo à pasta da unidade
/// quando o arquivo está dentro dela (assim a pasta continua valendo no Drive de cada
/// pessoa), senão absoluto.
pub fn caminho_na_unidade(u: &Unidade, escolhido: &Path) -> String {
    relativo(u.raiz(), escolhido).unwrap_or_else(|| escolhido.display().to_string())
}

/// `caminho` relativo a `raiz`, com `/`, se estiver dentro dela. No Windows a comparação
/// ignora maiúsculas e o prefixo de caminho longo.
fn relativo(raiz: &Path, caminho: &Path) -> Option<String> {
    let partes = |p: &Path| -> Vec<String> {
        let t = p.display().to_string().replace('\\', "/");
        let t = t.strip_prefix("//?/").unwrap_or(&t).to_string();
        t.split('/')
            .filter(|x| !x.is_empty() && *x != ".")
            .map(str::to_string)
            .collect()
    };
    let (r, c) = (partes(raiz), partes(caminho));
    let igual = |a: &String, b: &String| {
        if cfg!(windows) {
            a.to_lowercase() == b.to_lowercase()
        } else {
            a == b
        }
    };
    if c.len() <= r.len() || !r.iter().zip(&c).all(|(a, b)| igual(a, b)) {
        return None;
    }
    Some(c[r.len()..].join("/"))
}

/// O campo de um modelo `.ron` da unidade: a caixa de texto e o botão "Procurar…".
pub fn campo_modelo(
    ui: &mut egui::Ui,
    texto: &mut String,
    u: Option<&Unidade>,
    largura: f32,
) -> egui::Response {
    use utsim_ui_tema::arquivos::{self, Tipo};
    ui.horizontal(|ui| {
        let mut r = ui.add(
            egui::TextEdit::singleline(texto)
                .hint_text(tx("util-arquivo_ron_escreva_ou_clique"))
                .desired_width(largura),
        );
        if ui
            .button(arquivos::PROCURAR)
            .on_hover_text(tx("util-abre_o_explorador_de_arquivos"))
            .clicked()
        {
            let atual = match u {
                Some(u) if !texto.trim().is_empty() => {
                    u.caminho_modelo(texto.trim()).display().to_string()
                }
                Some(u) => u.raiz().display().to_string(),
                None => texto.clone(),
            };
            if let Some(p) = arquivos::procurar(
                Tipo::Abrir,
                tx("util-escolha_o_modelo_do_utsim"),
                &[arquivos::RON],
                &atual,
            ) {
                *texto = match u {
                    Some(u) => caminho_na_unidade(u, &p),
                    None => p.display().to_string(),
                };
                r.mark_changed();
            }
        }
        r
    })
    .inner
}

pub fn carregar_arquivo(c: &Path) -> Result<Model, String> {
    Model::load(c).map_err(|e| format!("{}: {e}", c.display()))
}

/// Abre um arquivo com o programa do sistema (o HTML de um relatório).
pub fn abrir_com_sistema(caminho: &Path) -> std::io::Result<()> {
    #[cfg(windows)]
    let mut c = {
        let mut c = std::process::Command::new("cmd");
        c.args(["/C", "start", ""]).arg(caminho);
        c
    };
    #[cfg(target_os = "macos")]
    let mut c = {
        let mut c = std::process::Command::new("open");
        c.arg(caminho);
        c
    };
    #[cfg(all(unix, not(target_os = "macos")))]
    let mut c = {
        let mut c = std::process::Command::new("xdg-open");
        c.arg(caminho);
        c
    };
    c.spawn().map(|_| ())
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
    fn caminho_relativo() {
        let r = Path::new("C:/Drive/Fábrica");
        assert_eq!(
            relativo(r, Path::new("C:/Drive/Fábrica/modelos/a.ron")).as_deref(),
            Some("modelos/a.ron")
        );
        assert_eq!(relativo(r, Path::new("C:/Outro/a.ron")), None);
        assert_eq!(relativo(r, r), None);
        if cfg!(windows) {
            assert_eq!(
                relativo(r, Path::new(r"\\?\c:\drive\fábrica\a.ron")).as_deref(),
                Some("a.ron")
            );
        }
    }

    #[test]
    fn cores() {
        assert_eq!(cor("#ff0000"), Color32::from_rgb(255, 0, 0));
        assert_eq!(cor("zz"), Color32::GRAY);
    }
}
