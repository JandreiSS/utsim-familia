//! As abas dos nós nas cinco línguas: nenhuma chave falta, os rótulos mudam com a língua e os
//! valores trocados com `editar` (caminhos, escolhas, `sim`/`não`) não mudam.

use utsim_edit::abas::{self, Aba};
use utsim_edit::texto::TEXTOS;
use utsim_idiomas::Idioma;
use utsim_model::Model;

const MODELO: &str = r#"Model(
    name: "abas",
    run: (stop_time: 100.0, seed: 1, replications: 1),
    objects: [
        (name: "chegada", kind: Source(interarrival: Const(1.0))),
        (name: "torno", kind: Processor(process: Const(0.5))),
        (name: "cliente", kind: Sink),
    ],
    connections: [("chegada", "torno"), ("torno", "cliente")],
)"#;

#[test]
fn as_abas_nas_cinco_linguas() {
    assert!(TEXTOS.faltas().is_empty(), "{:?}", TEXTOS.faltas());
    let m = Model::parse(MODELO).unwrap();
    let mut nomes = Vec::new();
    let mut caminhos = None;
    for i in Idioma::TODOS {
        utsim_idiomas::definir_nesta_thread(Some(i));
        nomes.push(Aba::Qualidade.nome());
        let mut agora = Vec::new();
        for a in Aba::TODAS {
            for c in abas::campos(&m, "torno", a).unwrap() {
                assert!(!c.rotulo.is_empty(), "{a:?}: {c:?}");
                agora.push((c.caminho, c.valor, c.escolhas));
            }
        }
        match &caminhos {
            None => caminhos = Some(agora),
            Some(antes) => assert_eq!(antes, &agora, "{i:?}"),
        }
        assert!(abas::editar(&m, "torno", "inspection", "sim").is_ok());
    }
    nomes.dedup();
    assert_eq!(nomes.len(), Idioma::TODOS.len(), "{nomes:?}");
    utsim_idiomas::definir_nesta_thread(None);
}
