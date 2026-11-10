//! The mission editor of the teacher: a new mission from a model of `examples/`, the
//! checks of the fields, the calibration with the suggested goals, the teacher's package
//! (write, open again, move, remove) and the round trip that keeps every field.

use std::path::PathBuf;

use utsim_ensino_core::editor::{Edicao, PacoteProfessor, calibrar, id_valido, slug};
use utsim_ensino_core::missao::{Alavanca, Objetivo, Tolerancia};
use utsim_ensino_core::pacote::carregar;

fn modelo_tutorial() -> String {
    let raiz = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    std::fs::read_to_string(raiz.join("examples/tutorial_02_linha.ron")).unwrap()
}

/// The mission of the builder's test, made in the editor: two levers of three, one
/// server less at the top of each range, a budget.
fn edicao() -> Edicao {
    let mut ed = Edicao::nova("prof/linha", "A linha do tutorial", &modelo_tutorial()).unwrap();
    assert_eq!(ed.alavancas.len(), 3);
    assert!(ed.alavancas.iter().all(|a| a.usar));
    // Lead times in use, throughputs shown but out of the grade.
    assert!(
        ed.metricas
            .iter()
            .any(|m| m.usar && m.metrica.nome == "saida_lead_time")
    );
    assert!(ed.metricas.iter().any(|m| !m.usar));
    for a in ed.alavancas.iter_mut() {
        if a.alavanca.id() == "corte_capacidade" {
            a.usar = false;
        }
        if let Alavanca::Quantidade { ate, .. } = &mut a.alavanca {
            *ate -= 1;
        }
    }
    ed.base.orcamento = Some(10000.0);
    ed
}

#[test]
fn ids_e_nomes_de_pasta() {
    assert!(id_valido("prof/linha_2"));
    assert!(!id_valido("Prof/linha"));
    assert!(!id_valido("prof//linha"));
    assert!(!id_valido(""));
    assert_eq!(
        slug("Gargalo na Usinagem: ação!"),
        "gargalo_na_usinagem_acao"
    );
    assert_eq!(slug("***"), "missao");
}

#[test]
fn campos_conferidos_antes_de_gravar() {
    let mut ed = edicao();
    assert!(ed.erros().is_empty(), "{:?}", ed.erros());
    ed.base.id = "Prof Linha".into();
    ed.base.titulo = " ".into();
    for a in ed.alavancas.iter_mut() {
        if let Alavanca::Quantidade { de, ate, .. } = &mut a.alavanca {
            *de = *ate + 1;
        }
    }
    for m in ed.metricas.iter_mut() {
        m.metrica.peso = 0.0;
        m.metrica.tolerancia = Tolerancia::Absoluta(-1.0);
    }
    let e = ed.erros().join("\n");
    for parte in [
        "id 'Prof Linha'",
        "título",
        "'de' maior",
        "peso maior",
        "tolerância",
    ] {
        assert!(e.contains(parte), "falta '{parte}' em:\n{e}");
    }
    let mut ed = edicao();
    for a in ed.alavancas.iter_mut() {
        a.usar = false;
    }
    assert!(ed.erros().iter().any(|x| x.contains("alavanca")));
    // Too many combinations for the calibration.
    let mut ed = edicao();
    for a in ed.alavancas.iter_mut() {
        a.usar = true;
        if let Alavanca::Quantidade { ate, .. } = &mut a.alavanca {
            *ate += 20;
        }
    }
    assert!(ed.combinacoes() > utsim_ensino_core::editor::LIMITE_COMBINACOES);
    assert!(ed.erros().iter().any(|x| x.contains("combinações")));
    // A wrong mission is not written.
    let mut p = PacoteProfessor::novo("meu", "Meu pacote", "Prof");
    assert!(p.gravar(&mut ed, 0).is_err());
    assert!(p.missoes().is_empty());
}

