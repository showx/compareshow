use eframe::egui::{self, Color32, CornerRadius, FontFamily, FontId, TextStyle};
use std::sync::Arc;

pub const BG: Color32 = Color32::from_rgb(14, 17, 22);
pub const RAISE: Color32 = Color32::from_rgb(21, 25, 34);
pub const HOVER: Color32 = Color32::from_rgb(28, 34, 48);
pub const ACTIVE: Color32 = Color32::from_rgb(35, 43, 60);
pub const LINE: Color32 = Color32::from_rgb(42, 49, 64);
pub const TEXT: Color32 = Color32::from_rgb(231, 234, 240);
pub const MUTED: Color32 = Color32::from_rgb(139, 147, 163);
pub const FAINT: Color32 = Color32::from_rgb(92, 101, 118);
pub const ACCENT: Color32 = Color32::from_rgb(77, 163, 255);
pub const OK: Color32 = Color32::from_rgb(60, 207, 145);
pub const WARN: Color32 = Color32::from_rgb(224, 138, 60);
pub const DANGER: Color32 = Color32::from_rgb(232, 93, 93);
pub const LEFT: Color32 = Color32::from_rgb(90, 167, 255);
pub const RIGHT: Color32 = Color32::from_rgb(181, 123, 255);
pub const SELECTED: Color32 = Color32::from_rgb(36, 58, 92);
pub const DIFF_BG: Color32 = Color32::from_rgb(48, 36, 22);
pub const LEFT_BG: Color32 = Color32::from_rgb(22, 36, 52);
pub const RIGHT_BG: Color32 = Color32::from_rgb(36, 26, 48);
pub const INSERT_BG: Color32 = Color32::from_rgb(18, 48, 36);
pub const DELETE_BG: Color32 = Color32::from_rgb(52, 24, 24);
pub const PRIMARY: Color32 = Color32::from_rgb(36, 92, 168);

pub fn install(ctx: &egui::Context) {
    load_fonts(ctx);
    ctx.set_theme(egui::ThemePreference::Dark);
    ctx.all_styles_mut(|style| {
        style
            .text_styles
            .insert(TextStyle::Body, FontId::new(14.0, FontFamily::Proportional));
        style.text_styles.insert(
            TextStyle::Button,
            FontId::new(14.0, FontFamily::Proportional),
        );
        style.text_styles.insert(
            TextStyle::Small,
            FontId::new(12.0, FontFamily::Proportional),
        );
        style.text_styles.insert(
            TextStyle::Heading,
            FontId::new(18.0, FontFamily::Proportional),
        );
        style.text_styles.insert(
            TextStyle::Monospace,
            FontId::new(13.0, FontFamily::Monospace),
        );
        style.visuals.dark_mode = true;
        style.visuals.panel_fill = BG;
        style.visuals.window_fill = RAISE;
        style.visuals.extreme_bg_color = Color32::from_rgb(10, 12, 16);
        style.visuals.faint_bg_color = HOVER;
        style.visuals.code_bg_color = Color32::from_rgb(10, 12, 16);
        style.visuals.selection.bg_fill = SELECTED;
        style.visuals.hyperlink_color = ACCENT;
        style.visuals.override_text_color = Some(TEXT);
        style.visuals.window_stroke.color = LINE;
        style.visuals.widgets.noninteractive.bg_fill = RAISE;
        style.visuals.widgets.noninteractive.fg_stroke.color = TEXT;
        style.visuals.widgets.inactive.bg_fill = RAISE;
        style.visuals.widgets.inactive.weak_bg_fill = RAISE;
        style.visuals.widgets.inactive.fg_stroke.color = TEXT;
        style.visuals.widgets.hovered.bg_fill = HOVER;
        style.visuals.widgets.hovered.fg_stroke.color = TEXT;
        style.visuals.widgets.active.bg_fill = ACTIVE;
        style.visuals.widgets.active.fg_stroke.color = TEXT;
        style.visuals.widgets.open.bg_fill = ACTIVE;
        let radius = CornerRadius::same(6);
        style.visuals.window_corner_radius = CornerRadius::same(8);
        style.visuals.menu_corner_radius = CornerRadius::same(8);
        style.visuals.widgets.noninteractive.corner_radius = radius;
        style.visuals.widgets.inactive.corner_radius = radius;
        style.visuals.widgets.hovered.corner_radius = radius;
        style.visuals.widgets.active.corner_radius = radius;
        style.visuals.widgets.open.corner_radius = radius;
        style.spacing.button_padding = egui::vec2(12.0, 6.0);
        style.spacing.item_spacing = egui::vec2(8.0, 8.0);
        style.spacing.interact_size.y = 32.0;
    });
}

fn load_fonts(ctx: &egui::Context) {
    let mut fonts = egui::FontDefinitions::default();
    if let Some(bytes) = read_first(&[
        "/System/Library/Fonts/Supplemental/Arial Unicode.ttf",
        "/Library/Fonts/Arial Unicode.ttf",
        "C:/Windows/Fonts/msyh.ttf",
        "C:/Windows/Fonts/segoeui.ttf",
        "/usr/share/fonts/opentype/noto/NotoSansCJK-Regular.ttc",
        "/usr/share/fonts/truetype/noto/NotoSansSC-Regular.otf",
    ]) {
        fonts.font_data.insert(
            "cjk".to_owned(),
            Arc::new(egui::FontData::from_owned(bytes)),
        );
        if let Some(family) = fonts.families.get_mut(&FontFamily::Proportional) {
            family.insert(0, "cjk".to_owned());
        }
        if let Some(family) = fonts.families.get_mut(&FontFamily::Monospace) {
            family.push("cjk".to_owned());
        }
    }
    if let Some(bytes) = read_first(&[
        "/System/Library/Fonts/Monaco.ttf",
        "C:/Windows/Fonts/consola.ttf",
        "/usr/share/fonts/truetype/dejavu/DejaVuSansMono.ttf",
    ]) {
        fonts.font_data.insert(
            "mono".to_owned(),
            Arc::new(egui::FontData::from_owned(bytes)),
        );
        if let Some(family) = fonts.families.get_mut(&FontFamily::Monospace) {
            family.insert(0, "mono".to_owned());
        }
    }
    ctx.set_fonts(fonts);
}

fn read_first(paths: &[&str]) -> Option<Vec<u8>> {
    paths.iter().find_map(|path| std::fs::read(path).ok())
}

pub fn status_color(status: crate::model::ItemStatus) -> Color32 {
    match status {
        crate::model::ItemStatus::Same => OK,
        crate::model::ItemStatus::Different => WARN,
        crate::model::ItemStatus::LeftOnly => LEFT,
        crate::model::ItemStatus::RightOnly => RIGHT,
        crate::model::ItemStatus::Unknown => FAINT,
    }
}
