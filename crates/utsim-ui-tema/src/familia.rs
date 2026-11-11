//! The flat look of the family's own tools (the launcher and the admin app): near-black
//! background, 1-pixel grey hairlines, square corners, no shadows; a sans for names and a
//! monospace in small capitals for labels, versions and buttons.
//!
//! Fonts are read from the system (never shipped): Segoe UI and Consolas on Windows, DejaVu
//! or Noto elsewhere, with egui's built-in fonts as the fallback of every family.

use egui::text::{LayoutJob, TextFormat};
use egui::{
    Align2, Color32, ColorImage, Context, CornerRadius, FontData, FontDefinitions, FontFamily,
    FontId, Margin, Painter, Pos2, Rect, Response, Sense, Shadow, Stroke, Style, TextStyle, Ui,
    Vec2, Visuals, pos2, vec2,
};

/// The palette.
pub mod cores {
    use egui::Color32;

    /// Window background.
    pub const FUNDO: Color32 = Color32::from_rgb(17, 18, 20);
    /// Slightly lighter surfaces (fields, menus, a hovered row).
    pub const SUPERFICIE: Color32 = Color32::from_rgb(24, 25, 28);
    /// A hovered surface.
    pub const SOBRE: Color32 = Color32::from_rgb(32, 34, 38);
    /// The 1-pixel lines between cards and around controls.
    pub const LINHA: Color32 = Color32::from_rgb(43, 45, 50);
    /// A line under the mouse or of a focused field.
    pub const LINHA_FORTE: Color32 = Color32::from_rgb(92, 96, 104);
    /// Body text.
    pub const TEXTO: Color32 = Color32::from_rgb(232, 233, 235);
    /// Secondary text (subtitles, dates).
    pub const TEXTO_FRACO: Color32 = Color32::from_rgb(150, 154, 161);
    /// Labels of columns and the footer.
    pub const TEXTO_APAGADO: Color32 = Color32::from_rgb(118, 122, 130);
    pub const BRANCO: Color32 = Color32::WHITE;
    pub const PRETO: Color32 = Color32::from_rgb(10, 10, 11);
    /// "Update available" and other things that ask for attention.
    pub const AMARELO: Color32 = Color32::from_rgb(236, 196, 76);
    /// Errors.
    pub const ERRO: Color32 = Color32::from_rgb(255, 112, 104);
    /// Things that are fine ("aberto para todos", "link ok").
    pub const VERDE: Color32 = Color32::from_rgb(110, 214, 120);
    /// Accent of the family when a product has none.
    pub const DESTAQUE: Color32 = Color32::from_rgb(34, 181, 255);
}

/// The palette as values, so the family's tools can switch between dark and light.
/// `branco` is the strongest ink (white on dark, near-black on light) and `preto` the ink
/// on a `branco` fill.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Paleta {
    pub claro: bool,
    pub fundo: Color32,
    pub superficie: Color32,
    pub sobre: Color32,
    pub linha: Color32,
    pub linha_forte: Color32,
    pub texto: Color32,
    pub texto_fraco: Color32,
    pub texto_apagado: Color32,
    pub branco: Color32,
    pub preto: Color32,
    pub amarelo: Color32,
    pub erro: Color32,
    pub verde: Color32,
    pub destaque: Color32,
}

