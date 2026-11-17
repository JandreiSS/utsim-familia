//! A cor do chão vai no replay, para um replay aberto sozinho mostrar o chão do seu modelo.

use utsim_model::Model;
use utsim_model::replay::Replay;

fn modelo(chao: &str) -> Model {
    let m = Model::parse(&format!(
        "Model(name: \"chao\", run: (stop_time: 20.0, seed: 1, replications: 1),
            {chao}
            objects: [
                (name: \"fonte\", kind: Source(interarrival: Constant(5.0)), position: (0.0, 0.0, 0.0)),
                (name: \"saida\", kind: Sink, position: (4.0, 0.0, 0.0)),
            ],
            connections: [(\"fonte\", \"saida\")])"
    ))
    .expect("modelo lê");
    m.validate().expect("modelo válido");
    m
}

#[test]
fn a_cor_do_chao_vai_no_replay_e_volta_do_json() {
    let m = modelo("floor_marking: Some((floor_color: Some(\"#d0c8a0\"))),");
    let r = m.replay(0).unwrap();
    assert_eq!(r.floor_color.as_deref(), Some("#d0c8a0"));
    let texto = serde_json::to_string(&r).unwrap();
    let lido: Replay = serde_json::from_str(&texto).unwrap();
    assert_eq!(lido.floor_color.as_deref(), Some("#d0c8a0"));
}

#[test]
fn sem_cor_o_replay_nao_ganha_o_campo_e_os_antigos_continuam_lendo() {
    let r = modelo("").replay(0).unwrap();
    assert_eq!(r.floor_color, None);
    let texto = serde_json::to_string(&r).unwrap();
    assert!(!texto.contains("floor_color"), "{texto}");
    // Um replay gravado antes do campo existir.
    let lido: Replay = serde_json::from_str(&texto).unwrap();
    assert_eq!(lido.floor_color, None);
}
