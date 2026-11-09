//! The class: the student joins (invitation and, optionally, the synced folder) and
//! delivers by file; the teacher creates the class, imports the student list, publishes
//! the package, grades every delivery, sees the matrix and the common mistakes and exports
//! the grades.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::AtomicBool;

use egui::{Color32, RichText, Ui};
use utsim_ensino_core::campanha::arquivos;
use utsim_ensino_core::nota::fmt_num;
use utsim_ensino_core::pacote::{PacoteCarregado, Registro};
use utsim_ensino_core::turma::{Aluno, PacotePublicado, Turma, importar_alunos};
use utsim_ensino_store::arquivos::Convite;
use utsim_ensino_store::correcao::{Correcao, corrigir_turma};
use utsim_ensino_store::pasta::{PastaTurma, importar_entregas};
use utsim_ensino_store::perfil::Vinculo;
use utsim_ensino_store::planilha::xlsx;

use crate::missao::Trabalho;
use crate::{App, numero_missao, texto};

const VERMELHO: Color32 = Color32::from_rgb(255, 112, 104);
const AMARELO: Color32 = Color32::from_rgb(236, 196, 76);

#[derive(Default)]
pub struct EstadoTurma {
    pub convite: String,
    pub pasta: String,
    pub destino_entrega: String,
    /// Folder of the company program of UTSim Seis Sigma, as typed.
    pub programa: String,
}

pub fn tela_turma(app: &mut App, ui: &mut Ui) {
    ui.heading(texto::turma_titulo());
    match app.config.turma.clone() {
        Some(v) => {
            ui.label(texto::f(
                texto::turma_voce_esta(),
                &[&v.convite.nome, &v.convite.periodo, &v.convite.turma],
            ));
            match &v.pasta {
                Some(p) => {
                    ui.label(texto::f(texto::turma_pasta_da_turma(), &[&p.display()]));
                    ui.label(texto::turma_gravada_na_pasta());
                    if ui.button(texto::turma_copiar_de_novo()).clicked() {
                        app.mensagem = Some(match app.perfil.sincronizar_pasta() {
                            Ok(n) => texto::f(texto::turma_tentativas_na_pasta(), &[&n]),
                            Err(e) => texto::f(texto::turma_erro(), &[&e]),
                        });
                    }
                }
                None => {
                    ui.label(texto::turma_por_arquivo());
                }
            }
            if ui.button(texto::turma_sair()).clicked() {
                app.config.turma = None;
                app.salvar_config();
                app.atualizar_pacotes();
            }
        }
        None => {
            ui.label(texto::turma_como_entrar());
            egui::Grid::new("entrar").num_columns(2).show(ui, |ui| {
                ui.label(texto::turma_convite());
                ui.text_edit_singleline(&mut app.turma.convite);
                ui.end_row();
                ui.label(texto::turma_pasta_opcional());
                ui.text_edit_singleline(&mut app.turma.pasta);
                ui.end_row();
            });
            if ui.button(texto::turma_entrar()).clicked() {
                match Convite::ler(Path::new(app.turma.convite.trim())) {
                    Ok(c) => {
                        let pasta = Some(app.turma.pasta.trim())
                            .filter(|p| !p.is_empty())
                            .map(PathBuf::from);
                        app.config.turma = Some(Vinculo { convite: c, pasta });
                        app.salvar_config();
                        app.atualizar_pacotes();
                        if let Err(e) = app.perfil.sincronizar_pasta() {
                            app.mensagem = Some(texto::f(texto::turma_nao_gravou_pasta(), &[&e]));
                        }
                    }
                    Err(e) => app.mensagem = Some(texto::f(texto::turma_convite_invalido(), &[&e])),
                }
            }
        }
    }
    ui.separator();
    ui.label(RichText::new(texto::turma_entrega_titulo()).strong());
    ui.label(texto::turma_entrega_texto());
    ui.horizontal(|ui| {
        ui.label(texto::turma_arquivo());
        ui.text_edit_singleline(&mut app.turma.destino_entrega);
        if ui.button(texto::turma_gravar_entrega()).clicked() {
            let mut destino = PathBuf::from(app.turma.destino_entrega.trim());
            if destino.extension().is_none() {
                destino.set_extension("utsr");
            }
            app.mensagem = Some(
                match app
                    .perfil
                    .entrega()
                    .and_then(|e| e.gravar(&destino).map(|_| e.tentativas.len()))
                {
                    Ok(n) => texto::f(texto::turma_entrega_gravada(), &[&n, &destino.display()]),
                    Err(e) => texto::f(texto::turma_entrega_falhou(), &[&e]),
                },
            );
        }
    });
    programa_seis_sigma(app, ui);
    // The scoreboards the teacher published in the class folder.
    if let Some(pasta) = app.config.turma.as_ref().and_then(|t| t.pasta.clone()) {
        let ps = utsim_ensino_store::desafio::placares(&pasta);
        if !ps.is_empty() {
            ui.separator();
            ui.label(RichText::new(texto::turma_placares()).strong());
            for (missao, csv) in ps {
                // The mission title, when the open package has it.
                let titulo = app
                    .pacote
                    .as_ref()
                    .and_then(|p| {
                        p.missoes
                            .iter()
                            .find(|m| utsim_ensino_store::pasta::slug(&m.missao.id) == missao)
                    })
                    .map_or(missao.clone(), |m| m.missao.titulo.clone());
                ui.collapsing(texto::f(texto::turma_desafio_de(), &[&titulo]), |ui| {
                    egui::Grid::new(format!("pl_{missao}"))
                        .striped(true)
                        .show(ui, |ui| {
                            for (i, l) in csv.lines().enumerate() {
                                let c: Vec<&str> = l.split(';').collect();
                                if c.len() < 5 {
                                    continue;
                                }
                                let pos = if i == 0 {
                                    texto::turma_posicao().to_string()
                                } else if c[4] == "sim" {
                                    format!("{}º =", c[0])
                                } else {
                                    format!("{}º", c[0])
                                };
                                ui.label(pos);
                                for x in &c[1..4] {
                                    ui.label(*x);
                                }
                                ui.end_row();
                            }
                        });
                });
            }
        }
    }
    ui.add_space(6.0);
    ui.label(RichText::new(texto::limite_cola()).weak());
}