impl Paleta {
    /// The dark palette (the [`cores`] constants).
    pub const ESCURA: Paleta = Paleta {
        claro: false,
        fundo: cores::FUNDO,
        superficie: cores::SUPERFICIE,
        sobre: cores::SOBRE,
        linha: cores::LINHA,
        linha_forte: cores::LINHA_FORTE,
        texto: cores::TEXTO,
        texto_fraco: cores::TEXTO_FRACO,
        texto_apagado: cores::TEXTO_APAGADO,
        branco: cores::BRANCO,
        preto: cores::PRETO,
        amarelo: cores::AMARELO,
        erro: cores::ERRO,
        verde: cores::VERDE,
        destaque: cores::DESTAQUE,
    };
    /// The light palette: the same roles on a near-white background, the signal colours
    /// darkened to stay readable on it.
    pub const CLARA: Paleta = Paleta {
        claro: true,
        fundo: Color32::from_rgb(245, 246, 248),
        superficie: Color32::from_rgb(255, 255, 255),
        sobre: Color32::from_rgb(232, 234, 238),
        linha: Color32::from_rgb(214, 217, 223),
        linha_forte: Color32::from_rgb(140, 146, 156),
        texto: Color32::from_rgb(30, 32, 36),
        texto_fraco: Color32::from_rgb(84, 90, 100),
        texto_apagado: Color32::from_rgb(106, 112, 122),
        branco: Color32::from_rgb(14, 15, 17),
        preto: Color32::WHITE,
        amarelo: Color32::from_rgb(158, 108, 0),
        erro: Color32::from_rgb(196, 38, 30),
        verde: Color32::from_rgb(28, 132, 48),
        destaque: Color32::from_rgb(0, 112, 192),
    };

    /// `c` (a product's accent) made readable as text or a line on this palette.
    pub fn legivel(&self, c: Color32) -> Color32 {
        if !self.claro {
            return c;
        }
        // Darken until the luminance is at most about 0.42 (contrast ≥ 4.5 on white).
        let lum = |c: Color32| {
            (0.2126 * c.r() as f32 + 0.7152 * c.g() as f32 + 0.0722 * c.b() as f32) / 255.0
        };
        let l = lum(c);
        if l <= 0.42 {
            return c;
        }
        let k = 0.42 / l;
        Color32::from_rgb(
            (c.r() as f32 * k) as u8,
            (c.g() as f32 * k) as u8,
            (c.b() as f32 * k) as u8,
        )
    }
}

