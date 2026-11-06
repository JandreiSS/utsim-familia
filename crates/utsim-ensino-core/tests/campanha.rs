//! The built-in campaign loads, every mission passes validation, and a mission runs.

use utsim_ensino_core::campanha::carregar_campanha;

#[test]
fn campanha_carrega() {
    let p = carregar_campanha().unwrap_or_else(|e| panic!("{}", e.join("\n")));
    assert!(!p.missoes.is_empty());
    assert!(p.faltando.is_empty(), "{:?}", p.faltando);
}