#[test]
fn calibragem_acha_a_referencia_e_sugere_metas() {
    let mut ed = edicao();
    let c = calibrar(&ed).unwrap_or_else(|e| panic!("{}", e.join("\n")));
    // The same reference the builder finds: buy the drilling machine.
    assert_eq!(c.conferencia.melhor.get("furacao_capacidade"), Some(&2.0));
    assert!(c.fracao_meta() <= 1.0);
    let lt = |v: &[(String, f64)]| v.iter().find(|x| x.0 == "saida_lead_time").unwrap().1;
    assert!(
        lt(&c.referencia) < lt(&c.base),
        "a referência melhora o lead time"
    );
    let meta = c.metas.iter().find(|x| x.0 == "saida_lead_time").unwrap().1;
    assert!(meta >= lt(&c.referencia) && meta <= lt(&c.referencia) * 1.06);
    ed.aplicar(&c, true);
    let m = ed.missao();
    assert_eq!(m.referencia, c.conferencia.melhor);
    let mt = m
        .metricas
        .iter()
        .find(|x| x.nome == "saida_lead_time")
        .unwrap();
    assert_eq!(mt.objetivo, Objetivo::Menor);
    assert_eq!(mt.meta, Some(meta));
    // With the reference and the goals, the mission passes the checker.
    let p = ed.carregar().unwrap();
    let c2 = utsim_ensino_core::conferir::conferir(&p.missoes[0]).unwrap();
    assert!(c2.passou(), "{:?}", c2.erros);
    assert!(c2.referencia_confere);
    // A lever more keeps the reference (the rest is completed with the initial values).
    let corte = ed
        .alavancas
        .iter_mut()
        .find(|a| a.alavanca.id() == "corte_capacidade")
        .unwrap();
    corte.usar = true;
    assert_eq!(ed.missao().referencia, c.conferencia.melhor);
    // Dropping a lever the reference names drops the reference.
    for a in ed.alavancas.iter_mut() {
        if a.alavanca.id() == "furacao_capacidade" {
            a.usar = false;
        }
    }
    assert!(ed.missao().referencia.is_empty());
}

#[test]
fn pacote_do_professor_ida_e_volta() {
    let mut p = PacoteProfessor::novo("meu_pacote", "Meu pacote", "Prof");
    assert!(p.conferir().is_err(), "sem missões");
    let mut ed = edicao();
    ed.texto = "# A linha\n\nTexto meu.\n".into();
    // A field the editor does not show goes back as it was.
    ed.base.dicas = vec!["Olhe a fila antes da furação.".into()];
    let c1 = p.gravar(&mut ed, 0).unwrap();
    let mut ed2 = Edicao::nova("prof/outra", "Outra linha", &modelo_tutorial()).unwrap();
    for a in ed2.alavancas.iter_mut() {
        a.usar = a.alavanca.id() == "furacao_capacidade";
    }
    // Act 2 (past the last one: a new act).
    let c2 = p.gravar(&mut ed2, 5).unwrap();
    assert_ne!(c1, c2);
    let lista = p.missoes();
    assert_eq!(
        lista
            .iter()
            .map(|x| (x.ato, x.id.as_str()))
            .collect::<Vec<_>>(),
        [(0, "prof/linha"), (1, "prof/outra")]
    );
    let carregado = p.conferir().unwrap_or_else(|e| panic!("{}", e.join("\n")));
    assert_eq!(carregado.missoes.len(), 2);
    assert_eq!(carregado.pacote.atos.len(), 2);

    // Written to files and read back: the same package, the same mission.
    let arquivos = p.todos();
    let p2 = PacoteProfessor::de_arquivos(arquivos.clone()).unwrap();
    assert_eq!(p2.missoes(), lista);
    let aberta = p2.abrir(&c1).unwrap();
    assert_eq!(aberta.missao(), ed.missao());
    assert_eq!(aberta.texto, ed.texto);
    assert_eq!(aberta.modelo_ron, ed.modelo_ron);
    // The lever left out comes back as a suggestion, unmarked.
    assert!(
        aberta
            .alavancas
            .iter()
            .any(|a| !a.usar && a.alavanca.id() == "corte_capacidade")
    );
    assert_eq!(aberta.missao().dicas, ed.base.dicas);

    // Writing the same id again replaces it in place.
    let mut ed3 = aberta.clone();
    ed3.base.titulo = "A linha, revista".into();
    let mut p3 = p2.clone();
    assert_eq!(p3.gravar(&mut ed3, 1).unwrap(), c1);
    assert_eq!(p3.missoes()[0].titulo, "A linha, revista");
    assert_eq!(p3.missoes().len(), 2);

    // Moving down the last of an act takes it to the start of the next act (the order
    // stays, the act changes); the empty act goes away. Down again swaps.
    p3.mover(&c1, 1);
    assert_eq!(p3.pacote.atos.len(), 1);
    assert_eq!(p3.missoes()[0].id, "prof/linha");
    p3.mover(&c1, 1);
    assert_eq!(
        p3.missoes()
            .iter()
            .map(|x| x.id.as_str())
            .collect::<Vec<_>>(),
        ["prof/outra", "prof/linha"]
    );
    assert_eq!(p3.pacote.atos.len(), 1);
    p3.mover(&c1, -1);
    assert_eq!(p3.missoes()[0].id, "prof/linha");
    // Removing takes its files too.
    p3.remover(&c2);
    assert_eq!(p3.missoes().len(), 1);
    let dir = c2.rsplit_once('/').unwrap().0;
    assert!(!p3.todos().keys().any(|k| k.starts_with(&format!("{dir}/"))));
    assert!(carregar(&p3.todos()).is_ok());
}

