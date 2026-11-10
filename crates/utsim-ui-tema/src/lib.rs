//! The UTSim look for egui: one set of colours, spacing and fonts for every egui surface of
//! the family (the Godot interface through `utsim-godot`, and later the launcher and the
//! Lean Six Sigma app).
//!
//! The colours are the ones of the old Godot theme (`godot/tema/utsim.tres`, removed when the
//! whole interface around the 3D view became egui). Pure egui: no backend, no window.
//!
//! ```
//! let ctx = egui::Context::default();
//! utsim_ui_tema::install(&ctx);
//! ```

use egui::{
    Color32, Context, CornerRadius, FontData, FontDefinitions, FontFamily, FontId, Margin, Shadow,
    Stroke, Style, TextStyle, Visuals,
};

/// The palette, named by role (same values as the Godot theme).
pub mod colors {
    use egui::Color32;

    /// Background of the panels and bars.
    pub const PANEL: Color32 = Color32::from_rgb(31, 33, 41);
    /// Background of the top bar (a little lighter than the panels).
    pub const BAR: Color32 = Color32::from_rgb(36, 39, 48);
    /// Thin borders and separators.
    pub const BORDER: Color32 = Color32::from_rgb(56, 61, 74);
    /// A button at rest.
    pub const BUTTON: Color32 = Color32::from_rgb(43, 48, 59);
    pub const BUTTON_BORDER: Color32 = Color32::from_rgb(61, 69, 82);
    /// A button under the mouse.
    pub const HOVER: Color32 = Color32::from_rgb(56, 64, 77);
    pub const HOVER_BORDER: Color32 = Color32::from_rgb(92, 107, 133);
    /// A pressed, selected or primary button.
    pub const SELECTED: Color32 = Color32::from_rgb(51, 84, 140);
    /// The accent (focus ring, selected border, links).
    pub const ACCENT: Color32 = Color32::from_rgb(128, 176, 255);
    /// A disabled button.
    pub const DISABLED: Color32 = Color32::from_rgb(36, 38, 46);
    /// Text fields and lists.
    pub const FIELD: Color32 = Color32::from_rgb(20, 23, 28);
    pub const FIELD_BORDER: Color32 = Color32::from_rgb(86, 96, 116);
    /// Menus and pop-ups.
    pub const MENU: Color32 = Color32::from_rgb(33, 36, 43);
    pub const MENU_BORDER: Color32 = Color32::from_rgb(69, 77, 92);
    /// Tooltips.
    pub const TOOLTIP: Color32 = Color32::from_rgb(15, 18, 23);
    /// Body text.
    pub const TEXT: Color32 = Color32::from_rgb(222, 227, 237);
    /// Text on a hovered or selected widget.
    pub const TEXT_STRONG: Color32 = Color32::WHITE;
    /// Short help lines and secondary text.
    pub const TEXT_WEAK: Color32 = Color32::from_rgb(166, 179, 199);
    /// Text of a disabled widget.
    pub const TEXT_DISABLED: Color32 = Color32::from_rgb(115, 122, 138);
    /// Section titles and labels of groups.
    pub const HEADING: Color32 = Color32::from_rgb(217, 224, 240);
    /// Problems (the model does not run).
    pub const ERROR: Color32 = Color32::from_rgb(255, 115, 107);
    /// Warnings (it runs, but something looks wrong).
    pub const WARNING: Color32 = Color32::from_rgb(250, 204, 89);
    /// Something checked and fine (the green light of a consistency check).
    pub const SUCCESS: Color32 = Color32::from_rgb(115, 217, 128);
    /// The "dados" seal and other badges.
    pub const BADGE: Color32 = Color32::from_rgb(51, 107, 184);
}

/// Sizes in points (one point is one pixel at scale 1).
pub mod sizes {
    /// Body text, buttons and fields (the Godot theme's `default_font_size`).
    pub const BODY: f32 = 15.0;
    /// Section and panel titles.
    pub const HEADING: f32 = 17.0;
    /// Help lines and captions.
    pub const SMALL: f32 = 13.0;
    /// Code and RON text.
    pub const MONO: f32 = 14.0;
    /// Height of the top bar.
    pub const BAR_HEIGHT: f32 = 46.0;
    /// Corner radius of buttons and fields.
    pub const RADIUS: u8 = 4;
}

