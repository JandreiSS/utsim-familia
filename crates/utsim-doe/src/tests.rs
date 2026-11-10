use super::*;

fn dot(a: &[i8], b: &[i8]) -> i32 {
    a.iter()
        .zip(b)
        .map(|(x, y)| (*x as i32) * (*y as i32))
        .sum()
}

#[test]
fn plackett_burman_e_ortogonal_e_equilibrado() {
    for n in PB_RUNS {
        let m = pb_matrix(n).unwrap();
        assert_eq!(m.len(), n);
        let cols: Vec<Vec<i8>> = (0..n - 1)
            .map(|j| m.iter().map(|r| r[j]).collect())
            .collect();
        for (a, ca) in cols.iter().enumerate() {
            assert_eq!(
                ca.iter().map(|v| *v as i32).sum::<i32>(),
                0,
                "n={n} col {a}"
            );
            for cb in &cols[a + 1..] {
                assert_eq!(dot(ca, cb), 0, "n={n}");
            }
        }
    }
    assert_eq!(pb_runs(7), Some(8));
    assert_eq!(pb_runs(11), Some(12));
    assert_eq!(pb_runs(23), Some(24));
    assert_eq!(pb_runs(24), None);
    let d = plackett_burman(5, None, 2, 7).unwrap();
    assert_eq!(d.points.len(), 10);
    assert_eq!(d.points[0].coded.len(), 5);
    assert!(plackett_burman(12, Some(12), 0, 1).is_err());
}

#[test]
fn ordem_de_execucao_e_permutacao_por_bloco() {
    let d = central_composite(3, Alpha::Rotatable, (4, 2), 2, 42).unwrap();
    let mut ordens: Vec<usize> = d.points.iter().map(|p| p.run_order).collect();
    ordens.sort();
    assert_eq!(ordens, (1..=d.points.len()).collect::<Vec<_>>());
    // O bloco 1 (8 vértices e 4 centros) corre antes do bloco 2.
    let max1 = d
        .points
        .iter()
        .filter(|p| p.block == 1)
        .map(|p| p.run_order)
        .max();
    assert_eq!(max1, Some(12));
    // Mesma semente, mesma ordem; outra semente, outra.
    let e = central_composite(3, Alpha::Rotatable, (4, 2), 2, 42).unwrap();
    assert_eq!(d, e);
    let f = central_composite(3, Alpha::Rotatable, (4, 2), 2, 43).unwrap();
    assert_ne!(d.run_sequence(), f.run_sequence());
}

#[test]
fn composto_central_alfas() {
    let d = central_composite(2, Alpha::Rotatable, (5, 0), 1, 1).unwrap();
    assert!((d.alpha.unwrap() - 2f64.sqrt()).abs() < 1e-12);
    assert_eq!(d.points.len(), 4 + 4 + 5);
    assert_eq!(d.blocks, 1);
    let d = central_composite(3, Alpha::Rotatable, (6, 0), 1, 1).unwrap();
    assert!((d.alpha.unwrap() - 8f64.powf(0.25)).abs() < 1e-12);
    // Blocos ortogonais com k = 2 e três centros em cada bloco: α = √2.
    let d = central_composite(2, Alpha::OrthogonalBlocks, (3, 3), 2, 1).unwrap();
    assert!((d.alpha.unwrap() - 2f64.sqrt()).abs() < 1e-12);
    assert_eq!(d.blocks, 2);
    assert!(central_composite(2, Alpha::OrthogonalBlocks, (3, 3), 1, 1).is_err());
    assert!(central_composite(7, Alpha::FaceCentered, (1, 0), 1, 1).is_err());
    let d = central_composite(3, Alpha::FaceCentered, (1, 0), 1, 1).unwrap();
    assert!(
        d.points
            .iter()
            .flat_map(|p| &p.coded)
            .all(|v| v.abs() <= 1.0)
    );
}

#[test]
fn box_behnken_tamanhos() {
    for (k, n) in [(3, 12), (4, 24), (5, 40)] {
        let d = box_behnken(k, 3, 0).unwrap();
        assert_eq!(d.points.len(), n + 3);
        // Nenhuma corrida num vértice do cubo.
        assert!(
            d.points
                .iter()
                .all(|p| p.coded.iter().filter(|v| v.abs() > 0.5).count() <= 2)
        );
    }
    assert!(box_behnken(2, 3, 0).is_err());
}

