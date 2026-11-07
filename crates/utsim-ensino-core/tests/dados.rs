//! Every generator of data with planted effects is recovered by the analyses the student
//! uses: the fit of `utsim-fit`, the gauge study of `utsim-quality`, the ANOVA of
//! `utsim-stats`, the Nelson rules of the control charts and the effects of a factorial.
//! The same recipe gives the same data.

use utsim_core::Dist;
use utsim_ensino_core::dados::{Gerador, Padrao, Receita};

#[test]
fn mesma_receita_mesmos_dados() {
    let r = Receita {
        gerador: Gerador::Tempos {
            dist: Dist::Gamma {
                shape: 4.0,
                scale: 2.0,
            },
            n: 50,
        },
        semente: 7,
    };
    assert_eq!(r.gerar(), r.gerar());
    let outra = Receita {
        semente: 8,
        ..r.clone()
    };
    assert_ne!(r.gerar().valores, outra.gerar().valores);
    assert!(r.gerar().csv().starts_with("observacao;tempo\n1;"));
}

#[test]
fn tempos_recuperados_pelo_ajuste() {
    let dist = Dist::Gamma {
        shape: 4.0,
        scale: 2.0,
    };
    let r = Receita {
        gerador: Gerador::Tempos {
            dist: dist.clone(),
            n: 600,
        },
        semente: 11,
    };
    let xs: Vec<f64> = r.gerar().valores.iter().map(|v| v[0]).collect();
    let fit = utsim_fit::auto::auto_fit("tempos", &xs, &Default::default()).unwrap();
    // The fitted distribution has the planted mean and spread (within 5 % and 10 %).
    let m = fit.dist.mean();
    assert!(
        (m - dist.mean()).abs() / dist.mean() < 0.05,
        "{} {m}",
        fit.family
    );
    let sd_planted = (4.0f64).sqrt() * 2.0;
    let n = xs.len() as f64;
    let media = xs.iter().sum::<f64>() / n;
    let sd = (xs.iter().map(|x| (x - media).powi(2)).sum::<f64>() / (n - 1.0)).sqrt();
    assert!((sd - sd_planted).abs() / sd_planted < 0.1, "{sd}");
}

#[test]
fn msa_recuperado_pelo_estudo_de_rr() {
    let r = Receita {
        gerador: Gerador::Msa {
            pecas: 10,
            operadores: 3,
            repeticoes: 3,
            media: 25.0,
            sd_peca: 0.5,
            sd_operador: 0.0,
            sd_repetibilidade: 0.1,
        },
        semente: 21,
    };
    let c = r.gerar();
    let ms: Vec<utsim_quality::msa::Measurement> = c
        .linhas
        .iter()
        .zip(&c.valores)
        .map(|(l, v)| utsim_quality::msa::Measurement {
            part: l[0].clone(),
            operator: l[1].clone(),
            value: v[3],
        })
        .collect();
    let data = utsim_quality::msa::GageData::from_measurements(&ms).unwrap();
    let rr = utsim_quality::msa::gage_anova(&data, &Default::default()).unwrap();
    let repet = rr
        .components
        .iter()
        .find(|k| k.name == utsim_quality::msa::REPEATABILITY)
        .unwrap()
        .sd;
    assert!((repet - 0.1).abs() / 0.1 < 0.25, "repetibilidade {repet}");
    let peca = rr
        .components
        .iter()
        .find(|k| k.name == utsim_quality::msa::PART)
        .unwrap()
        .sd;
    assert!((peca - 0.5).abs() / 0.5 < 0.4, "peça {peca}");
}

#[test]
fn media_deslocada_recuperada_pela_anova() {
    let r = Receita {
        gerador: Gerador::MediaDeslocada {
            grupos: vec!["turno A".into(), "turno B".into(), "turno C".into()],
            n_por_grupo: 40,
            media: 50.0,
            sd: 2.0,
            afetado: 2,
            deslocamento: 1.0,
        },
        semente: 31,
    };
    let c = r.gerar();
    let grupos: Vec<Vec<f64>> = (0..3)
        .map(|g| {
            c.valores
                .iter()
                .filter(|v| v[0] as usize == g)
                .map(|v| v[1])
                .collect()
        })
        .collect();
    let refs: Vec<&[f64]> = grupos.iter().map(Vec::as_slice).collect();
    let a = utsim_stats::hypothesis::one_way_anova(&refs, 0.95).unwrap();
    assert!(a.anova.p_value < 0.01, "p {}", a.anova.p_value);
    // The shifted group is the one with the largest mean, about 2 units above.
    let maior = (0..3)
        .max_by(|x, y| a.anova.means[*x].total_cmp(&a.anova.means[*y]))
        .unwrap();
    assert_eq!(maior, 2);
    let dif = a.anova.means[2] - (a.anova.means[0] + a.anova.means[1]) / 2.0;
    assert!((dif - 2.0).abs() < 1.0, "{dif}");
}

#[test]
fn padroes_recuperados_pelas_regras_de_nelson() {
    for padrao in [
        Padrao::Deslocamento {
            inicio: 60,
            tamanho: 2.0,
        },
        Padrao::Tendencia {
            inicio: 60,
            inclinacao: 0.15,
        },
    ] {
        let r = Receita {
            gerador: Gerador::Serie {
                n: 100,
                media: 10.0,
                sd: 0.5,
                padrao: padrao.clone(),
            },
            semente: 41,
        };
        let xs: Vec<f64> = r.gerar().valores.iter().map(|v| v[0]).collect();
        let z: Vec<Option<f64>> = xs.iter().map(|x| Some((x - 10.0) / 0.5)).collect();
        let v = utsim_quality::RuleSet::nelson().evaluate(&z);
        assert!(
            v.iter().any(|x| (60..85).contains(&x.index)),
            "{padrao:?}: {:?}",
            v.iter().map(|x| x.index).collect::<Vec<_>>()
        );
    }
}

#[test]
fn efeitos_do_fatorial_recuperados() {
    let efeitos = vec![4.0, -2.0, 0.0];
    let r = Receita {
        gerador: Gerador::Fatorial {
            fatores: vec!["temperatura".into(), "pressao".into(), "tempo".into()],
            replicas: 4,
            media: 30.0,
            sd: 1.0,
            efeitos: efeitos.clone(),
            interacao_ab: 3.0,
        },
        semente: 51,
    };
    let c = r.gerar();
    assert_eq!(c.valores.len(), 32);
    let efeito = |f: &dyn Fn(&[f64]) -> f64| {
        let (mut alto, mut na, mut baixo, mut nb) = (0.0, 0.0, 0.0, 0.0);
        for v in &c.valores {
            if f(v) > 0.0 {
                alto += v[3];
                na += 1.0;
            } else {
                baixo += v[3];
                nb += 1.0;
            }
        }
        alto / na - baixo / nb
    };
    // Standard error of an effect: 2σ/√N = 0.354; within 3 standard errors.
    for (i, e) in efeitos.iter().enumerate() {
        let est = efeito(&|v: &[f64]| v[i]);
        assert!((est - e).abs() < 1.1, "fator {i}: {est}");
    }
    let ab = efeito(&|v: &[f64]| v[0] * v[1]);
    assert!((ab - 3.0).abs() < 1.1, "interação {ab}");
}
