//! The approved demo's look: its colour tokens for light and dark, IBM Plex fonts and the few
//! widgets egui does not draw that way itself.
use std::sync::Arc;

use eframe::egui::{
    self, Color32, CornerRadius, FontData, FontDefinitions, FontFamily, FontId, Margin, Shadow,
    Stroke, TextStyle, Theme, Visuals,
};
use vault_forge_engine::Appearance;

/// At or below this window width the sidebar folds into a menu.
pub const NARROW: f32 = 900.0;
/// At or below this width the sheet's padding tightens, the top bar shows only the name and its
/// new-version notice keeps only its dot and link.
pub const PHONE: f32 = 600.0;

/// The demo's colour tokens.
pub struct Palette {
    pub ground: Color32,
    pub surface: Color32,
    pub border: Color32,
    pub raised: Color32,
    hover: Color32,
    active: Color32,
    rule: Color32,
    panel: Color32,
    field: Color32,
    pub text: Color32,
    pub text2: Color32,
    muted: Color32,
    primary: Color32,
    primary_hover: Color32,
    on_primary: Color32,
    control: Color32,
    toggle_off: Color32,
    pub accent: Color32,
    pub accent_pale: Color32,
    negative: Color32,
    negative_edge: Color32,
    pub warning: Color32,
    pub warning_wash: Color32,
    pub warning_edge: Color32,
}

const fn hex(rgb: u32) -> Color32 {
    Color32::from_rgb((rgb >> 16) as u8, (rgb >> 8) as u8, rgb as u8)
}

pub const LIGHT: Palette = Palette {
    ground: hex(0xf6f8f9),
    surface: hex(0xffffff),
    border: hex(0xe8eaec),
    raised: hex(0xf4f6f8),
    hover: hex(0xeceef0),
    active: hex(0xe6e9eb),
    rule: hex(0xf0f1f2),
    panel: hex(0xfafafa),
    field: hex(0xffffff),
    text: hex(0x131416),
    text2: hex(0x3b3d3f),
    muted: hex(0x5d5f60),
    primary: hex(0x1d1f20),
    primary_hover: hex(0x3a3d3f),
    on_primary: hex(0xffffff),
    control: hex(0xcdced1),
    toggle_off: hex(0xb9bcbf),
    accent: hex(0x1b6c80),
    accent_pale: hex(0xe3eff2),
    negative: hex(0xc4242a),
    negative_edge: hex(0xefc4c2),
    warning: hex(0x92400e),
    warning_wash: hex(0xfffbeb),
    warning_edge: hex(0xfde68a),
};

pub const DARK: Palette = Palette {
    ground: hex(0x0f1011),
    surface: hex(0x17181a),
    border: hex(0x2a2c2e),
    raised: hex(0x202123),
    hover: hex(0x1c1d1f),
    active: hex(0x2a2c2e),
    rule: hex(0x222325),
    panel: hex(0x1d1e20),
    field: hex(0x141517),
    text: hex(0xf2f3f4),
    text2: hex(0xd0d2d4),
    muted: hex(0xb0b3b6),
    primary: hex(0xf2f3f4),
    primary_hover: hex(0xd0d2d4),
    on_primary: hex(0x111213),
    control: hex(0x45474a),
    toggle_off: hex(0x55585b),
    accent: hex(0x5fb8cc),
    accent_pale: hex(0x163038),
    negative: hex(0xef6b6b),
    negative_edge: hex(0x5a2a2c),
    warning: hex(0xe8b45a),
    warning_wash: hex(0x2b2213),
    warning_edge: hex(0x5a4520),
};

pub fn palette(ui: &egui::Ui) -> &'static Palette {
    if ui.visuals().dark_mode {
        &DARK
    } else {
        &LIGHT
    }
}