fn arquivos_de(pasta: &str) -> utsim_ensino_core::pacote::Arquivos {
    let raiz = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join(pasta);
    let mut a = utsim_ensino_core::pacote::Arquivos::new();
    for e in walk(&raiz) {
        let k = e
            .strip_prefix(&raiz)
            .unwrap()
            .to_string_lossy()
            .replace('\\', "/");
        a.insert(k, std::fs::read(&e).unwrap());
    }
    a
}

#[test]
fn missoes_de_pecas_nao_abrem() {
    let p = PacoteProfessor::de_arquivos(arquivos_de("ensino/campanha")).unwrap();
    let it = p
        .missoes()
        .into_iter()
        .find(|it| it.caminho == "ato4/ventilador.ron")
        .unwrap();
    let e = p.abrir(&it.caminho).unwrap_err();
    assert!(e.contains("peças"), "{e}");
}

#[test]
fn missoes_de_dados_abrem_e_voltam_iguais() {
    let mut p =
        PacoteProfessor::de_arquivos(arquivos_de("ensino/pacotes/estatistica_qualidade")).unwrap();
    let antes = p.conferir().unwrap();
    let lista = p.missoes();
    assert!(lista.len() >= 6);
    for it in &lista {
        let mut ed = p.abrir(&it.caminho).unwrap();
        assert!(!ed.base.dados.is_empty(), "{}", it.id);
        assert!(ed.erros().is_empty(), "{}: {:?}", it.id, ed.erros());
        // The sample of every data question has an answer.
        for (i, q) in ed.base.previsao.iter().enumerate() {
            if matches!(q, utsim_ensino_core::missao::Previsao::Dados { .. }) {
                let a = ed.amostra_da_pergunta(i).unwrap();
                assert!(a.resposta.is_ok(), "{}: {:?}", it.id, a.resposta);
            }
        }
        p.gravar(&mut ed, 0).unwrap();
    }
    let depois = p.conferir().unwrap();
    for (a, b) in antes.missoes.iter().zip(&depois.missoes) {
        assert_eq!(a.missao, b.missao);
    }
}
fn walk(d: &std::path::Path) -> Vec<PathBuf> {
    let mut v = Vec::new();
    for e in std::fs::read_dir(d).unwrap() {
        let p = e.unwrap().path();
        if p.is_dir() {
            v.extend(walk(&p));
        } else {
            v.push(p);
        }
    }
    v
}

