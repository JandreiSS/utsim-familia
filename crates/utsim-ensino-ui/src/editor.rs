//! The mission editor of the teacher (`docs/plano_ensino_e_jogo.md`, 6.2): the teacher's
//! own package (open or create a folder, list, move and remove its missions, check it, zip
//! it as `.utse`) and the mission being edited (the model, the levers with ranges and
//! costs, the measures with goals, the statement), with the calibration in the background.
//! The rules are in [`utsim_ensino_core::editor`].

use std::path::PathBuf;

use egui::{Color32, RichText, Ui};
use utsim_ensino_core::editor::{Calibragem, Edicao, PacoteProfessor, calibrar, slug};
use utsim_ensino_core::missao::{Alavanca, Objetivo, Tolerancia};
use utsim_ensino_core::nota::fmt_num;
use utsim_ui_tema::arquivos::Tipo;

use crate::missao::Trabalho;
use crate::{App, texto};

const VERMELHO: Color32 = Color32::from_rgb(255, 112, 104);
const AMARELO: Color32 = Color32::from_rgb(240, 190, 80);

/// The editor's state.
#[derive(Default)]
pub struct EstadoEditor {
    /// Folder of the teacher's package, as typed.
    pub pasta: String,
    /// The package open and the files as they were last read or written (to remove the
    /// files of a mission taken out).
    pub pacote: Option<(
        PathBuf,
        PacoteProfessor,
        utsim_ensino_core::pacote::Arquivos,
    )>,
    /// New mission: the model and the title.
    pub modelo: String,
    pub titulo_novo: String,
    /// The mission being edited and where it goes.
    pub edicao: Option<Edicao>,
    pub ato: usize,
    pub trabalho: Option<Trabalho<Result<Calibragem, Vec<String>>>>,
    pub calibragem: Option<Calibragem>,
    pub mensagens: Vec<String>,
    pub erros: Vec<String>,
}

impl EstadoEditor {
    /// Takes the result of the calibration when it arrives.
    pub fn receber(&mut self) {
        if let Some(r) = self.trabalho.as_ref().and_then(|t| t.rx.try_recv().ok()) {
            self.trabalho = None;
            match r {
                Ok(c) => {
                    self.erros.clear();
                    self.calibragem = Some(c);
                }
                Err(e) => self.erros = e,
            }
        }
    }

    /// Opens the package of the typed folder, or starts a new one there.
    pub fn abrir_pasta(&mut self) {
        let dir = PathBuf::from(self.pasta.trim().trim_matches('"'));
        self.erros.clear();
        self.mensagens.clear();
        if dir.as_os_str().is_empty() {
            self.erros.push(texto::ed_escolha_pasta().into());
            return;
        }
        match utsim_ensino_store::caso::ler_pacote_professor(&dir) {
            Ok(Some(a)) => match PacoteProfessor::de_arquivos(a.clone()) {
                Ok(p) => {
                    self.mensagens.push(texto::f(
                        texto::ed_pacote_aberto(),
                        &[&p.pacote.titulo, &p.missoes().len()],
                    ));
                    self.pacote = Some((dir, p, a));
                }
                Err(e) => self.erros.push(e),
            },
            Ok(None) if std::fs::read_dir(&dir).is_ok_and(|mut r| r.next().is_some()) => {
                self.erros.push(texto::ed_pasta_ocupada().into());
            }
            Ok(None) => {
                let nome = dir
                    .file_name()
                    .map(|n| n.to_string_lossy().to_string())
                    .unwrap_or_else(|| texto::ed_meu_pacote().into());
                let p = PacoteProfessor::novo(&slug(&nome), &nome, "");
                self.mensagens
                    .push(texto::f(texto::ed_pacote_novo(), &[&dir.display()]));
                self.pacote = Some((dir, p, Default::default()));
            }
            Err(e) => self.erros.push(e.to_string()),
        }
    }

