//! A tela de abertura comum dos apps da família: o cabeçalho com o ícone e o nome do app,
//! as ações grandes ("o que você quer fazer?"), os recentes e, sempre visível, onde o
//! trabalho fica salvo. Cada app diz quais ações tem; o desenho é um só.
//!
//! O componente não abre nada: devolve a [`Escolha`] da pessoa e o app faz o resto.

use std::path::{Path, PathBuf};

use egui::{
    Align2, Color32, CornerRadius, FontId, Rect, RichText, Sense, Stroke, StrokeKind,
    TextureHandle, TextureOptions, Ui, Vec2, pos2, vec2,
};

/// O desenho de linha de cada ação.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Simbolo {
    /// Uma pasta: abrir o que já existe.
    Abrir,
    /// Um "+": criar.
    Novo,
    /// Três nós ligados: um modelo do UTSim.
    Modelo,
    /// Um livro: exemplos e modelos prontos.
    Exemplo,
    /// Uma seta entrando na bandeja: importar.
    Importar,
    /// Duas pessoas: turma, equipe.
    Pessoas,
    /// Uma grade: planilha.
    Planilha,
    /// Uma seta para a direita: continuar de onde parou.
    Continuar,
}

/// Uma ação grande da abertura.
#[derive(Debug, Clone, Copy)]
pub struct Acao<'a> {
    /// O que o app recebe de volta em [`Escolha::Acao`].
    pub id: &'static str,
    pub titulo: &'a str,
    /// Uma linha, em linguagem simples, do que acontece.
    pub descricao: &'a str,
    pub simbolo: Simbolo,
    /// A ação principal ganha a cor do app.
    pub principal: bool,
    /// Sem `None`, a ação fica apagada e o texto diz por quê.
    pub bloqueio: Option<&'a str>,
}

impl<'a> Acao<'a> {
    pub fn nova(id: &'static str, simbolo: Simbolo, titulo: &'a str, descricao: &'a str) -> Self {
        Acao {
            id,
            titulo,
            descricao,
            simbolo,
            principal: false,
            bloqueio: None,
        }
    }

    pub fn principal(mut self) -> Self {
        self.principal = true;
        self
    }

    pub fn bloqueada_se(mut self, bloquear: bool, motivo: &'a str) -> Self {
        if bloquear {
            self.bloqueio = Some(motivo);
        }
        self
    }
}

/// Um item da lista de recentes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Recente {
    pub titulo: String,
    pub caminho: PathBuf,
}

impl Recente {
    /// O título é o nome da pasta ou do arquivo.
    pub fn de(caminho: impl Into<PathBuf>) -> Recente {
        let caminho = caminho.into();
        let titulo = caminho
            .file_stem()
            .or_else(|| caminho.file_name())
            .map_or_else(
                || caminho.display().to_string(),
                |n| n.to_string_lossy().into_owned(),
            );
        Recente { titulo, caminho }
    }
}

/// O quadro "onde fica salvo".
#[derive(Debug, Clone, Copy)]
pub struct Onde<'a> {
    /// O que está aberto agora (`None`: nada aberto ainda).
    pub caminho: Option<&'a Path>,
    /// O que é essa pasta, em linguagem simples.
    pub explicacao: &'a str,
}

/// O que a pessoa escolheu.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Escolha {
    Acao(&'static str),
    /// Abrir o recente de índice dado.
    Recente(usize),
    /// Tirar o recente da lista (a pasta sumiu).
    Esquecer(usize),
    /// Mostrar a pasta no explorador de arquivos.
    MostrarPasta(PathBuf),
}

/// Os textos fixos da abertura (para os apps com mais de uma língua).
#[derive(Debug, Clone, Copy)]
pub struct Rotulos<'a> {
    pub o_que_fazer: &'a str,
    pub onde_salvo: &'a str,
    pub nada_aberto: &'a str,
    pub nada_aberto_texto: &'a str,
    pub aberto_agora: &'a str,
    pub mostrar_no_explorador: &'a str,
    pub abrir: &'a str,
    pub nao_encontrada: &'a str,
    pub tirar_da_lista: &'a str,
}

