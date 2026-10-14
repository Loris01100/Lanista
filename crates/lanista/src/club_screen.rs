//! The player's club, week after week: the general menu, the fighter list, a
//! fighter's sheet and the summary of the week's fights. Fights are watched in the
//! combat screen; the same seed and decisions give the same game as the `career` tool.
//! The game is saved after every week, and when the player asks.

use std::path::PathBuf;
use std::rc::Rc;

use lanista_core::calendar::SeasonPhase;
use lanista_core::combat::Arena;
use lanista_core::design_data::DesignData;
use lanista_core::economy::Flow;
use lanista_core::fighter::career::{Activity, CareerFighter, FightSummary, Status, WeekPlan};
use lanista_core::fighter::fame::Fame;
use lanista_core::fighter::form::Form;
use lanista_core::fighter::illness::IllnessLevel;
use lanista_core::fighter::injury::{Injury, Severity, Side};
use lanista_core::fighter::morale::Morale;
use lanista_core::state::{Decisions, GameState, next_week};
use macroquad::prelude::*;
use reesch::{Assets, Audio, Bindings, Screen, Transition, egui};

use crate::Action;
use crate::combat_screen::CombatScreen;
use crate::saves;
use crate::texts::{
    STAT_NAMES, activity_name, change, class_name, condition_text, fight_text, fighter_name,
    illness_text, injury_text, money_line, week_lines,
};

/// Seed of the standard duel offered from the club.
const FREE_DUEL_SEED: u64 = 42;
const BACKGROUND: Color = Color::new(0.16, 0.13, 0.11, 1.0);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum View {
    /// The general menu: date, money, alerts, last week, « Semaine suivante ».
    Menu,
    /// The fighter list, each fighter's week decided in one click.
    Roster,
    /// The sheet of the fighter at this index of the club.
    Sheet(usize),
    /// The summary of last week's fights.
    Fights,
}

/// One of the club's fights, as the fight summary shows it.
#[derive(Debug, Clone, PartialEq)]
struct FightReport {
    /// Index of the club's fighter.
    fighter: usize,
    fight: FightSummary,
    /// Entry fee, receipts and prize.
    money: i64,
    /// Form before and after the week.
    form: (u32, u32),
    injury: Option<Injury>,
}

pub struct ClubScreen {
    data: Rc<DesignData>,
    state: GameState,
    /// This week's decisions, the suggested ones until the player changes them.
    decisions: Decisions,
    view: View,
    /// Each fighter's weeks, newest first, in club order.
    histories: Vec<Vec<(u64, Vec<String>)>>,
    /// The last week played: its number, the money line, then one line per fighter.
    last_week: Option<(u64, Vec<String>)>,
    /// The last week's fights.
    fights: Vec<FightReport>,
    /// Fights still to watch, the next one last.
    to_watch: Vec<Arena>,
    /// Folder of the save files; `None` saves nothing.
    saves: Option<PathBuf>,
    /// The outcome of the last save or load, for the player.
    message: Option<String>,
    bindings: Bindings<Action>,
}

impl ClubScreen {
    /// The game of the autosave in `saves`, or else a new game of `seed`.
    pub fn open(
        data: Rc<DesignData>,
        seed: u64,
        saves: Option<PathBuf>,
        bindings: Bindings<Action>,
    ) -> Self {
        let autosave = saves
            .as_deref()
            .map_or(Ok(None), |folder| saves::read(folder, saves::AUTO, &data));
        let (state, message) = match autosave {
            Ok(Some(state)) => (state, Some("Partie reprise".to_owned())),
            Ok(None) => (GameState::new(seed, &data), None),
            Err(e) => (
                GameState::new(seed, &data),
                Some(format!("Sauvegarde automatique illisible : {e}")),
            ),
        };
        let mut screen = Self::new(data, state, saves, bindings);
        screen.message = message;
        screen
    }