/// The teacher's panel.
#[derive(Default)]
pub struct EstadoProfessor {
    pub pasta: String,
    pub nome: String,
    pub periodo: String,
    pub finalidade: String,
    pub csv: String,
    pub pasta_utsr: String,
    pub registro_ultima: bool,
    pub aberta: Option<(PathBuf, Turma, Vec<Aluno>)>,
    pub trabalho: Option<Trabalho<Correcao>>,
    pub correcao: Option<Correcao>,
    pub avisos_leitura: Vec<String>,
    pub missao_erros: usize,
    /// The case builder: model, title, output folder and the last report.
    pub caso_modelo: String,
    pub caso_titulo: String,
    pub caso_pasta: String,
    pub caso_trabalho: Option<Trabalho<Result<String, String>>>,
    pub caso_relatorio: Option<String>,
    /// The package of the last grading (the open one, or an exam rebuilt from its seed).
    pub pacote_corrigido: Option<Arc<PacoteCarregado>>,
    pub prova: EstadoProva,
    pub desafio: EstadoDesafio,
}

/// Challenges and teams in the teacher's panel.
#[derive(Default)]
pub struct EstadoDesafio {
    /// Index of the mission in the graded package.
    pub missao: usize,
    /// Deadline `AAAA-MM-DD HH:MM` (Brasília), empty for none.
    pub fim: String,
    pub placar: Option<Vec<utsim_ensino_core::desafio::Linha>>,
    /// Teams, one per line: `Name: record, record`.
    pub equipes: String,
    pub trabalho: Option<Trabalho<Result<utsim_ensino_core::desafio::ComparacaoEquipes, String>>>,
    pub comparacao: Option<utsim_ensino_core::desafio::ComparacaoEquipes>,
    pub sem_solucao: Vec<String>,
}

/// The exam form of the teacher's panel.
#[derive(Default)]
pub struct EstadoProva {
    pub id: String,
    pub titulo: String,
    /// Missions drawn per act.
    pub por_ato: std::collections::BTreeMap<u32, usize>,
    pub inicio: String,
    pub fim: String,
    pub duracao_min: u32,
    pub tentativas: u32,
    pub semente: String,
    /// Exam whose deliveries the next grading takes (`None`: the open package).
    pub corrigir: Option<String>,
}

impl EstadoProva {
    /// The assembly from the form (`agora` seeds the draw when the seed is empty).
    pub fn montagem(
        &self,
        base: &PacoteCarregado,
        agora: i64,
    ) -> Result<utsim_ensino_core::prova::Montagem, String> {
        use utsim_ensino_core::prova::{Montagem, Prova, Regra, ler_data_hora};
        let fuso = -180;
        let inicio = ler_data_hora(&self.inicio, fuso).ok_or(texto::turma_inicio_formato())?;
        let fim = ler_data_hora(&self.fim, fuso).ok_or(texto::turma_fim_formato())?;
        let semente = if self.semente.trim().is_empty() {
            agora as u64
        } else {
            self.semente
                .trim()
                .parse()
                .map_err(|_| texto::turma_semente_inteiro())?
        };
        let regras: Vec<Regra> = self
            .por_ato
            .iter()
            .filter(|(_, n)| **n > 0)
            .map(|(a, n)| Regra {
                ato: Some(*a),
                conceito: None,
                quantidade: *n,
            })
            .collect();
        let id = utsim_ensino_store::pasta::slug(self.id.trim());
        Ok(Montagem {
            titulo: if self.titulo.trim().is_empty() {
                id.clone()
            } else {
                self.titulo.trim().to_string()
            },
            id,
            pacote_base: base.pacote.id.clone(),
            hash_base: base.hash.clone(),
            regras,
            prova: Prova {
                semente,
                inicio,
                fim,
                duracao_min: self.duracao_min,
                tentativas: self.tentativas.max(1),
                fuso_min: fuso,
            },
        })
    }
}