/// Uma superfície conhecida com máximo em (0,5; −0,25): y = 80 − 2(A − 0,5)² − 3(B + 0,25)²
/// + AB·0 + 0,5·AB, ajustada sem ruído num composto central.
#[test]
fn quadratico_acha_o_maximo() {
    let d = central_composite(2, Alpha::Rotatable, (5, 0), 1, 3).unwrap();
    let f = |x: &[f64]| {
        80.0 - 2.0 * (x[0] - 0.5).powi(2) - 3.0 * (x[1] + 0.25).powi(2) + 0.5 * x[0] * x[1]
    };
    let x = d.matrix();
    // Um ruído pequeno e fixo, para a regressão ter erro.
    let y: Vec<f64> = x
        .iter()
        .enumerate()
        .map(|(i, r)| f(r) + 0.001 * ((i % 3) as f64 - 1.0))
        .collect();
    let m = Model::fit(&quadratic_terms(2), &x, &y, 0.95).unwrap();
    assert_eq!(m.regression.coefs.len(), 6);
    assert_eq!(m.regression.coefs[5].name, "B²");
    let s = m.stationary().unwrap();
    assert_eq!(s.nature, Nature::Maximum);
    // O máximo exato de f: ∇f = 0.
    let a = [[-4.0, 0.5], [0.5, -6.0]];
    let b = [-2.0, 1.5];
    let det = a[0][0] * a[1][1] - a[0][1] * a[1][0];
    let xs = [
        (b[0] * a[1][1] - a[0][1] * b[1]) / det,
        (a[0][0] * b[1] - b[0] * a[1][0]) / det,
    ];
    assert!((s.coded[0] - xs[0]).abs() < 1e-2, "{:?} {:?}", s.coded, xs);
    assert!((s.coded[1] - xs[1]).abs() < 1e-2, "{:?} {:?}", s.coded, xs);
    assert!(s.eigenvalues.iter().all(|v| *v < 0.0));
    assert!(m.regression.r2 > 0.999);
}

#[test]
fn sela_e_linear_sem_estacionario() {
    let d = box_behnken(3, 3, 0).unwrap();
    let x = d.matrix();
    let y: Vec<f64> = x
        .iter()
        .enumerate()
        .map(|(i, r)| 10.0 + r[0] * r[0] - r[1] * r[1] + 0.5 * r[2] * r[2] + 0.01 * (i % 2) as f64)
        .collect();
    let m = Model::fit(&quadratic_terms(3), &x, &y, 0.95).unwrap();
    assert_eq!(m.stationary().unwrap().nature, Nature::Saddle);
    let lin = Model::fit(&linear_terms(3), &x, &y, 0.95).unwrap();
    assert!(lin.stationary().is_none());
}

#[test]
fn autovalores_de_jacobi() {
    let a = vec![
        vec![2.0, 1.0, 0.0],
        vec![1.0, 2.0, 0.0],
        vec![0.0, 0.0, -3.0],
    ];
    let v = eigen_symmetric(&a);
    for (x, e) in v.iter().zip([-3.0, 1.0, 3.0]) {
        assert!((x - e).abs() < 1e-10, "{v:?}");
    }
}

#[test]
fn desejabilidade() {
    let max = Desirability::new(Goal::Maximize {
        low: 10.0,
        high: 20.0,
    });
    assert_eq!(max.value(5.0), 0.0);
    assert_eq!(max.value(15.0), 0.5);
    assert_eq!(max.value(25.0), 1.0);
    let min = Desirability::new(Goal::Minimize {
        low: 10.0,
        high: 20.0,
    });
    assert_eq!(min.value(12.5), 0.75);
    let mut alvo = Desirability::new(Goal::Target {
        low: 0.0,
        target: 10.0,
        high: 30.0,
    });
    alvo.weight_upper = 2.0;
    assert_eq!(alvo.value(5.0), 0.5);
    assert_eq!(alvo.value(10.0), 1.0);
    assert!((alvo.value(20.0) - 0.25).abs() < 1e-12);
    assert_eq!(alvo.value(f64::NAN), 0.0);
    assert!((overall(&[0.25, 1.0], &[1.0, 1.0]) - 0.5).abs() < 1e-12);
    assert_eq!(overall(&[0.0, 1.0], &[1.0, 1.0]), 0.0);
    // Importância 3 para a primeira: 0,25^(3/4).
    assert!((overall(&[0.25, 1.0], &[3.0, 1.0]) - 0.25f64.powf(0.75)).abs() < 1e-12);
    assert!(
        Desirability::new(Goal::Maximize {
            low: 2.0,
            high: 1.0
        })
        .check()
        .is_err()
    );
}

#[test]
fn otimiza_duas_respostas() {
    let d = central_composite(2, Alpha::FaceCentered, (3, 0), 1, 5).unwrap();
    let x = d.matrix();
    let ruido = |i: usize| 0.001 * ((i % 3) as f64 - 1.0);
    // Rendimento cresce com A; custo cresce com B. O melhor: A alto, B baixo.
    let y1: Vec<f64> = x
        .iter()
        .enumerate()
        .map(|(i, r)| 50.0 + 10.0 * r[0] + ruido(i))
        .collect();
    let y2: Vec<f64> = x
        .iter()
        .enumerate()
        .map(|(i, r)| 100.0 + 20.0 * r[1] + ruido(i + 1))
        .collect();
    let m1 = Model::fit(&linear_terms(2), &x, &y1, 0.95).unwrap();
    let m2 = Model::fit(&linear_terms(2), &x, &y2, 0.95).unwrap();
    let ds = [
        Desirability::new(Goal::Maximize {
            low: 40.0,
            high: 60.0,
        }),
        Desirability::new(Goal::Minimize {
            low: 80.0,
            high: 120.0,
        }),
    ];
    let o = optimize(&[m1, m2], &ds, Region::Cube(1.0)).unwrap();
    assert!((o.coded[0] - 1.0).abs() < 1e-6, "{:?}", o.coded);
    assert!((o.coded[1] + 1.0).abs() < 1e-6, "{:?}", o.coded);
    assert!(o.overall > 0.99, "{}", o.overall);
}

