//! O caso `eq/capacidade` do Ensino de ponta a ponta: os dois CSV que o jogo grava para o
//! aluno, com as opções que ele passa ao UTSim Qualidade, viram um estudo de capacidade
//! cujo Cpk bate com a resposta que o jogo corrige (dentro da tolerância da missão).

use utsim_ensino_core::questoes::QuestaoDados;
use utsim_ensino_store::analise::{argumentos, gravar_dados, opcoes};
use utsim_estudo::calculo::calcular;
use utsim_estudo::tabela::numero;
use utsim_estudo::{Ferramenta, relatorio::Identidade};
use utsim_posto_ui::{Abrir, Destino, FnResumo, Sessao, TelaFerramentas, TelaModelo, abrir};

fn telas() -> (TelaModelo, TelaFerramentas) {
    let resumo: FnResumo = std::sync::Arc::new(|_| Ok(Default::default()));
    (
        TelaModelo::nova("qualidade", utsim_edit::abas::Aba::Qualidade, resumo),
        TelaFerramentas::nova(utsim_estudo::App::Qualidade, Identidade::default(), None),
    )
}

#[test]
fn ensino_capability_mission_opens_and_matches_the_game() {
    let raiz = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../ensino/pacotes/estatistica_qualidade");
    let p = utsim_ensino_core::pacote::carregar(
        &utsim_ensino_store::arquivos::ler_pacote(&raiz).unwrap(),
    )
    .unwrap();
    let m = p.missao("eq/capacidade").unwrap();
    let questao = QuestaoDados::Cpk {
        lie: Some(24.985),
        lse: Some(25.015),
    };
    for aluno in ["ra042", "ra107", "ana.souza", "b"] {
        let dir = tempfile::tempdir().unwrap();
        let arqs = gravar_dados(dir.path(), m, aluno).unwrap();
        let mut args = argumentos(&[], &arqs);
        args.extend(opcoes("qualidade", m));
        let a = Abrir::de_args(args);
        assert!(a.erros.is_empty(), "{:?}", a.erros);

        let mut s = Sessao::nova(Some(dir.path().join("config.json")));
        let (mut tm, mut tf) = telas();
        assert_eq!(
            abrir::atender(&a, &mut s, &mut tm, &mut tf),
            Some(Destino::Ferramentas)
        );
        let e = tf.estudo.as_ref().expect("estudo aberto");
        assert_eq!(e.aplicacoes.len(), 2);
        for (ap, cj) in e.aplicacoes.iter().zip(&m.missao.dados) {
            assert_eq!(ap.ferramenta, Ferramenta::Capacidade);
            assert_eq!(ap.titulo, cj.id);
            assert_eq!(ap.parametros.lie, Some(24.985));
            assert_eq!(ap.parametros.lse, Some(25.015));
            assert_eq!(ap.parametros.subgrupo, 5, "x1..x5 viram subgrupos de 5");
            let r = calcular(ap, "");
            assert!(r.avisos.is_empty(), "{aluno} {}: {:?}", cj.id, r.avisos);
            let cpk = r
                .indicadores
                .iter()
                .find(|k| k.label == "Cpk")
                .and_then(|k| numero(&k.value))
                .expect("Cpk na tela");
            let jogo = questao.resposta(&cj.gerar(&m.missao.id, aluno)).unwrap();
            // A missão aceita 0,05 de diferença; a tela mostra duas casas.
            assert!(
                (cpk - jogo).abs() <= 0.006,
                "{aluno} {}: tela {cpk} contra jogo {jogo}",
                cj.id
            );
        }
    }
}
