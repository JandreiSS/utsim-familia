//! The built-in campaign loads, every mission passes validation, and a mission runs.

use utsim_ensino_core::campanha::carregar_campanha;

#[test]
fn campanha_carrega() {
    let p = carregar_campanha().unwrap_or_else(|e| panic!("{}", e.join("\n")));
    assert!(!p.missoes.is_empty());
    assert!(p.faltando.is_empty(), "{:?}", p.faltando);
}

#[test]
fn missoes_resolvidas_viram_competencias() {
    use std::collections::{BTreeMap, BTreeSet};
    use utsim_ensino_core::competencias::{competencias, ferramentas};
    let p = carregar_campanha().unwrap();
    assert!(ferramentas(&p).contains("msa"));
    let mut resolvidas = BTreeMap::new();
    resolvidas.insert(
        "ra1".to_string(),
        BTreeSet::from(["ato8/medir".to_string(), "ato2/ferramentaria".to_string()]),
    );
    resolvidas.insert("ra2".to_string(), BTreeSet::new());
    let c = competencias(&p, &resolvidas);
    let fs: Vec<&str> = c.iter().map(|c| c.ferramenta.as_str()).collect();
    assert_eq!(fs, ["capacidade", "msa"], "{c:#?}");
    assert!(
        c.iter()
            .all(|c| c.registro == "ra1" && c.missao == "ato8/medir")
    );
    // The evidence is stable, so recording twice does not repeat the entry.
    assert_eq!(c, competencias(&p, &resolvidas));
    assert!(c[0].evidencia.contains("ato8/medir"));
}
