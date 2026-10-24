//! The options window, closed at first in the top right corner of the fights and
//! under the club's top bar: language, volumes, full screen and interface size, kept for the next
//! launches.

use std::cell::Cell;
use std::path::Path;

use reesch::{Audio, egui};

use crate::lang::{self, tr};
use crate::saves::{self, Display};

/// The interface sizes offered, in percent of the theme's.
const INTERFACE_SIZES: [u16; 6] = [75, 100, 125, 150, 175, 200];

thread_local! {
    static DISPLAY: Cell<Display> = Cell::new(Display::default());
}

/// Puts the window in full screen or not and sizes the interface as `display` says.
pub fn apply(ui: &egui::Context, display: Display) {
    DISPLAY.set(display);
    macroquad::window::set_fullscreen(display.fullscreen);
    ui.set_zoom_factor(f32::from(display.interface) / 100.0);
}

/// Shows the window `top` points below the top of the screen, under a bar it would
/// hide; the text to show the player when keeping a choice failed.
pub fn window(
    ui: &egui::Context,
    audio: &mut Audio,
    folder: Option<&Path>,
    top: f32,
) -> Option<String> {
    let mut display = DISPLAY.get();
    let mut language = None;
    let mut set = false;
    // Set when changed or let go: a slider dragged changes at every frame.
    let mut slide = |ui: &mut egui::Ui, volume: &mut f32, text: String| {
        let slider = ui.add(egui::Slider::new(volume, 0.0..=1.0).text(text));
        set |= slider.drag_stopped() || (slider.changed() && !slider.dragged());
        slider.changed()
    };
    egui::Window::new(tr!("options"))
        .resizable(false)
        .default_open(false)
        .anchor(egui::Align2::RIGHT_TOP, [-12.0, top + 12.0])
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(tr!("menu-language"));
                let current = lang::language();
                let mut chosen = current;
                egui::ComboBox::from_id_salt("language")
                    .selected_text(
                        lang::LANGUAGES
                            .iter()
                            .find(|l| l.code == current)
                            .map_or(current, |l| l.name),
                    )
                    .show_ui(ui, |ui| {
                        for language in lang::LANGUAGES {
                            ui.selectable_value(&mut chosen, language.code, language.name);
                        }
                    });
                if chosen != current {
                    language = Some(chosen);
                }
            });
            let mut music = audio.music_volume();
            if slide(ui, &mut music, tr!("music")) {
                audio.set_music_volume(music);
            }
            let mut effects = audio.effects_volume();
            if slide(ui, &mut effects, tr!("effects")) {
                audio.set_effects_volume(effects);
            }
            ui.checkbox(&mut display.fullscreen, tr!("fullscreen"));
            ui.horizontal(|ui| {
                ui.label(tr!("interface-size"));
                egui::ComboBox::from_id_salt("interface-size")
                    .selected_text(format!("{} %", display.interface))
                    .show_ui(ui, |ui| {
                        for size in INTERFACE_SIZES {
                            ui.selectable_value(&mut display.interface, size, format!("{size} %"));
                        }
                    });
            });
        });
    if display != DISPLAY.get() {
        apply(ui, display);
        set = true;
    }
    if let Some(code) = language {
        lang::set_language(code);
    }
    let folder = folder?;
    let mut failed = language.and_then(|code| saves::write_language(folder, code).err());
    if set && let Err(e) = saves::write_settings(folder, audio, display) {
        failed = Some(e);
    }
    failed.map(|e| tr!("save-failed", error = e.to_string()))
}
