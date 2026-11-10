//! The mission editor of the teacher (`docs/plano_ensino_e_jogo.md`, 6.2): the teacher's
//! own package (open or create a folder, list, move and remove its missions, check it, zip
//! it as `.utse`) and the mission being edited (the model, the levers with ranges and
//! costs, the measures with goals, the statement), with the calibration in the background.
//! The rules are in [`utsim_ensino_core::editor`].

use std::collections::BTreeMap;
use std::path::PathBuf;

use egui::{Color32, RichText, Ui};
use utsim_ensino_core::editor::{
    Calibragem, Edicao, PacoteProfessor, calibrar, chaves_do_modelo, glossario_da_campanha, slug,
};
use utsim_ensino_core::missao::{
    Alavanca, Calculo, Medicao, Objetivo, OpcaoPrevisao, Pergunta, Previsao, Resposta, Termo,
    Tolerancia,
};
use utsim_ensino_core::nota::fmt_num;
use utsim_ensino_core::pacote::Glossario;
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
    /// The metric keys of the model of the mission (one replication, run when it opens).
    pub chaves: Vec<String>,
    /// The glossary the concepts come from, read when first drawn.
    pub glossario: Option<Glossario>,
    /// Show the statement as the student sees it.
    pub previa: bool,
    /// The times of the calculations as typed, by question.
    pub tempos: BTreeMap<usize, String>,
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
                    self.glossario = None;
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
                self.glossario = None;
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
        self.definir_edicao(None);
    }

    /// Replaces the mission being edited and forgets what belonged to the old one.
    fn definir_edicao(&mut self, ed: Option<Edicao>) {
        self.esquecer_calibragem();
        self.chaves.clear();
        self.glossario = None;
        self.previa = false;
        self.tempos.clear();
        if let Some(ed) = &ed {
            match chaves_do_modelo(&ed.modelo) {
                Ok(c) => self.chaves = c,
                Err(e) => self.erros.push(e),
            }
        }
        self.edicao = ed;
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
                self.definir_edicao(Some(ed));
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
                s.erros.clear();
                s.definir_edicao(Some(ed));
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
    if s.glossario.is_none() {
        s.glossario = Some(
            s.pacote
                .as_ref()
                .map_or_else(glossario_da_campanha, |p| p.1.glossario()),
        );
    }
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

    let mut chaves = s.chaves.clone();
    chaves.extend(ed.chaves_de_decisao());
    secao_enunciado(ui, ed, &mut s.previa);
    if let Some(g) = &s.glossario {
        secao_conceitos(ui, ed, g);
    }
    secao_medicoes(ui, ed, &chaves);
    secao_previsoes(ui, ed, &chaves, &mut s.tempos);
    secao_justificativa(ui, ed);
    secao_dicas(ui, ed);

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

/// The statement and, below the editor, the statement as the student sees it.
fn secao_enunciado(ui: &mut Ui, ed: &mut Edicao, previa: &mut bool) {
    ui.add_space(4.0);
    ui.horizontal(|ui| {
        ui.label(RichText::new(texto::ed_enunciado()).strong());
        ui.checkbox(previa, texto::ed_previa());
    });
    ui.add(
        egui::TextEdit::multiline(&mut ed.texto)
            .desired_rows(8)
            .desired_width(f32::INFINITY)
            .code_editor(),
    );
    if *previa {
        let v = ui.visuals().clone();
        egui::Frame::new()
            .fill(v.extreme_bg_color)
            .stroke(v.widgets.noninteractive.bg_stroke)
            .corner_radius(6)
            .inner_margin(10)
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                texto::markdown(ui, &ed.texto);
            });
    }
}

