//! The languages of the UTSim family, the same for every app (`docs/traducao.md`).
//!
//! Each app has one Fluent file per language, `idiomas/<código>/<app>.ftl` at the root of the
//! repository, which a translator edits without programming. The files are embedded when the
//! app is built ([`catalogo!`]), so the app stays a single program; a folder `idiomas` next to
//! the program (or the folder named by `UTSIM_IDIOMAS`) overrides them, so a translator sees a
//! change without building again. A key missing in a language falls back to Portuguese
//! (Brazil), the source language. Arguments are named (`{ $nome }`).
//!
//! Numbers, money and dates follow the language ([`Idioma::separador_decimal`],
//! [`dinheiro`], [`data`]); the currency stays R$, because it is the company's, not the
//! language's.

use std::borrow::Cow;
use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU8, Ordering};
use std::sync::{Mutex, OnceLock};

use fluent_bundle::{FluentArgs, FluentResource};

pub use fluent_bundle::FluentValue as Valor;

pub mod conferencia;
mod formatos;
#[cfg(feature = "egui")]
mod seletor;

pub use formatos::{agrupar_milhares, data, dinheiro, ler_numero, marca_decimal, mes};
#[cfg(feature = "egui")]
pub use seletor::seletor;

/// The languages of the family. Portuguese (Brazil) is the source: every text is written in
/// it first, and a text missing in another language shows in it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub enum Idioma {
    #[default]
    PtBr,
    En,
    Es,
    Fr,
    It,
}

impl Idioma {
    pub const TODOS: [Idioma; 5] = [Idioma::PtBr, Idioma::En, Idioma::Es, Idioma::Fr, Idioma::It];

    /// The code of the language: the name of its folder in `idiomas/` and the value saved in
    /// the configuration of each app.
    pub fn codigo(self) -> &'static str {
        match self {
            Idioma::PtBr => "pt-BR",
            Idioma::En => "en",
            Idioma::Es => "es",
            Idioma::Fr => "fr",
            Idioma::It => "it",
        }
    }

    /// The name of the language in itself.
    pub fn nome(self) -> &'static str {
        match self {
            Idioma::PtBr => "Português (Brasil)",
            Idioma::En => "English",
            Idioma::Es => "Español",
            Idioma::Fr => "Français",
            Idioma::It => "Italiano",
        }
    }

    /// From a code (`pt-BR`, `pt`, `en-US`, `es_419`, `fr-CA`, `it-IT`...), by its first part.
    pub fn de_codigo(c: &str) -> Option<Idioma> {
        let c = c.trim().to_ascii_lowercase();
        let primeiro = c.split(['-', '_', '.']).next().unwrap_or("");
        match primeiro {
            "pt" => Some(Idioma::PtBr),
            "en" => Some(Idioma::En),
            "es" => Some(Idioma::Es),
            "fr" => Some(Idioma::Fr),
            "it" => Some(Idioma::It),
            _ => None,
        }
    }

    /// The language of the system (on Windows, the language of the user's account), when it
    /// is one of the family's.
    pub fn do_sistema() -> Option<Idioma> {
        sys_locale::get_locale().and_then(|c| Idioma::de_codigo(&c))
    }

    /// The language of a first run: the system's, or Portuguese (Brazil).
    pub fn inicial() -> Idioma {
        Idioma::do_sistema().unwrap_or_default()
    }

    /// The decimal mark: a point in English, a comma in the others.
    pub fn separador_decimal(self) -> char {
        match self {
            Idioma::En => '.',
            _ => ',',
        }
    }

    /// The mark between thousands: a comma in English, a no-break space in French, a point in
    /// the others (always different from the decimal mark).
    pub fn separador_milhar(self) -> char {
        match self {
            Idioma::En => ',',
            Idioma::Fr => '\u{a0}',
            _ => '.',
        }
    }

    fn indice(self) -> usize {
        self as usize
    }

    fn de_indice(i: u8) -> Idioma {
        Idioma::TODOS.get(i as usize).copied().unwrap_or_default()
    }
}

static GLOBAL: AtomicU8 = AtomicU8::new(0);

