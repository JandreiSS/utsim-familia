//! The mission screen (statement, measures, prediction, levers with the budget,
//! justification, hints, run, results with intervals and the 2D view) and the sandbox.

use std::collections::BTreeMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::mpsc::{Receiver, channel};

use egui::{Color32, RichText, Ui};
use utsim_ensino_core::missao::{Alavanca, Decisoes, Previsao};
use utsim_ensino_core::motor::{
    Cenario, avaliar, modelo_da_tentativa, replay, rodar, simular, valor_metrica,
};
use utsim_ensino_core::nota::{
    Intervalo, RespostaPrevisao, Resultado, Situacao, Tentativa, corrigir, dinheiro, fmt_num,
};
use utsim_ensino_core::pacote::{MissaoCarregada, PacoteCarregado};
use utsim_model::analysis::{is_significant, paired_difference};
use utsim_model::replay::Replay;

use crate::vista2d::Animacao;
use crate::{App, Tela, numero_missao, texto};
use utsim_ui_tema::arquivos as seletor;

const VERDE: Color32 = Color32::from_rgb(110, 214, 120);
const AMARELO: Color32 = Color32::from_rgb(236, 196, 76);
const VERMELHO: Color32 = Color32::from_rgb(255, 112, 104);

/// A job running away from the screen.
pub struct Trabalho<T> {
    pub rx: Receiver<T>,
    pub progresso: Arc<AtomicU32>,
    pub cancelar: Arc<AtomicBool>,
}

impl<T: Send + 'static> Trabalho<T> {
    pub fn iniciar(
        executor: &dyn crate::Executor,
        ctx: Option<egui::Context>,
        job: impl FnOnce(&(dyn Fn(f32) -> bool + Sync)) -> T + Send + 'static,
    ) -> Trabalho<T> {
        let (tx, rx) = channel();
        let progresso = Arc::new(AtomicU32::new(0));
        let cancelar = Arc::new(AtomicBool::new(false));
        let (p, c) = (progresso.clone(), cancelar.clone());
        executor.executar(Box::new(move || {
            let cb = |f: f32| {
                p.store((f.clamp(0.0, 1.0) * 1000.0) as u32, Ordering::Relaxed);
                !c.load(Ordering::Relaxed)
            };
            let r = job(&cb);
            let _ = tx.send(r);
            if let Some(ctx) = ctx {
                ctx.request_repaint();
            }
        }));
        Trabalho {
            rx,
            progresso,
            cancelar,
        }
    }

    pub fn fracao(&self) -> f32 {
        self.progresso.load(Ordering::Relaxed) as f32 / 1000.0
    }
}

/// What a run of the mission brings back.
pub enum Rodada {
    Medicoes(Result<Vec<String>, String>),
    Resultado(Box<Result<(Tentativa, Resultado, Replay), String>>),
}

pub struct EstadoMissao {
    pub pacote: Arc<PacoteCarregado>,
    pub indice: usize,
    pub tentativa: Tentativa,
    /// Text typed in the number predictions.
    pub previsao_texto: BTreeMap<String, String>,
    /// Predictions locked after the first run of the mission.
    pub previsao_travada: bool,
    /// Runs done in this mission (all sessions).
    pub rodadas: u32,
    pub medicoes: Option<Vec<String>>,
    pub trabalho: Option<Trabalho<Rodada>>,
    pub resultado: Option<Resultado>,
    pub animacao: Option<Animacao>,
    pub erro: Option<String>,
    /// The task or machine picked by a click on the board or the grid.
    pub selecionada: Option<String>,
    /// The `.ron` file of a modelling mission.
    pub caminho_modelo: String,
    /// What happened reading or writing that file.
    pub aviso_modelo: Option<String>,
    /// Where the data of the mission are written (CSV or the Seis Sigma project).
    pub pasta_dados: String,
    pub aviso_dados: Option<String>,
    /// The data sets of the student (whose id comes first), made once: a simulation set
    /// runs the model.
    pub dados_gerados: Option<(String, Vec<utsim_ensino_core::dados::Conjunto>)>,
    /// A run has just arrived: the screen scrolls to its result once.
    pub rolar_resultado: bool,
}

impl EstadoMissao {
    pub fn new(pacote: Arc<PacoteCarregado>, indice: usize, rodadas: u32) -> EstadoMissao {
        let m = &pacote.missoes[indice].missao;
        let tentativa = Tentativa {
            decisoes: m.decisoes_iniciais(),
            justificativas: vec![None; m.justificativa.len()],
            ..Default::default()
        };
        EstadoMissao {
            indice,
            tentativa,
            previsao_texto: BTreeMap::new(),
            previsao_travada: rodadas > 0,
            rodadas,
            medicoes: None,
            trabalho: None,
            resultado: None,
            animacao: None,
            erro: None,
            selecionada: None,
            caminho_modelo: String::new(),
            aviso_modelo: None,
            pasta_dados: String::new(),
            aviso_dados: None,
            dados_gerados: None,
            rolar_resultado: false,
            pacote,
        }
    }

    pub fn missao(&self) -> &MissaoCarregada {
        &self.pacote.missoes[self.indice]
    }

    pub fn receber(&mut self) -> Option<Rodada> {
        let r = self.trabalho.as_ref()?.rx.try_recv().ok()?;
        self.trabalho = None;
        Some(r)
    }

    /// Whether every prediction has an answer.
    pub fn previsoes_completas(&self) -> bool {
        self.missao()
            .missao
            .previsao
            .iter()
            .all(|p| self.tentativa.previsoes.contains_key(p.id()))
    }