#[test]
fn ids_repetidos_e_id_trocado_depois_de_abrir() {
    let mut p = PacoteProfessor::novo("meu", "Meu", "Prof");
    let mut a = edicao();
    let ca = p.gravar(&mut a, 0).unwrap();
    // A second new mission with the same id does not overwrite the first.
    let mut b = edicao();
    let e = p.gravar(&mut b, 0).unwrap_err();
    assert!(e[0].contains("já existe"), "{e:?}");
    assert_eq!(p.missoes().len(), 1);
    // Opened and its id changed: it goes back to its place, no copy.
    let mut aberta = p.abrir(&ca).unwrap();
    assert_eq!(aberta.origem.as_deref(), Some(ca.as_str()));
    aberta.base.id = "prof/linha_nova".into();
    assert_eq!(p.gravar(&mut aberta, 0).unwrap(), ca);
    let l = p.missoes();
    assert_eq!(l.len(), 1);
    assert_eq!(l[0].id, "prof/linha_nova");
    // Written once more with no changes: the same files.
    let antes = p.todos();
    p.gravar(&mut aberta, 0).unwrap();
    assert_eq!(p.todos(), antes);
    // The first draft, written now under the old id, is a new mission.
    b.base.id = "prof/linha".into();
    let cb = p.gravar(&mut b, 0).unwrap();
    assert_ne!(cb, ca);
    assert!(carregar(&p.todos()).is_ok());
}

#[test]
fn remover_numa_pasta_comum_leva_so_a_missao() {
    let raiz = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../ensino/pacotes/estatistica_qualidade");
    let mut a = utsim_ensino_core::pacote::Arquivos::new();
    for e in walk(&raiz) {
        let k = e
            .strip_prefix(&raiz)
            .unwrap()
            .to_string_lossy()
            .replace('\\', "/");
        a.insert(k, std::fs::read(&e).unwrap());
    }
    let mut p = PacoteProfessor::de_arquivos(a).unwrap();
    let n = p.missoes().len();
    let alvo = p.missoes()[2].caminho.clone();
    assert!(alvo.starts_with("eq/"));
    p.remover(&alvo);
    let todos = p.todos();
    assert!(!todos.contains_key(&alvo));
    // The other missions of eq/ and the shared models stay, and the package loads.
    assert_eq!(p.missoes().len(), n - 1);
    assert!(todos.contains_key("modelos/eixos.ron"));
    let c = carregar(&todos).unwrap_or_else(|e| panic!("{}", e.join("\n")));
    assert_eq!(c.missoes.len(), n - 1);
}

#[test]
fn metas_sugeridas_a_referencia_sempre_bate() {
    use utsim_ensino_core::editor::meta_sugerida;
    assert_eq!(meta_sugerida(0.96, Objetivo::Maior, 0), 0.0);
    assert_eq!(meta_sugerida(1.4, Objetivo::Menor, 0), 2.0);
    assert_eq!(meta_sugerida(10.0, Objetivo::Menor, 1), 10.5);
    assert_eq!(meta_sugerida(10.0, Objetivo::Maior, 1), 9.5);
    // Negative values keep the slack on the right side.
    assert!(meta_sugerida(-10.0, Objetivo::Menor, 1) >= -10.0);
    assert!(meta_sugerida(-10.0, Objetivo::Maior, 1) <= -10.0);
    for r in [0.37, 3.21, 127.4, 0.001] {
        for c in 0..3 {
            assert!(meta_sugerida(r, Objetivo::Menor, c) >= r);
            assert!(meta_sugerida(r, Objetivo::Maior, c) <= r);
        }
    }
}

