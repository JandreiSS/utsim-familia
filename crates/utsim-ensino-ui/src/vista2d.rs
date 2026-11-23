//! The schematic 2D view: boxes for stations, queues as stacks of items, arrows of the
//! flow, stations coloured by state (with the state written too, never colour alone),
//! animated by the trace of a [`Replay`]: items moving, queues growing.

use std::collections::{BTreeMap, VecDeque};

use egui::{
    Align2, Color32, CornerRadius, FontId, Pos2, Rect, Sense, Stroke, StrokeKind, Ui, Vec2, pos2,
    vec2,
};
use utsim_core::{State, TraceEvent};
use utsim_model::replay::Replay;

use crate::texto;

/// Playback of one replay.
pub struct Animacao {
    pub replay: Replay,
    /// Simulated time shown.
    pub t: f64,
    /// Simulated time units per second of the screen.
    pub velocidade: f64,
    pub pausado: bool,
    pub unidade: String,
    cursor: usize,
    onde: BTreeMap<u64, usize>,
    pub estados: Vec<State>,
    pub conteudo: Vec<usize>,
    /// Recent moves (from, to, instant), drawn as items on the arrows.
    movimentos: VecDeque<(usize, usize, f64)>,
    posicoes: Vec<Pos2>,
}

/// Blocks that are drawn (the team dispatchers and mobile resources are left out).
fn desenhavel(kind: &str) -> bool {
    !matches!(kind, "Dispatcher" | "Transporter")
}

impl Animacao {
    pub fn new(replay: Replay, unidade: &str) -> Animacao {
        let n = replay.blocks.len();
        let posicoes = layout(&replay);
        let mut a = Animacao {
            replay,
            t: 0.0,
            velocidade: 5.0,
            pausado: false,
            unidade: unidade.to_string(),
            cursor: 0,
            onde: BTreeMap::new(),
            estados: vec![State::Idle; n],
            conteudo: vec![0; n],
            movimentos: VecDeque::new(),
            posicoes,
        };
        a.ir_para(0.0);
        a
    }

    /// Moves the playback to `t` (backwards replays from the start).
    pub fn ir_para(&mut self, t: f64) {
        let t = t.clamp(0.0, self.replay.stop_time);
        if t < self.t || self.cursor == 0 {
            self.cursor = 0;
            self.onde.clear();
            self.estados.iter_mut().for_each(|s| *s = State::Idle);
            self.conteudo.iter_mut().for_each(|c| *c = 0);
            self.movimentos.clear();
        }
        while let Some(r) = self.replay.records.get(self.cursor) {
            if r.t > t {
                break;
            }
            match &r.event {
                TraceEvent::Created { item, block } | TraceEvent::Unpacked { item, block } => {
                    self.onde.insert(*item, *block);
                    if let Some(c) = self.conteudo.get_mut(*block) {
                        *c += 1;
                    }
                }
                TraceEvent::Moved { item, from, to } => {
                    if let Some(c) = self.conteudo.get_mut(*from) {
                        *c = c.saturating_sub(1);
                    }
                    if let Some(c) = self.conteudo.get_mut(*to) {
                        *c += 1;
                    }
                    self.onde.insert(*item, *to);
                    self.movimentos.push_back((*from, *to, r.t));
                }
                TraceEvent::Packed { item, .. } | TraceEvent::Destroyed { item } => {
                    if let Some(b) = self.onde.remove(item)
                        && let Some(c) = self.conteudo.get_mut(b)
                    {
                        *c = c.saturating_sub(1);
                    }
                }
                TraceEvent::State { block, state } => {
                    if let Some(s) = self.estados.get_mut(*block) {
                        *s = *state;
                    }
                }
                _ => {}
            }
            self.cursor += 1;
        }
        self.t = t;
        let janela = self.duracao_movimento();
        while self.movimentos.front().is_some_and(|m| m.2 < t - janela) {
            self.movimentos.pop_front();
        }
        while self.movimentos.len() > 200 {
            self.movimentos.pop_front();
        }
    }

    fn duracao_movimento(&self) -> f64 {
        (self.velocidade * 0.5).max(1e-6)
    }

    /// Advances by `dt` seconds of screen time.
    pub fn avancar(&mut self, dt: f64) {
        if !self.pausado && self.t < self.replay.stop_time {
            self.ir_para(self.t + dt * self.velocidade);
        }
    }