/// The glossary concepts the mission names.
fn secao_conceitos(ui: &mut Ui, ed: &mut Edicao, g: &Glossario) {
    ui.add_space(4.0);
    egui::CollapsingHeader::new(RichText::new(texto::f(
        texto::ed_conceitos(),
        &[&ed.base.conceitos.len()],
    )))
    .id_salt("ed_conceitos")
    .show(ui, |ui| {
        egui::ScrollArea::vertical()
            .max_height(160.0)
            .id_salt("ed_conceitos_rolagem")
            .show(ui, |ui| {
                ui.horizontal_wrapped(|ui| {
                    for t in &g.termos {
                        let pos = ed.base.conceitos.iter().position(|c| c == &t.id);
                        let mut marcado = pos.is_some();
                        if ui
                            .checkbox(&mut marcado, &t.nome)
                            .on_hover_text(&t.texto)
                            .changed()
                        {
                            match pos {
                                Some(i) => {
                                    ed.base.conceitos.remove(i);
                                }
                                None => ed.base.conceitos.push(t.id.clone()),
                            }
                        }
                    }
                });
            });
        let mut tirar = None;
        for (i, c) in ed.base.conceitos.iter().enumerate() {
            if !g.termos.iter().any(|t| &t.id == c) {
                ui.horizontal(|ui| {
                    ui.colored_label(AMARELO, texto::f(texto::ed_conceito_fora(), &[c]));
                    if ui.small_button(texto::ed_remover()).clicked() {
                        tirar = Some(i);
                    }
                });
            }
        }
        if let Some(i) = tirar {
            ed.base.conceitos.remove(i);
        }
    });
}

/// A text field for a key with a list of the known ones, filtered by what is typed.
/// Returns true when the text changed.
fn campo_chave(ui: &mut Ui, id_salt: &str, chave: &mut String, chaves: &[String]) -> bool {
    ui.horizontal(|ui| {
        let mut mudou = ui
            .add(egui::TextEdit::singleline(chave).desired_width(220.0))
            .changed();
        let mut escolhida: Option<String> = None;
        egui::ComboBox::from_id_salt(id_salt)
            .selected_text("▾")
            .width(20.0)
            .show_ui(ui, |ui| {
                let filtro = chave.to_lowercase();
                let todas: Vec<&String> = chaves
                    .iter()
                    .filter(|k| filtro.is_empty() || k.to_lowercase().contains(&filtro))
                    .collect();
                for k in todas.iter().take(40) {
                    if ui
                        .selectable_label(k.as_str() == chave, k.as_str())
                        .clicked()
                    {
                        escolhida = Some((*k).clone());
                    }
                }
                if todas.len() > 40 {
                    ui.label(
                        RichText::new(texto::f(texto::ed_chave_mais(), &[&(todas.len() - 40)]))
                            .weak(),
                    );
                }
            });
        if let Some(k) = escolhida {
            *chave = k;
            mudou = true;
        }
        mudou
    })
    .inner
}

/// The key (and the factor) of a value made of terms. A formula of several terms is kept
/// as it is.
fn campo_termos(
    ui: &mut Ui,
    id_salt: &str,
    termos: &mut Vec<Termo>,
    chaves: &[String],
    com_fator: bool,
) {
    if termos.len() > 1 {
        ui.label(RichText::new(texto::f(texto::ed_formula_n(), &[&termos.len()])).weak());
        return;
    }
    let mut c = termos.first().map(|t| t.chave.clone()).unwrap_or_default();
    if campo_chave(ui, id_salt, &mut c, chaves) {
        if c.is_empty() {
            termos.clear();
        } else if let Some(t) = termos.first_mut() {
            t.chave = c;
        } else {
            termos.push(Termo {
                chave: c,
                fator: 1.0,
            });
        }
    }
    if com_fator && let Some(t) = termos.first_mut() {
        ui.label("×");
        ui.add(egui::DragValue::new(&mut t.fator).speed(0.1));
    }
}

/// A tolerance: relative (shown in %) or absolute.
fn campo_tolerancia(ui: &mut Ui, id_salt: &str, tol: &mut Tolerancia) {
    let (mut v, mut rel) = match *tol {
        Tolerancia::Relativa(x) => (x * 100.0, true),
        Tolerancia::Absoluta(x) => (x, false),
    };
    egui::ComboBox::from_id_salt(id_salt)
        .selected_text(if rel {
            texto::ed_tol_relativa()
        } else {
            texto::ed_tol_absoluta()
        })
        .show_ui(ui, |ui| {
            ui.selectable_value(&mut rel, true, texto::ed_tol_relativa());
            ui.selectable_value(&mut rel, false, texto::ed_tol_absoluta());
        });
    ui.add(
        egui::DragValue::new(&mut v)
            .speed(0.1)
            .range(0.0..=f64::MAX),
    );
    *tol = if rel {
        Tolerancia::Relativa(v / 100.0)
    } else {
        Tolerancia::Absoluta(v)
    };
}