impl EstadoProfessor {
    pub fn receber(&mut self) {
        if let Some(r) = self
            .caso_trabalho
            .as_ref()
            .and_then(|t| t.rx.try_recv().ok())
        {
            self.caso_trabalho = None;
            self.caso_relatorio = Some(match r {
                Ok(t) => t,
                Err(e) => texto::f(texto::turma_erro(), &[&e]),
            });
        }
        if let Some(r) = self
            .desafio
            .trabalho
            .as_ref()
            .and_then(|t| t.rx.try_recv().ok())
        {
            self.desafio.trabalho = None;
            match r {
                Ok(c) => self.desafio.comparacao = Some(c),
                Err(e) => self
                    .desafio
                    .sem_solucao
                    .push(texto::f(texto::turma_erro(), &[&e])),
            }
        }
        let Some(c) = self.trabalho.as_ref().and_then(|t| t.rx.try_recv().ok()) else {
            return;
        };
        self.trabalho = None;
        self.correcao = Some(c);
    }

    /// Starts the case builder: a draft package folder from a UTSim model, calibrated.
    pub fn criar_caso(&mut self, executor: &dyn crate::Executor, ctx: Option<egui::Context>) {
        let modelo = PathBuf::from(self.caso_modelo.trim());
        let pasta = PathBuf::from(self.caso_pasta.trim());
        let titulo = if self.caso_titulo.trim().is_empty() {
            texto::turma_missao_nova().to_string()
        } else {
            self.caso_titulo.trim().to_string()
        };
        self.caso_relatorio = None;
        self.caso_trabalho = Some(Trabalho::iniciar(executor, ctx, move |_| {
            utsim_ensino_store::caso::criar_caso(&modelo, &pasta, &titulo)
                .map_err(|e| e.to_string())
        }));
    }

    /// Checks the package folder again (after the teacher edited it).
    pub fn conferir_caso(&mut self, executor: &dyn crate::Executor, ctx: Option<egui::Context>) {
        let pasta = PathBuf::from(self.caso_pasta.trim());
        self.caso_relatorio = None;
        self.caso_trabalho = Some(Trabalho::iniciar(executor, ctx, move |_| {
            utsim_ensino_store::caso::conferir_pacote(&pasta).map_err(|e| e.to_string())
        }));
    }

    /// Opens the class folder at `dir`.
    pub fn abrir(&mut self, dir: &Path) -> Result<(), String> {
        let p = PastaTurma::abrir(dir).map_err(|e| e.to_string())?;
        let t = p.turma().map_err(|e| e.to_string())?;
        let a = p.alunos().map_err(|e| e.to_string())?;
        self.registro_ultima = t.registro == Registro::Ultima;
        self.aberta = Some((dir.to_path_buf(), t, a));
        self.correcao = None;
        Ok(())
    }

    /// Starts the batch grading of the open class: the folder deliveries plus the `.utsr`
    /// files of `pasta_utsr`.
    pub fn corrigir(
        &mut self,
        pacote: Arc<PacoteCarregado>,
        executor: &dyn crate::Executor,
        ctx: Option<egui::Context>,
    ) {
        let Some((dir, turma, alunos)) = self.aberta.clone() else {
            return;
        };
        let pasta = PastaTurma { dir };
        let (mut entregas, mut avisos) = pasta.entregas();
        if !self.pasta_utsr.trim().is_empty() {
            let (e, a) = importar_entregas(Path::new(self.pasta_utsr.trim()));
            entregas.extend(e);
            avisos.extend(a);
        }
        self.avisos_leitura = avisos;
        self.pacote_corrigido = Some(pacote.clone());
        let registro = turma.registro;
        self.trabalho = Some(Trabalho::iniciar(executor, ctx, move |prog| {
            let cancelar = AtomicBool::new(false);
            corrigir_turma(
                &pacote,
                registro,
                &alunos,
                &entregas,
                &|n, t| {
                    if t > 0 {
                        prog(n as f32 / t as f32);
                    }
                },
                &cancelar,
            )
        }));
    }
}

/// Creates a class folder with the built-in campaign published and the invitation.
pub fn criar_turma(
    dir: &Path,
    nome: &str,
    periodo: &str,
    finalidade: &str,
    chave_professor: &str,
    alunos: &[Aluno],
) -> Result<PastaTurma, String> {
    let id = format!(
        "{}-{}",
        nome.to_lowercase()
            .split_whitespace()
            .collect::<Vec<_>>()
            .join("-"),
        periodo.trim()
    );
    let turma = Turma {
        id,
        formato: 1,
        nome: nome.into(),
        periodo: periodo.into(),
        finalidade: finalidade.into(),
        chave_professor: chave_professor.into(),
        pacotes: vec![PacotePublicado {
            arquivo: "campanha".into(),
            prazo: None,
        }],
        registro: Registro::Melhor,
        desafios: Vec::new(),
        equipes: Vec::new(),
    };
    let p = PastaTurma::criar(dir, &turma, alunos).map_err(|e| e.to_string())?;
    p.publicar_pacote("campanha", &arquivos())
        .map_err(|e| e.to_string())?;
    p.convite()
        .and_then(|c| c.gravar(&dir.join("convite.utsc")))
        .map_err(|e| e.to_string())?;
    Ok(p)
}