    /// Forgets the calibration running or done (the mission changed): a late result of
    /// another mission must not be applied to this one.
    fn esquecer_calibragem(&mut self) {
        if let Some(t) = self.trabalho.take() {
            t.cancelar.store(true, std::sync::atomic::Ordering::Relaxed);
        }
        self.calibragem = None;
    }

    /// Closes the mission being edited.
    pub fn fechar(&mut self) {
        self.esquecer_calibragem();
        self.edicao = None;
    }

    /// Starts the calibration of the mission in the background.
    pub fn calibrar(&mut self, executor: &dyn crate::Executor, ctx: Option<egui::Context>) {
        let Some(ed) = &self.edicao else { return };
        let copia = ed.clone();
        self.esquecer_calibragem();
        self.trabalho = Some(Trabalho::iniciar(executor, ctx, move |_| calibrar(&copia)));
    }

    /// Applies the reference and the suggested goals of the last calibration.
    pub fn aplicar(&mut self) {
        if let (Some(ed), Some(c)) = (&mut self.edicao, &self.calibragem) {
            ed.aplicar(c, true);
            self.mensagens = vec![texto::ed_aplicado().into()];
        }
    }

    /// Writes the mission to the package and the package to its folder.
    pub fn gravar_missao(&mut self) {
        let (Some(ed), Some((dir, p, _))) = (&mut self.edicao, &mut self.pacote) else {
            return;
        };
        let dir = dir.clone();
        match p.gravar(ed, self.ato) {
            Ok(c) => {
                if self.gravar_disco() {
                    self.erros.clear();
                    self.mensagens =
                        vec![texto::f(texto::ed_missao_gravada(), &[&c, &dir.display()])];
                }
            }
            Err(e) => self.erros = e,
        }
    }

    fn gravar_disco(&mut self) -> bool {
        let Some((dir, p, antes)) = &mut self.pacote else {
            return false;
        };
        let depois = p.todos();
        match utsim_ensino_store::caso::gravar_trocando(dir, antes, &depois) {
            Ok(()) => {
                *antes = depois;
                true
            }
            Err(e) => {
                self.erros.push(e.to_string());
                false
            }
        }
    }

    /// A draft mission from the typed model and title.
    pub fn nova_missao(&mut self) {
        self.erros.clear();
        let caminho = PathBuf::from(self.modelo.trim().trim_matches('"'));
        let titulo = if self.titulo_novo.trim().is_empty() {
            texto::turma_missao_nova().to_string()
        } else {
            self.titulo_novo.trim().to_string()
        };
        let r = std::fs::read_to_string(&caminho)
            .map_err(|e| format!("{}: {e}", caminho.display()))
            .and_then(|txt| Edicao::nova(&format!("prof/{}", slug(&titulo)), &titulo, &txt));
        match r {
            Ok(ed) => {
                if ed.alavancas.is_empty() {
                    self.erros.push(texto::ed_sem_alavancas().into());
                }
                self.ato = 0;
                self.esquecer_calibragem();
                self.edicao = Some(ed);
            }
            Err(e) => self.erros.push(e),
        }
    }
}

/// A column header of the tables, as in the opening screen.
fn cabecalho(ui: &mut Ui, t: &str) {
    let cor = ui.visuals().weak_text_color();
    ui.label(
        RichText::new(t.to_uppercase())
            .size(11.0)
            .strong()
            .color(cor),
    );
}

/// A section of the screen in a frame.
fn quadro(ui: &mut Ui, f: impl FnOnce(&mut Ui)) {
    let v = ui.visuals().clone();
    egui::Frame::new()
        .fill(v.faint_bg_color)
        .stroke(v.widgets.noninteractive.bg_stroke)
        .corner_radius(8)
        .inner_margin(14)
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            f(ui)
        });
}

