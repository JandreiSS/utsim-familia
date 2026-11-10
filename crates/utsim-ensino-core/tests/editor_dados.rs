//! Stage C of the mission editor: data sets made by the generators of section 6.3, the
//! questions that fit each kind, the checks, the sample the teacher sees and the round
//! trip through the teacher's package.

use std::path::PathBuf;

use utsim_ensino_core::dados::{Gerador, Padrao};
use utsim_ensino_core::editor::{Edicao, PacoteProfessor};
use utsim_ensino_core::editor_dados::{TipoGerador, erros_do_gerador};
use utsim_ensino_core::missao::Previsao;
use utsim_ensino_core::questoes::QuestaoDados;

/// A mission with no lever: only questions about data.
fn edicao() -> Edicao {
    let raiz = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let modelo = std::fs::read_to_string(raiz.join("examples/tutorial_02_linha.ron")).unwrap();
    let mut ed = Edicao::nova("prof/dados", "Dados da linha", &modelo).unwrap();
    for a in ed.alavancas.iter_mut() {
        a.usar = false;
    }
    for m in ed.metricas.iter_mut() {
        m.usar = false;
    }
    ed
}

fn perguntar(ed: &mut Edicao, conjunto: &str, texto: &str) -> usize {
    ed.nova_pergunta_de_dados(conjunto);
    let i = ed.base.previsao.len() - 1;
    if let Previsao::Dados { pergunta, .. } = &mut ed.base.previsao[i] {
        *pergunta = texto.into();
    }
    i
}

fn questao(ed: &mut Edicao, i: usize) -> &mut QuestaoDados {
    match &mut ed.base.previsao[i] {
        Previsao::Dados { questao, .. } => questao,
        _ => panic!("not a data question"),
    }
}

#[test]
fn cada_gerador_comeca_valido_e_cada_pergunta_tem_resposta() {
    let mut ed = edicao();
    for tipo in TipoGerador::TODOS {
        assert!(erros_do_gerador(&tipo.padrao()).is_empty(), "{tipo:?}");
        let id = ed.novo_conjunto(tipo);
        let cj = ed.base.conjunto(&id).unwrap();
        assert_eq!(TipoGerador::de(&cj.gerador), tipo);
        for q in tipo.questoes() {
            assert!(tipo.aceita(&q));
            let i = perguntar(&mut ed, &id, "Quanto?");
            *questao(&mut ed, i) = match q {
                QuestaoDados::Ppk { .. } => QuestaoDados::Ppk {
                    lie: None,
                    lse: Some(1e6),
                },
                QuestaoDados::Cpk { .. } => QuestaoDados::Cpk {
                    lie: Some(9.5),
                    lse: Some(10.5),
                },
                q => q,
            };
        }
        assert!(ed.amostra_de_dados(&id).is_some());
    }
    assert_eq!(ed.base.dados.len(), 7);
    // The series and the subgroups need a pattern for the "first signal" questions.
    for cj in ed.base.dados.iter_mut() {
        if let Gerador::Subgrupos { padrao, .. } = &mut cj.gerador {
            *padrao = Some(Padrao::Deslocamento {
                inicio: 20,
                tamanho: 3.0,
            });
        }
    }
    assert!(ed.erros().is_empty(), "{:?}", ed.erros());
    for i in 0..ed.base.previsao.len() {
        let a = ed.amostra_da_pergunta(i).unwrap();
        assert!(a.resposta.is_ok(), "{i}: {:?}", a.resposta);
    }
}

#[test]
fn campos_dos_dados_conferidos() {
    let mut ed = edicao();
    let id = ed.novo_conjunto(TipoGerador::MediaDeslocada);
    let i = perguntar(&mut ed, &id, "Qual turno?");
    assert!(ed.erros().is_empty(), "{:?}", ed.erros());
    // A choice of group: the answer is one of the group names.
    let a = ed.amostra_da_pergunta(i).unwrap();
    assert!(a.resposta.unwrap().starts_with("Turno"));
    assert_eq!(a.sem_resposta, 0);

    if let Gerador::MediaDeslocada {
        grupos,
        n_por_grupo,
        ..
    } = &mut ed.base.dados[0].gerador
    {
        grupos.push("Turno A".into());
        *n_por_grupo = 5000;
    }
    ed.base.dados[0].ferramenta = Some("planilha".into());
    ed.base.dados[0].lie = Some(5.0);
    ed.base.dados[0].lse = Some(4.0);
    ed.base.analisar_em = vec!["estatistica".into(), "estatistica".into(), "outro".into()];
    let e = ed.erros().join("\n");
    for x in [
        "mesmo nome",
        "linhas: reduza",
        "ferramenta 'planilha'",
        "limite inferior",
        "'estatistica' repetido",
        "'outro' desconhecido",
    ] {
        assert!(e.contains(x), "{x} in {e}");
    }

    // A question that does not fit the kind of the set, and one with no set.
    let mut ed = edicao();
    let id = ed.novo_conjunto(TipoGerador::Msa);
    let i = perguntar(&mut ed, &id, "Efeito?");
    *questao(&mut ed, i) = QuestaoDados::Efeito { fator: 0 };
    let j = perguntar(&mut ed, &id, "Outra?");
    if let Previsao::Dados { conjunto, .. } = &mut ed.base.previsao[j] {
        *conjunto = "nenhum".into();
    }
    let e = ed.erros().join("\n");
    assert!(e.contains("previsão 1: a pergunta não serve"), "{e}");
    assert!(e.contains("previsão 2: escolha o conjunto"), "{e}");
}

