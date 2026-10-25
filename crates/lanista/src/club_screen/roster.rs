//! The fighter list: a card per fighter in activity, the former ones below.

use lanista_core::design_data::DesignData;
use lanista_core::fighter::career::{Activity, CareerFighter, Status, WeekPlan};
use lanista_core::fighter::fame::Fame;
use lanista_core::fighter::form::Form;
use lanista_core::fighter::morale::Morale;
use lanista_core::state::GameState;
use reesch::egui;

use super::{ClubScreen, View, chip, meter, settle_plan, wrapped_chip};
use crate::lang::tr;
use crate::texts::{
    activity_name, class_name, condition_text, illness_text, injury_text, loadout_name,
    record_text, stage_name, unsellable_text,
};
use crate::theme;

/// Width a card needs: the bars and their names, the tags beside the name.
const CARD_WIDTH: f32 = 420.0;

impl ClubScreen {
    /// A card per fighter in activity, side by side as the width allows: its name,
    /// class and age, its form, morale and fame as bars, its health and its week as
    /// tags; the former fighters below, folded.
    pub(super) fn roster(&mut self, ui: &mut egui::Ui) {
        ui.heading(tr!("view-roster"));
        ui.label(egui::RichText::new(tr!("roster-intro")).small().weak());
        ui.add_space(4.0);
        let data = &self.data;
        let state = &self.state;
        let care = state.care_rules(data);
        let special: Vec<_> = (0..state.fighters.len())
            .map(|i| self.special_fights(i))
            .collect();
        let mut cards: Vec<_> = state
            .fighters
            .iter()
            .zip(&mut self.decisions.plans)
            .enumerate()
            .filter(|(_, (f, _))| f.status == Status::Active)
            .collect();
        let width = ui.available_width();
        let per_row = if width >= 3.0 * CARD_WIDTH {
            3
        } else if width >= 2.0 * CARD_WIDTH {
            2
        } else {
            1
        };
        let mut goto = None;
        egui::ScrollArea::vertical().show(ui, |ui| {
            for row in cards.chunks_mut(per_row) {
                ui.columns(per_row, |columns| {
                    for (ui, (i, (f, plan))) in columns.iter_mut().zip(row.iter_mut()) {
                        settle_plan(plan, f, &special[*i], (state, &care), data);
                        if fighter_card(ui, *i, f, plan, state, data) {
                            goto = Some(View::Sheet(*i));
                        }
                    }
                });
                ui.add_space(6.0);
            }
            if let Some(i) = former(ui, state, data) {
                goto = Some(View::Sheet(i));
            }
        });
        if let Some(view) = goto {
            self.view = view;
        }
    }
}

/// "Flamma", or "n° 2" for a fighter without a name.
fn short_name(index: usize, fighter: &CareerFighter) -> String {
    if fighter.name.is_empty() {
        tr!("ranked-unnamed", number = index + 1)
    } else {
        stage_name(&fighter.name)
    }
}

/// One fighter's card; true when its name is clicked, to open its sheet.
fn fighter_card(
    ui: &mut egui::Ui,
    index: usize,
    fighter: &CareerFighter,
    plan: &WeekPlan,
    state: &GameState,
    data: &DesignData,
) -> bool {
    let mut open = false;
    egui::Frame::group(ui.style())
        .fill(ui.visuals().extreme_bg_color)
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal(|ui| {
                let name = egui::RichText::new(short_name(index, fighter)).strong();
                open = ui.link(name.color(theme::BRONZE)).clicked();
                let class = ui.visuals().widgets.inactive.bg_fill;
                chip(ui, class_name(fighter, data), class);
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let age = tr!("roster-age-years", age = fighter.age);
                    ui.label(egui::RichText::new(age).weak());
                    ui.label(egui::RichText::new(record_text(fighter.record)).weak());
                });
            });
            ui.add_space(2.0);
            bars(ui, index, fighter);
            ui.add_space(2.0);
            health_chips(ui, fighter, state, data);
            week_chips(ui, fighter, plan, data);
        });
    open
}

