//! A career followed week after week: the fighter's sheet, the week's decisions
//! (activity, care) and what each week changed. Fight weeks are watched in the
//! combat screen; the same seed and decisions give the same career as the `career` tool.

use std::rc::Rc;

use lanista_core::calendar::SeasonPhase;
use lanista_core::combat::Arena;
use lanista_core::design_data::DesignData;
use lanista_core::fighter::career::{
    Activity, CareerFighter, FightSummary, Status, WeekPlan, WeekReport, suggested_plan,
};
use lanista_core::fighter::fame::{Fame, FightResult};
use lanista_core::fighter::form::Form;
use lanista_core::fighter::illness::{Illness, IllnessLevel};
use lanista_core::fighter::injury::{Injury, Location, Severity, Side};
use lanista_core::fighter::morale::Morale;
use lanista_core::state::{Decisions, GameState, next_week};
use macroquad::prelude::*;
use reesch::{Assets, Audio, Bindings, Screen, Transition, egui};

use crate::Action;
use crate::combat_screen::CombatScreen;

/// Seed of the standard duel offered from the career screen.
const FREE_DUEL_SEED: u64 = 42;
const BACKGROUND: Color = Color::new(0.16, 0.13, 0.11, 1.0);
/// Stat names, in `Stats::to_array` order.
const STAT_NAMES: [&str; 6] = ["PV", "AC", "AL", "VIT", "DC", "DL"];

pub struct CareerScreen {
    data: Rc<DesignData>,
    seed: u64,
    state: GameState,
    /// This week's decisions, the suggested ones until the player changes them.
    plan: WeekPlan,
    /// What each week changed, newest first.
    journal: Vec<(u64, Vec<String>)>,
    bindings: Bindings<Action>,
}

impl CareerScreen {
    /// The career of the fighter generated from `seed`, as `career --seed <seed>`.
    pub fn new(data: Rc<DesignData>, seed: u64, bindings: Bindings<Action>) -> Self {
        let state = GameState::new(seed, &data);
        let plan = suggested_plan(&state.fighters[0], state.week);
        Self {
            data,
            seed,
            state,
            plan,
            journal: Vec::new(),
            bindings,
        }
    }

    /// Plays the week with the current plan and logs it; returns the fight to
    /// watch, if any. Nothing happens once the career is over.
    fn next_week(&mut self) -> Option<Arena> {
        if self.fighter().status != Status::Active {
            return None;
        }
        let before = self.fighter().clone();
        let week = self.state.week;
        let decisions = Decisions {
            plans: vec![self.plan],
        };
        let report = next_week(&mut self.state, &decisions, &self.data)
            .swap_remove(0)
            .expect("an active fighter plays the week");
        let lines = week_lines(&before, self.fighter(), &report, &self.data);
        self.journal.insert(0, (week, lines));
        self.plan = suggested_plan(self.fighter(), self.state.week);
        report.fight.map(|fight| fight.start)
    }

    /// The club's only fighter, followed by this screen.
    fn fighter(&self) -> &CareerFighter {
        &self.state.fighters[0]
    }

    /// Total price of caring for everything care would shorten.
    fn care_cost(&self) -> u32 {
        let care = &self.data.fighter.care;
        let injuries = self
            .fighter()
            .injuries
            .iter()
            .filter_map(|i| i.care_cost(care));
        let illness = self.fighter().illness.and_then(|i| i.care_cost(care));
        injuries.chain(illness).sum()
    }

    fn sheet(&self, ui: &mut egui::Ui) {
        let f = self.fighter();
        let rules = &self.data.fighter;
        ui.heading(format!("{}, {} ans", class_name(f, &self.data), f.age));
        let hand = match f.dominant_hand {
            Side::Right => "droitier",
            Side::Left => "gaucher",
        };
        ui.label(format!("Carrière n° {}, {hand}", self.seed));
        match f.status {
            Status::Active => {}
            Status::Retired => {
                ui.colored_label(egui::Color32::GOLD, "A pris sa retraite");
            }
            Status::Dead => {
                ui.colored_label(egui::Color32::LIGHT_RED, "Mort au combat");
            }
        }
        ui.add_space(8.0);
        gauge(ui, "Forme", f.form.value(), Form::MAX);
        gauge(ui, "Moral", f.morale.value(), Morale::MAX);
        gauge(ui, "Renommée", f.fame.value(), Fame::MAX);

        ui.add_space(8.0);
        egui::Grid::new("stats").striped(true).show(ui, |ui| {
            ui.label("");
            ui.label("Entraîné");
            ui.label("En combat");
            ui.end_row();
            let fighting = f.fighting_stats(rules).to_array();
            for ((name, trained), now) in STAT_NAMES.iter().zip(f.stats.to_array()).zip(fighting) {
                ui.label(*name);
                ui.label(trained.to_string());
                ui.label(now.to_string());
                ui.end_row();
            }
        });

        ui.add_space(8.0);
        let p = &f.personality;
        ui.label(format!(
            "Agressivité {}, prudence {}, goût du spectacle {}, détermination {}",
            p.aggression.value(),
            p.caution.value(),
            p.showmanship.value(),
            p.determination.value()
        ));
        ui.add_space(8.0);
        ui.strong("Blessures");
        if f.injuries.is_empty() {
            ui.label("Aucune");
        }
        for injury in &f.injuries {
            ui.label(injury_text(*injury, f.dominant_hand));
        }
        ui.strong("Maladie");
        ui.label(f.illness.map_or_else(|| "Aucune".into(), illness_text));
    }