thread_local! {
    static CLARO: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

/// The palette in use on this thread (the UI thread): dark unless [`usar_claro`] said light.
pub fn paleta() -> Paleta {
    if tema_claro() {
        Paleta::CLARA
    } else {
        Paleta::ESCURA
    }
}

/// Whether the light palette is in use on this thread.
pub fn tema_claro() -> bool {
    CLARO.with(|c| c.get())
}

/// Switches this thread to the light (`true`) or dark palette and restyles `ctx`.
pub fn usar_claro(ctx: &Context, claro: bool) {
    CLARO.with(|c| c.set(claro));
    install_style(ctx);
}

/// Font family of the bold sans (names, the wordmark).
pub const NEGRITO: &str = "familia-negrito";
/// Font family of the bold monospace (buttons).
pub const MONO_NEGRITO: &str = "familia-mono-negrito";

pub fn sans(size: f32) -> FontId {
    FontId::proportional(size)
}

pub fn negrito(size: f32) -> FontId {
    FontId::new(size, FontFamily::Name(NEGRITO.into()))
}

pub fn mono(size: f32) -> FontId {
    FontId::monospace(size)
}

pub fn mono_negrito(size: f32) -> FontId {
    FontId::new(size, FontFamily::Name(MONO_NEGRITO.into()))
}

/// System fonts tried for each role, first found wins.
const SANS: &[&str] = &[
    "C:/Windows/Fonts/segoeui.ttf",
    "/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf",
    "/usr/share/fonts/dejavu/DejaVuSans.ttf",
    "/usr/share/fonts/truetype/noto/NotoSans-Regular.ttf",
    "/System/Library/Fonts/Helvetica.ttc",
];
const SANS_BOLD: &[&str] = &[
    "C:/Windows/Fonts/segoeuib.ttf",
    "/usr/share/fonts/truetype/dejavu/DejaVuSans-Bold.ttf",
    "/usr/share/fonts/dejavu/DejaVuSans-Bold.ttf",
    "/usr/share/fonts/truetype/noto/NotoSans-Bold.ttf",
];
const MONO: &[&str] = &[
    "C:/Windows/Fonts/consola.ttf",
    "/usr/share/fonts/truetype/dejavu/DejaVuSansMono.ttf",
    "/usr/share/fonts/dejavu/DejaVuSansMono.ttf",
];
const MONO_BOLD: &[&str] = &[
    "C:/Windows/Fonts/consolab.ttf",
    "/usr/share/fonts/truetype/dejavu/DejaVuSansMono-Bold.ttf",
    "/usr/share/fonts/dejavu/DejaVuSansMono-Bold.ttf",
];

fn first(paths: &[&str]) -> Option<Vec<u8>> {
    paths.iter().find_map(|p| std::fs::read(p).ok())
}

/// The four families (sans, bold sans, mono, bold mono) from the system, each falling back to
/// egui's built-in fonts.
pub fn fonts() -> FontDefinitions {
    fonts_from(first(SANS), first(SANS_BOLD), first(MONO), first(MONO_BOLD))
}

/// [`fonts`] with the font files given (any may be missing).
pub fn fonts_from(
    sans: Option<Vec<u8>>,
    sans_bold: Option<Vec<u8>>,
    mono: Option<Vec<u8>>,
    mono_bold: Option<Vec<u8>>,
) -> FontDefinitions {
    let mut defs = FontDefinitions::default();
    let prop = defs.families[&FontFamily::Proportional].clone();
    let monos = defs.families[&FontFamily::Monospace].clone();
    let mut put = |name: &str, bytes: Option<Vec<u8>>| -> Option<String> {
        let bytes = bytes?;
        defs.font_data
            .insert(name.to_owned(), FontData::from_owned(bytes).into());
        Some(name.to_owned())
    };
    let s = put("familia-sans", sans);
    let sb = put("familia-sans-b", sans_bold);
    let m = put("familia-mono", mono);
    let mb = put("familia-mono-b", mono_bold);
    let chain = |front: Vec<Option<String>>, rest: &[String]| -> Vec<String> {
        front
            .into_iter()
            .flatten()
            .chain(rest.iter().cloned())
            .collect()
    };
    defs.families
        .insert(FontFamily::Proportional, chain(vec![s.clone()], &prop));
    defs.families
        .insert(FontFamily::Monospace, chain(vec![m.clone()], &monos));
    defs.families
        .insert(FontFamily::Name(NEGRITO.into()), chain(vec![sb, s], &prop));
    defs.families.insert(
        FontFamily::Name(MONO_NEGRITO.into()),
        chain(vec![mb, m], &monos),
    );
    defs
}

/// The egui style: flat, square, no shadows.
pub fn style() -> Style {
    let mut style = Style {
        visuals: visuals(),
        ..Style::default()
    };
    style.text_styles = [
        (TextStyle::Small, sans(12.0)),
        (TextStyle::Body, sans(14.0)),
        (TextStyle::Button, sans(14.0)),
        (TextStyle::Heading, negrito(20.0)),
        (TextStyle::Monospace, mono(13.0)),
    ]
    .into();
    let s = &mut style.spacing;
    s.item_spacing = vec2(8.0, 6.0);
    s.button_padding = vec2(10.0, 5.0);
    s.interact_size = vec2(36.0, 30.0);
    s.menu_margin = Margin::same(4);
    s.window_margin = Margin::same(12);
    s.combo_height = 320.0;
    s.menu_width = 240.0;
    style.interaction.tooltip_delay = 0.35;
    style.animation_time = 0.08;
    style.scroll_animation = egui::style::ScrollAnimation::none();
    style
}

pub fn visuals() -> Visuals {
    visuals_de(&paleta())
}

/// The egui visuals painted with `p`.
pub fn visuals_de(p: &Paleta) -> Visuals {
    let mut v = if p.claro {
        Visuals::light()
    } else {
        Visuals::dark()
    };
    let square = CornerRadius::ZERO;
    v.panel_fill = p.fundo;
    v.window_fill = p.superficie;
    v.window_stroke = Stroke::new(1.0, p.linha_forte);
    v.window_corner_radius = square;
    v.menu_corner_radius = square;
    v.window_shadow = Shadow::NONE;
    v.popup_shadow = Shadow::NONE;
    v.extreme_bg_color = p.fundo;
    v.text_edit_bg_color = Some(p.fundo);
    v.faint_bg_color = p.superficie;
    v.code_bg_color = p.superficie;
    v.hyperlink_color = p.destaque;
    v.warn_fg_color = p.amarelo;
    v.error_fg_color = p.erro;
    v.selection.bg_fill = if p.claro {
        Color32::from_rgb(196, 218, 245)
    } else {
        Color32::from_rgb(52, 74, 104)
    };
    v.selection.stroke = Stroke::new(1.0, p.branco);
    v.text_cursor.stroke = Stroke::new(2.0, p.branco);
    v.striped = false;
    v.collapsing_header_frame = false;
    let w = &mut v.widgets;
    for (state, bg, line, text) in [
        (&mut w.noninteractive, p.fundo, p.linha, p.texto),
        (&mut w.inactive, p.fundo, p.linha, p.texto),
        (&mut w.hovered, p.sobre, p.linha_forte, p.branco),
        (&mut w.active, p.sobre, p.branco, p.branco),
        (&mut w.open, p.superficie, p.linha_forte, p.branco),
    ] {
        state.bg_fill = bg;
        state.weak_bg_fill = bg;
        state.bg_stroke = Stroke::new(1.0, line);
        state.fg_stroke = Stroke::new(1.0, text);
        state.corner_radius = square;
        state.expansion = 0.0;
    }
    v
}

/// Applies the fonts and the style to `ctx`.
pub fn install(ctx: &Context) {
    ctx.set_fonts(fonts());
    install_style(ctx);
}

pub fn install_style(ctx: &Context) {
    ctx.set_theme(if tema_claro() {
        egui::ThemePreference::Light
    } else {
        egui::ThemePreference::Dark
    });
    let s = style();
    ctx.set_style_of(egui::Theme::Dark, s.clone());
    ctx.set_style_of(egui::Theme::Light, s);
}

/// `#RRGGBB` to a colour.
pub fn cor_hex(text: &str) -> Option<Color32> {
    let h = text.trim().trim_start_matches('#');
    if h.len() != 6 {
        return None;
    }
    let v = u32::from_str_radix(h, 16).ok()?;
    Some(Color32::from_rgb((v >> 16) as u8, (v >> 8) as u8, v as u8))
}

/// A line of text with wide letter spacing (labels in small capitals, the wordmark).
pub fn espacado(text: &str, font: FontId, color: Color32, spacing: f32) -> LayoutJob {
    let mut job = LayoutJob::default();
    job.append(
        text,
        0.0,
        TextFormat {
            font_id: font,
            color,
            extra_letter_spacing: spacing,
            ..Default::default()
        },
    );
    job
}

/// A monospace label in capitals with wide spacing ("ORDENAR", "VERSÃO").
pub fn rotulo(ui: &mut Ui, text: &str, size: f32, color: Color32) -> Response {
    ui.label(espacado(
        &text.to_uppercase(),
        mono(size),
        color,
        size * 0.18,
    ))
}

/// Paints `job` with its left edge at `left`, centred on `y`; returns the rect used.
pub fn pintar_texto(painter: &Painter, left: Pos2, job: LayoutJob) -> Rect {
    let galley = painter.layout_job(job);
    let pos = pos2(left.x, left.y - galley.size().y / 2.0);
    let rect = Rect::from_min_size(pos, galley.size());
    painter.galley(pos, galley, Color32::WHITE);
    rect
}

/// How a button looks.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Botao {
    /// White fill, black bold text: an action that changes something.
    Primario,
    /// Hairline border, white text.
    Contorno,
    /// No border until hovered.
    Fantasma,
    /// Outlined in red: a destructive action.
    Perigo,
}

