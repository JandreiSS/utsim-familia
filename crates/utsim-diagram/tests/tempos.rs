//! Time and method study on the example of one operator with three machines
//! (`examples/homem_maquina.ron`) and its improved method (`homem_maquina_melhorado.ron`).

use std::path::PathBuf;

use utsim_diagram::rastro;
use utsim_diagram::tempos::{self, Estudo, Opcoes, Tipo};
use utsim_model::Model;

fn example(name: &str) -> Model {
    Model::load(
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../examples")
            .join(name),
    )
    .unwrap()
}

fn study(model: &Model, opcoes: &Opcoes) -> Estudo {
    let r = rastro::capturar(model, 0).unwrap();
    tempos::estudar(model, &r, opcoes).unwrap()
}

fn element(e: &Estudo, ator: &str, elemento: &str) -> f64 {
    e.grupos[0]
        .elementos
        .iter()
        .find(|l| l.ator == ator && l.elemento == elemento)
        .unwrap_or_else(|| panic!("sem {ator}/{elemento}"))
        .resumo
        .media
}

#[test]
fn cycles_elements_and_man_machine_figures() {
    let m = example("homem_maquina.ron");
    let e = study(&m, &Opcoes::default());
    assert_eq!(e.grupos.len(), 1);
    let g = &e.grupos[0];
    assert_eq!(g.operador.as_deref(), Some("ana"));
    assert_eq!(g.maquinas, ["maq1", "maq2", "maq3"]);
    assert_eq!(g.ancora, "carga/descarga maq1");
    // About 6600 s of window / 97 s per cycle.
    assert!(g.ciclos.len() >= 60, "{} ciclos", g.ciclos.len());
    assert!(g.regulares() * 10 >= g.ciclos.len() * 9);
    let c = g.tempo_ciclo.unwrap();
    // a + t + walking ≈ 20.7 + 70 + 6.
    assert!((94.0..100.0).contains(&c.media), "ciclo {}", c.media);
    assert!(c.min <= c.media && c.media <= c.max && c.dp > 0.0);
    // Triangular(18, 20, 24) has mean 20.67; the machines run 70 s; walking 6 m at 1 m/s.
    let carga = element(&e, "ana", "carga/descarga maq1");
    assert!((19.5..22.0).contains(&carga), "{carga}");
    assert!((element(&e, "maq2", "automático") - 70.0).abs() < 1e-9);
    assert!((element(&e, "ana", "andar até maq2") - 6.0).abs() < 1e-9);
    let h = g.homem_maquina.as_ref().unwrap();
    assert!((19.5..22.0).contains(&h.abt.a));
    assert!((5.0..7.0).contains(&h.abt.b));
    assert!((h.abt.t - 70.0).abs() < 0.5);
    assert!((3.2..3.6).contains(&h.n_linha), "n' {}", h.n_linha);
    assert_eq!(
        h.arranjos.iter().map(|a| a.maquinas).collect::<Vec<_>>(),
        [3, 4]
    );
    // Actors: the machines wait for the operator (walking), the operator waits for them.
    let ana = g.atores.iter().find(|a| a.ator == "ana").unwrap();
    assert!((0.75..0.9).contains(&ana.utilizacao), "{}", ana.utilizacao);
    assert!(ana.interferencia > 10.0 && ana.interferencia <= ana.ocioso + 1e-9);
    let maq1 = g.atores.iter().find(|a| a.ator == "maq1").unwrap();
    assert!((5.0..8.0).contains(&maq1.interferencia));
    assert!((h.espera_maquina - maq1.interferencia).abs() < 1.0);
    // Without rating or allowance the normal cycle is the observed one.
    let p = g.padrao.as_ref().unwrap();
    assert!((p.unidades_por_ciclo - 3.0).abs() < 0.05);
    assert!((p.ciclo_normal - p.ciclo_observado).abs() / p.ciclo_observado < 0.01);
    assert_eq!(p.ciclo_padrao, p.ciclo_normal);
    // Representative cycle: every actor covers the whole cycle.
    let rep = &g.ciclos[g.representativo.unwrap()];
    for col in &g.grafico {
        let total: f64 = col.barras.iter().map(|b| b.t1 - b.t0).sum();
        assert!((total - rep.duracao).abs() < 1e-6, "{}", col.ator);
    }
    let svg = tempos::svg(g, "s", true).unwrap();
    assert!(svg.contains("carga/descarga") && svg.contains("ciclo n.º"));
    let html = tempos::html(&e, None, "teste");
    assert!(html.contains("n' = (a + t)/(a + b)") && html.contains("Tempo padrão"));
    let csv = tempos::csv_elementos(&e);
    assert!(csv.starts_with("grupo,ator,elemento,tipo,media"));
    assert!(csv.contains("automatico"));
}

