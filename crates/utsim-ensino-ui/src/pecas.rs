//! The screens of the pieces: the task board of line balancing (drag a task to a
//! station) and the layout grid (drag a machine to a cell). A click selects a task or a
//! machine and a click on a station or cell moves it, for who does not drag.

use egui::{Color32, CornerRadius, Frame, Id, RichText, Sense, Ui, vec2};
use utsim_ensino_core::missao::Decisoes;
use utsim_ensino_core::nota::fmt_num;
use utsim_ensino_core::pecas::{Arranjo, Balanceamento};

use crate::texto;

const AMARELO: Color32 = Color32::from_rgb(236, 196, 76);
const VERMELHO: Color32 = Color32::from_rgb(255, 112, 104);
const VERDE: Color32 = Color32::from_rgb(110, 214, 120);

#[derive(Clone)]
struct TarefaArrastada(String);

#[derive(Clone)]
struct MaquinaArrastada(String);

/// The task board. `selecionada` keeps the task picked by a click between frames.
pub fn quadro_balanceamento(
    ui: &mut Ui,
    b: &Balanceamento,
    d: &mut Decisoes,
    selecionada: &mut Option<String>,
) {
    let atrib = b.atribuicao(d);
    let resumo = b.resumo(&atrib);
    ui.label(
        RichText::new(texto::f(
            texto::vista_balanc_resumo(),
            &[
                &fmt_num(b.takt, 0),
                &fmt_num(b.soma(), 0),
                &resumo.estacoes_usadas,
                &fmt_num(resumo.eficiencia, 3),
            ],
        ))
        .strong(),
    );
    ui.label(RichText::new(texto::vista_balanc_ajuda()).weak());
    let mut mover: Option<(String, u32)> = None;
    egui::ScrollArea::horizontal()
        .id_salt("quadro")
        .show(ui, |ui| {
            ui.horizontal_top(|ui| {
                for s in 1..=b.estacoes {
                    let carga = resumo.cargas[s as usize - 1];
                    let frame = Frame::group(ui.style()).inner_margin(6.0);
                    let (resp, solto) = ui
                        .vertical(|ui| {
                            ui.dnd_drop_zone::<TarefaArrastada, ()>(frame, |ui| {
                                ui.set_width(118.0);
                                ui.set_min_height(170.0);
                                let cor = if carga > b.takt + 1e-9 {
                                    VERMELHO
                                } else if carga > 0.0 {
                                    VERDE
                                } else {
                                    ui.visuals().weak_text_color()
                                };
                                if ui
                                    .add(
                                        egui::Label::new(
                                            RichText::new(texto::f(
                                                texto::vista_estacao_n(),
                                                &[&s],
                                            ))
                                            .strong(),
                                        )
                                        .sense(Sense::click()),
                                    )
                                    .clicked()
                                    && let Some(t) = selecionada.take()
                                {
                                    mover = Some((t, s));
                                }
                                ui.colored_label(
                                    cor,
                                    texto::f(
                                        texto::vista_carga_de_takt(),
                                        &[&fmt_num(carga, 0), &fmt_num(b.takt, 0)],
                                    ),
                                );
                                let (r, p) = ui.allocate_painter(vec2(106.0, 6.0), Sense::hover());
                                p.rect_filled(
                                    r.rect,
                                    CornerRadius::same(2),
                                    ui.visuals().extreme_bg_color,
                                );
                                let f = (carga / b.takt).min(1.2) as f32 / 1.2;
                                let mut cheia = r.rect;
                                cheia.set_width(r.rect.width() * f);
                                p.rect_filled(cheia, CornerRadius::same(2), cor);
                                for t in b.tarefas.iter().filter(|t| atrib.get(&t.id) == Some(&s)) {
                                    let id = Id::new(("tarefa", &t.id));
                                    let sel = selecionada.as_deref() == Some(t.id.as_str());
                                    let r = ui.dnd_drag_source(
                                        id,
                                        TarefaArrastada(t.id.clone()),
                                        |ui| {
                                            let txt = texto::f(
                                                texto::vista_tarefa_tempo(),
                                                &[&t.id, &fmt_num(t.tempo, 0)],
                                            );
                                            let r = ui.add(egui::Button::new(txt).selected(sel));
                                            r.on_hover_text(format!(
                                                "{}{}",
                                                t.nome,
                                                if t.depois_de.is_empty() {
                                                    String::new()
                                                } else {
                                                    texto::f(
                                                        texto::vista_depois_de_lista(),
                                                        &[&t.depois_de.join(", ")],
                                                    )
                                                }
                                            ))
                                        },
                                    );
                                    if r.inner.clicked() {
                                        *selecionada = if sel { None } else { Some(t.id.clone()) };
                                    }
                                }
                            })
                        })
                        .inner;
                    if resp.response.clicked()
                        && let Some(t) = selecionada.take()
                    {
                        mover = Some((t, s));
                    }
                    if let Some(p) = solto {
                        mover = Some((p.0.clone(), s));
                    }
                }
            });
        });
    if let Some((t, s)) = mover {
        d.insert(Balanceamento::chave(&t), s as f64);
    }
    egui::CollapsingHeader::new(texto::vista_tarefas_precedencias())
        .default_open(true)
        .show(ui, |ui| {
            egui::Grid::new("tarefas").striped(true).show(ui, |ui| {
                for h in [
                    texto::vista_col_tarefa(),
                    texto::vista_col_descricao(),
                    texto::vista_col_tempo(),
                    texto::vista_col_depois_de(),
                    texto::vista_col_estacao(),
                ] {
                    ui.label(RichText::new(h).strong());
                }
                ui.end_row();
                for t in &b.tarefas {
                    ui.label(&t.id);
                    ui.label(&t.nome);
                    ui.label(fmt_num(t.tempo, 0));
                    ui.label(t.depois_de.join(", "));
                    ui.label(
                        atrib
                            .get(&t.id)
                            .map_or_else(|| "-".into(), |s| s.to_string()),
                    );
                    ui.end_row();
                }
            });
        });
}

