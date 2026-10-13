//! The egui theme: dark wood and parchment with bronze accents, sizes made
//! for the Steam Deck (1280 × 800, 7 inches, touch screen).

use reesch::egui::{self, Color32, FontFamily, FontId, Stroke, TextStyle, vec2};

const PARCHMENT: Color32 = Color32::from_rgb(0xEA, 0xDB, 0xC0);
const BRONZE: Color32 = Color32::from_rgb(0xC8, 0x96, 0x3E);
const BLOOD: Color32 = Color32::from_rgb(0xC0, 0x3A, 0x30);
const WOOD_DARKEST: Color32 = Color32::from_rgb(0x1A, 0x15, 0x12);
const WOOD_DARK: Color32 = Color32::from_rgb(0x24, 0x1E, 0x19);
const WOOD: Color32 = Color32::from_rgb(0x2E, 0x26, 0x20);
const WOOD_LIGHT: Color32 = Color32::from_rgb(0x4A, 0x3B, 0x2E);
const WOOD_LIGHTER: Color32 = Color32::from_rgb(0x5E, 0x4A, 0x38);
const WOOD_LIGHTEST: Color32 = Color32::from_rgb(0x75, 0x59, 0x3F);

pub fn apply(ui: &egui::Context) {
    // `style_mut` sets the active theme's style only: pin it to dark so egui
    // never switches to its untouched light style.
    ui.set_theme(egui::Theme::Dark);
    ui.style_mut(|style| {
        // Valve asks for text of at least 9 px at 1280 × 800; aim well above.
        style.text_styles = [
            (TextStyle::Small, 14.0, FontFamily::Proportional),
            (TextStyle::Body, 18.0, FontFamily::Proportional),
            (TextStyle::Button, 18.0, FontFamily::Proportional),
            (TextStyle::Heading, 26.0, FontFamily::Proportional),
            (TextStyle::Monospace, 16.0, FontFamily::Monospace),
        ]
        .into_iter()
        .map(|(text, size, family)| (text, FontId::new(size, family)))
        .collect();

        // Big enough targets for a finger on the touch screen.
        style.spacing.item_spacing = vec2(10.0, 8.0);
        style.spacing.button_padding = vec2(12.0, 6.0);
        style.spacing.interact_size.y = 32.0;

        let visuals = &mut style.visuals;
        *visuals = egui::Visuals::dark();
        visuals.override_text_color = Some(PARCHMENT);
        visuals.hyperlink_color = BRONZE;
        visuals.warn_fg_color = BRONZE;
        visuals.error_fg_color = BLOOD;
        visuals.window_fill = WOOD;
        visuals.panel_fill = WOOD_DARK;
        visuals.extreme_bg_color = WOOD_DARKEST;
        visuals.faint_bg_color = WOOD_DARK;
        visuals.window_stroke = Stroke::new(1.0_f32, WOOD_LIGHTEST);
        visuals.selection.bg_fill = BRONZE.gamma_multiply(0.5);
        visuals.selection.stroke = Stroke::new(1.0_f32, PARCHMENT);
        for (widget, fill) in [
            (&mut visuals.widgets.inactive, WOOD_LIGHT),
            (&mut visuals.widgets.hovered, WOOD_LIGHTER),
            (&mut visuals.widgets.active, WOOD_LIGHTEST),
            (&mut visuals.widgets.open, WOOD_LIGHTER),
        ] {
            widget.bg_fill = fill;
            widget.weak_bg_fill = fill;
        }
        visuals.widgets.hovered.bg_stroke = Stroke::new(1.0_f32, BRONZE);
        visuals.widgets.active.bg_stroke = Stroke::new(1.0_f32, BRONZE);
    });
}
