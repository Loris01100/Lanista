//! The egui theme: dark wood and parchment with bronze accents, sizes made
//! for the Steam Deck (1280 × 800, 7 inches, touch screen).

use reesch::NineSlice;
use reesch::egui::{self, Color32, FontFamily, FontId, Stroke, TextStyle, vec2};

pub const PARCHMENT: Color32 = Color32::from_rgb(0xEA, 0xDB, 0xC0);
pub const BRONZE: Color32 = Color32::from_rgb(0xC8, 0x96, 0x3E);
pub const BLOOD: Color32 = Color32::from_rgb(0xC0, 0x3A, 0x30);
/// Olive green: wins, good health, a full gauge.
pub const LAUREL: Color32 = Color32::from_rgb(0x7E, 0x9A, 0x48);
/// Faded blue: morale, the calm traits.
pub const SEA: Color32 = Color32::from_rgb(0x4E, 0x7C, 0x9A);
const WOOD_DARKEST: Color32 = Color32::from_rgb(0x1A, 0x15, 0x12);
/// Text on parchment: the newspaper, the calendar's weeks.
pub const INK: Color32 = WOOD_DARKEST;
/// Red ink on parchment: the newspaper's sections, the player's club in it.
pub const RED_INK: Color32 = Color32::from_rgb(0x8A, 0x24, 0x1C);
const WOOD_DARK: Color32 = Color32::from_rgb(0x24, 0x1E, 0x19);
const WOOD: Color32 = Color32::from_rgb(0x2E, 0x26, 0x20);
const WOOD_LIGHT: Color32 = Color32::from_rgb(0x4A, 0x3B, 0x2E);
const WOOD_LIGHTER: Color32 = Color32::from_rgb(0x5E, 0x4A, 0x38);
const WOOD_LIGHTEST: Color32 = Color32::from_rgb(0x75, 0x59, 0x3F);

/// Roman capitals for the titles.
const DISPLAY: &str = "display";
/// The body text.
const BODY: &str = "body";

/// The game's fonts, embedded so that the tests' themes have them too.
fn fonts() -> egui::FontDefinitions {
    let mut fonts = egui::FontDefinitions::default();
    for (name, bytes) in [
        (
            DISPLAY,
            &include_bytes!("../../../assets/fonts/Cinzel.ttf")[..],
        ),
        (
            BODY,
            &include_bytes!("../../../assets/fonts/Alegreya.ttf")[..],
        ),
    ] {
        fonts
            .font_data
            .insert(name.to_owned(), egui::FontData::from_static(bytes).into());
    }
    // In front of egui's own, kept as fallbacks for what ours lack (emoji…).
    fonts
        .families
        .entry(FontFamily::Proportional)
        .or_default()
        .insert(0, BODY.to_owned());
    let mut display = vec![DISPLAY.to_owned()];
    display.extend(fonts.families[&FontFamily::Proportional].iter().cloned());
    fonts
        .families
        .insert(FontFamily::Name(DISPLAY.into()), display);
    fonts
}

/// Width of the frames' border, in points.
pub const FRAME_BORDER: f32 = 8.0;

/// The 9-slice frames, embedded like the fonts.
#[derive(Clone, Copy)]
pub enum Frame {
    /// The combat screen's windows.
    Wood,
    /// The newspaper, in a bronze border.
    Parchment,
}

impl Frame {
    /// The frame for `ui`'s context, its texture loaded on first use.
    pub fn get(self, ui: &egui::Context) -> NineSlice {
        let (name, bytes) = match self {
            Self::Wood => (
                "frame-wood",
                &include_bytes!("../../../assets/ui/wood.png")[..],
            ),
            Self::Parchment => (
                "frame-parchment",
                &include_bytes!("../../../assets/ui/parchment.png")[..],
            ),
        };
        let id = egui::Id::new(name);
        if let Some(frame) = ui.data(|d| d.get_temp(id)) {
            return frame;
        }
        let frame =
            NineSlice::load(ui, name, bytes, FRAME_BORDER).expect("an embedded frame is a PNG");
        ui.data_mut(|d| d.insert_temp(id, frame.clone()));
        frame
    }

    /// `add`'s contents in the frame.
    pub fn show<R>(self, ui: &mut egui::Ui, add: impl FnOnce(&mut egui::Ui) -> R) -> R {
        self.get(ui.ctx()).show(ui, add).inner
    }
}

pub fn apply(ui: &egui::Context) {
    ui.set_fonts(fonts());
    // `style_mut` sets the active theme's style only: pin it to dark so egui
    // never switches to its untouched light style.
    ui.set_theme(egui::Theme::Dark);
    ui.style_mut(|style| {
        // Valve asks for text of at least 9 px at 1280 × 800; aim well above.
        style.text_styles = [
            (TextStyle::Small, 14.0, FontFamily::Proportional),
            (TextStyle::Body, 18.0, FontFamily::Proportional),
            (TextStyle::Button, 18.0, FontFamily::Proportional),
            (TextStyle::Heading, 26.0, FontFamily::Name(DISPLAY.into())),
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_theme_is_dark_parchment_on_wood() {
        let ui = egui::Context::default();
        apply(&ui);
        let style = ui.style();
        assert_eq!(style.visuals.override_text_color, Some(PARCHMENT));
        assert_eq!(style.visuals.panel_fill, WOOD_DARK);
        assert_eq!(style.text_styles[&TextStyle::Body].size, 18.0);
    }

    #[test]
    fn the_frames_are_loaded_once() {
        let ui = egui::Context::default();
        for frame in [Frame::Wood, Frame::Parchment] {
            assert_eq!(frame.get(&ui).texture.id(), frame.get(&ui).texture.id());
        }
    }
}
