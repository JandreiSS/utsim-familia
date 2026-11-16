//! The pieces of acts 4 and 5: every assignment that breaks the precedence or the takt is
//! refused with the reason, the indicators by hand on small cases, the exact searches,
//! and the distances of the layout grid equal to the from-to matrix of the run
//! (the trips of `utsim-diagram::rastro`).

use std::collections::BTreeMap;

use utsim_ensino_core::campanha::carregar_campanha;
use utsim_ensino_core::motor::modelo_da_tentativa;
use utsim_ensino_core::nota::portao;
use utsim_ensino_core::pecas::{Arranjo, Balanceamento, Fluxo, MaquinaArranjo, ModeloMix, Tarefa};

fn pequeno() -> Balanceamento {
    let t = |id: &str, tempo: f64, dep: &[&str]| Tarefa {
        id: id.into(),
        nome: id.into(),
        tempo,
        depois_de: dep.iter().map(|s| s.to_string()).collect(),
        tempos: BTreeMap::new(),
    };
    Balanceamento {
        tarefas: vec![
            t("a", 4.0, &[]),
            t("b", 3.0, &["a"]),
            t("c", 5.0, &["a"]),
            t("d", 2.0, &["b", "c"]),
        ],
        takt: 7.0,
        estacoes: 4,
        cv: 0.0,
        permite_passar_takt: false,
        editavel: true,
        inicial: [("a", 1), ("b", 2), ("c", 3), ("d", 4)]
            .iter()
            .map(|(k, v)| (k.to_string(), *v))
            .collect(),
        prefixo: "est".into(),
        mix: Vec::new(),
        rotulo: "modelo".into(),
    }
}

fn atrib(pares: &[(&str, u32)]) -> BTreeMap<String, u32> {
    pares.iter().map(|(k, v)| (k.to_string(), *v)).collect()
}

#[test]
fn indicadores_a_mao() {
    let b = pequeno();
    // a+b = 7, c+d = 7: two stations, efficiency 14 / (2 × 7) = 1.
    let r = b.resumo(&atrib(&[("a", 1), ("b", 1), ("c", 2), ("d", 2)]));
    assert_eq!(r.estacoes_usadas, 2);
    assert_eq!(r.ciclo, 7.0);
    assert!((r.eficiencia - 1.0).abs() < 1e-12);
    // Four stations: 14 / 28 = 0.5, delay 0.5.
    let r = b.resumo(&b.inicial);
    assert!((r.eficiencia - 0.5).abs() < 1e-12);
    assert!((r.atraso - 0.5).abs() < 1e-12);
    let o = b.otimo().unwrap();
    assert_eq!(b.resumo(&o).estacoes_usadas, 2);
    assert!(b.violacao(&o).is_none());
}

#[test]
fn precedencia_e_takt_recusados_com_a_razao() {
    let b = pequeno();
    let v = b
        .violacao(&atrib(&[("a", 2), ("b", 1), ("c", 2), ("d", 3)]))
        .unwrap();
    assert!(
        v.contains("precedência") && v.contains("\"b\"") && v.contains("\"a\""),
        "{v}"
    );
    let v = b
        .violacao(&atrib(&[("a", 1), ("b", 1), ("c", 1), ("d", 2)]))
        .unwrap();
    assert!(v.contains("takt"), "{v}");
    let v = b
        .violacao(&atrib(&[("a", 1), ("b", 1), ("c", 2), ("d", 9)]))
        .unwrap();
    assert!(v.contains("estação de 1 a 4"), "{v}");

    // Every assignment of the small case that breaks a precedence is refused.
    for a in 1..=4u32 {
        for bb in 1..=4u32 {
            for c in 1..=4u32 {
                for d in 1..=4u32 {
                    let x = atrib(&[("a", a), ("b", bb), ("c", c), ("d", d)]);
                    let quebra = bb < a || c < a || d < bb || d < c;
                    if quebra {
                        assert!(b.violacao(&x).is_some(), "{x:?}");
                    }
                }
            }
        }
    }

    // Through the mission gate, with the decision keys of the screen.
    let p = carregar_campanha().unwrap();
    let m = &p.missao("ato4/ventilador").unwrap().missao;
    let mut d = m.completar(&m.referencia);
    d.insert("tarefa.N".into(), 1.0);
    let motivo = portao(m, &d).unwrap();
    assert!(
        motivo.contains("precedência") && motivo.contains("Embalar"),
        "{motivo}"
    );
}

