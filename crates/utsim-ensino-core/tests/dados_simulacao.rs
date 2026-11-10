//! Data from a simulation run (`docs/plano_ensino_e_jogo.md`, 6.3): the mission's own
//! model with the process time of one machine changed, run with the student's seed. The
//! records are reproducible, the planted machine is recovered by the questions, and the
//! set goes through the editor, the package and the grading.

use std::path::PathBuf;

use utsim_core::Dist;
use utsim_ensino_core::dados::{Gerador, erros_da_simulacao, estacoes_do_modelo};
use utsim_ensino_core::editor::{Edicao, PacoteProfessor};
use utsim_ensino_core::editor_dados::TipoGerador;
use utsim_ensino_core::missao::Previsao;
use utsim_ensino_core::questoes::{ConjuntoMissao, QuestaoDados};
use utsim_model::Model;

fn modelo_ron() -> String {
    let raiz = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    std::fs::read_to_string(raiz.join("examples/tutorial_02_linha.ron")).unwrap()
}

fn modelo() -> Model {
    Model::parse(&modelo_ron()).unwrap()
}

fn conjunto(processo: Dist) -> ConjuntoMissao {
    ConjuntoMissao {
        id: "registros".into(),
        nome: "Registros da linha".into(),
        gerador: Gerador::Simulacao {
            estacoes: vec!["corte".into(), "furacao".into(), "pintura".into()],
            afetado: 1,
            processo,
            duracao: 960.0,
        },
        sortear_afetado: true,
        ferramenta: Some("estratificacao".into()),
        lie: None,
        lse: None,
    }
}

#[test]
fn registros_reprodutiveis_e_coerentes() {
    let m = modelo();
    assert_eq!(estacoes_do_modelo(&m), ["corte", "furacao", "pintura"]);
    let cj = conjunto(Dist::Exp { mean: 9.0 });
    let a = cj.gerar_com("m", "ra1", &m);
    assert_eq!(a, cj.gerar_com("m", "ra1", &m));
    assert_ne!(a.valores, cj.gerar_com("m", "ra2", &m).valores);
    assert!(a.csv().starts_with("estacao;peca;entrada;saida;tempo\n"));
    assert!(a.linhas.len() > 100, "{}", a.linhas.len());
    for (l, v) in a.linhas.iter().zip(&a.valores) {
        let num = |s: &str| s.replace(',', ".").parse::<f64>().unwrap();
        let (t0, t1, t) = (num(&l[2]), num(&l[3]), num(&l[4]));
        assert!(t0 >= 0.0 && t1 <= 960.0 && t >= 0.0);
        assert!((t1 - t0 - t).abs() < 1e-9, "{l:?}");
        assert_eq!(v[1], t);
        assert_eq!(l[0], ["corte", "furacao", "pintura"][v[0] as usize]);
    }
    // Every machine has records.
    for g in 0..3 {
        assert!(a.valores.iter().any(|v| v[0] as usize == g));
    }
    // With no model the set is empty and says why.
    let vazio = cj.gerar("m", "ra1");
    assert!(vazio.linhas.is_empty());
    assert!(vazio.resposta.contains("modelo"));
}

#[test]
fn a_maquina_plantada_e_recuperada() {
    let m = modelo();
    let cj = conjunto(Dist::Exp { mean: 9.0 });
    let mut vistas = std::collections::BTreeSet::new();
    for k in 0..20 {
        let aluno = format!("2027{k:03}");
        let r = cj.receita("ato8/linha", &aluno);
        let Gerador::Simulacao { afetado, .. } = r.gerador else {
            unreachable!()
        };
        vistas.insert(afetado);
        let c = r.gerar_com(Some(&m));
        for q in [
            QuestaoDados::GrupoMaiorMedia,
            QuestaoDados::GrupoMaisVariavel,
        ] {
            assert_eq!(q.resposta(&c).unwrap() as usize, afetado, "{aluno} {q:?}");
        }
        assert!(c.resposta.contains(&cj.grupos()[afetado]), "{}", c.resposta);
    }
    // The machine is drawn by student.
    assert!(vistas.len() > 1);
}

