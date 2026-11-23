//! The drawn statements (`cena`): the plan of the scene from the model, the elements that
//! exist as 3D files, the live content of the cards and the checks of a broken scene.

use std::path::PathBuf;

use utsim_ensino_core::campanha::{arquivos, carregar_campanha};
use utsim_ensino_core::cena::{Carga, Cena, Conteudo, TipoCartao, conteudo};
use utsim_ensino_core::missao::Missao;
use utsim_ensino_core::pacote::{junta, manifesto};
use utsim_model::Model;

fn raiz() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn texto(p: &str) -> String {
    String::from_utf8(arquivos()[p].clone()).unwrap()
}

/// Every mission of the campaign with a scene, with the model it draws (a modelling
/// mission draws its reference model).
fn com_cena() -> Vec<(Missao, Model)> {
    let arq = arquivos();
    let pacote = manifesto(&arq).unwrap();
    let mut v = Vec::new();
    for ato in &pacote.atos {
        for path in &ato.missoes {
            let m = Missao::parse(&texto(path)).unwrap();
            if m.cena.is_some() {
                let dir = path.rsplit_once('/').map_or("", |(d, _)| d);
                let arquivo = m.modelagem.as_ref().map_or(&m.modelo, |md| &md.gabarito);
                let modelo = Model::parse(&texto(&junta(dir, arquivo))).unwrap();
                v.push((m, modelo));
            }
        }
    }
    v
}

fn mix() -> (Missao, Model) {
    com_cena()
        .into_iter()
        .find(|(m, _)| m.id == "ato4/mix")
        .unwrap()
}

#[test]
fn planta_da_linha_mista() {
    let (m, modelo) = mix();
    let p = m.cena.as_ref().unwrap().planta(&modelo, m.arranjo.as_ref());
    let postos: Vec<_> = p.objetos.iter().filter(|o| o.elemento == "posto").collect();
    assert_eq!(postos.len(), 10);
    // Man and woman in turn, at every station.
    let pessoas: Vec<&str> = postos
        .iter()
        .map(|o| o.pessoa.as_deref().unwrap())
        .collect();
    assert_eq!(&pessoas[..4], ["homem", "mulher", "homem", "mulher"]);
    // The distances of the model at half: est1 is at 6 m in the model.
    let est1 = p.objetos.iter().find(|o| o.nome == "est1").unwrap();
    assert_eq!((est1.x, est1.y), (3.0, 0.0));
    assert_eq!(est1.fases.get("ciclo"), Some(&0.5));
    let chegada = p.objetos.iter().find(|o| o.nome == "chegada").unwrap();
    assert_eq!(chegada.elemento, "espera_chao");
    assert!(chegada.pessoa.is_none());
    let saida = p.objetos.iter().find(|o| o.nome == "saida_mesa").unwrap();
    assert_eq!(saida.elemento, "palete");
    // The products stand on their objects, with the offset of the scene.
    assert_eq!(p.pecas.len(), 11);
    assert_eq!(p.pecas[0].x, chegada.x - 1.0);
    assert_eq!(p.pecas[0].elemento, "ventilador_mesa");
}

#[test]
fn elementos_das_cenas_existem_em_3d() {
    let pasta = raiz().join("assets/modelos3d");
    let existe = |e: &str| pasta.join(format!("{e}.glb")).exists();
    assert!(existe("pessoa_homem") && existe("pessoa_mulher"));
    let todas = com_cena();
    assert!(!todas.is_empty());
    for (m, modelo) in todas {
        let p = m.cena.as_ref().unwrap().planta(&modelo, m.arranjo.as_ref());
        for e in p
            .objetos
            .iter()
            .map(|o| &o.elemento)
            .chain(p.pecas.iter().map(|x| &x.elemento))
            .chain(
                p.grade
                    .iter()
                    .flat_map(|g| g.maquinas.iter().map(|x| &x.elemento)),
            )
        {
            assert!(
                existe(e),
                "{}: o elemento '{e}' não existe em assets/modelos3d",
                m.id
            );
        }
    }
}

