//! Os desenhos sem janela, com cliques, arrastos e teclas que chegam como eventos do egui,
//! quadro a quadro, como chegariam do mouse. Com `UTSIM_CAPTURAS=<pasta>`, cada desenho é
//! também pintado num PNG em `<pasta>/visual/`, para revisar.

use std::cell::Cell;
use std::path::PathBuf;

use egui::{Event, Key, Modifiers, PointerButton, Pos2, RawInput, Rect, Vec2, vec2};
use utsim_ui_tema::captura::Captura;
use utsim_visual::{Nivel, arvore, casa, espinha, matriz, pareto};

const TELA: Vec2 = Vec2::new(1100.0, 620.0);

/// Um quadro com eventos; `f` desenha e devolve o que o componente devolveu.
fn quadro<T>(ctx: &egui::Context, eventos: Vec<Event>, mut f: impl FnMut(&mut egui::Ui) -> T) -> T {
    let entrada = RawInput {
        screen_rect: Some(Rect::from_min_size(Pos2::ZERO, TELA)),
        events: eventos,
        ..RawInput::default()
    };
    let mut saida = None;
    let mut out = ctx.run_ui(entrada, |ui| saida = Some(f(ui)));
    out.textures_delta.clear();
    saida.expect("o quadro desenhou")
}

fn mover(p: Pos2) -> Event {
    Event::PointerMoved(p)
}

fn botao(p: Pos2, apertado: bool) -> Event {
    Event::PointerButton {
        pos: p,
        button: PointerButton::Primary,
        pressed: apertado,
        modifiers: Modifiers::NONE,
    }
}

fn tecla(key: Key) -> Vec<Event> {
    [true, false]
        .into_iter()
        .map(|pressed| Event::Key {
            key,
            physical_key: None,
            pressed,
            repeat: false,
            modifiers: Modifiers::NONE,
        })
        .collect()
}

fn capturar(nome: &str, mut f: impl FnMut(&mut egui::Ui)) {
    let Some(dir) = std::env::var_os("UTSIM_CAPTURAS").map(PathBuf::from) else {
        return;
    };
    let ctx = egui::Context::default();
    ctx.set_visuals(egui::Visuals::dark());
    let mut cap = Captura::new(ctx, TELA);
    cap.imagem(4, |ui| f(ui))
        .salvar_png(&dir.join("visual").join(format!("{nome}.png")))
        .expect("PNG gravado");
}

fn ossos() -> Vec<espinha::Osso> {
    let o = |nome: &str, c: &[&str]| espinha::Osso {
        nome: nome.into(),
        cartoes: c.iter().map(|t| espinha::Cartao::novo(*t)).collect(),
    };
    vec![
        o("Máquina", &["Ferramenta gasta", "Folga no eixo"]),
        o("Método", &["Sem instrução de trabalho"]),
        o("Material", &[]),
        o("Mão de obra", &["Operador novo"]),
        o("Medição", &["Paquímetro sem calibração"]),
        o("Meio ambiente", &[]),
    ]
}

#[test]
fn arrastar_um_cartao_para_outro_osso_pede_para_mudar_a_categoria() {
    let ctx = egui::Context::default();
    let ossos = ossos();
    let e = espinha::Espinha::editavel("Rebarba na peça", &ossos);
    let canto = Cell::new(Pos2::ZERO);
    let largura = Cell::new(0.0);
    let mut desenhar = |ui: &mut egui::Ui| {
        canto.set(ui.cursor().min);
        largura.set(ui.available_width());
        espinha::mostrar(ui, "espinha", &e)
    };
    quadro(&ctx, vec![], &mut desenhar);
    let cont: Vec<usize> = ossos.iter().map(|o| o.cartoes.len()).collect();
    let rect = Rect::from_min_size(
        canto.get(),
        vec2(
            largura.get().max(600.0),
            espinha::altura(&cont, true, false),
        ),
    );
    let d = espinha::dispor(rect, &cont, true, false);
    let de = d.ossos[0].cartoes[1].center();
    let para = d.ossos[4].area.center();
    let mut acoes = Vec::new();
    for ev in [
        vec![mover(de)],
        vec![botao(de, true)],
        vec![mover(de + vec2(12.0, 8.0))],
        vec![mover(para)],
        vec![mover(para)],
        vec![botao(para, false)],
        vec![],
    ] {
        acoes.extend(quadro(&ctx, ev, &mut desenhar));
    }
    assert_eq!(
        acoes,
        [espinha::Acao::Mover {
            osso: 0,
            cartao: 1,
            para: 4
        }]
    );
}

