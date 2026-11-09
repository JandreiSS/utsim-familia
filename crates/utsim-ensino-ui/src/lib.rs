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
pub mod pecas;
pub mod texto;
pub mod turma;
pub mod vista2d;

use std::path::{Path, PathBuf};
use std::sync::Arc;

use egui::{Color32, RichText, Ui};
use utsim_ensino_core::campanha::carregar_campanha;
use utsim_ensino_core::pacote::{PacoteCarregado, carregar};
use utsim_ensino_core::progresso::Progresso;
use utsim_ensino_core::turma::{Aluno, registro_valido};
use utsim_ensino_store::perfil::{Config, Perfil};

/// The color of UTSim Ensino in the family (`#8A5CE6`, the launcher icon).
pub const ROXO: Color32 = Color32::from_rgb(0x8A, 0x5C, 0xE6);
/// Warnings that do not stop anything (a need of the package this version does not meet).
pub const AVISO: Color32 = Color32::from_rgb(240, 190, 80);

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
    /// Folder or `.utse` typed in the map to open another package.
    pub caminho_pacote: String,
    /// Packages installed beside the program (`../pacotes` from the executable).
    pub pacotes_instalados: Vec<utsim_ensino_store::arquivos::PacoteEncontrado>,
    /// Folder of the packages installed with the program.
    pub pasta_instalada: Option<PathBuf>,
    /// The UTSim client is installed by the launcher ("Ver em 3D").
    pub cliente_3d: bool,
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
        let mut app = App::com(perfil, Arc::new(Fios), agora_ms);
        app.cliente_3d = utsim_ensino_store::tres_d::cliente().is_some();
        match pasta_de_pacotes() {
            Some(dir) => app.procurar_pacotes(&dir),
            None => app.atualizar_pacotes(),
        }
        app
    }

    /// Lists the packages of `dir` in the map, so the student opens them with one click.
    pub fn procurar_pacotes(&mut self, dir: &Path) {
        self.pasta_instalada = Some(dir.to_path_buf());
        self.atualizar_pacotes();
    }

    /// The packages installed with the program and those the teacher published in the
    /// class folder (`pacotes/`), such as an exam.
    pub fn atualizar_pacotes(&mut self) {
        let mut v = Vec::new();
        if let Some(d) = &self.pasta_instalada {
            v.extend(utsim_ensino_store::arquivos::pacotes_em(d));
        }
        if let Some(d) = self.config.turma.as_ref().and_then(|t| t.pasta.as_ref()) {
            v.extend(utsim_ensino_store::arquivos::pacotes_em(&d.join("pacotes")));
        }
        self.pacotes_instalados = v;
    }

    /// The files of the open package (the built-in campaign or the one opened from disk).
    pub fn arquivos_do_pacote(&self) -> Option<utsim_ensino_core::pacote::Arquivos> {
        arquivos_de(self.config.pacote_aberto.as_deref())
    }

    /// The exam rules of the open package and when the student opened it.
    pub fn prova(&self) -> Option<(utsim_ensino_core::prova::Prova, Option<i64>)> {
        let p = self.pacote.as_ref()?;
        let pr = p.pacote.prova.clone()?;
        let aberta = self.config.provas_abertas.get(&p.pacote.id).copied();
        Some((pr, aberta))
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
        let caminho_pacote = config
            .pacote_aberto
            .as_ref()
            .map(|c| c.display().to_string())
            .unwrap_or_default();
        let mut app = App {
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
            caminho_pacote,
            pacotes_instalados: Vec::new(),
            pasta_instalada: None,
            cliente_3d: false,
            nome: ident.nome,
            registro: ident.registro,
            apelido: ident.apelido,
            estilo_aplicado: None,
            config,
        };
        // The package opened last time comes back; if it is gone, the campaign stays.
        if let Some(c) = app.config.pacote_aberto.clone()
            && let Err(e) = app.abrir_pacote(&c)
        {
            app.config.pacote_aberto = None;
            app.mensagem = Some(format!(
                "o pacote {} não abriu ({e}); voltei à campanha",
                c.display()
            ));
        }
        app
    }

    /// True while a package other than the built-in campaign is open.
    pub fn pacote_externo(&self) -> bool {
        self.config.pacote_aberto.is_some()
    }

    /// Opens a mission package (folder or `.utse`) in place of the campaign. The progress
    /// is kept per package, so going back to the campaign loses nothing.
    pub fn abrir_pacote(&mut self, caminho: &Path) -> Result<(), String> {
        let arquivos =
            utsim_ensino_store::arquivos::ler_pacote(caminho).map_err(|e| e.to_string())?;
        let p = carregar(&arquivos).map_err(|e| e.join("; "))?;
        let caminho = absoluto(caminho);
        self.caminho_pacote = caminho.display().to_string();
        self.config.pacote_aberto = Some(caminho);
        self.salvar_config();
        self.usar_pacote(p);
        Ok(())
    }

    /// Back to the campaign built into the app.
    pub fn voltar_campanha(&mut self) {
        self.config.pacote_aberto = None;
        self.salvar_config();
        match carregar_campanha() {
            Ok(p) => self.usar_pacote(p),
            Err(e) => {
                self.pacote = None;
                self.erro_pacote = e;
            }
        }
    }

    fn usar_pacote(&mut self, p: PacoteCarregado) {
        self.progresso = self.perfil.progresso(&p.pacote.id);
        self.pacote = Some(Arc::new(p));
        self.erro_pacote.clear();
        self.missao = None;
        self.areia = None;
        if self.tela == Tela::Missao || self.tela == Tela::Areia {
            self.tela = Tela::Mapa;
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
            self.mensagem = Some(texto::sem_identidade().into());
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
            let mut estado = missao::EstadoMissao::new(p.clone(), i, tentativas);
            // The last attempt of the journal brings back the locked predictions, the
            // answers, the hints already opened and the decisions.
            let id = &p.missoes[i].missao.id;
            if let Some(r) = self
                .perfil
                .diario()
                .iter()
                .rev()
                .filter_map(|t| t.registro().ok())
                .find(|r| &r.missao == id && r.pacote == p.pacote.id)
            {
                let anterior = r.tentativa;
                estado.tentativa.decisoes = p.missoes[i].missao.completar(&anterior.decisoes);
                estado.tentativa.previsoes = anterior.previsoes;
                estado.tentativa.dicas = anterior.dicas;
                if anterior.justificativas.len() == estado.tentativa.justificativas.len() {
                    estado.tentativa.justificativas = anterior.justificativas;
                }
                estado.previsao_travada = true;
            } else {
                // No attempt on record (another computer): the predictions stay open.
                estado.previsao_travada = false;
            }
            // The time limit of an exam counts from the first mission opened.
            if let Some(pr) = &p.pacote.prova {
                let agora = (self.relogio)();
                if pr.pode_rodar(agora, None).is_ok()
                    && !self.config.provas_abertas.contains_key(&p.pacote.id)
                {
                    self.config
                        .provas_abertas
                        .insert(p.pacote.id.clone(), agora);
                    self.salvar_config();
                }
            }
            self.missao = Some(estado);
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
            if self
                .pacote
                .as_ref()
                .is_none_or(|p| p.pacote.prova.is_none())
            {
                ui.label(format!("★ {}", self.progresso.estrelas()));
            }
        });
    }

    fn tela_identidade(&mut self, ui: &mut Ui) {
        ui.add_space(30.0);
        ui.vertical_centered(|ui| {
            ui.label(RichText::new(texto::NOME_APP).size(28.0).strong());
            ui.label(texto::subtitulo());
            ui.add_space(20.0);
            ui.label(RichText::new(texto::identidade_titulo()).size(18.0));
            ui.label(texto::identidade_texto());
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
            self.outros_pacotes(ui);
            return;
        };
        ui.heading(&p.pacote.titulo);
        ui.label(&p.pacote.descricao);
        if !p.faltando.is_empty() {
            ui.colored_label(AVISO, texto::requisitos(&p.faltando));
        }
        if let Some((pr, aberta)) = self.prova() {
            ui.colored_label(ROXO, texto_prova(&pr, aberta, (self.relogio)()));
        }
        self.atalhos_de_pacotes(ui);
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
                // In an exam every mission is open from the start.
                let liberada = p.pacote.prova.is_some() || self.progresso.liberada(&p, i);
                let reg = self.progresso.registro(&m.missao.id);
                ui.horizontal(|ui| {
                    let estrelas = reg.map_or(0, |r| r.estrelas);
                    let marca = format!(
                        "{}{}",
                        "★".repeat(estrelas as usize),
                        "☆".repeat(3 - estrelas.min(3) as usize)
                    );
                    // An exam shows no grade before the teacher's correction.
                    if p.pacote.prova.is_none() {
                        ui.label(RichText::new(marca).monospace());
                    }
                    let titulo = format!("{} {}", numero_missao(&p, i), m.missao.titulo);
                    if ui
                        .add_enabled(liberada, egui::Button::new(titulo))
                        .clicked()
                    {
                        abrir = Some(i);
                    }
                    if !liberada {
                        let primeira_do_ato = i == 0 || p.missoes[i - 1].ato != m.ato;
                        let txt = if primeira_do_ato {
                            "bloqueada: resolva o ato anterior"
                        } else {
                            "bloqueada: resolva a anterior"
                        };
                        ui.label(RichText::new(txt).weak());
                    }
                    if let Some(r) = reg.filter(|_| p.pacote.prova.is_none()) {
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
        self.outros_pacotes(ui);
        if let Some(i) = abrir {
            self.abrir_missao(i);
        }
        if let Some(i) = areia {
            self.abrir_areia(i);
        }
    }

    /// One line at the top of the map with the installed packages and the campaign, so a
    /// long campaign does not hide them at the bottom.
    fn atalhos_de_pacotes(&mut self, ui: &mut Ui) {
        if self.pacotes_instalados.is_empty() {
            return;
        }
        let aberto = self.config.pacote_aberto.as_deref().map(absoluto);
        let mut abrir = None;
        let mut campanha = false;
        ui.horizontal_wrapped(|ui| {
            ui.label(RichText::new("Pacotes:").weak());
            if ui.selectable_label(aberto.is_none(), "Campanha").clicked() && aberto.is_some() {
                campanha = true;
            }
            for p in &self.pacotes_instalados {
                let atual = aberto.as_ref() == Some(&absoluto(&p.caminho));
                if ui.selectable_label(atual, &p.titulo).clicked() && !atual {
                    abrir = Some(p.caminho.clone());
                }
            }
        });
        if campanha {
            self.voltar_campanha();
            self.mensagem = None;
        }
        if let Some(c) = abrir {
            match self.abrir_pacote(&c) {
                Ok(()) => self.mensagem = None,
                Err(e) => self.mensagem = Some(format!("o pacote não abriu: {e}")),
            }
        }
    }

    /// Other mission packages: a folder or a `.utse` from the teacher.
    fn outros_pacotes(&mut self, ui: &mut Ui) {
        ui.add_space(14.0);
        ui.separator();
        ui.label(RichText::new(texto::outros_pacotes()).strong());
        let aberto = self.config.pacote_aberto.clone();
        let mut abrir = None;
        for p in &self.pacotes_instalados {
            ui.horizontal(|ui| {
                let atual = aberto
                    .as_deref()
                    .is_some_and(|a| absoluto(a) == absoluto(&p.caminho));
                if ui
                    .add_enabled(!atual, egui::Button::new(&p.titulo))
                    .clicked()
                {
                    abrir = Some(p.caminho.clone());
                }
                if !p.faltando.is_empty() {
                    ui.colored_label(AVISO, "⚠")
                        .on_hover_text(texto::requisitos(&p.faltando));
                }
                ui.add(egui::Label::new(RichText::new(&p.descricao).weak()).truncate());
            });
        }
        if let Some(c) = abrir {
            match self.abrir_pacote(&c) {
                Ok(()) => self.mensagem = None,
                Err(e) => self.mensagem = Some(format!("o pacote não abriu: {e}")),
            }
        }
        ui.label(texto::outros_pacotes_texto());
        ui.horizontal(|ui| {
            ui.text_edit_singleline(&mut self.caminho_pacote);
            if ui.button("Abrir pacote").clicked() {
                let c = PathBuf::from(self.caminho_pacote.trim().trim_matches('"'));
                match self.abrir_pacote(&c) {
                    Ok(()) => self.mensagem = None,
                    Err(e) => self.mensagem = Some(format!("o pacote não abriu: {e}")),
                }
            }
            if self.pacote_externo() && ui.button("Voltar à campanha").clicked() {
                self.voltar_campanha();
                self.mensagem = None;
            }
        });
        if let Some(m) = &self.mensagem {
            ui.colored_label(Color32::from_rgb(255, 112, 104), m);
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

/// The files of a package opened from disk, or of the campaign when `None`.
pub fn arquivos_de(caminho: Option<&Path>) -> Option<utsim_ensino_core::pacote::Arquivos> {
    match caminho {
        Some(c) => utsim_ensino_store::arquivos::ler_pacote(c).ok(),
        None => Some(utsim_ensino_core::campanha::arquivos()),
    }
}

/// The exam line: window, time limit and runs per mission.
pub fn texto_prova(
    pr: &utsim_ensino_core::prova::Prova,
    aberta: Option<i64>,
    agora: i64,
) -> String {
    use utsim_ensino_core::prova::data_hora;
    let mut t = format!(
        "Prova de {} a {}, {} rodada(s) por missão",
        data_hora(pr.inicio, pr.fuso_min),
        data_hora(pr.fim, pr.fuso_min),
        pr.tentativas
    );
    if pr.duracao_min > 0 {
        t.push_str(&format!(
            ", {} min a partir da primeira missão aberta",
            pr.duracao_min
        ));
    }
    match pr.pode_rodar(agora, aberta) {
        Ok(()) => t.push_str(&format!(
            ". Restam {} min.",
            (pr.restante(agora, aberta) + 59_999) / 60_000
        )),
        Err(e) => t.push_str(&format!(". Agora não dá para simular: {e}.")),
    }
    t.push_str(" Sem dicas, sem nota na tela e sem caixa de areia: o professor corrige.");
    t
}

/// `pacotes` beside the program folder: the launcher installs `bin/utsim-ensino` and
/// `pacotes/` side by side.
fn pasta_de_pacotes() -> Option<PathBuf> {
    let exe = std::env::current_exe().ok()?;
    let dir = exe.parent()?.parent()?.join("pacotes");
    dir.is_dir().then_some(dir)
}

/// The full path without the `\\?\` prefix that `canonicalize` gives on Windows, so the
/// field shows a path the student recognizes.
fn absoluto(caminho: &Path) -> PathBuf {
    match std::fs::canonicalize(caminho) {
        Ok(c) => {
            let t = c.to_string_lossy();
            match t.strip_prefix(r"\\?\") {
                Some(r) if !r.starts_with("UNC") => PathBuf::from(r),
                _ => c,
            }
        }
        Err(_) => caminho.to_path_buf(),
    }
}

/// `1.2`: act and position of mission `i` inside the act.
pub fn numero_missao(p: &PacoteCarregado, i: usize) -> String {
    let ato = p.missoes[i].ato;
    let pos = p.missoes[..=i].iter().filter(|m| m.ato == ato).count();
    format!("{ato}.{pos}")
}