    /// The controls and the drawing.
    pub fn mostrar(&mut self, ui: &mut Ui) {
        ui.horizontal(|ui| {
            if ui
                .button(if self.pausado {
                    texto::vista_continuar()
                } else {
                    texto::vista_pausar()
                })
                .clicked()
            {
                self.pausado = !self.pausado;
            }
            if ui.button(texto::vista_do_inicio()).clicked() {
                self.ir_para(0.0);
                self.pausado = false;
            }
            ui.label(texto::vista_velocidade());
            for v in [1.0, 5.0, 20.0, 60.0] {
                if ui
                    .selectable_label((self.velocidade - v).abs() < 1e-9, format!("{v}×"))
                    .clicked()
                {
                    self.velocidade = v;
                }
            }
            ui.label(texto::f(
                texto::vista_tempo(),
                &[
                    &format!("{:.1}", self.t),
                    &self.unidade,
                    &format!("{:.0}", self.replay.stop_time),
                ],
            ));
        });
        let dt = ui.input(|i| i.stable_dt).min(0.1) as f64;
        self.avancar(dt);
        if !self.pausado && self.t < self.replay.stop_time {
            ui.ctx().request_repaint();
        }
        let largura = ui.available_width().max(300.0);
        let (resp, painter) = ui.allocate_painter(vec2(largura, 300.0), Sense::hover());
        let rect = resp.rect;
        painter.rect_filled(rect, CornerRadius::same(4), ui.visuals().extreme_bg_color);
        let pos = self.na_tela(rect);
        let texto = ui.visuals().text_color();
        let fraco = ui.visuals().weak_text_color();
        // Objects named `<maquina>__...` are copies of one machine (one per flow of a
        // layout mission): they are drawn as a single box.
        let base = |i: usize| -> &str {
            let n = self.replay.blocks[i].name.as_str();
            n.split("__").next().unwrap_or(n)
        };
        let mut setas = std::collections::BTreeSet::new();
        for (a, b) in &self.replay.connections {
            if !(self.desenha(*a) && self.desenha(*b)) || base(*a) == base(*b) {
                continue;
            }
            if !setas.insert((base(*a).to_string(), base(*b).to_string())) {
                continue;
            }
            // Boxes too close for an arrow between them get none.
            let (pa, pb) = (pos[*a], pos[*b]);
            let comprimento = (pb - pa).length();
            if comprimento < 100.0 {
                continue;
            }
            let dir = (pb - pa) / comprimento;
            let (inicio, ponta) = (pa + dir * 44.0, pb - dir * 44.0);
            let traco = Stroke::new(1.5, fraco);
            painter.line_segment([inicio, ponta], traco);
            // A tip of fixed size (egui's arrow grows with the length).
            let lado = egui::vec2(-dir.y, dir.x);
            for s in [-1.0, 1.0] {
                painter.line_segment([ponta, ponta - dir * 9.0 + lado * (5.0 * s)], traco);
            }
        }
        let dur = self.duracao_movimento();
        for (a, b, t0) in &self.movimentos {
            if !(self.desenha(*a) && self.desenha(*b)) {
                continue;
            }
            let f = ((self.t - t0) / dur).clamp(0.0, 1.0) as f32;
            let p = pos[*a] + (pos[*b] - pos[*a]) * f;
            painter.circle_filled(p, 4.0, crate::ROXO);
        }
        // One box per machine: the content added up, the busiest state.
        let prioridade = |st: State| match st {
            State::Processing | State::Utilize => 5,
            State::Setup => 4,
            State::Blocked | State::WaitingTransport => 3,
            State::Down | State::WaitingRepair | State::Maintenance | State::Stopped => 2,
            State::Idle => 0,
            _ => 1,
        };
        let mut caixas: Vec<(String, usize, usize, State, String)> = Vec::new();
        for (i, b) in self.replay.blocks.iter().enumerate() {
            if !desenhavel(&b.kind) {
                continue;
            }
            let nome = base(i).to_string();
            match caixas.iter_mut().find(|c| c.0 == nome) {
                Some(c) => {
                    c.2 += self.conteudo[i];
                    if prioridade(self.estados[i]) > prioridade(c.3) {
                        c.3 = self.estados[i];
                    }
                    if b.kind == "Processor" {
                        c.4 = b.kind.clone();
                    }
                }
                None => caixas.push((nome, i, self.conteudo[i], self.estados[i], b.kind.clone())),
            }
        }
        for (nome, i, conteudo, estado, kind) in &caixas {
            let c = pos[*i];
            let r = Rect::from_center_size(c, CAIXA);
            let (cor, nome_estado) = cor_estado(kind, *estado, *conteudo);
            painter.rect_filled(r, CornerRadius::same(4), cor);
            painter.rect_stroke(
                r,
                CornerRadius::same(4),
                Stroke::new(1.0, fraco),
                StrokeKind::Inside,
            );
            // The name as the student reads it ("Fila embalagem", as in "O que aconteceu");
            // a long one shrinks to fit the box instead of spilling.
            let nome = utsim_ensino_core::dicas::nome_visivel(nome);
            let largura = painter
                .layout_no_wrap(nome.clone(), FontId::proportional(13.0), Color32::WHITE)
                .size()
                .x;
            let tamanho = if largura > r.width() - 6.0 {
                (13.0 * (r.width() - 6.0) / largura).max(9.0)
            } else {
                13.0
            };
            painter.text(
                r.center_top() + vec2(0.0, 2.0),
                Align2::CENTER_TOP,
                nome,
                FontId::proportional(tamanho),
                Color32::WHITE,
            );
            painter.text(
                r.center_bottom() - vec2(0.0, 3.0),
                Align2::CENTER_BOTTOM,
                nome_estado,
                FontId::proportional(12.0),
                Color32::WHITE,
            );
            if kind == "Queue" || kind == "Processor" || kind == "Conveyor" {
                // The stack of items under the box.
                let n = *conteudo;
                for k in 0..n.min(12) {
                    let p = pos2(
                        r.left() + 5.0 + (k % 6) as f32 * 9.0,
                        r.bottom() + 6.0 + (k / 6) as f32 * 9.0,
                    );
                    painter.rect_filled(
                        Rect::from_min_size(p, vec2(7.0, 7.0)),
                        CornerRadius::same(1),
                        crate::ROXO,
                    );
                }
                painter.text(
                    r.right_bottom() + vec2(4.0, 6.0),
                    Align2::LEFT_TOP,
                    format!("{n}"),
                    FontId::proportional(13.0),
                    texto,
                );
            }
        }
        ui.horizontal_wrapped(|ui| {
            for (cor, nome) in [
                (COR_TRABALHANDO, texto::vista_trabalhando()),
                (COR_OCIOSA, texto::vista_ociosa()),
                (COR_BLOQUEADA, texto::vista_bloqueada()),
                (COR_QUEBRADA, texto::vista_parada_ou_quebrada()),
                (COR_SETUP, texto::vista_em_setup()),
            ] {
                let (r, p) = ui.allocate_painter(vec2(12.0, 12.0), Sense::hover());
                p.rect_filled(r.rect, CornerRadius::same(2), cor);
                ui.label(nome);
            }
        });
    }