/// A flat button of exactly `size`, its text in bold monospace capitals.
pub fn botao(ui: &mut Ui, text: &str, kind: Botao, size: Vec2, enabled: bool) -> Response {
    let sense = if enabled {
        Sense::click()
    } else {
        Sense::hover()
    };
    let (rect, resp) = ui.allocate_exact_size(size, sense);
    pintar_botao(ui, rect, &resp, text, kind, enabled);
    resp
}

/// [`botao`] placed at `rect`.
pub fn botao_em(ui: &mut Ui, rect: Rect, text: &str, kind: Botao, enabled: bool) -> Response {
    let sense = if enabled {
        Sense::click()
    } else {
        Sense::hover()
    };
    let resp = ui.allocate_rect(rect, sense);
    pintar_botao(ui, rect, &resp, text, kind, enabled);
    resp
}

fn pintar_botao(ui: &Ui, rect: Rect, resp: &Response, text: &str, kind: Botao, enabled: bool) {
    if ui.is_rect_visible(rect) {
        let hover = enabled && resp.hovered();
        let (fill, line, ink) = match kind {
            Botao::Primario => (
                if hover {
                    paleta().branco.gamma_multiply(0.85)
                } else {
                    paleta().branco
                },
                Color32::TRANSPARENT,
                paleta().preto,
            ),
            Botao::Contorno => (
                if hover {
                    paleta().sobre
                } else {
                    Color32::TRANSPARENT
                },
                if hover {
                    paleta().linha_forte
                } else {
                    paleta().linha
                },
                paleta().branco,
            ),
            Botao::Fantasma => (
                if hover {
                    paleta().sobre
                } else {
                    Color32::TRANSPARENT
                },
                Color32::TRANSPARENT,
                paleta().texto,
            ),
            Botao::Perigo => (
                if hover {
                    paleta().erro.gamma_multiply(0.15)
                } else {
                    Color32::TRANSPARENT
                },
                paleta().erro,
                paleta().erro,
            ),
        };
        let (fill, line, ink) = if enabled {
            (fill, line, ink)
        } else {
            (Color32::TRANSPARENT, paleta().linha, paleta().texto_apagado)
        };
        let p = ui.painter();
        p.rect_filled(rect, 0.0, fill);
        if line != Color32::TRANSPARENT {
            p.rect_stroke(
                rect.shrink(0.5),
                0.0,
                Stroke::new(1.0, line),
                egui::StrokeKind::Inside,
            );
        }
        let job = espacado(&text.to_uppercase(), mono_negrito(12.0), ink, 1.6);
        let galley = p.layout_job(job);
        let pos = rect.center() - galley.size() / 2.0;
        p.galley(pos, galley, ink);
    }
}