pub fn tela_professor(app: &mut App, ui: &mut Ui) {
    let ctx = ui.ctx().clone();
    let executor = app.executor.clone();
    let Some(pacote) = app.pacote.clone() else {
        return;
    };
    let pacote_aberto = app.config.pacote_aberto.clone();
    let perfil = utsim_ensino_store::perfil::Perfil::em(&app.perfil.dir);
    let agora = (app.relogio)();
    ui.heading(texto::turma_professor());
    let s = &mut app.professor;

    ui.collapsing(texto::turma_construtor(), |ui| {
        ui.label(texto::turma_construtor_texto());
        egui::Grid::new("caso").num_columns(2).show(ui, |ui| {
            ui.label(texto::turma_modelo());
            ui.text_edit_singleline(&mut s.caso_modelo);
            ui.end_row();
            ui.label(texto::turma_titulo_missao());
            ui.text_edit_singleline(&mut s.caso_titulo);
            ui.end_row();
            ui.label(texto::turma_pasta_pacote());
            ui.text_edit_singleline(&mut s.caso_pasta);
            ui.end_row();
        });
        ui.horizontal(|ui| {
            let livre = s.caso_trabalho.is_none();
            if ui
                .add_enabled(livre, egui::Button::new(texto::turma_criar_calibrar()))
                .clicked()
            {
                s.criar_caso(executor.as_ref(), Some(ctx.clone()));
            }
            if ui
                .add_enabled(livre, egui::Button::new(texto::turma_conferir_de_novo()))
                .clicked()
            {
                s.conferir_caso(executor.as_ref(), Some(ctx.clone()));
            }
            if ui.button(texto::turma_gravar_utse()).clicked() {
                let pasta = PathBuf::from(s.caso_pasta.trim());
                let destino = pasta.with_extension("utse");
                s.caso_relatorio = Some(
                    match utsim_ensino_store::caso::empacotar(&pasta, &destino) {
                        Ok(p) => texto::f(texto::turma_pacote_gravado(), &[&p.display()]),
                        Err(e) => texto::f(texto::turma_erro(), &[&e]),
                    },
                );
            }
            if s.caso_trabalho.is_some() {
                ui.spinner();
                ui.ctx()
                    .request_repaint_after(std::time::Duration::from_millis(200));
            }
        });
        if let Some(r) = &s.caso_relatorio {
            for l in r.lines() {
                ui.label(l);
            }
        }
    });

    ui.collapsing(texto::turma_nova(), |ui| {
        egui::Grid::new("nova_turma").num_columns(2).show(ui, |ui| {
            ui.label(texto::turma_pasta_local_drive());
            ui.text_edit_singleline(&mut s.pasta);
            ui.end_row();
            ui.label(texto::turma_nome());
            ui.text_edit_singleline(&mut s.nome);
            ui.end_row();
            ui.label(texto::turma_periodo());
            ui.text_edit_singleline(&mut s.periodo);
            ui.end_row();
            ui.label(texto::turma_finalidade());
            ui.text_edit_singleline(&mut s.finalidade);
            ui.end_row();
            ui.label(texto::turma_lista_csv());
            ui.text_edit_singleline(&mut s.csv);
            ui.end_row();
        });
        if ui.button(texto::turma_criar()).clicked() {
            let r = (|| {
                if s.nome.trim().is_empty() || s.pasta.trim().is_empty() {
                    return Err(texto::turma_informe_pasta_nome().to_string());
                }
                let alunos = if s.csv.trim().is_empty() {
                    Vec::new()
                } else {
                    let t = std::fs::read_to_string(s.csv.trim()).map_err(|e| e.to_string())?;
                    importar_alunos(&t).map_err(|e| e.join("; "))?
                };
                let chave = app
                    .perfil
                    .chave_professor()
                    .map_err(|e| e.to_string())?
                    .publica();
                let dir = PathBuf::from(s.pasta.trim());
                criar_turma(
                    &dir,
                    s.nome.trim(),
                    s.periodo.trim(),
                    s.finalidade.trim(),
                    &chave,
                    &alunos,
                )?;
                s.abrir(&dir)?;
                Ok(dir)
            })();
            app.mensagem = Some(match r {
                Ok(d) => {
                    if !app.config.turmas_professor.contains(&d) {
                        app.config.turmas_professor.push(d.clone());
                    }
                    let _ = app.perfil.salvar_config(&app.config);
                    texto::f(texto::turma_criada(), &[&d.display()])
                }
                Err(e) => texto::f(texto::turma_nao_criou(), &[&e]),
            });
        }
    });

    ui.horizontal(|ui| {
        ui.label(texto::turma_abrir_turma());
        ui.text_edit_singleline(&mut s.pasta);
        if ui.button(texto::turma_abrir()).clicked() {
            let dir = PathBuf::from(s.pasta.trim());
            if let Err(e) = s.abrir(&dir) {
                app.mensagem = Some(texto::f(texto::turma_nao_abriu(), &[&e]));
            }
        }
    });
    let recentes = app.config.turmas_professor.clone();
    if !recentes.is_empty() {
        ui.horizontal_wrapped(|ui| {
            ui.label(texto::turma_recentes());
            for d in recentes {
                if ui.button(d.display().to_string()).clicked() {
                    s.pasta = d.display().to_string();
                    if let Err(e) = s.abrir(&d) {
                        app.mensagem = Some(texto::f(texto::turma_nao_abriu(), &[&e]));
                    }
                }
            }
        });
    }

    let Some((dir, turma, alunos)) = s.aberta.clone() else {
        return;
    };
    ui.separator();
    ui.label(
        RichText::new(texto::f(
            texto::turma_cabecalho(),
            &[&turma.nome, &turma.periodo, &alunos.len()],
        ))
        .strong(),
    );
    ui.label(texto::f(
        texto::turma_pasta_convite(),
        &[&dir.display(), &dir.join("convite.utsc").display()],
    ));
    ui.horizontal(|ui| {
        ui.label(texto::turma_importar_lista());
        ui.text_edit_singleline(&mut s.csv);
        if ui.button(texto::turma_importar()).clicked() {
            let r = std::fs::read_to_string(s.csv.trim())
                .map_err(|e| e.to_string())
                .and_then(|t| importar_alunos(&t).map_err(|e| e.join("; ")))
                .and_then(|a| {
                    PastaTurma { dir: dir.clone() }
                        .salvar_alunos(&a)
                        .map_err(|e| e.to_string())
                        .map(|_| a.len())
                });
            app.mensagem = Some(match r {
                Ok(n) => texto::f(texto::turma_importados(), &[&n]),
                Err(e) => texto::f(texto::turma_lista_recusada(), &[&e]),
            });
            let _ = s.abrir(&dir);
        }
    });
    let mut ultima = s.registro_ultima;
    if ui
        .checkbox(&mut ultima, texto::turma_registrar_ultima())
        .changed()
    {
        s.registro_ultima = ultima;
        let mut t = turma.clone();
        t.registro = if ultima {
            Registro::Ultima
        } else {
            Registro::Melhor
        };
        if let Err(e) = (PastaTurma { dir: dir.clone() }).salvar_turma(&t) {
            app.mensagem = Some(texto::f(texto::turma_erro(), &[&e]));
        }
        let _ = s.abrir(&dir);
    }
    ui.horizontal(|ui| {
        ui.label(texto::turma_entregas_utsr());
        ui.text_edit_singleline(&mut s.pasta_utsr);
    });
    ui.horizontal(|ui| {
        if ui
            .add_enabled(
                s.trabalho.is_none(),
                egui::Button::new(texto::turma_corrigir_todas()),
            )
            .clicked()
        {
            let alvo = match s.prova.corrigir.clone() {
                None => Ok(pacote.clone()),
                Some(id) => utsim_ensino_store::prova::provas_salvas(&perfil)
                    .into_iter()
                    .find(|m| m.id == id)
                    .ok_or_else(|| texto::f(texto::turma_prova_nao_encontrada(), &[&id]))
                    .and_then(|m| {
                        let base = crate::arquivos_de(pacote_aberto.as_deref())
                            .ok_or(texto::turma_nao_leu_pacote())?;
                        utsim_ensino_store::prova::pacote_da_prova(&base, &pacote, &m)
                    })
                    .map(Arc::new),
            };
            match alvo {
                Ok(p) => s.corrigir(p, executor.as_ref(), Some(ctx.clone())),
                Err(e) => app.mensagem = Some(texto::f(texto::turma_nao_corrigiu(), &[&e])),
            }
        }
        if let Some(t) = &s.trabalho {
            ui.add(
                egui::ProgressBar::new(t.fracao())
                    .desired_width(200.0)
                    .show_percentage(),
            );
            ui.ctx()
                .request_repaint_after(std::time::Duration::from_millis(100));
        }
    });
    for a in &s.avisos_leitura {
        ui.colored_label(AMARELO, a);
    }
    if let Some(m) = secao_prova(
        ui,
        s,
        &dir,
        &perfil,
        &pacote,
        pacote_aberto.as_deref(),
        agora,
    ) {
        app.mensagem = Some(m);
    }
    let s = &mut app.professor;
    let pacote = s.pacote_corrigido.clone().unwrap_or(pacote);
    let Some(c) = &s.correcao else { return };
    ui.separator();
    ui.label(RichText::new(texto::turma_matriz()).strong());
    egui::ScrollArea::horizontal().show(ui, |ui| {
        egui::Grid::new("matriz").striped(true).show(ui, |ui| {
            ui.label(RichText::new(texto::turma_aluno()).strong());
            for i in 0..c.matriz.missoes.len() {
                ui.label(RichText::new(numero_missao(&pacote, i)).strong())
                    .on_hover_text(&c.matriz.titulos[i]);
            }
            ui.label(RichText::new(texto::turma_media()).strong());
            ui.label(RichText::new(texto::turma_avisos()).strong());
            ui.end_row();
            for l in &c.matriz.linhas {
                ui.label(format!("{} ({})", l.aluno.nome, l.aluno.registro));
                for cel in &l.celulas {
                    match cel {
                        Some(x) => ui
                            .label(format!(
                                "{} {}",
                                fmt_num(x.nota, 0),
                                "★".repeat(x.estrelas as usize)
                            ))
                            .on_hover_text(texto::f(
                                texto::turma_celula_dica(),
                                &[&x.tentativas, &x.dicas],
                            )),
                        None => ui.label("-"),
                    };
                }
                ui.label(fmt_num(l.media(), 1));
                if l.avisos.is_empty() {
                    ui.label("");
                } else {
                    ui.colored_label(VERMELHO, l.avisos.join("; "));
                }
                ui.end_row();
            }
        });
    });
    if !c.semelhancas.is_empty() {
        ui.separator();
        ui.label(RichText::new(texto::turma_pares()).strong());
        ui.label(RichText::new(texto::turma_semelhanca()).weak());
        for x in &c.semelhancas {
            ui.label(texto::f(
                texto::turma_par(),
                &[&x.a, &x.b, &x.missao, &x.motivo],
            ));
        }
    }
    ui.separator();
    ui.label(RichText::new(texto::turma_erros_comuns()).strong());
    if c.erros_comuns.values().all(Vec::is_empty) {
        ui.label(texto::turma_nenhum_erro());
    }
    for (i, m) in pacote.missoes.iter().enumerate() {
        if let Some(es) = c.erros_comuns.get(&m.missao.id).filter(|es| !es.is_empty()) {
            ui.label(
                RichText::new(format!("{} {}", numero_missao(&pacote, i), m.missao.titulo))
                    .strong(),
            );
            for e in es.iter().take(4) {
                ui.label(format!("• {}", e.texto));
            }
        }
    }
    ui.separator();
    ui.horizontal(|ui| {
        if ui.button(texto::turma_exportar()).clicked() {
            let pasta = PastaTurma { dir: dir.clone() };
            let (cab, linhas) = c.matriz.tabela();
            let r = pasta
                .gravar_correcao("notas.csv", c.matriz.csv().as_bytes())
                .and_then(|_| xlsx(&[(texto::turma_planilha_notas().into(), cab, linhas)]))
                .and_then(|b| pasta.gravar_correcao("notas.xlsx", &b));
            app.mensagem = Some(match r {
                Ok(p) => texto::f(
                    texto::turma_notas_exportadas(),
                    &[&p.parent().unwrap_or(&dir).display()],
                ),
                Err(e) => texto::f(texto::turma_nao_exportou(), &[&e]),
            });
        }
    });
    let alunos_turma = alunos.clone();
    if let Some(m) = secao_desafio(
        ui,
        &mut app.professor,
        &dir,
        &turma,
        &alunos_turma,
        &pacote,
        executor.as_ref(),
        &ctx,
    ) {
        app.mensagem = Some(m);
    }
}