/// Embedded IBM Plex fonts, text sizes and both palettes. Call once on the app's context.
pub fn install(ctx: &egui::Context) {
    let mut fonts = FontDefinitions::default();
    for (name, bytes) in [
        (
            "plex-sans",
            &include_bytes!("../fonts/IBMPlexSans-Regular.ttf")[..],
        ),
        (
            "plex-sans-medium",
            &include_bytes!("../fonts/IBMPlexSans-Medium.ttf")[..],
        ),
        (
            "plex-sans-semibold",
            &include_bytes!("../fonts/IBMPlexSans-SemiBold.ttf")[..],
        ),
        (
            "plex-mono",
            &include_bytes!("../fonts/IBMPlexMono-Regular.ttf")[..],
        ),
    ] {
        fonts
            .font_data
            .insert(name.into(), Arc::new(FontData::from_static(bytes)));
    }
    // egui's own fonts stay behind Plex as fallbacks for glyphs Plex lacks.
    let sans = fonts.families[&FontFamily::Proportional].clone();
    for weight in ["medium", "semibold"] {
        let mut family = vec![format!("plex-sans-{weight}")];
        family.extend(sans.iter().cloned());
        fonts
            .families
            .insert(FontFamily::Name(weight.into()), family);
    }
    for (family, font) in [
        (FontFamily::Proportional, "plex-sans"),
        (FontFamily::Monospace, "plex-mono"),
    ] {
        fonts
            .families
            .get_mut(&family)
            .unwrap()
            .insert(0, font.into());
    }
    ctx.set_fonts(fonts);

    ctx.all_styles_mut(|style| {
        style.text_styles = [
            (
                TextStyle::Heading,
                FontId::new(22.0, FontFamily::Name("semibold".into())),
            ),
            (TextStyle::Body, FontId::proportional(14.0)),
            (TextStyle::Button, FontId::proportional(14.0)),
            (TextStyle::Small, FontId::proportional(12.0)),
            (TextStyle::Monospace, FontId::monospace(13.0)),
        ]
        .into();
        style.spacing.button_padding = egui::vec2(14.0, 7.0);
    });
    ctx.set_visuals_of(Theme::Light, visuals(&LIGHT, Visuals::light()));
    ctx.set_visuals_of(Theme::Dark, visuals(&DARK, Visuals::dark()));
}

fn visuals(p: &Palette, base: Visuals) -> Visuals {
    let mut v = base;
    v.panel_fill = p.ground;
    v.window_fill = p.surface;
    v.window_stroke = Stroke::new(1.0, p.border);
    v.window_corner_radius = CornerRadius::same(12);
    v.window_shadow = Shadow {
        offset: [0, 16],
        blur: 48,
        spread: 0,
        color: Color32::from_black_alpha(46),
    };
    v.faint_bg_color = p.panel;
    v.extreme_bg_color = p.field;
    v.code_bg_color = p.raised;
    v.hyperlink_color = p.accent;
    v.warn_fg_color = p.warning;
    v.error_fg_color = p.negative;
    v.weak_text_color = Some(p.muted);
    // A selected row is marked in grey and ink; keyboard focus is `focus_ring`'s.
    v.selection.bg_fill = p.active;
    v.selection.stroke = Stroke::new(1.0, p.text);
    v.text_cursor.stroke = Stroke::new(2.0, p.text);

    let w = &mut v.widgets;
    w.noninteractive.bg_fill = p.surface;
    w.noninteractive.weak_bg_fill = p.surface;
    w.noninteractive.bg_stroke = Stroke::new(1.0, p.rule);
    w.noninteractive.fg_stroke = Stroke::new(1.0, p.text);
    for (state, fill, ink) in [
        (&mut w.inactive, p.surface, p.text2),
        (&mut w.hovered, p.raised, p.text),
        (&mut w.active, p.active, p.text),
        (&mut w.open, p.active, p.text),
    ] {
        state.weak_bg_fill = fill;
        state.bg_fill = fill;
        state.bg_stroke = Stroke::new(1.0, p.control);
        state.fg_stroke = Stroke::new(1.0, ink);
        state.corner_radius = CornerRadius::same(6);
        state.expansion = 0.0;
    }
    v
}