#[test]
fn tipo_nome_e_remocao_levam_as_perguntas_junto() {
    let mut ed = edicao();
    let id = ed.novo_conjunto(TipoGerador::Msa);
    perguntar(&mut ed, &id, "%R&R?");
    perguntar(&mut ed, &id, "Fonte?");
    *questao(&mut ed, 1) = QuestaoDados::FonteDominante;
    // A new kind: the questions that no longer fit go to its first question.
    ed.mudar_tipo(0, TipoGerador::Pareado);
    assert_eq!(*questao(&mut ed, 0), QuestaoDados::DiferencaMedia);
    assert_eq!(*questao(&mut ed, 1), QuestaoDados::DiferencaMedia);
    assert!(ed.base.dados[0].ferramenta.is_none());
    assert!(ed.erros().is_empty(), "{:?}", ed.erros());
    // A new id: the questions follow.
    ed.renomear_conjunto(0, "pares");
    assert!(
        ed.base
            .previsao
            .iter()
            .all(|p| matches!(p, Previsao::Dados { conjunto, .. } if conjunto == "pares"))
    );
    // Removed: its questions and the analysis apps go too.
    ed.base.analisar_em = vec!["estatistica".into()];
    let outra = ed.novo_conjunto(TipoGerador::Tempos);
    perguntar(&mut ed, &outra, "Meia largura?");
    assert_eq!(ed.remover_conjunto(0), 2);
    assert_eq!(ed.base.previsao.len(), 1);
    assert_eq!(ed.base.analisar_em, vec!["estatistica".to_string()]);
    assert_eq!(ed.remover_conjunto(0), 1);
    assert!(ed.base.analisar_em.is_empty());
}

#[test]
fn aviso_de_alunos_sem_resposta_e_conjunto_sem_pergunta() {
    let mut ed = edicao();
    let id = ed.novo_conjunto(TipoGerador::Serie);
    let i = perguntar(&mut ed, &id, "Primeiro sinal?");
    *questao(&mut ed, i) = QuestaoDados::PrimeiroSinal { base: 20 };
    // A tiny shift: the rules miss it for many students.
    if let Gerador::Serie { padrao, .. } = &mut ed.base.dados[0].gerador {
        *padrao = Padrao::Deslocamento {
            inicio: 45,
            tamanho: 0.01,
        };
    }
    ed.novo_conjunto(TipoGerador::Tempos);
    let avisos = ed.avisos_dos_dados().join("\n");
    assert!(avisos.contains("não tem pergunta"), "{avisos}");
    let a = ed.amostra_da_pergunta(i).unwrap();
    if a.sem_resposta > 0 {
        assert!(avisos.contains("alunos de teste"), "{avisos}");
    }
}

#[test]
fn missao_de_dados_vai_ao_pacote_e_volta() {
    let mut ed = edicao();
    let id = ed.novo_conjunto(TipoGerador::Fatorial);
    let i = perguntar(&mut ed, &id, "Qual é o efeito da pressão?");
    *questao(&mut ed, i) = QuestaoDados::Efeito { fator: 1 };
    ed.base.analisar_em = vec!["estatistica".into()];
    let mut p = PacoteProfessor::novo("prof", "Pacote", "Prof");
    let caminho = p.gravar(&mut ed, 0).unwrap();
    let carregado = p.conferir().unwrap();
    assert_eq!(carregado.missoes[0].missao.dados, ed.base.dados);
    let de_novo = p.abrir(&caminho).unwrap();
    assert_eq!(de_novo.base.dados, ed.base.dados);
    assert_eq!(de_novo.base.previsao, ed.base.previsao);
    assert_eq!(de_novo.base.analisar_em, ed.base.analisar_em);
}
