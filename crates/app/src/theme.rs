//! The approved demo's look: its colour tokens for light and dark, IBM Plex fonts and the few
//! widgets egui does not draw that way itself.
use std::sync::Arc;

use eframe::egui::{
    self, Color32, CornerRadius, FontData, FontDefinitions, FontFamily, FontId, Margin, Shadow,
    Stroke, TextStyle, Theme, Visuals,
};
use vault_forge_engine::Appearance;

/// Below this window width the sidebar folds into a menu.
pub const NARROW: f32 = 900.0;

/// At or below this window width the top bar's new-version notice keeps only its dot and link.
pub const PHONE: f32 = 600.0;

/// The demo's colour tokens.
pub struct Palette {
    pub ground: Color32,
    pub surface: Color32,
    pub border: Color32,
    raised: Color32,
    active: Color32,
    rule: Color32,
    panel: Color32,
    field: Color32,
    text: Color32,
    text2: Color32,
    muted: Color32,
    primary: Color32,
    primary_hover: Color32,
    on_primary: Color32,
    control: Color32,
    pub accent: Color32,
    negative: Color32,
    negative_edge: Color32,
    warning: Color32,
}

const fn hex(rgb: u32) -> Color32 {
    Color32::from_rgb((rgb >> 16) as u8, (rgb >> 8) as u8, rgb as u8)
}

pub const LIGHT: Palette = Palette {
    ground: hex(0xf6f8f9),
    surface: hex(0xffffff),
    border: hex(0xe8eaec),
    raised: hex(0xf4f6f8),
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
    accent: hex(0x1b6c80),
    negative: hex(0xc4242a),
    negative_edge: hex(0xefc4c2),
    warning: hex(0x92400e),
};

pub const DARK: Palette = Palette {
    ground: hex(0x0f1011),
    surface: hex(0x17181a),
    border: hex(0x2a2c2e),
    raised: hex(0x202123),
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
    accent: hex(0x5fb8cc),
    negative: hex(0xef6b6b),
    negative_edge: hex(0x5a2a2c),
    warning: hex(0xe8b45a),
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
    let mut semibold = vec!["plex-sans-semibold".to_owned()];
    semibold.extend(sans);
    fonts
        .families
        .insert(FontFamily::Name("semibold".into()), semibold);
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
    // Teal is for data only, so a selected row or focused field is marked in grey and ink.
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

/// The sheet the work sits on. When the window is narrow it is square and edge to edge, and the
/// caller draws its one edge, along the top.
pub fn sheet(ui: &egui::Ui, narrow: bool) -> egui::Frame {
    let p = palette(ui);
    let frame = egui::Frame::new().fill(p.surface);
    if narrow {
        frame.inner_margin(Margin::symmetric(16, 20))
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
        ui.add_enabled(enabled, egui::Button::new(text))
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

/// A text field as tall as the demo's, filling the width it is given.
pub fn input(edit: egui::TextEdit<'_>) -> egui::TextEdit<'_> {
    edit.desired_width(f32::INFINITY)
        .margin(egui::vec2(12.0, 10.0))
}

/// A sidebar row: full width, with an optional tag such as the profile's rules on the right.
pub fn nav_row(ui: &mut egui::Ui, selected: bool, text: &str, tag: &str) -> egui::Response {
    let p = palette(ui);
    let ink = if selected { p.text } else { p.text2 };
    let button = egui::Button::selectable(selected, egui::RichText::new(text).color(ink))
        .stroke(Stroke::NONE)
        .corner_radius(8)
        .min_size(egui::vec2(ui.available_width(), 32.0));
    // The tag is part of the row's name for screen readers, so an empty one is left out.
    ui.add(match tag {
        "" => button.right_text(egui::Atom::grow()),
        tag => button.right_text(egui::RichText::new(tag).small().color(p.muted)),
    })
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
                    let mut text =
                        egui::RichText::new(*label).color(if chosen { p.text } else { p.muted });
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

/// The engine's appearance choice as egui's theme preference.
pub fn preference(appearance: Appearance) -> egui::ThemePreference {
    match appearance {
        Appearance::System => egui::ThemePreference::System,
        Appearance::Light => egui::ThemePreference::Light,
        Appearance::Dark => egui::ThemePreference::Dark,
    }
}