    /// The week's decisions; returns whether to play the week.
    fn decisions(&mut self, ui: &mut egui::Ui) -> bool {
        let date = self.state.date(&self.data);
        let phase = match date.phase(&self.data.calendar) {
            SeasonPhase::Competition => "compétition",
            SeasonPhase::GreatEvent => "grand événement",
            SeasonPhase::OffSeason => "intersaison",
        };
        ui.heading(format!(
            "Saison {}, semaine {} : {phase}",
            date.season, date.week
        ));
        if self.fighter().status != Status::Active {
            ui.label("La carrière est terminée.");
            return false;
        }
        ui.horizontal(|ui| {
            for activity in [Activity::Rest, Activity::Train, Activity::Fight] {
                ui.selectable_value(&mut self.plan.activity, activity, activity_name(activity));
            }
        });
        let cost = self.care_cost();
        if cost == 0 {
            self.plan.care = false;
            ui.add_enabled(
                false,
                egui::Checkbox::new(&mut self.plan.care, "Rien à soigner"),
            );
        } else {
            ui.checkbox(&mut self.plan.care, format!("Soigner (coût {cost})"));
        }
        let ill = self.fighter().illness.is_some_and(|i| !i.fight_advised());
        if ill && self.plan.activity == Activity::Fight {
            ui.colored_label(
                egui::Color32::LIGHT_RED,
                "Combat déconseillé : la maladie peut s'aggraver",
            );
        }
        ui.button("Semaine suivante (Espace)").clicked()
    }
}

impl Screen for CareerScreen {
    fn update(&mut self, ui: &egui::Context, _assets: &Assets, _audio: &mut Audio) -> Transition {
        if self.bindings.pressed(ui, &Action::Quit) {
            return Transition::Pop;
        }
        let mut play = self.bindings.pressed(ui, &Action::NextWeek);
        let mut free_duel = false;
        egui::SidePanel::left("sheet")
            .resizable(false)
            .exact_width(430.0)
            .show(ui, |ui| self.sheet(ui));
        egui::CentralPanel::default().show(ui, |ui| {
            play |= self.decisions(ui);
            ui.separator();
            egui::ScrollArea::vertical()
                .max_height(ui.available_height() - 50.0)
                .show(ui, |ui| {
                    for (week, lines) in &self.journal {
                        ui.strong(format!("Semaine {week}"));
                        for line in lines {
                            ui.label(line);
                        }
                        ui.add_space(6.0);
                    }
                });
            ui.separator();
            free_duel = ui.button("Duel standard").clicked();
        });
        if free_duel {
            let duel =
                CombatScreen::new(Rc::clone(&self.data), FREE_DUEL_SEED, self.bindings.clone());
            return Transition::Push(Box::new(duel));
        }
        if play && let Some(fight) = self.next_week() {
            let screen = CombatScreen::watch(Rc::clone(&self.data), fight, self.bindings.clone());
            return Transition::Push(Box::new(screen));
        }
        Transition::None
    }

    fn draw(&self, _assets: &Assets) {
        clear_background(BACKGROUND);
    }
}

fn gauge(ui: &mut egui::Ui, name: &str, value: u32, max: u32) {
    let fraction = f32::from(u16::try_from(value).unwrap_or(u16::MAX))
        / f32::from(u16::try_from(max.max(1)).unwrap_or(u16::MAX));
    ui.add(egui::ProgressBar::new(fraction).text(format!("{name} {value}")));
}