/// The measurements shown to the student before the prediction.
fn secao_medicoes(ui: &mut Ui, ed: &mut Edicao, chaves: &[String]) {
    ui.add_space(4.0);
    ui.label(RichText::new(texto::ed_medicoes()).strong());
    let mut tirar = None;
    if !ed.base.medicoes.is_empty() {
        egui::Grid::new("ed_medicoes")
            .striped(true)
            .spacing([12.0, 8.0])
            .show(ui, |ui| {
                for h in [
                    texto::ed_rotulo(),
                    texto::ed_chave(),
                    texto::ed_unidade(),
                    texto::ed_casas(),
                    "",
                ] {
                    cabecalho(ui, h);
                }
                ui.end_row();
                for (i, m) in ed.base.medicoes.iter_mut().enumerate() {
                    ui.add(egui::TextEdit::singleline(&mut m.rotulo).desired_width(160.0));
                    ui.horizontal(|ui| {
                        campo_termos(ui, &format!("ed_med_chave_{i}"), &mut m.valor, chaves, true);
                    });
                    ui.add(egui::TextEdit::singleline(&mut m.unidade).desired_width(70.0));
                    ui.add(egui::DragValue::new(&mut m.casas).range(0..=6));
                    if ui.small_button(texto::ed_remover()).clicked() {
                        tirar = Some(i);
                    }
                    ui.end_row();
                }
            });
    }
    if let Some(i) = tirar {
        ed.base.medicoes.remove(i);
    }
    if ui.button(texto::ed_add_medicao()).clicked() {
        ed.base.medicoes.push(Medicao {
            rotulo: String::new(),
            valor: Vec::new(),
            unidade: String::new(),
            casas: 2,
        });
    }
}

enum Passo {
    Subir(usize),
    Descer(usize),
    Tirar(usize),
}

fn calculo_padrao(i: usize) -> Calculo {
    match i {
        0 => Calculo::MaquinasPorOperador {
            a: 0.5,
            b: 0.2,
            t: 2.0,
        },
        1 => Calculo::TempoNormal {
            tempos: vec![1.0, 1.1, 0.9],
            ritmo: 1.0,
        },
        2 => Calculo::TempoPadrao {
            tempos: vec![1.0, 1.1, 0.9],
            ritmo: 1.0,
            tolerancia: 0.1,
        },
        _ => Calculo::Ciclos {
            tempos: vec![1.0, 1.2, 0.9, 1.1],
            erro: 0.05,
        },
    }
}

fn calculo_indice(c: &Calculo) -> usize {
    match c {
        Calculo::MaquinasPorOperador { .. } => 0,
        Calculo::TempoNormal { .. } => 1,
        Calculo::TempoPadrao { .. } => 2,
        Calculo::Ciclos { .. } => 3,
    }
}

fn calculo_nome(i: usize) -> &'static str {
    match i {
        0 => texto::ed_calc_abt(),
        1 => texto::ed_calc_normal(),
        2 => texto::ed_calc_padrao(),
        _ => texto::ed_calc_ciclos(),
    }
}

/// Numbers typed separated by spaces or semicolons (a comma is a decimal mark).
fn ler_tempos(txt: &str) -> Vec<f64> {
    txt.split(|c: char| c.is_whitespace() || c == ';')
        .filter_map(|x| x.replace(',', ".").parse::<f64>().ok())
        .collect()
}

fn campo_numero(ui: &mut Ui, rotulo: &str, v: &mut f64, velocidade: f64) {
    ui.label(rotulo);
    ui.add(egui::DragValue::new(v).speed(velocidade));
}