    fn desenha(&self, i: usize) -> bool {
        self.replay
            .blocks
            .get(i)
            .is_some_and(|b| desenhavel(&b.kind))
    }

    fn na_tela(&self, rect: Rect) -> Vec<Pos2> {
        let pts: Vec<Pos2> = self
            .posicoes
            .iter()
            .zip(&self.replay.blocks)
            .filter(|(_, b)| desenhavel(&b.kind))
            .map(|(p, _)| *p)
            .collect();
        if pts.is_empty() {
            return self.posicoes.clone();
        }
        let (mut min, mut max) = (pts[0], pts[0]);
        for p in &pts {
            min = min.min(*p);
            max = max.max(*p);
        }
        let area = rect.shrink2(vec2(56.0, 44.0));
        let w = (max.x - min.x).max(1e-3);
        let h = (max.y - min.y).max(1e-3);
        let s = (area.width() / w).min(if h > 1e-2 {
            area.height() / h
        } else {
            f32::INFINITY
        });
        let centro_mundo = pos2((min.x + max.x) / 2.0, (min.y + max.y) / 2.0);
        let mut tela: Vec<Pos2> = self
            .posicoes
            .iter()
            .map(|p| {
                let d = *p - centro_mundo;
                area.center() + vec2(d.x * s, -d.y * s) - vec2(0.0, 10.0)
            })
            .collect();
        self.separar(&mut tela, rect);
        tela
    }