/// The small icons drawn with lines.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Icone {
    Engrenagem,
    Atualizar,
    Grade,
    Lista,
    Reticencias,
    Seta,
    Copiar,
    Fechar,
}

/// A square icon button; `on` fills it white (the active view).
pub fn botao_icone(ui: &mut Ui, icon: Icone, on: bool, side: f32, enabled: bool) -> Response {
    let sense = if enabled {
        Sense::click()
    } else {
        Sense::hover()
    };
    let (rect, resp) = ui.allocate_exact_size(Vec2::splat(side), sense);
    pintar_botao_icone(ui, rect, &resp, icon, on, enabled);
    resp
}

/// [`botao_icone`] placed at `rect`.
pub fn botao_icone_em(ui: &mut Ui, rect: Rect, icon: Icone, on: bool, enabled: bool) -> Response {
    let sense = if enabled {
        Sense::click()
    } else {
        Sense::hover()
    };
    let resp = ui.allocate_rect(rect, sense);
    pintar_botao_icone(ui, rect, &resp, icon, on, enabled);
    resp
}

fn pintar_botao_icone(ui: &Ui, rect: Rect, resp: &Response, icon: Icone, on: bool, enabled: bool) {
    let side = rect.width().min(rect.height());
    if ui.is_rect_visible(rect) {
        let hover = enabled && resp.hovered();
        let p = ui.painter();
        let (fill, ink) = if on {
            (paleta().branco, paleta().preto)
        } else if hover {
            (paleta().sobre, paleta().branco)
        } else if enabled {
            (Color32::TRANSPARENT, paleta().texto)
        } else {
            (Color32::TRANSPARENT, paleta().texto_apagado)
        };
        p.rect_filled(rect, 0.0, fill);
        if !on {
            p.rect_stroke(
                rect.shrink(0.5),
                0.0,
                Stroke::new(
                    1.0,
                    if hover {
                        paleta().linha_forte
                    } else {
                        paleta().linha
                    },
                ),
                egui::StrokeKind::Inside,
            );
        }
        pintar_icone(p, rect.shrink(side * 0.3), icon, ink);
    }
}