    /// The game `state`, saved in `saves`.
    pub fn new(
        data: Rc<DesignData>,
        state: GameState,
        saves: Option<PathBuf>,
        bindings: Bindings<Action>,
    ) -> Self {
        Self {
            decisions: state.suggested_decisions(),
            histories: vec![Vec::new(); state.fighters.len()],
            data,
            state,
            view: View::Menu,
            last_week: None,
            fights: Vec::new(),
            to_watch: Vec::new(),
            saves,
            message: None,
            bindings,
        }
    }

    /// Plays on from `state`, a loaded or new game.
    // ponytail: the week log and fight summaries are not saved, so they start
    // empty; save them once the game keeps a history in `GameState`.
    fn replace(&mut self, state: GameState) {
        let saves = self.saves.take();
        *self = Self::new(Rc::clone(&self.data), state, saves, self.bindings.clone());
    }

    /// Writes the game to `file`; tells the player only when asked or when it fails.
    fn save(&mut self, file: &str, asked: bool) {
        let Some(folder) = &self.saves else { return };
        match saves::write(folder, file, &self.state) {
            Ok(()) if asked => self.message = Some("Partie sauvegardée".into()),
            Ok(()) => {}
            Err(e) => self.message = Some(format!("Sauvegarde impossible : {e}")),
        }
    }

    fn load(&mut self) {
        let Some(folder) = &self.saves else { return };
        match saves::read(folder, saves::MANUAL, &self.data) {
            Ok(Some(state)) => {
                self.replace(state);
                self.message = Some("Partie chargée".into());
            }
            Ok(None) => self.message = Some("Aucune sauvegarde".into()),
            Err(e) => self.message = Some(format!("Sauvegarde illisible : {e}")),
        }
    }

    /// Plays the week with the current decisions and logs it; its fights are
    /// queued to be watched, then summed up.
    fn next_week(&mut self) {
        let before = self.state.fighters.clone();
        let (week, money) = (self.state.week, self.state.money);
        let summary = next_week(&mut self.state, &self.decisions, &self.data);
        let data = &self.data;
        self.histories
            .resize_with(self.state.fighters.len(), Vec::new);
        let mut overview = vec![money_line(money, self.state.money, &summary.ledger)];
        self.fights.clear();
        for (i, report) in summary.reports.into_iter().enumerate() {
            let Some(report) = report else { continue };
            let after = &self.state.fighters[i];
            // A free agent signed this week was not there before it.
            let before = before.get(i).unwrap_or(after);
            let lines = week_lines(before, after, &report, data);
            let mut line = format!("{} : {}", fighter_name(i, after, data), lines[0]);
            if let Some(fight) = &report.fight {
                line = format!("{line}, {}", fight_text(fight, data));
            }
            overview.push(line);
            self.histories[i].insert(0, (week, lines));
            if let Some(fight) = report.fight {
                let money = summary
                    .ledger
                    .iter()
                    .filter(|t| t.fighter == i)
                    .filter(|t| matches!(t.flow, Flow::EntryFee | Flow::Receipts | Flow::WinPrize))
                    .map(|t| t.amount)
                    .sum();
                self.fights.push(FightReport {
                    fighter: i,
                    fight,
                    money,
                    form: (before.form.value(), after.form.value()),
                    injury: report.new_injury,
                });
            }
        }
        self.last_week = Some((week, overview));
        self.to_watch = self
            .fights
            .iter()
            .rev()
            .map(|f| f.fight.start.clone())
            .collect();
        if !self.fights.is_empty() {
            self.view = View::Fights;
        }
        self.decisions = self.state.suggested_decisions();
        // Saved before the fights are watched: quitting during one keeps its outcome.
        self.save(saves::AUTO, false);
    }

