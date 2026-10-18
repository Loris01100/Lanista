//! The player's club, week after week: the general menu, the fighter list, a
//! fighter's sheet, the summary of the week's fights, the colosseums, the market and the
//! staff (in their own modules), with the sheet of a fighter out of the club and the page
//! of a rival club. Fights are watched in the combat screen; the same seed and decisions
//! give the same game as the `career` tool. The game is saved after every week, and when
//! the player asks.

mod colosseums;
mod market;
mod staff;

use std::path::PathBuf;
use std::rc::Rc;

use lanista_core::calendar::SeasonPhase;
use lanista_core::colosseum::{Challenge, ColosseumId, Contender};
use lanista_core::combat::Arena;
use lanista_core::design_data::DesignData;
use lanista_core::economy::{Flow, Wealth};
use lanista_core::fighter::FighterId;
use lanista_core::fighter::care::CareRules;
use lanista_core::fighter::career::{Activity, CareerFighter, FightSummary, Status, WeekPlan};
use lanista_core::fighter::equipment::Refused;
use lanista_core::fighter::fame::Fame;
use lanista_core::fighter::form::Form;
use lanista_core::fighter::illness::IllnessLevel;
use lanista_core::fighter::injury::{Injury, Severity, Side};
use lanista_core::fighter::morale::Morale;
use lanista_core::market::Target;
use lanista_core::rivals::RivalId;
use lanista_core::state::{Decisions, GameState, next_week};
use macroquad::prelude::*;
use reesch::{Assets, Audio, Bindings, Screen, Transition, egui};

use crate::Action;
use crate::combat_screen::CombatScreen;
use crate::saves;
use crate::texts::{
    STAT_NAMES, activity_name, challenger_text, change, class_name, club_name, colosseum_name,
    condition_text, contender_name, fight_text, fighter_name, illness_text, injury_text,
    market_news_text, money_line, move_text, personality_text, rival_fighter_name, staff_move_text,
    staff_news_text, title_news_text, wealth_name, wealth_text, week_lines,
};

/// Seed of the standard duel offered from the club.
const FREE_DUEL_SEED: u64 = 42;
const BACKGROUND: Color = Color::new(0.16, 0.13, 0.11, 1.0);
/// Ranks shown in a colosseum's ranking; the others are unranked.
const RANKS_SHOWN: usize = 50;

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
    /// Each colosseum: arena, prestige, purses, champion and ranking.
    Colosseums,
    /// Free agents, rival fighters for sale, and the club's fighters to sell or release.
    Market,
    /// The trainer, the doctor and the recruiter, hired and dismissed.
    Staff,
    /// What the club sees of a free agent or a rival club's fighter.
    Outside(Target),
    /// The rival club at this index: profile, money, titles and fighters.
    Rival(usize),
}