#[test]
fn rating_allowance_and_costs() {
    let m = example("homem_maquina.ron");
    let e = study(
        &m,
        &Opcoes {
            ritmo: 1.1,
            tolerancia: Some(0.15),
            custos: Some((30.0, 50.0)),
            ..Opcoes::default()
        },
    );
    let g = &e.grupos[0];
    let l = g
        .elementos
        .iter()
        .find(|l| l.elemento == "carga/descarga maq1")
        .unwrap();
    assert!((l.normal.unwrap() - l.resumo.media * 1.1).abs() < 1e-9);
    assert!((l.padrao.unwrap() - l.resumo.media * 1.1 * 1.15).abs() < 1e-9);
    let auto = g
        .elementos
        .iter()
        .find(|l| l.tipo == Tipo::Automatico)
        .unwrap();
    assert_eq!(
        auto.padrao,
        Some(auto.resumo.media),
        "machine time is not rated"
    );
    let ocioso = g.elementos.iter().find(|l| l.tipo == Tipo::Ocioso).unwrap();
    assert_eq!(ocioso.padrao, None);
    let p = g.padrao.as_ref().unwrap();
    assert!(p.ciclo_padrao > p.ciclo_normal && p.ciclo_normal > p.ciclo_observado);
    assert!(p.por_unidade_padrao.unwrap() > p.por_unidade_observado.unwrap());
    let h = g.homem_maquina.as_ref().unwrap();
    assert!(h.arranjos.iter().all(|a| a.custo_por_unidade.is_some()));
    // Machines are dearer than the operator's idle time: n = ⌊n'⌋.
    assert_eq!(h.otimo.as_ref().unwrap().maquinas, 3);
}

#[test]
fn the_allowance_of_the_person_profile_is_reused() {
    let mut m = example("homem_maquina.ron");
    let ana = m.objects.iter_mut().find(|o| o.name == "ana").unwrap();
    ana.person = Some(Box::new(utsim_core::Person {
        fatigue: Some(utsim_core::Fatigue::Allowance(0.1)),
        ..utsim_core::Person::default()
    }));
    let e = study(&m, &Opcoes::default());
    let g = &e.grupos[0];
    let p = g.padrao.as_ref().unwrap();
    assert_eq!(p.tolerancia, 0.1);
    assert_eq!(p.tolerancia_no_modelo, 0.1);
    assert!(p.origem_tolerancia.contains("ana"));
    // The simulated loading already has the 10 %: normal = observed / 1.1, standard back
    // to the observed; walking is not lengthened by the allowance.
    let l = g
        .elementos
        .iter()
        .find(|l| l.elemento == "carga/descarga maq1")
        .unwrap();
    assert!((21.0..25.0).contains(&l.resumo.media), "{}", l.resumo.media);
    assert!((l.normal.unwrap() - l.resumo.media / 1.1).abs() < 1e-9);
    assert!((l.padrao.unwrap() - l.resumo.media).abs() < 1e-9);
    let w = g
        .elementos
        .iter()
        .find(|l| l.elemento == "andar até maq2")
        .unwrap();
    assert!((w.normal.unwrap() - w.resumo.media).abs() < 1e-9);
}