    fn top_bar(&mut self, ui: &mut egui::Ui) -> Option<Transition> {
        let date = self.state.date(&self.data);
        let phase = match date.phase(&self.data.calendar) {
            SeasonPhase::Competition => "compétition",
            SeasonPhase::GreatEvent => "grand événement",
            SeasonPhase::OffSeason => "intersaison",
        };
        let mut transition = None;
        ui.horizontal(|ui| {
            ui.strong(format!(
                "Saison {}, semaine {} : {phase}",
                date.season, date.week
            ));
            ui.separator();
            ui.strong(format!("Argent : {}", self.state.money));
            ui.separator();
            if let Some(message) = &self.message {
                ui.label(message);
                ui.separator();
            }
            ui.selectable_value(&mut self.view, View::Menu, "Menu");
            ui.selectable_value(&mut self.view, View::Roster, "Combattants");
            ui.add_enabled_ui(!self.fights.is_empty(), |ui| {
                ui.selectable_value(&mut self.view, View::Fights, "Résumé des combats");
            });
            ui.separator();
            if ui.button("Duel standard").clicked() {
                let duel =
                    CombatScreen::new(Rc::clone(&self.data), FREE_DUEL_SEED, self.bindings.clone());
                transition = Some(Transition::Push(Box::new(duel)));
            }
        });
        transition
    }

    /// The general menu; returns whether to play the week.
    fn menu(&mut self, ui: &mut egui::Ui) -> bool {
        ui.heading("Menu général");
        let alerts = self.alerts();
        if !alerts.is_empty() {
            ui.add_space(6.0);
            ui.strong("Alertes");
            for alert in alerts {
                ui.colored_label(egui::Color32::LIGHT_RED, alert);
            }
        }
        ui.add_space(6.0);
        let active = self
            .state
            .fighters
            .iter()
            .filter(|f| f.status == Status::Active)
            .count();
        ui.label(format!("{active} combattant(s) en activité"));
        ui.checkbox(
            &mut self.decisions.hire_free_agent,
            "Recruter un free agent (gratuit) en début de semaine",
        );
        let play = ui.button("Semaine suivante (Espace)").clicked();
        if self.saves.is_some() {
            ui.horizontal(|ui| {
                if ui.button("Sauvegarder").clicked() {
                    self.save(saves::MANUAL, true);
                }
                if ui.button("Charger").clicked() {
                    self.load();
                }
                if ui.button("Nouvelle partie").clicked() {
                    let seed = new_seed();
                    self.replace(GameState::new(seed, &self.data));
                    self.message = Some(format!("Nouvelle partie, graine {seed}"));
                }
            });
        }
        ui.separator();
        if let Some((week, lines)) = &self.last_week {
            ui.strong(format!("Semaine {week}"));
            for line in lines {
                ui.label(line);
            }
        }
        play
    }

    /// What needs the player's attention before the week.
    fn alerts(&self) -> Vec<String> {
        let mut alerts = Vec::new();
        if self.state.money < 0 {
            alerts.push(format!("Dettes : {}", self.state.money));
        }
        let mut active = 0;
        for (i, f) in self.state.fighters.iter().enumerate() {
            if f.status != Status::Active {
                continue;
            }
            active += 1;
            let name = fighter_name(i, f, &self.data);
            if f.injuries
                .iter()
                .any(|i| matches!(i.severity, Severity::Medium | Severity::Severe))
            {
                alerts.push(format!("{name} : blessé"));
            }
            if f.illness.is_some_and(|i| i.level != IllnessLevel::Mild) {
                alerts.push(format!("{name} : malade"));
            }
            if f.form.value() < lanista_core::fighter::career::REST_BELOW_FORM {
                alerts.push(format!("{name} : fatigué (forme {})", f.form.value()));
            }
        }
        if active == 0 {
            alerts.push("Plus aucun combattant : recrutez un free agent".into());
        }
        alerts
    }

    fn roster(&mut self, ui: &mut egui::Ui) {
        ui.heading("Combattants");
        let data = &self.data;
        let money = self.state.money;
        egui::ScrollArea::vertical().show(ui, |ui| {
            egui::Grid::new("roster").striped(true).show(ui, |ui| {
                for title in [
                    "Combattant",
                    "Âge",
                    "Forme",
                    "Moral",
                    "Renommée",
                    "État",
                    "Semaine",
                ] {
                    ui.strong(title);
                }
                ui.end_row();
                for (i, (f, plan)) in self
                    .state
                    .fighters
                    .iter()
                    .zip(&mut self.decisions.plans)
                    .enumerate()
                {
                    if ui.button(fighter_name(i, f, data)).clicked() {
                        self.view = View::Sheet(i);
                    }
                    ui.label(f.age.to_string());
                    ui.label(f.form.value().to_string());
                    ui.label(f.morale.value().to_string());
                    ui.label(f.fame.value().to_string());
                    ui.label(condition_text(f));
                    if f.status == Status::Active {
                        ui.horizontal(|ui| plan_editor(ui, plan, f, money, data));
                    } else {
                        ui.label("");
                    }
                    ui.end_row();
                }
            });
        });
    }