/// The scoreboard of a challenge and the paired comparison of the teams, from the last
/// grading. Returns a message for the status line.
#[allow(clippy::too_many_arguments)]
fn secao_desafio(
    ui: &mut Ui,
    s: &mut EstadoProfessor,
    dir: &Path,
    turma: &Turma,
    alunos: &[Aluno],
    pacote: &Arc<PacoteCarregado>,
    executor: &dyn crate::Executor,
    ctx: &egui::Context,
) -> Option<String> {
    use utsim_ensino_core::desafio::{Desafio, comparar_equipes, ler_equipes};
    let mut msg = None;
    let c = s.correcao.as_ref()?;
    ui.separator();
    let aberto = s.desafio.placar.is_some() || s.desafio.comparacao.is_some();
    egui::CollapsingHeader::new(texto::turma_desafio_equipes())
        .default_open(aberto)
        .show(ui, |ui| {
            ui.label(texto::desafio_texto());
            let d = &mut s.desafio;
            if d.equipes.is_empty() && !turma.equipes.is_empty() {
                use std::fmt::Write;
                let mut t = String::new();
                for e in &turma.equipes {
                    let _ = writeln!(t, "{}: {}", e.nome, e.membros.join(", "));
                }
                d.equipes = t.trim_end().to_string();
            }
            d.missao = d.missao.min(pacote.missoes.len().saturating_sub(1));
            let rotulo = |i: usize| {
                format!(
                    "{} {}",
                    numero_missao(pacote, i),
                    pacote.missoes[i].missao.titulo
                )
            };
            egui::ComboBox::from_label(texto::turma_missao())
                .selected_text(rotulo(d.missao))
                .show_ui(ui, |ui| {
                    for i in 0..pacote.missoes.len() {
                        ui.selectable_value(&mut d.missao, i, rotulo(i));
                    }
                });
            let missao = pacote.missoes[d.missao].missao.id.clone();
            ui.horizontal(|ui| {
                ui.label(texto::turma_prazo());
                ui.text_edit_singleline(&mut d.fim);
            });
            ui.horizontal(|ui| {
                if ui.button(texto::turma_montar_placar()).clicked() {
                    let fim = if d.fim.trim().is_empty() {
                        Ok(None)
                    } else {
                        utsim_ensino_core::prova::ler_data_hora(&d.fim, -180)
                            .map(Some)
                            .ok_or(texto::turma_prazo_formato())
                    };
                    match fim {
                        Ok(fim) => {
                            let desafio = Desafio {
                                missao: missao.clone(),
                                fim,
                            };
                            d.placar = Some(utsim_ensino_store::desafio::placar_do_desafio(
                                c,
                                alunos,
                                &desafio,
                                turma.registro,
                            ));
                            let mut t = turma.clone();
                            t.desafios.retain(|x| x.missao != missao);
                            t.desafios.push(desafio);
                            if let Err(e) = (PastaTurma {
                                dir: dir.to_path_buf(),
                            })
                            .salvar_turma(&t)
                            {
                                msg = Some(texto::f(texto::turma_erro(), &[&e]));
                            }
                        }
                        Err(e) => msg = Some(e.to_string()),
                    }
                }
                if let Some(linhas) = &d.placar
                    && ui.button(texto::turma_publicar_placar()).clicked()
                {
                    msg = Some(
                        match utsim_ensino_store::desafio::gravar_placar(
                            &PastaTurma {
                                dir: dir.to_path_buf(),
                            },
                            &missao,
                            linhas,
                        ) {
                            Ok(p) => texto::f(texto::turma_placar_publicado(), &[&p.display()]),
                            Err(e) => texto::f(texto::turma_nao_publicou(), &[&e]),
                        },
                    );
                }
            });
            if let Some(linhas) = &d.placar {
                placar_tabela(ui, linhas);
            }
            ui.separator();
            ui.label(texto::turma_equipes_texto());
            ui.add(egui::TextEdit::multiline(&mut d.equipes).desired_rows(3));
            ui.horizontal(|ui| {
                if ui
                    .add_enabled(
                        d.trabalho.is_none(),
                        egui::Button::new(texto::turma_comparar_equipes()),
                    )
                    .clicked()
                {
                    match ler_equipes(&d.equipes) {
                        Ok(eqs) => {
                            let mut t = turma.clone();
                            t.equipes = eqs.clone();
                            let _ = (PastaTurma {
                                dir: dir.to_path_buf(),
                            })
                            .salvar_turma(&t);
                            let (sol, sem) = utsim_ensino_store::desafio::solucoes_das_equipes(
                                c,
                                &eqs,
                                &missao,
                                turma.registro,
                            );
                            d.sem_solucao = sem;
                            d.comparacao = None;
                            if sol.len() < 2 {
                                msg = Some(texto::turma_duas_equipes().into());
                            } else {
                                let m = pacote.missoes[d.missao].clone();
                                d.trabalho = Some(Trabalho::iniciar(
                                    executor,
                                    Some(ctx.clone()),
                                    move |prog| {
                                        comparar_equipes(&m, &sol, prog).map_err(|e| e.to_string())
                                    },
                                ));
                            }
                        }
                        Err(e) => msg = Some(e),
                    }
                }
                if let Some(t) = &d.trabalho {
                    ui.add(egui::ProgressBar::new(t.fracao()).desired_width(160.0));
                    ui.ctx()
                        .request_repaint_after(std::time::Duration::from_millis(100));
                }
            });
            for e in &d.sem_solucao {
                ui.colored_label(AMARELO, texto::f(texto::turma_sem_tentativa(), &[&e]));
            }
            if let Some(k) = &d.comparacao {
                comparacao_tabela(ui, k);
            }
        });
    msg
}