impl Rotulos<'static> {
    /// Em português.
    pub const PT: Rotulos<'static> = Rotulos {
        o_que_fazer: "O que você quer fazer?",
        onde_salvo: "Onde fica salvo",
        nada_aberto: "Nada aberto ainda.",
        nada_aberto_texto: "O que você abrir ou criar aparece aqui, para voltar com um clique.",
        aberto_agora: "Aberto agora:",
        mostrar_no_explorador: "Mostrar no explorador",
        abrir: "Abrir",
        nao_encontrada: "não encontrada",
        tirar_da_lista: "Tirar da lista",
    };
}

/// A abertura de um app.
pub struct Abertura<'a> {
    /// O nome do app ("UTSim Qualidade").
    pub nome: &'a str,
    /// O subtítulo do lançador ("Engenharia da qualidade").
    pub subtitulo: &'a str,
    /// A cor do app (a mesma do lançador).
    pub cor: Color32,
    /// O ícone do app (PNG), se houver.
    pub icone: Option<&'static [u8]>,
    pub acoes: &'a [Acao<'a>],
    /// O título dos recentes ("Unidades recentes", "Projetos recentes").
    pub titulo_recentes: &'a str,
    pub recentes: &'a [Recente],
    pub onde: Onde<'a>,
    pub rotulos: Rotulos<'a>,
}

const RAIO: u8 = 8;