    /// Objects closer than a box on the floor plan (a press beside its exit, a dock beside
    /// its arrivals) would hide each other: the boxes are pushed apart a little, and the
    /// copies of one machine (`<maquina>__<fluxo>`) move together.
    fn separar(&self, tela: &mut [Pos2], rect: Rect) {
        let mut grupos: Vec<(String, Vec<usize>)> = Vec::new();
        for (i, b) in self.replay.blocks.iter().enumerate() {
            if !desenhavel(&b.kind) || i >= tela.len() {
                continue;
            }
            let base = b.name.split("__").next().unwrap_or(&b.name).to_string();
            match grupos.iter_mut().find(|g| g.0 == base) {
                Some(g) => g.1.push(i),
                None => grupos.push((base, vec![i])),
            }
        }
        let mut c: Vec<Pos2> = grupos.iter().map(|g| tela[g.1[0]]).collect();
        let (lx, ly) = (CAIXA.x + 8.0, CAIXA.y + 8.0);
        for _ in 0..60 {
            let mut mexeu = false;
            for i in 0..c.len() {
                for j in i + 1..c.len() {
                    let d = c[j] - c[i];
                    let (fx, fy) = (lx - d.x.abs(), ly - d.y.abs());
                    if fx <= 0.0 || fy <= 0.0 {
                        continue;
                    }
                    mexeu = true;
                    // Along the axis that needs the smaller move (ties to the side).
                    let m = if fx * ly <= fy * lx {
                        vec2(fx / 2.0 * if d.x < 0.0 { -1.0 } else { 1.0 }, 0.0)
                    } else {
                        vec2(0.0, fy / 2.0 * if d.y < 0.0 { -1.0 } else { 1.0 })
                    };
                    c[i] -= m;
                    c[j] += m;
                }
            }
            if !mexeu {
                break;
            }
        }
        let dentro = rect.shrink2(CAIXA / 2.0 + vec2(4.0, 4.0));
        for (g, p) in grupos.iter().zip(&c) {
            let p = dentro.clamp(*p);
            let d = p - tela[g.1[0]];
            for &i in &g.1 {
                tela[i] += d;
            }
        }
    }
}

/// Size of a station box.
const CAIXA: Vec2 = vec2(84.0, 46.0);

pub const COR_TRABALHANDO: Color32 = Color32::from_rgb(46, 140, 80);
pub const COR_OCIOSA: Color32 = Color32::from_rgb(88, 92, 100);
pub const COR_BLOQUEADA: Color32 = Color32::from_rgb(196, 120, 30);
pub const COR_QUEBRADA: Color32 = Color32::from_rgb(190, 60, 55);
pub const COR_SETUP: Color32 = Color32::from_rgb(52, 110, 190);

/// Colour and word of a block state.
pub fn cor_estado(kind: &str, s: State, conteudo: usize) -> (Color32, &'static str) {
    match kind {
        "Queue" => {
            if conteudo == 0 {
                (COR_OCIOSA, texto::vista_vazia())
            } else {
                (Color32::from_rgb(70, 76, 110), texto::vista_fila())
            }
        }
        "Source" => (Color32::from_rgb(60, 90, 120), texto::vista_chegadas()),
        "Sink" => (Color32::from_rgb(60, 90, 120), texto::vista_saida()),
        _ => match s {
            State::Processing | State::Utilize => (COR_TRABALHANDO, texto::vista_trabalhando()),
            State::Setup => (COR_SETUP, texto::vista_setup()),
            State::Blocked | State::WaitingTransport => (COR_BLOQUEADA, texto::vista_bloqueada()),
            State::Down | State::WaitingRepair | State::Maintenance | State::Stopped => {
                (COR_QUEBRADA, texto::vista_parada())
            }
            State::OffShift => (Color32::from_rgb(40, 42, 48), texto::vista_fora_do_turno()),
            State::WaitingOperator => {
                (Color32::from_rgb(120, 80, 150), texto::vista_sem_operador())
            }
            _ => (COR_OCIOSA, texto::vista_ociosa()),
        },
    }
}

/// Positions from the model; when every block is at the origin, a left-to-right layout
/// by the order of the flow.
fn layout(r: &Replay) -> Vec<Pos2> {
    let pos: Vec<Pos2> = r
        .blocks
        .iter()
        .map(|b| pos2(b.position[0] as f32, b.position[1] as f32))
        .collect();
    let distintos = pos
        .iter()
        .zip(&r.blocks)
        .filter(|(_, b)| desenhavel(&b.kind))
        .any(|(p, _)| p.x.abs() > 1e-6 || p.y.abs() > 1e-6);
    if distintos {
        return pos;
    }
    // Depth of each block from the sources.
    let n = r.blocks.len();
    let mut nivel = vec![0usize; n];
    for _ in 0..n {
        for (a, b) in &r.connections {
            if nivel[*b] < nivel[*a] + 1 && nivel[*a] + 1 < n {
                nivel[*b] = nivel[*a] + 1;
            }
        }
    }
    let mut linha: BTreeMap<usize, f32> = BTreeMap::new();
    (0..n)
        .map(|i| {
            let y = linha.entry(nivel[i]).or_insert(0.0);
            let p = pos2(nivel[i] as f32 * 4.0, -*y);
            *y += 4.0;
            p
        })
        .collect()
}