#[test]
fn estacoes_conferidas_contra_o_modelo() {
    let m = modelo();
    let mut g = conjunto(Dist::Exp { mean: 9.0 }).gerador;
    assert!(erros_da_simulacao(&g, &m).is_empty());
    if let Gerador::Simulacao {
        estacoes,
        afetado,
        duracao,
        ..
    } = &mut g
    {
        estacoes.push("chegada".into());
        estacoes.push("corte".into());
        *afetado = 9;
        *duracao = 0.0;
    }
    let e = erros_da_simulacao(&g, &m).join("\n");
    for x in [
        "'chegada' não é uma máquina",
        "'corte' aparece duas vezes",
        "causa plantada",
        "duração",
    ] {
        assert!(e.contains(x), "{x} in {e}");
    }
}

fn edicao() -> Edicao {
    let mut ed = Edicao::nova("prof/registros", "Registros da linha", &modelo_ron()).unwrap();
    for a in ed.alavancas.iter_mut() {
        a.usar = false;
    }
    for m in ed.metricas.iter_mut() {
        m.usar = false;
    }
    ed
}

#[test]
fn editor_monta_o_conjunto_a_partir_do_modelo() {
    let mut ed = edicao();
    let id = ed.novo_conjunto(TipoGerador::Simulacao);
    let cj = ed.base.conjunto(&id).unwrap();
    assert!(cj.sortear_afetado);
    assert_eq!(cj.ferramenta.as_deref(), Some("estratificacao"));
    match &cj.gerador {
        Gerador::Simulacao {
            estacoes, processo, ..
        } => {
            assert_eq!(estacoes, &["corte", "furacao", "pintura"]);
            // The first machine (exponential of mean 3) half as slow again.
            assert_eq!(
                *processo,
                Dist::Normal {
                    mean: 4.5,
                    sd: 0.45
                }
            );
        }
        _ => unreachable!(),
    }
    ed.nova_pergunta_de_dados(&id);
    if let Previsao::Dados { pergunta, .. } = &mut ed.base.previsao[0] {
        *pergunta = "Qual máquina ficou mais lenta?".into();
    }
    assert!(ed.erros().is_empty(), "{:?}", ed.erros());
    let a = ed.amostra_da_pergunta(0).unwrap();
    assert!(a.resposta.is_ok(), "{:?}", a.resposta);
    assert_eq!((a.sem_resposta, a.fora_do_plantado), (0, 0));
    assert!(
        ed.avisos_dos_dados().is_empty(),
        "{:?}",
        ed.avisos_dos_dados()
    );

    // A cause too small for the noise: the check warns.
    if let Gerador::Simulacao { processo, .. } = &mut ed.base.dados[0].gerador {
        *processo = Dist::Exp { mean: 3.0 };
    }
    let a = ed.amostra_da_pergunta(0).unwrap();
    assert!(a.fora_do_plantado > 0);
    assert!(
        ed.avisos_dos_dados()
            .iter()
            .any(|x| x.contains("efeito plantado"))
    );

    // A machine the model does not have stops writing.
    if let Gerador::Simulacao { estacoes, .. } = &mut ed.base.dados[0].gerador {
        estacoes[2] = "lixadeira".into();
    }
    assert!(ed.erros().iter().any(|x| x.contains("'lixadeira'")));
    assert!(ed.amostra_de_dados(&id).is_none());
}

#[test]
fn conjunto_de_simulacao_vai_ao_pacote_e_volta() {
    let mut ed = edicao();
    let id = ed.novo_conjunto(TipoGerador::Simulacao);
    ed.nova_pergunta_de_dados(&id);
    if let Previsao::Dados {
        pergunta, questao, ..
    } = &mut ed.base.previsao[0]
    {
        *pergunta = "Qual máquina ficou menos estável?".into();
        *questao = QuestaoDados::GrupoMaisVariavel;
    }
    if let Gerador::Simulacao { processo, .. } = &mut ed.base.dados[0].gerador {
        *processo = Dist::Exp { mean: 9.0 };
    }
    ed.base.analisar_em = vec!["qualidade".into()];
    let mut p = PacoteProfessor::novo("prof", "Pacote", "Prof");
    let caminho = p.gravar(&mut ed, 0).unwrap();
    let carregado = p.conferir().unwrap();
    let mc = &carregado.missoes[0];
    assert_eq!(mc.missao.dados, ed.base.dados);
    // The grading runs the set on the model of the mission, like the editor.
    let cj = &mc.missao.dados[0];
    assert_eq!(
        cj.gerar_com(&mc.missao.id, "ra7", &mc.modelo),
        cj.gerar_com(&ed.base.id, "ra7", &ed.modelo)
    );
    let de_novo = p.abrir(&caminho).unwrap();
    assert_eq!(de_novo.base.dados, ed.base.dados);
    assert_eq!(de_novo.base.previsao, ed.base.previsao);
}