/// A scoreboard: position (= for ties), nickname, grade and cost.
pub fn placar_tabela(ui: &mut Ui, linhas: &[utsim_ensino_core::desafio::Linha]) {
    egui::Grid::new("placar").striped(true).show(ui, |ui| {
        for h in [
            texto::turma_posicao(),
            texto::turma_apelido(),
            texto::turma_nota(),
            texto::turma_custo(),
        ] {
            ui.label(RichText::new(h).strong());
        }
        ui.end_row();
        for l in linhas {
            ui.label(if l.empate {
                format!("{}º =", l.posicao)
            } else {
                format!("{}º", l.posicao)
            });
            ui.label(&l.apelido);
            ui.label(fmt_num(l.nota, 1));
            ui.label(fmt_num(l.custo, 0));
            ui.end_row();
        }
    });
    if linhas.iter().any(|l| l.empate) {
        ui.label(RichText::new(texto::turma_empate()).weak());
    }
}

fn comparacao_tabela(ui: &mut Ui, k: &utsim_ensino_core::desafio::ComparacaoEquipes) {
    ui.label(
        RichText::new(texto::f(
            texto::turma_comparacao_titulo(),
            &[&k.metrica, &k.unidade],
        ))
        .strong(),
    );
    egui::Grid::new("equipes").striped(true).show(ui, |ui| {
        ui.label(RichText::new(texto::turma_posicao()).strong());
        ui.label(RichText::new(texto::turma_equipe()).strong());
        ui.label(RichText::new(texto::turma_media()).strong());
        for n in &k.equipes {
            ui.label(RichText::new(texto::f(texto::turma_menos(), &[&n])).strong());
        }
        ui.end_row();
        for i in 0..k.equipes.len() {
            let empate = k.posicoes.iter().filter(|p| **p == k.posicoes[i]).count() > 1;
            ui.label(if empate {
                format!("{}º =", k.posicoes[i])
            } else {
                format!("{}º", k.posicoes[i])
            });
            ui.label(&k.equipes[i]);
            ui.label(format!(
                "{} ± {}",
                fmt_num(k.medias[i].media, 2),
                fmt_num(k.medias[i].meia_largura, 2)
            ));
            for j in 0..k.equipes.len() {
                if i == j {
                    ui.label("");
                } else {
                    let d = &k.diferencas[i][j];
                    ui.label(format!(
                        "{} ± {}",
                        fmt_num(d.media, 2),
                        fmt_num(d.meia_largura, 2)
                    ));
                }
            }
            ui.end_row();
        }
    });
    ui.label(RichText::new(texto::turma_a_frente()).weak());
}