    fn sheet(&mut self, ui: &mut egui::Ui, index: usize) {
        let data = &self.data;
        let Some(f) = self.state.fighters.get(index) else {
            self.view = View::Roster;
            return;
        };
        if ui.button("Retour à la liste").clicked() {
            self.view = View::Roster;
        }
        ui.columns(2, |columns| {
            sheet(&mut columns[0], index, f, data);
            let ui = &mut columns[1];
            ui.heading("Cette semaine");
            if f.status == Status::Active
                && let Some(plan) = self.decisions.plans.get_mut(index)
            {
                plan_editor(ui, plan, f, self.state.money, data);
            } else {
                ui.label("Carrière terminée");
            }
            ui.separator();
            ui.heading("Historique");
            egui::ScrollArea::vertical()
                .id_salt("history")
                .show(ui, |ui| {
                    for (week, lines) in self.histories.get(index).into_iter().flatten() {
                        ui.strong(format!("Semaine {week}"));
                        for line in lines {
                            ui.label(line);
                        }
                        ui.add_space(6.0);
                    }
                });
        });
    }

    /// The fight summary; returns a fight to watch again.
    fn fight_summary(&self, ui: &mut egui::Ui) -> Option<Arena> {
        let data = &self.data;
        let mut again = None;
        ui.heading("Résumé des combats");
        egui::ScrollArea::vertical().show(ui, |ui| {
            for (n, report) in self.fights.iter().enumerate() {
                let fight = &report.fight;
                let fighter = &self.state.fighters[report.fighter];
                ui.add_space(8.0);
                ui.strong(fighter_name(report.fighter, fighter, data));
                ui.label(fight_text(fight, data));
                ui.label(format!(
                    "Durée {} s, graine {}",
                    fight.ticks / lanista_core::combat::TICKS_PER_SECOND,
                    fight.seed
                ));
                egui::Grid::new(("fight", n)).striped(true).show(ui, |ui| {
                    ui.label("");
                    ui.strong("Votre combattant");
                    ui.strong(format!(
                        "L'adversaire ({})",
                        class_name(&fight.opponent, data)
                    ));
                    ui.end_row();
                    let sides = &fight.report.fighters;
                    let rows = sides
                        .iter()
                        .map(|f| [f.damage_dealt, f.damage_taken, f.spells_cast, f.hp]);
                    let names = [
                        "Dégâts infligés",
                        "Dégâts reçus",
                        "Sorts lancés",
                        "PV restants",
                    ];
                    let values: Vec<[u32; 4]> = rows.collect();
                    for (row, name) in names.into_iter().enumerate() {
                        ui.label(name);
                        for side in &values {
                            ui.label(side[row].to_string());
                        }
                        ui.end_row();
                    }
                });
                ui.label(format!(
                    "Argent {:+}, renommée {:+}, {}",
                    report.money,
                    fight.fame_change,
                    change("forme", report.form.0, report.form.1)
                ));
                if let Some(injury) = report.injury {
                    ui.label(format!(
                        "Blessure : {}",
                        injury_text(injury, fighter.dominant_hand)
                    ));
                }
                if ui.button("Revoir le combat").clicked() {
                    again = Some(fight.start.clone());
                }
                ui.separator();
            }
        });
        again
    }
}

impl ClubScreen {
    fn watch(&self, arena: Arena) -> Transition {
        let screen = CombatScreen::watch(Rc::clone(&self.data), arena, self.bindings.clone());
        Transition::Push(Box::new(screen))
    }
}