#[test]
fn arranjo_momento_e_otimo() {
    let a = Arranjo {
        colunas: 3,
        linhas: 1,
        celula: 10.0,
        maquinas: ["x", "y", "z"]
            .iter()
            .map(|m| MaquinaArranjo {
                id: m.to_string(),
                nome: m.to_string(),
                fixa: false,
            })
            .collect(),
        fluxos: vec![Fluxo {
            nome: "f".into(),
            volume: 5.0,
            roteiro: vec!["x".into(), "z".into()],
        }],
        inicial: atrib(&[("x", 0), ("y", 1), ("z", 2)]),
    };
    // x and z 20 m apart, 5 trips.
    assert_eq!(a.momento(&a.inicial), 100.0);
    let o = a.otimo().unwrap();
    assert_eq!(a.momento(&o), 50.0);
    let v = a.violacao(&atrib(&[("x", 0), ("y", 0), ("z", 2)])).unwrap();
    assert!(v.contains("mesma célula"), "{v}");
}

/// The search with pruning gives the same moment as trying every placement.
#[test]
fn busca_do_arranjo_igual_a_forca_bruta() {
    let ids = ["doca", "a", "b", "c", "d", "e"];
    let a = Arranjo {
        colunas: 3,
        linhas: 3,
        celula: 10.0,
        maquinas: ids
            .iter()
            .map(|m| MaquinaArranjo {
                id: m.to_string(),
                nome: m.to_string(),
                fixa: *m == "doca",
            })
            .collect(),
        fluxos: [
            (30.0, "doca a b c doca"),
            (20.0, "doca d b e doca"),
            (12.0, "doca e a doca"),
            (7.0, "doca c d e b doca"),
        ]
        .iter()
        .map(|(v, r)| Fluxo {
            nome: r.to_string(),
            volume: *v,
            roteiro: r.split(' ').map(String::from).collect(),
        })
        .collect(),
        inicial: atrib(&[
            ("doca", 3),
            ("a", 0),
            ("b", 1),
            ("c", 2),
            ("d", 4),
            ("e", 5),
        ]),
    };
    let o = a.otimo().unwrap();
    assert!(a.violacao(&o).is_none());
    let livres: Vec<u32> = (0..9).filter(|c| *c != 3).collect();
    let mut melhor = f64::INFINITY;
    for &ca in &livres {
        for &cb in &livres {
            for &cc in &livres {
                for &cd in &livres {
                    for &ce in &livres {
                        let cs = [ca, cb, cc, cd, ce];
                        let mut u = cs.to_vec();
                        u.sort();
                        u.dedup();
                        if u.len() < 5 {
                            continue;
                        }
                        let p = atrib(&[
                            ("doca", 3),
                            ("a", ca),
                            ("b", cb),
                            ("c", cc),
                            ("d", cd),
                            ("e", ce),
                        ]);
                        melhor = melhor.min(a.momento(&p));
                    }
                }
            }
        }
    }
    assert!(
        (a.momento(&o) - melhor).abs() < 1e-6,
        "{} {melhor}",
        a.momento(&o)
    );
}

/// In the small workshop the cells walk a little more than the best functional layout.
#[test]
fn celulas_andam_mais_que_o_funcional_na_oficina() {
    let p = carregar_campanha().unwrap();
    let otimo = |id: &str| {
        let a = p.missao(id).unwrap().missao.arranjo.clone().unwrap();
        let o = a.otimo().unwrap();
        a.momento(&o)
    };
    let (funcional, celular) = (otimo("ato5/oficina"), otimo("ato5/celular"));
    assert!((funcional - 6943.5).abs() < 0.5, "{funcional}");
    assert!((celular - 7128.9).abs() < 0.5, "{celular}");
}

#[test]
fn distancias_da_grade_batem_com_a_matriz_de_para() {
    let p = carregar_campanha().unwrap();
    for id in ["ato5/oficina", "ato5/celular", "ato5/armazem"] {
        let m = p.missao(id).unwrap();
        let ar = m.missao.arranjo.as_ref().unwrap();
        let d = m.missao.completar(&m.missao.referencia);
        let pos = ar.posicoes(&d);
        let modelo = modelo_da_tentativa(m, "", &d).unwrap();
        let rastro = utsim_diagram::rastro::capturar(&modelo, 0).unwrap();
        // The from-to matrix of the run, as `utsim-diagram` builds it (trips and total
        // distance per ordered pair of objects).
        let mut pares: BTreeMap<(usize, usize), (usize, f64)> = BTreeMap::new();
        for v in &rastro.viagens {
            let e = pares.entry((v.de, v.para)).or_default();
            e.0 += 1;
            e.1 += v.dist;
        }
        assert!(!pares.is_empty());
        let maquina = |i: usize| rastro.nomes[i].split("__").next().unwrap().to_string();
        let mut conferidos = 0;
        for ((de, para), (viagens, dist)) in &pares {
            let (a, b) = (maquina(*de), maquina(*para));
            let esperado = ar.distancia(pos[&a], pos[&b]);
            let medido = dist / *viagens as f64;
            assert!(
                (esperado - medido).abs() < 1e-6,
                "{id}: {a}->{b} grade {esperado}, rodada {medido}"
            );
            conferidos += 1;
        }
        // Every pair of the routes appears in the run.
        let de_para = ar.de_para(&pos);
        let na_rodada: std::collections::BTreeSet<(String, String)> = pares
            .keys()
            .map(|(de, para)| (maquina(*de), maquina(*para)))
            .collect();
        for (a, b) in de_para.keys() {
            assert!(
                na_rodada.contains(&(a.clone(), b.clone())),
                "{id}: {a}->{b} não apareceu"
            );
        }
        assert!(conferidos >= de_para.len());
    }
}