    /// Starts the run of the attempt.
    pub fn simular(
        &mut self,
        aluno: String,
        executor: &dyn crate::Executor,
        ctx: Option<egui::Context>,
    ) {
        if self.trabalho.is_some() {
            return;
        }
        let mut t = self.tentativa.clone();
        t.rodada = self.rodadas + 1;
        t.aluno = aluno.clone();
        let pacote = self.pacote.clone();
        let i = self.indice;
        self.erro = None;
        self.trabalho = Some(Trabalho::iniciar(executor, ctx, move |prog| {
            let m = &pacote.missoes[i];
            let r = (|| {
                if m.missao.modelagem.is_some() {
                    let res = utsim_ensino_core::modelagem::corrigir_modelagem(
                        m,
                        &pacote.pacote.pesos,
                        &t,
                    )
                    .map_err(|e| e.to_string())?;
                    // The animation shows the delivered model when it loads.
                    let entregue = t
                        .modelo
                        .as_deref()
                        .and_then(|txt| utsim_model::Model::parse(txt).ok())
                        .unwrap_or_else(|| m.modelo.clone());
                    let mut mo = entregue;
                    mo.run.stop_time = m
                        .missao
                        .animacao
                        .duracao
                        .unwrap_or(480.0)
                        .min(mo.run.stop_time);
                    mo.run.warmup = 0.0;
                    let rep = mo.replay(0).map_err(|e| e.to_string())?;
                    return Ok((t, res, rep));
                }
                let sim = simular(m, &aluno, &t.decisoes, prog).map_err(|e| e.to_string())?;
                let res = corrigir(&m.missao, &pacote.pacote.pesos, &sim, &t)
                    .map_err(|e| e.to_string())?;
                let rep = replay(m, &aluno, &t.decisoes).map_err(|e| e.to_string())?;
                Ok((t, res, rep))
            })();
            Rodada::Resultado(Box::new(r))
        }));
    }

    /// Runs the initial state and shows the mission's measures.
    pub fn medir(
        &mut self,
        aluno: String,
        executor: &dyn crate::Executor,
        ctx: Option<egui::Context>,
    ) {
        if self.trabalho.is_some() {
            return;
        }
        let pacote = self.pacote.clone();
        let i = self.indice;
        self.trabalho = Some(Trabalho::iniciar(executor, ctx, move |prog| {
            let m = &pacote.missoes[i];
            let r = rodar(m, &aluno, &[m.missao.decisoes_iniciais()], prog)
                .map_err(|e| e.to_string())
                .and_then(|c| {
                    m.missao
                        .medicoes
                        .iter()
                        .map(|md| {
                            let v = Intervalo::de(
                                &avaliar(&m.missao, &md.valor, &c[0]).map_err(|e| e.to_string())?,
                            );
                            Ok(format!(
                                "{}: {} ± {} {}",
                                md.rotulo,
                                fmt_num(v.media, md.casas),
                                fmt_num(v.meia_largura, md.casas),
                                md.unidade
                            ))
                        })
                        .collect()
                });
            Rodada::Medicoes(r)
        }));
    }
}

/// Applies a finished job: progress, signed journal, results.
pub fn aplicar_rodada(app: &mut App, r: Rodada) {
    let Some(m) = app.missao.as_mut() else { return };
    match r {
        Rodada::Medicoes(Ok(v)) => m.medicoes = Some(v),
        Rodada::Medicoes(Err(e)) => m.erro = Some(e),
        Rodada::Resultado(b) => match *b {
            Err(e) => m.erro = Some(e),
            Ok((t, res, rep)) => {
                m.rodadas += 1;
                m.previsao_travada = true;
                let id = m.missao().missao.id.clone();
                let unidade = m
                    .missao()
                    .missao
                    .animacao
                    .unidade
                    .clone()
                    .unwrap_or_else(|| "min".into());
                m.animacao = Some(Animacao::new(rep, &unidade));
                app.progresso.registrar(&id, &res, t.dicas);
                m.resultado = Some(res.clone());
                m.rolar_resultado = true;
                let pacote = m.pacote.clone();
                if let Err(e) = app
                    .perfil
                    .salvar_progresso(&pacote.pacote.id, &app.progresso)
                {
                    app.mensagem = Some(texto::f(texto::missao_erro_salvar_progresso(), &[&e]));
                }
                // A solved mission records the practice in the company program.
                if res.resolvida
                    && pacote.pacote.prova.is_none()
                    && let Some(prog) = app.config.programa_seis_sigma.clone()
                {
                    let r = utsim_ensino_store::seis_sigma::identidade_local().and_then(|me| {
                        utsim_ensino_store::seis_sigma::registrar_competencias(
                            &prog,
                            &me,
                            &utsim_lss_store::SystemClock,
                            &pacote,
                            &app.progresso,
                            Some(&id),
                        )
                    });
                    match r {
                        Ok(x) if !x.erros.is_empty() => {
                            app.mensagem = Some(texto::f(
                                texto::missao_competencias_erros(),
                                &[&x.erros.join("; ")],
                            ))
                        }
                        Ok(_) => {}
                        Err(e) => {
                            app.mensagem = Some(texto::f(
                                texto::missao_competencias_nao_registradas(),
                                &[&e],
                            ))
                        }
                    }
                }
                match app
                    .perfil
                    .registrar(&pacote, &id, &t, Some(res.nota), (app.relogio)())
                {
                    Ok((_, Some(aviso))) => app.mensagem = Some(aviso),
                    Ok(_) => {}
                    Err(e) => {
                        app.mensagem =
                            Some(texto::f(texto::missao_tentativa_nao_registrada(), &[&e]))
                    }
                }
            }
        },
    }
}

fn situacao_texto(s: Situacao) -> (&'static str, Color32) {
    match s {
        Situacao::Atingida => (texto::missao_meta_atingida(), VERDE),
        Situacao::DentroDaTolerancia => (texto::missao_dentro_tolerancia(), AMARELO),
        Situacao::NaoAtingida => (texto::missao_meta_nao_atingida(), VERMELHO),
        Situacao::SemMeta => (texto::missao_sem_meta(), Color32::GRAY),
    }
}