thread_local! {
    static DA_THREAD: std::cell::Cell<Option<Idioma>> = const { std::cell::Cell::new(None) };
}

/// The language of the interface now: the one of this thread when set
/// ([`definir_nesta_thread`]), otherwise the program's ([`definir`]).
pub fn atual() -> Idioma {
    DA_THREAD
        .with(|c| c.get())
        .unwrap_or_else(|| Idioma::de_indice(GLOBAL.load(Ordering::Relaxed)))
}

/// Chooses the language of the whole program.
pub fn definir(i: Idioma) {
    GLOBAL.store(i as u8, Ordering::Relaxed);
}

/// Chooses the language of this thread only (`None` follows the program's again): the tests
/// that run side by side each keep their own.
pub fn definir_nesta_thread(i: Option<Idioma>) {
    DA_THREAD.with(|c| c.set(i));
}

/// Embeds the five files of an app (`idiomas/<código>/<app>.ftl`) in a [`Catalogo`]. Used
/// from a crate in `crates/`, as `static TEXTOS: Catalogo = utsim_idiomas::catalogo!("ensino");`.
#[macro_export]
macro_rules! catalogo {
    ($app:literal) => {
        $crate::Catalogo::novo(
            $app,
            [
                include_str!(concat!(
                    env!("CARGO_MANIFEST_DIR"),
                    "/../../idiomas/pt-BR/",
                    $app,
                    ".ftl"
                )),
                include_str!(concat!(
                    env!("CARGO_MANIFEST_DIR"),
                    "/../../idiomas/en/",
                    $app,
                    ".ftl"
                )),
                include_str!(concat!(
                    env!("CARGO_MANIFEST_DIR"),
                    "/../../idiomas/es/",
                    $app,
                    ".ftl"
                )),
                include_str!(concat!(
                    env!("CARGO_MANIFEST_DIR"),
                    "/../../idiomas/fr/",
                    $app,
                    ".ftl"
                )),
                include_str!(concat!(
                    env!("CARGO_MANIFEST_DIR"),
                    "/../../idiomas/it/",
                    $app,
                    ".ftl"
                )),
            ],
        )
    };
}

type Pacote = fluent_bundle::concurrent::FluentBundle<FluentResource>;

/// The texts of one app in the five languages.
pub struct Catalogo {
    app: &'static str,
    fontes: [&'static str; 5],
    pacotes: [OnceLock<Pacote>; 5],
    fixos: Mutex<BTreeMap<(u8, &'static str), &'static str>>,
    faltas: Mutex<BTreeSet<String>>,
}

impl Catalogo {
    /// The catalogue of `app` from the embedded files, in the order of [`Idioma::TODOS`].
    /// [`catalogo!`] writes this call.
    pub const fn novo(app: &'static str, fontes: [&'static str; 5]) -> Catalogo {
        Catalogo {
            app,
            fontes,
            pacotes: [const { OnceLock::new() }; 5],
            fixos: Mutex::new(BTreeMap::new()),
            faltas: Mutex::new(BTreeSet::new()),
        }
    }

    /// The name of the app's files.
    pub fn app(&self) -> &'static str {
        self.app
    }

    fn pacote(&self, i: Idioma) -> &Pacote {
        self.pacotes[i.indice()].get_or_init(|| {
            let lingua = i.codigo().parse().expect("language code");
            let mut p = Pacote::new_concurrent(vec![lingua]);
            p.set_use_isolating(false);
            p.set_formatter(Some(if i.separador_decimal() == ',' {
                numero_com_virgula
            } else {
                numero_com_ponto
            }));
            // A file with a mistake still gives the entries that parsed.
            let embutido = match FluentResource::try_new(self.fontes[i.indice()].to_string()) {
                Ok(r) | Err((r, _)) => r,
            };
            let _ = p.add_resource(embutido);
            if let Some(texto) = substituto(i, self.app) {
                let r = match FluentResource::try_new(texto) {
                    Ok(r) | Err((r, _)) => r,
                };
                p.add_resource_overriding(r);
            }
            p
        })
    }

