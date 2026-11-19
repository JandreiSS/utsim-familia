//! Campos de formulário comuns aos apps da família.
//!
//! [`CampoData`]: uma data guardada como texto `AAAA-MM-DD`. Aceita também `DD/MM/AAAA`
//! (e `D/M/AAAA`), que vira `AAAA-MM-DD` quando o campo perde o foco. Data inválida, ou
//! antes do limite dado, ganha borda vermelha e a explicação no hover. Vazio é válido (a
//! data ainda não foi marcada).
//!
//! [`CampoHora`] (`HH:MM`) e [`CampoDataHora`] (`AAAA-MM-DD HH:MM`) seguem a mesma regra.
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

/// Textos do campo; o padrão vem de `tema.ftl` na língua de agora, e um app pode passar os
/// seus.
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
        use crate::texto::tx;
        TextosData {
            dica: tx("campos-data_dica"),
            invalida: tx("campos-data_invalida"),
            antes: tx("campos-data_antes"),
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
        let hover = match situacao {
            Situacao::Invalida => Some(textos.invalida.to_string()),
            Situacao::AntesDoLimite(_) => {
                let l = limite.map(|l| l.to_string()).unwrap_or_default();
                Some(textos.antes.replace("{limite}", &l))
            }
            _ => None,
        };
        desenhar(ui, texto, textos.dica, largura, hover, |t| {
            Data::ler(t).map(|d| d.to_string())
        })
    }
}

/// O desenho comum dos campos: borda vermelha e `hover` quando há problema, e o texto
/// trocado por `normal(texto)` ao perder o foco (com `mark_changed`).
fn desenhar(
    ui: &mut Ui,
    texto: &mut String,
    dica: &str,
    largura: f32,
    hover: Option<String>,
    normal: impl Fn(&str) -> Option<String>,
) -> Response {
    let erro = ui.visuals().error_fg_color;
    let mut resp = ui
        .scope(|ui| {
            if hover.is_some() {
                let w = &mut ui.visuals_mut().widgets;
                for s in [&mut w.inactive, &mut w.hovered, &mut w.active] {
                    s.bg_stroke = Stroke::new(1.5, erro);
                }
                ui.visuals_mut().selection.stroke = Stroke::new(1.5, erro);
            }
            ui.add(
                egui::TextEdit::singleline(texto)
                    .hint_text(dica)
                    .desired_width(largura),
            )
        })
        .inner;
    if resp.lost_focus()
        && let Some(n) = normal(texto)
        && *texto != n
    {
        *texto = n;
        resp.mark_changed();
    }
    match hover {
        Some(h) => resp.on_hover_text(h),
        None => resp,
    }
}

/// Uma hora do dia, de `00:00` a `23:59`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Hora {
    pub h: u8,
    pub m: u8,
}

impl Hora {
    pub fn nova(h: u8, m: u8) -> Option<Hora> {
        (h < 24 && m < 60).then_some(Hora { h, m })
    }

    /// Lê `HH:MM` ou `H:MM` (também `HHhMM`), ignorando espaços nas pontas.
    pub fn ler(texto: &str) -> Option<Hora> {
        let t = texto.trim().to_ascii_lowercase();
        let (h, m) = t.split_once(':').or_else(|| t.split_once('h'))?;
        let num = |p: &str| -> Option<u8> {
            if p.is_empty() || p.len() > 2 || !p.bytes().all(|b| b.is_ascii_digit()) {
                None
            } else {
                p.parse().ok()
            }
        };
        if m.len() != 2 {
            return None;
        }
        Hora::nova(num(h)?, num(m)?)
    }
}

/// Sempre `HH:MM`.
impl fmt::Display for Hora {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:02}:{:02}", self.h, self.m)
    }
}

/// Uma data com hora (sem fuso: quem guarda diz qual é).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct DataHora {
    pub data: Data,
    pub hora: Hora,
}

impl DataHora {
    /// Lê `AAAA-MM-DD HH:MM` ou `DD/MM/AAAA HH:MM` (data e hora separadas por espaço).
    pub fn ler(texto: &str) -> Option<DataHora> {
        let (d, h) = texto.trim().split_once(char::is_whitespace)?;
        Some(DataHora {
            data: Data::ler(d)?,
            hora: Hora::ler(h)?,
        })
    }
}