#[test]
fn cartoes_seguem_as_decisoes() {
    let (m, _) = mix();
    let cena = m.cena.as_ref().unwrap();
    let cartao = |no: &str| cena.cartoes.iter().find(|c| c.no == no).unwrap();
    // The old balance: station 1 does A and B, the column model goes over the takt.
    let inicial = m.decisoes_iniciais();
    let c = conteudo(&m, cartao("est1"), &inicial, None).unwrap();
    assert!(c.alerta());
    let Conteudo::Estacao { n, tarefas, cargas } = c else {
        panic!("{c:?}")
    };
    assert_eq!((n, tarefas), (1, vec!["A".to_string(), "B".to_string()]));
    assert_eq!(
        cargas,
        vec![
            Carga {
                modelo: Some(0),
                nome: "Mesa".into(),
                valor: 25.0,
                passa: false
            },
            Carga {
                modelo: Some(1),
                nome: "Coluna".into(),
                valor: 65.0,
                passa: true
            },
        ]
    );
    // Dragging task B to station 2 changes the card at once.
    let mut d = inicial.clone();
    d.insert("tarefa.B".into(), 2.0);
    let Some(Conteudo::Estacao { cargas, .. }) = conteudo(&m, cartao("est1"), &d, None) else {
        panic!()
    };
    assert_eq!((cargas[0].valor, cargas[1].valor), (15.0, 25.0));
    // The lever card shows the option in use; the measure, its goal before the run.
    assert_eq!(
        conteudo(&m, cartao("chegada@pe"), &inicial, None),
        Some(Conteudo::Alavanca {
            rotulo: "Sequência de entrada na linha".into(),
            valor: "Lotes de 40 (40 de mesa, depois 40 de coluna)".into()
        })
    );
    let Some(Conteudo::Metrica {
        meta, valor, ok, ..
    }) = conteudo(&m, cartao("saida_mesa"), &inicial, None)
    else {
        panic!()
    };
    assert_eq!((meta, valor, ok), (Some(29.5), None, None));
    let Some(Conteudo::Previsao { pergunta }) = conteudo(&m, cartao("fila3@pe"), &inicial, None)
    else {
        panic!()
    };
    assert!(pergunta.starts_with("Com o mesmo balanceamento"));
}

#[test]
fn cena_quebrada_recusada() {
    let (mut m, modelo) = mix();
    let cena = m.cena.as_mut().unwrap();
    cena.imagem = "mix.jpg".into();
    cena.ocultar.push("est99".into());
    cena.cartoes.push(utsim_ensino_core::cena::Cartao {
        no: "est3".into(),
        tipo: TipoCartao::Estacao(11),
    });
    cena.cartoes.push(utsim_ensino_core::cena::Cartao {
        no: "saida_mesa".into(),
        tipo: TipoCartao::Metrica("vazao".into()),
    });
    cena.cartoes.push(utsim_ensino_core::cena::Cartao {
        no: "est2".into(),
        tipo: TipoCartao::Valor {
            rotulo: "Vazão".into(),
            valor: vec![utsim_ensino_core::missao::Termo {
                chave: "saida_mesa.input".into(),
                fator: 1.0,
            }],
            unidade: String::new(),
            casas: 0,
        },
    });
    let e = m.cena.as_ref().unwrap().validar(&m, &modelo).join("\n");
    for trecho in [
        "a imagem 'mix.jpg' precisa ser .png",
        "o objeto 'est99' não está no modelo",
        "estação 11 fora do balanceamento",
        "métrica 'vazao' não existe",
        "usa 'saida_mesa.input'",
    ] {
        assert!(e.contains(trecho), "falta \"{trecho}\" em:\n{e}");
    }
}

#[test]
fn campanha_carrega_imagem_e_ancoras() {
    let p = carregar_campanha().unwrap_or_else(|e| panic!("{}", e.join("\n")));
    let mc = p
        .missoes
        .iter()
        .find(|m| m.missao.id == "ato4/mix")
        .unwrap();
    let c = mc.cena.as_ref().expect("a 4.3 tem cena");
    assert!(c.png.len() > 10_000);
    // Every card has its anchor inside the picture.
    for cartao in &c.cena.cartoes {
        let [x, y] = c.ancoras.nos[&cartao.no];
        assert!(
            (0.0..=c.ancoras.largura).contains(&x) && (0.0..=c.ancoras.altura).contains(&y),
            "{}: ({x}, {y}) fora da imagem",
            cartao.no
        );
    }
    // Every mission with a scene was read with its picture.
    for m in &p.missoes {
        assert_eq!(m.missao.cena.is_some(), m.cena.is_some(), "{}", m.missao.id);
    }
}

#[test]
fn capacidade_vira_postos_lado_a_lado() {
    let (m, modelo) = com_cena()
        .into_iter()
        .find(|(m, _)| m.id == "ato2/central")
        .unwrap();
    let p = m.cena.as_ref().unwrap().planta(&modelo, m.arranjo.as_ref());
    // Three attendants: three stations across the flow, the middle one keeps the name.
    let nomes: Vec<&str> = p
        .objetos
        .iter()
        .filter(|o| o.nome.starts_with("atendentes"))
        .map(|o| o.nome.as_str())
        .collect();
    assert_eq!(nomes, ["atendentes#1", "atendentes", "atendentes#3"]);
    let ys: Vec<f64> = p
        .objetos
        .iter()
        .filter(|o| o.nome.starts_with("atendentes"))
        .map(|o| o.y)
        .collect();
    assert_eq!(ys, [-2.4, 0.0, 2.4]);
    assert!(
        p.objetos.iter().all(|o| o.nome != "chamadas"),
        "a fonte está oculta"
    );
    // The urgent orders are drawn in red.
    let (u, mu) = com_cena()
        .into_iter()
        .find(|(m, _)| m.id == "ato2/urgente")
        .unwrap();
    let pu = u.cena.as_ref().unwrap().planta(&mu, None);
    assert!(pu.pecas.iter().any(|x| x.cor.as_deref() == Some("#d63031")));
}