impl Screen for ClubScreen {
    fn update(&mut self, ui: &egui::Context, _assets: &Assets, _audio: &mut Audio) -> Transition {
        if let Some(arena) = self.to_watch.pop() {
            return self.watch(arena);
        }
        if self.bindings.pressed(ui, &Action::Quit) {
            if self.view == View::Menu {
                return Transition::Pop;
            }
            self.view = View::Menu;
        }
        let mut play = self.bindings.pressed(ui, &Action::NextWeek);
        let mut transition = None;
        let mut again = None;
        egui::TopBottomPanel::top("club").show(ui, |ui| transition = self.top_bar(ui));
        egui::CentralPanel::default().show(ui, |ui| match self.view {
            View::Menu => play |= self.menu(ui),
            View::Roster => self.roster(ui),
            View::Sheet(i) => self.sheet(ui, i),
            View::Fights => again = self.fight_summary(ui),
        });
        if let Some(transition) = transition {
            return transition;
        }
        if let Some(arena) = again {
            return self.watch(arena);
        }
        if play {
            self.next_week();
        }
        Transition::None
    }

    fn draw(&self, _assets: &Assets) {
        clear_background(BACKGROUND);
    }
}

/// A seed for a new game: the game, unlike `lanista_core`, may read the clock.
fn new_seed() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs())
}

/// A fighter's week, in one line: rest, training or a fight, and care.
fn plan_editor(
    ui: &mut egui::Ui,
    plan: &mut WeekPlan,
    fighter: &CareerFighter,
    money: i64,
    data: &DesignData,
) {
    for activity in [Activity::Rest, Activity::Train, Activity::Fight] {
        ui.selectable_value(&mut plan.activity, activity, activity_name(activity));
    }
    let cost = fighter.care_cost(&data.fighter);
    if cost == 0 {
        plan.care = false;
    } else if i64::from(cost) > money {
        plan.care = false;
        let text = format!("Soigner ({cost}) : pas assez d'argent");
        ui.add_enabled(false, egui::Checkbox::new(&mut plan.care, text));
    } else {
        ui.checkbox(&mut plan.care, format!("Soigner ({cost})"));
    }
    let ill = fighter.illness.is_some_and(|i| !i.fight_advised());
    if ill && plan.activity == Activity::Fight {
        ui.colored_label(egui::Color32::LIGHT_RED, "Combat déconseillé");
    }
}