/// The editor tab.
pub fn tela_editor(app: &mut App, ui: &mut Ui) {
    let ctx = ui.ctx().clone();
    let executor = app.executor.clone();
    let s = &mut app.editor;
    s.receber();
    ui.heading(texto::ed_titulo());
    ui.label(RichText::new(texto::ed_texto()).weak());
    ui.add_space(6.0);

    quadro(ui, |ui| secao_pacote(s, ui));
    ui.add_space(8.0);
    quadro(ui, |ui| secao_nova(s, ui));
    if s.edicao.is_some() {
        ui.add_space(8.0);
        quadro(ui, |ui| secao_missao(s, ui, executor.as_ref(), &ctx));
    }
    for m in &s.mensagens {
        ui.label(m);
    }
    for e in &s.erros {
        ui.colored_label(VERMELHO, e);
    }
}

fn secao_pacote(s: &mut EstadoEditor, ui: &mut Ui) {
    ui.label(RichText::new(texto::ed_pacote()).strong());
    ui.horizontal(|ui| {
        ui.label(texto::ed_pasta_pacote());
        crate::abertura::campo(
            ui,
            &mut s.pasta,
            Tipo::Pasta,
            texto::ed_pasta_pacote(),
            &[],
            360.0,
        );
        if ui.button(texto::ed_abrir_criar()).clicked() {
            s.abrir_pasta();
        }
    });
    let Some((dir, p, _)) = &mut s.pacote else {
        return;
    };
    let dir = dir.clone();
    ui.horizontal(|ui| {
        ui.label(texto::ed_titulo_pacote());
        ui.text_edit_singleline(&mut p.pacote.titulo);
        ui.label(texto::ed_autor());
        ui.text_edit_singleline(&mut p.pacote.autor);
    });
    let lista = p.missoes();
    let mut acao: Option<(String, i32)> = None;
    let mut abrir: Option<String> = None;
    if lista.is_empty() {
        ui.label(RichText::new(texto::ed_pacote_vazio()).weak());
    }
    egui::Grid::new("ed_missoes").striped(true).show(ui, |ui| {
        for it in &lista {
            ui.label(texto::f(texto::ed_ato_n(), &[&(it.ato + 1)]));
            ui.label(format!("{} ({})", it.titulo, it.id));
            if ui.small_button(texto::ed_editar()).clicked() {
                abrir = Some(it.caminho.clone());
            }
            if ui.small_button("↑").clicked() {
                acao = Some((it.caminho.clone(), -1));
            }
            if ui.small_button("↓").clicked() {
                acao = Some((it.caminho.clone(), 1));
            }
            if ui.small_button(texto::ed_remover()).clicked() {
                acao = Some((it.caminho.clone(), 0));
            }
            ui.end_row();
        }
    });
    let mut gravar = false;
    if let Some((c, passo)) = acao {
        if passo == 0 {
            p.remover(&c);
        } else {
            p.mover(&c, passo);
        }
        gravar = true;
    }
    if let Some(c) = abrir {
        match p.abrir(&c) {
            Ok(ed) => {
                s.ato = lista.iter().find(|x| x.caminho == c).map_or(0, |x| x.ato);
                s.esquecer_calibragem();
                s.edicao = Some(ed);
                s.erros.clear();
            }
            Err(e) => s.erros = vec![e],
        }
    }
    let mut conferir = false;
    let mut utse = false;
    ui.horizontal(|ui| {
        conferir = ui.button(texto::ed_conferir_pacote()).clicked();
        utse = ui.button(texto::ed_gravar_utse()).clicked();
        if ui.button(texto::ed_gravar_pasta()).clicked() {
            gravar = true;
        }
    });
    if gravar && s.gravar_disco() {
        s.mensagens = vec![texto::f(texto::ed_gravado(), &[&dir.display()])];
    }
    let Some((_, p, _)) = &s.pacote else { return };
    if conferir || utse {
        s.mensagens.clear();
        match p.conferir() {
            Ok(c) => {
                s.erros.clear();
                s.mensagens.push(texto::f(
                    texto::ed_pacote_confere(),
                    &[&c.missoes.len(), &c.pacote.atos.len()],
                ));
                if utse && s.gravar_disco() {
                    let destino = dir.with_extension("utse");
                    match utsim_ensino_store::caso::empacotar(&dir, &destino) {
                        Ok(d) => s
                            .mensagens
                            .push(texto::f(texto::turma_pacote_gravado(), &[&d.display()])),
                        Err(e) => s.erros.push(e.to_string()),
                    }
                }
            }
            Err(e) => s.erros = e,
        }
    }
}