/// The egui style of UTSim (dark).
pub fn style() -> Style {
    let mut style = Style {
        visuals: visuals(),
        ..Style::default()
    };
    style.text_styles = [
        (TextStyle::Small, FontId::proportional(sizes::SMALL)),
        (TextStyle::Body, FontId::proportional(sizes::BODY)),
        (TextStyle::Button, FontId::proportional(sizes::BODY)),
        (TextStyle::Heading, FontId::proportional(sizes::HEADING)),
        (TextStyle::Monospace, FontId::monospace(sizes::MONO)),
    ]
    .into();
    let s = &mut style.spacing;
    s.item_spacing = egui::vec2(6.0, 6.0);
    s.button_padding = egui::vec2(9.0, 4.0);
    s.interact_size = egui::vec2(36.0, 26.0);
    s.menu_margin = Margin::same(6);
    s.window_margin = Margin::same(10);
    s.indent = 14.0;
    s.combo_height = 320.0;
    s.tooltip_width = 420.0;
    s.menu_width = 260.0;
    s.icon_width = 16.0;
    s.icon_width_inner = 9.0;
    s.icon_spacing = 6.0;
    // Tooltips come quickly but not on a mouse just passing by.
    style.interaction.tooltip_delay = 0.35;
    style.animation_time = 0.08;
    // Scrolling to a widget (the focus, a scripted step) jumps there: no animation that a
    // click could land in the middle of.
    style.scroll_animation = egui::style::ScrollAnimation::none();
    style
}

/// Maximiza a janela de verdade nos primeiros quadros. No Windows, o pedido do construtor
/// (`with_maximized`) às vezes marca a janela como maximizada sem mudar o tamanho; tirar e
/// pôr de novo resolve. Chame em todo quadro: só age uma vez.
pub fn maximizar_no_inicio(ctx: &Context) {
    let id = egui::Id::new("utsim-maximizar-no-inicio");
    let passo = ctx.data(|d| d.get_temp::<u8>(id)).unwrap_or(0);
    match passo {
        0 => ctx.send_viewport_cmd(egui::ViewportCommand::Maximized(false)),
        1 => ctx.send_viewport_cmd(egui::ViewportCommand::Maximized(true)),
        _ => return,
    }
    ctx.data_mut(|d| d.insert_temp(id, passo + 1));
    ctx.request_repaint();
}

/// The colours of every widget state.
pub fn visuals() -> Visuals {
    use colors::*;
    let mut v = Visuals::dark();
    let radius = CornerRadius::same(sizes::RADIUS);
    v.override_text_color = None;
    v.panel_fill = PANEL;
    v.window_fill = PANEL;
    v.window_stroke = Stroke::new(1.0, BORDER);
    v.window_corner_radius = CornerRadius::same(6);
    v.menu_corner_radius = CornerRadius::same(5);
    v.window_shadow = Shadow {
        offset: [0, 6],
        blur: 18,
        spread: 0,
        color: Color32::from_black_alpha(110),
    };
    v.popup_shadow = Shadow {
        offset: [0, 4],
        blur: 10,
        spread: 0,
        color: Color32::from_black_alpha(100),
    };
    v.extreme_bg_color = FIELD;
    v.text_edit_bg_color = Some(FIELD);
    v.faint_bg_color = Color32::from_rgb(36, 39, 47);
    v.code_bg_color = FIELD;
    v.hyperlink_color = ACCENT;
    v.warn_fg_color = WARNING;
    v.error_fg_color = ERROR;
    v.selection.bg_fill = Color32::from_rgb(64, 107, 179);
    v.selection.stroke = Stroke::new(1.0, TEXT_STRONG);
    v.text_cursor.stroke = Stroke::new(2.0, Color32::from_rgb(204, 224, 255));
    v.striped = true;
    v.indent_has_left_vline = false;
    v.collapsing_header_frame = false;

    let w = &mut v.widgets;
    w.noninteractive.bg_fill = PANEL;
    w.noninteractive.weak_bg_fill = PANEL;
    w.noninteractive.bg_stroke = Stroke::new(1.0, BORDER);
    w.noninteractive.fg_stroke = Stroke::new(1.0, TEXT);
    w.noninteractive.corner_radius = radius;

    w.inactive.bg_fill = FIELD;
    w.inactive.weak_bg_fill = BUTTON;
    // Brighter than the button border: an unchecked box must read on the panel.
    w.inactive.bg_stroke = Stroke::new(1.0, FIELD_BORDER);
    w.inactive.fg_stroke = Stroke::new(1.0, TEXT);
    w.inactive.corner_radius = radius;
    w.inactive.expansion = 0.0;

    w.hovered.bg_fill = HOVER;
    w.hovered.weak_bg_fill = HOVER;
    w.hovered.bg_stroke = Stroke::new(1.0, HOVER_BORDER);
    w.hovered.fg_stroke = Stroke::new(1.5, TEXT_STRONG);
    w.hovered.corner_radius = radius;
    w.hovered.expansion = 0.0;

    w.active.bg_fill = SELECTED;
    w.active.weak_bg_fill = SELECTED;
    w.active.bg_stroke = Stroke::new(1.0, ACCENT);
    w.active.fg_stroke = Stroke::new(1.5, TEXT_STRONG);
    w.active.corner_radius = radius;
    w.active.expansion = 0.0;

    w.open.bg_fill = HOVER;
    w.open.weak_bg_fill = HOVER;
    w.open.bg_stroke = Stroke::new(1.0, HOVER_BORDER);
    w.open.fg_stroke = Stroke::new(1.0, TEXT_STRONG);
    w.open.corner_radius = radius;
    v
}

