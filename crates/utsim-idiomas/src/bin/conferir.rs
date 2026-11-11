//! `utsim-idiomas [pasta]`: lists, for each language and app, the keys missing or left over
//! compared with Portuguese (Brazil), the arguments that differ and the mistakes of syntax.
//! The folder is `idiomas` (the one of the repository, or the one next to the program) when
//! not given. Exits with 1 when something differs.

use std::path::PathBuf;

fn main() {
    let raiz = std::env::args_os()
        .nth(1)
        .map(PathBuf::from)
        .or_else(|| {
            let aqui = PathBuf::from("idiomas");
            aqui.is_dir().then_some(aqui)
        })
        .or_else(utsim_idiomas::pasta_substituta)
        .unwrap_or_else(|| PathBuf::from("idiomas"));
    if !raiz.join("pt-BR").is_dir() {
        eprintln!(
            "{} não tem a pasta pt-BR. Uso: utsim-idiomas [pasta idiomas]",
            raiz.display()
        );
        std::process::exit(2);
    }
    let relatorios = utsim_idiomas::conferencia::conferir(&raiz);
    let texto = utsim_idiomas::conferencia::texto(&relatorios);
    let apps: std::collections::BTreeSet<_> = relatorios.iter().map(|r| &r.app).collect();
    if texto.is_empty() {
        println!(
            "{}: {} app(s), todos os idiomas completos.",
            raiz.display(),
            apps.len()
        );
    } else {
        print!("{texto}");
        std::process::exit(1);
    }
}