fn secao_nova(s: &mut EstadoEditor, ui: &mut Ui) {
    ui.label(RichText::new(texto::ed_nova()).strong());
    egui::Grid::new("ed_nova")
        .num_columns(2)
        .spacing([12.0, 8.0])
        .show(ui, |ui| {
            ui.label(texto::turma_modelo());
            crate::abertura::campo(
                ui,
                &mut s.modelo,
                Tipo::Abrir,
                texto::turma_modelo(),
                &[utsim_ui_tema::arquivos::RON],
                360.0,
            );
            ui.end_row();
            ui.label(texto::turma_titulo_missao());
            ui.add(egui::TextEdit::singleline(&mut s.titulo_novo).desired_width(300.0));
            ui.end_row();
        });
    if ui.button(texto::ed_criar_rascunho()).clicked() {
        s.nova_missao();
    }
}

fn secao_missao(
    s: &mut EstadoEditor,
    ui: &mut Ui,
    executor: &dyn crate::Executor,
    ctx: &egui::Context,
) {
    let n_atos = s.pacote.as_ref().map_or(1, |p| p.1.pacote.atos.len());
    s.ato = s.ato.min(n_atos);
    let ocupado = s.trabalho.is_some();
    let Some(ed) = &mut s.edicao else { return };
    let gravada = ed.origem.is_some();
    ui.label(RichText::new(texto::ed_missao()).strong());
    egui::Grid::new("ed_cab")
        .num_columns(2)
        .spacing([12.0, 8.0])
        .show(ui, |ui| {
            ui.label(texto::turma_titulo_missao());
            ui.add(egui::TextEdit::singleline(&mut ed.base.titulo).desired_width(300.0));
            ui.end_row();
            ui.label(texto::ed_id());
            ui.add(egui::TextEdit::singleline(&mut ed.base.id).desired_width(300.0));
            ui.end_row();
            ui.label(texto::ed_ato());
            if gravada {
                ui.label(RichText::new(texto::ed_ato_fixo()).weak());
            } else {
                egui::ComboBox::from_id_salt("ed_ato")
                    .selected_text(if s.ato >= n_atos {
                        texto::ed_ato_novo().to_string()
                    } else {
                        texto::f(texto::ed_ato_n(), &[&(s.ato + 1)])
                    })
                    .show_ui(ui, |ui| {
                        for i in 0..n_atos {
                            ui.selectable_value(
                                &mut s.ato,
                                i,
                                texto::f(texto::ed_ato_n(), &[&(i + 1)]),
                            );
                        }
                        ui.selectable_value(&mut s.ato, n_atos, texto::ed_ato_novo());
                    });
            }
            ui.end_row();
            ui.label(texto::ed_orcamento());
            ui.horizontal(|ui| {
                let mut tem = ed.base.orcamento.is_some();
                if ui.checkbox(&mut tem, "").changed() {
                    ed.base.orcamento = tem.then_some(ed.base.orcamento.unwrap_or(10000.0));
                }
                if let Some(o) = &mut ed.base.orcamento {
                    ui.add(egui::DragValue::new(o).speed(100.0).range(0.0..=f64::MAX));
                }
            });
            ui.end_row();
        });

    ui.add_space(4.0);
    ui.label(RichText::new(texto::ed_alavancas()).strong());
    egui::Grid::new("ed_alavancas")
        .striped(true)
        .show(ui, |ui| {
            for h in [
                "",
                texto::ed_rotulo(),
                texto::ed_de(),
                texto::ed_ate(),
                texto::ed_passo(),
                texto::ed_custo(),
            ] {
                cabecalho(ui, h);
            }
            ui.end_row();
            for it in &mut ed.alavancas {
                ui.checkbox(&mut it.usar, "");
                match &mut it.alavanca {
                    Alavanca::Quantidade {
                        rotulo,
                        de,
                        ate,
                        custo_unitario,
                        ..
                    } => {
                        ui.text_edit_singleline(rotulo);
                        ui.add(egui::DragValue::new(de).range(0..=1000));
                        ui.add(egui::DragValue::new(ate).range(0..=1000));
                        ui.label("1");
                        ui.add(
                            egui::DragValue::new(custo_unitario)
                                .speed(100.0)
                                .range(0.0..=f64::MAX),
                        );
                    }
                    Alavanca::Valor {
                        rotulo,
                        de,
                        ate,
                        passo,
                        custo_unitario,
                        ..
                    } => {
                        ui.text_edit_singleline(rotulo);
                        ui.add(egui::DragValue::new(de).speed(0.1));
                        ui.add(egui::DragValue::new(ate).speed(0.1));
                        ui.add(
                            egui::DragValue::new(passo)
                                .speed(0.01)
                                .range(0.0..=f64::MAX),
                        );
                        ui.add(
                            egui::DragValue::new(custo_unitario)
                                .speed(100.0)
                                .range(0.0..=f64::MAX),
                        );
                    }
                    Alavanca::Escolha { rotulo, opcoes, .. } => {
                        ui.text_edit_singleline(rotulo);
                        ui.label(texto::f(texto::ed_opcoes(), &[&opcoes.len()]));
                        ui.label("");
                        ui.label("");
                        ui.label("");
                    }
                }
                ui.end_row();
            }
        });
    ui.label(RichText::new(texto::f(texto::ed_combinacoes(), &[&ed.combinacoes()])).weak());

    ui.add_space(4.0);
    ui.label(RichText::new(texto::ed_metricas()).strong());
    egui::Grid::new("ed_metricas").striped(true).show(ui, |ui| {
        for h in [
            "",
            texto::ed_rotulo(),
            texto::ed_melhor(),
            texto::ed_meta(),
            texto::ed_tolerancia(),
            texto::ed_peso(),
        ] {
            cabecalho(ui, h);
        }
        ui.end_row();
        for it in &mut ed.metricas {
            let m = &mut it.metrica;
            ui.checkbox(&mut it.usar, "");
            ui.text_edit_singleline(&mut m.rotulo);
            egui::ComboBox::from_id_salt(("ed_obj", m.nome.clone()))
                .selected_text(match m.objetivo {
                    Objetivo::Menor => texto::ed_menor(),
                    Objetivo::Maior => texto::ed_maior(),
                })
                .show_ui(ui, |ui| {
                    ui.selectable_value(&mut m.objetivo, Objetivo::Menor, texto::ed_menor());
                    ui.selectable_value(&mut m.objetivo, Objetivo::Maior, texto::ed_maior());
                });
            ui.horizontal(|ui| {
                let mut tem = m.meta.is_some();
                if ui.checkbox(&mut tem, "").changed() {
                    m.meta = tem.then_some(m.meta.unwrap_or(0.0));
                }
                if let Some(v) = &mut m.meta {
                    ui.add(
                        egui::DragValue::new(v)
                            .speed(0.1)
                            .max_decimals(m.casas as usize),
                    );
                }
            });
            ui.horizontal(|ui| {
                let (mut v, mut rel) = match m.tolerancia {
                    Tolerancia::Relativa(x) => (x * 100.0, true),
                    Tolerancia::Absoluta(x) => (x, false),
                };
                ui.add(
                    egui::DragValue::new(&mut v)
                        .speed(0.1)
                        .range(0.0..=f64::MAX),
                );
                ui.checkbox(&mut rel, "%");
                m.tolerancia = if rel {
                    Tolerancia::Relativa(v / 100.0)
                } else {
                    Tolerancia::Absoluta(v)
                };
            });
            ui.add(
                egui::DragValue::new(&mut m.peso)
                    .speed(0.1)
                    .range(0.0..=100.0),
            );
            ui.end_row();
        }
    });

    ui.add_space(4.0);
    ui.label(RichText::new(texto::ed_enunciado()).strong());
    ui.add(
        egui::TextEdit::multiline(&mut ed.texto)
            .desired_rows(8)
            .desired_width(f32::INFINITY)
            .code_editor(),
    );

    let erros = ed.erros();
    for e in &erros {
        ui.colored_label(AMARELO, e);
    }
    let mut fechar = false;
    let mut calibrar_agora = false;
    let mut aplicar = false;
    let mut gravar = false;
    ui.horizontal(|ui| {
        calibrar_agora = ui
            .add_enabled(
                !ocupado && erros.is_empty(),
                egui::Button::new(texto::ed_calibrar()),
            )
            .on_hover_text(texto::ed_calibrar_dica())
            .clicked();
        if let Some(t) = &s.trabalho {
            ui.spinner();
            ui.label(texto::ed_calibrando());
            let _ = t;
        }
        gravar = ui
            .add_enabled(
                erros.is_empty() && s.pacote.is_some(),
                utsim_ui_tema::widgets::primary(texto::ed_gravar_no_pacote()),
            )
            .on_hover_text(texto::ed_gravar_dica())
            .clicked();
        if ui.button(texto::ed_fechar()).clicked() {
            fechar = true;
        }
    });
    if let Some(c) = &s.calibragem {
        mostrar_calibragem(ui, c, s.edicao.as_ref());
        aplicar = ui.button(texto::ed_aplicar()).clicked();
    }
    if fechar {
        s.fechar();
        return;
    }
    if calibrar_agora {
        s.calibrar(executor, Some(ctx.clone()));
    }
    if aplicar {
        s.aplicar();
    }
    if gravar {
        s.gravar_missao();
    }
}