#[test]
fn before_and_after_comparison() {
    let antes = study(&example("homem_maquina.ron"), &Opcoes::default());
    let depois = study(&example("homem_maquina_melhorado.ron"), &Opcoes::default());
    let c = tempos::comparar(&antes, &depois).unwrap();
    let carga = c
        .elementos
        .iter()
        .find(|l| l.ator == "ana" && l.elemento == "carga/descarga maq1")
        .unwrap();
    // Triangular(12, 14, 16) against Triangular(18, 20, 24): about a third less.
    let v = carga.variacao().unwrap();
    assert!((-0.4..-0.25).contains(&v), "{v}");
    let ciclo = &c.indicadores[0];
    assert!(ciclo.diferenca().unwrap() < -5.0);
    let auto = c
        .elementos
        .iter()
        .find(|l| l.ator == "maq1" && l.elemento == "automático")
        .unwrap();
    assert!(auto.diferenca().unwrap().abs() < 1e-9);
    let n = c
        .indicadores
        .iter()
        .find(|l| l.elemento.starts_with("n'"))
        .unwrap();
    assert!(n.depois.unwrap() > 4.5);
    let texto = tempos::texto(&antes, Some(&c));
    assert!(texto.contains("Comparação") && texto.contains("p.p."));
    assert!(tempos::csv_comparacao(&c).contains("carga/descarga maq1"));
}

#[test]
fn chosen_cycle_window_and_errors() {
    let m = example("homem_maquina.ron");
    let e = study(
        &m,
        &Opcoes {
            janela: Some((1000.0, 2000.0)),
            ciclo: Some(2),
            ..Opcoes::default()
        },
    );
    let g = &e.grupos[0];
    assert_eq!(g.representativo, Some(1));
    assert!(
        g.ciclos
            .iter()
            .all(|c| c.inicio >= 1000.0 && c.fim <= 2000.0)
    );
    assert!((8..=11).contains(&g.ciclos.len()));
    let r = rastro::capturar(&m, 0).unwrap();
    let erro = |o: Opcoes| tempos::estudar(&m, &r, &o).unwrap_err();
    assert!(
        erro(Opcoes {
            ciclo: Some(999),
            ..Opcoes::default()
        })
        .contains("--ciclo")
    );
    assert!(
        erro(Opcoes {
            ator: Some("ninguem".into()),
            ..Opcoes::default()
        })
        .contains("ninguem")
    );
    assert!(
        erro(Opcoes {
            janela: Some((10.0, 5.0)),
            ..Opcoes::default()
        })
        .contains("intervalo")
    );
    // Naming a machine studies the operator that tends it.
    let por_maquina = study(
        &m,
        &Opcoes {
            ator: Some("maq2".into()),
            ..Opcoes::default()
        },
    );
    assert_eq!(por_maquina.grupos[0].operador.as_deref(), Some("ana"));
}

/// Every example either gives a study or a clear error, and machines without operators
/// are studied alone.
#[test]
fn every_example_runs() {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../examples");
    let mut ok = 0;
    for entry in std::fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        let name = path.file_name().unwrap().to_string_lossy().to_string();
        if path.extension().is_none_or(|e| e != "ron")
            || name.starts_with("experimento_")
            || name.starts_with("otimizacao_")
        {
            continue;
        }
        let mut m = Model::load(&path).unwrap();
        if m.run.default_decision.is_none() && name.contains("_rl") {
            continue;
        }
        m.run.stop_time = m.run.stop_time.min(3000.0);
        m.run.warmup = m.run.warmup.min(300.0);
        let Ok(r) = rastro::capturar(&m, 0) else {
            continue;
        };
        if let Ok(e) = tempos::estudar(&m, &r, &Opcoes::default()) {
            for g in &e.grupos {
                assert!(!g.ciclos.is_empty(), "{name}");
                let _ = tempos::html(&e, None, "x");
            }
            ok += 1;
        }
    }
    assert!(ok >= 10, "só {ok} exemplos com estudo");
}