/// The layout grid.
pub fn grade_arranjo(ui: &mut Ui, a: &Arranjo, d: &mut Decisoes, selecionada: &mut Option<String>) {
    let pos = a.posicoes(d);
    ui.label(
        RichText::new(texto::f(
            texto::vista_arranjo_resumo(),
            &[&fmt_num(a.momento(&pos), 0), &fmt_num(a.celula, 0)],
        ))
        .strong(),
    );
    ui.label(RichText::new(texto::vista_arranjo_ajuda()).weak());
    let mut mover: Option<(String, u32)> = None;
    let lado = 96.0;
    egui::Grid::new("grade")
        .spacing(vec2(4.0, 4.0))
        .show(ui, |ui| {
            for lin in 0..a.linhas {
                for col in 0..a.colunas {
                    let c = lin * a.colunas + col;
                    let aqui = a.maquinas.iter().find(|m| pos.get(&m.id) == Some(&c));
                    let frame = Frame::group(ui.style()).inner_margin(4.0);
                    let (resp, solto) = ui.dnd_drop_zone::<MaquinaArrastada, ()>(frame, |ui| {
                        ui.set_width(lado);
                        ui.set_min_height(48.0);
                        ui.label(
                            RichText::new(texto::f(texto::vista_celula_n(), &[&c]))
                                .weak()
                                .small(),
                        );
                        if let Some(m) = aqui {
                            if m.fixa {
                                ui.label(RichText::new(&m.nome).strong().color(AMARELO));
                            } else {
                                let sel = selecionada.as_deref() == Some(m.id.as_str());
                                let r = ui.dnd_drag_source(
                                    Id::new(("maquina", &m.id)),
                                    MaquinaArrastada(m.id.clone()),
                                    |ui| ui.add(egui::Button::new(&m.nome).selected(sel)),
                                );
                                if r.inner.clicked() {
                                    *selecionada = if sel { None } else { Some(m.id.clone()) };
                                }
                            }
                        }
                    });
                    if aqui.is_none()
                        && resp.response.interact(Sense::click()).clicked()
                        && let Some(m) = selecionada.take()
                    {
                        mover = Some((m, c));
                    }
                    if let Some(p) = solto {
                        mover = Some((p.0.clone(), c));
                    }
                }
                ui.end_row();
            }
        });
    if let Some((m, c)) = mover {
        // Dropping on an occupied cell swaps the two machines (the dock never moves).
        let ocupante = a.maquinas.iter().find(|x| pos.get(&x.id) == Some(&c));
        match ocupante {
            Some(o) if o.fixa => {}
            Some(o) if o.id != m => {
                if let Some(&antiga) = pos.get(&m) {
                    d.insert(Arranjo::chave(&o.id), antiga as f64);
                }
                d.insert(Arranjo::chave(&m), c as f64);
            }
            Some(_) => {}
            None => {
                d.insert(Arranjo::chave(&m), c as f64);
            }
        }
    }
    ui.collapsing(texto::vista_matriz_de_para(), |ui| {
        let dp = a.de_para(&pos);
        egui::Grid::new("depara").striped(true).show(ui, |ui| {
            for h in [
                texto::vista_col_de(),
                texto::vista_col_para(),
                texto::vista_col_viagens(),
                texto::vista_col_distancia(),
                texto::vista_col_momento(),
            ] {
                ui.label(RichText::new(h).strong());
            }
            ui.end_row();
            let nome = |id: &str| {
                a.maquinas
                    .iter()
                    .find(|m| m.id == id)
                    .map_or(id.to_string(), |m| m.nome.clone())
            };
            for ((x, y), (v, mom)) in &dp {
                ui.label(nome(x));
                ui.label(nome(y));
                ui.label(fmt_num(*v, 0));
                ui.label(fmt_num(a.distancia(pos[x], pos[y]), 1));
                ui.label(fmt_num(*mom, 0));
                ui.end_row();
            }
        });
    });
}
