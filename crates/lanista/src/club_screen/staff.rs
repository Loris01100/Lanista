//! The staff view.

use lanista_core::design_data::DesignData;
use lanista_core::staff::{Candidate, Person, Role, Staff, StaffMove};
use reesch::egui;

use super::ClubScreen;
use crate::lang::tr;
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
        ui.heading(tr!("view-staff"));
        ui.label(tr!("staff-intro"));
        let mut cancel = None;
        for (n, &m) in chosen.iter().enumerate() {
            ui.horizontal(|ui| {
                ui.strong(tr!("this-week", what = staff_move_text(m, market)));
                if ui.button(tr!("cancel")).clicked() {
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
                        Some(text) => {
                            ui.label(tr!("staff-held", level = held.unwrap_or(0), text = text))
                        }
                        None if role == Role::Trainer => ui.label(tr!(
                            "staff-no-trainer",
                            session = data.economy.training_cost
                        )),
                        None => ui.label(tr!("staff-nobody")),
                    };
                    if held.is_some() {
                        let m = StaffMove::Dismiss(role);
                        staff_toggle(
                            ui,
                            chosen,
                            market,
                            m,
                            &tr!("dismiss"),
                            state.may_staff(m, data),
                        );
                    }
                });
                let candidates: Vec<usize> = (0..market.len())
                    .filter(|&c| market[c].person.role() == role)
                    .collect();
                if candidates.is_empty() {
                    ui.label(tr!("staff-no-candidate"));
                    continue;
                }
                egui::Grid::new(("staff", role_title(role)))
                    .striped(true)
                    .show(ui, |ui| {
                        for c in candidates {
                            let candidate = &market[c];
                            ui.label(tr!("staff-level", level = candidate.person.level()));
                            ui.label(person_text(&candidate.person, data));
                            ui.label(weeks_left_text(candidate.weeks_left));
                            let m = StaffMove::Hire(c);
                            staff_toggle(
                                ui,
                                chosen,
                                market,
                                m,
                                &tr!("hire"),
                                state.may_staff(m, data),
                            );
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
