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

use colosseums::contender_view;
use std::rc::Rc;

use lanista_core::calendar::SeasonPhase;
use lanista_core::colosseum::{Challenge, ColosseumId, Contender, Continent};
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
use lanista_core::fighter::retirement::too_maimed;
use lanista_core::journal::FightLine;
use lanista_core::market::Target;
use lanista_core::names::{self, ClubName};
use lanista_core::rivals::{self, RivalId};
use lanista_core::state::{Decisions, GameState, next_week};
use macroquad::prelude::*;
use rand_chacha::ChaCha8Rng;
use rand_chacha::rand_core::SeedableRng;
use reesch::{Assets, Audio, Bindings, Screen, Transition, egui};

use crate::Action;
use crate::combat_screen::{CombatScreen, Watched};
use crate::fight_view::Cast;
use crate::lang::{self, tr};
use crate::options;
use crate::saves;
use crate::texts::{
    activity_name, challenger_text, change, class_name, club_name, colosseum_name,
    colosseum_rules_name, condition_text, contender_name, continent_name, culture_name,
    date_in_sentence, date_text, ease_text, fight_text, fighter_name, illness_text, injury_text,
    loadout_name, market_news_text, money_line, move_text, named_club, origin_text,
    personality_text, priced, rival_fighter_name, staff_move_text, staff_news_text, stat_names,
    title_news_text, vowel, wealth_name, wealth_text, week_lines, world_news_text,
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

/// What a save slot holds, as the menu shows it.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Slot {
    Empty,
    /// A game at this week, of a club of this name and wealth on this continent.
    Game(u64, ClubName, Wealth, Continent),
    Unreadable,
}

/// A choice that loses a game, carried out only once the player confirms it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Confirm {
    /// Saves over the game in this slot.
    Save(usize),
    /// Loads this slot over the game being played.
    Load(usize),
    /// Starts a new game over the one being played.
    NewGame(Wealth, Continent),
}

