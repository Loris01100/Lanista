//! The staff view.

use lanista_core::design_data::DesignData;
use lanista_core::staff::{Role, StaffMove};
use reesch::egui;

use super::ClubScreen;
use crate::texts::{doctor_text, recruiter_text, role_title, staff_move_text, trainer_text};

impl ClubScreen {
    /// The staff: for each post, who holds it, and each level to hire in its place.
    pub(super) fn staff(&mut self, ui: &mut egui::Ui) {
        let data = &self.data;
        let state = &self.state;
        let chosen = &mut self.decisions.staff;
        ui.heading("Personnel");
        ui.label(
            "L'entraîneur rend l'entraînement plus profitable et moins fatigant, le médecin \
             rend les soins plus efficaces et évite des séquelles, le recruteur montre mieux \
             les combattants observés. Chaque salaire est payé chaque semaine, même à \
             crédit ; un changement compte dès la semaine qui se joue.",
        );
        let mut cancel = None;
        for (n, &m) in chosen.iter().enumerate() {
            ui.horizontal(|ui| {
                ui.strong(format!("Cette semaine : {}", staff_move_text(m)));
                if ui.button("Annuler").clicked() {
                    cancel = Some(n);
                }
            });
        }
        if let Some(n) = cancel {
            chosen.remove(n);
        }
        egui::ScrollArea::vertical().show(ui, |ui| {
            for role in Role::ALL {
                ui.add_space(6.0);
                ui.strong(role_title(role));
                let held = state.staff.level(role);
                match held.and_then(|level| level_text(role, level, data)) {
                    Some(text) => ui.label(format!("Niveau {} : {text}", held.unwrap_or(0))),
                    None if role == Role::Trainer => ui.label(format!(
                        "Aucun : séance {}, sans bonus",
                        data.economy.training_cost
                    )),
                    None => ui.label("Aucun"),
                };
                egui::Grid::new(("staff", role_title(role)))
                    .striped(true)
                    .show(ui, |ui| {
                        for level in (1..).take(data.economy.staff.levels(role)) {
                            ui.label(format!("Niveau {level}"));
                            ui.label(level_text(role, level, data).unwrap_or_default());
                            let (m, label) = if held == Some(level) {
                                (StaffMove::Dismiss(role), "Renvoyer")
                            } else {
                                (StaffMove::Hire(role, level), "Engager")
                            };
                            staff_toggle(ui, chosen, m, label, state.may_staff(m, data));
                            ui.end_row();
                        }
                    });
            }
        });
    }
}

/// What `level` of `role` costs and brings; `None` for a level not on offer.
fn level_text(role: Role, level: u32, data: &DesignData) -> Option<String> {
    let staff = &data.economy.staff;
    match role {
        Role::Trainer => staff.trainer(level).map(trainer_text),
        Role::Doctor => staff
            .doctor(level)
            .map(|d| doctor_text(d, &data.fighter.care)),
        Role::Recruiter => staff.recruiter(level).map(recruiter_text),
    }
}

/// A button choosing `m` as the week's change for its post, in place of another, or
/// taking it back when chosen already; only a chosen one may be clicked when not `enabled`.
fn staff_toggle(
    ui: &mut egui::Ui,
    chosen: &mut Vec<StaffMove>,
    m: StaffMove,
    label: &str,
    enabled: bool,
) {
    let selected = chosen.contains(&m);
    let button = egui::SelectableLabel::new(selected, label);
    if ui.add_enabled(enabled || selected, button).clicked() {
        chosen.retain(|other| other.role() != m.role());
        if !selected {
            chosen.push(m);
        }
    }
}
