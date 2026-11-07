//! The pieces of acts 4 and 5: every assignment that breaks the precedence or the takt is
//! refused with the reason, the indicators by hand on small cases, the exact searches,
//! and the distances of the layout grid equal to the from-to matrix of the run
//! (the trips of `utsim-diagram::rastro`).

use std::collections::BTreeMap;

use utsim_ensino_core::campanha::carregar_campanha;
use utsim_ensino_core::motor::modelo_da_tentativa;
use utsim_ensino_core::nota::portao;
use utsim_ensino_core::pecas::{Arranjo, Balanceamento, Fluxo, MaquinaArranjo, Tarefa};

fn pequeno() -> Balanceamento {
    let t = |id: &str, tempo: f64, dep: &[&str]| Tarefa {
        id: id.into(),
        nome: id.into(),
        tempo,
        depois_de: dep.iter().map(|s| s.to_string()).collect(),
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

#[test]
fn distancias_da_grade_batem_com_a_matriz_de_para() {
    let p = carregar_campanha().unwrap();
    for id in ["ato5/oficina", "ato5/armazem"] {
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