/// The sheet the work sits on, for a window this wide. When the window is narrow it is square and
/// edge to edge, and the caller draws its one edge, along the top.
pub fn sheet(ui: &egui::Ui, width: f32) -> egui::Frame {
    let p = palette(ui);
    let frame = egui::Frame::new().fill(p.surface);
    if width <= PHONE {
        frame.inner_margin(Margin::symmetric(16, 20))
    } else if width <= NARROW {
        frame.inner_margin(24)
    } else {
        frame
            .stroke(Stroke::new(1.0, p.border))
            .corner_radius(16)
            .outer_margin(Margin {
                left: 0,
                right: 8,
                top: 0,
                bottom: 8,
            })
            .inner_margin(24)
    }
}

/// The demo's dialog: no edge, 12px corners, 24 padding and a soft shadow.
pub fn dialog(ctx: &egui::Context) -> egui::Frame {
    egui::Frame::window(&ctx.global_style())
        .stroke(Stroke::NONE)
        .inner_margin(24)
}

/// The dialog's content width for this window: the demo's `min(480px, 100vw - 32px)` less its
/// padding.
pub fn dialog_width(ctx: &egui::Context) -> f32 {
    (ctx.content_rect().width() - 32.0).min(480.0) - 48.0
}

/// The dialog's heading: 18px SemiBold with 8px below.
pub fn dialog_title(ui: &mut egui::Ui, text: &str) -> egui::Response {
    let title = ui.label(
        egui::RichText::new(text).font(FontId::new(18.0, FontFamily::Name("semibold".into()))),
    );
    ui.add_space(8.0 - ui.spacing().item_spacing.y);
    title
}

/// The dialog's buttons: at the right, 8px apart, 20px below the content. `add` adds them in
/// reading order, the main action last, so Tab moves left to right.
pub fn dialog_actions(ui: &mut egui::Ui, add: impl FnOnce(&mut egui::Ui)) {
    ui.add_space(20.0 - ui.spacing().item_spacing.y);
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 8.0;
        // Pushed right by the buttons' width from the frame before; a modal's first frame is an
        // unseen sizing pass, so it never shows them unaligned.
        let id = ui.scope_id().with("actions_width");
        let width = ui.data(|d| d.get_temp::<f32>(id)).unwrap_or(0.0);
        ui.add_space((ui.available_width() - width).max(0.0));
        let start = ui.cursor().min.x;
        add(ui);
        let width = ui.cursor().min.x - start - ui.spacing().item_spacing.x;
        ui.data_mut(|d| d.insert_temp(id, width));
    });
}

/// The near-black main action; grey and muted while it cannot be pressed.
pub fn primary_button(ui: &mut egui::Ui, enabled: bool, text: &str) -> egui::Response {
    let p = palette(ui);
    let (rest, ink) = if enabled {
        (p.primary, p.on_primary)
    } else {
        (p.active, p.muted)
    };
    ui.scope(|ui| {
        let v = ui.visuals_mut();
        // Drawn in its own disabled colours rather than faded.
        v.disabled_alpha = 1.0;
        let w = &mut v.widgets;
        for (state, fill) in [
            (&mut w.inactive, rest),
            (&mut w.hovered, p.primary_hover),
            (&mut w.active, p.primary_hover),
        ] {
            state.weak_bg_fill = fill;
            state.bg_fill = fill;
            state.bg_stroke = Stroke::new(1.0, fill);
            state.fg_stroke = Stroke::new(1.0, ink);
        }
        ui.add_enabled(enabled, egui::Button::new(medium(text)))
    })
    .inner
}

/// The app's mark and name at the head of the sidebar and the menu.
pub fn brand(ui: &mut egui::Ui) {
    let (mark, _) = ui.allocate_exact_size(egui::vec2(22.0, 22.0), egui::Sense::hover());
    let painter = ui.painter();
    // The demo's mark is the same teal in both palettes.
    painter.rect_filled(mark.shrink(1.0), 5, LIGHT.accent);
    // An open book: two pages meeting at the spine.
    let ink = Stroke::new(1.4, Color32::WHITE);
    let at = |x: f32, y: f32| mark.min + egui::vec2(x, y);
    for edge in [6.0, 16.0] {
        let page = vec![at(11.0, 7.1), at(edge, 6.5), at(edge, 15.7), at(11.0, 16.5)];
        painter.add(egui::Shape::line(page, ink));
    }
    painter.line_segment([at(11.0, 7.1), at(11.0, 16.5)], ink);
    ui.label(
        egui::RichText::new("Vault Forge")
            .font(FontId::new(15.0, FontFamily::Name("semibold".into()))),
    );
}

