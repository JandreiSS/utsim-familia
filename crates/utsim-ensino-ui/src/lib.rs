//! The screens of UTSim Ensino in egui, with no backend (`docs/plano_ensino_e_jogo.md`,
//! 5.1): the host gives the `Ui` each frame and an [`Executor`] for the runs, so the same
//! screens can live in the eframe window now and in the Godot bridge later.
//!
//! - [`App`]: navigation, identity, campaign map, glossary, settings;
//! - [`missao`]: the mission screen (statement, prediction, levers, results, hints) and
//!   the sandbox;
//! - [`vista2d`]: the animated schematic view;
//! - [`turma`]: the student's class tab and the teacher's panel;
//! - [`texto`]: the text catalogue.

pub mod missao;
pub mod texto;
pub mod turma;
pub mod vista2d;

use std::sync::Arc;

use egui::{Color32, RichText, Ui};
use utsim_ensino_core::campanha::carregar_campanha;
use utsim_ensino_core::pacote::PacoteCarregado;
use utsim_ensino_core::progresso::Progresso;
use utsim_ensino_core::turma::{Aluno, registro_valido};
use utsim_ensino_store::perfil::{Config, Perfil};

/// Runs jobs away from the screen (a thread in the eframe app, inline in tests).
pub trait Executor: Send + Sync {
    fn executar(&self, job: Box<dyn FnOnce() + Send>);
}

/// One thread per job.
pub struct Fios;

impl Executor for Fios {
    fn executar(&self, job: Box<dyn FnOnce() + Send>) {
        std::thread::spawn(job);
    }
}

/// Runs the job right away (headless tests).
pub struct NaHora;

impl Executor for NaHora {
    fn executar(&self, job: Box<dyn FnOnce() + Send>) {
        job();
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tela {
    Identidade,
    Mapa,
    Missao,
    Areia,
    Glossario,
    Turma,
    Professor,
    Ajustes,
}

pub struct App {
    pub perfil: Perfil,
    pub config: Config,
    pub pacote: Option<Arc<PacoteCarregado>>,
    pub erro_pacote: Vec<String>,
    pub progresso: Progresso,
    pub tela: Tela,
    pub missao: Option<missao::EstadoMissao>,
    pub areia: Option<missao::EstadoAreia>,
    pub turma: turma::EstadoTurma,
    pub professor: turma::EstadoProfessor,
    pub executor: Arc<dyn Executor>,
    pub relogio: fn() -> i64,
    pub mensagem: Option<String>,
    nome: String,
    registro: String,
    apelido: String,
    estilo_aplicado: Option<f32>,
}

fn agora_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_millis() as i64)
}

impl App {
    pub fn new(perfil: Perfil) -> App {
        App::com(perfil, Arc::new(Fios), agora_ms)
    }

    pub fn com(perfil: Perfil, executor: Arc<dyn Executor>, relogio: fn() -> i64) -> App {
        let config = perfil.config();
        let (pacote, erro_pacote) = match carregar_campanha() {
            Ok(p) => (Some(Arc::new(p)), Vec::new()),
            Err(e) => (None, e),
        };
        let progresso = pacote
            .as_ref()
            .map(|p| perfil.progresso(&p.pacote.id))
            .unwrap_or_default();
        let tela = if config.identidade.is_some() {
            Tela::Mapa
        } else {
            Tela::Identidade
        };
        let ident = config.identidade.clone().unwrap_or(Aluno {
            nome: String::new(),
            registro: String::new(),
            apelido: String::new(),
        });
        App {
            perfil,
            pacote,
            erro_pacote,
            progresso,
            tela,
            missao: None,
            areia: None,
            turma: turma::EstadoTurma::default(),
            professor: turma::EstadoProfessor::default(),
            executor,
            relogio,
            mensagem: None,
            nome: ident.nome,
            registro: ident.registro,
            apelido: ident.apelido,
            estilo_aplicado: None,
            config,
        }
    }

    /// The student's record ("" before the identity).
    pub fn aluno(&self) -> String {
        self.config
            .identidade
            .as_ref()
            .map(|a| a.registro.clone())
            .unwrap_or_default()
    }

    /// Saves the identity typed in the first screen.
    pub fn salvar_identidade(&mut self, nome: &str, registro: &str, apelido: &str) -> bool {
        let (nome, registro) = (nome.trim(), registro.trim());
        if nome.len() < 2 || !registro_valido(registro) {
            self.mensagem = Some(texto::SEM_IDENTIDADE.into());
            return false;
        }
        self.config.identidade = Some(Aluno {
            nome: nome.into(),
            registro: registro.into(),
            apelido: if apelido.trim().is_empty() {
                registro.into()
            } else {
                apelido.trim().into()
            },
        });
        self.salvar_config();
        self.mensagem = None;
        self.tela = Tela::Mapa;
        true
    }

    pub fn salvar_config(&mut self) {
        if let Err(e) = self.perfil.salvar_config(&self.config) {
            self.mensagem = Some(format!("não foi possível salvar a configuração: {e}"));
        }
    }