/// The title of a part of the mission (prediction, decisions...), a step above the body.
fn secao(ui: &mut Ui, titulo: &str) {
    ui.add_space(6.0);
    ui.label(RichText::new(titulo).size(17.0).strong());
}

/// The label of a slider in a column of fixed width, so the sliders line up.
fn rotulo_alinhado(ui: &mut Ui, rotulo: &str) {
    let r = ui.label(rotulo);
    ui.add_space((300.0 - r.rect.width()).max(6.0));
    ui.spacing_mut().slider_width = 220.0;
}

/// Draws one lever; returns whether it changed.
fn alavanca(ui: &mut Ui, a: &Alavanca, v: &mut f64, id: &str) -> bool {
    let antes = *v;
    match a {
        Alavanca::Quantidade {
            de, ate, rotulo, ..
        } => {
            let mut n = v.round() as i64;
            ui.horizontal(|ui| {
                rotulo_alinhado(ui, rotulo);
                ui.add(egui::Slider::new(&mut n, *de..=*ate));
                if let Alavanca::Quantidade { custo_unitario, .. } = a
                    && *custo_unitario > 0.0
                {
                    ui.label(
                        RichText::new(texto::f(
                            texto::missao_cada_unidade(),
                            &[&dinheiro(*custo_unitario)],
                        ))
                        .weak(),
                    );
                }
            });
            *v = n as f64;
        }
        Alavanca::Valor {
            de,
            ate,
            passo,
            rotulo,
            unidade,
            ..
        } => {
            ui.horizontal(|ui| {
                rotulo_alinhado(ui, rotulo);
                ui.add(
                    egui::Slider::new(v, *de..=*ate)
                        .step_by(*passo)
                        .suffix(format!(" {unidade}")),
                );
            });
        }
        Alavanca::Escolha { opcoes, rotulo, .. } => {
            ui.label(rotulo);
            let mut sel = v.round().max(0.0) as usize;
            let rotulos: Vec<String> = opcoes
                .iter()
                .map(|o| {
                    if o.custo > 0.0 {
                        format!("{} ({})", o.rotulo, dinheiro(o.custo))
                    } else {
                        o.rotulo.clone()
                    }
                })
                .collect();
            em_linha_ou_coluna(ui, &rotulos, |ui| {
                for (i, r) in rotulos.iter().enumerate() {
                    ui.push_id((id, i), |ui| ui.radio_value(&mut sel, i, r));
                }
            });
            *v = sel as f64;
        }
    }
    (antes - *v).abs() > 1e-12
}

