//! Campos de formulário comuns aos apps da família.
//!
//! [`CampoData`]: uma data guardada como texto `AAAA-MM-DD`. Aceita também `DD/MM/AAAA`
//! (e `D/M/AAAA`), que vira `AAAA-MM-DD` quando o campo perde o foco. Data inválida, ou
//! antes do limite dado, ganha borda vermelha e a explicação no hover. Vazio é válido (a
//! data ainda não foi marcada).
//!
//! ```
//! # let ctx = egui::Context::default();
//! # let mut inicio = String::from("2026-11-13");
//! # let mut fim = String::from("2026-11-12");
//! # let mut saida = ctx.run_ui(Default::default(), |ui| {
//! use utsim_ui_tema::campos::{CampoData, Data};
//! CampoData::new(&mut inicio).mostrar(ui);
//! CampoData::new(&mut fim)
//!     .nao_antes(Data::ler(&inicio))
//!     .mostrar(ui);
//! # });
//! # saida.textures_delta.clear();
//! ```

use std::fmt;

use egui::{Response, Stroke, Ui};

/// Uma data do calendário (sem hora).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Data {
    pub ano: i32,
    pub mes: u8,
    pub dia: u8,
}

impl Data {
    /// A data, se existir no calendário (ano de 1 a 9999).
    pub fn nova(ano: i32, mes: u8, dia: u8) -> Option<Data> {
        let ok = (1..=9999).contains(&ano)
            && (1..=12).contains(&mes)
            && dia >= 1
            && dia <= dias_no_mes(ano, mes);
        ok.then_some(Data { ano, mes, dia })
    }

    /// Lê `AAAA-MM-DD` ou `DD/MM/AAAA` (com ou sem zeros à esquerda), ignorando espaços
    /// nas pontas. `None` para vazio ou inválido.
    pub fn ler(texto: &str) -> Option<Data> {
        let t = texto.trim();
        let numeros = |partes: Vec<&str>| -> Option<Vec<u32>> {
            partes
                .iter()
                .map(|p| {
                    if p.is_empty() || p.len() > 4 || !p.bytes().all(|b| b.is_ascii_digit()) {
                        None
                    } else {
                        p.parse().ok()
                    }
                })
                .collect()
        };
        if t.contains('-') {
            let p: Vec<&str> = t.split('-').collect();
            if p.len() != 3 || p[0].len() != 4 {
                return None;
            }
            let n = numeros(p)?;
            Data::nova(
                n[0] as i32,
                u8::try_from(n[1]).ok()?,
                u8::try_from(n[2]).ok()?,
            )
        } else if t.contains('/') {
            let p: Vec<&str> = t.split('/').collect();
            if p.len() != 3 || p[2].len() != 4 {
                return None;
            }
            let n = numeros(p)?;
            Data::nova(
                n[2] as i32,
                u8::try_from(n[1]).ok()?,
                u8::try_from(n[0]).ok()?,
            )
        } else {
            None
        }
    }
}

/// Sempre `AAAA-MM-DD`.
impl fmt::Display for Data {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:04}-{:02}-{:02}", self.ano, self.mes, self.dia)
    }
}

fn bissexto(ano: i32) -> bool {
    (ano % 4 == 0 && ano % 100 != 0) || ano % 400 == 0
}

fn dias_no_mes(ano: i32, mes: u8) -> u8 {
    match mes {
        2 if bissexto(ano) => 29,
        2 => 28,
        4 | 6 | 9 | 11 => 30,
        _ => 31,
    }
}

/// O que o campo achou do texto.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Situacao {
    /// Nada escrito.
    Vazia,
    Valida(Data),
    /// Não é uma data.
    Invalida,
    /// É uma data, mas antes do limite.
    AntesDoLimite(Data),
}

/// Textos do campo; o padrão é em português, e cada app passa os do seu idioma.
#[derive(Debug, Clone, Copy)]
pub struct TextosData<'a> {
    /// A dica no campo vazio.
    pub dica: &'a str,
    /// O hover de uma data inválida.
    pub invalida: &'a str,
    /// O hover de uma data antes do limite; `{limite}` vira a data limite.
    pub antes: &'a str,
}

impl Default for TextosData<'_> {
    fn default() -> Self {
        TextosData {
            dica: "AAAA-MM-DD",
            invalida: "Data inválida. Use AAAA-MM-DD (ou DD/MM/AAAA).",
            antes: "Esta data vem antes de {limite}.",
        }
    }
}

/// Um campo de data; veja o módulo.
pub struct CampoData<'a> {
    texto: &'a mut String,
    limite: Option<Data>,
    textos: TextosData<'a>,
    largura: f32,
}

impl<'a> CampoData<'a> {
    pub fn new(texto: &'a mut String) -> Self {
        CampoData {
            texto,
            limite: None,
            textos: TextosData::default(),
            largura: 104.0,
        }
    }