/// Text in IBM Plex Sans Medium, the demo's weight 500 for labels and emphasis.
pub fn medium(text: impl Into<String>) -> egui::RichText {
    egui::RichText::new(text).family(FontFamily::Name("medium".into()))
}

/// The demo's text field: 40 tall, filling the width it is given, with a 1px edge and 10px
/// corners.
pub fn input(ui: &mut egui::Ui, edit: egui::TextEdit<'_>) -> egui::Response {
    field(ui, edit, CornerRadius::same(10), 40.0)
}

/// The demo's smaller field, 32 tall with 8px corners, as the Books search is.
pub fn small_input(ui: &mut egui::Ui, edit: egui::TextEdit<'_>) -> egui::Response {
    field(ui, edit, CornerRadius::same(8), 32.0)
}

/// One Setup section: its title and description on the left, 340 wide, and its panel on the
/// right. A narrow window stacks the description above the panel.
pub fn section(
    ui: &mut egui::Ui,
    describe: impl FnOnce(&mut egui::Ui),
    panel: impl FnOnce(&mut egui::Ui),
) {
    let narrow = ui.ctx().content_rect().width() <= NARROW;
    let layout = if narrow {
        egui::Layout::top_down(egui::Align::Min)
    } else {
        egui::Layout::left_to_right(egui::Align::Min)
    };
    ui.with_layout(layout, |ui| {
        let width = if narrow { ui.available_width() } else { 340.0 };
        let column = egui::Layout::top_down(egui::Align::Min);
        ui.allocate_ui_with_layout(egui::vec2(width, 0.0), column, |ui| {
            ui.set_width(width);
            describe(ui);
        });
        if narrow {
            ui.add_space(12.0 - ui.spacing().item_spacing.y);
        } else {
            ui.add_space(24.0);
        }
        egui::Frame::new()
            .fill(ui.visuals().faint_bg_color)
            .corner_radius(16.0)
            .inner_margin(16.0)
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                // The row around it is horizontal; the panel's contents stack.
                ui.vertical(panel);
            });
    });
}

/// The demo's path line: a field joined to the button that fills it, as one 40-tall control.
/// Returns the field's response and the button's.
pub fn pathline(
    ui: &mut egui::Ui,
    edit: egui::TextEdit<'_>,
    button: &str,
    enabled: bool,
) -> (egui::Response, egui::Response) {
    let p = palette(ui);
    let layout = egui::Layout::right_to_left(egui::Align::Center);
    ui.allocate_ui_with_layout(egui::vec2(ui.available_width(), 40.0), layout, |ui| {
        // The two overlap by a pixel, so they share one edge between them.
        ui.spacing_mut().item_spacing.x = -1.0;
        let right = CornerRadius {
            ne: 10,
            se: 10,
            ..CornerRadius::ZERO
        };
        let button = ui.add_enabled(
            enabled,
            egui::Button::new(button)
                .corner_radius(right)
                .min_size(egui::vec2(0.0, 40.0)),
        );
        ring_corners(&button, right);
        let left = CornerRadius {
            nw: 10,
            sw: 10,
            ..CornerRadius::ZERO
        };
        let field = field(ui, edit, left, 40.0);
        // The button has no left edge of its own: the shared edge is the field's.
        let edge = button.rect.left() + 0.5;
        ui.painter().vline(
            edge,
            button.rect.y_range().shrink(1.0),
            Stroke::new(1.0, p.border),
        );
        (field, button)
    })
    .inner
}