pub fn tela_missao(app: &mut App, ui: &mut Ui) {
    let aluno = app.aluno();
    let nome_aluno = app
        .config
        .identidade
        .as_ref()
        .map(|a| a.nome.clone())
        .unwrap_or_default();
    let ctx = ui.ctx().clone();
    let executor = app.executor.clone();
    let agora = (app.relogio)();
    let prova = app.prova();
    // Whether the mission after this one is open (asked before borrowing the mission).
    let proxima = app.missao.as_ref().is_some_and(|m| {
        m.indice + 1 < m.pacote.missoes.len() && app.progresso.liberada(&m.pacote, m.indice + 1)
    });
    let mut seguir: Option<bool> = None;
    let Some(m) = app.missao.as_mut() else {
        app.tela = Tela::Mapa;
        return;
    };
    let mc = m.missao().clone();
    let missao = &mc.missao;
    ui.horizontal(|ui| {
        if ui.button(texto::missao_campanha()).clicked() {
            app.tela = Tela::Mapa;
        }
        ui.label(
            RichText::new(texto::f_literal(
                texto::missao_numero(),
                &[&numero_missao(&m.pacote, m.indice)],
            ))
            .weak(),
        );
    });
    let Some(m) = app.missao.as_mut() else { return };
    crate::cena::desenhar(ui, &mc, &m.tentativa.decisoes, m.resultado.as_ref());
    texto::markdown(ui, &mc.texto);
    if !missao.conceitos.is_empty() {
        let nomes: Vec<String> = missao
            .conceitos
            .iter()
            .filter_map(|c| {
                m.pacote
                    .glossario
                    .termos
                    .iter()
                    .find(|t| &t.id == c)
                    .map(|t| t.nome.clone())
            })
            .collect();
        ui.label(RichText::new(texto::f(texto::missao_conceitos(), &[&nomes.join(", ")])).weak());
    }
    ui.separator();

    if !missao.medicoes.is_empty() {
        secao(ui, texto::missao_medicoes());
        match &m.medicoes {
            Some(v) => {
                for l in v {
                    ui.label(l);
                }
            }
            None => {
                if ui
                    .add_enabled(m.trabalho.is_none(), egui::Button::new(texto::medir()))
                    .clicked()
                {
                    m.medir(aluno.clone(), executor.as_ref(), Some(ctx.clone()));
                }
            }
        }
        ui.add_space(6.0);
    }

    if !missao.dados.is_empty() {
        secao(ui, texto::missao_dados_titulo());
        ui.label(RichText::new(texto::missao_dados_texto()).weak());
        if m.dados_gerados.as_ref().is_none_or(|(a, _)| *a != aluno) {
            let v = missao
                .dados
                .iter()
                .map(|cj| cj.gerar_com(&missao.id, &aluno, &mc.modelo))
                .collect();
            m.dados_gerados = Some((aluno.clone(), v));
        }
        let gerados = m
            .dados_gerados
            .as_ref()
            .map_or(&[][..], |(_, v)| v.as_slice());
        for (cj, c) in missao.dados.iter().zip(gerados) {
            ui.collapsing(
                texto::f(texto::missao_dados_linhas(), &[&cj.nome, &c.linhas.len()]),
                |ui| {
                    egui::Grid::new(("dados", &cj.id))
                        .striped(true)
                        .show(ui, |ui| {
                            for h in &c.cabecalho {
                                ui.label(RichText::new(h).strong());
                            }
                            ui.end_row();
                            for l in c.linhas.iter().take(12) {
                                for v in l {
                                    ui.label(v);
                                }
                                ui.end_row();
                            }
                        });
                    if c.linhas.len() > 12 {
                        ui.label(
                            RichText::new(texto::f(
                                texto::missao_mais_linhas(),
                                &[&(c.linhas.len() - 12)],
                            ))
                            .weak(),
                        );
                    }
                    ui.label(RichText::new(texto::missao_ferramentas()).strong());
                    for l in utsim_ensino_core::questoes::ferramenta(cj, c) {
                        ui.label(l);
                    }
                },
            );
        }
        ui.horizontal(|ui| {
            ui.label(texto::missao_pasta());
            crate::abertura::campo(
                ui,
                &mut m.pasta_dados,
                seletor::Tipo::Pasta,
                texto::sel_pasta(),
                &[],
                360.0,
            );
            if ui.button(texto::missao_gravar_csv()).clicked() {
                let pasta = std::path::PathBuf::from(m.pasta_dados.trim());
                let r = std::fs::create_dir_all(&pasta)
                    .map_err(|e| e.to_string())
                    .and_then(|_| {
                        for cj in &missao.dados {
                            let c = cj.gerar_com(&missao.id, &aluno, &mc.modelo);
                            std::fs::write(pasta.join(format!("{}.csv", cj.id)), c.csv())
                                .map_err(|e| e.to_string())?;
                        }
                        Ok(missao.dados.len())
                    });
                m.aviso_dados = Some(match r {
                    Ok(n) => texto::f(texto::missao_arquivos_gravados(), &[&n, &pasta.display()]),
                    Err(e) => texto::f(texto::missao_erro_gravar(), &[&e]),
                });
            }
            if ui.button(texto::missao_levar_seis_sigma()).clicked() {
                let pasta = std::path::PathBuf::from(m.pasta_dados.trim());
                m.aviso_dados = Some(
                    match utsim_ensino_store::seis_sigma::exportar(&pasta, &mc, &aluno, &nome_aluno)
                    {
                        Ok(p) => texto::f(texto::missao_projeto_criado(), &[&p.display()]),
                        Err(e) => texto::f(texto::missao_erro_projeto(), &[&e]),
                    },
                );
            }
        });
        // The apps of the family named by the mission, when installed.
        let apps: Vec<&String> = missao
            .analisar_em
            .iter()
            .filter(|a| app.apps_analise.contains(a))
            .collect();
        if !apps.is_empty() {
            ui.horizontal(|ui| {
                for produto in apps {
                    let rotulo = match produto.as_str() {
                        "estatistica" => texto::missao_analisar_estatistica(),
                        _ => texto::missao_analisar_qualidade(),
                    };
                    if ui
                        .button(rotulo)
                        .on_hover_text(texto::missao_analisar_dica())
                        .clicked()
                    {
                        let dir = app.perfil.dir.join("dados");
                        m.aviso_dados = Some(
                            match utsim_ensino_store::analise::analisar(produto, &dir, &mc, &aluno)
                            {
                                Ok(arqs) => texto::f(
                                    texto::missao_dados_abertos(),
                                    &[
                                        &arqs.len(),
                                        &arqs
                                            .first()
                                            .and_then(|p| p.parent())
                                            .map(|p| p.display().to_string())
                                            .unwrap_or_default(),
                                    ],
                                ),
                                Err(e) => texto::f(texto::missao_erro_gravar(), &[&e]),
                            },
                        );
                    }
                }
            });
        }
        if let Some(a) = &m.aviso_dados {
            ui.label(a);
        }
        ui.add_space(6.0);
    }

    if !missao.previsao.is_empty() {
        secao(ui, texto::missao_previsao());
        ui.label(RichText::new(texto::prever_antes()).weak());
        let travada = m.previsao_travada;
        for p in &missao.previsao {
            ui.label(p.pergunta());
            ui.add_enabled_ui(!travada, |ui| match p {
                Previsao::Escolha { id, opcoes, .. } => {
                    let atual = match m.tentativa.previsoes.get(id) {
                        Some(RespostaPrevisao::Escolha(i)) => Some(*i),
                        _ => None,
                    };
                    let rotulos: Vec<&str> = opcoes.iter().map(|o| o.rotulo.as_str()).collect();
                    em_linha_ou_coluna(ui, &rotulos, |ui| {
                        for (i, o) in opcoes.iter().enumerate() {
                            if ui.radio(atual == Some(i), &o.rotulo).clicked() {
                                m.tentativa
                                    .previsoes
                                    .insert(id.clone(), RespostaPrevisao::Escolha(i));
                            }
                        }
                    });
                }
                Previsao::Dados {
                    id,
                    conjunto,
                    questao,
                    unidade,
                    ..
                } if questao.e_escolha() => {
                    let opcoes = missao
                        .conjunto(conjunto)
                        .map(|c| questao.opcoes(c))
                        .unwrap_or_default();
                    let atual = match m.tentativa.previsoes.get(id) {
                        Some(RespostaPrevisao::Escolha(i)) => Some(*i),
                        _ => None,
                    };
                    ui.horizontal_wrapped(|ui| {
                        for (i, o) in opcoes.iter().enumerate() {
                            if ui.radio(atual == Some(i), o).clicked() {
                                m.tentativa
                                    .previsoes
                                    .insert(id.clone(), RespostaPrevisao::Escolha(i));
                            }
                        }
                        let _ = unidade;
                    });
                }
                Previsao::Valor { id, unidade, .. }
                | Previsao::Calculo { id, unidade, .. }
                | Previsao::Dados { id, unidade, .. } => {
                    ui.horizontal(|ui| {
                        let txt = m.previsao_texto.entry(id.clone()).or_default();
                        if ui.text_edit_singleline(txt).changed() {
                            match txt.trim().replace(',', ".").parse::<f64>() {
                                Ok(x) if x.is_finite() => {
                                    m.tentativa
                                        .previsoes
                                        .insert(id.clone(), RespostaPrevisao::Valor(x));
                                }
                                _ => {
                                    m.tentativa.previsoes.remove(id);
                                }
                            }
                        }
                        ui.label(unidade);
                    });
                }
            });
        }
        ui.add_space(6.0);
    }

    if !missao.alavancas.is_empty() {
        secao(ui, texto::missao_decisoes());
        for a in &missao.alavancas {
            let v = m
                .tentativa
                .decisoes
                .entry(a.id().to_string())
                .or_insert(a.inicial());
            alavanca(ui, a, v, a.id());
        }
        let custo = missao.custo(&m.tentativa.decisoes);
        match missao.orcamento {
            Some(o) => {
                let cor = if custo > o + 1e-9 { VERMELHO } else { VERDE };
                ui.colored_label(
                    cor,
                    texto::f(
                        texto::missao_custo_orcamento(),
                        &[&dinheiro(custo), &dinheiro(o)],
                    ),
                );
            }
            None => {
                ui.label(texto::f(texto::missao_custo(), &[&dinheiro(custo)]));
            }
        }
        if let Some(motivo) = utsim_ensino_core::nota::portao(missao, &m.tentativa.decisoes) {
            ui.colored_label(VERMELHO, texto::f(texto::missao_restricao(), &[&motivo]));
        }
        ui.add_space(6.0);
    }

    if let Some(b) = missao.balanceamento.as_ref().filter(|b| b.editavel) {
        secao(ui, texto::missao_balanceamento());
        crate::pecas::quadro_balanceamento(ui, b, &mut m.tentativa.decisoes, &mut m.selecionada);
    }
    if let Some(a) = &missao.arranjo {
        secao(ui, texto::missao_arranjo());
        crate::pecas::grade_arranjo(ui, a, &mut m.tentativa.decisoes, &mut m.selecionada);
    }
    if missao.tem_pecas()
        && let Some(motivo) = utsim_ensino_core::nota::portao(missao, &m.tentativa.decisoes)
    {
        ui.colored_label(VERMELHO, texto::f(texto::missao_restricao(), &[&motivo]));
    }
    if missao.tem_pecas() {
        ui.add_space(6.0);
    }

    if missao.modelagem.is_some() {
        secao(ui, texto::missao_seu_modelo());
        ui.label(RichText::new(texto::missao_modelo_texto()).weak());
        ui.horizontal(|ui| {
            ui.label(texto::missao_arquivo_ron());
            crate::abertura::campo(
                ui,
                &mut m.caminho_modelo,
                seletor::Tipo::Salvar,
                texto::sel_gravar_modelo(),
                &[seletor::RON],
                360.0,
            );
            if ui.button(texto::missao_gravar_modelo_inicial()).clicked() {
                let r = ron::ser::to_string_pretty(
                    &mc.modelo,
                    ron::ser::PrettyConfig::default().struct_names(true),
                )
                .map_err(|e| e.to_string())
                .and_then(|t| {
                    std::fs::write(m.caminho_modelo.trim(), t).map_err(|e| e.to_string())
                });
                m.aviso_modelo = Some(match r {
                    Ok(()) => texto::f(
                        texto::missao_modelo_inicial_gravado(),
                        &[&m.caminho_modelo.trim()],
                    ),
                    Err(e) => texto::f(texto::missao_erro_gravar(), &[&e]),
                });
            }
            if ui.button(texto::missao_ler_modelo()).clicked() {
                match std::fs::read_to_string(m.caminho_modelo.trim()) {
                    Ok(t) => {
                        m.aviso_modelo =
                            Some(texto::f(texto::missao_modelo_lido(), &[&t.lines().count()]));
                        m.tentativa.modelo = Some(t);
                    }
                    Err(e) => m.aviso_modelo = Some(texto::f(texto::missao_erro_ler(), &[&e])),
                }
            }
        });
        if let Some(a) = &m.aviso_modelo {
            ui.label(a);
        } else if m.tentativa.modelo.is_none() {
            ui.label(RichText::new(texto::missao_nenhum_modelo()).weak());
        }
        ui.add_space(6.0);
    }

    if !missao.justificativa.is_empty() {
        secao(ui, texto::missao_justificativa());
        for (qi, q) in missao.justificativa.iter().enumerate() {
            ui.label(&q.pergunta);
            em_linha_ou_coluna(ui, &q.opcoes, |ui| {
                for (i, o) in q.opcoes.iter().enumerate() {
                    let sel = m.tentativa.justificativas.get(qi).copied().flatten() == Some(i);
                    if ui.radio(sel, o).clicked() {
                        if m.tentativa.justificativas.len() <= qi {
                            m.tentativa.justificativas.resize(qi + 1, None);
                        }
                        m.tentativa.justificativas[qi] = Some(i);
                    }
                }
            });
        }
        ui.add_space(6.0);
    }

    if !missao.dicas.is_empty() && prova.is_none() {
        ui.collapsing(texto::missao_dicas(), |ui| {
            let pesos = &m.pacote.pacote.pesos;
            for (i, d) in missao.dicas.iter().enumerate() {
                let nivel = i as u8 + 1;
                if m.tentativa.dicas >= nivel {
                    ui.label(format!("{nivel}. {d}"));
                } else {
                    let desc = pesos.dicas[i];
                    let rot = if desc > 0.0 {
                        texto::f(texto::missao_ver_dica_desconto(), &[&nivel, &desc])
                    } else {
                        texto::f(texto::missao_ver_dica(), &[&nivel])
                    };
                    if ui.button(rot).clicked() {
                        m.tentativa.dicas = nivel;
                    }
                    break;
                }
            }
        });
    }

    ui.separator();
    // An exam: inside the window and the time, and up to the runs allowed.
    let bloqueio = prova.as_ref().and_then(|(pr, aberta)| {
        pr.pode_rodar(agora, *aberta).err().or_else(|| {
            (m.rodadas >= pr.tentativas)
                .then(|| texto::f(texto::missao_rodadas_usadas(), &[&pr.tentativas]))
        })
    });
    if let Some((pr, aberta)) = &prova {
        ui.colored_label(AMARELO, crate::texto_prova(pr, *aberta, agora));
        ui.ctx()
            .request_repaint_after(std::time::Duration::from_secs(20));
    }
    if let Some(b) = &bloqueio {
        ui.colored_label(VERMELHO, texto::f(texto::missao_simular_bloqueado(), &[b]));
    }
    ui.horizontal(|ui| {
        let pode = m.trabalho.is_none() && m.previsoes_completas() && bloqueio.is_none();
        if ui
            .add_enabled(
                pode,
                egui::Button::new(
                    RichText::new(texto::simular())
                        .size(16.0)
                        .strong()
                        .color(Color32::WHITE),
                )
                .fill(crate::ROXO)
                .min_size(egui::vec2(120.0, 32.0)),
            )
            .clicked()
        {
            m.simular(aluno.clone(), executor.as_ref(), Some(ctx.clone()));
        }
        if !m.previsoes_completas() {
            ui.label(RichText::new(texto::missao_responda_previsoes()).weak());
        }
        if let Some((pr, _)) = &prova {
            ui.label(texto::f(
                texto::missao_rodadas_de(),
                &[&m.rodadas, &pr.tentativas],
            ));
        } else if let Some(o) = missao.rodadas.orcamento {
            ui.label(texto::f(
                texto::missao_rodadas_sem_desconto(),
                &[&m.rodadas, &o],
            ));
        } else {
            ui.label(texto::f(texto::missao_rodadas_feitas(), &[&m.rodadas]));
        }
        if let Some(t) = &m.trabalho {
            ui.add(
                egui::ProgressBar::new(t.fracao())
                    .desired_width(160.0)
                    .show_percentage(),
            );
            if ui.button(texto::cancelar()).clicked() {
                t.cancelar.store(true, Ordering::Relaxed);
            }
            ui.ctx()
                .request_repaint_after(std::time::Duration::from_millis(100));
        }
    });
    if let Some(e) = &m.erro {
        ui.colored_label(VERMELHO, e);
    }
    if let Some(r) = m.resultado.clone() {
        ui.separator();
        if std::mem::take(&mut m.rolar_resultado) {
            ui.scroll_to_cursor(Some(egui::Align::TOP));
        }
        resultado(ui, &r, missao, prova.is_some());
        // Solved: the next step right under the grade, not after the animation.
        if prova.is_none() && r.resolvida {
            ui.add_space(4.0);
            ui.horizontal(|ui| {
                if proxima
                    && ui
                        .add(
                            egui::Button::new(
                                RichText::new(texto::missao_proxima())
                                    .strong()
                                    .color(Color32::WHITE),
                            )
                            .fill(crate::ROXO),
                        )
                        .clicked()
                {
                    seguir = Some(true);
                }
                if ui.button(texto::missao_caixa_areia()).clicked() {
                    seguir = Some(false);
                }
            });
        }
    }
    if let Some(a) = m.animacao.as_mut() {
        ui.separator();
        secao(ui, texto::missao_sua_fabrica());
        a.mostrar(ui);
    }
    if app.cliente_3d && m.animacao.is_some() {
        ui.horizontal(|ui| {
            if ui.button(texto::missao_ver_3d()).clicked() {
                let dir = app.perfil.dir.join("3d");
                // The client opens in its own window; only a failure is said here.
                m.erro = utsim_ensino_store::tres_d::abrir_em_3d(
                    &dir,
                    &mc,
                    &aluno,
                    &m.tentativa.decisoes,
                )
                .err()
                .map(|e| texto::f(texto::missao_erro_3d(), &[&e]));
            }
            // Out of an exam: the model goes on in the full simulator.
            if prova.is_none()
                && ui
                    .button(texto::missao_levar_utsim())
                    .on_hover_text(texto::missao_levar_utsim_dica())
                    .clicked()
            {
                let dir = app.perfil.dir.join("modelos");
                match utsim_ensino_store::tres_d::levar_para_utsim(
                    &dir,
                    &mc,
                    &aluno,
                    &m.tentativa.decisoes,
                ) {
                    Ok(p) => {
                        app.mensagem =
                            Some(texto::f(texto::missao_levado_utsim(), &[&p.display()]));
                    }
                    Err(e) => m.erro = Some(texto::f(texto::missao_erro_3d(), &[&e])),
                }
            }
        });
    }
    // The 3D error is told by the start of its text, in the language now.
    let prefixo_3d = texto::f(texto::missao_erro_3d(), &[&""]);
    if let Some(e) = m
        .erro
        .as_ref()
        .filter(|e| e.starts_with(prefixo_3d.as_str()))
    {
        ui.colored_label(VERMELHO, e);
    }
    let Some(i) = app.missao.as_ref().map(|m| m.indice) else {
        return;
    };
    match seguir {
        Some(true) => app.abrir_missao(i + 1),
        Some(false) => app.abrir_areia(i),
        None => {}
    }
}

