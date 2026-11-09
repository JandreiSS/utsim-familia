//! The text catalogue of the interface (EN13, `docs/plano_ensino_e_jogo.md`, 3.8): every
//! text a screen shows comes from here, in Portuguese (Brazil), English and Spanish, and a
//! small renderer of the statements' Markdown (headings, lists, bold).
//!
//! Each entry is a function that answers in the language chosen in "Ajustes"
//! ([`definir`]). A text with values has `{}` (in order) or `{0}`, `{1}` (by position, for a
//! language with another word order) and goes through [`f`]. The screens never write a
//! literal text: the test `nenhum_texto_fora_do_catalogo` reads their source and fails on
//! one. The mission packages bring their own texts, in the language of the package.

use std::fmt::Display;
use std::sync::atomic::{AtomicU8, Ordering};

use egui::{RichText, Ui};

/// The languages of the interface.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Idioma {
    Pt,
    En,
    Es,
}

impl Idioma {
    pub const TODOS: [Idioma; 3] = [Idioma::Pt, Idioma::En, Idioma::Es];

    /// The code saved in the profile.
    pub fn codigo(self) -> &'static str {
        match self {
            Idioma::Pt => "pt-BR",
            Idioma::En => "en",
            Idioma::Es => "es",
        }
    }

    /// The name of the language in itself.
    pub fn nome(self) -> &'static str {
        match self {
            Idioma::Pt => "Português",
            Idioma::En => "English",
            Idioma::Es => "Español",
        }
    }

    /// From a code (`pt-BR`, `pt`, `en-US`, `es`...); Portuguese when unknown.
    pub fn de_codigo(c: &str) -> Idioma {
        let c = c.trim().to_ascii_lowercase();
        if c.starts_with("en") {
            Idioma::En
        } else if c.starts_with("es") {
            Idioma::Es
        } else {
            Idioma::Pt
        }
    }
}

static IDIOMA: AtomicU8 = AtomicU8::new(0);

/// The language of the interface now.
pub fn idioma_atual() -> Idioma {
    match IDIOMA.load(Ordering::Relaxed) {
        1 => Idioma::En,
        2 => Idioma::Es,
        _ => Idioma::Pt,
    }
}

/// Chooses the language of the interface (for the whole program).
pub fn definir(i: Idioma) {
    IDIOMA.store(
        match i {
            Idioma::Pt => 0,
            Idioma::En => 1,
            Idioma::Es => 2,
        },
        Ordering::Relaxed,
    );
}

/// One of the three texts, by the language now.
pub fn escolher(pt: &'static str, en: &'static str, es: &'static str) -> &'static str {
    match idioma_atual() {
        Idioma::Pt => pt,
        Idioma::En => en,
        Idioma::Es => es,
    }
}

/// Fills a text of the catalogue: `{}` takes the next value, `{n}` the value `n`.
pub fn f(modelo: &str, valores: &[&dyn Display]) -> String {
    let mut s = String::with_capacity(modelo.len() + 16);
    let mut proximo = 0;
    let mut resto = modelo;
    while let Some(i) = resto.find('{') {
        s.push_str(&resto[..i]);
        let depois = &resto[i + 1..];
        match depois.find('}') {
            Some(j) if depois[..j].chars().all(|c| c.is_ascii_digit()) => {
                let n = if j == 0 {
                    proximo += 1;
                    proximo - 1
                } else {
                    depois[..j].parse().unwrap_or(usize::MAX)
                };
                if let Some(v) = valores.get(n) {
                    s.push_str(&v.to_string());
                }
                resto = &depois[j + 1..];
            }
            _ => {
                s.push('{');
                resto = depois;
            }
        }
    }
    s.push_str(resto);
    s
}

/// Declares the entries of a catalogue: `nome: "pt", "en", "es";`. Each becomes
/// `pub fn nome() -> &'static str`, and `TODOS` lists them for the tests.
macro_rules! textos {
    ($($(#[$m:meta])* $nome:ident: $pt:expr, $en:expr, $es:expr;)*) => {
        $(
            $(#[$m])*
            pub fn $nome() -> &'static str {
                $crate::texto::escolher($pt, $en, $es)
            }
        )*
        /// Every entry: name, Portuguese, English, Spanish.
        #[allow(dead_code)]
        pub(super) const TODOS: &[(&str, &str, &str, &str)] = &[$((stringify!($nome), $pt, $en, $es)),*];
    };
}

mod cat_abertura;
mod cat_missao;
mod cat_turma;
mod cat_vista;
mod geral;
pub use cat_abertura::*;
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