impl Abertura<'_> {
    /// Desenha a abertura. `topo` desenha o que é só do app logo acima das ações (por
    /// exemplo o nome do autor); `rodape`, o que vem depois de "onde fica salvo".
    pub fn mostrar(
        &self,
        ui: &mut Ui,
        topo: impl FnOnce(&mut Ui),
        rodape: impl FnOnce(&mut Ui),
    ) -> Option<Escolha> {
        let mut escolha = None;
        egui::ScrollArea::vertical()
            .auto_shrink([false, false])
            .show(ui, |ui| {
                let largura = ui.available_width().min(1120.0);
                let margem = ((ui.available_width() - largura) / 2.0).max(0.0);
                ui.horizontal(|ui| {
                    ui.add_space(margem);
                    ui.vertical(|ui| {
                        ui.set_width(largura);
                        ui.add_space(28.0);
                        self.cabecalho(ui);
                        ui.add_space(22.0);
                        topo(ui);
                        let duas = largura >= 860.0;
                        if duas {
                            let direita = (largura * 0.36).clamp(300.0, 420.0);
                            let esquerda = largura - direita - 28.0;
                            ui.horizontal_top(|ui| {
                                ui.vertical(|ui| {
                                    ui.set_width(esquerda);
                                    if let Some(e) = self.acoes(ui, esquerda) {
                                        escolha = Some(e);
                                    }
                                });
                                ui.add_space(28.0);
                                ui.vertical(|ui| {
                                    ui.set_width(direita);
                                    if let Some(e) = self.recentes(ui) {
                                        escolha = Some(e);
                                    }
                                });
                            });
                        } else {
                            if let Some(e) = self.acoes(ui, largura) {
                                escolha = Some(e);
                            }
                            ui.add_space(20.0);
                            if let Some(e) = self.recentes(ui) {
                                escolha = Some(e);
                            }
                        }
                        ui.add_space(24.0);
                        if let Some(e) = self.onde(ui) {
                            escolha = Some(e);
                        }
                        rodape(ui);
                        ui.add_space(24.0);
                    });
                });
            });
        escolha
    }

    fn cabecalho(&self, ui: &mut Ui) {
        cabecalho(ui, self.nome, self.subtitulo, self.cor, self.icone);
    }

    fn acoes(&self, ui: &mut Ui, largura: f32) -> Option<Escolha> {
        titulo_secao(ui, self.rotulos.o_que_fazer);
        let colunas = if largura >= 560.0 { 2 } else { 1 };
        let espaco = 14.0;
        let w = (largura - espaco * (colunas as f32 - 1.0)) / colunas as f32;
        let mut escolha = None;
        for linha in self.acoes.chunks(colunas) {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = espaco;
                for a in linha {
                    if cartao(ui, a, self.cor, w) {
                        escolha = Some(Escolha::Acao(a.id));
                    }
                }
            });
            ui.add_space(espaco - ui.spacing().item_spacing.y);
        }
        escolha
    }

    fn recentes(&self, ui: &mut Ui) -> Option<Escolha> {
        titulo_secao(ui, self.titulo_recentes);
        let mut escolha = None;
        quadro(ui).show(ui, |ui| {
            ui.set_width(ui.available_width());
            if self.recentes.is_empty() {
                ui.add_space(6.0);
                ui.label(RichText::new(self.rotulos.nada_aberto).color(fraco(ui)));
                ui.label(
                    RichText::new(self.rotulos.nada_aberto_texto)
                        .size(13.0)
                        .color(fraco(ui)),
                );
                ui.add_space(6.0);
                return;
            }
            for (i, r) in self.recentes.iter().enumerate().take(8) {
                if i > 0 {
                    ui.separator();
                }
                let existe = r.caminho.exists();
                ui.horizontal(|ui| {
                    let w = ui.available_width();
                    ui.vertical(|ui| {
                        ui.set_width(w - 34.0);
                        let titulo = RichText::new(&r.titulo).size(15.0).strong();
                        let resp = if existe {
                            ui.add(egui::Label::new(titulo.color(forte(ui))).sense(Sense::click()))
                                .on_hover_cursor(egui::CursorIcon::PointingHand)
                                .on_hover_text(self.rotulos.abrir)
                        } else {
                            ui.label(titulo.color(fraco(ui)))
                        };
                        if resp.clicked() {
                            escolha = Some(Escolha::Recente(i));
                        }
                        let caminho = r.caminho.display().to_string();
                        let sub = if existe {
                            caminho
                        } else {
                            format!("{}: {caminho}", self.rotulos.nao_encontrada)
                        };
                        ui.add(
                            egui::Label::new(RichText::new(sub).size(12.0).color(fraco(ui)))
                                .truncate(),
                        );
                    });
                    if existe {
                        if ui
                            .small_button("↗")
                            .on_hover_text(self.rotulos.mostrar_no_explorador)
                            .clicked()
                        {
                            escolha = Some(Escolha::MostrarPasta(r.caminho.clone()));
                        }
                    } else if ui
                        .small_button("✕")
                        .on_hover_text(self.rotulos.tirar_da_lista)
                        .clicked()
                    {
                        escolha = Some(Escolha::Esquecer(i));
                    }
                });
            }
        });
        escolha
    }

    fn onde(&self, ui: &mut Ui) -> Option<Escolha> {
        titulo_secao(ui, self.rotulos.onde_salvo);
        let mut escolha = None;
        quadro(ui).show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal(|ui| {
                let (r, _) = ui.allocate_exact_size(Vec2::splat(40.0), Sense::hover());
                ui.painter().rect_filled(
                    r,
                    CornerRadius::same(RAIO),
                    self.cor.gamma_multiply(0.22),
                );
                pintar_simbolo(ui.painter(), r.shrink(10.0), Simbolo::Abrir, self.cor);
                ui.add_space(6.0);
                ui.vertical(|ui| {
                    match self.onde.caminho {
                        Some(c) => {
                            ui.horizontal_wrapped(|ui| {
                                ui.label(RichText::new(self.rotulos.aberto_agora).color(fraco(ui)));
                                ui.label(
                                    RichText::new(c.display().to_string())
                                        .monospace()
                                        .color(forte(ui)),
                                );
                                if ui.button(self.rotulos.mostrar_no_explorador).clicked() {
                                    escolha = Some(Escolha::MostrarPasta(c.to_path_buf()));
                                }
                            });
                        }
                        None => {
                            ui.label(
                                RichText::new(self.rotulos.nada_aberto)
                                    .strong()
                                    .color(forte(ui)),
                            );
                        }
                    }
                    ui.label(RichText::new(self.onde.explicacao).color(fraco(ui)));
                });
            });
        });
        escolha
    }
}