/// `pequeno` with two models, mesa (weight 1) and coluna (weight 3); `tempo` is the
/// weighted average of the model times.
fn com_mix() -> Balanceamento {
    let mut b = pequeno();
    b.mix = vec![
        ModeloMix {
            id: "mesa".into(),
            nome: "Mesa".into(),
            parcela: 1.0,
        },
        ModeloMix {
            id: "coluna".into(),
            nome: "Coluna".into(),
            parcela: 3.0,
        },
    ];
    for (t, (m, c)) in b
        .tarefas
        .iter_mut()
        .zip([(8.0, 2.0), (3.0, 3.0), (1.0, 6.0), (2.0, 2.0)])
    {
        t.tempos = [("mesa".to_string(), m), ("coluna".to_string(), c)].into();
        t.tempo = (m + 3.0 * c) / 4.0;
    }
    b
}

#[test]
fn mix_cargas_por_modelo_receita_e_pico() {
    let b = com_mix();
    assert!(b.validar().is_empty(), "{:?}", b.validar());
    let a = atrib(&[("a", 1), ("b", 1), ("c", 2), ("d", 2)]);
    // Mesa: 8 + 3 and 1 + 2; coluna: 2 + 3 and 6 + 2.
    assert_eq!(
        b.cargas_modelos(&a),
        vec![vec![11.0, 3.0, 0.0, 0.0], vec![5.0, 8.0, 0.0, 0.0]]
    );
    // The average load is what the takt is checked against: (11 + 3 × 5) / 4 and (3 + 3 × 8) / 4.
    assert_eq!(b.resumo(&a).cargas, vec![6.5, 6.75, 0.0, 0.0]);
    assert_eq!(b.violacao(&a), None);
    assert_eq!(b.pico(&a), 11.0);
    let aj = b.ajustes(&a);
    assert_eq!(aj["est1.process"], "Const(11.0)");
    assert_eq!(
        aj["est1.recipe"],
        "Some((label: \"modelo\", times: [(value: 1.0, time: Const(11.0)), (value: 2.0, time: Const(5.0))]))"
    );
    assert_eq!(
        aj["est4.recipe"],
        "Some((label: \"modelo\", times: [(value: 1.0, time: Const(0.0)), (value: 2.0, time: Const(0.0))]))"
    );
    // Without a mix the pico is the cycle and there is no recipe.
    let p = pequeno();
    assert_eq!(p.pico(&a), p.resumo(&a).ciclo);
    assert!(!p.ajustes(&a).keys().any(|k| k.ends_with(".recipe")));
}

#[test]
fn mix_invalido_recusado() {
    let mut b = com_mix();
    b.tarefas[0].tempo = 5.0;
    b.tarefas[1].tempos.remove("coluna");
    b.tarefas[2].tempos.insert("piso".into(), 1.0);
    b.mix.push(ModeloMix {
        id: "mesa".into(),
        nome: "Outra".into(),
        parcela: 0.0,
    });
    let e = b.validar().join("\n");
    for trecho in [
        "o tempo de 'a' (5.0) precisa ser a média ponderada pelo mix",
        "'b' sem tempo do modelo 'coluna'",
        "'c' tem tempo do modelo 'piso'",
        "modelo repetido no mix",
        "parcela do mix precisa ser positiva",
    ] {
        assert!(e.contains(trecho), "falta \"{trecho}\" em:\n{e}");
    }
    let mut p = pequeno();
    p.tarefas[0].tempos.insert("mesa".into(), 4.0);
    assert!(
        p.validar()
            .join("\n")
            .contains("tem tempos por modelo, mas não há mix")
    );
}

#[test]
fn mix_do_ventilador_no_modelo() {
    let p = carregar_campanha().unwrap_or_else(|e| panic!("{}", e.join("\n")));
    let m = p
        .missoes
        .iter()
        .find(|m| m.missao.id == "ato4/mix")
        .unwrap();
    let b = m.missao.balanceamento.as_ref().unwrap();
    // No assignment in 8 stations keeps both models inside the takt of 60 s.
    let r = m.missao.completar(&m.missao.referencia);
    assert_eq!(b.pico(&b.atribuicao(&r)), 65.0);
    assert_eq!(b.resumo(&b.atribuicao(&r)).estacoes_usadas, 8);
    let modelo = modelo_da_tentativa(m, "", &r).unwrap();
    let est1 = modelo.objects.iter().find(|o| o.name == "est1").unwrap();
    let txt = format!("{:?}", est1.kind);
    assert!(txt.contains("recipe: Some"), "{txt}");
}