/// One of the club's fights, as the fight summary shows it.
#[derive(Debug, Clone, PartialEq)]
struct FightReport {
    /// Index of the club's fighter.
    fighter: usize,
    colosseum: ColosseumId,
    fight: FightSummary,
    /// The fight as the journal tells it.
    line: FightLine,
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
    to_watch: Vec<Watched>,
    /// Folder of the save files; `None` saves nothing.
    saves: Option<PathBuf>,
    /// The outcome of the last save or load, for the player.
    message: Option<String>,
    /// What each of `saves::SLOTS` holds.
    slots: [Slot; saves::SLOTS.len()],
    /// The choice waiting for the player to confirm it.
    confirm: Option<Confirm>,
    /// The offer being written on the market: the rival fighter and the price.
    offer_draft: Option<(RivalId, u32)>,
    /// The continent the colosseums view shows, the club's at first.
    shown_continent: Continent,
    /// The prestige of the colosseums the colosseums view shows; `None` shows them all.
    shown_prestige: Option<u32>,
    /// Where a new game starts.
    new_continent: Continent,
    /// The new game's club: the key of its culture, one of `new_continent`'s, and its
    /// name, as the player chose or drew them.
    new_club: (String, ClubName),
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
        if let Some(code) = saves.as_deref().and_then(saves::read_language) {
            lang::set_language(&code);
        }
        let autosave = saves
            .as_deref()
            .map_or(Ok(None), |folder| saves::read(folder, saves::AUTO, &data));
        let (state, message) = match autosave {
            Ok(Some(state)) => (state, Some(tr!("save-resumed"))),
            Ok(None) => (GameState::new(seed, &data), None),
            Err(e) => (
                GameState::new(seed, &data),
                Some(tr!("save-autosave-unreadable", error = e)),
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
        let mut screen = Self {
            decisions: state.suggested_decisions(&data),
            histories: vec![Vec::new(); state.fighters.len()],
            shown_continent: state.continent,
            shown_prestige: None,
            new_continent: state.continent,
            new_club: (state.culture.clone(), state.name.clone()),
            slots: read_slots(saves.as_deref(), &data),
            data,
            state,
            view: View::Menu,
            back: View::Menu,
            last_week: None,
            fights: Vec::new(),
            to_watch: Vec::new(),
            saves,
            message: None,
            confirm: None,
            offer_draft: None,
            bindings,
        };
        screen.read_journal();
        screen
    }

    /// Each fighter's weeks, the last week and its fights, as the game's journal
    /// keeps them: after a week, and when a game is loaded.
    fn read_journal(&mut self) {
        let data = &self.data;
        let journal = &self.state.journal;
        let fighters = &self.state.fighters;
        let dominant = |i: usize| fighters.get(i).map_or(Side::Right, |f| f.dominant_hand);
        self.histories = (0..fighters.len().max(journal.fighters.len()))
            .map(|i| {
                let weeks = journal.fighters.get(i).into_iter().flatten();
                weeks
                    .map(|w| (w.week, week_lines(w, dominant(i), data)))
                    .collect()
            })
            .collect();
        self.fights.clear();
        let Some(played) = &journal.last else {
            self.last_week = None;
            return;
        };
        let summary = &played.summary;
        let mut overview = vec![money_line(played.money.0, played.money.1, &summary.ledger)];
        if let Some(wealth) = summary.wealth {
            overview.push(tr!("club-becomes", wealth = wealth_name(wealth)));
        }
        overview.extend(summary.staff.iter().map(|&m| staff_news_text(m)));
        overview.extend(
            summary
                .market
                .iter()
                .map(|&news| market_news_text(news, &self.state, data)),
        );
        for (i, report) in summary.reports.iter().enumerate() {
            let (Some(report), Some(fighter)) = (report, fighters.get(i)) else {
                continue;
            };
            let entry = journal.fighters.get(i).and_then(|weeks| weeks.first());
            let Some(entry) = entry.filter(|e| e.week == played.week) else {
                continue;
            };
            let mut line = tr!(
                "labelled",
                label = fighter_name(i, fighter, data),
                text = activity_name(entry.activity, data)
            );
            if let Some(fight) = &entry.fight {
                line = format!("{line}, {}", fight_text(fight, data));
            }
            overview.push(line);
            if let (
                Some(fight),
                Some(line),
                Activity::Fight(colosseum)
                | Activity::TitleFight(colosseum)
                | Activity::Challenge(colosseum, _)
                | Activity::Melee(colosseum),
            ) = (&report.fight, &entry.fight, report.plan.activity)
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
                    fight: fight.clone(),
                    line: line.clone(),
                    money,
                    form: entry.form,
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
            let challenger = challenger_text(ch, &self.state, data);
            tr!(
                "challenge-refused",
                fighter = fighters
                    .get(ch.fighter)
                    .map_or_else(|| "?".into(), |f| fighter_name(ch.fighter, f, data)),
                vowel = vowel(&challenger),
                challenger = challenger,
                fame = refusal.refusal_fame,
                elo = refusal.refusal_elo
            )
        }));
        self.last_week = Some((played.week, overview));
    }

    /// Plays on from `state`, a loaded or new game, which the autosave now holds:
    /// the next launch resumes it, not the game it replaced.
    fn replace(&mut self, state: GameState) {
        let saves = self.saves.take();
        *self = Self::new(Rc::clone(&self.data), state, saves, self.bindings.clone());
        self.save(saves::AUTO, false);
    }

    /// Writes the game to `file`; tells the player only when asked or when it fails.
    fn save(&mut self, file: &str, asked: bool) {
        let Some(folder) = &self.saves else { return };
        match saves::write(folder, file, &self.state) {
            Ok(()) if asked => self.message = Some(tr!("save-saved")),
            Ok(()) => {}
            Err(e) => self.message = Some(tr!("save-failed", error = e.to_string())),
        }
    }

    /// Writes the game to the save slot `slot`.
    fn save_slot(&mut self, slot: usize) {
        self.save(saves::SLOTS[slot], true);
        self.slots = read_slots(self.saves.as_deref(), &self.data);
    }

    /// Loads the save slot `slot` over the game being played.
    fn load(&mut self, slot: usize) {
        let Some(folder) = &self.saves else { return };
        match saves::read(folder, saves::SLOTS[slot], &self.data) {
            Ok(Some(state)) => {
                self.replace(state);
                self.message = Some(tr!("save-loaded"));
            }
            Ok(None) => self.message = Some(tr!("save-none")),
            Err(e) => self.message = Some(tr!("save-unreadable", error = e)),
        }
    }

    /// Asks the player to confirm `choice`, or carries it out at once when it loses
    /// nothing: saving to an empty slot.
    fn ask(&mut self, choice: Confirm) {
        match choice {
            Confirm::Save(slot) if self.slots[slot] == Slot::Empty => self.carry_out(choice),
            _ => self.confirm = Some(choice),
        }
    }

    fn carry_out(&mut self, choice: Confirm) {
        self.confirm = None;
        match choice {
            Confirm::Save(slot) => self.save_slot(slot),
            Confirm::Load(slot) => self.load(slot),
            Confirm::NewGame(wealth, continent) => {
                let seed = new_seed();
                let club = self.new_club.clone();
                let state = GameState::start(seed, (wealth, continent), Some(club), &self.data);
                let club = named_club(&state.name);
                self.replace(state);
                self.message = Some(tr!(
                    "menu-new-game-started",
                    club = club,
                    wealth = wealth_name(wealth),
                    continent = continent_name(continent),
                    seed = seed.to_string()
                ));
            }
        }
    }

    /// Plays the week with the current decisions; its fights are queued to be
    /// watched, then summed up.
    fn next_week(&mut self) {
        next_week(&mut self.state, &self.decisions, &self.data);
        self.read_journal();
        self.to_watch = to_watch(&self.fights, &self.state, &self.data);
        // Free agents are new each week: the one on screen may be gone or replaced. A
        // rival club's fighter sold or let go is followed.
        if matches!(self.view, View::Outside(Target::FreeAgent(_))) {
            self.view = View::Market;
        }
        self.view = self.followed(self.view);
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
            SeasonPhase::Competition => "competition",
            SeasonPhase::GreatEvent => "great-event",
            SeasonPhase::OffSeason => "off-season",
        };
        let mut transition = None;
        ui.horizontal(|ui| {
            ui.strong(named_club(&self.state.name));
            ui.separator();
            ui.strong(tr!(
                "top-date",
                season = date.season,
                week = date.week,
                phase = phase
            ));
            ui.separator();
            ui.strong(tr!("top-money", money = self.state.money));
            ui.separator();
            if let Some(message) = &self.message {
                ui.label(message);
                ui.separator();
            }
            ui.selectable_value(&mut self.view, View::Menu, tr!("view-menu"));
            ui.selectable_value(&mut self.view, View::Roster, tr!("view-roster"));
            ui.add_enabled_ui(!self.fights.is_empty(), |ui| {
                ui.selectable_value(&mut self.view, View::Fights, tr!("view-fights"));
            });
            ui.selectable_value(&mut self.view, View::Colosseums, tr!("view-colosseums"));
            ui.selectable_value(&mut self.view, View::Market, tr!("view-market"));
            ui.selectable_value(&mut self.view, View::Staff, tr!("view-staff"));
            ui.separator();
            if ui.button(tr!("standard-duel")).clicked() {
                let duel =
                    CombatScreen::new(Rc::clone(&self.data), FREE_DUEL_SEED, self.bindings.clone())
                        .keeping_settings_in(self.saves.clone());
                transition = Some(Transition::Push(Box::new(duel)));
            }
        });
        transition
    }

    /// The general menu; returns whether to play the week.
    fn menu(&mut self, ui: &mut egui::Ui) -> bool {
        ui.heading(tr!("menu-title"));
        ui.label(tr!(
            "menu-club",
            club = named_club(&self.state.name),
            wealth = wealth_text(self.state.wealth),
            continent = continent_name(self.state.continent)
        ));
        let alerts = self.alerts();
        if !alerts.is_empty() {
            ui.add_space(6.0);
            ui.strong(tr!("menu-alerts"));
            for alert in alerts {
                ui.colored_label(egui::Color32::LIGHT_RED, alert);
            }
        }
        ui.add_space(6.0);
        let max = self.data.economy.max_fighters;
        ui.label(tr!(
            "menu-active",
            active = self.state.active_fighters(),
            max = max
        ));
        for line in self.pending_lines() {
            ui.label(line);
        }
        let play = ui.button(tr!("menu-next-week")).clicked();
        if self.saves.is_some() {
            self.files(ui);
        }
        ui.separator();
        egui::ScrollArea::vertical().show(ui, |ui| {
            if let Some((week, lines)) = &self.last_week {
                ui.strong(tr!("week-title", week = *week));
                for line in lines {
                    ui.label(line);
                }
                ui.add_space(6.0);
            }
            ui.strong(tr!("menu-news"));
            for line in self.news_lines() {
                ui.label(line);
            }
        });
        play
    }

    /// The save slots, saved and loaded, and starting a new game; a choice that loses
    /// a game waits for the player to confirm it.
    fn files(&mut self, ui: &mut egui::Ui) {
        if let Some(choice) = self.confirm {
            ui.horizontal(|ui| {
                ui.colored_label(egui::Color32::LIGHT_RED, self.question(choice));
                if ui.button(tr!("confirm")).clicked() {
                    self.carry_out(choice);
                }
                if ui.button(tr!("cancel")).clicked() {
                    self.confirm = None;
                }
            });
            return;
        }
        let mut chosen = None;
        for (i, slot) in self.slots.iter().enumerate() {
            ui.horizontal(|ui| {
                ui.label(tr!("save-slot", slot = i + 1, game = self.slot_text(slot)));
                if ui.button(tr!("menu-save")).clicked() {
                    chosen = Some(Confirm::Save(i));
                }
                let loadable = matches!(slot, Slot::Game(..));
                if ui
                    .add_enabled(loadable, egui::Button::new(tr!("menu-load")))
                    .clicked()
                {
                    chosen = Some(Confirm::Load(i));
                }
            });
        }
        ui.horizontal(|ui| {
            ui.label(tr!("menu-new-game"));
            let continent = self.new_continent;
            egui::ComboBox::from_id_salt("new continent")
                .selected_text(continent_name(self.new_continent))
                .show_ui(ui, |ui| {
                    for continent in Continent::ALL {
                        ui.selectable_value(
                            &mut self.new_continent,
                            continent,
                            continent_name(continent),
                        );
                    }
                });
            if self.new_continent != continent {
                self.draw_new_club(None);
            }
            self.new_club_name(ui);
        });
        ui.horizontal(|ui| {
            // A club needs a name.
            let named = !self.new_club.1.name.trim().is_empty();
            for wealth in Wealth::ALL {
                let button = egui::Button::new(tr!("menu-new-club", wealth = wealth_name(wealth)));
                if ui.add_enabled(named, button).clicked() {
                    chosen = Some(Confirm::NewGame(wealth, self.new_continent));
                }
            }
        });
        if let Some(choice) = chosen {
            self.ask(choice);
        }
    }

    /// The culture, pattern and name of the new game's club, and « Tirer » to draw
    /// another name in its culture.
    fn new_club_name(&mut self, ui: &mut egui::Ui) {
        let data = Rc::clone(&self.data);
        let continent = self.new_continent;
        let (culture, name) = &mut self.new_club;
        let before = culture.clone();
        egui::ComboBox::from_id_salt("new culture")
            .selected_text(culture_name(culture))
            .show_ui(ui, |ui| {
                for c in data.cultures.iter().filter(|c| c.continent == continent) {
                    ui.selectable_value(culture, c.key.clone(), culture_name(&c.key));
                }
            });
        if *culture != before {
            let culture = culture.clone();
            self.draw_new_club(Some(&culture));
            return;
        }
        let patterns = data.culture(culture).map_or(&[][..], |c| &c.clubs[..]);
        egui::ComboBox::from_id_salt("new pattern")
            .selected_text(named_club(name))
            .show_ui(ui, |ui| {
                for pattern in patterns {
                    let shown = ClubName {
                        pattern: pattern.clone(),
                        ..name.clone()
                    };
                    ui.selectable_value(&mut name.pattern, pattern.clone(), named_club(&shown));
                }
            });
        ui.add(egui::TextEdit::singleline(&mut name.name).desired_width(120.0));
        if ui.button(tr!("menu-draw-name")).clicked() {
            let culture = culture.clone();
            self.draw_new_club(Some(&culture));
        }
    }

    /// Draws the new game's club a name in `culture`, or in a culture drawn on its
    /// continent.
    fn draw_new_club(&mut self, culture: Option<&str>) {
        let mut rng = ChaCha8Rng::seed_from_u64(new_seed());
        let key = match culture {
            Some(key) => Some(key.to_owned()),
            None => rivals::culture_on(&mut rng, Some(self.new_continent), &self.data),
        };
        let Some(culture) = key.as_deref().and_then(|key| self.data.culture(key)) else {
            return;
        };
        let name = names::club_name(&mut rng, culture, &[]);
        self.new_club = (culture.key.clone(), name);
    }

    fn slot_text(&self, slot: &Slot) -> String {
        match slot {
            Slot::Empty => tr!("save-slot-empty"),
            Slot::Game(week, club, wealth, continent) => tr!(
                "save-slot-game",
                club = named_club(club),
                date = date_text(*week, &self.data),
                wealth = wealth_name(*wealth),
                continent = continent_name(*continent)
            ),
            Slot::Unreadable => tr!("save-slot-unreadable"),
        }
    }

    /// The question asked before carrying out `choice`.
    fn question(&self, choice: Confirm) -> String {
        match choice {
            Confirm::Save(slot) => tr!(
                "confirm-overwrite",
                slot = slot + 1,
                game = self.slot_text(&self.slots[slot])
            ),
            Confirm::Load(slot) => tr!(
                "confirm-load",
                slot = slot + 1,
                game = self.slot_text(&self.slots[slot])
            ),
            Confirm::NewGame(wealth, continent) => tr!(
                "confirm-new-game",
                club = named_club(&self.new_club.1),
                wealth = wealth_name(wealth),
                continent = continent_name(continent)
            ),
        }
    }

    /// The moves decided for the week and the pending counter-offer.
    fn pending_lines(&self) -> Vec<String> {
        let moves = self.decisions.moves.iter().map(|&m| {
            tr!(
                "pending-market",
                what = move_text(m, &self.state, &self.data)
            )
        });
        let staff = self.decisions.staff.iter().map(|&m| {
            tr!(
                "pending-staff",
                what = staff_move_text(m, &self.state.staff_market)
            )
        });
        let counter = self.state.counter_offer.map(|(id, price)| {
            tr!(
                "pending-counter-offer",
                club = club_name(id.club, &self.state),
                price = price,
                fighter = rival_fighter_name(id, &self.state, &self.data)
            )
        });
        moves.chain(staff).chain(counter).collect()
    }

    /// The world's news, newest first.
    fn news_lines(&self) -> Vec<String> {
        let news = self.state.news.iter().rev();
        news.map(|&n| world_news_text(n, &self.state, &self.data))
            .collect()
    }

    /// What needs the player's attention before the week.
    fn alerts(&self) -> Vec<String> {
        let mut alerts = Vec::new();
        if self.state.money < 0 {
            alerts.push(tr!("alert-debts", money = self.state.money));
        }
        for (i, f) in self.state.fighters.iter().enumerate() {
            if f.status != Status::Active {
                continue;
            }
            let name = fighter_name(i, f, &self.data);
            for injury in &f.injuries {
                if matches!(injury.severity, Severity::Medium | Severity::Severe) {
                    let injury = injury_text(*injury, f.dominant_hand);
                    alerts.push(tr!(
                        "alert-injury",
                        fighter = name.as_str(),
                        injury = injury
                    ));
                }
            }
            if let Some(illness) = f.illness.filter(|i| i.level != IllnessLevel::Mild) {
                alerts.push(tr!(
                    "labelled",
                    label = name.as_str(),
                    text = illness_text(illness)
                ));
            }
            if f.form.value() < lanista_core::fighter::career::REST_BELOW_FORM {
                alerts.push(tr!(
                    "alert-tired",
                    fighter = name.as_str(),
                    form = f.form.value()
                ));
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
                alerts.push(tr!(
                    "alert-title-defense",
                    fighter = fighter_name(i, f, &self.data),
                    place = colosseum_rules_name(rules),
                    week = colosseum.defense_deadline(&self.data.world)
                ));
            }
        }
        let refusal = &self.data.world.challenges;
        for &ch in &self.state.challenges {
            if let Some(f) = self.state.fighters.get(ch.fighter) {
                let challenger = challenger_text(ch, &self.state, &self.data);
                alerts.push(tr!(
                    "alert-challenge",
                    fighter = fighter_name(ch.fighter, f, &self.data),
                    vowel = vowel(&challenger),
                    challenger = challenger,
                    fame = refusal.refusal_fame,
                    elo = refusal.refusal_elo
                ));
            }
        }
        for offer in &self.state.sale_offers {
            if let Some(f) = self.state.fighters.get(offer.fighter) {
                alerts.push(tr!(
                    "alert-sale-offer",
                    club = club_name(offer.rival, &self.state),
                    fighter = fighter_name(offer.fighter, f, &self.data),
                    price = offer.price
                ));
            }
        }
        let rivalry = &self.data.world.relations.rivalry;
        for (&club, relation) in &self.state.relations {
            if let Some(week) = relation.rival_since {
                alerts.push(tr!(
                    "alert-rival",
                    club = club_name(club, &self.state),
                    since = date_in_sentence(week, &self.data),
                    receipts = rivalry.receipts_percent,
                    fame = rivalry.fame
                ));
            }
        }
        if self.state.active_fighters() == 0 {
            alerts.push(tr!("alert-no-fighter"));
        }
        alerts
    }

    /// The title fights the club's fighter `i` may have this week, the challenges it
    /// received and, in the season's opening week, the melee of each colosseum of the
    /// club's continent, with their names.
    fn special_fights(&self, i: usize) -> Vec<(Activity, String)> {
        let data = &self.data;
        let melees = data
            .world
            .colosseums_of(self.state.continent)
            .filter(|_| self.state.opens_season(data))
            .map(|id| {
                let activity = Activity::Melee(id);
                let fee = self.state.entry_fee(id, data).unwrap_or(0);
                (activity, priced(activity, fee, data))
            });
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
                    tr!("challenge-from", vowel = vowel(&name), challenger = name),
                )
            });
        melees.chain(titles).chain(challenges).collect()
    }

    fn roster(&mut self, ui: &mut egui::Ui) {
        ui.heading(tr!("view-roster"));
        let data = &self.data;
        let care = self.state.care_rules(data);
        let special: Vec<_> = (0..self.state.fighters.len())
            .map(|i| self.special_fights(i))
            .collect();
        egui::ScrollArea::vertical().show(ui, |ui| {
            egui::Grid::new("roster").striped(true).show(ui, |ui| {
                for title in [
                    tr!("roster-fighter"),
                    tr!("roster-age"),
                    tr!("roster-form"),
                    tr!("roster-morale"),
                    tr!("roster-fame"),
                    tr!("roster-condition"),
                    tr!("roster-week"),
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
                            let club = (&self.state, &care);
                            plan_editor(ui, plan, f, &special[i], club, data);
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
        if ui.button(tr!("back-to-list")).clicked() {
            self.view = View::Roster;
        }
        ui.columns(2, |columns| {
            sheet(&mut columns[0], index, f, data);
            let ui = &mut columns[1];
            ui.heading(tr!("sheet-this-week"));
            if f.status == Status::Active
                && let Some(plan) = self.decisions.plans.get_mut(index)
            {
                let care = self.state.care_rules(data);
                plan_editor(ui, plan, f, &special, (&self.state, &care), data);
                challenge_picker(ui, plan, index, &self.state, data);
            } else {
                ui.label(tr!("sheet-career-over"));
            }
            ui.separator();
            ui.heading(tr!("sheet-history"));
            egui::ScrollArea::vertical()
                .id_salt("history")
                .show(ui, |ui| {
                    for (week, lines) in self.histories.get(index).into_iter().flatten() {
                        ui.strong(tr!("week-title", week = *week));
                        for line in lines {
                            ui.label(line);
                        }
                        ui.add_space(6.0);
                    }
                });
        });
    }

    /// The fight summary; returns the seed of a fight to watch again.
    fn fight_summary(&self, ui: &mut egui::Ui) -> Option<u64> {
        let data = &self.data;
        let mut again = None;
        ui.heading(tr!("view-fights"));
        egui::ScrollArea::vertical().show(ui, |ui| {
            for (n, report) in self.fights.iter().enumerate() {
                let fight = &report.fight;
                let fighter = &self.state.fighters[report.fighter];
                ui.add_space(8.0);
                ui.strong(tr!(
                    "summary-fighter-at",
                    fighter = fighter_name(report.fighter, fighter, data),
                    place = colosseum_name(report.colosseum, data)
                ));
                ui.label(fight_text(&report.line, data));
                ui.label(tr!(
                    "summary-duration",
                    seconds = fight.ticks / lanista_core::combat::TICKS_PER_SECOND,
                    seed = fight.seed.to_string()
                ));
                egui::Grid::new(("fight", n)).striped(true).show(ui, |ui| {
                    ui.label("");
                    ui.strong(tr!("summary-yours"));
                    // A melee shows the fighter's line only, a duel both sides.
                    let sides: Vec<_> =
                        if let (false, Some(opponent)) = (fight.is_melee(), fight.opponent()) {
                            ui.strong(tr!("summary-opponent", class = class_name(opponent, data)));
                            fight.report.fighters.iter().collect()
                        } else {
                            fight.mine().into_iter().collect()
                        };
                    ui.end_row();
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
                        tr!("summary-damage-dealt"),
                        tr!("summary-damage-taken"),
                        tr!("summary-spells-cast"),
                        tr!("summary-misses"),
                        tr!("summary-stuns"),
                        tr!("summary-hp-left"),
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
                ui.label(tr!(
                    "summary-money",
                    money = format!("{:+}", report.money),
                    fame = format!("{:+}", fight.fame_change),
                    form = change(&tr!("summary-form"), report.form.0, report.form.1)
                ));
                if let Some(injury) = report.injury {
                    ui.label(tr!(
                        "week-injury",
                        injury = injury_text(injury, fighter.dominant_hand)
                    ));
                }
                if ui.button(tr!("summary-watch-again")).clicked() {
                    again = Some(fight.seed);
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
        self.view = self.followed(view);
    }

    /// `view`, or the page of where its fighter went if it is out of the club and gone
    /// elsewhere.
    fn followed(&self, view: View) -> View {
        let View::Outside(target) = view else {
            return view;
        };
        let contender = match target {
            Target::Rival(id) => Contender::Rival(id),
            Target::FreeAgent(a) => Contender::FreeAgent(a),
        };
        contender_view(self.state.whereabouts(contender))
    }

    /// A « Retour » button to the view the current one was opened from.
    fn back_button(&mut self, ui: &mut egui::Ui) {
        if ui.button(tr!("back")).clicked() {
            self.view = self.back;
        }
    }

    /// Watches all the week's `fights` at once.
    fn watch(&self, fights: Vec<Watched>) -> Transition {
        let data = Rc::clone(&self.data);
        let screen = CombatScreen::watch(data, fights, self.bindings.clone())
            .keeping_settings_in(self.saves.clone());
        Transition::Push(Box::new(screen))
    }
}

impl Screen for ClubScreen {
    fn update(&mut self, ui: &egui::Context, _assets: &Assets, audio: &mut Audio) -> Transition {
        if !self.to_watch.is_empty() {
            let fights = std::mem::take(&mut self.to_watch);
            return self.watch(fights);
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
        if let Some(failed) = options::window(ui, audio, self.saves.as_deref()) {
            self.message = Some(failed);
        }
        if let Some(transition) = transition {
            return transition;
        }
        if let Some(seed) = again {
            let all = to_watch(&self.fights, &self.state, &self.data);
            let fight: Vec<Watched> = all.into_iter().filter(|w| w.arena.seed == seed).collect();
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

/// The week's fights to watch, in club order, each once, though several of the club's
/// fighters shared a melee: the fight before its first tick, the club's fighters named
/// and ringed, the others by their class, and those who fall at 0 HP but survive the
/// fatal blow.
fn to_watch(fights: &[FightReport], state: &GameState, data: &DesignData) -> Vec<Watched> {
    let mut seeds = Vec::new();
    let mut watched = Vec::new();
    for report in fights {
        let fight = &report.fight;
        if seeds.contains(&fight.seed) {
            continue;
        }
        seeds.push(fight.seed);
        let mine: Vec<&FightReport> = fights
            .iter()
            .filter(|f| f.fight.seed == fight.seed)
            .collect();
        let names = (0..fight.setup.fighters.len())
            .map(|k| {
                let id = FighterId(u32::try_from(k).unwrap_or(u32::MAX));
                if let Some(club) = mine.iter().find(|f| f.fight.me == id) {
                    let i = club.fighter;
                    return state
                        .fighters
                        .get(i)
                        .map_or_else(|| "?".into(), |f| fighter_name(i, f, data));
                }
                fight
                    .setup
                    .fighters
                    .get(k)
                    .map_or_else(|| "?".into(), |o| class_name(o, data))
            })
            .collect();
        let survivors = (0..)
            .zip(fight.survived_fatal_blow.iter().copied())
            .filter(|&(_, survived)| survived)
            .map(|(id, _)| FighterId(id))
            .collect();
        watched.push(Watched {
            arena: fight.start(data),
            cast: Cast {
                names,
                club: mine.iter().map(|f| f.fight.me).collect(),
                survivors,
            },
        });
    }
    watched
}

/// What each save slot in `folder` holds; all empty without a folder.
fn read_slots(folder: Option<&std::path::Path>, data: &DesignData) -> [Slot; saves::SLOTS.len()] {
    saves::SLOTS.map(|file| match folder.map(|f| saves::read(f, file, data)) {
        None | Some(Ok(None)) => Slot::Empty,
        Some(Ok(Some(state))) => Slot::Game(state.week, state.name, state.wealth, state.continent),
        Some(Err(_)) => Slot::Unreadable,
    })
}

/// A seed for a new game or a drawn name: the game, unlike `lanista_core`, may read
/// the clock.
fn new_seed() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs() ^ u64::from(d.subsec_nanos()))
}

/// A fighter's week, in one line: rest, training, a fight at home or, in a list, abroad,
/// a title fight or a received challenge (in `special`, with their names), and care at
/// the price of the club's `care`, all within the money of the club's `state`.
fn plan_editor(
    ui: &mut egui::Ui,
    plan: &mut WeekPlan,
    fighter: &CareerFighter,
    special: &[(Activity, String)],
    (state, care): (&GameState, &CareRules),
    data: &DesignData,
) {
    // The loadout, changed freely from one week to the next.
    let loadouts = data.class(fighter.class).map_or(0, |c| c.loadouts.len());
    ui.horizontal(|ui| {
        ui.label(tr!("plan-loadout"));
        for loadout in 0..loadouts {
            let name = loadout_name(fighter, loadout, data);
            ui.selectable_value(&mut plan.loadout, loadout, name);
        }
    });
    let money = state.money;
    for activity in [Activity::Rest, Activity::Train] {
        ui.selectable_value(&mut plan.activity, activity, activity_name(activity, data));
    }
    let mut abroad = Vec::new();
    for (id, rules) in (0..).map(ColosseumId).zip(&data.world.colosseums) {
        let activity = Activity::Fight(id);
        let fee = state.entry_fee(id, data).unwrap_or(0);
        let name = priced(activity, fee, data);
        // The week would turn into rest: `next_week` drops what the club cannot pay.
        let affordable = i64::from(fee) <= money;
        if !affordable && plan.activity == activity {
            plan.activity = Activity::Rest;
        }
        if rules.continent == state.continent {
            ui.add_enabled_ui(affordable, |ui| {
                ui.selectable_value(&mut plan.activity, activity, name);
            });
        } else {
            abroad.push((activity, name, affordable));
        }
    }
    let chosen = abroad.iter().find(|(a, ..)| *a == plan.activity);
    let text = chosen.map_or_else(
        || tr!("plan-abroad", travel = data.world.travel_fee),
        |(_, name, _)| name.clone(),
    );
    egui::ComboBox::from_id_salt("abroad")
        .selected_text(text)
        .show_ui(ui, |ui| {
            for (activity, name, affordable) in abroad {
                ui.add_enabled_ui(affordable, |ui| {
                    ui.selectable_value(&mut plan.activity, activity, name);
                });
            }
        });
    // A challenge the fighter sent is chosen in its sheet.
    if matches!(plan.activity, Activity::TitleFight(_) | Activity::Melee(_))
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
        let text = tr!("plan-care-unaffordable", cost = cost);
        ui.add_enabled(false, egui::Checkbox::new(&mut plan.care, text));
    } else {
        ui.checkbox(&mut plan.care, tr!("plan-care", cost = cost));
    }
    let fights = plan.activity.fights();
    // The colosseum's armour, refused for fame; nothing to refuse without a fight.
    if fights {
        let fame = data.fighter.fame.refused_armour;
        ui.checkbox(&mut plan.refused.armour, tr!("plan-no-armour"))
            .on_hover_text(tr!("plan-no-armour-effect", fame = fame));
    } else {
        plan.refused = Refused::NONE;
    }
    let ill = fighter.illness.is_some_and(|i| !i.fight_advised());
    if ill && fights {
        ui.colored_label(egui::Color32::LIGHT_RED, tr!("plan-fight-ill-advised"));
    }
}

/// The challenges the club's fighter `i` may send, one list per colosseum of the club's
/// continent or where it is ranked: the residents, best ranked first, those who would
/// refuse greyed.
fn challenge_picker(
    ui: &mut egui::Ui,
    plan: &mut WeekPlan,
    i: usize,
    state: &GameState,
    data: &DesignData,
) {
    ui.horizontal(|ui| {
        ui.label(tr!("plan-challenge"));
        for (c, colosseum) in (0..).zip(&state.colosseums) {
            let id = ColosseumId(c);
            let home = data
                .world
                .colosseum(id)
                .is_some_and(|r| r.continent == state.continent);
            let ranked = colosseum
                .ranking
                .iter()
                .any(|r| r.contender == Contender::Club(i));
            if !home && !ranked {
                continue;
            }
            let fee = state.entry_fee(id, data).unwrap_or(0);
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
                _ => colosseum_name(id, data),
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
                            let text = tr!(
                                "plan-challenge-rival",
                                name = name,
                                elo = rating.elo,
                                accepts = if accepts { "true" } else { "false" }
                            );
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
    ui.heading(tr!(
        "sheet-title",
        fighter = fighter_name(index, f, data),
        age = f.age
    ));
    if let Some(origin) = origin_text(f, data) {
        ui.label(origin);
    }
    let hand = match f.dominant_hand {
        _ if f.ambidextrous => tr!("sheet-ambidextrous"),
        Side::Right => tr!("sheet-right-handed"),
        Side::Left => tr!("sheet-left-handed"),
    };
    ui.label(hand);
    ui.label(tr!(
        "sheet-loadout",
        loadout = loadout_name(f, f.loadout, data)
    ));
    ui.label(tr!("sheet-ease", ease = ease_text(f, data)));
    match f.status {
        Status::Active => {}
        Status::Retired if too_maimed(&f.injuries, &rules.retirement) => {
            ui.colored_label(egui::Color32::GOLD, tr!("sheet-retired-maimed"));
        }
        Status::Retired => {
            ui.colored_label(egui::Color32::GOLD, tr!("sheet-retired"));
        }
        Status::Dead => {
            ui.colored_label(egui::Color32::LIGHT_RED, tr!("sheet-dead"));
        }
        Status::Sold => {
            ui.label(tr!("sheet-sold"));
        }
        Status::Released => {
            ui.label(tr!("sheet-released"));
        }
    }
    ui.add_space(8.0);
    gauge(ui, &tr!("roster-form"), f.form.value(), Form::MAX);
    gauge(ui, &tr!("roster-morale"), f.morale.value(), Morale::MAX);
    gauge(ui, &tr!("roster-fame"), f.fame.value(), Fame::MAX);

    ui.add_space(8.0);
    egui::Grid::new("stats").striped(true).show(ui, |ui| {
        ui.label("");
        ui.label(tr!("sheet-trained"));
        ui.label(tr!("sheet-fighting"));
        ui.end_row();
        let fighting = f.fighting_stats(rules).to_array();
        for ((name, trained), now) in stat_names()
            .into_iter()
            .zip(f.stats.to_array())
            .zip(fighting)
        {
            ui.label(name);
            ui.label(trained.to_string());
            ui.label(now.to_string());
            ui.end_row();
        }
    });

    ui.add_space(8.0);
    ui.label(personality_text(&f.personality));
    ui.add_space(8.0);
    ui.strong(tr!("sheet-injuries"));
    if f.injuries.is_empty() {
        ui.label(tr!("sheet-none"));
    }
    for injury in &f.injuries {
        ui.label(injury_text(*injury, f.dominant_hand));
    }
    ui.strong(tr!("sheet-illness"));
    ui.label(f.illness.map_or_else(|| tr!("sheet-none"), illness_text));
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
    use lanista_core::log::Report;
    use lanista_core::market::Move;
    use lanista_core::relations::{Change, Reason};
    use lanista_core::staff::{Candidate, Person, Role, Scouting, Staff, StaffMove};
    use lanista_core::state::{News, WorldNews};
    use rand_chacha::ChaCha8Rng;
    use rand_chacha::rand_core::SeedableRng;

    use super::*;
    use crate::texts::{
        club_name as rival_club_name, date_in_sentence, free_agent_name, named_club,
        outside_ease_text, outside_personality_text, potential_text, range_texts, staff_text,
        stage_name, unsellable_text, weeks_left_text,
    };
    use colosseums::reign_text;
    use lanista_core::colosseum::{Reign, ReignEnd, TitleLoss};

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
    fn a_new_club_bears_the_name_chosen_or_drawn_in_a_culture_of_its_continent() {
        let mut screen = screen(7);
        // The game's club, named as it opened, offered again for the next one.
        assert_eq!(screen.new_club.1, screen.state.name);
        assert!(
            screen
                .state
                .rivals
                .iter()
                .all(|r| r.name != screen.state.name)
        );
        // Another continent draws a culture of it and a name of that culture.
        screen.new_continent = Continent::Africa;
        screen.draw_new_club(None);
        let culture = screen.data.culture(&screen.new_club.0).unwrap();
        assert_eq!(culture.continent, Continent::Africa);
        assert!(culture.names.contains(&screen.new_club.1.name));
        // The player writes its own name.
        screen.new_club.1.name = "Spartacus".into();
        let chosen = screen.new_club.clone();
        screen.carry_out(Confirm::NewGame(Wealth::Rich, Continent::Africa));
        assert_eq!(
            (screen.state.culture.clone(), screen.state.name.clone()),
            chosen
        );
        assert_eq!(screen.state.continent, Continent::Africa);
        let club = named_club(&chosen.1);
        assert!(club.contains("Spartacus"), "{club}");
        assert!(screen.message.as_ref().unwrap().contains(&club));
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
        // A world whose fighter lives to a duel and a challenge.
        const SEED: u64 = 2;
        let mut screen = screen(SEED);
        // The season opens with Capua's melee, where its fighter would have fought.
        assert_eq!(
            screen.decisions.plans[0].activity,
            Activity::Melee(ColosseumId(0))
        );
        screen.next_week();
        assert_eq!(screen.view, View::Fights);
        let Watched {
            arena: mut fight,
            cast,
        } = screen.to_watch.remove(0);
        let survivors = cast.survivors;
        let report = &screen.fights[0];
        let melee = &report.fight;
        assert!(melee.is_melee());
        assert_eq!(fight.tick, 0);
        assert_eq!(fight.fighters.len(), melee.setup.fighters.len());
        let survived = melee.survived_fatal_blow.iter().filter(|&&s| s).count();
        assert_eq!(survivors.len(), survived);
        let me = usize::try_from(melee.me.0).unwrap();
        assert_eq!(fight.fighters[me].stats, {
            let mut rng = ChaCha8Rng::seed_from_u64(SEED);
            let f = CareerFighter::random(&mut rng, &screen.data);
            let at = Vec2::new(0.0, 0.0);
            f.combat_fighter(melee.me, Refused::NONE, &screen.data, at)
                .stats
        });
        play(&mut fight);
        assert!(fight.is_over() || fight.tick == fight.max_ticks);
        // The summary tells the very melee that was watched, which cost 30 form.
        assert_eq!(report.fight.report, Report::new(&fight));
        assert_eq!(report.form, (100, 70));
        // A duel week afterwards: one opponent, watched as fought.
        while screen.decisions.plans[0].activity != Activity::Fight(ColosseumId(0)) {
            assert!(screen.state.week < 10, "no fight");
            screen.next_week();
        }
        screen.next_week();
        let mut duel = screen.to_watch.remove(0).arena;
        let report = &screen.fights[0];
        assert!(!report.fight.is_melee() && report.fight.opponent().is_some());
        play(&mut duel);
        assert_eq!(report.fight.report, Report::new(&duel));
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
        let mine = fighter_name(0, &screen.state.fighters[0], &screen.data);
        assert!(
            screen
                .alerts()
                .iter()
                .any(|a| a.starts_with(&format!("{mine} : défi ")) && a.contains(&name)),
            "{:?}",
            screen.alerts()
        );
        screen.next_week();
        assert_eq!(screen.fights[0].fight.opponent(), Some(&resident));
        // A week of rest: nothing to watch, nothing to sum up.
        screen.decisions.plans[0].activity = Activity::Rest;
        screen.next_week();
        assert!(screen.to_watch.is_empty() && screen.fights.is_empty());
    }

    #[test]
    fn a_melee_the_club_s_fighters_shared_is_watched_once_with_their_names() {
        let mut screen = screen(3);
        let mut rng = ChaCha8Rng::seed_from_u64(9);
        for _ in 0..2 {
            let f = CareerFighter::random(&mut rng, &screen.data);
            screen.state.fighters.push(f);
            screen.histories.push(Vec::new());
        }
        let melee = WeekPlan {
            activity: Activity::Melee(ColosseumId(0)),
            care: false,
            refused: Refused::NONE,
            loadout: 0,
        };
        screen.decisions.plans = vec![melee; 3];
        screen.next_week();
        assert_eq!(screen.fights.len(), 3);
        assert_eq!(screen.to_watch.len(), 1);
        let cast = &screen.to_watch[0].cast;
        assert_eq!(cast.club.len(), 3);
        assert_eq!(cast.names.len(), screen.to_watch[0].arena.fighters.len());
        for report in &screen.fights {
            let i = report.fighter;
            let me = usize::try_from(report.fight.me.0).unwrap();
            let name = fighter_name(i, &screen.state.fighters[i], &screen.data);
            assert_eq!(cast.names[me], name);
        }
    }

    #[test]
    fn refused_equipment_is_kept_from_week_to_week_and_told() {
        let mut screen = screen(7);
        let armour = Refused { armour: true };
        screen.decisions.plans[0].activity = Activity::Fight(ColosseumId(0));
        screen.decisions.plans[0].refused = armour;
        screen.next_week();
        assert_eq!(screen.decisions.plans[0].refused, armour);
        let lines = &screen.histories[0][0].1;
        assert!(lines[1].contains(", sans armure :"), "{lines:?}");
    }

    #[test]
    fn the_loadout_chosen_is_kept_from_week_to_week() {
        let mut screen = screen(7);
        let other = 1 - screen.state.fighters[0].loadout;
        screen.decisions.plans[0].loadout = other;
        screen.next_week();
        assert_eq!(screen.state.fighters[0].loadout, other);
        assert_eq!(screen.decisions.plans[0].loadout, other);
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
        assert_eq!(first[0], "Mêlée à Capoue");
        assert!(first[1].starts_with("Mêlée à "), "{first:?}");
        assert!(
            first.iter().any(|l| l.starts_with("Forme 70 (-30)")),
            "{first:?}"
        );
        let (week, menu) = screen.last_week.as_ref().unwrap();
        assert_eq!(*week, 3);
        assert!(
            menu[0].starts_with("Argent ") && menu[0].contains("entretien -10"),
            "{menu:?}"
        );
        let name = fighter_name(0, &screen.state.fighters[0], &screen.data);
        assert!(menu[1].starts_with(&format!("{name} : ")), "{menu:?}");
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
        // The recruiter sees the observed fighter's potential, personality and ease.
        let state = &screen.state;
        let scouting = state.scouting();
        let observed = state.rival_fighter(id).unwrap();
        assert_eq!(observed.observed_weeks, 1);
        assert_ne!(potential_text(observed, &scouting), "?");
        assert!(outside_personality_text(observed, &scouting).starts_with("Agr "));
        let data = &screen.data;
        let ease = outside_ease_text(observed, &scouting, data);
        assert_eq!(ease, ease_text(observed, data));
        assert_eq!(ease.matches(", ").count() + 1, observed.ease.len());
        let unseen = &state.free_agents[0];
        assert_eq!(outside_personality_text(unseen, &scouting), "?");
        assert_eq!(outside_ease_text(unseen, &scouting, data), "?");
    }

    #[test]
    fn the_player_can_change_the_plan() {
        let mut screen = screen(7);
        screen.decisions.plans[0] = WeekPlan {
            activity: Activity::Rest,
            care: false,
            refused: Refused::NONE,
            loadout: 0,
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
        // The first free agent still on the market: the opening melee may have killed
        // one, or a rival club signed it.
        let agents = &screen.state.free_agents;
        let a = agents
            .iter()
            .position(|f| f.status == Status::Active)
            .unwrap();
        let signed = free_agent_name(a, &agents[a], &screen.data);
        screen.decisions.moves = vec![Move::Sign(a)];
        screen.next_week();
        assert_eq!(screen.state.fighters.len(), 2);
        assert_eq!(screen.decisions.plans.len(), 2);
        assert_eq!(screen.histories[1].len(), 1);
        assert_eq!(screen.decisions.moves, []);
        let (_, overview) = screen.last_week.as_ref().unwrap();
        let name = fighter_name(1, &screen.state.fighters[1], &screen.data);
        assert_eq!(overview[1], format!("Recrute {name}"));
        // Signed, it keeps its name.
        let stage = stage_name(&screen.state.fighters[1].name);
        assert!(signed.starts_with(&format!("{stage} (")), "{signed}");
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
        let owner = club_name(2, state);
        let shown = stage_name(&wanted.name);
        assert_eq!(
            move_text(Move::Offer(target, 100), state, &data),
            format!("Offrir 100 pour {shown} ({class}, {owner})")
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
            format!("{owner} refuse l'offre pour {shown} ({class}, {owner})")
        );
        assert_eq!(screen.state.transfer_list, [0]);
        // Offers came for the listed fighter.
        let offer = screen.state.sale_offers[0];
        let club = club_name(offer.rival, &screen.state);
        let name = fighter_name(0, &screen.state.fighters[0], &data);
        assert!(screen.alerts().contains(&format!(
            "{club} veut acheter {name} pour {} (à accepter au marché cette semaine)",
            offer.price
        )));
        screen.decisions.moves = vec![Move::AcceptOffer {
            fighter: 0,
            rival: offer.rival,
        }];
        let money = screen.state.money;
        screen.next_week();
        let (_, overview) = screen.last_week.as_ref().unwrap();
        let gain = i64::from(offer.price);
        assert_eq!(
            overview[0],
            format!("Argent {} (+{gain}) : vente +{gain}", money + gain)
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
        screen.save_slot(0);
        screen.next_week();
        screen.load(0);
        assert_eq!(screen.state, resumed.state);
        // Quitting right after loading resumes the loaded game, not the one before.
        assert_eq!(open().state, resumed.state);
        std::fs::remove_dir_all(folder).unwrap();
    }

    #[test]
    fn a_choice_that_loses_a_game_waits_for_confirmation() {
        let data = Rc::new(DesignData::load().unwrap());
        let folder = std::env::temp_dir().join(format!("lanista-slots-{}", std::process::id()));
        let mut screen = ClubScreen::open(
            Rc::clone(&data),
            7,
            Some(folder.clone()),
            Bindings::new(Vec::new()),
        );
        screen.next_week();
        // Saving to an empty slot loses nothing: it is done at once.
        screen.ask(Confirm::Save(1));
        assert_eq!(screen.confirm, None);
        let saved = screen.state.clone();
        assert_eq!(
            screen.slots,
            [
                Slot::Empty,
                Slot::Game(
                    saved.week,
                    saved.name.clone(),
                    saved.wealth,
                    saved.continent
                ),
                Slot::Empty
            ]
        );
        screen.next_week();
        // A new game cancelled leaves the game and its autosave as they were.
        screen.ask(Confirm::NewGame(Wealth::Rich, screen.state.continent));
        assert_eq!(screen.state.week, 3);
        screen.confirm = None;
        // Saving over a slot and loading wait for the player too.
        screen.ask(Confirm::Save(1));
        assert_eq!(screen.confirm, Some(Confirm::Save(1)));
        screen.confirm = None;
        screen.ask(Confirm::Load(1));
        assert_eq!(screen.state.week, 3);
        screen.carry_out(Confirm::Load(1));
        assert_eq!((screen.state.clone(), screen.confirm), (saved, None));
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
        // The journal comes back with the game: each fighter's weeks, the last week, and
        // its fights to watch again.
        assert_ne!(reloaded.histories[0], []);
        assert_eq!(reloaded.histories, screen.histories);
        assert_eq!(reloaded.last_week, screen.last_week);
        assert_eq!(reloaded.fights, screen.fights);
        let again = to_watch(&reloaded.fights, &reloaded.state, &data);
        assert_ne!(again, []);
        assert_eq!(again, to_watch(&screen.fights, &screen.state, &data));
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

    /// Phase 6 milestone: in the season's opening week, the club's three fighters fight
    /// a melee of 20 in Capua and duels in Capua and Rome; the three fights are watched
    /// together, each as the week played it, and the season goes on as without a screen.
    #[test]
    fn three_fights_are_watched_together_one_of_them_a_melee_of_twenty() {
        let data = Rc::new(DesignData::load().unwrap());
        let mut screen = screen(7);
        let mut rng = ChaCha8Rng::seed_from_u64(9);
        for _ in 0..2 {
            screen
                .state
                .fighters
                .push(CareerFighter::random(&mut rng, &data));
            screen.histories.push(Vec::new());
        }
        let mut headless = screen.state.clone();
        let plan = |activity| WeekPlan {
            activity,
            care: false,
            refused: Refused::NONE,
            loadout: 0,
        };
        let opening = Decisions {
            plans: vec![
                plan(Activity::Melee(ColosseumId(0))),
                plan(Activity::Fight(ColosseumId(0))),
                plan(Activity::Fight(ColosseumId(1))),
            ],
            ..Decisions::default()
        };
        screen.decisions = opening.clone();
        screen.next_week();
        next_week(&mut headless, &opening, &data);
        assert_eq!(screen.state, headless);
        assert_eq!(screen.to_watch.len(), 3);
        let melee = &screen.to_watch[0];
        assert_eq!(melee.arena.fighters.len(), 20);
        assert_eq!(melee.cast.club, [screen.fights[0].fight.me]);
        let fights = std::mem::take(&mut screen.to_watch);
        let mut watching = CombatScreen::watch(Rc::clone(&data), fights, Bindings::new(Vec::new()));
        let mut frame = 0;
        while watching.running() {
            // Uneven frames, as on screen: how a fight is shown never changes it.
            watching.advance([1.0 / 60.0, 1.0 / 144.0, 0.05][frame % 3] * 1.5);
            frame += 1;
        }
        for (view, report) in watching.views().iter().zip(&screen.fights) {
            assert_eq!(Report::new(&view.arena), report.fight.report);
            assert_eq!(view.arena.tick, report.fight.ticks);
        }
        // The rest of the season, on screen and without one.
        for _ in 1..data.calendar.season_weeks() {
            let decisions = screen.decisions.clone();
            screen.next_week();
            next_week(&mut headless, &decisions, &data);
        }
        assert_eq!(screen.state, headless);
        assert!(screen.state.opens_season(&data));
    }

    #[test]
    fn titles_are_told_in_the_menu() {
        let mut screen = screen(7);
        screen.state.colosseums[1].champion = Some(Contender::Club(0));
        let name = fighter_name(0, &screen.state.fighters[0], &screen.data);
        assert_eq!(
            screen.alerts(),
            [format!(
                "{name} : titre de Rome à défendre au plus tard en semaine 5"
            )]
        );
        // The rival champions defend in week 5, without the player.
        let mut screen = self::screen(7);
        for _ in 0..5 {
            screen.next_week();
        }
        let (_, menu) = screen.last_week.as_ref().unwrap();
        let clubs = &screen.state.rivals;
        assert!(
            menu.iter().any(|line| line.starts_with("Titre de ")
                && clubs
                    .iter()
                    .any(|c| line.contains(&format!(", {})", named_club(&c.name))))),
            "{menu:?}"
        );
    }

    #[test]
    fn the_rival_clubs_are_listed() {
        let screen = screen(7);
        let lines = screen.rival_lines();
        assert_eq!(lines.len(), screen.state.rivals.len());
        assert_eq!(
            lines[..3],
            [
                "Palestre d'Arrhichion (Grèce), agressif et pauvre : 10 combattants ; relation +0, neutre",
                "Dar Zénobie (Monde arabe), vétéran et pauvre : 10 combattants ; relation +0, neutre",
                "Clan de Cochise (Tribus amérindiennes), agressif et pauvre : 10 combattants ; relation +0, neutre",
            ]
        );
    }

    #[test]
    fn a_club_s_page_tells_why_its_relation_moved_newest_first() {
        let mut screen = screen(7);
        let victim = RivalId {
            club: 0,
            fighter: 2,
        };
        let relation = screen.state.relations.entry(0).or_default();
        relation.score = -25;
        relation.changes = vec![
            Change {
                week: 3,
                points: 5,
                why: Reason::Sold { fighter: 0 },
            },
            Change {
                week: 34,
                points: -30,
                why: Reason::Killed { fighter: 0, victim },
            },
        ];
        let fighter = fighter_name(0, &screen.state.fighters[0], &screen.data);
        let victim = rival_fighter_name(victim, &screen.state, &screen.data);
        assert_eq!(
            screen.relation_lines(0),
            [
                format!("-30 : {fighter} a tué {victim} (saison 2, semaine 4)"),
                format!("+5 : vous lui avez vendu {fighter} (saison 1, semaine 3)"),
            ]
        );
        assert!(screen.rival_lines()[0].ends_with("; relation -25, tendu"));
        assert_eq!(screen.relation_lines(1), Vec::<String>::new());
    }

    #[test]
    fn the_world_s_news_are_told_newest_first() {
        let mut screen = screen(7);
        screen.state.news = vec![
            News {
                week: 30,
                what: WorldNews::Closed(1),
            },
            News {
                week: 31,
                what: WorldNews::Founded(0),
            },
        ];
        assert_eq!(
            screen.news_lines(),
            [
                "Saison 2, semaine 1 : Fondation de Palestre d'Arrhichion (Grèce), en Europe",
                "Saison 1, semaine 30 : Dar Zénobie ferme, ruiné : ses combattants partent au marché",
            ]
        );
    }

    #[test]
    fn a_declared_rivalry_is_told_and_alerted() {
        let mut screen = screen(7);
        screen.state.relations.entry(1).or_default().rival_since = Some(4);
        screen.state.news = vec![News {
            week: 4,
            what: WorldNews::Rivalry(1, true),
        }];
        assert_eq!(
            screen.news_lines(),
            [
                "Saison 1, semaine 4 : Dar Zénobie se déclare votre rival : vos combats contre lui attirent les foules"
            ]
        );
        assert!(screen.alerts().contains(
            &"Dar Zénobie est votre rival depuis saison 1, semaine 4 : recettes à 150 %, renommée +3 par combat"
                .to_owned()
        ));
        assert!(screen.rival_lines()[1].ends_with("; relation +0, rival"));
    }

    /// Phase 6 bis milestone: over three seasons on screen, a player who fills the club
    /// and challenges, week after week, the club it gets along with least makes it a
    /// declared rival, whose page tells the fights that did it; meanwhile a club closes
    /// and another is founded, and every club's relation tells its reasons.
    #[test]
    fn three_seasons_where_a_club_becomes_a_rival_one_closes_and_one_is_founded() {
        let data = Rc::new(DesignData::load().unwrap());
        let mut screen = screen(3);
        let mut rivalry = None;
        let (mut closed, mut founded) = (false, false);
        for _ in 0..3 * u64::from(data.calendar.season_weeks()) {
            let state = &screen.state;
            if state.can_hire(&data)
                && let Some(a) = state
                    .free_agents
                    .iter()
                    .position(|f| f.status == Status::Active)
            {
                screen.decisions.moves.push(Move::Sign(a));
            }
            for (i, plan) in screen.decisions.plans.iter_mut().enumerate() {
                let Activity::Fight(id) = plan.activity else {
                    continue;
                };
                let open = (0..state.rivals.len()).filter(|&c| state.rivals[c].is_open());
                let target = open
                    .flat_map(|club| {
                        let fighters = 0..state.rivals[club].fighters.len();
                        fighters.map(move |fighter| RivalId { club, fighter })
                    })
                    .filter(|&rival| state.may_challenge(i, id, rival, &data))
                    .min_by_key(|rival| (state.relation(rival.club), rival.club));
                if let Some(rival) = target {
                    plan.activity = Activity::Challenge(id, rival);
                }
            }
            screen.next_week();
            let week = screen.state.week - 1;
            for news in screen.state.news.iter().filter(|n| n.week == week) {
                match news.what {
                    WorldNews::Rivalry(club, true) if rivalry.is_none() => {
                        rivalry = Some((club, screen.relation_lines(club)));
                    }
                    WorldNews::Closed(_) => closed = true,
                    WorldNews::Founded(_) => founded = true,
                    _ => {}
                }
            }
        }
        // The rival club's page, the week it declared itself, tells the fights between
        // the two clubs that brought it there.
        let (club, lines) = rivalry.unwrap();
        assert_ne!(lines, Vec::<String>::new());
        let changes = &screen.state.relations[&club].changes;
        assert!(changes.iter().all(|c| match c.why {
            Reason::Killed { victim: r, .. }
            | Reason::Beaten { rival: r, .. }
            | Reason::ChallengeRefused { rival: r, .. }
            | Reason::PactKept { rival: r, .. }
            | Reason::Betrayed { rival: r, .. } => r.club == club,
            _ => true,
        }));
        assert!(
            lines
                .iter()
                .any(|l| l.contains(" a battu ") || l.contains(" a tué ")),
            "{lines:?}"
        );
        assert!(closed && founded);
        // Every relation away from neutral tells why.
        for (&club, relation) in &screen.state.relations {
            assert!(relation.score == 0 || !screen.relation_lines(club).is_empty());
        }
    }

    #[test]
    fn a_fighter_who_just_joined_tells_when_it_may_be_sold() {
        let mut screen = screen(7);
        let (state, data) = (&mut screen.state, &screen.data);
        assert_eq!(unsellable_text(&state.fighters[0], state, data), None);
        state.fighters[0].joined = Some(1);
        assert_eq!(
            unsellable_text(&state.fighters[0], state, data).unwrap(),
            "Arrivé récemment : vendable à partir de la saison 1, semaine 16"
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
        let standings = screen.standings(Contender::Rival(gone));
        assert_eq!(standings.len(), screen.state.colosseums.len());
        assert!(standings.iter().all(|s| s == "non classé"));
    }

    #[test]
    fn a_colosseum_s_honours_tell_each_reign() {
        let mut screen = screen(7);
        let reign = screen.state.colosseums[0].reigns[0];
        let Contender::Rival(id) = reign.holder else {
            panic!("no rival champion in Capua");
        };
        let club = rival_club_name(id.club, &screen.state);
        let from = date_in_sentence(reign.from, &screen.data);
        assert_eq!(
            reign_text(&reign, &screen.state, &screen.data),
            format!("pour {club}, de {from} à aujourd'hui, aucune défense")
        );
        // The champion died with the title after two defenses.
        screen.state.rivals[id.club].fighters[id.fighter].status = Status::Dead;
        let died = Reign {
            defenses: 2,
            end: Some(ReignEnd {
                week: reign.from + 3,
                how: TitleLoss::Vacated,
            }),
            ..reign
        };
        let until = date_in_sentence(reign.from + 3, &screen.data);
        assert_eq!(
            reign_text(&died, &screen.state, &screen.data),
            format!("pour {club}, de {from} à {until}, 2 défenses, mort avec le titre")
        );
    }

    #[test]
    fn the_page_of_a_fighter_gone_elsewhere_is_where_it_went() {
        let mut screen = screen(7);
        let id = RivalId {
            club: 1,
            fighter: 0,
        };
        let gone = &mut screen.state.rivals[1].fighters[0];
        gone.status = Status::Released;
        *gone = gone.moved(Contender::FreeAgent(0));
        screen.go_to(View::Outside(Target::Rival(id)));
        assert_eq!(screen.view, View::Outside(Target::FreeAgent(0)));
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
            [format!(
                "{} : blessure, {arm}",
                fighter_name(0, &screen.state.fighters[0], &screen.data)
            )]
        );
        assert!(arm.starts_with("Bras droit"), "{arm}");
    }
}