fn field(
    ui: &mut egui::Ui,
    edit: egui::TextEdit<'_>,
    corners: CornerRadius,
    height: f32,
) -> egui::Response {
    let p = palette(ui);
    ui.scope(|ui| {
        let v = ui.visuals_mut();
        // The edge stays as it is while focused; `focus_ring` marks the focus.
        v.selection.stroke = Stroke::new(1.0, p.border);
        for state in [
            &mut v.widgets.inactive,
            &mut v.widgets.hovered,
            &mut v.widgets.active,
        ] {
            state.bg_stroke = Stroke::new(1.0, p.border);
            state.corner_radius = corners;
        }
        let field = ui.add(
            edit.desired_width(f32::INFINITY)
                .margin(egui::vec2(12.0, (height - 20.0) / 2.0))
                .min_size(egui::vec2(0.0, height))
                .align(egui::Align2::LEFT_CENTER),
        );
        ring_corners(&field, corners);
        field
    })
    .inner
}

/// The demo's keyboard focus mark: a 2px accent outline, 2px outside the focused control. Call
/// once a frame, after everything else is drawn.
pub fn focus_ring(ctx: &egui::Context) {
    let Some(focused) = ctx
        .memory(|m| m.focused())
        .and_then(|id| ctx.read_response(id))
    else {
        return;
    };
    // The outline follows the control's corners out; a square corner stays square.
    let corners = ctx
        .data(|d| d.get_temp::<CornerRadius>(focused.id))
        .unwrap_or(CornerRadius::same(6));
    let out = |r: u8| if r == 0 { 0 } else { r + 2 };
    let corners = CornerRadius {
        nw: out(corners.nw),
        ne: out(corners.ne),
        sw: out(corners.sw),
        se: out(corners.se),
    };
    let accent = ctx.global_style().visuals.hyperlink_color;
    let layer = egui::LayerId::new(egui::Order::Tooltip, egui::Id::unique("focus_ring"));
    ctx.layer_painter(layer).rect_stroke(
        focused.rect.expand(2.0),
        corners,
        Stroke::new(2.0, accent),
        egui::StrokeKind::Outside,
    );
}

/// Tells `focus_ring` the corners of a control not drawn with the usual 6px ones.
fn ring_corners(response: &egui::Response, corners: CornerRadius) {
    response
        .ctx
        .data_mut(|d| d.insert_temp(response.id, corners));
}

/// A sidebar row: full width, with an optional tag such as the profile's rules on the right.
pub fn nav_row(ui: &mut egui::Ui, selected: bool, text: &str, tag: &str) -> egui::Response {
    let p = palette(ui);
    let ink = if selected { p.text } else { p.text2 };
    row(ui, selected, egui::RichText::new(text).color(ink), tag)
}

/// The sidebar's quieter row that adds something, such as "+ New profile".
pub fn add_row(ui: &mut egui::Ui, selected: bool, text: &str) -> egui::Response {
    let p = palette(ui);
    row(ui, selected, egui::RichText::new(text).color(p.muted), "")
}

fn row(ui: &mut egui::Ui, selected: bool, text: egui::RichText, tag: &str) -> egui::Response {
    let p = palette(ui);
    ui.scope(|ui| {
        ui.spacing_mut().button_padding.x = 10.0;
        let hovered = &mut ui.visuals_mut().widgets.hovered;
        hovered.weak_bg_fill = p.hover;
        hovered.bg_fill = p.hover;
        let button = egui::Button::selectable(selected, text)
            .stroke(Stroke::NONE)
            .corner_radius(8)
            .min_size(egui::vec2(ui.available_width(), 32.0));
        // The tag is part of the row's name for screen readers, so an empty one is left out.
        let row = ui.add(match tag {
            "" => button.right_text(egui::Atom::grow()),
            tag => button.right_text(egui::RichText::new(tag).small().color(p.muted)),
        });
        ring_corners(&row, CornerRadius::same(8));
        row
    })
    .inner
}

/// A button for an action that removes something.
pub fn danger_button(ui: &mut egui::Ui, text: &str) -> egui::Response {
    let p = palette(ui);
    ui.add(
        egui::Button::new(egui::RichText::new(text).color(p.negative))
            .stroke(Stroke::new(1.0, p.negative_edge)),
    )
}