/// System UI fonts tried before egui's own (they are not shipped: read where the system
/// keeps them). The first one found becomes the primary proportional font.
pub const SYSTEM_FONTS: &[&str] = &[
    "C:/Windows/Fonts/segoeui.ttf",
    "/usr/share/fonts/truetype/noto/NotoSans-Regular.ttf",
    "/usr/share/fonts/noto/NotoSans-Regular.ttf",
    "/usr/share/fonts/google-noto/NotoSans-Regular.ttf",
    "/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf",
    "/usr/share/fonts/dejavu/DejaVuSans.ttf",
    "/System/Library/Fonts/SFNS.ttf",
    "/System/Library/Fonts/Helvetica.ttc",
];

/// The fonts: the first system UI font of [`SYSTEM_FONTS`] that exists, then egui's
/// defaults as fallbacks (for the symbols the system font lacks).
pub fn fonts() -> FontDefinitions {
    let found = SYSTEM_FONTS
        .iter()
        .find_map(|p| std::fs::read(p).ok().map(|bytes| (*p, bytes)));
    fonts_with(found.map(|(_, bytes)| bytes))
}

/// The fonts with `primary` (a `.ttf`) in front of egui's defaults, or the defaults alone.
pub fn fonts_with(primary: Option<Vec<u8>>) -> FontDefinitions {
    let mut defs = FontDefinitions::default();
    if let Some(bytes) = primary {
        defs.font_data
            .insert("utsim-ui".to_owned(), FontData::from_owned(bytes).into());
        defs.families
            .entry(FontFamily::Proportional)
            .or_default()
            .insert(0, "utsim-ui".to_owned());
    }
    defs
}

/// Applies the UTSim fonts and style to `ctx` (both themes get the dark style).
pub fn install(ctx: &Context) {
    ctx.set_fonts(fonts());
    install_style(ctx);
}

/// Applies only the style (the fonts are left as they are).
pub fn install_style(ctx: &Context) {
    ctx.set_theme(egui::ThemePreference::Dark);
    let s = style();
    ctx.set_style_of(egui::Theme::Dark, s.clone());
    ctx.set_style_of(egui::Theme::Light, s);
}

#[cfg(feature = "familia")]
pub mod familia;

#[cfg(feature = "captura")]
pub mod captura;

#[cfg(feature = "abertura")]
pub mod abertura;

pub mod arquivos;

/// Widgets with the UTSim look that egui does not have.
pub mod widgets {
    use egui::{Button, CornerRadius, Response, RichText, Stroke, Ui, Widget};

    use super::{colors, sizes};

    /// The small "opens a list" arrow after a menu's name (`("Organizar", down_arrow())`):
    /// a full-size triangle drawn small, which every UI font has.
    pub fn down_arrow() -> RichText {
        RichText::new("▼").size(9.0).color(colors::TEXT_WEAK)
    }

    /// A disclosure triangle at `center`: pointing down when `open`, right when closed.
    pub fn paint_chevron(
        painter: &egui::Painter,
        center: egui::Pos2,
        open: bool,
        color: egui::Color32,
    ) {
        let s = 4.5;
        let pts = if open {
            vec![
                center + egui::vec2(-s, -s * 0.6),
                center + egui::vec2(s, -s * 0.6),
                center + egui::vec2(0.0, s * 0.7),
            ]
        } else {
            vec![
                center + egui::vec2(-s * 0.6, -s),
                center + egui::vec2(s * 0.7, 0.0),
                center + egui::vec2(-s * 0.6, s),
            ]
        };
        painter.add(egui::Shape::convex_polygon(pts, color, Stroke::NONE));
    }