    /// Opens mission `i` of the campaign.
    pub fn abrir_missao(&mut self, i: usize) {
        if let Some(p) = &self.pacote {
            let tentativas = p
                .missoes
                .get(i)
                .and_then(|m| self.progresso.registro(&m.missao.id))
                .map_or(0, |r| r.tentativas);
            self.missao = Some(missao::EstadoMissao::new(p.clone(), i, tentativas));
            self.tela = Tela::Missao;
        }
    }

    pub fn abrir_areia(&mut self, i: usize) {
        if let Some(p) = &self.pacote {
            self.areia = Some(missao::EstadoAreia::new(p.clone(), i));
            self.tela = Tela::Areia;
        }
    }

    fn aplicar_estilo(&mut self, ctx: &egui::Context) {
        let escala = self.config.escala_texto.unwrap_or(1.0).clamp(0.8, 1.6);
        if self.estilo_aplicado != Some(escala) {
            utsim_ui_tema::install(ctx);
            ctx.all_styles_mut(|s| {
                for f in s.text_styles.values_mut() {
                    f.size *= escala;
                }
            });
            self.estilo_aplicado = Some(escala);
        }
    }

    /// One frame.
    pub fn draw(&mut self, ui: &mut Ui) {
        self.aplicar_estilo(ui.ctx());
        self.receber();
        if self.tela == Tela::Identidade {
            egui::CentralPanel::default().show(ui, |ui| self.tela_identidade(ui));
            return;
        }
        egui::Panel::top("topo").show(ui, |ui| self.barra(ui));
        if let Some(m) = self.mensagem.clone() {
            egui::Panel::bottom("mensagem").show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.label(RichText::new(m).color(Color32::from_rgb(236, 196, 76)));
                    if ui.button("ok").clicked() {
                        self.mensagem = None;
                    }
                });
            });
        }
        egui::CentralPanel::default().show(ui, |ui| {
            egui::ScrollArea::vertical()
                .auto_shrink([false, false])
                .show(ui, |ui| match self.tela {
                    Tela::Mapa => self.tela_mapa(ui),
                    Tela::Missao => missao::tela_missao(self, ui),
                    Tela::Areia => missao::tela_areia(self, ui),
                    Tela::Glossario => self.tela_glossario(ui),
                    Tela::Turma => turma::tela_turma(self, ui),
                    Tela::Professor => turma::tela_professor(self, ui),
                    Tela::Ajustes => self.tela_ajustes(ui),
                    Tela::Identidade => {}
                });
        });
    }

    /// Results of background jobs.
    fn receber(&mut self) {
        if let Some(m) = self.missao.as_mut()
            && let Some(r) = m.receber()
        {
            missao::aplicar_rodada(self, r);
        }
        if let Some(a) = self.areia.as_mut() {
            a.receber();
        }
        self.professor.receber();
    }

    fn barra(&mut self, ui: &mut Ui) {
        ui.horizontal(|ui| {
            ui.label(RichText::new(texto::NOME_APP).strong().size(18.0));
            ui.separator();
            for (t, nome) in [
                (Tela::Mapa, "Campanha"),
                (Tela::Glossario, "Glossário"),
                (Tela::Turma, "Turma"),
            ] {
                if ui.selectable_label(self.tela == t, nome).clicked() {
                    self.tela = t;
                }
            }
            if self.config.professor
                && ui
                    .selectable_label(self.tela == Tela::Professor, "Professor")
                    .clicked()
            {
                self.tela = Tela::Professor;
            }
            if ui
                .selectable_label(self.tela == Tela::Ajustes, "Ajustes")
                .clicked()
            {
                self.tela = Tela::Ajustes;
            }
            if let Some(a) = &self.config.identidade {
                ui.separator();
                ui.label(format!("{} ({})", a.nome, a.registro));
            }
            ui.separator();
            ui.label(format!("★ {}", self.progresso.estrelas()));
        });
    }

    fn tela_identidade(&mut self, ui: &mut Ui) {
        ui.add_space(30.0);
        ui.vertical_centered(|ui| {
            ui.label(RichText::new(texto::NOME_APP).size(28.0).strong());
            ui.label(texto::SUBTITULO);
            ui.add_space(20.0);
            ui.label(RichText::new(texto::IDENTIDADE_TITULO).size(18.0));
            ui.label(texto::IDENTIDADE_TEXTO);
        });
        ui.add_space(10.0);
        egui::Grid::new("identidade").num_columns(2).show(ui, |ui| {
            ui.label("Nome");
            ui.text_edit_singleline(&mut self.nome);
            ui.end_row();
            ui.label("Registro acadêmico");
            ui.text_edit_singleline(&mut self.registro);
            ui.end_row();
            ui.label("Apelido no placar (opcional)");
            ui.text_edit_singleline(&mut self.apelido);
            ui.end_row();
        });
        if let Some(m) = &self.mensagem {
            ui.colored_label(Color32::from_rgb(255, 112, 104), m);
        }
        if ui.button("Começar").clicked() {
            let (n, r, a) = (
                self.nome.clone(),
                self.registro.clone(),
                self.apelido.clone(),
            );
            self.salvar_identidade(&n, &r, &a);
        }
    }

    fn tela_mapa(&mut self, ui: &mut Ui) {
        let Some(p) = self.pacote.clone() else {
            ui.colored_label(Color32::from_rgb(255, 112, 104), "A campanha não carregou:");
            for e in &self.erro_pacote {
                ui.label(e);
            }
            return;
        };
        ui.heading(&p.pacote.titulo);
        ui.label(&p.pacote.descricao);
        ui.add_space(8.0);
        let mut abrir = None;
        let mut areia = None;
        for ato in &p.pacote.atos {
            ui.add_space(6.0);
            ui.label(
                RichText::new(format!("Ato {}. {}", ato.numero, ato.titulo))
                    .size(17.0)
                    .strong(),
            );
            for (i, m) in p
                .missoes
                .iter()
                .enumerate()
                .filter(|(_, m)| m.ato == ato.numero)
            {
                let liberada = self.progresso.liberada(&p, i);
                let reg = self.progresso.registro(&m.missao.id);
                ui.horizontal(|ui| {
                    let estrelas = reg.map_or(0, |r| r.estrelas);
                    let marca = format!(
                        "{}{}",
                        "★".repeat(estrelas as usize),
                        "☆".repeat(3 - estrelas.min(3) as usize)
                    );
                    ui.label(RichText::new(marca).monospace());
                    let titulo = format!("{} {}", numero_missao(&p, i), m.missao.titulo);
                    if ui
                        .add_enabled(liberada, egui::Button::new(titulo))
                        .clicked()
                    {
                        abrir = Some(i);
                    }
                    if !liberada {
                        ui.label(RichText::new("bloqueada: resolva a anterior").weak());
                    }
                    if let Some(r) = reg {
                        ui.label(format!(
                            "melhor nota {:.1} em {} tentativa(s)",
                            r.melhor_nota, r.tentativas
                        ));
                        if r.resolvida && ui.button("Caixa de areia").clicked() {
                            areia = Some(i);
                        }
                    }
                });
            }
        }
        if let Some(i) = abrir {
            self.abrir_missao(i);
        }
        if let Some(i) = areia {
            self.abrir_areia(i);
        }
    }

    fn tela_glossario(&mut self, ui: &mut Ui) {
        ui.heading("Glossário");
        ui.label("Cada conceito aparece quando uma missão o usa. O texto completo está no manual.");
        let Some(p) = self.pacote.clone() else { return };
        for t in &p.glossario.termos {
            let usado: Vec<String> = p
                .missoes
                .iter()
                .enumerate()
                .filter(|(_, m)| m.missao.conceitos.contains(&t.id))
                .map(|(i, _)| numero_missao(&p, i))
                .collect();
            ui.add_space(4.0);
            ui.label(RichText::new(&t.nome).strong());
            ui.label(&t.texto);
            let mut rodape = String::new();
            if !usado.is_empty() {
                rodape.push_str(&format!("Missões: {}. ", usado.join(", ")));
            }
            if let Some(m) = &t.manual {
                rodape.push_str(&format!("Manual: {m}"));
            }
            if !rodape.is_empty() {
                ui.label(RichText::new(rodape).weak());
            }
        }
    }

    fn tela_ajustes(&mut self, ui: &mut Ui) {
        ui.heading("Ajustes");
        let mut escala = self.config.escala_texto.unwrap_or(1.0);
        ui.horizontal(|ui| {
            ui.label("Tamanho da letra");
            if ui
                .add(egui::Slider::new(&mut escala, 0.8..=1.6).step_by(0.1))
                .changed()
            {
                self.config.escala_texto = Some(escala);
                self.salvar_config();
            }
        });
        ui.add_space(8.0);
        let mut prof = self.config.professor;
        if ui
            .checkbox(
                &mut prof,
                "Modo professor (turmas, correção em lote, matriz da turma)",
            )
            .changed()
        {
            self.config.professor = prof;
            if prof && let Err(e) = self.perfil.chave_professor() {
                self.mensagem = Some(format!("não foi possível criar a chave do professor: {e}"));
            }
            self.salvar_config();
        }
        ui.add_space(8.0);
        ui.label(format!("Pasta do perfil: {}", self.perfil.dir.display()));
        if ui.button("Trocar de aluno").clicked() {
            self.tela = Tela::Identidade;
        }
        ui.add_space(8.0);
        ui.label(RichText::new(format!("Núcleo {}", utsim_ensino_core::VERSAO_NUCLEO)).weak());
        if let Some(p) = &self.pacote {
            ui.label(
                RichText::new(format!(
                    "Pacote {} {} ({})",
                    p.pacote.id,
                    p.pacote.versao,
                    &p.hash[..12]
                ))
                .weak(),
            );
        }
    }
}

/// `1.2`: act and position of mission `i` inside the act.
pub fn numero_missao(p: &PacoteCarregado, i: usize) -> String {
    let ato = p.missoes[i].ato;
    let pos = p.missoes[..=i].iter().filter(|m| m.ato == ato).count();
    format!("{ato}.{pos}")
}
