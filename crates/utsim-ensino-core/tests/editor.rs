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
    assert!(p.gravar(&ed, 0).is_err());
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
    // Dropping a lever drops the reference (it named the old levers).
    ed.alavancas[0].usar = !ed.alavancas[0].usar;
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
    let c1 = p.gravar(&ed, 0).unwrap();
    let mut ed2 = Edicao::nova("prof/outra", "Outra linha", &modelo_tutorial()).unwrap();
    for a in ed2.alavancas.iter_mut() {
        a.usar = a.alavanca.id() == "furacao_capacidade";
    }
    // Act 2 (past the last one: a new act).
    let c2 = p.gravar(&ed2, 5).unwrap();
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
    assert_eq!(p3.gravar(&ed3, 1).unwrap(), c1);
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

#[test]
fn provas_e_missoes_especiais_nao_abrem() {
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
    let p = PacoteProfessor::de_arquivos(a).unwrap();
    let it = &p.missoes()[0];
    let e = p.abrir(&it.caminho).unwrap_err();
    assert!(e.contains("dados"), "{e}");
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