    /// A primary button ("Aplicar"): the selected fill and strong text.
    pub fn primary(text: &str) -> Button<'static> {
        Button::new(RichText::new(text.to_owned()).color(colors::TEXT_STRONG))
            .fill(colors::SELECTED)
            .stroke(Stroke::new(1.0, colors::ACCENT))
    }

    /// A row of toggles that read as one control (only one is on): `items` are the labels,
    /// `selected` the one on. Returns the response of each segment, in order.
    pub fn segmented(ui: &mut Ui, items: &[&str], selected: Option<usize>) -> Vec<Response> {
        let r = sizes::RADIUS;
        let n = items.len();
        ui.scope(|ui| {
            ui.spacing_mut().item_spacing.x = 0.0;
            items
                .iter()
                .enumerate()
                .map(|(k, label)| {
                    let on = selected == Some(k);
                    let corners = CornerRadius {
                        nw: if k == 0 { r } else { 0 },
                        sw: if k == 0 { r } else { 0 },
                        ne: if k + 1 == n { r } else { 0 },
                        se: if k + 1 == n { r } else { 0 },
                    };
                    let text = RichText::new(*label).color(if on {
                        colors::TEXT_STRONG
                    } else {
                        colors::TEXT
                    });
                    Button::new(text)
                        .fill(if on { colors::SELECTED } else { colors::BUTTON })
                        .stroke(Stroke::new(
                            1.0,
                            if on {
                                colors::ACCENT
                            } else {
                                colors::BUTTON_BORDER
                            },
                        ))
                        .corner_radius(corners)
                        .ui(ui)
                })
                .collect()
        })
        .inner
    }
}

/// The fill of a primary button ("Aplicar"): the selected colour.
pub fn primary_fill() -> Color32 {
    colors::SELECTED
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Relative luminance (WCAG) of an sRGB colour.
    fn luminance(c: Color32) -> f32 {
        let f = |v: u8| {
            let v = v as f32 / 255.0;
            if v <= 0.04045 {
                v / 12.92
            } else {
                ((v + 0.055) / 1.055).powf(2.4)
            }
        };
        0.2126 * f(c.r()) + 0.7152 * f(c.g()) + 0.0722 * f(c.b())
    }

    fn contrast(a: Color32, b: Color32) -> f32 {
        let (la, lb) = (luminance(a), luminance(b));
        (la.max(lb) + 0.05) / (la.min(lb) + 0.05)
    }

    #[test]
    fn text_is_readable_on_every_background() {
        use colors::*;
        for bg in [PANEL, BAR, BUTTON, HOVER, SELECTED, FIELD, MENU, TOOLTIP] {
            assert!(
                contrast(TEXT, bg) >= 4.5,
                "texto sobre {bg:?}: {}",
                contrast(TEXT, bg)
            );
        }
        for bg in [PANEL, BAR, FIELD] {
            assert!(contrast(TEXT_WEAK, bg) >= 4.5, "ajuda sobre {bg:?}");
            assert!(contrast(ERROR, bg) >= 4.5, "erro sobre {bg:?}");
            assert!(contrast(WARNING, bg) >= 4.5, "aviso sobre {bg:?}");
            assert!(contrast(SUCCESS, bg) >= 4.5, "certo sobre {bg:?}");
        }
    }

    #[test]
    fn style_uses_the_palette_and_sizes() {
        let s = style();
        assert_eq!(s.visuals.panel_fill, colors::PANEL);
        assert_eq!(s.visuals.widgets.inactive.weak_bg_fill, colors::BUTTON);
        assert_eq!(s.visuals.widgets.active.weak_bg_fill, colors::SELECTED);
        assert_eq!(s.text_styles[&TextStyle::Body].size, sizes::BODY);
        assert_eq!(s.text_styles[&TextStyle::Button].size, sizes::BODY);
        assert!(s.visuals.dark_mode);
    }

    #[test]
    fn fonts_keep_egui_defaults_as_fallback() {
        let defs = fonts_with(Some(vec![0, 1, 2]));
        let family = &defs.families[&FontFamily::Proportional];
        assert_eq!(family[0], "utsim-ui");
        assert!(
            family.len() > 1,
            "os padrões do egui continuam como reserva"
        );
        let plain = fonts_with(None);
        assert!(!plain.families[&FontFamily::Proportional].contains(&"utsim-ui".to_owned()));
    }

    #[test]
    fn segments_touch_and_report_each_click() {
        let ctx = Context::default();
        install_style(&ctx);
        let mut rects = Vec::new();
        ctx.run_ui(egui::RawInput::default(), |ui| {
            ui.horizontal(|ui| {
                let r = widgets::segmented(ui, &["Editar", "Simular"], Some(0));
                rects = r.iter().map(|r| r.rect).collect();
            });
        })
        .drop_without_applying_deltas();
        assert_eq!(rects.len(), 2);
        assert!((rects[0].max.x - rects[1].min.x).abs() < 0.5, "{rects:?}");
    }

    #[test]
    fn install_sets_the_dark_style() {
        let ctx = Context::default();
        install_style(&ctx);
        assert_eq!(ctx.global_style().visuals.panel_fill, colors::PANEL);
    }
}
