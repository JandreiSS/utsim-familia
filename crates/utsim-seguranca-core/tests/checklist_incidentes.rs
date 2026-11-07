//! Machine checklist, protective equipment register, incidents and indicators.

use utsim_posto::{Pessoa, Posto, Unidade, nomes_no_texto};
use utsim_seguranca_core::checklist::{
    self, Epi, InspecaoMaquina, Modelo, Resposta, RespostaItem, gravar_perigos, pendencias_epi,
};
use utsim_seguranca_core::incidentes::{
    Config, HorasHomem, INCIDENTES, Incidente, Tipo, carta_mensal, indicadores,
};
use utsim_seguranca_core::inventario::{Inventario, PERIGOS};

#[test]
fn non_compliant_items_become_hazards_once() {
    let dir = tempfile::tempdir().unwrap();
    let mut u = Unidade::criar(dir.path(), "Fábrica", "tecnica").unwrap();
    let posto = u
        .gravar_posto(&Posto {
            nome: "Prensa".into(),
            epi: vec!["oculos".into(), "protetor_auricular".into()],
            ..Posto::default()
        })
        .unwrap();
    let m = Modelo::proprio();
    assert_eq!(m.itens.len(), 16);
    assert!(
        checklist::TEMAS
            .iter()
            .all(|t| m.itens.iter().any(|i| &i.tema == t))
    );
    let mut insp = InspecaoMaquina {
        id: "i1".into(),
        posto: posto.clone(),
        maquina: "prensa excêntrica".into(),
        modelo: m.id.clone(),
        data: "2026-11-07".into(),
        ..InspecaoMaquina::default()
    };
    for (k, item) in m.itens.iter().enumerate() {
        let resposta = match k {
            3 | 11 => Resposta::NaoConforme,
            15 => Resposta::NaoSeAplica,
            _ => Resposta::Conforme,
        };
        insp.respostas.insert(
            item.id.clone(),
            RespostaItem {
                resposta,
                observacao: if resposta == Resposta::NaoConforme {
                    "visto na ronda".into()
                } else {
                    String::new()
                },
                ..RespostaItem::default()
            },
        );
    }
    assert!(insp.pendentes(&m).is_empty());
    assert_eq!(gravar_perigos(&mut u, &insp, &m).unwrap(), 2);
    // Again: nothing new.
    assert_eq!(gravar_perigos(&mut u, &insp, &m).unwrap(), 0);
    let inv = Inventario::ler(&u);
    let ps = inv.perigos_do_posto(&posto);
    assert_eq!(ps.len(), 2);
    assert!(ps.iter().all(|p| p.origem.starts_with("checklist:i1:")));
    assert!(ps.iter().any(|p| p.descricao.contains("proteção móvel")));
    let _ = PERIGOS;
    // Company checklist by CSV.
    let e = Modelo::de_csv(
        "Roteiro da empresa",
        "tema;item;severidade\nComandos;Botão bimanual testado;4\nSinalização;Placa no posto\n",
    )
    .unwrap();
    assert_eq!(e.itens.len(), 2);
    assert_eq!(e.itens[1].severidade, 2);
    assert!(Modelo::de_csv("x", "só uma coluna\n").is_err());
    // Protective equipment register.
    let epis = vec![Epi {
        tipo: "oculos".into(),
        ca: "12345".into(),
        validade_ca: "2026-01-01".into(),
        ..Epi::default()
    }];
    let p = pendencias_epi(&u.postos(), &epis, "2026-11-07");
    assert!(p.iter().any(|x| x.contains("protetor_auricular")));
    assert!(p.iter().any(|x| x.contains("vencida")));
}

fn ev(data: &str, tipo: Tipo, posto: &str, causa: &str, perdidos: f64) -> Incidente {
    Incidente {
        data: data.into(),
        tipo,
        posto: posto.into(),
        zona: "corredor".into(),
        descricao: "caso".into(),
        causas: vec![causa.into()],
        pessoas: vec!["P-0a0b0c".into()],
        dias_perdidos: perdidos,
        ..Incidente::default()
    }
}

