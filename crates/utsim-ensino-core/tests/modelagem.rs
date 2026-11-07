//! The third step (EN6): the reference models of the case studies get full marks, and a
//! model with a planted error fails the right criterion. Also the computed questions of
//! work measurement against the formulas by hand.

use std::path::PathBuf;

use utsim_ensino_core::campanha::carregar_campanha;
use utsim_ensino_core::missao::Calculo;
use utsim_ensino_core::modelagem::corrigir_modelagem;
use utsim_ensino_core::nota::{RespostaPrevisao, Resultado, Tentativa};
use utsim_ensino_core::pacote::PacoteCarregado;

fn raiz() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn ler(p: &str) -> String {
    std::fs::read_to_string(raiz().join(p)).unwrap()
}

fn corrigir(p: &PacoteCarregado, id: &str, modelo: Option<String>, prev: usize) -> Resultado {
    let m = p.missao(id).unwrap();
    let mut t = Tentativa {
        modelo,
        justificativas: vec![Some(0); m.missao.justificativa.len()],
        ..Default::default()
    };
    for pv in &m.missao.previsao {
        t.previsoes
            .insert(pv.id().into(), RespostaPrevisao::Escolha(prev));
    }
    corrigir_modelagem(m, &p.pacote.pesos, &t).unwrap()
}

fn falhas(r: &Resultado) -> Vec<String> {
    r.criterios
        .iter()
        .filter(|c| !c.ok)
        .map(|c| c.rotulo.clone())
        .collect()
}

#[test]
fn gabaritos_tiram_nota_maxima() {
    let p = carregar_campanha().unwrap();
    // The case study's own reference model, as published in the manual.
    let caso1 = ler("docs/estudos_de_caso/01-linha-de-montagem/gabarito/modelo.ron");
    // "Não": the line does not reach 850 approved controllers a day.
    let r = corrigir(&p, "ato7/linha_retrabalho", Some(caso1), 1);
    assert!(r.resolvida, "{:#?}", r.criterios);
    assert_eq!(r.nota, 100.0, "{r:#?}");
    assert_eq!(r.estrelas, 3);

    let padaria = ler("ensino/campanha/modelos/padaria_gabarito.ron");
    // The oven limits the bakery.
    let r = corrigir(&p, "ato7/padaria", Some(padaria), 1);
    assert!(r.resolvida, "{:#?}", r.criterios);
    assert_eq!(r.nota, 100.0, "{r:#?}");
}

