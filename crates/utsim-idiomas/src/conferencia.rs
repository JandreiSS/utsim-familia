//! Compares the files of each language with Portuguese (Brazil), the source: keys missing,
//! keys left over, arguments (`{ $nome }`) that differ, and mistakes of syntax. The test of
//! the crate and the program `utsim-idiomas` both use it.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use fluent_syntax::ast::{Entry, Expression, InlineExpression, Pattern, PatternElement};

use crate::Idioma;

/// Each key of a file and the names of the arguments it uses.
pub type Chaves = BTreeMap<String, BTreeSet<String>>;

/// The keys of a Fluent file, or its mistakes of syntax.
pub fn chaves(fonte: &str) -> Result<Chaves, Vec<String>> {
    let recurso = match fluent_syntax::parser::parse(fonte) {
        Ok(r) => r,
        Err((_, erros)) => {
            return Err(erros
                .iter()
                .map(|e| {
                    let linha = fonte[..e.pos.start.min(fonte.len())].lines().count().max(1);
                    format!("linha {linha}: {:?}", e.kind)
                })
                .collect());
        }
    };
    let mut c = Chaves::new();
    for e in &recurso.body {
        let (id, valor, atributos) = match e {
            Entry::Message(m) => (m.id.name.to_string(), m.value.as_ref(), &m.attributes),
            Entry::Term(t) => (format!("-{}", t.id.name), Some(&t.value), &t.attributes),
            _ => continue,
        };
        let mut args = BTreeSet::new();
        if let Some(p) = valor {
            argumentos_padrao(p, &mut args);
        }
        for a in atributos {
            argumentos_padrao(&a.value, &mut args);
        }
        c.insert(id, args);
    }
    Ok(c)
}

fn argumentos_padrao(p: &Pattern<&str>, args: &mut BTreeSet<String>) {
    for e in &p.elements {
        if let PatternElement::Placeable { expression } = e {
            argumentos_expressao(expression, args);
        }
    }
}

fn argumentos_expressao(e: &Expression<&str>, args: &mut BTreeSet<String>) {
    match e {
        Expression::Inline(i) => argumentos_inline(i, args),
        Expression::Select { selector, variants } => {
            argumentos_inline(selector, args);
            for v in variants {
                argumentos_padrao(&v.value, args);
            }
        }
    }
}

fn argumentos_inline(i: &InlineExpression<&str>, args: &mut BTreeSet<String>) {
    match i {
        InlineExpression::VariableReference { id } => {
            args.insert(id.name.to_string());
        }
        InlineExpression::Placeable { expression } => argumentos_expressao(expression, args),
        InlineExpression::FunctionReference { arguments, .. } => {
            for a in &arguments.positional {
                argumentos_inline(a, args);
            }
            for a in &arguments.named {
                argumentos_inline(&a.value, args);
            }
        }
        InlineExpression::TermReference {
            arguments: Some(arguments),
            ..
        } => {
            for a in &arguments.positional {
                argumentos_inline(a, args);
            }
            for a in &arguments.named {
                argumentos_inline(&a.value, args);
            }
        }
        _ => {}
    }
}

/// What a file of one language has different from Portuguese (Brazil).
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Relatorio {
    pub idioma: String,
    pub app: String,
    /// Keys of pt-BR that this language lacks (they show in Portuguese).
    pub faltando: Vec<String>,
    /// Keys this language has that pt-BR no longer has.
    pub sobrando: Vec<String>,
    /// Keys whose arguments differ: key, arguments in pt-BR, arguments here.
    pub argumentos: Vec<(String, Vec<String>, Vec<String>)>,
    /// Mistakes of syntax, or a file missing.
    pub erros: Vec<String>,
}

impl Relatorio {
    pub fn vazio(&self) -> bool {
        self.faltando.is_empty()
            && self.sobrando.is_empty()
            && self.argumentos.is_empty()
            && self.erros.is_empty()
    }
}