/// Sempre `AAAA-MM-DD HH:MM`.
impl fmt::Display for DataHora {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} {}", self.data, self.hora)
    }
}

/// Um campo de hora `HH:MM` (aceita `H:MM` e `HHhMM`, que viram `HH:MM` ao perder o foco).
/// Vazio vale.
pub struct CampoHora<'a> {
    texto: &'a mut String,
    largura: f32,
}

impl<'a> CampoHora<'a> {
    pub fn new(texto: &'a mut String) -> Self {
        CampoHora {
            texto,
            largura: 60.0,
        }
    }

    pub fn largura(mut self, largura: f32) -> Self {
        self.largura = largura;
        self
    }

    pub fn mostrar(self, ui: &mut Ui) -> Response {
        use crate::texto::tx;
        let t = self.texto.trim();
        let hover = (!t.is_empty() && Hora::ler(t).is_none())
            .then(|| tx("campos-hora_invalida").to_string());
        desenhar(
            ui,
            self.texto,
            tx("campos-hora_dica"),
            self.largura,
            hover,
            |t| Hora::ler(t).map(|h| h.to_string()),
        )
    }
}

/// Um campo de data com hora, `AAAA-MM-DD HH:MM` (aceita `DD/MM/AAAA HH:MM`), com limite
/// opcional como o [`CampoData`]. Vazio vale.
pub struct CampoDataHora<'a> {
    texto: &'a mut String,
    limite: Option<DataHora>,
    largura: f32,
}

impl<'a> CampoDataHora<'a> {
    pub fn new(texto: &'a mut String) -> Self {
        CampoDataHora {
            texto,
            limite: None,
            largura: 150.0,
        }
    }

    /// Não pode vir antes deste instante (ex.: o fim de uma prova não antes do início).
    pub fn nao_antes(mut self, limite: Option<DataHora>) -> Self {
        self.limite = limite;
        self
    }

    pub fn largura(mut self, largura: f32) -> Self {
        self.largura = largura;
        self
    }

    pub fn mostrar(self, ui: &mut Ui) -> Response {
        use crate::texto::tx;
        let t = self.texto.trim();
        let hover = match (t.is_empty(), DataHora::ler(t), self.limite) {
            (true, _, _) => None,
            (false, None, _) => Some(tx("campos-data_hora_invalida").to_string()),
            (false, Some(d), Some(l)) if d < l => {
                Some(tx("campos-data_antes").replace("{limite}", &l.to_string()))
            }
            _ => None,
        };
        desenhar(
            ui,
            self.texto,
            tx("campos-data_hora_dica"),
            self.largura,
            hover,
            |t| DataHora::ler(t).map(|d| d.to_string()),
        )
    }
}

/// Campo de texto de uma linha com o rótulo à esquerda, ocupando o resto da largura.
pub fn texto_com_rotulo(ui: &mut Ui, rotulo: &str, texto: &mut String) {
    ui.horizontal(|ui| {
        ui.label(rotulo);
        ui.add(egui::TextEdit::singleline(texto).desired_width(f32::INFINITY));
    });
}

/// Número opcional: caixa para ligar (com o rótulo) e o valor ao lado quando ligada.
/// Ligar começa em 0; desligar apaga o valor. `max_decimais` limita as casas mostradas
/// (`None`: o padrão do egui).
pub fn numero_opcional(
    ui: &mut Ui,
    rotulo: &str,
    v: &mut Option<f64>,
    max_decimais: Option<usize>,
) {
    let mut ligado = v.is_some();
    ui.checkbox(&mut ligado, rotulo);
    match (ligado, v.is_some()) {
        (true, false) => *v = Some(0.0),
        (false, true) => *v = None,
        _ => {}
    }
    if let Some(x) = v {
        let mut d = egui::DragValue::new(x)
            .speed(0.01)
            .custom_parser(ler_numero);
        if let Some(n) = max_decimais {
            d = d.max_decimals(n);
        }
        ui.add(d);
    }
}