/// O cabeçalho da abertura (ícone, nome, subtítulo e a linha na cor do app), para as
/// telas que vêm antes dela (a identidade, por exemplo).
pub fn cabecalho(
    ui: &mut Ui,
    nome: &str,
    subtitulo: &str,
    cor: Color32,
    icone: Option<&'static [u8]>,
) {
    ui.horizontal(|ui| {
        let lado = 60.0;
        let (r, _) = ui.allocate_exact_size(Vec2::splat(lado), Sense::hover());
        match icone.and_then(|b| textura(ui, nome, b)) {
            Some(t) => {
                ui.painter().image(
                    t.id(),
                    r,
                    Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)),
                    Color32::WHITE,
                );
            }
            None => {
                ui.painter().rect_filled(r, CornerRadius::same(14), cor);
                ui.painter().text(
                    r.center(),
                    Align2::CENTER_CENTER,
                    iniciais(nome),
                    FontId::proportional(24.0),
                    Color32::WHITE,
                );
            }
        }
        ui.add_space(10.0);
        ui.vertical(|ui| {
            ui.add_space(4.0);
            ui.label(RichText::new(nome).size(28.0).strong().color(forte(ui)));
            ui.label(RichText::new(subtitulo).size(15.0).color(fraco(ui)));
        });
    });
    ui.add_space(14.0);
    let (r, _) = ui.allocate_exact_size(vec2(ui.available_width(), 3.0), Sense::hover());
    let p = ui.painter();
    p.rect_filled(r, 0.0, ui.visuals().widgets.noninteractive.bg_stroke.color);
    p.rect_filled(
        Rect::from_min_size(r.min, vec2(120.0_f32.min(r.width()), r.height())),
        0.0,
        cor,
    );
}

/// Uma coluna central de até `largura` pontos (as telas da abertura).
pub fn coluna(ui: &mut Ui, largura: f32, conteudo: impl FnOnce(&mut Ui)) {
    let w = ui.available_width().min(largura);
    let margem = ((ui.available_width() - w) / 2.0).max(0.0);
    ui.horizontal(|ui| {
        ui.add_space(margem);
        ui.vertical(|ui| {
            ui.set_width(w);
            conteudo(ui);
        });
    });
}

fn forte(ui: &Ui) -> Color32 {
    ui.visuals().strong_text_color()
}

fn fraco(ui: &Ui) -> Color32 {
    ui.visuals().weak_text_color()
}

/// Um quadro com a borda na cor do app, para o que falta antes de começar (o nome do
/// autor, por exemplo).
pub fn quadro_destaque(cor: Color32) -> egui::Frame {
    egui::Frame::new()
        .fill(cor.gamma_multiply(0.12))
        .stroke(Stroke::new(1.0, cor))
        .corner_radius(CornerRadius::same(RAIO))
        .inner_margin(egui::Margin::same(14))
}

fn titulo_secao(ui: &mut Ui, texto: &str) {
    ui.label(
        RichText::new(texto.to_uppercase())
            .size(12.0)
            .strong()
            .color(fraco(ui)),
    );
    ui.add_space(6.0);
}

fn quadro(ui: &Ui) -> egui::Frame {
    let v = ui.visuals();
    egui::Frame::new()
        .fill(v.faint_bg_color)
        .stroke(v.widgets.noninteractive.bg_stroke)
        .corner_radius(CornerRadius::same(RAIO))
        .inner_margin(egui::Margin::same(14))
}