/// Draws `icon` inside `rect` with 1.4-pixel lines.
pub fn pintar_icone(p: &Painter, rect: Rect, icon: Icone, color: Color32) {
    let st = Stroke::new(1.4, color);
    let c = rect.center();
    let r = rect.width().min(rect.height()) / 2.0;
    match icon {
        Icone::Engrenagem => {
            p.circle_stroke(c, r * 0.42, st);
            for k in 0..8 {
                let a = k as f32 * std::f32::consts::TAU / 8.0;
                let d = vec2(a.cos(), a.sin());
                p.line_segment([c + d * r * 0.62, c + d * r], Stroke::new(2.2, color));
            }
            p.circle_stroke(c, r * 0.68, st);
        }
        Icone::Atualizar => {
            let n = 20;
            let pts: Vec<Pos2> = (0..=n)
                .map(|k| {
                    let a = -1.2 + k as f32 / n as f32 * 5.0;
                    c + vec2(a.cos(), a.sin()) * r * 0.85
                })
                .collect();
            let tip = *pts.last().unwrap_or(&c);
            p.add(egui::Shape::line(pts, st));
            let a = -1.2_f32 + 5.0;
            let tangent = vec2(-a.sin(), a.cos());
            let normal = vec2(a.cos(), a.sin());
            p.line_segment([tip, tip - tangent * r * 0.45 + normal * r * 0.05], st);
            p.line_segment([tip, tip + normal * r * 0.42], st);
        }
        Icone::Grade => {
            let g = r * 0.18;
            let s = r - g / 2.0;
            for (dx, dy) in [(-1.0, -1.0), (1.0, -1.0), (-1.0, 1.0), (1.0, 1.0)] {
                let o = c + vec2(dx * (g / 2.0 + s / 2.0), dy * (g / 2.0 + s / 2.0));
                p.rect_filled(Rect::from_center_size(o, Vec2::splat(s)), 0.0, color);
            }
        }
        Icone::Lista => {
            for k in [-0.7, 0.0, 0.7] {
                let y = c.y + k * r;
                p.line_segment(
                    [pos2(c.x - r, y), pos2(c.x + r, y)],
                    Stroke::new(1.6, color),
                );
            }
        }
        Icone::Reticencias => {
            for k in [-0.6, 0.0, 0.6] {
                p.circle_filled(c + vec2(k * r, r * 0.35), 1.2, color);
            }
        }
        Icone::Seta => {
            let a = c + vec2(-r * 0.6, r * 0.6);
            let b = c + vec2(r * 0.6, -r * 0.6);
            p.line_segment([a, b], st);
            p.line_segment([b, b + vec2(-r * 0.75, 0.0)], st);
            p.line_segment([b, b + vec2(0.0, r * 0.75)], st);
        }
        Icone::Copiar => {
            let s = r * 1.2;
            p.rect_stroke(
                Rect::from_min_size(c + vec2(-r, -r * 0.6), Vec2::splat(s)),
                0.0,
                st,
                egui::StrokeKind::Middle,
            );
            p.rect_stroke(
                Rect::from_min_size(c + vec2(-r * 0.6, -r), Vec2::splat(s)),
                0.0,
                st,
                egui::StrokeKind::Middle,
            );
        }
        Icone::Fechar => {
            p.line_segment([c + vec2(-r, -r) * 0.7, c + vec2(r, r) * 0.7], st);
            p.line_segment([c + vec2(-r, r) * 0.7, c + vec2(r, -r) * 0.7], st);
        }
    }
}

/// Paints an outlined badge with its left edge at `left`, centred on `left.y`; returns its rect.
pub fn pintar_etiqueta(p: &Painter, left: Pos2, text: &str, color: Color32, size: f32) -> Rect {
    let job = espacado(&text.to_uppercase(), mono_negrito(size), color, size * 0.11);
    let galley = p.layout_job(job);
    let rect = Rect::from_min_size(
        pos2(left.x, left.y - (galley.size().y + 6.0) / 2.0),
        galley.size() + vec2(12.0, 6.0),
    );
    p.rect_stroke(
        rect.shrink(0.5),
        0.0,
        Stroke::new(1.0, color.gamma_multiply(0.75)),
        egui::StrokeKind::Inside,
    );
    p.galley(rect.center() - galley.size() / 2.0, galley, color);
    rect
}