/// The recipe of a calculation question and its answer.
fn campo_calculo(
    ui: &mut Ui,
    i: usize,
    calculo: &mut Calculo,
    tempos: &mut BTreeMap<usize, String>,
) {
    let atual = calculo_indice(calculo);
    let mut novo = atual;
    egui::ComboBox::from_id_salt(("ed_calc_receita", i))
        .selected_text(calculo_nome(atual))
        .show_ui(ui, |ui| {
            for k in 0..4 {
                ui.selectable_value(&mut novo, k, calculo_nome(k));
            }
        });
    if novo != atual {
        *calculo = calculo_padrao(novo);
        tempos.remove(&i);
    }
    match calculo {
        Calculo::MaquinasPorOperador { a, b, t } => {
            ui.horizontal(|ui| {
                campo_numero(ui, texto::ed_calc_a(), a, 0.05);
                campo_numero(ui, texto::ed_calc_b(), b, 0.05);
                campo_numero(ui, texto::ed_calc_t(), t, 0.05);
            });
        }
        Calculo::TempoNormal { tempos: ts, ritmo } => {
            campo_tempos(ui, i, ts, tempos);
            ui.horizontal(|ui| campo_numero(ui, texto::ed_ritmo(), ritmo, 0.01));
        }
        Calculo::TempoPadrao {
            tempos: ts,
            ritmo,
            tolerancia,
        } => {
            campo_tempos(ui, i, ts, tempos);
            ui.horizontal(|ui| {
                campo_numero(ui, texto::ed_ritmo(), ritmo, 0.01);
                campo_numero(ui, texto::ed_folgas(), tolerancia, 0.01);
            });
        }
        Calculo::Ciclos { tempos: ts, erro } => {
            campo_tempos(ui, i, ts, tempos);
            ui.horizontal(|ui| campo_numero(ui, texto::ed_erro_rel(), erro, 0.005));
        }
    }
    ui.label(texto::f(
        texto::ed_calc_resposta(),
        &[&fmt_num(calculo.valor(), 2)],
    ));
}

/// The observed times as a text, parsed on every change (the text is kept in `buffer`).
fn campo_tempos(ui: &mut Ui, i: usize, ts: &mut Vec<f64>, buffer: &mut BTreeMap<usize, String>) {
    let txt = buffer.entry(i).or_insert_with(|| {
        ts.iter()
            .map(|x| fmt_num(*x, 3))
            .collect::<Vec<_>>()
            .join(" ")
    });
    ui.label(texto::ed_tempos());
    if ui
        .add(egui::TextEdit::singleline(txt).desired_width(f32::INFINITY))
        .changed()
    {
        *ts = ler_tempos(txt);
    }
}