    /// A data não pode vir antes desta (ex.: o fim não antes do início). `None` não limita.
    pub fn nao_antes(mut self, limite: Option<Data>) -> Self {
        self.limite = limite;
        self
    }

    pub fn textos(mut self, textos: TextosData<'a>) -> Self {
        self.textos = textos;
        self
    }

    pub fn largura(mut self, largura: f32) -> Self {
        self.largura = largura;
        self
    }

    /// O que o texto atual é, com o limite deste campo.
    pub fn situacao(texto: &str, limite: Option<Data>) -> Situacao {
        if texto.trim().is_empty() {
            return Situacao::Vazia;
        }
        match Data::ler(texto) {
            None => Situacao::Invalida,
            Some(d) if limite.is_some_and(|l| d < l) => Situacao::AntesDoLimite(d),
            Some(d) => Situacao::Valida(d),
        }
    }

    /// Desenha o campo. `changed()` vale também quando o texto foi normalizado para
    /// `AAAA-MM-DD` ao perder o foco.
    pub fn mostrar(self, ui: &mut Ui) -> Response {
        let CampoData {
            texto,
            limite,
            textos,
            largura,
        } = self;
        let situacao = Self::situacao(texto, limite);
        let ruim = matches!(situacao, Situacao::Invalida | Situacao::AntesDoLimite(_));
        let erro = ui.visuals().error_fg_color;
        let mut resp = ui
            .scope(|ui| {
                if ruim {
                    let w = &mut ui.visuals_mut().widgets;
                    for s in [&mut w.inactive, &mut w.hovered, &mut w.active] {
                        s.bg_stroke = Stroke::new(1.5, erro);
                    }
                    ui.visuals_mut().selection.stroke = Stroke::new(1.5, erro);
                }
                ui.add(
                    egui::TextEdit::singleline(texto)
                        .hint_text(textos.dica)
                        .desired_width(largura),
                )
            })
            .inner;
        if resp.lost_focus()
            && let Some(d) = Data::ler(texto)
        {
            let normal = d.to_string();
            if *texto != normal {
                *texto = normal;
                resp.mark_changed();
            }
        }
        match situacao {
            Situacao::Invalida => resp.on_hover_text(textos.invalida),
            Situacao::AntesDoLimite(_) => {
                let l = limite.map(|l| l.to_string()).unwrap_or_default();
                resp.on_hover_text(textos.antes.replace("{limite}", &l))
            }
            _ => resp,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn le_os_dois_formatos() {
        let d = Data::nova(2026, 11, 13).unwrap();
        assert_eq!(Data::ler("2026-11-13"), Some(d));
        assert_eq!(Data::ler(" 13/11/2026 "), Some(d));
        assert_eq!(Data::ler("3/2/2026"), Data::nova(2026, 2, 3));
        assert_eq!(d.to_string(), "2026-11-13");
    }

    #[test]
    fn recusa_o_que_nao_existe() {
        for t in [
            "",
            "2026-13-01",
            "2026-02-29",
            "2026-04-31",
            "26-11-13",
            "2026/11/13",
            "13-11-2026",
            "2026-1x-01",
            "2026-11-13-1",
            "+2026-11-13",
            "AAAA-MM-DD",
        ] {
            assert_eq!(Data::ler(t), None, "{t}");
        }
        assert!(Data::ler("2024-02-29").is_some());
        assert!(Data::ler("2000-02-29").is_some());
        assert!(Data::ler("1900-02-29").is_none());
    }

    #[test]
    fn situacao_com_limite() {
        let inicio = Data::ler("2026-11-13");
        assert_eq!(CampoData::situacao("  ", inicio), Situacao::Vazia);
        assert_eq!(CampoData::situacao("ontem", inicio), Situacao::Invalida);
        assert!(matches!(
            CampoData::situacao("2026-11-12", inicio),
            Situacao::AntesDoLimite(_)
        ));
        assert!(matches!(
            CampoData::situacao("13/11/2026", inicio),
            Situacao::Valida(_)
        ));
        assert!(matches!(
            CampoData::situacao("2026-11-12", None),
            Situacao::Valida(_)
        ));
    }

    #[test]
    fn normaliza_ao_perder_o_foco() {
        let ctx = egui::Context::default();
        let mut texto = String::from("5/1/2027");
        let id = egui::Id::new("campo");
        // Quadro 1: foca o campo; quadro 2: tira o foco; o texto vira AAAA-MM-DD.
        for focar in [true, false] {
            let mut out = ctx.run_ui(Default::default(), |ui| {
                ui.push_id(id, |ui| {
                    let r = CampoData::new(&mut texto).mostrar(ui);
                    if focar {
                        r.request_focus();
                    } else {
                        r.surrender_focus();
                    }
                });
            });
            out.textures_delta.clear();
        }
        let mut out = ctx.run_ui(Default::default(), |ui| {
            ui.push_id(id, |ui| CampoData::new(&mut texto).mostrar(ui));
        });
        out.textures_delta.clear();
        assert_eq!(texto, "2027-01-05");
    }
}
