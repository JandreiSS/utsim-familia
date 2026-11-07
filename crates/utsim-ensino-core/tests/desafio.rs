//! Challenges and teams (EN10): two statistically equal solutions appear tied, a clearly
//! better one stands alone, and the paired team comparison agrees.

use utsim_ensino_core::campanha::carregar_campanha;
use utsim_ensino_core::desafio::{Entrada, comparar_equipes, empatados, ler_equipes, placar};
use utsim_ensino_core::missao::Decisoes;
use utsim_ensino_core::motor::simular;
use utsim_ensino_core::nota::{Resultado, Tentativa, corrigir};
use utsim_ensino_core::pacote::PacoteCarregado;

fn resultado(p: &PacoteCarregado, id: &str, aluno: &str, d: &Decisoes) -> Resultado {
    let m = p.missao(id).unwrap();
    let t = Tentativa {
        aluno: aluno.into(),
        decisoes: m.missao.completar(d),
        ..Default::default()
    };
    let sim = simular(m, aluno, &t.decisoes, &|_| true).unwrap();
    corrigir(&m.missao, &p.pacote.pesos, &sim, &t).unwrap()
}

#[test]
fn solucoes_iguais_empatam_no_placar() {
    let p = carregar_campanha().unwrap();
    let id = "ato1/furadeira";
    let m = p.missao(id).unwrap();
    let referencia = m.missao.referencia.clone();
    let inicial = m.missao.decisoes_iniciais();
    // ana and beto found the reference (each in their own variant), caio kept the start.
    let ana = resultado(&p, id, "ra001", &referencia);
    let beto = resultado(&p, id, "ra002", &referencia);
    let caio = resultado(&p, id, "ra003", &inicial);
    assert!(
        empatados(&ana, &beto),
        "{:?} {:?}",
        ana.metricas,
        beto.metricas
    );
    assert!(!empatados(&ana, &caio));
    let linhas = placar(vec![
        Entrada {
            apelido: "caio".into(),
            resultado: caio,
        },
        Entrada {
            apelido: "beto".into(),
            resultado: beto,
        },
        Entrada {
            apelido: "ana".into(),
            resultado: ana,
        },
    ]);
    let pos: Vec<(usize, bool)> = linhas.iter().map(|l| (l.posicao, l.empate)).collect();
    assert_eq!(pos, [(1, true), (1, true), (3, false)], "{linhas:#?}");
    assert_eq!(linhas[2].apelido, "caio");
}

#[test]
fn equipes_comparadas_por_diferenca_pareada() {
    let p = carregar_campanha().unwrap();
    let m = p.missao("ato1/furadeira").unwrap();
    let r = m.missao.completar(&m.missao.referencia);
    let i = m.missao.completar(&m.missao.decisoes_iniciais());
    let c = comparar_equipes(
        m,
        &[
            ("Azul".into(), r.clone()),
            ("Verde".into(), i),
            ("Rosa".into(), r),
        ],
        &|_| true,
    )
    .unwrap();
    assert_eq!(c.equipes, ["Azul", "Verde", "Rosa"]);
    // The same decisions on the same random numbers: a difference of exactly zero.
    assert_eq!(c.diferencas[0][2].media, 0.0);
    assert_eq!(c.diferencas[0][2].meia_largura, 0.0);
    assert_eq!(c.posicoes, [1, 3, 1], "{c:#?}");
}

#[test]
fn equipes_em_texto() {
    let e = ler_equipes("Azul: 2027001, 2027002\n\nVerde: 2027003; 2027004 ").unwrap();
    assert_eq!(e.len(), 2);
    assert_eq!(e[1].membros, ["2027003", "2027004"]);
    assert!(ler_equipes("sem dois pontos").is_err());
    assert!(ler_equipes("Azul:").is_err());
}