/// The demo's segmented switch: a grey track with the chosen option on a field-coloured chip.
/// `fill` stretches it across the available width in small type, as the sidebar's Appearance
/// switch does.
pub fn segmented<T: PartialEq + Copy>(
    ui: &mut egui::Ui,
    value: &mut T,
    choices: &[(T, &str)],
    fill: bool,
) {
    let p = palette(ui);
    egui::Frame::new()
        .fill(p.active)
        .corner_radius(8)
        .inner_margin(3)
        .show(ui, |ui| {
            ui.spacing_mut().item_spacing.x = 2.0;
            let (width, height) = if fill {
                let gaps = 2.0 * (choices.len() - 1) as f32;
                ((ui.available_width() - gaps) / choices.len() as f32, 28.0)
            } else {
                (0.0, 30.0)
            };
            ui.horizontal(|ui| {
                for (choice, label) in choices {
                    let chosen = *value == *choice;
                    let mut text = if chosen {
                        medium(*label).color(p.text)
                    } else {
                        egui::RichText::new(*label).color(p.muted)
                    };
                    if fill {
                        text = text.small();
                    }
                    let mut button = egui::Button::selectable(chosen, text)
                        .corner_radius(6)
                        .stroke(Stroke::NONE)
                        .min_size(egui::vec2(width, height));
                    if chosen {
                        button = button.fill(p.field);
                    }
                    if ui.add(button).clicked() {
                        *value = *choice;
                    }
                }
            });
        });
}

/// The demo's on/off switch row: the label on the left, the switch at the right edge.
pub fn toggle(ui: &mut egui::Ui, on: &mut bool, label: &str) -> egui::Response {
    let p = palette(ui);
    ui.horizontal(|ui| {
        ui.label(medium(label).color(p.text));
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            let (rect, mut response) =
                ui.allocate_exact_size(egui::vec2(32.0, 18.0), egui::Sense::click());
            if response.clicked() {
                *on = !*on;
                response.mark_changed();
            }
            let enabled = ui.is_enabled();
            response.widget_info(|| {
                egui::WidgetInfo::selected(egui::Role::Switch, enabled, *on, label)
            });
            let painter = ui.painter();
            painter.rect_filled(rect, 9, if *on { p.primary } else { p.toggle_off });
            // The demo's knob slides over 120 ms.
            let t = ui.ctx().animate_bool_with_time(response.id, *on, 0.12);
            let x = egui::lerp(rect.left() + 9.0..=rect.right() - 9.0, t);
            painter.circle_filled(egui::pos2(x, rect.center().y), 7.0, p.surface);
            response
        })
        .inner
    })
    .inner
}

/// The demo's progress track: an accent bar sliding along a thin grey track, for work whose
/// length is not known.
pub fn track(ui: &mut egui::Ui) {
    let p = palette(ui);
    let size = egui::vec2(ui.available_width(), 6.0);
    let (rect, _) = ui.allocate_exact_size(size, egui::Sense::hover());
    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, 3, p.active);
    // The demo's slide: 28% wide, from -100% to 360% of its width, ease-in-out over 1.4 s.
    let width = rect.width() * 0.28;
    let t = (ui.input(|i| i.time) / 1.4).fract() as f32;
    let eased = t * t * (3.0 - 2.0 * t);
    let bar = egui::Rect::from_min_size(
        egui::pos2(rect.left() + width * (4.6 * eased - 1.0), rect.top()),
        size * egui::vec2(0.28, 1.0),
    );
    painter.rect_filled(bar, 3, p.accent);
    ui.ctx()
        .request_repaint_after(std::time::Duration::from_millis(16));
}

/// The engine's appearance choice as egui's theme preference.
pub fn preference(appearance: Appearance) -> egui::ThemePreference {
    match appearance {
        Appearance::System => egui::ThemePreference::System,
        Appearance::Light => egui::ThemePreference::Light,
        Appearance::Dark => egui::ThemePreference::Dark,
    }
}