/// Um cartão de ação. Devolve `true` no clique.
fn cartao(ui: &mut Ui, a: &Acao<'_>, cor: Color32, largura: f32) -> bool {
    let ativa = a.bloqueio.is_none();
    let altura = 92.0;
    let sense = if ativa {
        Sense::click()
    } else {
        Sense::hover()
    };
    let (r, resp) = ui.allocate_exact_size(vec2(largura, altura), sense);
    let resp = match a.bloqueio {
        Some(m) => resp.on_hover_text(m),
        None => resp.on_hover_cursor(egui::CursorIcon::PointingHand),
    };
    if ui.is_rect_visible(r) {
        let v = ui.visuals();
        let sobre = ativa && resp.hovered();
        let fundo = if sobre {
            v.widgets.hovered.weak_bg_fill
        } else {
            v.faint_bg_color
        };
        let borda = if a.principal && ativa {
            Stroke::new(1.5, cor)
        } else if sobre {
            Stroke::new(1.0, cor.gamma_multiply(0.8))
        } else {
            v.widgets.noninteractive.bg_stroke
        };
        let p = ui.painter();
        p.rect(
            r,
            CornerRadius::same(RAIO),
            fundo,
            borda,
            StrokeKind::Inside,
        );
        let icone =
            Rect::from_min_size(r.min + vec2(16.0, (altura - 48.0) / 2.0), Vec2::splat(48.0));
        let (fundo_icone, tinta) = if !ativa {
            (
                v.widgets.noninteractive.bg_stroke.color,
                v.weak_text_color(),
            )
        } else if a.principal {
            (cor, Color32::WHITE)
        } else {
            (cor.gamma_multiply(0.22), cor)
        };
        p.rect_filled(icone, CornerRadius::same(RAIO), fundo_icone);
        pintar_simbolo(p, icone.shrink(12.0), a.simbolo, tinta);
        let x = icone.right() + 14.0;
        let w = (r.right() - 14.0 - x).max(40.0);
        let cor_titulo = if ativa {
            v.strong_text_color()
        } else {
            v.weak_text_color()
        };
        let mut job = egui::text::LayoutJob::simple_singleline(
            a.titulo.to_string(),
            FontId::proportional(17.0),
            cor_titulo,
        );
        job.wrap = egui::text::TextWrapping::truncate_at_width(w);
        let titulo = p.layout_job(job);
        let texto = a.descricao;
        let desc = p.layout(
            texto.to_string(),
            FontId::proportional(13.0),
            v.weak_text_color(),
            w,
        );
        let total = titulo.size().y + 4.0 + desc.size().y.min(36.0);
        let y = r.center().y - total / 2.0;
        let h_titulo = titulo.size().y;
        p.galley(pos2(x, y), titulo, cor_titulo);
        let clip = Rect::from_min_max(
            pos2(x, y + h_titulo + 4.0),
            pos2(r.right() - 10.0, r.bottom() - 6.0),
        );
        p.with_clip_rect(clip)
            .galley(pos2(x, y + h_titulo + 4.0), desc, v.weak_text_color());
    }
    ativa && resp.clicked()
}