fn mostrar_calibragem(ui: &mut Ui, c: &Calibragem, ed: Option<&Edicao>) {
    let cf = &c.conferencia;
    ui.label(texto::f(
        texto::ed_resultado(),
        &[
            &cf.combinacoes,
            &cf.viaveis,
            &cf.batem_meta,
            &fmt_num(100.0 * c.fracao_meta(), 0),
        ],
    ));
    let rotulo = |nome: &str| {
        ed.and_then(|e| e.metricas.iter().find(|m| m.metrica.nome == nome))
            .map_or(nome.to_string(), |m| m.metrica.rotulo.clone())
    };
    egui::Grid::new("ed_calib").striped(true).show(ui, |ui| {
        for h in [
            texto::ed_metrica(),
            texto::ed_base(),
            texto::ed_referencia(),
            texto::ed_meta_sugerida(),
        ] {
            cabecalho(ui, h);
        }
        ui.end_row();
        for (nome, b) in &c.base {
            ui.label(rotulo(nome));
            ui.label(fmt_num(*b, 2));
            let r = c.referencia.iter().find(|x| &x.0 == nome).map(|x| x.1);
            ui.label(r.map_or("-".into(), |r| fmt_num(r, 2)));
            let m = c.metas.iter().find(|x| &x.0 == nome).map(|x| x.1);
            ui.label(m.map_or("-".into(), |m| fmt_num(m, 2)));
            ui.end_row();
        }
    });
    let decis: Vec<String> = cf
        .melhor
        .iter()
        .map(|(k, v)| format!("{k} = {}", fmt_num(*v, 2)))
        .collect();
    ui.label(texto::f(
        texto::ed_melhor_combinacao(),
        &[&decis.join(", ")],
    ));
    for e in &cf.erros {
        ui.colored_label(VERMELHO, e);
    }
    for a in &cf.avisos {
        ui.colored_label(AMARELO, a);
    }
}