/// Stage B: a prediction of each kind, a justification, hints and concepts, checked in the
/// editor, written to the package with the glossary and opened again as they were.
#[test]
fn perguntas_dicas_e_conceitos_vao_e_voltam() {
    use utsim_ensino_core::editor::chaves_do_modelo;
    use utsim_ensino_core::missao::{
        Calculo, Medicao, OpcaoPrevisao, Pergunta, Previsao, Resposta, Termo,
    };
    let mut ed = edicao();
    let chaves = chaves_do_modelo(&ed.modelo).unwrap();
    let lead = "saida.time_in_system.mean".to_string();
    assert!(chaves.contains(&lead), "{chaves:?}");
    assert!(ed.chaves_de_decisao().contains(&"custo".to_string()));
    assert!(
        ed.chaves_de_decisao()
            .iter()
            .all(|k| k != "decisao.corte_capacidade")
    );

    // Wrong fields are caught before writing.
    ed.base.previsao.push(Previsao::Escolha {
        id: "P 1".into(),
        pergunta: " ".into(),
        opcoes: vec![OpcaoPrevisao {
            rotulo: "a".into(),
            valor: vec![],
        }],
        resposta: Resposta::Maior,
    });
    ed.base.justificativa.push(Pergunta {
        pergunta: "Por quê?".into(),
        opcoes: vec!["só uma".into()],
        certa: 3,
    });
    ed.base.dicas = vec!["".into(), "b".into(), "c".into(), "d".into()];
    ed.base.conceitos = vec!["gargalo".into(), "gargalo".into()];
    let e = ed.erros().join("\n");
    for parte in [
        "previsão 1: id 'P 1'",
        "escreva a pergunta",
        "duas opções",
        "métrica que decide",
        "justificativa 1: marque",
        "três dicas",
        "dica vazia",
        "'gargalo' repetido",
    ] {
        assert!(e.contains(parte), "falta '{parte}' em:\n{e}");
    }

    // The right ones go to the package.
    ed.base.medicoes = vec![Medicao {
        rotulo: "Lead time hoje".into(),
        valor: vec![Termo {
            chave: lead.clone(),
            fator: 1.0,
        }],
        unidade: "min".into(),
        casas: 1,
    }];
    let p1 = ed.id_previsao_livre();
    assert_eq!(p1, "p1");
    ed.base.previsao = vec![Previsao::Escolha {
        id: p1,
        pergunta: "O que limita a linha?".into(),
        opcoes: vec![
            OpcaoPrevisao {
                rotulo: "A furação".into(),
                valor: vec![],
            },
            OpcaoPrevisao {
                rotulo: "A chegada".into(),
                valor: vec![],
            },
        ],
        resposta: Resposta::Fixa(0),
    }];
    let p2 = ed.id_previsao_livre();
    assert_eq!(p2, "p2");
    ed.base.previsao.push(Previsao::Valor {
        id: p2,
        pergunta: "Qual o lead time de hoje?".into(),
        valor: vec![Termo {
            chave: lead,
            fator: 1.0,
        }],
        unidade: "min".into(),
        tolerancia: Tolerancia::Relativa(0.1),
    });
    ed.base.previsao.push(Previsao::Calculo {
        id: "p3".into(),
        pergunta: "Qual o tempo normal?".into(),
        calculo: Calculo::TempoNormal {
            tempos: vec![1.0, 1.2, 0.8],
            ritmo: 1.1,
        },
        unidade: "min".into(),
        tolerancia: Tolerancia::Absoluta(0.05),
    });
    ed.base.justificativa = vec![Pergunta {
        pergunta: "Por que a vazão quase não mudou?".into(),
        opcoes: vec!["O gargalo é outro".into(), "Faltou orçamento".into()],
        certa: 0,
    }];
    ed.base.dicas = vec!["Onde a fila cresce?".into(), "Veja a utilização".into()];
    ed.base.conceitos = vec!["gargalo".into(), "utilizacao".into()];
    assert!(ed.erros().is_empty(), "{:?}", ed.erros());

    let mut p = PacoteProfessor::novo("meu", "Meu pacote", "Prof");
    assert!(p.glossario().termos.iter().any(|t| t.id == "gargalo"));
    let caminho = p.gravar(&mut ed, 0).unwrap();
    let carregado = p.conferir().unwrap_or_else(|e| panic!("{e:?}"));
    let ids: Vec<&str> = carregado
        .glossario
        .termos
        .iter()
        .map(|t| t.id.as_str())
        .collect();
    assert_eq!(ids, ["gargalo", "utilizacao"]);
    assert_eq!(p.pacote.glossario.as_deref(), Some("glossario.ron"));

    let de_volta = p.abrir(&caminho).unwrap();
    assert_eq!(de_volta.base.previsao, ed.base.previsao);
    assert_eq!(de_volta.base.justificativa, ed.base.justificativa);
    assert_eq!(de_volta.base.dicas, ed.base.dicas);
    assert_eq!(de_volta.base.conceitos, ed.base.conceitos);
    assert_eq!(de_volta.base.medicoes, ed.base.medicoes);

    // A concept out of every glossary is refused when writing.
    let mut ed2 = de_volta;
    ed2.base.conceitos.push("nao_existe".into());
    let e = p.gravar(&mut ed2, 0).unwrap_err().join("\n");
    assert!(e.contains("conceito 'nao_existe'"), "{e}");
}