#[test]
fn soltar_o_cartao_no_mesmo_osso_nao_muda_nada() {
    let ctx = egui::Context::default();
    let ossos = ossos();
    let e = espinha::Espinha::editavel("", &ossos);
    let canto = Cell::new(Pos2::ZERO);
    let mut desenhar = |ui: &mut egui::Ui| {
        canto.set(ui.cursor().min);
        espinha::mostrar(ui, "espinha", &e)
    };
    quadro(&ctx, vec![], &mut desenhar);
    let cont: Vec<usize> = ossos.iter().map(|o| o.cartoes.len()).collect();
    let rect = Rect::from_min_size(
        canto.get(),
        vec2(
            TELA.x - 2.0 * canto.get().x,
            espinha::altura(&cont, true, false),
        ),
    );
    let d = espinha::dispor(rect, &cont, true, false);
    let de = d.ossos[1].cartoes[0].center();
    let perto = de + vec2(0.0, 30.0);
    let mut acoes = Vec::new();
    for ev in [
        vec![mover(de)],
        vec![botao(de, true)],
        vec![mover(perto)],
        vec![botao(perto, false)],
        vec![],
    ] {
        acoes.extend(quadro(&ctx, ev, &mut desenhar));
    }
    assert!(acoes.is_empty(), "{acoes:?}");
}

#[test]
fn clicar_no_osso_e_digitar_acrescenta_uma_causa() {
    let ctx = egui::Context::default();
    let ossos = ossos();
    let e = espinha::Espinha::editavel("Rebarba", &ossos);
    let canto = Cell::new(Pos2::ZERO);
    let mut desenhar = |ui: &mut egui::Ui| {
        canto.set(ui.cursor().min);
        espinha::mostrar(ui, "espinha", &e)
    };
    quadro(&ctx, vec![], &mut desenhar);
    let cont: Vec<usize> = ossos.iter().map(|o| o.cartoes.len()).collect();
    let rect = Rect::from_min_size(
        canto.get(),
        vec2(
            TELA.x - 2.0 * canto.get().x,
            espinha::altura(&cont, true, false),
        ),
    );
    let d = espinha::dispor(rect, &cont, true, false);
    // Um ponto da faixa do osso "Material" fora de qualquer cartão.
    let alvo = d.ossos[2].novo.center() + vec2(0.0, 40.0);
    assert_eq!(d.osso_em(alvo), Some(2));
    let mut acoes = Vec::new();
    for ev in [
        vec![mover(alvo)],
        vec![botao(alvo, true)],
        vec![botao(alvo, false)],
        vec![],
        vec![],
        vec![Event::Text("Lote com dureza baixa".into())],
        tecla(Key::Enter),
        vec![],
    ] {
        acoes.extend(quadro(&ctx, ev, &mut desenhar));
    }
    assert_eq!(
        acoes,
        [espinha::Acao::Acrescentar {
            osso: 2,
            texto: "Lote com dureza baixa".into()
        }]
    );
}

fn barras() -> Vec<pareto::Barra> {
    pareto::barras_de(&[
        ("Risco".into(), 42.0),
        ("Mancha".into(), 25.0),
        ("Bolha".into(), 12.0),
        ("Trinca".into(), 6.0),
        ("Outros".into(), 4.0),
    ])
}