/// One of the club's fights, as the fight summary shows it.
#[derive(Debug, Clone, PartialEq)]
struct FightReport {
    /// Index of the club's fighter.
    fighter: usize,
    colosseum: ColosseumId,
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
    /// The view « Retour » leaves a fighter out of the club or a rival club for.
    back: View,
    /// Each fighter's weeks, newest first, in club order.
    histories: Vec<Vec<(u64, Vec<String>)>>,
    /// The last week played: its number, the money line, then one line per fighter.
    last_week: Option<(u64, Vec<String>)>,
    /// The last week's fights.
    fights: Vec<FightReport>,
    /// Fights still to watch, the next one last, each with those who survive a fatal
    /// blow in it.
    to_watch: Vec<(Arena, Vec<FighterId>)>,
    /// Folder of the save files; `None` saves nothing.
    saves: Option<PathBuf>,
    /// The outcome of the last save or load, for the player.
    message: Option<String>,
    /// The offer being written on the market: the rival fighter and the price.
    offer_draft: Option<(RivalId, u32)>,
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
            decisions: state.suggested_decisions(&data),
            histories: vec![Vec::new(); state.fighters.len()],
            data,
            state,
            view: View::Menu,
            back: View::Menu,
            last_week: None,
            fights: Vec::new(),
            to_watch: Vec::new(),
            saves,
            message: None,
            offer_draft: None,
            bindings,
        }
    }

    /// Plays on from `state`, a loaded or new game, which the autosave now holds:
    /// the next launch resumes it, not the game it replaced.
    // ponytail: the week log and fight summaries are not saved, so they start
    // empty; save them once the game keeps a history in `GameState`.
    fn replace(&mut self, state: GameState) {
        let saves = self.saves.take();
        *self = Self::new(Rc::clone(&self.data), state, saves, self.bindings.clone());
        self.save(saves::AUTO, false);
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
        if let Some(wealth) = summary.wealth {
            overview.push(format!(
                "Le club devient {} : {}",
                wealth_name(wealth),
                wealth_text(wealth, data)
            ));
        }
        overview.extend(summary.staff.iter().map(|&m| staff_news_text(m)));
        overview.extend(
            summary
                .market
                .iter()
                .map(|&news| market_news_text(news, &self.state, data)),
        );
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
            if let (
                Some(fight),
                Activity::Fight(colosseum)
                | Activity::TitleFight(colosseum)
                | Activity::Challenge(colosseum, _),
            ) = (report.fight, report.plan.activity)
            {
                let money = summary
                    .ledger
                    .iter()
                    .filter(|t| t.fighter == Some(i))
                    .filter(|t| {
                        matches!(
                            t.flow,
                            Flow::EntryFee | Flow::Receipts | Flow::WinPrize | Flow::ChampionBonus
                        )
                    })
                    .map(|t| t.amount)
                    .sum();
                self.fights.push(FightReport {
                    fighter: i,
                    colosseum,
                    fight,
                    money,
                    form: (before.form.value(), after.form.value()),
                    injury: report.new_injury,
                });
            }
        }
        overview.extend(
            summary
                .titles
                .iter()
                .map(|&(c, news)| title_news_text(c, news, &self.state, data)),
        );
        let refusal = &data.world.challenges;
        overview.extend(summary.refusals.iter().map(|&ch| {
            let club = &self.state.fighters;
            format!(
                "{} refuse le défi de {} sans combattre : renommée -{}, Elo -{}",
                club.get(ch.fighter)
                    .map_or_else(|| "?".into(), |f| fighter_name(ch.fighter, f, data)),
                challenger_text(ch, &self.state, data),
                refusal.refusal_fame,
                refusal.refusal_elo
            )
        }));
        self.last_week = Some((week, overview));
        self.to_watch = self
            .fights
            .iter()
            .rev()
            .map(|f| watched(&f.fight))
            .collect();
        // Free agents are new each week: the one on screen may be gone or replaced.
        if matches!(self.view, View::Outside(Target::FreeAgent(_))) {
            self.view = View::Market;
        }
        if !self.fights.is_empty() {
            self.view = View::Fights;
        }
        let refused: Vec<Refused> = self.decisions.plans.iter().map(|p| p.refused).collect();
        self.decisions = self.state.suggested_decisions(&self.data);
        // A fighter keeps fighting without what it refused, until the player says otherwise.
        for (plan, refused) in self.decisions.plans.iter_mut().zip(refused) {
            plan.refused = refused;
        }
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
            ui.selectable_value(&mut self.view, View::Colosseums, "Colisées");
            ui.selectable_value(&mut self.view, View::Market, "Marché");
            ui.selectable_value(&mut self.view, View::Staff, "Personnel");
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
        ui.label(wealth_text(self.state.wealth, &self.data));
        let alerts = self.alerts();
        if !alerts.is_empty() {
            ui.add_space(6.0);
            ui.strong("Alertes");
            for alert in alerts {
                ui.colored_label(egui::Color32::LIGHT_RED, alert);
            }
        }
        ui.add_space(6.0);
        let max = self.data.economy.max_fighters;
        ui.label(format!(
            "{} / {max} combattants en activité",
            self.state.active_fighters()
        ));
        for &m in &self.decisions.moves {
            ui.label(format!(
                "Marché : {}",
                move_text(m, &self.state, &self.data)
            ));
        }
        for &m in &self.decisions.staff {
            ui.label(format!(
                "Personnel : {}",
                staff_move_text(m, &self.state.staff_market)
            ));
        }
        if let Some((id, price)) = self.state.counter_offer {
            ui.label(format!(
                "Marché : {} demande {price} pour {}",
                club_name(id.club, &self.data),
                rival_fighter_name(id, &self.state, &self.data)
            ));
        }
        let play = ui.button("Semaine suivante (Espace)").clicked();
        if self.saves.is_some() {
            ui.horizontal(|ui| {
                if ui.button("Sauvegarder").clicked() {
                    self.save(saves::MANUAL, true);
                }
                if ui.button("Charger").clicked() {
                    self.load();
                }
                ui.label("Nouvelle partie :");
                for wealth in Wealth::ALL {
                    if ui.button(format!("club {}", wealth_name(wealth))).clicked() {
                        let seed = new_seed();
                        self.replace(GameState::with_wealth(seed, wealth, &self.data));
                        self.message = Some(format!(
                            "Nouvelle partie, club {}, graine {seed}",
                            wealth_name(wealth)
                        ));
                    }
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
        for (i, f) in self.state.fighters.iter().enumerate() {
            if f.status != Status::Active {
                continue;
            }
            let name = fighter_name(i, f, &self.data);
            for injury in &f.injuries {
                if matches!(injury.severity, Severity::Medium | Severity::Severe) {
                    let injury = injury_text(*injury, f.dominant_hand);
                    alerts.push(format!("{name} : blessure, {injury}"));
                }
            }
            if let Some(illness) = f.illness.filter(|i| i.level != IllnessLevel::Mild) {
                alerts.push(format!("{name} : {}", illness_text(illness)));
            }
            if f.form.value() < lanista_core::fighter::career::REST_BELOW_FORM {
                alerts.push(format!("{name} : fatigué (forme {})", f.form.value()));
            }
        }
        for (colosseum, rules) in self
            .state
            .colosseums
            .iter()
            .zip(&self.data.world.colosseums)
        {
            if let Some(Contender::Club(i)) = colosseum.champion
                && let Some(f) = self.state.fighters.get(i)
            {
                alerts.push(format!(
                    "{} : titre de {} à défendre au plus tard en semaine {}",
                    fighter_name(i, f, &self.data),
                    rules.name,
                    colosseum.defense_deadline(&self.data.world)
                ));
            }
        }
        let refusal = &self.data.world.challenges;
        for &ch in &self.state.challenges {
            if let Some(f) = self.state.fighters.get(ch.fighter) {
                alerts.push(format!(
                    "{} : défi de {} (refus sans combattre : renommée -{}, Elo -{})",
                    fighter_name(ch.fighter, f, &self.data),
                    challenger_text(ch, &self.state, &self.data),
                    refusal.refusal_fame,
                    refusal.refusal_elo
                ));
            }
        }
        if self.state.active_fighters() == 0 {
            alerts.push("Plus aucun combattant : recrutez un free agent au marché".into());
        }
        alerts
    }

    /// The title fights the club's fighter `i` may have this week and the challenges
    /// it received, with their names.
    fn special_fights(&self, i: usize) -> Vec<(Activity, String)> {
        let data = &self.data;
        let titles = (0..data.world.colosseums.len())
            .filter_map(|c| u32::try_from(c).ok().map(ColosseumId))
            .filter(|&id| self.state.title_opponent(i, id, data).is_some())
            .map(|id| {
                let activity = Activity::TitleFight(id);
                (activity, activity_name(activity, data))
            });
        let challenges = self
            .state
            .challenges
            .iter()
            .filter(|ch| ch.fighter == i)
            .map(|&ch| {
                let name = challenger_text(ch, &self.state, data);
                (
                    Activity::Challenge(ch.colosseum, ch.rival),
                    format!("Défi de {name}"),
                )
            });
        titles.chain(challenges).collect()
    }

    fn roster(&mut self, ui: &mut egui::Ui) {
        ui.heading("Combattants");
        let data = &self.data;
        let money = self.state.money;
        let care = self.state.care_rules(data);
        let special: Vec<_> = (0..self.state.fighters.len())
            .map(|i| self.special_fights(i))
            .collect();
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
                        ui.horizontal(|ui| {
                            plan_editor(ui, plan, f, &special[i], (money, &care), data);
                        });
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
        let special = self.special_fights(index);
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
                let care = self.state.care_rules(data);
                plan_editor(ui, plan, f, &special, (self.state.money, &care), data);
                challenge_picker(ui, plan, index, &self.state, data);
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
    fn fight_summary(&self, ui: &mut egui::Ui) -> Option<(Arena, Vec<FighterId>)> {
        let data = &self.data;
        let mut again = None;
        ui.heading("Résumé des combats");
        egui::ScrollArea::vertical().show(ui, |ui| {
            for (n, report) in self.fights.iter().enumerate() {
                let fight = &report.fight;
                let fighter = &self.state.fighters[report.fighter];
                ui.add_space(8.0);
                ui.strong(format!(
                    "{}, à {}",
                    fighter_name(report.fighter, fighter, data),
                    colosseum_name(report.colosseum, data)
                ));
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
                    let rows = sides.iter().map(|f| {
                        [
                            f.damage_dealt,
                            f.damage_taken,
                            f.spells_cast,
                            f.misses,
                            f.stuns,
                            f.hp,
                        ]
                    });
                    let names = [
                        "Dégâts infligés",
                        "Dégâts reçus",
                        "Sorts lancés",
                        "Coups ratés",
                        "Étourdi",
                        "PV restants",
                    ];
                    let values: Vec<[u32; 6]> = rows.collect();
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
                    again = Some(watched(fight));
                }
                ui.separator();
            }
        });
        again
    }

    /// Opens `view` from the current one, which « Retour » comes back to when it is
    /// one of the top bar's views.
    fn go_to(&mut self, view: View) {
        if !matches!(self.view, View::Outside(_) | View::Rival(_)) {
            self.back = self.view;
        }
        self.view = view;
    }

    /// A « Retour » button to the view the current one was opened from.
    fn back_button(&mut self, ui: &mut egui::Ui) {
        if ui.button("Retour").clicked() {
            self.view = self.back;
        }
    }

    fn watch(&self, (arena, survivors): (Arena, Vec<FighterId>)) -> Transition {
        let data = Rc::clone(&self.data);
        let screen = CombatScreen::watch(data, arena, survivors, self.bindings.clone());
        Transition::Push(Box::new(screen))
    }
}

impl Screen for ClubScreen {
    fn update(&mut self, ui: &egui::Context, _assets: &Assets, _audio: &mut Audio) -> Transition {
        if let Some(fight) = self.to_watch.pop() {
            return self.watch(fight);
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
            View::Colosseums => self.colosseums(ui),
            View::Market => self.market(ui),
            View::Staff => self.staff(ui),
            View::Outside(target) => self.outside(ui, target),
            View::Rival(club) => self.rival_club(ui, club),
        });
        if let Some(transition) = transition {
            return transition;
        }
        if let Some(fight) = again {
            return self.watch(fight);
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

/// What the combat screen needs to show `fight`: the fight before its first tick, and
/// those who fall at 0 HP but survive the fatal blow.
fn watched(fight: &FightSummary) -> (Arena, Vec<FighterId>) {
    let survivors = (0..)
        .zip(fight.survived_fatal_blow)
        .filter(|&(_, survived)| survived)
        .map(|(id, _)| FighterId(id))
        .collect();
    (fight.start.clone(), survivors)
}

/// A seed for a new game: the game, unlike `lanista_core`, may read the clock.
fn new_seed() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs())
}

/// A fighter's week, in one line: rest, training, a fight, a title fight or a received
/// challenge (in `special`, with their names), and care at the price of the club's
/// `care`, all within its `money`.
fn plan_editor(
    ui: &mut egui::Ui,
    plan: &mut WeekPlan,
    fighter: &CareerFighter,
    special: &[(Activity, String)],
    (money, care): (i64, &CareRules),
    data: &DesignData,
) {
    for activity in [Activity::Rest, Activity::Train] {
        ui.selectable_value(&mut plan.activity, activity, activity_name(activity, data));
    }
    for (i, rules) in data.world.colosseums.iter().enumerate() {
        let activity = Activity::Fight(ColosseumId(u32::try_from(i).unwrap_or(u32::MAX)));
        let fee = rules.purses.entry_fee;
        let mut name = activity_name(activity, data);
        if fee > 0 {
            name = format!("{name} ({fee})");
        }
        // The week would turn into rest: `next_week` drops what the club cannot pay.
        let affordable = i64::from(fee) <= money;
        if !affordable && plan.activity == activity {
            plan.activity = Activity::Rest;
        }
        ui.add_enabled_ui(affordable, |ui| {
            ui.selectable_value(&mut plan.activity, activity, name);
        });
    }
    // A challenge the fighter sent is chosen in its sheet.
    if matches!(plan.activity, Activity::TitleFight(_))
        && !special.iter().any(|(a, _)| *a == plan.activity)
    {
        plan.activity = Activity::Rest;
    }
    for (activity, name) in special {
        ui.selectable_value(&mut plan.activity, *activity, name);
    }
    let cost = fighter.care_cost(care);
    if cost == 0 {
        plan.care = false;
    } else if i64::from(cost) > money {
        plan.care = false;
        let text = format!("Soigner ({cost}) : pas assez d'argent");
        ui.add_enabled(false, egui::Checkbox::new(&mut plan.care, text));
    } else {
        ui.checkbox(&mut plan.care, format!("Soigner ({cost})"));
    }
    let fights = matches!(
        plan.activity,
        Activity::Fight(_) | Activity::TitleFight(_) | Activity::Challenge(..)
    );
    // The colosseum's equipment, refused for fame; nothing to refuse without a fight.
    if fights {
        let fame = &data.fighter.fame;
        ui.checkbox(&mut plan.refused.armour, "Sans armure")
            .on_hover_text(format!(
                "Défenses très réduites, renommée +{} (+{} sans rien)",
                fame.refused_armour, fame.refused_all
            ));
        ui.checkbox(&mut plan.refused.weapon, "Sans arme")
            .on_hover_text(format!(
                "Attaques très réduites, coups à mains nues, renommée +{} (+{} sans rien)",
                fame.refused_weapon, fame.refused_all
            ));
    } else {
        plan.refused = Refused::NONE;
    }
    let ill = fighter.illness.is_some_and(|i| !i.fight_advised());
    if ill && fights {
        ui.colored_label(egui::Color32::LIGHT_RED, "Combat déconseillé");
    }
}

/// The challenges the club's fighter `i` may send, one list per colosseum: the
/// residents, best ranked first, those who would refuse greyed.
fn challenge_picker(
    ui: &mut egui::Ui,
    plan: &mut WeekPlan,
    i: usize,
    state: &GameState,
    data: &DesignData,
) {
    ui.horizontal(|ui| {
        ui.label("Défier :");
        for (c, colosseum) in (0..).zip(&state.colosseums) {
            let id = ColosseumId(c);
            let fee = data.world.colosseum(id).map_or(0, |r| r.purses.entry_fee);
            let chosen = match plan.activity {
                Activity::Challenge(at, rival) if at == id => challenger_text(
                    Challenge {
                        fighter: i,
                        colosseum: id,
                        rival,
                    },
                    state,
                    data,
                ),
                _ => colosseum_name(id, data).to_string(),
            };
            ui.add_enabled_ui(i64::from(fee) <= state.money, |ui| {
                egui::ComboBox::from_id_salt(("challenge", c))
                    .selected_text(chosen)
                    .show_ui(ui, |ui| {
                        for rating in colosseum.active_ranking(state.rosters()) {
                            let Contender::Rival(r) = rating.contender else {
                                continue;
                            };
                            let name = contender_name(rating.contender, state, data);
                            let accepts = state.may_challenge(i, id, r, data);
                            let text = if accepts {
                                format!("{name}, Elo {}", rating.elo)
                            } else {
                                format!("{name}, Elo {} : refuse", rating.elo)
                            };
                            ui.add_enabled_ui(accepts, |ui| {
                                ui.selectable_value(
                                    &mut plan.activity,
                                    Activity::Challenge(id, r),
                                    text,
                                );
                            });
                        }
                    });
            });
        }
    });
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
        Status::Sold => {
            ui.label("Vendu");
        }
        Status::Released => {
            ui.label("Libéré");
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
    ui.label(personality_text(&f.personality));
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
    use lanista_core::log::Report;
    use lanista_core::market::Move;
    use lanista_core::staff::{Candidate, Person, Role, Scouting, Staff, StaffMove};
    use rand_chacha::ChaCha8Rng;
    use rand_chacha::rand_core::SeedableRng;

    use super::*;
    use crate::texts::{
        free_agent_name, outside_personality_text, potential_text, range_texts, staff_text,
        weeks_left_text,
    };

    fn screen(seed: u64) -> ClubScreen {
        let data = Rc::new(DesignData::load().unwrap());
        let state = GameState::new(seed, &data);
        ClubScreen::new(data, state, None, Bindings::new(Vec::new()))
    }

    /// What `career --seed <seed> --weeks <weeks>` plays: the suggested decisions.
    fn tool_career(seed: u64, weeks: u64) -> GameState {
        let data = DesignData::load().unwrap();
        let mut state = GameState::new(seed, &data);
        for _ in 0..weeks {
            next_week(&mut state, &Decisions::default(), &data);
        }
        state
    }

    #[test]
    fn ten_weeks_on_screen_are_the_tool_s_ten_weeks() {
        for seed in [7, 0, 1, 42] {
            let mut screen = screen(seed);
            for _ in 0..10 {
                screen.next_week();
            }
            assert_eq!(screen.state, tool_career(seed, 10), "seed {seed}");
            assert_eq!(screen.state.week, 11);
        }
    }

    #[test]
    fn a_fight_week_queues_the_fight_and_sums_it_up() {
        let mut screen = screen(7);
        // Seed 7's first week is a fight.
        assert_eq!(
            screen.decisions.plans[0].activity,
            Activity::Fight(ColosseumId(0))
        );
        screen.next_week();
        assert_eq!(screen.view, View::Fights);
        let (mut fight, survivors) = screen.to_watch.pop().unwrap();
        assert_eq!(survivors, []);
        assert_eq!(fight.tick, 0);
        assert_eq!(fight.fighters[0].stats, {
            let mut rng = ChaCha8Rng::seed_from_u64(7);
            let f = CareerFighter::random(&mut rng, &screen.data);
            f.combat_fighter(
                FighterId(0),
                Refused::NONE,
                &screen.data,
                Vec2::new(3.0, 10.0),
            )
            .stats
        });
        play(&mut fight);
        assert!(fight.is_over());
        // The summary tells the very fight that was watched.
        let report = &screen.fights[0];
        assert_eq!(report.fight.report, Report::new(&fight));
        // Given up in Capua: the receipts only.
        assert_eq!((report.money, report.form), (20, (100, 80)));
        // A rival fighter challenges it and the suggestion accepts: a fight against
        // that fighter.
        while screen.state.challenges.is_empty() {
            assert!(screen.state.week < 60, "no challenge");
            screen.next_week();
        }
        let challenge = screen.state.challenges[0];
        let resident = screen.state.rival_fighter(challenge.rival).unwrap().clone();
        assert_eq!(
            screen.decisions.plans[0].activity,
            Activity::Challenge(challenge.colosseum, challenge.rival)
        );
        let name = rival_fighter_name(challenge.rival, &screen.state, &screen.data);
        assert!(
            screen
                .alerts()
                .iter()
                .any(|a| a.starts_with(&format!("Mirmillon n° 1 : défi de {name}"))),
            "{:?}",
            screen.alerts()
        );
        screen.next_week();
        assert_eq!(screen.fights[0].fight.opponent, resident);
        // A week of rest: nothing to watch, nothing to sum up.
        screen.decisions.plans[0].activity = Activity::Rest;
        screen.next_week();
        assert!(screen.to_watch.is_empty() && screen.fights.is_empty());
    }

    #[test]
    fn refused_equipment_is_kept_from_week_to_week_and_told() {
        let mut screen = screen(7);
        let weapon = Refused {
            armour: false,
            weapon: true,
        };
        screen.decisions.plans[0].activity = Activity::Fight(ColosseumId(0));
        screen.decisions.plans[0].refused = weapon;
        screen.next_week();
        assert_eq!(screen.decisions.plans[0].refused, weapon);
        let lines = &screen.histories[0][0].1;
        assert!(lines[1].contains(", sans arme :"), "{lines:?}");
    }

    #[test]
    fn the_week_is_told_in_the_menu_and_on_the_sheet() {
        let mut screen = screen(7);
        for _ in 0..3 {
            screen.next_week();
        }
        let weeks: Vec<u64> = screen.histories[0].iter().map(|(week, _)| *week).collect();
        assert_eq!(weeks, [3, 2, 1]);
        let first = &screen.histories[0][2].1;
        assert_eq!(first[0], "Combat à Capoue");
        assert!(
            first[1].starts_with("Contre un Mirmillon, plus fort : abandon"),
            "{first:?}"
        );
        assert!(
            first.contains(&"Forme 80 (-20), moral 40 (-10), renommée 0".to_owned()),
            "{first:?}"
        );
        let (week, menu) = screen.last_week.as_ref().unwrap();
        assert_eq!(*week, 3);
        assert_eq!(
            menu[0],
            "Argent 955 (+60) : mécène +50, recettes +20, entretien -10"
        );
        assert_eq!(
            menu[1],
            "Mirmillon n° 1 : Défi à Capoue, Contre un Augure, plus fort : abandon, renommée -8"
        );
    }

    #[test]
    fn hired_staff_is_told_in_the_menu_and_paid() {
        let mut screen = screen(7);
        // Candidates exactly as their levels are, at the end of the market.
        let market = &mut screen.state.staff_market;
        for (role, level) in [(Role::Trainer, 2), (Role::Doctor, 1), (Role::Recruiter, 5)] {
            let person = Person::of_level(role, level, &screen.data.economy.staff).unwrap();
            screen.decisions.staff.push(StaffMove::Hire(market.len()));
            market.push(Candidate {
                person,
                weeks_left: 1,
            });
        }
        let market = &screen.state.staff_market;
        assert_eq!(
            staff_move_text(StaffMove::Hire(market.len() - 1), market),
            "Engager un recruteur de niveau 5"
        );
        assert_eq!(weeks_left_text(1), "dernière semaine");
        assert_eq!(weeks_left_text(3), "encore 3 semaines");
        let id = RivalId {
            club: 0,
            fighter: 0,
        };
        screen.decisions.moves = vec![Move::Observe(Target::Rival(id))];
        screen.next_week();
        let (_, menu) = screen.last_week.as_ref().unwrap();
        assert!(
            menu[0].contains("salaires -20, salaires -10, salaires -65"),
            "{menu:?}"
        );
        assert_eq!(
            menu[1..4],
            [
                "Entraîneur de niveau 2 engagé",
                "Médecin de niveau 1 engagé",
                "Recruteur de niveau 5 engagé"
            ]
        );
        // The next week's decisions keep the staff, with no new change.
        assert_eq!(screen.decisions.staff, []);
        // The recruiter sees the observed fighter's potential and personality.
        let state = &screen.state;
        let scouting = state.scouting();
        let observed = state.rival_fighter(id).unwrap();
        assert_eq!(observed.observed_weeks, 1);
        assert_ne!(potential_text(observed, &scouting), "?");
        assert!(outside_personality_text(observed, &scouting).starts_with("Agr "));
        let unseen = &state.free_agents[0];
        assert_eq!(outside_personality_text(unseen, &scouting), "?");
    }

    #[test]
    fn the_player_can_change_the_plan() {
        let mut screen = screen(7);
        screen.decisions.plans[0] = WeekPlan {
            activity: Activity::Rest,
            care: false,
            refused: Refused::NONE,
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
            ["Plus aucun combattant : recrutez un free agent au marché"]
        );
        screen.next_week();
        assert_eq!(screen.state.week, 2);
        assert_eq!(screen.histories[0], []);
        let signed = free_agent_name(0, &screen.state.free_agents[0], &screen.data);
        screen.decisions.moves = vec![Move::Sign(0)];
        screen.next_week();
        assert_eq!(screen.state.fighters.len(), 2);
        assert_eq!(screen.decisions.plans.len(), 2);
        assert_eq!(screen.histories[1].len(), 1);
        assert_eq!(screen.decisions.moves, []);
        let (_, overview) = screen.last_week.as_ref().unwrap();
        let name = fighter_name(1, &screen.state.fighters[1], &screen.data);
        assert_eq!(overview[1], format!("Recrute {name}"));
        assert!(signed.starts_with("Free agent n° 1"));
    }

    #[test]
    fn the_market_tells_moves_offers_and_what_is_seen() {
        let mut screen = screen(7);
        let data = Rc::clone(&screen.data);
        let target = RivalId {
            club: 2,
            fighter: 1,
        };
        let state = &screen.state;
        let wanted = state.rival_fighter(target).unwrap();
        let class = class_name(wanted, &data);
        assert_eq!(
            move_text(Move::Offer(target, 100), state, &data),
            format!("Offrir 100 pour {class} n° 2 (Ludus Dacicus)")
        );
        // Two stats seen at first, the others hidden.
        let seen = range_texts(wanted, &Scouting::default(), &data);
        assert_eq!(seen.iter().filter(|r| *r == "?").count(), 5);
        // A low offer is refused, and the week tells it.
        screen.decisions.moves = vec![Move::Offer(target, 10), Move::List(0)];
        screen.next_week();
        let (_, overview) = screen.last_week.as_ref().unwrap();
        assert_eq!(
            overview[1],
            format!("Ludus Dacicus refuse l'offre pour {class} n° 2 (Ludus Dacicus)")
        );
        assert_eq!(screen.state.transfer_list, [0]);
        // Offers came for the listed fighter.
        let offer = screen.state.sale_offers[0];
        let club = club_name(offer.rival, &data).to_owned();
        let name = fighter_name(0, &screen.state.fighters[0], &data);
        screen.decisions.moves = vec![Move::AcceptOffer {
            fighter: 0,
            rival: offer.rival,
        }];
        let money = screen.state.money;
        screen.next_week();
        let (_, overview) = screen.last_week.as_ref().unwrap();
        let patron = i64::from(data.economy.wealth.modest.patron);
        let gain = i64::from(offer.price) + patron;
        assert_eq!(
            overview[0],
            format!(
                "Argent {} (+{gain}) : vente +{}, mécène +{patron}",
                money + gain,
                offer.price
            )
        );
        assert_eq!(
            overview[1],
            format!("Vend {name} à {club} pour {}", offer.price)
        );
        assert_eq!(condition_text(&screen.state.fighters[0]), "vendu");
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
        // Quitting right after loading resumes the loaded game, not the one before.
        assert_eq!(open().state, resumed.state);
        std::fs::remove_dir_all(folder).unwrap();
    }

    /// The milestone of phase 4: a whole season with one club, saved and reloaded
    /// halfway, plays on exactly as if it had never stopped.
    #[test]
    fn a_whole_season_is_played_saved_and_reloaded() {
        let data = Rc::new(DesignData::load().unwrap());
        let folder = std::env::temp_dir().join(format!("lanista-season-{}", std::process::id()));
        let open = || {
            ClubScreen::open(
                Rc::clone(&data),
                7,
                Some(folder.clone()),
                Bindings::new(Vec::new()),
            )
        };
        let mut screen = open();
        let half = u64::from(data.calendar.season_weeks() / 2);
        for _ in 0..half {
            screen.next_week();
        }
        let mut reloaded = open();
        for _ in half..u64::from(data.calendar.season_weeks()) {
            screen.next_week();
            reloaded.next_week();
            assert_eq!(reloaded.fights, screen.fights);
        }
        assert_eq!(reloaded.state, screen.state);
        let next = screen.state.date(&data);
        assert_eq!((next.season, next.week), (2, 1));
        // The rival fighters who lived the whole season took a year.
        let start = GameState::new(7, &data);
        let survivors: Vec<_> = start
            .rivals
            .iter()
            .zip(&screen.state.rivals)
            .flat_map(|(a, b)| a.fighters.iter().zip(&b.fighters))
            .filter(|(_, b)| b.status == Status::Active)
            .collect();
        assert_ne!(survivors, []);
        assert!(survivors.iter().all(|(a, b)| b.age == a.age + 1));
        std::fs::remove_dir_all(folder).unwrap();
    }

    #[test]
    fn titles_are_told_in_the_menu() {
        let mut screen = screen(7);
        screen.state.colosseums[1].champion = Some(Contender::Club(0));
        assert_eq!(
            screen.alerts(),
            ["Mirmillon n° 1 : titre de Rome à défendre au plus tard en semaine 5"]
        );
        // The rival champions defend in week 5, without the player.
        let mut screen = self::screen(7);
        for _ in 0..5 {
            screen.next_week();
        }
        let (_, menu) = screen.last_week.as_ref().unwrap();
        assert!(
            menu.iter()
                .any(|line| line.starts_with("Titre de Capoue : ") && line.contains(" (Ludus ")),
            "{menu:?}"
        );
    }

    #[test]
    fn the_rival_clubs_are_listed() {
        let screen = screen(7);
        assert_eq!(
            screen.rival_lines(),
            [
                "Ludus Magnus, prudent et riche : 10 combattants",
                "Ludus Gallicus, agressif et pauvre : 10 combattants",
                "Ludus Dacicus, marchand et modeste : 10 combattants",
            ]
        );
    }

    #[test]
    fn a_rival_club_s_staff_is_told() {
        let data = DesignData::load().unwrap();
        assert_eq!(staff_text(&Staff::default()), "Personnel : aucun");
        let staff = Staff::of_levels(
            &[(Role::Trainer, 5), (Role::Doctor, 2)],
            &data.economy.staff,
        );
        assert_eq!(
            staff_text(&staff),
            "Personnel : entraîneur de niveau 5, médecin de niveau 2"
        );
    }

    #[test]
    fn a_rival_fighter_s_standing_and_titles_are_told() {
        let screen = screen(7);
        let rosters = screen.state.rosters();
        let first = *screen.state.colosseums[0]
            .active_ranking(rosters)
            .next()
            .unwrap();
        let elo = first.elo;
        assert!(
            screen.standings(first.contender)[0].ends_with(&format!("1er, Elo {elo}")),
            "{:?}",
            screen.standings(first.contender)
        );
        let Some(Contender::Rival(champion)) = screen.state.colosseums[0].champion else {
            panic!("no rival champion in Capua");
        };
        let name = rival_fighter_name(champion, &screen.state, &screen.data);
        assert!(
            screen
                .titles_of(champion.club)
                .contains(&format!("Champion de Capoue : {name}"))
        );
        assert!(screen.standings(Contender::Rival(champion))[0].starts_with("champion, "));
        // Gone fighters are no longer ranked.
        let gone = RivalId {
            club: 0,
            fighter: 999,
        };
        assert_eq!(
            screen.standings(Contender::Rival(gone)),
            ["non classé", "non classé"]
        );
    }

    #[test]
    fn pages_out_of_the_club_open_and_come_back() {
        let mut screen = screen(7);
        screen.view = View::Colosseums;
        let id = RivalId {
            club: 1,
            fighter: 0,
        };
        screen.go_to(View::Outside(Target::Rival(id)));
        screen.go_to(View::Rival(1));
        // « Retour » leads back to the colosseums, not from page to page.
        assert_eq!(
            (screen.view, screen.back),
            (View::Rival(1), View::Colosseums)
        );
        // A free agent's sheet does not outlive the week: the list changes.
        screen.view = View::Market;
        screen.go_to(View::Outside(Target::FreeAgent(0)));
        screen.decisions.plans[0].activity = Activity::Rest;
        screen.next_week();
        assert_eq!(screen.view, View::Market);
        let mut f = screen.state.free_agents[0].clone();
        assert_eq!(market::observed_text(&f), "Jamais observé");
        f.observed_weeks = 2;
        assert_eq!(market::observed_text(&f), "Observé 2 semaines");
    }

    #[test]
    fn alerts_tell_where_a_fighter_is_hurt() {
        let mut screen = screen(7);
        screen.state.fighters[0].injuries.push(Injury {
            location: lanista_core::fighter::injury::Location::Arm(Side::Right),
            severity: Severity::Medium,
            weeks_left: 3,
        });
        let hand = screen.state.fighters[0].dominant_hand;
        let arm = injury_text(screen.state.fighters[0].injuries[0], hand);
        assert_eq!(
            screen.alerts(),
            [format!("Mirmillon n° 1 : blessure, {arm}")]
        );
        assert!(arm.starts_with("Bras droit"), "{arm}");
    }
}