#[test]
fn erro_plantado_reprovado_no_criterio_certo() {
    let p = carregar_campanha().unwrap();
    let caso1 = ler("docs/estudos_de_caso/01-linha-de-montagem/gabarito/modelo.ron");

    // Rework that never comes back: the reproved go to a sink of their own.
    let sem_volta = caso1
        .replace(
            r#"        ("retrabalho", "fila_retorno"),"#,
            r#"        ("retrabalho", "descarte"),"#,
        )
        .replace(
            r#"        (name: "expedicao", kind: Sink, position: (48.0, 0.0, 0.0)),"#,
            "        (name: \"expedicao\", kind: Sink, position: (48.0, 0.0, 0.0)),\n        (name: \"descarte\", kind: Sink, position: (30.0, 9.0, 0.0)),",
        );
    assert_ne!(sem_volta, caso1);
    let r = corrigir(&p, "ato7/linha_retrabalho", Some(sem_volta), 1);
    let f = falhas(&r);
    assert!(f.iter().any(|x| x.contains("voltam")), "{f:?}");
    assert!(
        !f.iter()
            .any(|x| x.contains("3 bancadas") || x.contains("quebra")),
        "{f:?}"
    );
    assert!(!r.resolvida);

    // One inspection bench instead of three.
    let uma = caso1.replace(
        "(name: \"inspecao\", kind: Processor(capacity: 3,",
        "(name: \"inspecao\", kind: Processor(capacity: 1,",
    );
    assert_ne!(uma, caso1);
    let r = corrigir(&p, "ato7/linha_retrabalho", Some(uma), 1);
    let f = falhas(&r);
    assert!(f.iter().any(|x| x.contains("3 bancadas")), "{f:?}");
    assert!(!f.iter().any(|x| x.contains("voltam")), "{f:?}");

    // The oven without breakdowns.
    let padaria = ler("ensino/campanha/modelos/padaria_gabarito.ron");
    let sem_quebra = padaria.replace(
        "\n         downtime: (mtbf: Exp(mean: 300.0), mttr: Exp(mean: 20.0)),",
        "",
    );
    assert_ne!(sem_quebra, padaria);
    let r = corrigir(&p, "ato7/padaria", Some(sem_quebra), 1);
    let f = falhas(&r);
    assert!(f.iter().any(|x| x.contains("forno quebra")), "{f:?}");
    assert!(!f.iter().any(|x| x.contains("câmara")), "{f:?}");

    // Wrong names: the measure says which object is missing.
    let nomes = padaria.replace("\"expedicao\"", "\"saida\"");
    let r = corrigir(&p, "ato7/padaria", Some(nomes), 1);
    let detalhes: Vec<&str> = r
        .criterios
        .iter()
        .filter(|c| !c.ok)
        .map(|c| c.detalhe.as_str())
        .collect();
    assert!(
        detalhes.iter().any(|d| d.contains("expedicao")),
        "{detalhes:?}"
    );

    // Nothing delivered, or a file that does not load.
    let r = corrigir(&p, "ato7/padaria", None, 1);
    assert!(!r.resolvida && r.criterios[0].detalhe.contains("nenhum modelo"));
    let r = corrigir(&p, "ato7/padaria", Some("Model(".into()), 1);
    assert!(!r.criterios[0].ok);
    assert!(r.nota < 40.0, "{}", r.nota);
}

#[test]
fn calculos_de_tempos_a_mao() {
    // n' = (20 + 70) / (20 + 4) = 3.75.
    let n = Calculo::MaquinasPorOperador {
        a: 20.0,
        b: 4.0,
        t: 70.0,
    }
    .valor();
    assert!((n - 3.75).abs() < 1e-12);
    let tempos = vec![10.0, 12.0, 11.0, 13.0];
    // Mean 11.5, normal 11.5 × 1.2 = 13.8, standard 13.8 × 1.1 = 15.18.
    let normal = Calculo::TempoNormal {
        tempos: tempos.clone(),
        ritmo: 1.2,
    }
    .valor();
    assert!((normal - 13.8).abs() < 1e-9);
    let padrao = Calculo::TempoPadrao {
        tempos: tempos.clone(),
        ritmo: 1.2,
        tolerancia: 0.1,
    }
    .valor();
    assert!((padrao - 15.18).abs() < 1e-9);
    // s = 1.2910, n = (1.96 × 1.2910 / (0.05 × 11.5))² = 19.37 → 20.
    let ciclos = Calculo::Ciclos { tempos, erro: 0.05 }.valor();
    assert_eq!(ciclos, 20.0);

    // The question mission is solved only with every answer right.
    let p = carregar_campanha().unwrap();
    let m = p.missao("ato6/cronometragem").unwrap();
    let sim = utsim_ensino_core::motor::simular(m, "", &Default::default(), &|_| true).unwrap();
    let mut t = Tentativa::default();
    for pv in &m.missao.previsao {
        if let utsim_ensino_core::missao::Previsao::Calculo { id, calculo, .. } = pv {
            t.previsoes
                .insert(id.clone(), RespostaPrevisao::Valor(calculo.valor()));
        }
    }
    t.justificativas = vec![Some(0), Some(0)];
    let r = utsim_ensino_core::nota::corrigir(&m.missao, &p.pacote.pesos, &sim, &t).unwrap();
    assert!(r.resolvida && r.nota == 100.0, "{r:#?}");
    t.previsoes
        .insert("ciclos".into(), RespostaPrevisao::Valor(10.0));
    let r = utsim_ensino_core::nota::corrigir(&m.missao, &p.pacote.pesos, &sim, &t).unwrap();
    assert!(!r.resolvida && r.nota < 100.0);
}
