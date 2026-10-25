//! The staff view.

use lanista_core::design_data::DesignData;
use lanista_core::fighter::care::CareRules;
use lanista_core::staff::{Candidate, Coaching, Medicine, Person, Role, Scouting, StaffMove};
use lanista_core::state::GameState;
use reesch::egui;

use super::{ClubScreen, chip};
use crate::lang::tr;
use crate::texts::{role_title, staff_move_text, weeks_left_text};
use crate::theme;

impl ClubScreen {
    /// The staff: this week's changes, then a card for each post, side by side: who
    /// holds it and what it brings, and the candidates on the market to hire in its
    /// place, what each brings in green where better than the holder, in red where worse.
    pub(super) fn staff(&mut self, ui: &mut egui::Ui) {
        let data = &self.data;
        let state = &self.state;
        let market = &state.staff_market;
        let chosen = &mut self.decisions.staff;
        ui.heading(tr!("view-staff"));
        ui.label(egui::RichText::new(tr!("staff-intro")).small().weak());
        ui.add_space(4.0);
        let mut cancel = None;
        if !chosen.is_empty() {
            egui::Frame::group(ui.style())
                .fill(ui.visuals().extreme_bg_color)
                .show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    let title = egui::RichText::new(tr!("market-this-week"));
                    ui.label(title.strong().color(theme::BRONZE));
                    for (n, &m) in chosen.iter().enumerate() {
                        ui.horizontal(|ui| {
                            ui.label(staff_move_text(m, market));
                            if ui.small_button(tr!("cancel")).clicked() {
                                cancel = Some(n);
                            }
                        });
                    }
                });
            ui.add_space(4.0);
        }
        if let Some(n) = cancel {
            chosen.remove(n);
        }
        egui::ScrollArea::vertical().show(ui, |ui| {
            ui.columns(Role::ALL.len(), |columns| {
                for (role, ui) in Role::ALL.into_iter().zip(columns) {
                    post_card(ui, role, state, chosen, data);
                }
            });
        });
    }
}

/// One post's card: its title and the holder's level, what the holder brings, or the
/// club without one, then each candidate for it.
fn post_card(
    ui: &mut egui::Ui,
    role: Role,
    state: &GameState,
    chosen: &mut Vec<StaffMove>,
    data: &DesignData,
) {
    let market = &state.staff_market;
    let held = state.staff.level(role);
    let effects = held_effects(role, state, data);
    egui::Frame::group(ui.style())
        .fill(ui.visuals().extreme_bg_color)
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal(|ui| {
                let title = egui::RichText::new(role_title(role)).strong();
                ui.label(title.color(theme::BRONZE));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let m = StaffMove::Dismiss(role);
                    if held.is_some() {
                        let allowed = state.may_staff(m, data);
                        staff_toggle(ui, chosen, market, m, &tr!("dismiss"), allowed);
                    }
                    level_chip(ui, held);
                });
            });
            effect_chips(ui, &effects, None);
            ui.add_space(4.0);
            ui.label(egui::RichText::new(tr!("staff-candidates")).strong());
            let mut none = true;
            for (c, candidate) in market.iter().enumerate() {
                if candidate.person.role() != role {
                    continue;
                }
                none = false;
                candidate_card(ui, c, candidate, &effects, state, chosen, data);
            }
            if none {
                ui.label(egui::RichText::new(tr!("staff-no-candidate")).weak());
            }
        });
}

/// A candidate on the market: its level, how long it stays, what it brings against
/// `held`, and the button hiring it.
fn candidate_card(
    ui: &mut egui::Ui,
    c: usize,
    candidate: &Candidate,
    held: &[Effect],
    state: &GameState,
    chosen: &mut Vec<StaffMove>,
    data: &DesignData,
) {
    let market = &state.staff_market;
    egui::Frame::group(ui.style()).show(ui, |ui| {
        ui.set_width(ui.available_width());
        ui.horizontal(|ui| {
            level_chip(ui, Some(candidate.person.level()));
            let left = egui::RichText::new(weeks_left_text(candidate.weeks_left));
            ui.label(left.small().weak());
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let m = StaffMove::Hire(c);
                let allowed = state.may_staff(m, data);
                staff_toggle(ui, chosen, market, m, &tr!("hire"), allowed);
            });
        });
        effect_chips(ui, &person_effects(&candidate.person, data), Some(held));
    });
}

/// "Niveau 3" in bronze, or « Aucun » faded.
fn level_chip(ui: &mut egui::Ui, level: Option<u32>) {
    if let Some(level) = level {
        let text = tr!("staff-level", level = level);
        chip(ui, text, theme::BRONZE.gamma_multiply(0.6));
    } else {
        let plain = ui.visuals().widgets.inactive.bg_fill;
        chip(ui, tr!("staff-nobody"), plain);
    }
}

