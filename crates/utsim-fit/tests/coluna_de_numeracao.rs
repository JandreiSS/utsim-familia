//! Arquivos reais dos estudos de caso, todos com uma coluna de numeração antes dos dados:
//! sem `--coluna`, o ajuste usa a coluna de dados e diz qual escolheu.

use std::path::{Path, PathBuf};

use utsim_fit::data::{DataOptions, load};

fn caso(arquivo: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../docs/estudos_de_caso")
        .join(arquivo)
}

fn media(v: &[f64]) -> f64 {
    v.iter().sum::<f64>() / v.len() as f64
}

#[test]
fn tempos_com_numeracao_usam_a_coluna_de_tempos() {
    // `medida;tempo_s`: 1..250 e depois os tempos (média 44,4 s). Antes, o programa
    // ajustava 1..250 e recomendava Uniform(1, 250).
    let l = load(
        &caso("01-linha-de-montagem/dados/tempos_calibracao.csv"),
        &DataOptions::default(),
    )
    .unwrap();
    assert_eq!(l.column, "tempo_s");
    assert_eq!(l.values.len(), 250);
    assert!(
        (media(&l.values) - 44.444).abs() < 0.01,
        "{}",
        media(&l.values)
    );
    assert!(l.values.iter().all(|&v| v > 20.0 && v < 70.0));
    assert!(
        l.notes.iter().any(|n| n.contains("'medida'")),
        "{:?}",
        l.notes
    );
}

#[test]
fn instantes_com_numeracao_usam_a_coluna_de_instantes() {
    // `caminhao;instante_s` com --timestamps: 119 intervalos de média 1732,4 s. Antes,
    // usava a numeração e achava 119 intervalos iguais a 1.
    let o = DataOptions {
        timestamps: true,
        ..DataOptions::default()
    };
    let l = load(
        &caso("02-centro-de-distribuicao/dados/chegadas_caminhoes.csv"),
        &o,
    )
    .unwrap();
    assert_eq!(l.column, "instante_s");
    assert_eq!(l.values.len(), 119);
    assert!(
        (media(&l.values) - 1732.4).abs() < 0.1,
        "{}",
        media(&l.values)
    );
}

#[test]
fn todos_os_dados_dos_casos_carregam_sem_coluna() {
    for arquivo in [
        "01-linha-de-montagem/dados/entre_falhas_solda.csv",
        "01-linha-de-montagem/dados/tempos_inspecao.csv",
        "01-linha-de-montagem/dados/tempos_reparo_solda.csv",
        "02-centro-de-distribuicao/dados/tempos_doca.csv",
        "02-centro-de-distribuicao/dados/tempos_picking.csv",
        "03-clinica-com-turnos/dados/consultas.csv",
        "03-clinica-com-turnos/dados/triagem.csv",
    ] {
        let l = load(&caso(arquivo), &DataOptions::default()).unwrap();
        assert!(l.values.len() >= 60, "{arquivo}");
        assert!(l.values.iter().any(|&v| v > 300.0 || v.fract() != 0.0));
        assert!(!l.column.starts_with("coluna "), "{arquivo}: {}", l.column);
        assert!(
            l.values[0] != 1.0 || l.values[1] != 2.0,
            "{arquivo}: numeração usada"
        );
    }
}