/// The exam: draw, window and runs, published in the class folder; and which exam the
/// next grading takes. Returns a message for the status line.
fn secao_prova(
    ui: &mut Ui,
    s: &mut EstadoProfessor,
    dir: &Path,
    perfil: &utsim_ensino_store::perfil::Perfil,
    pacote: &Arc<PacoteCarregado>,
    pacote_aberto: Option<&Path>,
    agora: i64,
) -> Option<String> {
    let mut msg = None;
    ui.collapsing(texto::turma_prova(), |ui| {
        ui.label(texto::prova_texto());
        let p = &mut s.prova;
        if p.tentativas == 0 {
            p.tentativas = 1;
        }
        egui::Grid::new("prova").num_columns(2).show(ui, |ui| {
            ui.label(texto::turma_id_prova());
            ui.text_edit_singleline(&mut p.id);
            ui.end_row();
            ui.label(texto::turma_titulo_campo());
            ui.text_edit_singleline(&mut p.titulo);
            ui.end_row();
            ui.label(texto::turma_inicio());
            ui.text_edit_singleline(&mut p.inicio);
            ui.end_row();
            ui.label(texto::turma_fim());
            ui.text_edit_singleline(&mut p.fim);
            ui.end_row();
            ui.label(texto::turma_tempo_maximo());
            ui.add(egui::DragValue::new(&mut p.duracao_min).range(0..=600));
            ui.end_row();
            ui.label(texto::turma_rodadas());
            ui.add(egui::DragValue::new(&mut p.tentativas).range(1..=20));
            ui.end_row();
            ui.label(texto::turma_semente());
            ui.text_edit_singleline(&mut p.semente);
            ui.end_row();
        });
        ui.label(texto::turma_por_ato());
        ui.horizontal_wrapped(|ui| {
            for ato in &pacote.pacote.atos {
                let n = p.por_ato.entry(ato.numero).or_default();
                ui.label(texto::f(texto::turma_ato(), &[&ato.numero]));
                ui.add(egui::DragValue::new(n).range(0..=ato.missoes.len()));
            }
        });
        if ui.button(texto::turma_sortear()).clicked() {
            let r = p.montagem(pacote, agora).and_then(|m| {
                let base = crate::arquivos_de(pacote_aberto).ok_or(texto::turma_nao_leu_pacote())?;
                let caminho = utsim_ensino_store::prova::publicar_prova(
                    &PastaTurma {
                        dir: dir.to_path_buf(),
                    },
                    perfil,
                    &base,
                    pacote,
                    &m,
                )
                .map_err(|e| e.to_string())?;
                Ok((m, caminho))
            });
            msg = Some(match r {
                Ok((m, c)) => {
                    p.semente = m.prova.semente.to_string();
                    p.corrigir = Some(m.id.clone());
                    texto::f(
                        texto::turma_prova_publicada(),
                        &[&m.id, &c.display(), &m.prova.semente],
                    )
                }
                Err(e) => texto::f(texto::turma_prova_nao_publicada(), &[&e]),
            });
        }
        let salvas = utsim_ensino_store::prova::provas_salvas(perfil);
        ui.horizontal_wrapped(|ui| {
            ui.label(texto::turma_correcao_usa());
            ui.selectable_value(&mut p.corrigir, None, texto::turma_pacote_aberto());
            for m in &salvas {
                ui.selectable_value(
                    &mut p.corrigir,
                    Some(m.id.clone()),
                    texto::f(texto::turma_a_prova(), &[&m.id]),
                );
            }
        });
    });
    msg
}

