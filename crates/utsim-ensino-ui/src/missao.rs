//! The mission screen (statement, measures, prediction, levers with the budget,
//! justification, hints, run, results with intervals and the 2D view) and the sandbox.

use std::collections::BTreeMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::mpsc::{Receiver, channel};

use egui::{Color32, RichText, Ui};
use utsim_ensino_core::missao::{Alavanca, Decisoes, Previsao};
use utsim_ensino_core::motor::{Cenario, avaliar, modelo_da_tentativa, replay, rodar, simular};
use utsim_ensino_core::nota::{
    Intervalo, RespostaPrevisao, Resultado, Situacao, Tentativa, corrigir, dinheiro, fmt_num,
};
use utsim_ensino_core::pacote::{MissaoCarregada, PacoteCarregado};
use utsim_model::analysis::{is_significant, paired_difference};
use utsim_model::replay::Replay;

use crate::vista2d::Animacao;
use crate::{App, Tela, numero_missao, texto};

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
        let pacote = self.pacote.clone();
        let i = self.indice;
        self.erro = None;
        self.trabalho = Some(Trabalho::iniciar(executor, ctx, move |prog| {
            let m = &pacote.missoes[i];
            let r = (|| {
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
                let pacote = m.pacote.clone();
                if let Err(e) = app
                    .perfil
                    .salvar_progresso(&pacote.pacote.id, &app.progresso)
                {
                    app.mensagem = Some(format!("não foi possível salvar o progresso: {e}"));
                }
                match app
                    .perfil
                    .registrar(&pacote, &id, &t, Some(res.nota), (app.relogio)())
                {
                    Ok((_, Some(aviso))) => app.mensagem = Some(aviso),
                    Ok(_) => {}
                    Err(e) => {
                        app.mensagem = Some(format!("tentativa não registrada no diário: {e}"))
                    }
                }
            }
        },
    }
}

fn situacao_texto(s: Situacao) -> (&'static str, Color32) {
    match s {
        Situacao::Atingida => ("meta atingida", VERDE),
        Situacao::DentroDaTolerancia => ("dentro da tolerância", AMARELO),
        Situacao::NaoAtingida => ("meta não atingida", VERMELHO),
        Situacao::SemMeta => ("sem meta", Color32::GRAY),
    }
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
                        RichText::new(format!("{} cada unidade a mais", dinheiro(*custo_unitario)))
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
            ui.horizontal_wrapped(|ui| {
                for (i, o) in opcoes.iter().enumerate() {
                    let r = if o.custo > 0.0 {
                        format!("{} ({})", o.rotulo, dinheiro(o.custo))
                    } else {
                        o.rotulo.clone()
                    };
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
    let ctx = ui.ctx().clone();
    let executor = app.executor.clone();
    let Some(m) = app.missao.as_mut() else {
        app.tela = Tela::Mapa;
        return;
    };
    let mc = m.missao().clone();
    let missao = &mc.missao;
    ui.horizontal(|ui| {
        if ui.button("← Campanha").clicked() {
            app.tela = Tela::Mapa;
        }
        ui.label(RichText::new(format!("Missão {}", numero_missao(&m.pacote, m.indice))).weak());
    });
    let Some(m) = app.missao.as_mut() else { return };
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
        ui.label(RichText::new(format!("Conceitos: {}", nomes.join(", "))).weak());
    }
    ui.separator();

    if !missao.medicoes.is_empty() {
        ui.label(RichText::new("Medições do estado atual").strong());
        match &m.medicoes {
            Some(v) => {
                for l in v {
                    ui.label(l);
                }
            }
            None => {
                if ui
                    .add_enabled(m.trabalho.is_none(), egui::Button::new(texto::MEDIR))
                    .clicked()
                {
                    m.medir(aluno.clone(), executor.as_ref(), Some(ctx.clone()));
                }
            }
        }
        ui.add_space(6.0);
    }

    if !missao.previsao.is_empty() {
        ui.label(RichText::new("Previsão").strong());
        ui.label(RichText::new(texto::PREVER_ANTES).weak());
        let travada = m.previsao_travada;
        for p in &missao.previsao {
            ui.label(p.pergunta());
            ui.add_enabled_ui(!travada, |ui| match p {
                Previsao::Escolha { id, opcoes, .. } => {
                    let atual = match m.tentativa.previsoes.get(id) {
                        Some(RespostaPrevisao::Escolha(i)) => Some(*i),
                        _ => None,
                    };
                    ui.horizontal_wrapped(|ui| {
                        for (i, o) in opcoes.iter().enumerate() {
                            if ui.radio(atual == Some(i), &o.rotulo).clicked() {
                                m.tentativa
                                    .previsoes
                                    .insert(id.clone(), RespostaPrevisao::Escolha(i));
                            }
                        }
                    });
                }
                Previsao::Valor { id, unidade, .. } => {
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
        ui.label(RichText::new("Decisões").strong());
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
                    format!(
                        "Custo das decisões: {} de {} do orçamento",
                        dinheiro(custo),
                        dinheiro(o)
                    ),
                );
            }
            None => {
                ui.label(format!("Custo das decisões: {}", dinheiro(custo)));
            }
        }
        if let Some(motivo) = utsim_ensino_core::nota::portao(missao, &m.tentativa.decisoes) {
            ui.colored_label(VERMELHO, format!("Restrição: {motivo}"));
        }
        ui.add_space(6.0);
    }

    if !missao.justificativa.is_empty() {
        ui.label(RichText::new("Justificativa").strong());
        for (qi, q) in missao.justificativa.iter().enumerate() {
            ui.label(&q.pergunta);
            ui.horizontal_wrapped(|ui| {
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

    if !missao.dicas.is_empty() {
        ui.collapsing("Dicas", |ui| {
            let pesos = &m.pacote.pacote.pesos;
            for (i, d) in missao.dicas.iter().enumerate() {
                let nivel = i as u8 + 1;
                if m.tentativa.dicas >= nivel {
                    ui.label(format!("{nivel}. {d}"));
                } else {
                    let desc = pesos.dicas[i];
                    let rot = if desc > 0.0 {
                        format!("Ver dica {nivel} (desconta {desc} pontos)")
                    } else {
                        format!("Ver dica {nivel} (sem desconto)")
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
    ui.horizontal(|ui| {
        let pode = m.trabalho.is_none() && m.previsoes_completas();
        if ui
            .add_enabled(
                pode,
                egui::Button::new(RichText::new(texto::SIMULAR).strong()),
            )
            .clicked()
        {
            m.simular(aluno.clone(), executor.as_ref(), Some(ctx.clone()));
        }
        if !m.previsoes_completas() {
            ui.label(RichText::new("Responda às previsões para simular.").weak());
        }
        if let Some(o) = missao.rodadas.orcamento {
            ui.label(format!("Rodadas: {} de {o} sem desconto", m.rodadas));
        } else {
            ui.label(format!("Rodadas feitas: {}", m.rodadas));
        }
        if let Some(t) = &m.trabalho {
            ui.add(
                egui::ProgressBar::new(t.fracao())
                    .desired_width(160.0)
                    .show_percentage(),
            );
            if ui.button(texto::CANCELAR).clicked() {
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
        resultado(ui, &r, missao);
    }
    if let Some(a) = m.animacao.as_mut() {
        ui.separator();
        ui.label(RichText::new("A sua fábrica (uma replicação)").strong());
        a.mostrar(ui);
    }
    if m.resultado.as_ref().is_some_and(|r| r.resolvida) {
        let i = m.indice;
        let proxima = i + 1 < m.pacote.missoes.len() && app.progresso.liberada(&m.pacote, i + 1);
        ui.horizontal(|ui| {
            if proxima && ui.button("Próxima missão →").clicked() {
                app.abrir_missao(i + 1);
            } else if ui.button("Caixa de areia").clicked() {
                app.abrir_areia(i);
            }
        });
    }
}

fn resultado(ui: &mut Ui, r: &Resultado, missao: &utsim_ensino_core::missao::Missao) {
    let estrelas = format!(
        "{}{}",
        "★".repeat(r.estrelas as usize),
        "☆".repeat(3 - r.estrelas.min(3) as usize)
    );
    ui.horizontal(|ui| {
        ui.label(
            RichText::new(format!("Nota {}", fmt_num(r.nota, 1)))
                .size(22.0)
                .strong(),
        );
        ui.label(RichText::new(estrelas).size(22.0));
        if r.resolvida {
            ui.colored_label(VERDE, "resolvida");
        } else {
            ui.colored_label(VERMELHO, "não resolvida");
        }
        if r.selo {
            ui.colored_label(AMARELO, "selo: melhor que a referência");
        }
    });
    if let Some(p) = &r.portao {
        ui.colored_label(VERMELHO, format!("Restrição violada: {p}"));
    }
    egui::Grid::new("metricas").striped(true).show(ui, |ui| {
        ui.label(RichText::new("Indicador").strong());
        ui.label(RichText::new("Estado inicial").strong());
        ui.label(RichText::new("Sua tentativa").strong());
        ui.label(RichText::new("Meta").strong());
        ui.label(RichText::new("Situação").strong());
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
            let (t, c) = situacao_texto(m.situacao);
            ui.colored_label(c, t);
            ui.end_row();
        }
    });
    ui.label(format!(
        "Pontos: desempenho {}, previsão {}, justificativa {} (de {} possíveis) · descontos {} (dicas) e {} (rodadas)",
        fmt_num(r.pontos_desempenho, 1),
        fmt_num(r.pontos_previsao, 1),
        fmt_num(r.pontos_justificativa, 1),
        fmt_num(r.pontos_possiveis, 0),
        fmt_num(r.desconto_dicas, 0),
        fmt_num(r.desconto_rodadas, 0),
    ));
    for (p, q) in r.previsoes.iter().zip(&missao.previsao) {
        if p.certa {
            ui.colored_label(
                VERDE,
                format!("Previsão certa: {} {}", q.pergunta(), p.esperado),
            );
        } else {
            ui.colored_label(
                VERMELHO,
                format!(
                    "Previsão errada: {} A resposta da simulação é {}.",
                    q.pergunta(),
                    p.esperado
                ),
            );
        }
    }
    if !missao.justificativa.is_empty() {
        ui.label(format!(
            "Justificativa: {} de {} certas",
            r.justificativas_certas,
            missao.justificativa.len()
        ));
    }
    if !r.dicas_automaticas.is_empty() {
        ui.label(RichText::new("O que aconteceu").strong());
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
    if ui.button("← Campanha").clicked() {
        app.tela = Tela::Mapa;
        return;
    }
    ui.heading(format!("Caixa de areia: {}", missao.titulo));
    ui.label(texto::AREIA_TEXTO);
    ui.columns(2, |cols| {
        for (k, (nome, d)) in [("Cenário A", &mut s.a), ("Cenário B", &mut s.b)]
            .into_iter()
            .enumerate()
        {
            let ui = &mut cols[k];
            ui.label(RichText::new(nome).strong());
            for a in &missao.alavancas {
                let v = d.entry(a.id().to_string()).or_insert(a.inicial());
                alavanca(ui, a, v, &format!("{nome}{}", a.id()));
            }
            ui.label(format!("Custo: {}", dinheiro(missao.custo(d))));
        }
    });
    ui.horizontal(|ui| {
        if ui
            .add_enabled(s.trabalho.is_none(), egui::Button::new("Comparar A e B"))
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
            for h in ["Indicador", "A", "B", "B menos A (pareada)", ""] {
                ui.label(RichText::new(h).strong());
            }
            ui.end_row();
            for m in &missao.metricas {
                let (Ok(xa), Ok(xb)) = (
                    avaliar(missao, &m.valor, &v[0]),
                    avaliar(missao, &m.valor, &v[1]),
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
                    "diferença significativa"
                } else {
                    "empate estatístico"
                });
                ui.end_row();
            }
        });
    }
    ui.separator();
    ui.label("Levar o cenário A para o UTSim (grava o modelo .ron com as decisões aplicadas):");
    ui.horizontal(|ui| {
        ui.text_edit_singleline(&mut s.caminho_ron);
        if ui.button("Gravar .ron").clicked() {
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
                Ok(()) => format!("modelo gravado em {}", s.caminho_ron.trim()),
                Err(e) => format!("não foi possível gravar: {e}"),
            });
        }
    });
}