/// The teacher's own glossary: its terms can be named (the calibration does not refuse
/// them), they survive a new campaign term, and a glossary that does not parse is never
/// rewritten. A package without a glossary whose missions name unknown concepts is left
/// without one, so it loads as before.
#[test]
fn glossario_proprio_do_professor() {
    let mut p = PacoteProfessor::novo("meu", "Meu pacote", "Prof");
    let mut ed = edicao();
    p.gravar(&mut ed, 0).unwrap();
    let proprio = r#"Glossario(termos: [(id: "meu_termo", nome: "Meu termo", texto: "Um termo do professor.")])"#;
    p.arquivos
        .insert("glossario.ron".into(), proprio.as_bytes().to_vec());
    p.pacote.glossario = Some("glossario.ron".into());
    assert!(p.glossario().termos.iter().any(|t| t.id == "meu_termo"));

    ed.base.conceitos = vec!["meu_termo".into(), "gargalo".into()];
    assert!(calibrar(&ed).is_ok());
    p.gravar(&mut ed, 0).unwrap();
    let c = p.conferir().unwrap_or_else(|e| panic!("{e:?}"));
    let ids: Vec<&str> = c.glossario.termos.iter().map(|t| t.id.as_str()).collect();
    assert_eq!(ids, ["meu_termo", "gargalo"]);

    // A broken glossary stays as it is and the check reports it.
    p.arquivos
        .insert("glossario.ron".into(), b"Glossario(termos: [".to_vec());
    ed.base.conceitos = vec!["gargalo".into(), "utilizacao".into()];
    p.gravar(&mut ed, 0).unwrap();
    assert_eq!(p.arquivos["glossario.ron"], b"Glossario(termos: [".to_vec());
    assert!(p.conferir().is_err());

    // Without a glossary, a hand-written unknown concept keeps the package as it was.
    let mut q = PacoteProfessor::novo("outro", "Outro", "");
    let mut ed = edicao();
    q.gravar(&mut ed, 0).unwrap();
    let caminho = q.missoes()[0].caminho.clone();
    let mut m = utsim_ensino_core::missao::Missao::parse(
        std::str::from_utf8(&q.arquivos[&caminho]).unwrap(),
    )
    .unwrap();
    m.conceitos = vec!["a_mao".into()];
    let cfg = ron::ser::PrettyConfig::new().struct_names(true);
    q.arquivos.insert(
        caminho.clone(),
        ron::ser::to_string_pretty(&m, cfg).unwrap().into_bytes(),
    );
    q.mover(&caminho, 1);
    assert!(q.pacote.glossario.is_none());
    q.conferir().unwrap_or_else(|e| panic!("{e:?}"));
}