/// Form, morale and fame as bars, form and morale in red when low.
fn bars(ui: &mut egui::Ui, index: usize, fighter: &CareerFighter) {
    let level = |value: u32| match value {
        60.. => theme::LAUREL,
        30.. => theme::BRONZE,
        _ => theme::BLOOD,
    };
    let form = fighter.form.value();
    let morale = fighter.morale.value();
    let fame = fighter.fame.value();
    egui::Grid::new(("roster-bars", index)).show(ui, |ui| {
        for (name, value, max, color) in [
            (tr!("roster-form"), form, Form::MAX, level(form)),
            (tr!("roster-morale"), morale, Morale::MAX, level(morale)),
            (tr!("roster-fame"), fame, Fame::MAX, theme::BRONZE),
        ] {
            ui.label(name);
            meter(ui, value, max, color, None);
            ui.label(value.to_string());
            ui.end_row();
        }
    });
}

/// Its health as tags: each injury healing in red, each after-effect plain, an
/// illness in bronze, or "En forme" in green; and whether it is too new to be sold.
fn health_chips(ui: &mut egui::Ui, fighter: &CareerFighter, state: &GameState, data: &DesignData) {
    let plain = ui.visuals().widgets.inactive.bg_fill;
    ui.horizontal_wrapped(|ui| {
        let mut fit = true;
        for &injury in &fighter.injuries {
            let text = injury_text(injury, fighter.dominant_hand);
            if injury.is_permanent() {
                wrapped_chip(ui, text, plain);
            } else {
                fit = false;
                wrapped_chip(ui, text, theme::BLOOD);
            }
        }
        if let Some(illness) = fighter.illness {
            fit = false;
            wrapped_chip(ui, illness_text(illness), theme::BRONZE.gamma_multiply(0.6));
        }
        if fit {
            wrapped_chip(ui, tr!("roster-fit"), theme::LAUREL);
        }
        if let Some(unsellable) = unsellable_text(fighter, state, data) {
            wrapped_chip(ui, tr!("roster-recent"), plain).on_hover_text(unsellable);
        }
    });
}

/// Its week as tags: the activity, in red for a fight, the loadout and its ease with
/// it, care and the armour left.
fn week_chips(ui: &mut egui::Ui, fighter: &CareerFighter, plan: &WeekPlan, data: &DesignData) {
    let neutral = ui.visuals().widgets.inactive.bg_fill;
    ui.horizontal_wrapped(|ui| {
        let color = match plan.activity {
            Activity::Rest => theme::SEA,
            Activity::Train => theme::BRONZE.gamma_multiply(0.6),
            _ => theme::BLOOD,
        };
        wrapped_chip(ui, activity_name(plan.activity, data), color);
        let loadout = tr!(
            "roster-loadout",
            loadout = loadout_name(fighter, plan.loadout, data),
            ease = fighter.ease_with(plan.loadout)
        );
        wrapped_chip(ui, loadout, neutral).on_hover_text(tr!("roster-ease-hint"));
        if plan.care {
            wrapped_chip(ui, tr!("roster-care"), theme::LAUREL);
        }
        if plan.refused.armour {
            wrapped_chip(ui, tr!("roster-no-armour"), neutral);
        }
    });
}

/// The fighters no longer in activity, folded: dead, retired, sold or released;
/// returns the one whose name is clicked.
fn former(ui: &mut egui::Ui, state: &GameState, data: &DesignData) -> Option<usize> {
    let gone: Vec<_> = state
        .fighters
        .iter()
        .enumerate()
        .filter(|(_, f)| f.status != Status::Active)
        .collect();
    if gone.is_empty() {
        return None;
    }
    let mut open = None;
    let title = tr!("roster-former", count = gone.len());
    egui::CollapsingHeader::new(title)
        .id_salt("roster-former")
        .show(ui, |ui| {
            egui::Grid::new("roster-former")
                .striped(true)
                .show(ui, |ui| {
                    let plain = ui.visuals().widgets.inactive.bg_fill;
                    for (i, f) in gone {
                        if ui.link(short_name(i, f)).clicked() {
                            open = Some(i);
                        }
                        ui.label(egui::RichText::new(class_name(f, data)).weak());
                        chip(ui, condition_text(f), plain);
                        ui.label(record_text(f.record));
                        ui.end_row();
                    }
                });
        });
    open
}