/// Desenha `s` com linhas dentro de `r`.
pub fn pintar_simbolo(p: &egui::Painter, r: Rect, s: Simbolo, cor: Color32) {
    let st = Stroke::new(2.0, cor);
    let (x0, y0, w, h) = (r.left(), r.top(), r.width(), r.height());
    let at = |fx: f32, fy: f32| pos2(x0 + fx * w, y0 + fy * h);
    match s {
        Simbolo::Abrir => {
            let pts = vec![
                at(0.0, 0.18),
                at(0.36, 0.18),
                at(0.46, 0.32),
                at(1.0, 0.32),
                at(1.0, 0.9),
                at(0.0, 0.9),
                at(0.0, 0.18),
            ];
            p.add(egui::Shape::line(pts, st));
            p.line_segment([at(0.0, 0.45), at(1.0, 0.45)], st);
        }
        Simbolo::Novo => {
            p.line_segment([at(0.5, 0.08), at(0.5, 0.92)], Stroke::new(2.6, cor));
            p.line_segment([at(0.08, 0.5), at(0.92, 0.5)], Stroke::new(2.6, cor));
        }
        Simbolo::Modelo => {
            let (a, b, c) = (at(0.18, 0.25), at(0.82, 0.25), at(0.5, 0.8));
            p.line_segment([a, b], st);
            p.line_segment([a, c], st);
            p.line_segment([b, c], st);
            for q in [a, b, c] {
                p.circle_filled(q, w * 0.16, cor);
            }
        }
        Simbolo::Exemplo => {
            p.add(egui::Shape::line(
                vec![at(0.5, 0.22), at(0.05, 0.1), at(0.05, 0.85), at(0.5, 0.97)],
                st,
            ));
            p.add(egui::Shape::line(
                vec![at(0.5, 0.22), at(0.95, 0.1), at(0.95, 0.85), at(0.5, 0.97)],
                st,
            ));
            p.line_segment([at(0.5, 0.22), at(0.5, 0.97)], st);
        }
        Simbolo::Importar => {
            p.line_segment([at(0.5, 0.0), at(0.5, 0.62)], st);
            p.add(egui::Shape::line(
                vec![at(0.25, 0.38), at(0.5, 0.62), at(0.75, 0.38)],
                st,
            ));
            p.add(egui::Shape::line(
                vec![at(0.05, 0.6), at(0.05, 0.95), at(0.95, 0.95), at(0.95, 0.6)],
                st,
            ));
        }
        Simbolo::Pessoas => {
            p.circle_stroke(at(0.35, 0.28), w * 0.17, st);
            p.circle_stroke(at(0.72, 0.34), w * 0.13, st);
            p.add(egui::Shape::line(
                vec![
                    at(0.02, 0.95),
                    at(0.1, 0.62),
                    at(0.35, 0.55),
                    at(0.6, 0.62),
                    at(0.68, 0.95),
                ],
                st,
            ));
            p.add(egui::Shape::line(
                vec![at(0.68, 0.6), at(0.88, 0.64), at(0.98, 0.92)],
                st,
            ));
        }
        Simbolo::Planilha => {
            p.rect_stroke(r, CornerRadius::same(2), st, StrokeKind::Middle);
            for f in [0.33, 0.66] {
                p.line_segment([at(0.0, f), at(1.0, f)], st);
            }
            p.line_segment([at(0.38, 0.0), at(0.38, 1.0)], st);
        }
        Simbolo::Continuar => {
            p.line_segment([at(0.05, 0.5), at(0.92, 0.5)], Stroke::new(2.4, cor));
            p.add(egui::Shape::line(
                vec![at(0.55, 0.15), at(0.92, 0.5), at(0.55, 0.85)],
                Stroke::new(2.4, cor),
            ));
        }
    }
}

fn iniciais(nome: &str) -> String {
    nome.split_whitespace()
        .filter(|p| p.chars().next().is_some_and(char::is_uppercase))
        .filter_map(|p| p.chars().next())
        .take(2)
        .collect()
}

/// O ícone do app como textura, carregada uma vez por contexto.
fn textura(ui: &Ui, nome: &str, png: &[u8]) -> Option<TextureHandle> {
    let id = egui::Id::new(("abertura-icone", nome));
    if let Some(t) = ui.ctx().data(|d| d.get_temp::<TextureHandle>(id)) {
        return Some(t);
    }
    let img = crate::familia::imagem_png(png)?;
    let t = ui
        .ctx()
        .load_texture(format!("icone-{nome}"), img, TextureOptions::LINEAR);
    ui.ctx().data_mut(|d| d.insert_temp(id, t.clone()));
    Some(t)
}

/// A lista de recentes guardada na configuração de cada app: o mais novo primeiro, sem
/// repetir, no máximo `MAX_RECENTES`.
pub const MAX_RECENTES: usize = 8;

/// Põe `caminho` no topo de `lista`.
pub fn lembrar(lista: &mut Vec<String>, caminho: &Path) {
    let c = caminho.display().to_string();
    lista.retain(|x| x != &c);
    lista.insert(0, c);
    lista.truncate(MAX_RECENTES);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recentes_sem_repetir() {
        let mut l = vec![];
        lembrar(&mut l, Path::new("a"));
        lembrar(&mut l, Path::new("b"));
        lembrar(&mut l, Path::new("a"));
        assert_eq!(l, vec!["a".to_string(), "b".to_string()]);
        for i in 0..20 {
            lembrar(&mut l, Path::new(&format!("x{i}")));
        }
        assert_eq!(l.len(), MAX_RECENTES);
        assert_eq!(Recente::de("C:/u/Fábrica Norte").titulo, "Fábrica Norte");
        assert_eq!(iniciais("UTSim Qualidade"), "UQ");
    }
}