fn resultado(ui: &mut Ui, r: &Resultado, missao: &utsim_ensino_core::missao::Missao, prova: bool) {
    if prova {
        // No grade in an exam: the indicators only (the goals are in the statement).
        ui.label(RichText::new(texto::missao_resultado_rodada()).strong());
        ui.label(RichText::new(texto::missao_nota_professor()).weak());
        tabela_metricas(ui, r, false);
        return;
    }
    let estrelas = format!(
        "{}{}",
        "★".repeat(r.estrelas as usize),
        "☆".repeat(3 - r.estrelas.min(3) as usize)
    );
    ui.horizontal(|ui| {
        ui.label(
            RichText::new(texto::f(texto::missao_nota(), &[&fmt_num(r.nota, 1)]))
                .size(22.0)
                .strong(),
        );
        ui.label(RichText::new(estrelas).size(22.0));
        if r.resolvida {
            ui.colored_label(VERDE, texto::missao_resolvida());
        } else {
            ui.colored_label(VERMELHO, texto::missao_nao_resolvida());
        }
        if r.selo {
            ui.colored_label(AMARELO, texto::missao_selo());
        }
    });
    if let Some(p) = &r.portao {
        ui.colored_label(VERMELHO, texto::f(texto::missao_restricao_violada(), &[p]));
    }
    match utsim_ensino_core::nota::mais_longe(r) {
        Some(utsim_ensino_core::nota::MaisLonge::Metrica { rotulo, falta }) => {
            ui.colored_label(
                VERMELHO,
                texto::f_literal(texto::missao_mais_longe(), &[rotulo.as_str(), falta.as_str()]),
            );
        }
        Some(utsim_ensino_core::nota::MaisLonge::Criterio { rotulo }) => {
            ui.colored_label(
                VERMELHO,
                texto::f_literal(texto::missao_criterio_primeiro(), &[rotulo.as_str()]),
            );
        }
        None => {}
    }
    tabela_metricas(ui, r, true);
    ui.label(texto::f(
        texto::missao_pontos(),
        &[
            &fmt_num(r.pontos_desempenho, 1),
            &fmt_num(r.pontos_previsao, 1),
            &fmt_num(r.pontos_justificativa, 1),
            &fmt_num(r.pontos_possiveis, 0),
            &fmt_num(r.desconto_dicas, 0),
            &fmt_num(r.desconto_rodadas, 0),
        ],
    ));
    resto_do_resultado(ui, r, missao);
}