/// A fighter's sheet: identity, gauges, stats, personality and health.
fn sheet(ui: &mut egui::Ui, index: usize, f: &CareerFighter, data: &DesignData) {
    let rules = &data.fighter;
    ui.heading(format!("{}, {} ans", fighter_name(index, f, data), f.age));
    let hand = match f.dominant_hand {
        Side::Right => "droitier",
        Side::Left => "gaucher",
    };
    ui.label(hand);
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

fn gauge(ui: &mut egui::Ui, name: &str, value: u32, max: u32) {
    let fraction = f32::from(u16::try_from(value).unwrap_or(u16::MAX))
        / f32::from(u16::try_from(max.max(1)).unwrap_or(u16::MAX));
    ui.add(egui::ProgressBar::new(fraction).text(format!("{name} {value}")));
}

#[cfg(test)]
mod tests {
    use lanista_core::combat::Vec2;
    use lanista_core::duel::play;
    use lanista_core::fighter::FighterId;
    use lanista_core::fighter::career::suggested_plan;
    use lanista_core::log::Report;
    use rand_chacha::ChaCha8Rng;
    use rand_chacha::rand_core::SeedableRng;

    use super::*;

    fn screen(seed: u64) -> ClubScreen {
        let data = Rc::new(DesignData::load().unwrap());
        let state = GameState::new(seed, &data);
        ClubScreen::new(data, state, None, Bindings::new(Vec::new()))
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
            assert_eq!(
                screen.state.fighters,
                [tool_career(seed, 10)],
                "seed {seed}"
            );
            assert_eq!(screen.state.week, 11);
        }
    }

    #[test]
    fn a_fight_week_queues_the_fight_and_sums_it_up() {
        let mut screen = screen(7);
        // Seed 7's first week is a fight.
        assert_eq!(screen.decisions.plans[0].activity, Activity::Fight);
        screen.next_week();
        assert_eq!(screen.view, View::Fights);
        let mut fight = screen.to_watch.pop().unwrap();
        assert_eq!(fight.tick, 0);
        assert_eq!(fight.fighters[0].stats, {
            let mut rng = ChaCha8Rng::seed_from_u64(7);
            let f = CareerFighter::random(&mut rng, &screen.data);
            f.combat_fighter(FighterId(0), &screen.data, Vec2::new(3.0, 10.0))
                .stats
        });
        play(&mut fight);
        assert!(fight.is_over());
        // The summary tells the very fight that was watched.
        let report = &screen.fights[0];
        assert_eq!(report.fight.report, Report::new(&fight));
        assert_eq!((report.money, report.form), (20, (100, 80)));
        // Week 2 is training: nothing to watch, nothing to sum up.
        assert_eq!(screen.decisions.plans[0].activity, Activity::Train);
        screen.next_week();
        assert!(screen.to_watch.is_empty() && screen.fights.is_empty());
    }

    #[test]
    fn the_week_is_told_in_the_menu_and_on_the_sheet() {
        let mut screen = screen(7);
        screen.next_week();
        screen.next_week();
        let weeks: Vec<u64> = screen.histories[0].iter().map(|(week, _)| *week).collect();
        assert_eq!(weeks, [2, 1]);
        let first = &screen.histories[0][1].1;
        assert_eq!(first[0], "Combat");
        assert!(
            first[1].starts_with("Contre un Augure, plus fort : abandon"),
            "{first:?}"
        );
        assert!(
            first.contains(&"Forme 80 (-20), moral 40 (-10), renommée 0".to_owned()),
            "{first:?}"
        );
        let (week, menu) = screen.last_week.as_ref().unwrap();
        assert_eq!(*week, 2);
        assert_eq!(menu[0], "Argent 495 (-15) : entraînement -5, entretien -10");
        assert_eq!(menu[1], "Mirmillon n° 1 : Entraînement");
    }

    #[test]
    fn the_player_can_change_the_plan() {
        let mut screen = screen(7);
        screen.decisions.plans[0] = WeekPlan {
            activity: Activity::Rest,
            care: false,
        };
        screen.next_week();
        assert_eq!(screen.to_watch, []);
        assert_eq!(screen.histories[0][0].1[0], "Repos");
        assert_eq!(screen.state.fighters[0].form, Form::full());
    }

    #[test]
    fn a_free_agent_joins_the_club_once_the_last_fighter_is_gone() {
        let mut screen = screen(7);
        screen.state.fighters[0].status = Status::Retired;
        assert_eq!(
            screen.alerts(),
            ["Plus aucun combattant : recrutez un free agent"]
        );
        screen.next_week();
        assert_eq!(screen.state.week, 2);
        assert_eq!(screen.histories[0], []);
        screen.decisions.hire_free_agent = true;
        screen.next_week();
        assert_eq!(screen.state.fighters.len(), 2);
        assert_eq!(screen.decisions.plans.len(), 2);
        assert_eq!(screen.histories[1].len(), 1);
        assert!(!screen.decisions.hire_free_agent);
    }

    #[test]
    fn each_week_is_saved_and_resumed_at_launch() {
        let data = Rc::new(DesignData::load().unwrap());
        let folder = std::env::temp_dir().join(format!("lanista-club-{}", std::process::id()));
        let open = || {
            ClubScreen::open(
                Rc::clone(&data),
                7,
                Some(folder.clone()),
                Bindings::new(Vec::new()),
            )
        };
        let mut screen = open();
        assert_eq!((screen.state.week, &screen.message), (1, &None));
        screen.next_week();
        screen.next_week();
        let resumed = open();
        assert_eq!(resumed.state, screen.state);
        assert_eq!(resumed.message.as_deref(), Some("Partie reprise"));
        // A manual save is loaded back over a game played further.
        screen.save(saves::MANUAL, true);
        screen.next_week();
        screen.load();
        assert_eq!(screen.state, resumed.state);
        std::fs::remove_dir_all(folder).unwrap();
    }
}
