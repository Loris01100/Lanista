//! The staff view.

use lanista_core::design_data::DesignData;
use lanista_core::staff::{Candidate, Person, Role, Staff, StaffMove};
use reesch::egui;

use super::ClubScreen;
use crate::texts::{
    doctor_text, recruiter_text, role_title, staff_move_text, trainer_text, weeks_left_text,
};

impl ClubScreen {
    /// The staff: for each post, who holds it, and the candidates on the market to hire
    /// in its place.
    pub(super) fn staff(&mut self, ui: &mut egui::Ui) {
        let data = &self.data;
        let state = &self.state;
        let market = &state.staff_market;
        let chosen = &mut self.decisions.staff;
        ui.heading("Personnel");
        ui.label(
            "L'entraîneur rend l'entraînement plus profitable et moins fatigant, le médecin \
             rend les soins plus efficaces et évite des séquelles, le recruteur montre mieux \
             les combattants observés. Les candidats arrivent et partent du marché, chacun \
             avec ses stats et son salaire, et les clubs rivaux y engagent aussi. Chaque \
             salaire est payé chaque semaine, même à crédit ; un changement compte dès la \
             semaine qui se joue.",
        );
        let mut cancel = None;
        for (n, &m) in chosen.iter().enumerate() {
            ui.horizontal(|ui| {
                ui.strong(format!("Cette semaine : {}", staff_move_text(m, market)));
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
                ui.horizontal(|ui| {
                    match held_text(role, &state.staff, data) {
                        Some(text) => ui.label(format!("Niveau {} : {text}", held.unwrap_or(0))),
                        None if role == Role::Trainer => ui.label(format!(
                            "Aucun : séance {}, sans bonus",
                            data.economy.training_cost
                        )),
                        None => ui.label("Aucun"),
                    };
                    if held.is_some() {
                        let m = StaffMove::Dismiss(role);
                        staff_toggle(ui, chosen, market, m, "Renvoyer", state.may_staff(m));
                    }
                });
                let candidates: Vec<usize> = (0..market.len())
                    .filter(|&c| market[c].person.role() == role)
                    .collect();
                if candidates.is_empty() {
                    ui.label("Aucun candidat sur le marché");
                    continue;
                }
                egui::Grid::new(("staff", role_title(role)))
                    .striped(true)
                    .show(ui, |ui| {
                        for c in candidates {
                            let candidate = &market[c];
                            ui.label(format!("Niveau {}", candidate.person.level()));
                            ui.label(person_text(&candidate.person, data));
                            ui.label(weeks_left_text(candidate.weeks_left));
                            let m = StaffMove::Hire(c);
                            staff_toggle(ui, chosen, market, m, "Engager", state.may_staff(m));
                            ui.end_row();
                        }
                    });
            }
        });
    }
}

/// What the one holding `role` costs and brings; `None` when no one does.
fn held_text(role: Role, staff: &Staff, data: &DesignData) -> Option<String> {
    match role {
        Role::Trainer => staff.trainer().map(trainer_text),
        Role::Doctor => staff.doctor().map(|d| doctor_text(d, &data.fighter.care)),
        Role::Recruiter => staff.recruiter().map(recruiter_text),
    }
}

/// What a candidate costs and brings.
fn person_text(person: &Person, data: &DesignData) -> String {
    match person {
        Person::Trainer(m) => trainer_text(&m.rules),
        Person::Doctor(m) => doctor_text(&m.rules, &data.fighter.care),
        Person::Recruiter(m) => recruiter_text(&m.rules),
    }
}

/// A button choosing `m` as the week's change for its post, in place of another, or
/// taking it back when chosen already; only a chosen one may be clicked when not `enabled`.
fn staff_toggle(
    ui: &mut egui::Ui,
    chosen: &mut Vec<StaffMove>,
    market: &[Candidate],
    m: StaffMove,
    label: &str,
    enabled: bool,
) {
    let selected = chosen.contains(&m);
    let button = egui::SelectableLabel::new(selected, label);
    if ui.add_enabled(enabled || selected, button).clicked() {
        chosen.retain(|other| other.role(market) != m.role(market));
        if !selected {
            chosen.push(m);
        }
    }
}
