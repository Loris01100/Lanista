//! The options, laid over the screen from the main menu, a button of the club's top
//! bar or the fight's pause menu: graphics, audio and language in tabs, kept for the next launches.

use std::cell::Cell;
use std::path::Path;

use reesch::{Audio, egui};

use crate::lang::{self, tr};
use crate::saves::{self, Display};

/// The interface sizes offered, in percent of the theme's.
const INTERFACE_SIZES: [u16; 6] = [75, 100, 125, 150, 175, 200];
/// Width of the options, so that changing tabs does not resize them.
const WIDTH: f32 = 420.0;

/// The kinds of options, one tab each.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
enum Tab {
    #[default]
    Graphics,
    Audio,
    Language,
}

thread_local! {
    static DISPLAY: Cell<Display> = Cell::new(Display::default());
    /// The tab shown, the same when the options open again.
    static TAB: Cell<Tab> = Cell::new(Tab::default());
}

/// Puts the window in full screen or not and sizes the interface as `display` says.
pub fn apply(display: Display) {
    DISPLAY.set(display);
    macroquad::window::set_fullscreen(display.fullscreen);
    reesch::set_ui_scale(f32::from(display.interface) / 100.0);
}

/// Shows the options over the screen while `open`, which a click outside, Échap or
/// « Fermer » sets back to false; the text to show the player when keeping a choice
/// failed.
pub fn window(
    ui: &egui::Context,
    audio: &mut Audio,
    folder: Option<&Path>,
    open: &mut bool,
) -> Option<String> {
    if !*open {
        return None;
    }
    let mut display = DISPLAY.get();
    let mut tab = TAB.get();
    let mut language = None;
    let mut set = false;
    let mut close = false;
    let modal = egui::Modal::new(egui::Id::new("options")).show(ui, |ui| {
        ui.set_width(WIDTH);
        ui.heading(tr!("options"));
        ui.horizontal(|ui| {
            ui.selectable_value(&mut tab, Tab::Graphics, tr!("options-graphics"));
            ui.selectable_value(&mut tab, Tab::Audio, tr!("options-audio"));
            ui.selectable_value(&mut tab, Tab::Language, tr!("options-language"));
        });
        ui.separator();
        match tab {
            Tab::Graphics => graphics_tab(ui, &mut display),
            Tab::Audio => set |= audio_tab(ui, audio),
            Tab::Language => language = language_tab(ui),
        }
        ui.separator();
        close = ui.button(tr!("close")).clicked();
    });
    if close || modal.should_close() {
        *open = false;
    }
    TAB.set(tab);
    if display != DISPLAY.get() {
        apply(display);
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

/// Full screen and the interface's size, changed in `display`.
fn graphics_tab(ui: &mut egui::Ui, display: &mut Display) {
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
}

/// The music and effects volumes, applied at once; whether a volume is to be kept.
fn audio_tab(ui: &mut egui::Ui, audio: &mut Audio) -> bool {
    let mut set = false;
    // Set when changed or let go: a slider dragged changes at every frame.
    let mut slide = |ui: &mut egui::Ui, volume: &mut f32, text: String| {
        let slider = volume_slider(ui, volume, text);
        set |= slider.drag_stopped() || (slider.changed() && !slider.dragged());
        slider.changed()
    };
    let mut music = audio.music_volume();
    if slide(ui, &mut music, tr!("music")) {
        audio.set_music_volume(music);
    }
    let mut effects = audio.effects_volume();
    if slide(ui, &mut effects, tr!("effects")) {
        audio.set_effects_volume(effects);
    }
    set
}

/// The language picker; the code of the language chosen, if it changed.
fn language_tab(ui: &mut egui::Ui) -> Option<&'static str> {
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
        (chosen != current).then_some(chosen)
    })
    .inner
}

/// A volume between 0 and 1, shown and typed in whole percents.
fn volume_slider(ui: &mut egui::Ui, volume: &mut f32, text: String) -> egui::Response {
    ui.add(
        egui::Slider::new(volume, 0.0..=1.0)
            .text(text)
            .step_by(0.01)
            .custom_formatter(|v, _| format!("{:.0} %", v * 100.0))
            .custom_parser(|s| {
                let percent = s.trim().trim_end_matches('%').trim();
                percent.parse::<f64>().ok().map(|p| p / 100.0)
            }),
    )
}