/// The indicators of a run, with the situation against the goal when `situacao`.
fn tabela_metricas(ui: &mut Ui, r: &Resultado, situacao: bool) {
    if !r.metricas.is_empty() {
        egui::Grid::new("metricas").striped(true).show(ui, |ui| {
            ui.label(RichText::new(texto::missao_indicador()).strong());
            ui.label(RichText::new(texto::missao_estado_inicial()).strong());
            ui.label(RichText::new(texto::missao_sua_tentativa()).strong());
            ui.label(RichText::new(texto::missao_meta()).strong());
            if situacao {
                ui.label(RichText::new(texto::missao_situacao()).strong());
            }
            ui.end_row();
            for m in &r.metricas {
                let f = |i: &Intervalo| {
                    format!(
                        "{} ± {} {}",
                        fmt_num(i.media, m.casas),
                        fmt_num(i.meia_largura, m.casas),
                        m.unidade
                    )
                };
                ui.label(&m.rotulo);
                ui.label(f(&m.base));
                ui.label(f(&m.aluno));
                let sinal = match m.objetivo {
                    utsim_ensino_core::missao::Objetivo::Menor => "≤",
                    utsim_ensino_core::missao::Objetivo::Maior => "≥",
                };
                ui.label(m.meta.map_or_else(
                    || "-".into(),
                    |v| format!("{sinal} {}", fmt_num(v, m.casas)),
                ));
                if situacao {
                    let (t, c) = situacao_texto(m.situacao);
                    ui.colored_label(c, t);
                }
                ui.end_row();
            }
        });
    }
}