#[test]
fn clicar_numa_barra_escolhe_a_categoria_e_clicar_de_novo_desfaz() {
    let ctx = egui::Context::default();
    let b = barras();
    let canto = Cell::new(Pos2::ZERO);
    let largura = Cell::new(0.0);
    let mut desenhar = |ui: &mut egui::Ui| {
        canto.set(ui.cursor().min);
        largura.set(ui.available_width());
        pareto::mostrar(ui, "pareto", &b, 260.0)
    };
    assert_eq!(quadro(&ctx, vec![], &mut desenhar), None);
    let rect = Rect::from_min_size(canto.get(), vec2(largura.get().max(280.0), 260.0));
    let g = pareto::dispor(rect, &b);
    let p = g.barras[1].center();
    let mut sel = None;
    for ev in [
        vec![mover(p)],
        vec![botao(p, true)],
        vec![botao(p, false)],
        vec![],
    ] {
        sel = quadro(&ctx, ev, &mut desenhar);
    }
    assert_eq!(sel.as_deref(), Some("Mancha"));
    assert_eq!(pareto::escolhida(&ctx, "pareto").as_deref(), Some("Mancha"));
    for ev in [vec![botao(p, true)], vec![botao(p, false)], vec![]] {
        sel = quadro(&ctx, ev, &mut desenhar);
    }
    assert_eq!(sel, None);
}

#[test]
fn clicar_num_ponto_da_matriz_leva_a_linha() {
    let ctx = egui::Context::default();
    let pontos = vec![
        matriz::Ponto {
            linha: 3,
            s: 8,
            o: 6,
            nivel: Nivel::Alto,
            rotulo: "Vazamento".into(),
        },
        matriz::Ponto {
            linha: 0,
            s: 3,
            o: 2,
            nivel: Nivel::Baixo,
            rotulo: "Risco".into(),
        },
    ];
    let canto = Cell::new(Pos2::ZERO);
    let mut desenhar = |ui: &mut egui::Ui| {
        canto.set(ui.cursor().min);
        matriz::mostrar(ui, "matriz", &pontos, None)
    };
    quadro(&ctx, vec![], &mut desenhar);
    // A grade começa depois da margem dos números de S.
    let grade = Rect::from_min_size(
        canto.get() + vec2(34.0, 4.0),
        vec2(10.0 * matriz::CELULA, 10.0 * matriz::CELULA),
    );
    let p = matriz::celula(grade, 8, 6).center();
    let mut clicada = None;
    for ev in [vec![mover(p)], vec![botao(p, true)], vec![botao(p, false)]] {
        clicada = clicada.or(quadro(&ctx, ev, &mut desenhar));
    }
    assert_eq!(clicada, Some(3));
}

#[test]
fn clicar_na_celula_da_casa_troca_a_relacao() {
    let ctx = egui::Context::default();
    let nec = vec![
        ("Fecha fácil".to_string(), 5.0),
        ("Não vaza".to_string(), 4.0),
    ];
    let req = vec![
        "Torque".to_string(),
        "Vedação".to_string(),
        "Peso".to_string(),
    ];
    let rel = vec![vec![9, 0, 1], vec![0, 3, 0]];
    let c = casa::Casa {
        necessidades: &nec,
        requisitos: &req,
        relacoes: &rel,
        telhado: &[(0, 1, -2)],
        pesos: &[50.0, 30.0, 20.0],
        editavel: true,
    };
    let canto = Cell::new(Pos2::ZERO);
    let mut desenhar = |ui: &mut egui::Ui| {
        canto.set(ui.cursor().min);
        casa::mostrar(ui, "casa", &c)
    };
    quadro(&ctx, vec![], &mut desenhar);
    let pl = casa::dispor(canto.get(), 2, 3);
    let p = pl.celula(1, 2).center();
    let t = pl.losango(1, 2);
    let mut acoes = Vec::new();
    for ev in [
        vec![mover(p)],
        vec![botao(p, true)],
        vec![botao(p, false)],
        vec![mover(t)],
        vec![botao(t, true)],
        vec![botao(t, false)],
    ] {
        acoes.extend(quadro(&ctx, ev, &mut desenhar));
    }
    assert_eq!(
        acoes,
        [
            casa::Acao::Relacao {
                necessidade: 1,
                requisito: 2,
                valor: 9
            },
            casa::Acao::Correlacao {
                a: 1,
                b: 2,
                valor: 2
            }
        ]
    );
}