fn class_name(fighter: &CareerFighter, data: &DesignData) -> String {
    data.class(fighter.class)
        .map_or_else(|| "?".into(), |c| c.name.clone())
}

fn activity_name(activity: Activity) -> &'static str {
    match activity {
        Activity::Rest => "Repos",
        Activity::Train => "Entraînement",
        Activity::Fight => "Combat",
    }
}

fn location_name(location: Location, dominant: Side) -> String {
    let side = |side: Side, feminine: bool| match (side, feminine) {
        (Side::Left, _) => "gauche",
        (Side::Right, false) => "droit",
        (Side::Right, true) => "droite",
    };
    match location {
        Location::Head => "Tête".into(),
        Location::Body => "Corps".into(),
        Location::Arm(s) if s == dominant => format!("Bras {} (dominant)", side(s, false)),
        Location::Arm(s) => format!("Bras {}", side(s, false)),
        Location::Leg(s) => format!("Jambe {}", side(s, true)),
    }
}

fn weeks(n: u32) -> String {
    if n == 1 {
        "1 semaine".into()
    } else {
        format!("{n} semaines")
    }
}

fn injury_text(injury: Injury, dominant: Side) -> String {
    let location = location_name(injury.location, dominant);
    let severity = match injury.severity {
        Severity::Light => "légère",
        Severity::Medium => "moyenne",
        Severity::Severe => "grave",
        Severity::Permanent => return format!("{location} : séquelle définitive"),
    };
    format!("{location} : {severity}, {}", weeks(injury.weeks_left))
}

fn illness_text(illness: Illness) -> String {
    let level = match illness.level {
        IllnessLevel::Mild => "faible",
        IllnessLevel::Moderate => "moyenne",
        IllnessLevel::Severe => "sévère",
    };
    format!("Maladie {level}, {}", weeks(illness.weeks_left))
}

fn fight_text(fight: &FightSummary, data: &DesignData) -> String {
    let result = match (fight.result, fight.died) {
        (_, true) => "mort au combat",
        (FightResult::Won, _) => "victoire",
        (FightResult::Lost, _) => "défaite",
        (FightResult::GaveUp, _) => "abandon",
        (FightResult::Unfinished, _) => "pas de vainqueur après une minute",
    };
    let stronger = if fight.against_stronger {
        ", plus fort"
    } else {
        ""
    };
    format!(
        "Contre un {}{stronger} : {result}, renommée {:+}",
        class_name(&fight.opponent, data),
        fight.fame_change
    )
}

/// `name after (±difference)`, or `name value` when it did not change. Plain ASCII
/// signs: the theme's font has no arrow or minus sign.
fn change(name: &str, before: u32, after: u32) -> String {
    if before == after {
        format!("{name} {after}")
    } else {
        format!(
            "{name} {after} ({:+})",
            i64::from(after) - i64::from(before)
        )
    }
}

/// What a week changed, in the player's language.
fn week_lines(
    before: &CareerFighter,
    after: &CareerFighter,
    report: &WeekReport,
    data: &DesignData,
) -> Vec<String> {
    let dominant = after.dominant_hand;
    let mut lines = vec![activity_name(report.plan.activity).to_owned()];
    if report.care_cost > 0 {
        lines.push(format!("Soins : {}", report.care_cost));
    }
    if let Some(fight) = &report.fight {
        lines.push(fight_text(fight, data));
    }
    if let Some(injury) = &report.new_injury {
        lines.push(format!("Blessure : {}", injury_text(*injury, dominant)));
    }
    for injury in &report.healed_injuries {
        lines.push(format!(
            "Guéri : {}",
            location_name(injury.location, dominant)
        ));
    }
    if let Some(illness) = &report.new_illness {
        lines.push(format!("Tombe malade : {}", illness_text(*illness)));
    }
    if report.illness_healed {
        lines.push("Maladie guérie".into());
    }
    if report.birthday {
        lines.push(format!("Un an de plus : {} ans", after.age));
    }
    lines.push(format!(
        "{}, {}, {}",
        change("Forme", before.form.value(), after.form.value()),
        change("moral", before.morale.value(), after.morale.value()),
        change("renommée", before.fame.value(), after.fame.value())
    ));
    let stats: Vec<String> = STAT_NAMES
        .iter()
        .zip(
            before
                .stats
                .to_array()
                .into_iter()
                .zip(after.stats.to_array()),
        )
        .filter(|(_, (b, a))| a != b)
        .map(|(name, (b, a))| format!("{name} {:+}", i64::from(a) - i64::from(b)))
        .collect();
    if !stats.is_empty() {
        lines.push(format!("Stats : {}", stats.join(", ")));
    }
    match after.status {
        Status::Active | Status::Dead => {}
        Status::Retired => lines.push(format!("Prend sa retraite à {} ans", after.age)),
    }
    lines
}