/// The company program of UTSim Seis Sigma: the solved missions record `praticou` in its
/// competence map, by the student themself.
fn programa_seis_sigma(app: &mut App, ui: &mut Ui) {
    ui.separator();
    ui.label(RichText::new(texto::turma_programa_titulo()).strong());
    ui.label(texto::programa_texto());
    ui.horizontal(|ui| {
        ui.label(texto::turma_pasta_programa());
        ui.text_edit_singleline(&mut app.turma.programa);
        if ui.button(texto::turma_ligar()).clicked() {
            let dir = PathBuf::from(app.turma.programa.trim().trim_matches('"'));
            let r = utsim_ensino_store::seis_sigma::identidade_local().and_then(|me| {
                utsim_lss_store::program::teaching_check(&dir, &me)
                    .map_err(|e| e.to_string())
                    .map(|_| me)
            });
            match r {
                Ok(me) => {
                    app.config.programa_seis_sigma = Some(dir);
                    app.salvar_config();
                    app.mensagem = Some(texto::f(texto::turma_programa_ligado(), &[&me.name]));
                }
                Err(e) => app.mensagem = Some(texto::f(texto::turma_nao_ligou(), &[&e])),
            }
        }
    });
    let Some(prog) = app.config.programa_seis_sigma.clone() else {
        return;
    };
    ui.label(texto::f(texto::turma_ligado_a(), &[&prog.display()]));
    ui.horizontal(|ui| {
        if ui.button(texto::turma_registrar_missoes()).clicked() {
            let r = match &app.pacote {
                Some(p) => utsim_ensino_store::seis_sigma::identidade_local().and_then(|me| {
                    utsim_ensino_store::seis_sigma::registrar_competencias(
                        &prog,
                        &me,
                        &utsim_lss_store::SystemClock,
                        p,
                        &app.progresso,
                        None,
                    )
                }),
                None => Err(texto::turma_nenhum_pacote().into()),
            };
            app.mensagem = Some(match r {
                Ok(x) if x.erros.is_empty() => {
                    texto::f(texto::turma_competencias(), &[&x.gravadas.len()])
                }
                Ok(x) => texto::f(texto::turma_algumas_nao(), &[&x.erros.join("; ")]),
                Err(e) => texto::f(texto::turma_nao_registrou(), &[&e]),
            });
        }
        if ui.button(texto::turma_desligar()).clicked() {
            app.config.programa_seis_sigma = None;
            app.salvar_config();
        }
    });
}