/// Campo de texto de várias linhas com o rótulo em cima, ocupando a largura toda. Para
/// descrições ("o que aconteceu", ações, observações), onde uma linha só esconde o texto.
pub fn texto_longo_com_rotulo(ui: &mut Ui, rotulo: &str, texto: &mut String) {
    ui.label(rotulo);
    ui.add(
        egui::TextEdit::multiline(texto)
            .desired_rows(2)
            .desired_width(f32::INFINITY),
    );
}

/// Lê um número digitado com vírgula ou ponto decimal: `2,5`, `2.5`, `1.234,5` e
/// `1,234.5` dão o que se espera. Com os dois separadores, o último é o decimal e o
/// outro é de milhar. Espaços são ignorados. Para usar em
/// `DragValue::custom_parser(ler_numero)`, já que o egui só aceita ponto.
pub fn ler_numero(texto: &str) -> Option<f64> {
    let t: String = texto.chars().filter(|c| !c.is_whitespace()).collect();
    let normal = match (t.rfind(','), t.rfind('.')) {
        (Some(v), Some(p)) if v > p => t.replace('.', "").replace(',', "."),
        (Some(_), Some(_)) => t.replace(',', ""),
        (Some(_), None) if t.matches(',').count() == 1 => t.replace(',', "."),
        (Some(_), None) => return None,
        _ => t,
    };
    normal.parse().ok()
}

/// Botão que pede confirmação antes de valer (para remover, apagar, descartar): o
/// primeiro clique troca o botão por "Confirmar" (em vermelho) e "Cancelar"; só o
/// "Confirmar" devolve `true`. `id` distingue os botões de uma mesma lista (ex.: o id do
/// item). O estado fica na memória do egui, então a tela não precisa de campo próprio.
pub fn botao_confirmado(
    ui: &mut Ui,
    texto: &str,
    id: impl std::hash::Hash + std::fmt::Debug,
) -> bool {
    use crate::texto::tx;
    let chave = egui::Id::new(("utsim_botao_confirmado", id));
    let pendente = ui.data(|d| d.get_temp::<bool>(chave)).unwrap_or(false);
    if !pendente {
        if ui.button(texto).clicked() {
            ui.data_mut(|d| d.insert_temp(chave, true));
        }
        return false;
    }
    let erro = ui.visuals().error_fg_color;
    let mut sim = false;
    ui.horizontal(|ui| {
        let confirmar = egui::RichText::new(tx("campos-confirmar")).color(erro);
        if ui
            .button(confirmar)
            .on_hover_text(tx("campos-confirmar_dica"))
            .clicked()
        {
            sim = true;
            ui.data_mut(|d| d.remove::<bool>(chave));
        }
        if ui.button(tx("campos-cancelar")).clicked() {
            ui.data_mut(|d| d.remove::<bool>(chave));
        }
    });
    sim
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn numero_com_virgula_ou_ponto() {
        assert_eq!(ler_numero("2,5"), Some(2.5));
        assert_eq!(ler_numero(" 2.5 "), Some(2.5));
        assert_eq!(ler_numero("1.234,5"), Some(1234.5));
        assert_eq!(ler_numero("1,234.5"), Some(1234.5));
        assert_eq!(ler_numero("1 000"), Some(1000.0));
        assert_eq!(ler_numero("-0,75"), Some(-0.75));
        assert_eq!(ler_numero("1,2,3"), None);
        assert_eq!(ler_numero("abc"), None);
    }

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
    fn hora_e_data_com_hora() {
        assert_eq!(
            Hora::ler("7:05").map(|h| h.to_string()),
            Some("07:05".into())
        );
        assert_eq!(Hora::ler("14h30"), Hora::nova(14, 30));
        for t in ["24:00", "12:60", "7:5", "12", "", "a:00", "12:345"] {
            assert_eq!(Hora::ler(t), None, "{t}");
        }
        let d = DataHora::ler("17/11/2026 8:00").unwrap();
        assert_eq!(d.to_string(), "2026-11-17 08:00");
        assert!(DataHora::ler("2026-11-17 08:01").unwrap() > d);
        assert_eq!(DataHora::ler("2026-11-17"), None);
        assert_eq!(DataHora::ler("2026-02-30 08:00"), None);
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