#[test]
fn esfera_limita_a_busca() {
    let d = central_composite(2, Alpha::Rotatable, (5, 0), 1, 1).unwrap();
    let x = d.matrix();
    let y: Vec<f64> = x
        .iter()
        .enumerate()
        .map(|(i, r)| r[0] + r[1] + 0.001 * (i % 2) as f64)
        .collect();
    let m = Model::fit(&linear_terms(2), &x, &y, 0.95).unwrap();
    let ds = [Desirability::new(Goal::Maximize {
        low: -5.0,
        high: 5.0,
    })];
    let r = d.radius();
    let o = optimize(&[m], &ds, Region::Sphere(r)).unwrap();
    let n = o.coded.iter().map(|v| v * v).sum::<f64>().sqrt();
    assert!(n <= r + 1e-9);
    assert!((o.coded[0] - o.coded[1]).abs() < 1e-3, "{:?}", o.coded);
    assert!(n > r - 1e-3);
}

#[test]
fn fator_converte_unidades() {
    let f = Factor::new("temperatura", 150.0, 200.0);
    assert_eq!(f.natural(-1.0), 150.0);
    assert_eq!(f.natural(0.0), 175.0);
    assert_eq!(f.coded(200.0), 1.0);
    assert!((f.natural(2f64.sqrt()) - (175.0 + 25.0 * 2f64.sqrt())).abs() < 1e-12);
}

#[test]
fn desejabilidade_fora_dos_limites_com_peso() {
    for w in [2.0, 0.5] {
        let mut max = Desirability::new(Goal::Maximize {
            low: 10.0,
            high: 20.0,
        });
        max.weight = w;
        assert_eq!(max.value(5.0), 0.0);
        assert_eq!(max.value(25.0), 1.0);
        let mut alvo = Desirability::new(Goal::Target {
            low: 0.0,
            target: 10.0,
            high: 30.0,
        });
        alvo.weight = w;
        alvo.weight_upper = w;
        assert_eq!(alvo.value(-5.0), 0.0);
        assert_eq!(alvo.value(40.0), 0.0);
    }
    assert_eq!(overall(&[f64::NAN, 1.0], &[1.0, 1.0]), 0.0);
}

#[test]
fn otimiza_muitos_fatores_sem_grade() {
    // Doze fatores num Plackett-Burman de 16 corridas com centros: a busca usa amostras.
    let d = plackett_burman(12, Some(16), 2, 9).unwrap();
    let x = d.matrix();
    let y: Vec<f64> = x
        .iter()
        .enumerate()
        .map(|(i, r)| 10.0 + 2.0 * r[0] - r[1] + 0.01 * (i % 3) as f64)
        .collect();
    let m = Model::fit(&linear_terms(12), &x, &y, 0.95).unwrap();
    let ds = [Desirability::new(Goal::Maximize {
        low: 5.0,
        high: 13.0,
    })];
    let o = optimize(&[m], &ds, Region::Cube(1.0)).unwrap();
    assert!(o.overall > 0.99, "{}", o.overall);
    assert!(o.coded[0] > 0.9 && o.coded[1] < -0.9, "{:?}", o.coded);
}

#[test]
fn sem_ponto_aceitavel_vai_para_o_mais_perto() {
    let d = central_composite(2, Alpha::FaceCentered, (3, 0), 1, 5).unwrap();
    let x = d.matrix();
    let y: Vec<f64> = x
        .iter()
        .enumerate()
        .map(|(i, r)| 50.0 + 10.0 * r[0] + 0.001 * (i % 3) as f64)
        .collect();
    let m = Model::fit(&linear_terms(2), &x, &y, 0.95).unwrap();
    // Pede de 100 a 200: inatingível; o melhor é ir para A alto.
    let ds = [Desirability::new(Goal::Maximize {
        low: 100.0,
        high: 200.0,
    })];
    let o = optimize(&[m], &ds, Region::Cube(1.0)).unwrap();
    assert_eq!(o.overall, 0.0);
    assert!(o.coded[0] > 0.99, "{:?}", o.coded);
}

#[test]
fn autovalores_pequenos_ainda_rodam() {
    let a = vec![vec![1e-13, 2e-13], vec![2e-13, 1e-13]];
    let v = eigen_symmetric(&a);
    assert!(
        (v[0] + 1e-13).abs() < 1e-20 && (v[1] - 3e-13).abs() < 1e-20,
        "{v:?}"
    );
}

#[test]
fn fator_confere_niveis() {
    assert!(Factor::new("a", 1.0, 1.0).check().is_err());
    assert!(Factor::new("a", 1.0, 2.0).check().is_ok());
}