#[test]
fn arranjo_desenha_a_grade_e_move_as_maquinas() {
    let (m, modelo) = com_cena()
        .into_iter()
        .find(|(m, _)| m.id == "ato5/oficina")
        .unwrap();
    let cena = m.cena.as_ref().unwrap();
    let p = cena.planta(&modelo, m.arranjo.as_ref());
    // The objects of the model are the compiled routes: the picture shows the grid.
    assert!(p.objetos.is_empty());
    let g = p.grade.as_ref().unwrap();
    assert_eq!(g.celulas.len(), 12);
    assert_eq!(g.lado, 4.0);
    assert_eq!(g.referencia, "celula0");
    let serra = g.maquinas.iter().find(|x| x.nome == "serra").unwrap();
    assert_eq!(serra.elemento, "prensa");
    assert!(serra.pessoa.is_some());
    // The card of a machine follows it to the cell the student puts it in.
    let cartao = cena.cartoes.iter().find(|c| c.no == "serra@pe").unwrap();
    let mut d = m.decisoes_iniciais();
    let antes = Cena::ancora(&m, cartao, &d);
    d.insert("pos.serra".into(), 6.0);
    assert_eq!(Cena::ancora(&m, cartao, &d), "celula6@pe");
    assert_ne!(antes, "celula6@pe");
    assert_eq!(
        conteudo(&m, cartao, &d, None),
        Some(Conteudo::Maquina {
            nome: "Serra".into(),
            fixa: false
        })
    );
    // The package reads one picture per machine, of the size of the scene.
    let pacote = carregar_campanha().unwrap();
    let mc = pacote
        .missoes
        .iter()
        .find(|x| x.missao.id == "ato5/oficina")
        .unwrap();
    let cc = mc.cena.as_ref().unwrap();
    assert_eq!(cc.maquinas.len(), 7);
}

#[test]
fn pessoas_com_nome_seguem_a_missao() {
    let (m, modelo) = com_cena()
        .into_iter()
        .find(|(m, _)| m.id == "ato6/celula")
        .unwrap();
    let p = m.cena.as_ref().unwrap().planta(&modelo, None);
    let el = |n: &str| {
        p.objetos
            .iter()
            .find(|o| o.nome == n)
            .unwrap()
            .elemento
            .clone()
    };
    assert_eq!(
        (el("ana"), el("bruno"), el("carla")),
        (
            "pessoa_mulher".to_string(),
            "pessoa_homem".to_string(),
            "pessoa_mulher".to_string()
        )
    );
    // The model has its own operators: the machines get none.
    assert!(p.objetos.iter().all(|o| o.pessoa.is_none()));
}

#[test]
fn modelagem_desenha_o_gabarito_sem_copias() {
    let (m, modelo) = com_cena()
        .into_iter()
        .find(|(m, _)| m.id == "ato7/padaria")
        .unwrap();
    let p = m.cena.as_ref().unwrap().planta(&modelo, m.arranjo.as_ref());
    // The process of the statement, from the reference model: the student's model has only
    // the entrance and the exit.
    for n in ["divisora", "camara", "forno", "embalagem", "expedicao"] {
        assert!(p.objetos.iter().any(|o| o.nome == n), "{n}");
    }
    // The chamber (capacity 10) and the oven (capacity 2) are one element each.
    assert!(p.objetos.iter().all(|o| !o.nome.contains('#')));
    let forno = p.objetos.iter().find(|o| o.nome == "forno").unwrap();
    assert_eq!(forno.elemento, "forno");
}

#[test]
fn mover_tira_a_operadora_de_tras_das_maquinas() {
    let (m, modelo) = com_cena()
        .into_iter()
        .find(|(m, _)| m.id == "ato6/trabalho_externo")
        .unwrap();
    let c = m.cena.as_ref().unwrap();
    let p = c.planta(&modelo, None);
    // Ana stands at y = 1 in the model, behind the machines; the picture brings her to the
    // front (the camera looks from -y), and the simulation keeps her place.
    let ana = p.objetos.iter().find(|o| o.nome == "ana").unwrap();
    assert!(ana.y < 0.0, "{}", ana.y);
    assert_eq!(
        modelo
            .objects
            .iter()
            .find(|o| o.name == "ana")
            .unwrap()
            .position[1],
        1.0
    );
    // Three lathes, as the statement says.
    let maquinas = p.objetos.iter().filter(|o| o.elemento == "cnc").count();
    assert_eq!(maquinas, 3);
}