    fn formatar_em(&self, i: Idioma, chave: &str, args: Option<&FluentArgs>) -> Option<String> {
        let p = self.pacote(i);
        let padrao = p.get_message(chave)?.value()?;
        let mut erros = Vec::new();
        Some(p.format_pattern(padrao, args, &mut erros).into_owned())
    }

    fn formatar(&self, chave: &str, args: Option<&FluentArgs>) -> String {
        let i = atual();
        if let Some(s) = self.formatar_em(i, chave, args) {
            return s;
        }
        if i != Idioma::PtBr
            && let Some(s) = self.formatar_em(Idioma::PtBr, chave, args)
        {
            return s;
        }
        if let Ok(mut f) = self.faltas.lock() {
            f.insert(chave.to_string());
        }
        chave.to_string()
    }

    /// The text of `chave` in the language now (without arguments), kept for the rest of the
    /// program, so a screen can hold it as `&'static str`.
    pub fn t(&self, chave: &'static str) -> &'static str {
        let i = atual();
        let k = (i as u8, chave);
        if let Some(s) = self.fixos.lock().ok().and_then(|m| m.get(&k).copied()) {
            return s;
        }
        let s: &'static str = Box::leak(self.formatar(chave, None).into_boxed_str());
        if let Ok(mut m) = self.fixos.lock() {
            m.insert(k, s);
        }
        s
    }

    /// The text of `chave` in the language now, with named arguments:
    /// `f("dias", &[("n", 3.into())])`. Numbers go as numbers (a plural chooses by them),
    /// written with the language's decimal mark.
    pub fn f(&self, chave: &str, args: &[(&str, Valor)]) -> String {
        let mut a = FluentArgs::with_capacity(args.len());
        for (nome, v) in args {
            a.set(Cow::Owned(nome.to_string()), v.clone());
        }
        self.formatar(chave, Some(&a))
    }

    /// Whether the source language (pt-BR) has `chave`.
    pub fn tem(&self, chave: &str) -> bool {
        self.pacote(Idioma::PtBr).has_message(chave)
    }

    /// The keys asked for that no file has (not even pt-BR): a test draws every screen and
    /// expects none.
    pub fn faltas(&self) -> Vec<String> {
        self.faltas
            .lock()
            .map(|f| f.iter().cloned().collect())
            .unwrap_or_default()
    }
}

/// One value per language, made the first time the language asks for it: an app keeps its
/// table of texts (`&'static str` fields) here and rebuilds nothing when the language changes.
pub struct PorIdioma<T>([OnceLock<T>; 5]);

impl<T> PorIdioma<T> {
    pub const fn new() -> PorIdioma<T> {
        PorIdioma([const { OnceLock::new() }; 5])
    }

    /// The value of the language now.
    pub fn obter(&self, criar: impl FnOnce() -> T) -> &T {
        self.0[atual().indice()].get_or_init(criar)
    }
}

impl<T> Default for PorIdioma<T> {
    fn default() -> Self {
        PorIdioma::new()
    }
}

/// A number in a text, with a comma as decimal mark.
fn numero_com_virgula<M>(v: &Valor, _: &M) -> Option<String> {
    match v {
        Valor::Number(n) => Some(n.as_string().replace('.', ",")),
        _ => None,
    }
}

fn numero_com_ponto<M>(_: &Valor, _: &M) -> Option<String> {
    None
}

/// The folder of the files that override the embedded ones: `UTSIM_IDIOMAS`, or `idiomas`
/// next to the program.
pub fn pasta_substituta() -> Option<PathBuf> {
    if let Some(p) = std::env::var_os("UTSIM_IDIOMAS") {
        return Some(PathBuf::from(p));
    }
    let exe = std::env::current_exe().ok()?;
    Some(exe.parent()?.join("idiomas"))
}

fn substituto(i: Idioma, app: &str) -> Option<String> {
    let arquivo = pasta_substituta()?
        .join(i.codigo())
        .join(format!("{app}.ftl"));
    std::fs::read_to_string(arquivo).ok()
}

#[cfg(test)]
mod testes;
