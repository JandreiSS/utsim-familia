//! Control charts on real data: the same limits as `utsim qualidade` for the same file,
//! phase II with frozen limits, reaction cards and the comparison with the simulation.

use std::path::Path;

use utsim_qualidade_core::cep::{Conjunto, Origem, cartoes, comparar, fase1, fase2};
use utsim_quality::charts::{self, ChartData, ChartKind};
use utsim_quality::data;
use utsim_quality::rules::RuleSet;

fn arquivo(nome: &str) -> Vec<u8> {
    std::fs::read(
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../examples/qualidade")
            .join(nome),
    )
    .unwrap()
}

#[test]
fn same_limits_as_utsim_qualidade_for_the_same_file() {
    let bytes = arquivo("diametros.csv");
    let c = Conjunto::de_csv("diametros.csv", Origem::Real, &bytes, Some(5)).unwrap();
    let regras = RuleSet::western_electric();
    let ch = fase1(&c, &regras, &[]).unwrap();
    // The path of `utsim qualidade carta --subgrupo 5 --tipo xbarr --regras we`.
    let (texto, _) = data::decode(&bytes);
    let (grupos, _) = data::subgroups(&data::parse(&texto), Some(5)).unwrap();
    let cli = charts::phase1(
        ChartKind::XbarR,
        &ChartData::Variables(grupos),
        &[],
        &regras,
    )
    .unwrap();
    assert_eq!(ch.limits, cli.limits);
    assert_eq!(ch.panels[0].ucl, cli.panels[0].ucl);
    assert_eq!(c.hash.len(), 64);
    // Phase II with the frozen limits on the same data gives the same panels.
    let f2 = fase2(&ch.limits, &c, &regras).unwrap();
    assert_eq!(f2.panels[0].ucl, ch.panels[0].ucl);
}

#[test]
fn signals_open_reaction_cards_and_the_comparison_finds_a_shift() {
    // 30 values around 10 then 10 values shifted up by 3 sd: phase II on frozen limits.
    let base: Vec<f64> = (0..30)
        .map(|i| 10.0 + ((i * 7 % 11) as f64 - 5.0) * 0.02)
        .collect();
    let novo: Vec<f64> = (0..10)
        .map(|i| 10.3 + ((i * 3 % 5) as f64 - 2.0) * 0.02)
        .collect();
    let estudo = Conjunto::simulado("estudo", &base);
    let regras = RuleSet::western_electric();
    let ch = fase1(&estudo, &regras, &[]).unwrap();
    let real = Conjunto {
        origem: Origem::Real,
        ..Conjunto::simulado("hoje", &novo)
    };
    let f2 = fase2(&ch.limits, &real, &regras).unwrap();
    let c = cartoes(&f2, "parar a máquina e chamar o preparador");
    assert!(!c.is_empty());
    assert!(c.iter().all(|k| k.reacao.starts_with("parar")));
    let cmp = comparar(&real, &estudo).unwrap();
    assert!(!cmp.media_compativel);
    assert!(cmp.diferenca > 0.25);
    assert!(cmp.deriva_em.is_some());
    // The model against itself: compatible, no drift.
    let igual = comparar(&estudo, &estudo).unwrap();
    assert!(igual.media_compativel && igual.deriva_em.is_none());
}