fn resto_do_resultado(ui: &mut Ui, r: &Resultado, missao: &utsim_ensino_core::missao::Missao) {
    for (p, q) in r.previsoes.iter().zip(&missao.previsao) {
        if p.certa {
            ui.colored_label(
                VERDE,
                texto::f(
                    texto::missao_previsao_certa(),
                    &[&q.pergunta(), &p.esperado],
                ),
            );
        } else {
            ui.colored_label(
                VERMELHO,
                texto::f(
                    texto::missao_previsao_errada(),
                    &[&q.pergunta(), &p.esperado],
                ),
            );
        }
    }
    if !missao.justificativa.is_empty() {
        ui.label(texto::f(
            texto::missao_justificativa_certas(),
            &[&r.justificativas_certas, &missao.justificativa.len()],
        ));
    }
    if !r.criterios.is_empty() {
        ui.label(RichText::new(texto::missao_criterios()).strong());
        for k in &r.criterios {
            let (marca, cor) = if k.ok {
                (texto::missao_criterio_ok(), VERDE)
            } else {
                (texto::missao_criterio_falta(), VERMELHO)
            };
            ui.horizontal_wrapped(|ui| {
                ui.colored_label(cor, format!("{marca}: {}", k.rotulo));
                ui.label(RichText::new(&k.detalhe).weak());
            });
        }
    }
    if !r.dicas_automaticas.is_empty() {
        ui.label(RichText::new(texto::missao_o_que_aconteceu()).strong());
        for d in &r.dicas_automaticas {
            ui.label(format!("• {d}"));
        }
    }
}

/// The sandbox: two scenarios side by side, paired difference with interval.
pub struct EstadoAreia {
    pub pacote: Arc<PacoteCarregado>,
    pub indice: usize,
    pub a: Decisoes,
    pub b: Decisoes,
    pub trabalho: Option<Trabalho<Result<Vec<Cenario>, String>>>,
    pub resultado: Option<Vec<Cenario>>,
    pub erro: Option<String>,
    pub caminho_ron: String,
}

impl EstadoAreia {
    pub fn new(pacote: Arc<PacoteCarregado>, indice: usize) -> EstadoAreia {
        let m = &pacote.missoes[indice].missao;
        EstadoAreia {
            a: m.decisoes_iniciais(),
            b: m.decisoes_iniciais(),
            indice,
            trabalho: None,
            resultado: None,
            erro: None,
            caminho_ron: String::new(),
            pacote,
        }
    }