#[cfg(test)]
mod tests {
    use lanista_core::combat::Vec2;
    use lanista_core::duel::play;
    use lanista_core::fighter::FighterId;
    use rand_chacha::ChaCha8Rng;
    use rand_chacha::rand_core::SeedableRng;

    use super::*;

    fn screen(seed: u64) -> CareerScreen {
        CareerScreen::new(
            Rc::new(DesignData::load().unwrap()),
            seed,
            Bindings::new(Vec::new()),
        )
    }

    /// What `career --seed <seed> --weeks <weeks>` plays.
    fn tool_career(seed: u64, weeks: u64) -> CareerFighter {
        let data = DesignData::load().unwrap();
        let mut rng = ChaCha8Rng::seed_from_u64(seed);
        let mut fighter = CareerFighter::random(&mut rng, &data);
        for week in 1..=weeks {
            let plan = suggested_plan(&fighter, week);
            if fighter.play_week(plan, &mut rng, &data).is_none() {
                break;
            }
        }
        fighter
    }

    #[test]
    fn ten_weeks_on_screen_are_the_tool_s_ten_weeks() {
        for seed in [7, 0, 1, 42] {
            let mut screen = screen(seed);
            for _ in 0..10 {
                screen.next_week();
            }
            assert_eq!(*screen.fighter(), tool_career(seed, 10), "seed {seed}");
            assert_eq!(screen.state.week, 11);
        }
    }

    #[test]
    fn a_fight_week_hands_over_the_fight_to_watch() {
        let mut screen = screen(7);
        // Seed 7's first week is a fight.
        assert_eq!(screen.plan.activity, Activity::Fight);
        let mut fight = screen.next_week().unwrap();
        assert_eq!(fight.tick, 0);
        assert_eq!(fight.fighters[0].stats, {
            let mut rng = ChaCha8Rng::seed_from_u64(7);
            let f = CareerFighter::random(&mut rng, &screen.data);
            f.combat_fighter(FighterId(0), &screen.data, Vec2::new(3.0, 10.0))
                .stats
        });
        play(&mut fight);
        assert!(fight.is_over());
        // Week 2 is training: nothing to watch.
        assert_eq!(screen.plan.activity, Activity::Train);
        assert!(screen.next_week().is_none());
    }

    #[test]
    fn the_journal_tells_each_week_newest_first() {
        let mut screen = screen(7);
        screen.next_week();
        screen.next_week();
        let weeks: Vec<u64> = screen.journal.iter().map(|(week, _)| *week).collect();
        assert_eq!(weeks, [2, 1]);
        let first = &screen.journal[1].1;
        assert_eq!(first[0], "Combat");
        assert!(
            first[1].starts_with("Contre un Augure, plus fort : abandon"),
            "{first:?}"
        );
        assert!(
            first.contains(&"Forme 80 (-20), moral 40 (-10), renommée 0".to_owned()),
            "{first:?}"
        );
    }

    #[test]
    fn the_player_can_change_the_plan() {
        let mut screen = screen(7);
        screen.plan = WeekPlan {
            activity: Activity::Rest,
            care: false,
        };
        assert!(screen.next_week().is_none());
        assert_eq!(screen.journal[0].1[0], "Repos");
        assert_eq!(screen.fighter().form, Form::full());
    }

    #[test]
    fn a_finished_career_plays_no_more_weeks() {
        let mut screen = screen(7);
        screen.state.fighters[0].status = Status::Retired;
        assert!(screen.next_week().is_none());
        assert_eq!(screen.state.week, 1);
        assert_eq!(screen.journal, []);
    }

    #[test]
    fn injuries_read_in_french() {
        let leg = Injury {
            location: Location::Leg(Side::Right),
            severity: Severity::Light,
            weeks_left: 1,
        };
        assert_eq!(
            injury_text(leg, Side::Right),
            "Jambe droite : légère, 1 semaine"
        );
        let arm = Injury {
            location: Location::Arm(Side::Left),
            severity: Severity::Permanent,
            weeks_left: 0,
        };
        assert_eq!(
            injury_text(arm, Side::Left),
            "Bras gauche (dominant) : séquelle définitive"
        );
        assert_eq!(
            location_name(Location::Arm(Side::Right), Side::Left),
            "Bras droit"
        );
    }
}