/// The prediction questions.
fn secao_previsoes(
    ui: &mut Ui,
    ed: &mut Edicao,
    chaves: &[String],
    tempos: &mut BTreeMap<usize, String>,
) {
    ui.add_space(4.0);
    ui.label(RichText::new(texto::ed_previsoes()).strong());
    let n = ed.base.previsao.len();
    let mut passo: Option<Passo> = None;
    for (i, p) in ed.base.previsao.iter_mut().enumerate() {
        egui::Frame::group(ui.style()).show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal(|ui| {
                let tipo = match p {
                    Previsao::Escolha { .. } => texto::ed_pv_escolha(),
                    Previsao::Valor { .. } => texto::ed_pv_valor(),
                    Previsao::Calculo { .. } => texto::ed_pv_calculo(),
                    Previsao::Dados { .. } => "",
                };
                cabecalho(ui, tipo);
                let (Previsao::Escolha { id, .. }
                | Previsao::Valor { id, .. }
                | Previsao::Calculo { id, .. }
                | Previsao::Dados { id, .. }) = p;
                ui.label(texto::ed_id());
                ui.add(egui::TextEdit::singleline(id).desired_width(80.0));
                if ui
                    .add_enabled(i > 0, egui::Button::new("↑").small())
                    .clicked()
                {
                    passo = Some(Passo::Subir(i));
                }
                if ui
                    .add_enabled(i + 1 < n, egui::Button::new("↓").small())
                    .clicked()
                {
                    passo = Some(Passo::Descer(i));
                }
                if ui.small_button(texto::ed_remover()).clicked() {
                    passo = Some(Passo::Tirar(i));
                }
            });
            match p {
                Previsao::Dados { pergunta, .. } => {
                    ui.label(RichText::new(texto::ed_pv_dados()).weak());
                    ui.label(&*pergunta);
                }
                Previsao::Escolha {
                    pergunta,
                    opcoes,
                    resposta,
                    ..
                } => {
                    pergunta_campo(ui, pergunta);
                    escolha(ui, i, opcoes, resposta, chaves);
                }
                Previsao::Valor {
                    pergunta,
                    valor,
                    unidade,
                    tolerancia,
                    ..
                } => {
                    pergunta_campo(ui, pergunta);
                    ui.horizontal(|ui| {
                        ui.label(texto::ed_chave());
                        campo_termos(ui, &format!("ed_pv_chave_{i}"), valor, chaves, true);
                    });
                    ui.horizontal(|ui| {
                        ui.label(texto::ed_unidade());
                        ui.add(egui::TextEdit::singleline(unidade).desired_width(70.0));
                        ui.label(texto::ed_tolerancia());
                        campo_tolerancia(ui, &format!("ed_pv_tol_{i}"), tolerancia);
                    });
                }
                Previsao::Calculo {
                    pergunta,
                    calculo,
                    unidade,
                    tolerancia,
                    ..
                } => {
                    pergunta_campo(ui, pergunta);
                    campo_calculo(ui, i, calculo, tempos);
                    ui.horizontal(|ui| {
                        ui.label(texto::ed_unidade());
                        ui.add(egui::TextEdit::singleline(unidade).desired_width(70.0));
                        ui.label(texto::ed_tolerancia());
                        campo_tolerancia(ui, &format!("ed_pv_tol_{i}"), tolerancia);
                    });
                }
            }
        });
    }
    match passo {
        Some(Passo::Subir(i)) => ed.base.previsao.swap(i, i - 1),
        Some(Passo::Descer(i)) => ed.base.previsao.swap(i, i + 1),
        Some(Passo::Tirar(i)) => {
            ed.base.previsao.remove(i);
        }
        None => {}
    }
    if passo.is_some() {
        tempos.clear();
    }
    ui.horizontal(|ui| {
        if ui.button(texto::ed_add_escolha()).clicked() {
            let id = ed.id_previsao_livre();
            ed.base.previsao.push(Previsao::Escolha {
                id,
                pergunta: String::new(),
                opcoes: vec![opcao_vazia(), opcao_vazia()],
                resposta: Resposta::Fixa(0),
            });
        }
        if ui.button(texto::ed_add_valor()).clicked() {
            let id = ed.id_previsao_livre();
            ed.base.previsao.push(Previsao::Valor {
                id,
                pergunta: String::new(),
                valor: Vec::new(),
                unidade: String::new(),
                tolerancia: Tolerancia::Relativa(0.05),
            });
        }
        if ui.button(texto::ed_add_calculo()).clicked() {
            let id = ed.id_previsao_livre();
            ed.base.previsao.push(Previsao::Calculo {
                id,
                pergunta: String::new(),
                calculo: calculo_padrao(0),
                unidade: String::new(),
                tolerancia: Tolerancia::Relativa(0.02),
            });
        }
    });
}

fn opcao_vazia() -> OpcaoPrevisao {
    OpcaoPrevisao {
        rotulo: String::new(),
        valor: Vec::new(),
    }
}

fn pergunta_campo(ui: &mut Ui, pergunta: &mut String) {
    ui.label(texto::ed_pergunta());
    ui.add(
        egui::TextEdit::multiline(pergunta)
            .desired_rows(2)
            .desired_width(f32::INFINITY),
    );
}