#[test]
fn indicators_by_hand() {
    let eventos = vec![
        ev(
            "2026-01-10",
            Tipo::QuaseAcidente,
            "prensa",
            "piso escorregadio",
            0.0,
        ),
        ev(
            "2026-01-20",
            Tipo::QuaseAcidente,
            "prensa",
            "piso escorregadio",
            0.0,
        ),
        ev("2026-02-05", Tipo::SemAfastamento, "solda", "sem luva", 0.0),
        ev(
            "2026-03-15",
            Tipo::ComAfastamento,
            "prensa",
            "piso escorregadio",
            12.0,
        ),
        ev("2026-03-20", Tipo::QuaseAcidente, "solda", "sem luva", 0.0),
        ev(
            "2025-12-30",
            Tipo::ComAfastamento,
            "prensa",
            "fora do período",
            5.0,
        ),
    ];
    let horas = vec![
        HorasHomem {
            mes: "2026-01".into(),
            horas: 40_000.0,
            ..HorasHomem::default()
        },
        HorasHomem {
            mes: "2026-02".into(),
            horas: 38_000.0,
            ..HorasHomem::default()
        },
        HorasHomem {
            mes: "2026-03".into(),
            horas: 42_000.0,
            ..HorasHomem::default()
        },
    ];
    let k = indicadores(
        &eventos,
        &horas,
        Config::default(),
        "2026-01-01",
        "2026-03-31",
        "2026-04-04",
    );
    assert_eq!(k.horas, 120_000.0);
    // 2 injuries in 120 000 h: 2 * 1e6 / 120 000.
    assert!((k.taxa_frequencia - 2.0e6 / 120_000.0).abs() < 1e-9);
    assert!((k.taxa_frequencia_afastamento - 1.0e6 / 120_000.0).abs() < 1e-9);
    assert!((k.taxa_gravidade - 12.0e6 / 120_000.0).abs() < 1e-9);
    assert_eq!(k.razao_quase, Some(1.5));
    // Last injury on 2026-03-15, today 2026-04-04: 20 days.
    assert_eq!(k.dias_sem_acidente, Some(20));
    assert_eq!(k.pareto_causas.rows[0].name, "piso escorregadio");
    assert_eq!(k.por_zona["corredor"], 5);
    assert_eq!(k.mensal.len(), 3);
    assert!(carta_mensal(&k, Config::default()).is_some());
    for e in &eventos {
        assert!(e.problemas().is_empty());
    }
    let mut com_nome = eventos[0].clone();
    com_nome.pessoas = vec!["Maria".into()];
    assert!(!com_nome.problemas().is_empty());
}

#[test]
fn names_never_leave_the_restricted_folder() {
    let dir = tempfile::tempdir().unwrap();
    let mut u = Unidade::criar(dir.path(), "Fábrica", "tecnica").unwrap();
    let mut r = u.criar_restrita(None, &["tecnica".to_string()]).unwrap();
    let codigo = r
        .cadastrar(Pessoa {
            nome: "Joana Ribeiro".into(),
            saude: "corte na mão".into(),
            ..Pessoa::default()
        })
        .unwrap();
    let mut i = ev(
        "2026-03-15",
        Tipo::ComAfastamento,
        "prensa",
        "sem luva",
        3.0,
    );
    i.pessoas = vec![codigo.clone()];
    u.gravar(INCIDENTES, "", &i).unwrap();
    let todos: Vec<Incidente> = u.estado().lista(INCIDENTES);
    let k = indicadores(
        &todos,
        &[],
        Config::default(),
        "2026-01-01",
        "2026-12-31",
        "2026-12-31",
    );
    let texto = format!("{todos:?} {:?} {:?}", k.pareto_causas, k.por_zona);
    assert!(nomes_no_texto(&texto, &r.pessoas()).is_empty());
    assert!(texto.contains(&codigo));
}
