//! Ergonomics on the time study of the man-machine example and its improved version.

use std::path::Path;

use utsim_diagram::{rastro, tempos};
use utsim_model::Model;
use utsim_seguranca_core::ergonomia::{Marcacao, Metodo, avaliar, comparar};

fn estudo(nome: &str) -> tempos::Estudo {
    let p = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../examples")
        .join(nome);
    let model = Model::load(&p).unwrap();
    let r = rastro::capturar(&model, 0).unwrap();
    tempos::estudar(&model, &r, &tempos::Opcoes::default()).unwrap()
}

#[test]
fn grades_follow_the_marks_and_compare_with_the_same_pair_of_studies() {
    let antes = estudo("homem_maquina.ron");
    let depois = estudo("homem_maquina_melhorado.ron");
    let m = Metodo::default();
    // Unmarked: every element neutral, grade zero.
    let neutro = avaliar(&antes, &[], &m);
    assert!(!neutro.is_empty());
    assert!(neutro.iter().all(|p| p.nota == 0.0));
    // Heavy and bent on every element of the "before" study.
    let pesado: Vec<Marcacao> = neutro
        .iter()
        .flat_map(|p| p.elementos.iter())
        .map(|e| Marcacao {
            ator: e.ator.clone(),
            elemento: e.elemento.clone(),
            tronco: 2,
            bracos: 2,
            carga_kg: 15.0,
            pega: 1,
            ..Marcacao::default()
        })
        .collect();
    let a = avaliar(&antes, &pesado, &m);
    assert!(
        a.iter()
            .all(|p| p.nota > 0.0 && p.elementos.iter().all(|e| e.marcado))
    );
    for p in &a {
        assert!(p.media_ponderada <= p.nota + 1e-12);
        for e in &p.elementos {
            assert!(e.por_hora > 0.0 && (0.0..=1.0).contains(&e.fracao));
        }
    }
    // Same marks after the change: the comparison pairs the stations and carries the
    // standard times of the two studies.
    let d = avaliar(&depois, &pesado, &m);
    let c = comparar(&a, &d);
    assert!(!c.is_empty());
    let t = tempos::comparar(&antes, &depois).unwrap();
    let _ = t;
    for linha in &c {
        let pa = a
            .iter()
            .find(|p| p.ator == linha.ator)
            .map(|p| p.tempo_padrao);
        if let Some(pa) = pa {
            assert_eq!(linha.padrao_antes, pa);
        }
    }
}
