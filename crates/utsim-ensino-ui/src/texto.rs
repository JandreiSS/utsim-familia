//! The text catalogue of the interface (EN13, `docs/plano_ensino_e_jogo.md`, 3.8): every
//! text a screen shows comes from `idiomas/<código>/ensino.ftl` (`docs/traducao.md`), in the
//! five languages of the family, and a small renderer of the statements' Markdown (headings,
//! lists, bold).
//!
//! Each entry is a function that answers in the language chosen in "Ajustes"
//! ([`definir`]). A text with values is a [`Modelo`] whose values have names in the files
//! (`{ $nome }`) and go, in the order of its declaration, through [`f`]. The screens never
//! write a literal text: the test `nenhum_texto_fora_do_catalogo` reads their source and
//! fails on one. The mission packages bring their own texts, in the language of the package.

use std::fmt::Display;

use egui::{RichText, Ui};

pub use utsim_idiomas::{Idioma, atual as idioma_atual, definir};

/// The texts of the app in the five languages, embedded from `idiomas/`.
pub static TEXTOS: utsim_idiomas::Catalogo = utsim_idiomas::catalogo!("ensino");

/// A text with values: its key in the files and the names of its values, in the order
/// [`f`] takes them.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Modelo {
    pub chave: &'static str,
    pub nomes: &'static [&'static str],
}

/// The language of a code saved in the profile (`pt-BR`, `en`, `fr`...); Portuguese when
/// unknown.
pub fn idioma_de(codigo: &str) -> Idioma {
    Idioma::de_codigo(codigo).unwrap_or_default()
}

/// Fills a text of the catalogue with its values, in the order of its declaration. A value
/// that is a whole number or a decimal goes as a number (a plural chooses by it, and it is
/// written with the language's decimal mark).
pub fn f(modelo: Modelo, valores: &[&dyn Display]) -> String {
    let args: Vec<(&str, utsim_idiomas::Valor)> = modelo
        .nomes
        .iter()
        .zip(valores)
        .map(|(n, v)| (*n, valor(v.to_string())))
        .collect();
    TEXTOS.f(modelo.chave, &args)
}

/// Like [`f`], with every value kept as written: a mission number `6.2` is not a decimal
/// (it would become `6,2`).
pub fn f_literal(modelo: Modelo, valores: &[&str]) -> String {
    let args: Vec<(&str, utsim_idiomas::Valor)> = modelo
        .nomes
        .iter()
        .zip(valores)
        .map(|(n, v)| (*n, v.to_string().into()))
        .collect();
    TEXTOS.f(modelo.chave, &args)
}

fn valor(s: String) -> utsim_idiomas::Valor<'static> {
    if let Ok(n) = s.parse::<i64>()
        && n.to_string() == s
    {
        return n.into();
    }
    if let Ok(x) = s.parse::<f64>()
        && x.is_finite()
        && x.to_string() == s
    {
        return x.into();
    }
    s.into()
}

/// Declares the plain entries of a catalogue: `nome;`. Each becomes
/// `pub fn nome() -> &'static str`, the text of the key `nome`.
macro_rules! textos {
    ($($(#[$m:meta])* $nome:ident;)*) => {
        $(
            $(#[$m])*
            pub fn $nome() -> &'static str {
                $crate::texto::TEXTOS.t(stringify!($nome))
            }
        )*
        /// Every plain entry, for the tests.
        #[allow(dead_code)]
        pub(super) const NOMES: &[&str] = &[$(stringify!($nome)),*];
    };
}

/// Declares the entries with values: `nome(valor1, valor2);`. Each becomes
/// `pub fn nome() -> Modelo`, filled by [`f`].
macro_rules! modelos {
    ($($(#[$m:meta])* $nome:ident($($arg:ident),*);)*) => {
        $(
            $(#[$m])*
            pub fn $nome() -> $crate::texto::Modelo {
                $crate::texto::Modelo {
                    chave: stringify!($nome),
                    nomes: &[$(stringify!($arg)),*],
                }
            }
        )*
        /// Every entry with values, for the tests.
        #[allow(dead_code)]
        pub(super) const MODELOS: &[(&str, &[&str])] = &[$((stringify!($nome), &[$(stringify!($arg)),*])),*];
    };
}

mod cat_abertura;
mod cat_editor;
mod cat_missao;
mod cat_turma;
mod cat_vista;
mod geral;
pub use cat_abertura::*;
pub use cat_editor::*;
pub use cat_missao::*;
pub use cat_turma::*;
pub use cat_vista::*;
pub use geral::*;

/// The warning of a package that needs more than this version has.
pub fn requisitos(faltas: &[utsim_ensino_core::requisitos::Falta]) -> String {
    let lista: Vec<String> = faltas.iter().map(|x| x.to_string()).collect();
    f(requisitos_aviso(), &[&lista.join(", ")])
}

/// Draws a statement: `#` headings, `-` and `1.` lists, `**bold**` as strong text.
pub fn markdown(ui: &mut Ui, md: &str) {
    for bloco in md.split("\n\n") {
        let bloco = bloco.trim();
        if bloco.is_empty() {
            continue;
        }
        for linha in bloco.lines() {
            let l = linha.trim_end();
            if let Some(t) = l.strip_prefix("# ") {
                ui.label(RichText::new(t).size(22.0).strong());
            } else if let Some(t) = l.strip_prefix("## ") {
                ui.add_space(4.0);
                ui.label(RichText::new(t).strong().size(16.0));
            } else if let Some(t) = l.trim_start().strip_prefix("- ") {
                ui.horizontal_wrapped(|ui| {
                    ui.label("  •");
                    linha_rica(ui, t);
                });
            } else if l
                .trim_start()
                .chars()
                .next()
                .is_some_and(|c| c.is_ascii_digit())
                && l.trim_start().contains(". ")
            {
                ui.horizontal_wrapped(|ui| {
                    ui.label("  ");
                    linha_rica(ui, l.trim_start());
                });
            } else {
                ui.horizontal_wrapped(|ui| linha_rica(ui, l));
            }
        }
        ui.add_space(6.0);
    }
}

fn linha_rica(ui: &mut Ui, t: &str) {
    ui.spacing_mut().item_spacing.x = 0.0;
    for (i, parte) in t.split("**").enumerate() {
        if parte.is_empty() {
            continue;
        }
        if i % 2 == 1 {
            ui.label(RichText::new(parte).strong());
        } else {
            ui.label(parte);
        }
    }
}

#[cfg(test)]
mod testes;