/// A one-line text job cut with "…" at `width`.
pub fn linha(text: &str, font: FontId, color: Color32, width: f32) -> LayoutJob {
    let mut job = LayoutJob::simple_singleline(text.to_owned(), font, color);
    job.wrap = egui::text::TextWrapping::truncate_at_width(width.max(1.0));
    job
}

/// A wrapped text job of at most `rows` lines, cut with "…".
pub fn paragrafo(text: &str, font: FontId, color: Color32, width: f32, rows: usize) -> LayoutJob {
    let mut job = LayoutJob::simple(text.to_owned(), font, color, width.max(1.0));
    job.wrap.max_rows = rows;
    job
}

/// An outlined badge in monospace capitals ("ATUALIZAÇÃO DISPONÍVEL").
pub fn etiqueta(ui: &mut Ui, text: &str, color: Color32) -> Response {
    let job = espacado(&text.to_uppercase(), mono_negrito(10.5), color, 1.2);
    let galley = ui.painter().layout_job(job);
    let size = galley.size() + vec2(12.0, 7.0);
    let (rect, resp) = ui.allocate_exact_size(size, Sense::hover());
    if ui.is_rect_visible(rect) {
        let p = ui.painter();
        p.rect_stroke(
            rect.shrink(0.5),
            0.0,
            Stroke::new(1.0, color.gamma_multiply(0.75)),
            egui::StrokeKind::Inside,
        );
        p.galley(rect.center() - galley.size() / 2.0, galley, color);
    }
    resp
}

/// A generated square icon: the initials on the accent colour, rounded corners.
pub fn pintar_monograma(p: &Painter, rect: Rect, initials: &str, accent: Color32) {
    let radius = (rect.width() * 0.22) as u8;
    let dark = Color32::from_rgb(
        (accent.r() as f32 * 0.35) as u8,
        (accent.g() as f32 * 0.35) as u8,
        (accent.b() as f32 * 0.35) as u8,
    );
    p.rect_filled(rect, radius, dark);
    p.rect_stroke(
        rect.shrink(0.5),
        radius,
        Stroke::new(1.0, accent.gamma_multiply(0.8)),
        egui::StrokeKind::Inside,
    );
    p.text(
        rect.center(),
        Align2::CENTER_CENTER,
        initials,
        negrito(rect.height() * 0.42),
        Color32::WHITE,
    );
}

/// Up to two capital initials of a name ("UTSim Seis Sigma" gives "SS", "Ensino" gives "EN").
pub fn iniciais(name: &str) -> String {
    let words: Vec<&str> = name
        .split_whitespace()
        .filter(|w| !w.eq_ignore_ascii_case("utsim") || name.split_whitespace().count() == 1)
        .collect();
    let mut out: String = match words.as_slice() {
        [] => String::new(),
        [one] => one.chars().take(2).collect(),
        [a, b, ..] => a.chars().take(1).chain(b.chars().take(1)).collect(),
    };
    out = out.to_uppercase();
    if out.is_empty() { "?".into() } else { out }
}

/// Decodes a PNG into an egui image (RGBA), or `None` if it is not a readable PNG.
pub fn imagem_png(bytes: &[u8]) -> Option<ColorImage> {
    let (w, h, rgba) = png_rgba(bytes)?;
    Some(ColorImage::from_rgba_unmultiplied([w, h], &rgba))
}