/// The options of a choice question and the way its answer is decided.
fn escolha(
    ui: &mut Ui,
    i: usize,
    opcoes: &mut Vec<OpcaoPrevisao>,
    resposta: &mut Resposta,
    chaves: &[String],
) {
    let mut tirar = None;
    for (j, o) in opcoes.iter_mut().enumerate() {
        ui.horizontal(|ui| {
            ui.label(texto::f(texto::ed_opcao(), &[&(j + 1)]));
            ui.add(egui::TextEdit::singleline(&mut o.rotulo).desired_width(200.0));
            campo_termos(
                ui,
                &format!("ed_pv_{i}_op_{j}"),
                &mut o.valor,
                chaves,
                false,
            );
            if ui.small_button(texto::ed_remover()).clicked() {
                tirar = Some(j);
            }
        });
    }
    if let Some(j) = tirar {
        opcoes.remove(j);
        if let Resposta::Fixa(k) = resposta {
            if j < *k {
                *k -= 1;
            }
            *k = (*k).min(opcoes.len().saturating_sub(1));
        }
    }
    ui.horizontal(|ui| {
        if ui.button(texto::ed_add_opcao()).clicked() {
            opcoes.push(opcao_vazia());
        }
        ui.label(texto::ed_resposta());
        let atual = match *resposta {
            Resposta::Maior => texto::ed_resp_maior().to_string(),
            Resposta::Menor => texto::ed_resp_menor().to_string(),
            Resposta::Fixa(k) => texto::f(texto::ed_resp_fixa(), &[&(k + 1)]),
        };
        egui::ComboBox::from_id_salt(("ed_pv_resposta", i))
            .selected_text(atual)
            .show_ui(ui, |ui| {
                for k in 0..opcoes.len() {
                    ui.selectable_value(
                        resposta,
                        Resposta::Fixa(k),
                        texto::f(texto::ed_resp_fixa(), &[&(k + 1)]),
                    );
                }
                ui.selectable_value(resposta, Resposta::Maior, texto::ed_resp_maior());
                ui.selectable_value(resposta, Resposta::Menor, texto::ed_resp_menor());
            });
    });
}

/// The objective questions about the why.
fn secao_justificativa(ui: &mut Ui, ed: &mut Edicao) {
    ui.add_space(4.0);
    ui.label(RichText::new(texto::ed_justificativa()).strong());
    let mut tirar = None;
    for (i, q) in ed.base.justificativa.iter_mut().enumerate() {
        egui::Frame::group(ui.style()).show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal(|ui| {
                ui.label(texto::ed_pergunta());
                ui.add(egui::TextEdit::singleline(&mut q.pergunta).desired_width(400.0));
                if ui.small_button(texto::ed_remover()).clicked() {
                    tirar = Some(i);
                }
            });
            let mut sai = None;
            for (j, o) in q.opcoes.iter_mut().enumerate() {
                ui.horizontal(|ui| {
                    ui.radio_value(&mut q.certa, j, "")
                        .on_hover_text(texto::ed_certa());
                    ui.add(egui::TextEdit::singleline(o).desired_width(400.0));
                    if ui.small_button(texto::ed_remover()).clicked() {
                        sai = Some(j);
                    }
                });
            }
            if let Some(j) = sai {
                q.opcoes.remove(j);
                if j < q.certa {
                    q.certa -= 1;
                }
                q.certa = q.certa.min(q.opcoes.len().saturating_sub(1));
            }
            if ui.button(texto::ed_add_opcao()).clicked() {
                q.opcoes.push(String::new());
            }
        });
    }
    if let Some(i) = tirar {
        ed.base.justificativa.remove(i);
    }
    if ui.button(texto::ed_add_justificativa()).clicked() {
        ed.base.justificativa.push(Pergunta {
            pergunta: String::new(),
            opcoes: vec![String::new(), String::new()],
            certa: 0,
        });
    }
}

/// The three hint levels.
fn secao_dicas(ui: &mut Ui, ed: &mut Edicao) {
    ui.add_space(4.0);
    ui.label(RichText::new(texto::ed_dicas()).strong());
    let rotulos = [texto::ed_dica_1(), texto::ed_dica_2(), texto::ed_dica_3()];
    let mut tirar = None;
    for (i, d) in ed.base.dicas.iter_mut().enumerate() {
        ui.horizontal(|ui| {
            ui.label(rotulos[i.min(2)]);
            ui.add(egui::TextEdit::singleline(d).desired_width(400.0));
            if ui.small_button(texto::ed_remover()).clicked() {
                tirar = Some(i);
            }
        });
    }
    if let Some(i) = tirar {
        ed.base.dicas.remove(i);
    }
    if ed.base.dicas.len() < 3 && ui.button(texto::ed_add_dica()).clicked() {
        ed.base.dicas.push(String::new());
    }
}