fn nos() -> Vec<arvore::No> {
    use arvore::{No, Tipo};
    let mut acao = No::novo(Some(2), Tipo::Acao, "Plano de lubrificação");
    acao.pode_filho = false;
    vec![
        No::novo(None, Tipo::Raiz, "A prensa parou no turno B"),
        No::novo(Some(0), Tipo::Porque, "O motor aqueceu"),
        No::novo(Some(1), Tipo::Porque, "Faltou óleo"),
        No::novo(Some(1), Tipo::Porque, "O ventilador estava sujo"),
        acao,
    ]
}

#[test]
fn o_mais_do_no_acrescenta_um_porque_abaixo() {
    let ctx = egui::Context::default();
    let nos = nos();
    let a = arvore::Arvore {
        nos: &nos,
        marcar: None,
    };
    let canto = Cell::new(Pos2::ZERO);
    let mut desenhar = |ui: &mut egui::Ui| {
        canto.set(ui.cursor().min);
        arvore::mostrar(ui, "arvore", &a)
    };
    quadro(&ctx, vec![], &mut desenhar);
    let (r, _) = arvore::dispor(canto.get(), &nos);
    let no = r[3].center();
    let mais = r[3].right_center() + vec2(12.0, 0.0);
    let mut acoes = Vec::new();
    for ev in [
        vec![mover(no)],
        vec![mover(mais)],
        vec![botao(mais, true)],
        vec![botao(mais, false)],
        vec![],
        vec![],
        vec![Event::Text("Sem rotina de limpeza".into())],
        tecla(Key::Enter),
        vec![],
    ] {
        acoes.extend(quadro(&ctx, ev, &mut desenhar));
    }
    assert_eq!(
        acoes,
        [arvore::Acao::Filho {
            no: 3,
            texto: "Sem rotina de limpeza".into()
        }]
    );
}

#[test]
fn capturas_dos_desenhos() {
    let ossos = ossos();
    capturar("espinha", |ui| {
        espinha::mostrar(
            ui,
            "e",
            &espinha::Espinha::editavel("Rebarba na peça", &ossos),
        );
    });
    let b = barras();
    capturar("pareto", |ui| {
        pareto::mostrar(ui, "p", &b, 300.0);
    });
    let nos = nos();
    capturar("arvore", |ui| {
        arvore::mostrar(
            ui,
            "a",
            &arvore::Arvore {
                nos: &nos,
                marcar: None,
            },
        );
    });
    let pontos: Vec<matriz::Ponto> = [
        (9, 7, Nivel::Alto),
        (6, 4, Nivel::Medio),
        (6, 4, Nivel::Alto),
        (2, 3, Nivel::Baixo),
    ]
    .iter()
    .enumerate()
    .map(|(i, &(s, o, nivel))| matriz::Ponto {
        linha: i,
        s,
        o,
        nivel,
        rotulo: format!("modo {i}"),
    })
    .collect();
    capturar("matriz", |ui| {
        matriz::mostrar(ui, "m", &pontos, Some(1));
    });
    let nec = vec![
        ("Fecha sem esforço".to_string(), 5.0),
        ("Não vaza".to_string(), 4.0),
        ("Bonita".to_string(), 2.0),
    ];
    let req = vec![
        "Torque de fechamento".to_string(),
        "Pressão de vedação".to_string(),
        "Rugosidade".to_string(),
    ];
    let rel = vec![vec![9, 3, 0], vec![0, 9, 1], vec![0, 0, 3]];
    capturar("casa", |ui| {
        casa::mostrar(
            ui,
            "c",
            &casa::Casa {
                necessidades: &nec,
                requisitos: &req,
                relacoes: &rel,
                telhado: &[(0, 1, -2), (1, 2, 1)],
                pesos: &[45.0, 40.0, 15.0],
                editavel: true,
            },
        );
    });
    capturar("espinha_miniatura", |ui| {
        ui.set_max_width(420.0);
        espinha::mostrar(ui, "m", &espinha::Espinha::miniatura("Rebarba", &ossos));
    });
}