/// Decodes a PNG into width, height and straight (not premultiplied) RGBA bytes.
pub fn png_rgba(bytes: &[u8]) -> Option<(usize, usize, Vec<u8>)> {
    let mut dec = png::Decoder::new(std::io::Cursor::new(bytes));
    dec.set_transformations(png::Transformations::normalize_to_color8());
    let mut reader = dec.read_info().ok()?;
    let mut buf = vec![0; reader.output_buffer_size()?];
    let info = reader.next_frame(&mut buf).ok()?;
    let (w, h) = (info.width as usize, info.height as usize);
    if w == 0 || h == 0 || w > 4096 || h > 4096 {
        return None;
    }
    let data = &buf[..info.buffer_size()];
    let rgba: Vec<u8> = match info.color_type {
        png::ColorType::Rgba => data.to_vec(),
        png::ColorType::Rgb => data
            .as_chunks::<3>()
            .0
            .iter()
            .flat_map(|c| [c[0], c[1], c[2], 255])
            .collect(),
        png::ColorType::Grayscale => data.iter().flat_map(|&g| [g, g, g, 255]).collect(),
        png::ColorType::GrayscaleAlpha => data
            .as_chunks::<2>()
            .0
            .iter()
            .flat_map(|c| [c[0], c[0], c[0], c[1]])
            .collect(),
        png::ColorType::Indexed => return None,
    };
    (rgba.len() == w * h * 4).then_some((w, h, rgba))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn families_always_have_a_font() {
        let defs = fonts_from(None, None, None, None);
        for fam in [
            FontFamily::Proportional,
            FontFamily::Monospace,
            FontFamily::Name(NEGRITO.into()),
            FontFamily::Name(MONO_NEGRITO.into()),
        ] {
            assert!(!defs.families[&fam].is_empty(), "{fam:?}");
        }
        let defs = fonts_from(Some(vec![1]), Some(vec![2]), None, None);
        assert_eq!(
            defs.families[&FontFamily::Name(NEGRITO.into())][0],
            "familia-sans-b"
        );
        assert_eq!(
            defs.families[&FontFamily::Name(NEGRITO.into())][1],
            "familia-sans"
        );
    }

    #[test]
    fn helpers() {
        assert_eq!(
            cor_hex("#22B5FF"),
            Some(Color32::from_rgb(0x22, 0xb5, 0xff))
        );
        assert_eq!(cor_hex("xyz"), None);
        assert_eq!(iniciais("UTSim Seis Sigma"), "SS");
        assert_eq!(iniciais("UTSim"), "UT");
        assert_eq!(iniciais("ensino"), "EN");
        assert_eq!(iniciais(""), "?");
        assert!(style().visuals.window_shadow == Shadow::NONE);
        assert_eq!(
            style().visuals.widgets.inactive.corner_radius,
            CornerRadius::ZERO
        );
    }

    #[test]
    fn png_icons_decode() {
        let bytes = include_bytes!("../../../assets/icone/familia/utsim_64.png");
        let img = imagem_png(bytes).expect("png");
        assert_eq!(img.size, [64, 64]);
        assert!(imagem_png(b"nada").is_none());
    }

    #[test]
    fn widgets_draw() {
        let ctx = Context::default();
        ctx.set_fonts(fonts_from(None, None, None, None));
        install_style(&ctx);
        ctx.run_ui(egui::RawInput::default(), |ui| {
            botao(ui, "Abrir", Botao::Primario, vec2(120.0, 32.0), true);
            botao(ui, "Abrir", Botao::Contorno, vec2(120.0, 32.0), false);
            botao(ui, "Desinstalar", Botao::Perigo, vec2(120.0, 32.0), true);
            for i in [
                Icone::Engrenagem,
                Icone::Atualizar,
                Icone::Grade,
                Icone::Lista,
                Icone::Reticencias,
                Icone::Seta,
                Icone::Copiar,
                Icone::Fechar,
            ] {
                botao_icone(ui, i, i == Icone::Grade, 32.0, true);
            }
            etiqueta(ui, "Instalado", cores::TEXTO_FRACO);
            rotulo(ui, "Ordenar", 11.0, cores::TEXTO_APAGADO);
            pintar_monograma(
                ui.painter(),
                Rect::from_min_size(pos2(0.0, 0.0), Vec2::splat(56.0)),
                "SS",
                cores::VERDE,
            );
        })
        .drop_without_applying_deltas();
    }
}