/// One thing a staff member brings, as a tag.
struct Effect {
    text: String,
    /// The higher the better, to compare with the holder's.
    score: i64,
}

impl Effect {
    fn new(text: String, score: impl Into<i64>) -> Self {
        Self {
            text,
            score: score.into(),
        }
    }
}

/// `effects` as tags; against `held`, each green where better, red where worse.
fn effect_chips(ui: &mut egui::Ui, effects: &[Effect], held: Option<&[Effect]>) {
    let plain = ui.visuals().widgets.inactive.bg_fill;
    ui.horizontal_wrapped(|ui| {
        for (n, effect) in effects.iter().enumerate() {
            let other = held.and_then(|h| h.get(n)).map(|h| h.score);
            let color = match other.map(|o| effect.score.cmp(&o)) {
                Some(std::cmp::Ordering::Greater) => theme::LAUREL,
                Some(std::cmp::Ordering::Less) => theme::BLOOD,
                _ => plain,
            };
            chip(ui, &effect.text, color);
        }
    });
}

/// What the one holding `role` brings, or the club without one.
fn held_effects(role: Role, state: &GameState, data: &DesignData) -> Vec<Effect> {
    let staff = &state.staff;
    let care = &data.fighter.care;
    match role {
        Role::Trainer => staff.trainer().map_or_else(
            || trainer_effects(0, data.economy.training_cost, Coaching::default()),
            |t| trainer_effects(t.salary, t.training_cost, t.coaching),
        ),
        Role::Doctor => staff.doctor().map_or_else(
            || doctor_effects(0, &Medicine::default(), care),
            |d| doctor_effects(d.salary, &d.medicine, care),
        ),
        Role::Recruiter => staff.recruiter().map_or_else(
            || recruiter_effects(0, &Scouting::default()),
            |r| recruiter_effects(r.salary, &r.scouting),
        ),
    }
}

/// What a candidate brings.
fn person_effects(person: &Person, data: &DesignData) -> Vec<Effect> {
    match person {
        Person::Trainer(m) => {
            let t = &m.rules;
            trainer_effects(t.salary, t.training_cost, t.coaching)
        }
        Person::Doctor(m) => doctor_effects(m.rules.salary, &m.rules.medicine, &data.fighter.care),
        Person::Recruiter(m) => recruiter_effects(m.rules.salary, &m.rules.scouting),
    }
}

fn salary_effect(salary: u32) -> Effect {
    Effect::new(tr!("staff-salary", salary = salary), -i64::from(salary))
}

/// Salary, price of a session, progress and form spared.
fn trainer_effects(salary: u32, session: u32, coaching: Coaching) -> Vec<Effect> {
    vec![
        salary_effect(salary),
        Effect::new(tr!("staff-session", session = session), -i64::from(session)),
        Effect::new(
            tr!("staff-progress", progress = coaching.progress_percent),
            coaching.progress_percent,
        ),
        Effect::new(
            tr!("staff-form", form = coaching.form_spared),
            coaching.form_spared,
        ),
    ]
}

/// Salary, weeks care keeps, price of care and after-effects spared.
fn doctor_effects(salary: u32, medicine: &Medicine, care: &CareRules) -> Vec<Effect> {
    let kept = medicine.care_rules(care).weeks_kept_percent;
    let cost = medicine.care_cost_percent;
    let spared = medicine.after_effect_spared_percent;
    vec![
        salary_effect(salary),
        Effect::new(tr!("staff-care-weeks", kept = kept), -i64::from(kept)),
        Effect::new(tr!("staff-care-cost", cost = cost), -i64::from(cost)),
        Effect::new(tr!("staff-spared", spared = spared), spared),
    ]
}

/// Salary, stats revealed, width of the ranges, potential, personality and ease.
fn recruiter_effects(salary: u32, scouting: &Scouting) -> Vec<Effect> {
    let potential = scouting.potential_range_percent;
    // No estimate at all is worse than the widest one.
    let potential_score = if potential == 0 {
        i64::MIN
    } else {
        -i64::from(potential)
    };
    let seen = |seen: bool| if seen { "true" } else { "false" };
    vec![
        salary_effect(salary),
        Effect::new(
            tr!("staff-revealed", revealed = scouting.revealed_per_week),
            scouting.revealed_per_week,
        ),
        Effect::new(
            tr!("staff-ranges", width = scouting.range_width_percent),
            -i64::from(scouting.range_width_percent),
        ),
        Effect::new(
            tr!("staff-potential", potential = potential),
            potential_score,
        ),
        Effect::new(
            tr!("staff-personality", seen = seen(scouting.personality)),
            i64::from(scouting.personality),
        ),
        Effect::new(
            tr!("staff-ease", seen = seen(scouting.ease)),
            i64::from(scouting.ease),
        ),
    ]
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