    pub fn receber(&mut self) {
        let Some(r) = self.trabalho.as_ref().and_then(|t| t.rx.try_recv().ok()) else {
            return;
        };
        self.trabalho = None;
        match r {
            Ok(v) => self.resultado = Some(v),
            Err(e) => self.erro = Some(e),
        }
    }

    pub fn comparar(
        &mut self,
        aluno: String,
        executor: &dyn crate::Executor,
        ctx: Option<egui::Context>,
    ) {
        let pacote = self.pacote.clone();
        let i = self.indice;
        let lista = vec![self.a.clone(), self.b.clone()];
        self.erro = None;
        self.trabalho = Some(Trabalho::iniciar(executor, ctx, move |prog| {
            rodar(&pacote.missoes[i], &aluno, &lista, prog).map_err(|e| e.to_string())
        }));
    }
}

pub fn tela_areia(app: &mut App, ui: &mut Ui) {
    let aluno = app.aluno();
    let ctx = ui.ctx().clone();
    let executor = app.executor.clone();
    let Some(s) = app.areia.as_mut() else {
        app.tela = Tela::Mapa;
        return;
    };
    let mc = s.pacote.missoes[s.indice].clone();
    let missao = &mc.missao;
    if ui.button(texto::missao_campanha()).clicked() {
        app.tela = Tela::Mapa;
        return;
    }
    ui.heading(texto::f(texto::missao_areia_titulo(), &[&missao.titulo]));
    ui.label(texto::areia_texto());
    ui.columns(2, |cols| {
        for (k, (nome, d)) in [
            (texto::missao_cenario_a(), &mut s.a),
            (texto::missao_cenario_b(), &mut s.b),
        ]
        .into_iter()
        .enumerate()
        {
            let ui = &mut cols[k];
            ui.label(RichText::new(nome).strong());
            for a in &missao.alavancas {
                let v = d.entry(a.id().to_string()).or_insert(a.inicial());
                alavanca(ui, a, v, &format!("{nome}{}", a.id()));
            }
            ui.label(texto::f(
                texto::missao_custo_curto(),
                &[&dinheiro(missao.custo(d))],
            ));
        }
    });
    ui.horizontal(|ui| {
        if ui
            .add_enabled(
                s.trabalho.is_none(),
                egui::Button::new(texto::missao_comparar()),
            )
            .clicked()
        {
            s.comparar(aluno.clone(), executor.as_ref(), Some(ctx.clone()));
        }
        if let Some(t) = &s.trabalho {
            ui.add(egui::ProgressBar::new(t.fracao()).desired_width(160.0));
            ui.ctx()
                .request_repaint_after(std::time::Duration::from_millis(100));
        }
    });
    if let Some(e) = &s.erro {
        ui.colored_label(VERMELHO, e);
    }
    if let Some(v) = &s.resultado {
        egui::Grid::new("areia").striped(true).show(ui, |ui| {
            for h in [
                texto::missao_indicador(),
                texto::missao_coluna_a(),
                texto::missao_coluna_b(),
                texto::missao_diferenca_pareada(),
                "",
            ] {
                ui.label(RichText::new(h).strong());
            }
            ui.end_row();
            for m in &missao.metricas {
                let (Ok(xa), Ok(xb)) = (
                    valor_metrica(missao, m, &v[0]),
                    valor_metrica(missao, m, &v[1]),
                ) else {
                    continue;
                };
                let (ia, ib) = (Intervalo::de(&xa), Intervalo::de(&xb));
                let d = paired_difference(&xb, &xa);
                let hw = if d.half_width.is_nan() {
                    0.0
                } else {
                    d.half_width
                };
                ui.label(&m.rotulo);
                ui.label(format!(
                    "{} ± {}",
                    fmt_num(ia.media, m.casas),
                    fmt_num(ia.meia_largura, m.casas)
                ));
                ui.label(format!(
                    "{} ± {}",
                    fmt_num(ib.media, m.casas),
                    fmt_num(ib.meia_largura, m.casas)
                ));
                ui.label(format!(
                    "{} ± {}",
                    fmt_num(d.mean, m.casas),
                    fmt_num(hw, m.casas)
                ));
                ui.label(if is_significant(&d) {
                    texto::missao_diferenca_significativa()
                } else {
                    texto::missao_empate()
                });
                ui.end_row();
            }
        });
    }
    ui.separator();
    ui.label(texto::missao_levar_cenario());
    ui.horizontal(|ui| {
        crate::abertura::campo(
            ui,
            &mut s.caminho_ron,
            seletor::Tipo::Salvar,
            texto::sel_gravar_modelo(),
            &[seletor::RON],
            360.0,
        );
        if ui.button(texto::missao_gravar_ron()).clicked() {
            let r = modelo_da_tentativa(&mc, &aluno, &s.a)
                .map_err(|e| e.to_string())
                .and_then(|m| {
                    ron::ser::to_string_pretty(
                        &m,
                        ron::ser::PrettyConfig::default().struct_names(true),
                    )
                    .map_err(|e| e.to_string())
                })
                .and_then(|t| std::fs::write(s.caminho_ron.trim(), t).map_err(|e| e.to_string()));
            app.mensagem = Some(match r {
                Ok(()) => texto::f(texto::missao_modelo_gravado(), &[&s.caminho_ron.trim()]),
                Err(e) => texto::f(texto::missao_erro_gravar(), &[&e]),
            });
        }
    });
}

/// Options side by side when they are short, one under the other when long: a long label at
/// the end of a row would wrap into a column one word wide.
fn em_linha_ou_coluna<S: AsRef<str>>(ui: &mut Ui, rotulos: &[S], conteudo: impl FnOnce(&mut Ui)) {
    let total: usize = rotulos.iter().map(|r| r.as_ref().chars().count() + 6).sum();
    // About 7 points a character in the body font.
    if total as f32 * 7.0 <= ui.available_width() {
        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing.x = 14.0;
            conteudo(ui)
        });
    } else {
        ui.vertical(conteudo);
    }
}