/// Compares every file of a folder `idiomas` (one subfolder per language) with pt-BR. Each
/// app of pt-BR gives one report per language, pt-BR included (its own mistakes); the apps a
/// language has and pt-BR has not give a report with an error.
pub fn conferir(raiz: &Path) -> Vec<Relatorio> {
    let mut saida = Vec::new();
    let fonte = Idioma::PtBr.codigo();
    let apps = arquivos(&raiz.join(fonte));
    for app in &apps {
        let base = std::fs::read_to_string(raiz.join(fonte).join(format!("{app}.ftl")))
            .unwrap_or_default();
        let base = match chaves(&base) {
            Ok(c) => c,
            Err(erros) => {
                saida.push(Relatorio {
                    idioma: fonte.into(),
                    app: app.clone(),
                    erros,
                    ..Relatorio::default()
                });
                continue;
            }
        };
        saida.push(Relatorio {
            idioma: fonte.into(),
            app: app.clone(),
            ..Relatorio::default()
        });
        for i in Idioma::TODOS.into_iter().skip(1) {
            let caminho = raiz.join(i.codigo()).join(format!("{app}.ftl"));
            let mut r = Relatorio {
                idioma: i.codigo().into(),
                app: app.clone(),
                ..Relatorio::default()
            };
            match std::fs::read_to_string(&caminho) {
                Err(_) => r.erros.push(format!("falta o arquivo {}", caminho.display())),
                Ok(texto) => match chaves(&texto) {
                    Err(erros) => r.erros = erros,
                    Ok(outro) => comparar(&base, &outro, &mut r),
                },
            }
            saida.push(r);
        }
    }
    for i in Idioma::TODOS.into_iter().skip(1) {
        for app in arquivos(&raiz.join(i.codigo())) {
            if !apps.contains(&app) {
                saida.push(Relatorio {
                    idioma: i.codigo().into(),
                    app: app.clone(),
                    erros: vec![format!("{app}.ftl não existe em pt-BR")],
                    ..Relatorio::default()
                });
            }
        }
    }
    saida
}

/// Fills a report with the differences between pt-BR (`base`) and another language.
pub fn comparar(base: &Chaves, outro: &Chaves, r: &mut Relatorio) {
    for (k, args) in base {
        match outro.get(k) {
            None => r.faltando.push(k.clone()),
            Some(a) if a != args => r.argumentos.push((
                k.clone(),
                args.iter().cloned().collect(),
                a.iter().cloned().collect(),
            )),
            Some(_) => {}
        }
    }
    for k in outro.keys() {
        if !base.contains_key(k) {
            r.sobrando.push(k.clone());
        }
    }
}

/// The apps of a language folder: the names of its `.ftl` files.
fn arquivos(pasta: &Path) -> BTreeSet<String> {
    std::fs::read_dir(pasta)
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|e| {
            let p = e.path();
            (p.extension()? == "ftl").then(|| p.file_stem()?.to_str().map(String::from))?
        })
        .collect()
}

/// The report as text for a translator, in Portuguese.
pub fn texto(relatorios: &[Relatorio]) -> String {
    use std::fmt::Write;
    let mut s = String::new();
    for r in relatorios.iter().filter(|r| !r.vazio()) {
        let _ = writeln!(s, "== {} / {}.ftl", r.idioma, r.app);
        for e in &r.erros {
            let _ = writeln!(s, "  erro: {e}");
        }
        for k in &r.faltando {
            let _ = writeln!(s, "  falta: {k}");
        }
        for k in &r.sobrando {
            let _ = writeln!(s, "  sobra: {k}");
        }
        for (k, base, aqui) in &r.argumentos {
            let _ = writeln!(
                s,
                "  argumentos de {k}: pt-BR tem {{{}}}, aqui {{{}}}",
                base.join(", "),
                aqui.join(", ")
            );
        }
    }
    s
}
