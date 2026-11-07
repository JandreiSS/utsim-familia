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
}

pub fn tela_turma(app: &mut App, ui: &mut Ui) {
    ui.heading("Turma");
    match app.config.turma.clone() {
        Some(v) => {
            ui.label(format!(
                "Você está na turma {} {} ({})",
                v.convite.nome, v.convite.periodo, v.convite.turma
            ));
            match &v.pasta {
                Some(p) => {
                    ui.label(format!("Pasta da turma: {}", p.display()));
                    ui.label("Cada tentativa é gravada nessa pasta assim que termina. O Google Drive para computador sincroniza.");
                    if ui
                        .button("Copiar de novo todas as tentativas para a pasta")
                        .clicked()
                    {
                        app.mensagem = Some(match app.perfil.sincronizar_pasta() {
                            Ok(n) => format!("{n} tentativa(s) na pasta da turma"),
                            Err(e) => format!("erro: {e}"),
                        });
                    }
                }
                None => {
                    ui.label("Esta turma recebe as entregas por arquivo.");
                }
            }
            if ui.button("Sair da turma").clicked() {
                app.config.turma = None;
                app.salvar_config();
            }
        }
        None => {
            ui.label("Para entrar numa turma, abra o convite (.utsc) que o professor enviou. Se a turma usa uma pasta compartilhada no Drive, indique também a pasta sincronizada.");
            egui::Grid::new("entrar").num_columns(2).show(ui, |ui| {
                ui.label("Convite (.utsc)");
                ui.text_edit_singleline(&mut app.turma.convite);
                ui.end_row();
                ui.label("Pasta da turma (opcional)");
                ui.text_edit_singleline(&mut app.turma.pasta);
                ui.end_row();
            });
            if ui.button("Entrar na turma").clicked() {
                match Convite::ler(Path::new(app.turma.convite.trim())) {
                    Ok(c) => {
                        let pasta = Some(app.turma.pasta.trim())
                            .filter(|p| !p.is_empty())
                            .map(PathBuf::from);
                        app.config.turma = Some(Vinculo { convite: c, pasta });
                        app.salvar_config();
                        if let Err(e) = app.perfil.sincronizar_pasta() {
                            app.mensagem = Some(format!("não foi possível gravar na pasta: {e}"));
                        }
                    }
                    Err(e) => app.mensagem = Some(format!("convite inválido: {e}")),
                }
            }
        }
    }
    ui.separator();
    ui.label(RichText::new("Entrega por arquivo").strong());
    ui.label("Grava todas as suas tentativas assinadas num arquivo .utsr para enviar ao professor (ambiente da disciplina, e-mail ou pen drive).");
    ui.horizontal(|ui| {
        ui.label("Arquivo");
        ui.text_edit_singleline(&mut app.turma.destino_entrega);
        if ui.button("Gravar entrega").clicked() {
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
                    Ok(n) => format!(
                        "entrega com {n} tentativa(s) gravada em {}",
                        destino.display()
                    ),
                    Err(e) => format!("não foi possível gravar a entrega: {e}"),
                },
            );
        }
    });
    ui.add_space(6.0);
    ui.label(RichText::new(texto::LIMITE_COLA).weak());
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
                Err(e) => format!("erro: {e}"),
            });
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
            "Missão nova".to_string()
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
    ui.heading("Professor");
    let s = &mut app.professor;

    ui.collapsing("Construtor de casos", |ui| {
        ui.label("Gera uma missão de rascunho a partir de um modelo do UTSim: alavancas sugeridas (capacidades, filas limitadas, equipes), medidas de cada saída, referência pela busca exaustiva e metas a 5 % dela. Revise o texto, as faixas, os custos e as metas na pasta e confira de novo.");
        egui::Grid::new("caso").num_columns(2).show(ui, |ui| {
            ui.label("Modelo (.ron)");
            ui.text_edit_singleline(&mut s.caso_modelo);
            ui.end_row();
            ui.label("Título da missão");
            ui.text_edit_singleline(&mut s.caso_titulo);
            ui.end_row();
            ui.label("Pasta do pacote");
            ui.text_edit_singleline(&mut s.caso_pasta);
            ui.end_row();
        });
        ui.horizontal(|ui| {
            let livre = s.caso_trabalho.is_none();
            if ui.add_enabled(livre, egui::Button::new("Criar e calibrar")).clicked() {
                s.criar_caso(executor.as_ref(), Some(ctx.clone()));
            }
            if ui.add_enabled(livre, egui::Button::new("Conferir de novo")).clicked() {
                s.conferir_caso(executor.as_ref(), Some(ctx.clone()));
            }
            if ui.button("Gravar .utse").clicked() {
                let pasta = PathBuf::from(s.caso_pasta.trim());
                let destino = pasta.with_extension("utse");
                s.caso_relatorio = Some(match utsim_ensino_store::caso::empacotar(&pasta, &destino) {
                    Ok(p) => format!("pacote gravado em {}", p.display()),
                    Err(e) => format!("erro: {e}"),
                });
            }
            if s.caso_trabalho.is_some() {
                ui.spinner();
                ui.ctx().request_repaint_after(std::time::Duration::from_millis(200));
            }
        });
        if let Some(r) = &s.caso_relatorio {
            for l in r.lines() {
                ui.label(l);
            }
        }
    });

    ui.collapsing("Nova turma", |ui| {
        egui::Grid::new("nova_turma").num_columns(2).show(ui, |ui| {
            ui.label("Pasta da turma (local ou no Drive)");
            ui.text_edit_singleline(&mut s.pasta);
            ui.end_row();
            ui.label("Nome");
            ui.text_edit_singleline(&mut s.nome);
            ui.end_row();
            ui.label("Período");
            ui.text_edit_singleline(&mut s.periodo);
            ui.end_row();
            ui.label("Finalidade dos dados (LGPD)");
            ui.text_edit_singleline(&mut s.finalidade);
            ui.end_row();
            ui.label("Lista de alunos (.csv: nome;registro;apelido)");
            ui.text_edit_singleline(&mut s.csv);
            ui.end_row();
        });
        if ui.button("Criar turma").clicked() {
            let r = (|| {
                if s.nome.trim().is_empty() || s.pasta.trim().is_empty() {
                    return Err("informe a pasta e o nome".to_string());
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
                    format!(
                        "turma criada em {}; envie o convite.utsc aos alunos",
                        d.display()
                    )
                }
                Err(e) => format!("não foi possível criar a turma: {e}"),
            });
        }
    });

    ui.horizontal(|ui| {
        ui.label("Abrir turma:");
        ui.text_edit_singleline(&mut s.pasta);
        if ui.button("Abrir").clicked() {
            let dir = PathBuf::from(s.pasta.trim());
            if let Err(e) = s.abrir(&dir) {
                app.mensagem = Some(format!("não foi possível abrir: {e}"));
            }
        }
    });
    let recentes = app.config.turmas_professor.clone();
    if !recentes.is_empty() {
        ui.horizontal_wrapped(|ui| {
            ui.label("Recentes:");
            for d in recentes {
                if ui.button(d.display().to_string()).clicked() {
                    s.pasta = d.display().to_string();
                    if let Err(e) = s.abrir(&d) {
                        app.mensagem = Some(format!("não foi possível abrir: {e}"));
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
        RichText::new(format!(
            "{} {} · {} aluno(s)",
            turma.nome,
            turma.periodo,
            alunos.len()
        ))
        .strong(),
    );
    ui.label(format!(
        "Pasta: {} · convite: {}",
        dir.display(),
        dir.join("convite.utsc").display()
    ));
    ui.horizontal(|ui| {
        ui.label("Importar lista de alunos (.csv):");
        ui.text_edit_singleline(&mut s.csv);
        if ui.button("Importar").clicked() {
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
                Ok(n) => format!("{n} aluno(s) importado(s)"),
                Err(e) => format!("lista recusada: {e}"),
            });
            let _ = s.abrir(&dir);
        }
    });
    let mut ultima = s.registro_ultima;
    if ui
        .checkbox(
            &mut ultima,
            "Registrar a última tentativa (desmarcado: a melhor)",
        )
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
            app.mensagem = Some(format!("erro: {e}"));
        }
        let _ = s.abrir(&dir);
    }
    ui.horizontal(|ui| {
        ui.label("Entregas por arquivo (pasta com .utsr, opcional):");
        ui.text_edit_singleline(&mut s.pasta_utsr);
    });
    ui.horizontal(|ui| {
        if ui
            .add_enabled(
                s.trabalho.is_none(),
                egui::Button::new("Corrigir todas as entregas"),
            )
            .clicked()
        {
            s.corrigir(pacote.clone(), executor.as_ref(), Some(ctx.clone()));
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
    let Some(c) = &s.correcao else { return };
    ui.separator();
    ui.label(RichText::new("Matriz aluno por missão").strong());
    egui::ScrollArea::horizontal().show(ui, |ui| {
        egui::Grid::new("matriz").striped(true).show(ui, |ui| {
            ui.label(RichText::new("Aluno").strong());
            for i in 0..c.matriz.missoes.len() {
                ui.label(RichText::new(numero_missao(&pacote, i)).strong())
                    .on_hover_text(&c.matriz.titulos[i]);
            }
            ui.label(RichText::new("Média").strong());
            ui.label(RichText::new("Avisos").strong());
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
                            .on_hover_text(format!(
                                "{} tentativa(s), dica até o nível {}",
                                x.tentativas, x.dicas
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
    ui.separator();
    ui.label(RichText::new("Erros mais comuns").strong());
    if c.erros_comuns.values().all(Vec::is_empty) {
        ui.label("Nenhum erro frequente nas entregas corrigidas.");
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
        if ui
            .button("Exportar notas (CSV e .xlsx em correcao/)")
            .clicked()
        {
            let pasta = PastaTurma { dir: dir.clone() };
            let (cab, linhas) = c.matriz.tabela();
            let r = pasta
                .gravar_correcao("notas.csv", c.matriz.csv().as_bytes())
                .and_then(|_| xlsx(&[("Notas".into(), cab, linhas)]))
                .and_then(|b| pasta.gravar_correcao("notas.xlsx", &b));
            app.mensagem = Some(match r {
                Ok(p) => format!(
                    "notas exportadas em {}",
                    p.parent().unwrap_or(&dir).display()
                ),
                Err(e) => format!("não foi possível exportar: {e}"),
            });
        }
    });
}
