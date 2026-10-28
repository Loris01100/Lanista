//! The player's club, week after week: the general menu and its calendar, the fighter
//! list, a fighter's sheet, the summary of the week's fights, the newspaper, the colosseums, the market and the
//! staff (in their own modules), with the sheet of a fighter out of the club and the page
//! of a rival club. Fights are watched in the combat screen; the same seed and decisions
//! give the same game as the `career` tool. The game is saved after every week, and when
//! the player asks.

mod colosseums;
mod market;
mod roster;
mod staff;
mod wiki;

use std::path::PathBuf;

use colosseums::{ContinentPart, arrow, contender_view, ranking_recap};
#[cfg(test)]
use market::SortBy;
use market::{MarketFilter, MarketSort, MarketTab};
use std::rc::Rc;
use wiki::Entry;

use lanista_core::calendar::{CalendarRules, Date, SeasonPhase};
use lanista_core::colosseum::{ColosseumId, Contender, Continent};
use lanista_core::design_data::DesignData;
use lanista_core::economy::Flow;
use lanista_core::fighter::care::CareRules;
use lanista_core::fighter::career::{
    Activity, CareerFighter, FightSummary, Record, Status, WeekPlan,
};
use lanista_core::fighter::equipment::Refused;
use lanista_core::fighter::fame::{Fame, FightResult};
use lanista_core::fighter::form::Form;
use lanista_core::fighter::illness::IllnessLevel;
use lanista_core::fighter::injury::{Injury, Severity, Side};
use lanista_core::fighter::morale::Morale;
use lanista_core::fighter::retirement::too_maimed;
use lanista_core::fighter::{FighterId, Stats};
use lanista_core::journal::FightLine;
use lanista_core::market::{Move, Target};
use lanista_core::relations::{Club, MAX_SCORE};
use lanista_core::rivals::RivalId;
use lanista_core::staff::Role;
use lanista_core::state::{Decisions, GameState, WorldNews, next_week};
use macroquad::prelude::*;
use reesch::{Assets, Audio, Bindings, Screen, Transition, egui};

use crate::Action;
use crate::combat_screen::{CombatScreen, Watched};
use crate::fight_view::Cast;
use crate::lang::tr;
use crate::options;
use crate::saves;
use crate::texts::{
    Article, activity_name, against_text, challenger_text, change, class_name, club_name,
    colosseum_name, colosseum_rules_name, contender_name, continent_in, date_in_sentence,
    date_text, fight_text, fighter_name, illness_effect, illness_text, injury_effect, injury_text,
    loadout_name, market_news_text, money_line, move_text, named_club, origin_place, priced,
    result_text, rival_fighter_name, staff_move_text, staff_news_text, stage_name, stat_names,
    temperament_name, temperament_text, title_article, title_changed, trait_memory, trait_name,
    trait_text, wealth_name, wealth_text, week_lines, world_article, year_text,
};
use crate::theme;
use crate::title_screen::{Slot, read_slots, slot_text};

const BACKGROUND: Color = Color::new(0.16, 0.13, 0.11, 1.0);
/// Ranks shown in a colosseum's ranking; the others are unranked.
const RANKS_SHOWN: usize = 50;
/// Width of the game's window, as the options'.
const GAME_WIDTH: f32 = 420.0;
/// Widest a page of the newspaper gets, margins included.
const PAPER_WIDTH: f32 = 960.0;
/// Width from which the newspaper sets its last issue in two columns.
const PAPER_COLUMNS_FROM: f32 = 720.0;
/// The phases of a season, in order, as the calendar's legend shows them.
const PHASES: [SeasonPhase; 3] = [
    SeasonPhase::Competition,
    SeasonPhase::GreatEvent,
    SeasonPhase::OffSeason,
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum View {
    /// The general menu: the season's calendar, alerts, the last week's paper.
    Menu,
    /// The fighter list, each fighter's week decided in one click.
    Roster,
    /// The sheet of the fighter at this index of the club.
    Sheet(usize),
    /// The summary of last week's fights.
    Fights,
    /// The newspaper, the Acta Diurna, one issue at a time.
    Paper,
    /// One continent's colosseums (arena, prestige, purses, champion and ranking) or
    /// its rival clubs.
    Colosseums,
    /// Free agents, rival fighters for sale, and the club's fighters to sell or release.
    Market,
    /// The trainer, the doctor and the recruiter, hired and dismissed.
    Staff,
    /// What the club sees of a free agent or a rival club's fighter.
    Outside(Target),
    /// The rival club at this index: profile, money, titles and fighters.
    Rival(usize),
    /// The game's encyclopedia, open on this page.
    Wiki(Entry),
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
    /// The views opened before the current one, the last the first « Retour » comes
    /// back to.
    history: Vec<View>,
    /// The views « Retour » left, the last the first « Suivant » opens again.
    forward: Vec<View>,
    /// Each fighter's weeks, newest first, in club order.
    histories: Vec<Vec<(u64, Vec<String>)>>,
    /// The last week played: its number, the money line, then one line per fighter.
    last_week: Option<(u64, Vec<String>)>,
    /// The last week's title fights and titles changing hands, each told whether the
    /// title changed hands.
    week_titles: Vec<(bool, Article)>,
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
    /// The slot waiting for the player to confirm saving over it.
    confirm: Option<usize>,
    /// The offer being written on the market: the rival fighter and the price.
    offer_draft: Option<(RivalId, u32)>,
    /// The signing or renewal being negotiated, in its window, with the terms offered.
    terms_draft: Option<Move>,
    /// The club's fighter the sheet of a fighter out of the club is compared with.
    compared: Option<usize>,
    /// Which fighters out of the club the market lists.
    market_filter: MarketFilter,
    /// How the market's lists are sorted.
    market_sort: MarketSort,
    /// The market's tab shown.
    market_tab: MarketTab,
    /// The post the staff view shows.
    shown_role: Role,
    /// What the encyclopedia's search looks for; empty, the pages of the category shown.
    wiki_search: String,
    /// The week of the newspaper's issue shown; `None` shows the last one.
    shown_issue: Option<u64>,
    /// Whether the options are laid over the screen.
    options_open: bool,
    /// Whether the game's window (save slots, main menu) is laid over the screen.
    game_open: bool,
    /// The continent the colosseums view shows, the club's at first.
    shown_continent: Continent,
    /// The prestige of the colosseums the colosseums view shows; `None` shows them all.
    shown_prestige: Option<u32>,
    /// Whether the colosseums view shows the continent's colosseums or its rival clubs.
    shown_part: ContinentPart,
    bindings: Bindings<Action>,
}

impl ClubScreen {
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
            shown_part: ContinentPart::Colosseums,
            slots: read_slots(saves.as_deref(), &data),
            data,
            state,
            view: View::Menu,
            history: Vec::new(),
            forward: Vec::new(),
            last_week: None,
            week_titles: Vec::new(),
            fights: Vec::new(),
            to_watch: Vec::new(),
            saves,
            message: None,
            confirm: None,
            offer_draft: None,
            terms_draft: None,
            compared: None,
            market_filter: MarketFilter::default(),
            market_sort: MarketSort::default(),
            market_tab: MarketTab::FreeAgents,
            shown_role: Role::Trainer,
            wiki_search: String::new(),
            shown_issue: None,
            options_open: false,
            game_open: false,
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
        self.week_titles.clear();
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
        let titles = summary.titles.iter().map(|&(c, news)| {
            let article = title_article(played.week, c, news, &self.state, data);
            (title_changed(news), article)
        });
        self.week_titles = titles.collect();
        // Titles changing hands first, the player's club first among each.
        self.week_titles
            .sort_by_key(|(changed, article)| (!changed, !article.ours));
        self.last_week = Some((played.week, overview));
    }

    #[cfg(test)]
    pub fn state(&self) -> &GameState {
        &self.state
    }

    #[cfg(test)]
    pub fn message(&self) -> Option<&str> {
        self.message.as_deref()
    }

    /// Shows `message` to the player in the top bar.
    pub fn telling(mut self, message: String) -> Self {
        self.message = Some(message);
        self
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
        self.confirm = None;
        self.save(saves::SLOTS[slot], true);
        self.slots = read_slots(self.saves.as_deref(), &self.data);
    }

    /// Saves to the slot `slot` at once when it is empty, else asks the player to
    /// confirm saving over the game it holds.
    fn ask(&mut self, slot: usize) {
        if self.slots[slot] == Slot::Empty {
            self.save_slot(slot);
        } else {
            self.confirm = Some(slot);
        }
    }

    /// Plays the week with the current decisions; its fights are queued to be
    /// watched, then summed up.
    fn next_week(&mut self) {
        next_week(&mut self.state, &self.decisions, &self.data);
        self.read_journal();
        self.shown_issue = None;
        self.to_watch = to_watch(&self.fights, &self.state, &self.data);
        let before = self.view;
        if !self.fights.is_empty() {
            self.go_to(View::Fights);
        }
        // Free agents are new each week: the one on screen or in the history may be gone
        // or replaced. A rival club's fighter sold or let go is followed.
        let free_agent = |v: &View| matches!(v, View::Outside(Target::FreeAgent(_)));
        for views in [&mut self.history, &mut self.forward] {
            views.retain(|v| !free_agent(v));
        }
        for i in 0..self.history.len() {
            self.history[i] = self.followed(self.history[i]);
        }
        for i in 0..self.forward.len() {
            self.forward[i] = self.followed(self.forward[i]);
        }
        self.history.dedup();
        self.forward.dedup();
        if free_agent(&before) && self.view == before {
            self.view = View::Market;
        }
        self.view = self.followed(self.view);
        let refused: Vec<Refused> = self.decisions.plans.iter().map(|p| p.refused).collect();
        self.decisions = self.state.suggested_decisions(&self.data);
        // A fighter keeps fighting without what it refused, until the player says otherwise.
        for (plan, refused) in self.decisions.plans.iter_mut().zip(refused) {
            plan.refused = refused;
        }
        // Saved before the fights are watched: quitting during one keeps its outcome.
        self.save(saves::AUTO, false);
    }

    /// The bar over every view; returns whether to play the week.
    fn top_bar(&mut self, ui: &mut egui::Ui) -> bool {
        let mut play = false;
        let date = self.state.date(&self.data);
        let phase = phase_key(date.phase(&self.data.calendar));
        // Wrapped: a narrow window or a large interface puts the end on a second row
        // rather than out of the screen.
        ui.horizontal_wrapped(|ui| {
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
            if arrow_link(ui, !self.history.is_empty(), egui::Vec2::LEFT, tr!("back")) {
                self.back();
            }
            if arrow_link(ui, !self.forward.is_empty(), egui::Vec2::RIGHT, tr!("next")) {
                self.next();
            }
            ui.separator();
            let views = [
                (View::Menu, "view-menu"),
                (View::Roster, "view-roster"),
                (View::Fights, "view-fights"),
                (View::Paper, "view-paper"),
                (View::Colosseums, "view-colosseums"),
                (View::Market, "view-market"),
                (View::Staff, "view-staff"),
            ];
            for (view, key) in views {
                let label = egui::SelectableLabel::new(self.view == view, tr!(key));
                let enabled = view != View::Fights || !self.fights.is_empty();
                if ui.add_enabled(enabled, label).clicked() {
                    self.go_to(view);
                }
            }
            ui.separator();
            let open = matches!(self.view, View::Wiki(_));
            let wiki = egui::SelectableLabel::new(open, tr!("view-wiki"));
            if ui.add(wiki).on_hover_text(tr!("view-wiki-hint")).clicked() {
                self.go_to(View::Wiki(Entry::CATEGORIES[0]));
            }
            ui.toggle_value(&mut self.game_open, tr!("game"));
            ui.toggle_value(&mut self.options_open, tr!("options"));
            ui.separator();
            // On every view, as the game's main action, so the week is played from where
            // its decisions are taken.
            let next = egui::Button::new(egui::RichText::new(tr!("menu-next-week")).strong())
                .fill(theme::BRONZE.gamma_multiply(0.5));
            play = ui.add(next).clicked();
        });
        // On its own line: a long message would push the views out of the bar.
        if let Some(message) = &self.message {
            ui.label(message);
        }
        play
    }

    /// The general menu.
    fn menu(&mut self, ui: &mut egui::Ui) {
        ui.heading(tr!("menu-title"));
        ui.label(tr!(
            "menu-club",
            club = named_club(&self.state.name),
            wealth = wealth_text(self.state.wealth),
            continent = continent_in(self.state.continent)
        ));
        ui.add_space(6.0);
        self.calendar(ui);
        let alerts = self.alerts();
        if !alerts.is_empty() {
            ui.add_space(6.0);
            ui.strong(tr!("menu-alerts"));
            for (alert, view) in alerts {
                // A link to where the player acts on it.
                if ui
                    .link(egui::RichText::new(alert).color(egui::Color32::LIGHT_RED))
                    .clicked()
                {
                    if view == View::Market {
                        // To sell, or to recruit when the club has nobody left.
                        self.market_tab = if self.state.active_fighters() == 0 {
                            MarketTab::FreeAgents
                        } else {
                            MarketTab::Yours
                        };
                    }
                    self.go_to(view);
                }
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
        if self.last_week.is_some() && ui.button(tr!("menu-read-paper")).clicked() {
            self.shown_issue = None;
            self.go_to(View::Paper);
        }
    }

    /// The season as a Roman calendar: its year since the founding of Rome in Roman
    /// numerals, then a cell per week in its phase's colour, the weeks played faded and
    /// the week to play ringed, and the phases' legend.
    fn calendar(&self, ui: &mut egui::Ui) {
        let rules = &self.data.calendar;
        let today = self.state.date(&self.data);
        let weeks = rules.season_weeks();
        egui::Frame::group(ui.style())
            .fill(ui.visuals().extreme_bg_color)
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.horizontal_wrapped(|ui| {
                    let year = year_text(self.state.week, &self.data).to_uppercase();
                    ui.label(egui::RichText::new(year).heading().color(theme::BRONZE));
                    let season = tr!(
                        "calendar-season",
                        season = today.season,
                        week = today.week,
                        weeks = weeks
                    );
                    ui.label(egui::RichText::new(season).weak());
                });
                let gap = 4.0;
                let count = f32::from(u16::try_from(weeks).unwrap_or(u16::MAX).max(1));
                let side = ((ui.available_width() - gap * (count - 1.0)) / count).clamp(24.0, 48.0);
                ui.horizontal_wrapped(|ui| {
                    ui.spacing_mut().item_spacing = egui::vec2(gap, gap);
                    for week in 1..=weeks {
                        let date = Date {
                            season: today.season,
                            week,
                        };
                        week_cell(ui, date, today, side, rules);
                    }
                });
                ui.horizontal_wrapped(|ui| {
                    for phase in PHASES {
                        let (rect, _) =
                            ui.allocate_exact_size(egui::vec2(14.0, 14.0), egui::Sense::hover());
                        ui.painter().rect_filled(rect, 3.0, phase_color(phase));
                        let name = tr!("phase-name", phase = phase_key(phase));
                        ui.label(egui::RichText::new(name).small());
                    }
                });
            });
    }

    /// The weeks with an issue of the newspaper, newest first: the last week played, then
    /// each past week with news.
    fn issues(&self) -> Vec<u64> {
        let played = self.last_week.as_ref().map(|(week, _)| *week);
        let past = self.news_pages().into_iter().map(|(week, _)| week);
        played
            .into_iter()
            .chain(past.filter(|&week| Some(week) != played))
            .collect()
    }

    /// The club's newspaper, the Acta Diurna, one issue at a time on a page of
    /// parchment, « Numéro précédent » and « Numéro suivant » above it. The last
    /// issue tells the club's week, the week's title fights and the world's other news;
    /// a past one, the world's news of its week.
    fn paper(&mut self, ui: &mut egui::Ui) {
        let issues = self.issues();
        let shown = self
            .shown_issue
            .and_then(|week| issues.iter().position(|&w| w == week))
            .unwrap_or(0);
        ui.horizontal(|ui| {
            let older = shown + 1 < issues.len();
            if ui
                .add_enabled(older, egui::Button::new(tr!("paper-older")))
                .clicked()
            {
                self.shown_issue = issues.get(shown + 1).copied();
            }
            if ui
                .add_enabled(shown > 0, egui::Button::new(tr!("paper-newer")))
                .clicked()
            {
                self.shown_issue = issues.get(shown - 1).copied();
            }
            if shown > 0 && ui.button(tr!("paper-latest")).clicked() {
                self.shown_issue = None;
            }
        });
        let week = issues.get(shown).copied();
        let latest = shown == 0 && self.last_week.is_some();
        egui::ScrollArea::vertical().show(ui, |ui| {
            parchment(ui, |ui| {
                self.masthead(ui, week);
                match week {
                    None => {
                        ui.label(egui::RichText::new(tr!("paper-no-issue")).italics());
                    }
                    Some(_) if latest => self.last_issue(ui),
                    Some(week) => {
                        let pages = self.news_pages();
                        let articles = pages.iter().find(|(w, _)| *w == week);
                        paper_section(ui, &tr!("paper-world"));
                        for article in articles.into_iter().flat_map(|(_, a)| a) {
                            article_view(ui, article, true);
                        }
                    }
                }
            });
        });
    }

    /// The newspaper's name, then the issue's date and year between two rules.
    fn masthead(&self, ui: &mut egui::Ui, week: Option<u64>) {
        ui.vertical_centered(|ui| {
            let name = egui::RichText::new(tr!("paper-name").to_uppercase())
                .size(44.0)
                .strong()
                .extra_letter_spacing(6.0);
            ui.label(name);
            let issue = week.map_or_else(
                || tr!("paper-first-issue"),
                |week| {
                    tr!(
                        "paper-issue",
                        date = date_text(week, &self.data),
                        year = year_text(week, &self.data)
                    )
                },
            );
            ui.separator();
            ui.label(egui::RichText::new(issue).italics());
            ui.separator();
        });
    }

    /// The last issue: the club's week beside the week's title fights and the world's
    /// other news, in two columns on a wide enough page.
    fn last_issue(&self, ui: &mut egui::Ui) {
        let club = |ui: &mut egui::Ui| {
            paper_section(ui, &tr!("paper-club"));
            for line in self.last_week.iter().flat_map(|(_, lines)| lines) {
                ui.label(line);
            }
        };
        let world = |ui: &mut egui::Ui| {
            let (changed, defended): (Vec<_>, Vec<_>) =
                self.week_titles.iter().partition(|(changed, _)| *changed);
            if !self.week_titles.is_empty() {
                paper_section(ui, &tr!("paper-titles"));
                for (_, article) in changed {
                    article_view(ui, article, true);
                }
                if !defended.is_empty() {
                    ui.add_space(4.0);
                    ui.label(egui::RichText::new(tr!("paper-defended")).italics());
                    for (_, article) in defended {
                        article_view(ui, article, false);
                    }
                }
            }
            let played = self.last_week.as_ref().map(|(week, _)| *week);
            let pages = self.news_pages();
            if let Some((_, articles)) = pages.iter().find(|(week, _)| Some(*week) == played) {
                paper_section(ui, &tr!("paper-world"));
                for article in articles {
                    article_view(ui, article, true);
                }
            }
        };
        if ui.available_width() >= PAPER_COLUMNS_FROM {
            ui.columns(2, |columns| {
                club(&mut columns[0]);
                world(&mut columns[1]);
            });
        } else {
            club(ui);
            world(ui);
        }
    }

    /// The game's window over the screen while `game_open`: each save slot with
    /// « Sauvegarder », saving over a game once the player confirms it, then « Menu
    /// principal »; returns whether to go back to the main menu.
    fn game_window(&mut self, ui: &egui::Context) -> bool {
        if !self.game_open {
            return false;
        }
        let mut leave = false;
        let mut close = false;
        let modal = egui::Modal::new(egui::Id::new("game")).show(ui, |ui| {
            ui.set_width(GAME_WIDTH);
            ui.heading(tr!("game"));
            if self.saves.is_some() {
                self.save_slots(ui);
                ui.separator();
            }
            ui.horizontal(|ui| {
                leave = ui.button(tr!("main-menu")).clicked();
                close = ui.button(tr!("close")).clicked();
            });
        });
        if close || modal.should_close() {
            self.game_open = false;
            self.confirm = None;
        }
        leave
    }

    /// Each save slot with « Sauvegarder », or the question before saving over one.
    fn save_slots(&mut self, ui: &mut egui::Ui) {
        if let Some(slot) = self.confirm {
            ui.colored_label(
                egui::Color32::LIGHT_RED,
                tr!(
                    "confirm-overwrite",
                    slot = slot + 1,
                    game = slot_text(&self.slots[slot], &self.data)
                ),
            );
            ui.horizontal(|ui| {
                if ui.button(tr!("confirm")).clicked() {
                    self.save_slot(slot);
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
                if ui.button(tr!("menu-save")).clicked() {
                    chosen = Some(i);
                }
                ui.label(tr!(
                    "save-slot",
                    slot = i + 1,
                    game = slot_text(slot, &self.data)
                ));
            });
        }
        if let Some(slot) = chosen {
            self.ask(slot);
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

    /// The world's news grouped by week, newest first; the last week played leaves out
    /// its titles, which the week's title fights tell.
    fn news_pages(&self) -> Vec<(u64, Vec<Article>)> {
        let played = self.last_week.as_ref().map(|(week, _)| *week);
        let mut pages: Vec<(u64, Vec<Article>)> = Vec::new();
        for &news in self.state.news.iter().rev() {
            if Some(news.week) == played && matches!(news.what, WorldNews::Title(..)) {
                continue;
            }
            let article = world_article(news, &self.state, &self.data);
            match pages.last_mut() {
                Some((week, articles)) if *week == news.week => articles.push(article),
                _ => pages.push((news.week, vec![article])),
            }
        }
        pages
    }

    /// What needs the player's attention before the week, each with the view where
    /// the player acts on it.
    fn alerts(&self) -> Vec<(String, View)> {
        let mut alerts = Vec::new();
        if self.state.money < 0 {
            alerts.push((tr!("alert-debts", money = self.state.money), View::Market));
        }
        for (i, f) in self.state.fighters.iter().enumerate() {
            if f.status != Status::Active {
                continue;
            }
            let name = fighter_name(i, f, &self.data);
            for injury in &f.injuries {
                if matches!(injury.severity, Severity::Medium | Severity::Severe) {
                    let injury = injury_text(*injury, f.dominant_hand);
                    let text = tr!("alert-injury", fighter = name.as_str(), injury = injury);
                    alerts.push((text, View::Sheet(i)));
                }
            }
            if let Some(illness) = f.illness.filter(|i| i.level != IllnessLevel::Mild) {
                let text = tr!(
                    "labelled",
                    label = name.as_str(),
                    text = illness_text(illness)
                );
                alerts.push((text, View::Sheet(i)));
            }
            if f.form.value() < lanista_core::fighter::career::REST_BELOW_FORM {
                let text = tr!(
                    "alert-tired",
                    fighter = name.as_str(),
                    form = f.form.value()
                );
                alerts.push((text, View::Sheet(i)));
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
                let text = tr!(
                    "alert-title-defense",
                    fighter = fighter_name(i, f, &self.data),
                    place = colosseum_rules_name(rules),
                    week =
                        date_in_sentence(colosseum.defense_deadline(&self.data.world), &self.data)
                );
                alerts.push((text, View::Sheet(i)));
            }
        }
        for offer in &self.state.sale_offers {
            if let Some(f) = self.state.fighters.get(offer.fighter) {
                let text = tr!(
                    "alert-sale-offer",
                    club = club_name(offer.rival, &self.state),
                    fighter = fighter_name(offer.fighter, f, &self.data),
                    price = offer.price
                );
                alerts.push((text, View::Market));
            }
        }
        let rivalry = &self.data.world.relations.rivalry;
        for (&club, relation) in &self.state.relations {
            if let Some(week) = relation.rival_since {
                let text = tr!(
                    "alert-rival",
                    club = club_name(club, &self.state),
                    since = date_in_sentence(week, &self.data),
                    receipts = rivalry.receipts_percent,
                    fame = rivalry.fame
                );
                alerts.push((text, View::Rival(club)));
            }
        }
        if self.state.active_fighters() == 0 {
            alerts.push((tr!("alert-no-fighter"), View::Market));
        }
        alerts
    }

    /// The title fights the club's fighter `i` may have this week and, in the season's
    /// opening week, the melee of each colosseum of the
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
        melees.chain(titles).collect()
    }

    fn sheet(&mut self, ui: &mut egui::Ui, index: usize) {
        let data = &self.data;
        let Some(f) = self.state.fighters.get(index) else {
            self.view = View::Roster;
            return;
        };
        let special = self.special_fights(index);
        if ui.button(tr!("back-to-list")).clicked() {
            // As `go_to`, which the borrowed fighter keeps from being called.
            self.history.push(self.view);
            self.forward.clear();
            self.view = View::Roster;
        }
        ui.columns(2, |columns| {
            sheet(&mut columns[0], index, f, best_stats(&self.state), data);
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
                        week_card(ui, *week, lines);
                    }
                });
        });
    }

    /// The fight summary, a card per fight, two side by side; returns the seed of a
    /// fight to watch again.
    fn fight_summary(&self, ui: &mut egui::Ui) -> Option<u64> {
        let mut again = None;
        ui.heading(tr!("view-fights"));
        egui::ScrollArea::vertical().show(ui, |ui| {
            for (row, pair) in self.fights.chunks(2).enumerate() {
                ui.columns(2, |columns| {
                    for (column, report) in columns.iter_mut().zip(pair) {
                        if fight_card(column, report, &self.state, &self.data) {
                            again = Some(report.fight.seed);
                        }
                    }
                });
                if row + 1 < self.fights.len().div_ceil(2) {
                    ui.add_space(8.0);
                }
            }
            let played = self.state.journal.last.iter();
            for (id, before) in played.flat_map(|w| &w.summary.rankings_before) {
                ui.add_space(8.0);
                ranking_recap(ui, *id, before, &self.state, &self.data);
            }
        });
        again
    }

    /// Opens `view` from the current one, which « Retour » comes back to; « Suivant »
    /// has nothing left to open again, as in a browser.
    fn go_to(&mut self, view: View) {
        let view = self.followed(view);
        if view != self.view {
            self.history.push(self.view);
            self.forward.clear();
            self.view = view;
        }
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

    /// Back to the view the current one was opened from.
    fn back(&mut self) {
        if let Some(view) = self.history.pop() {
            self.forward.push(self.view);
            self.view = view;
        }
    }

    /// Opens again the view « Retour » left.
    fn next(&mut self) {
        if let Some(view) = self.forward.pop() {
            self.history.push(self.view);
            self.view = view;
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
        // Échap and Espace belong to the windows laid over the screen while they are open.
        let keys = !self.options_open && !self.game_open && self.terms_draft.is_none();
        if keys && self.bindings.pressed(ui, &Action::Quit) {
            if self.view == View::Menu {
                return Transition::Pop;
            }
            self.go_to(View::Menu);
        }
        // The mouse's back and forward buttons, as in a browser.
        if ui.input(|i| i.pointer.button_pressed(egui::PointerButton::Extra1)) {
            self.back();
        }
        if ui.input(|i| i.pointer.button_pressed(egui::PointerButton::Extra2)) {
            self.next();
        }
        let mut play = keys && self.bindings.pressed(ui, &Action::NextWeek);
        let mut again = None;
        egui::TopBottomPanel::top("club").show(ui, |ui| play |= self.top_bar(ui));
        egui::CentralPanel::default().show(ui, |ui| match self.view {
            View::Menu => self.menu(ui),
            View::Roster => self.roster(ui),
            View::Sheet(i) => self.sheet(ui, i),
            View::Fights => again = self.fight_summary(ui),
            View::Paper => self.paper(ui),
            View::Colosseums => self.colosseums(ui),
            View::Market => self.market(ui),
            View::Staff => self.staff(ui),
            View::Outside(target) => self.outside(ui, target),
            View::Rival(club) => self.rival_club(ui, club),
            View::Wiki(entry) => self.wiki(ui, entry),
        });
        if let Some(failed) =
            options::window(ui, audio, self.saves.as_deref(), &mut self.options_open)
        {
            self.message = Some(failed);
        }
        self.terms_window(ui);
        if self.game_window(ui) {
            return Transition::Pop;
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

/// An arrow along `dir` to click, `text` shown on hover, faded when not `enabled`.
fn arrow_link(ui: &mut egui::Ui, enabled: bool, dir: egui::Vec2, text: String) -> bool {
    ui.add_enabled_ui(enabled, |ui| {
        let color = ui.visuals().text_color();
        arrow(ui, dir, color)
            .on_hover_text(text)
            .on_hover_cursor(egui::CursorIcon::PointingHand)
            .clicked()
    })
    .inner
}

/// One week of a fighter's history in a frame: the week in bronze beside what it did,
/// what came of it below in small print.
fn week_card(ui: &mut egui::Ui, week: u64, lines: &[String]) {
    egui::Frame::group(ui.style())
        .fill(ui.visuals().extreme_bg_color)
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            let mut lines = lines.iter();
            ui.horizontal(|ui| {
                let title = tr!("week-title", week = week);
                ui.label(egui::RichText::new(title).strong().color(theme::BRONZE));
                if let Some(what) = lines.next() {
                    ui.strong(what);
                }
            });
            for line in lines {
                ui.label(egui::RichText::new(line).small());
            }
        });
}

/// A week of the season's calendar, `side` wide and high: its number on its phase's
/// colour, faded once played, ringed when it is `today`; its date and phase on hover.
fn week_cell(ui: &mut egui::Ui, date: Date, today: Date, side: f32, rules: &CalendarRules) {
    let phase = date.phase(rules);
    let (rect, response) = ui.allocate_exact_size(egui::vec2(side, side), egui::Sense::hover());
    let played = date < today;
    let mut fill = phase_color(phase);
    if played {
        fill = fill.gamma_multiply(0.3);
    }
    let painter = ui.painter();
    painter.rect_filled(rect, 4.0, fill);
    if date == today {
        let ring = egui::Stroke::new(3.0_f32, ui.visuals().text_color());
        painter.rect_stroke(rect, 4.0, ring, egui::StrokeKind::Outside);
    }
    let ink = if played {
        ui.visuals().weak_text_color()
    } else {
        theme::INK
    };
    painter.text(
        rect.center(),
        egui::Align2::CENTER_CENTER,
        date.week.to_string(),
        egui::FontId::proportional(14.0),
        ink,
    );
    response.on_hover_text(tr!(
        "top-date",
        season = date.season,
        week = date.week,
        phase = phase_key(phase)
    ));
}

/// The key of `phase` in the language files.
fn phase_key(phase: SeasonPhase) -> &'static str {
    match phase {
        SeasonPhase::Competition => "competition",
        SeasonPhase::GreatEvent => "great-event",
        SeasonPhase::OffSeason => "off-season",
    }
}

/// The colour of `phase` in the season's calendar.
fn phase_color(phase: SeasonPhase) -> egui::Color32 {
    match phase {
        SeasonPhase::Competition => theme::BRONZE,
        SeasonPhase::GreatEvent => theme::BLOOD,
        SeasonPhase::OffSeason => theme::SEA,
    }
}

/// A section of a sheet: its name in bronze over a rule.
fn section(ui: &mut egui::Ui, name: &str) {
    ui.add_space(8.0);
    ui.label(egui::RichText::new(name).strong().color(theme::BRONZE));
    ui.separator();
}

/// A page of the newspaper: dark ink on parchment in a bronze frame, centred, no wider
/// than `PAPER_WIDTH`.
fn parchment(ui: &mut egui::Ui, add: impl FnOnce(&mut egui::Ui)) {
    let margin = 24;
    let width = ui.available_width().min(PAPER_WIDTH) - 2.0 * f32::from(margin);
    ui.vertical_centered(|ui| {
        egui::Frame::new()
            .fill(theme::PARCHMENT)
            .stroke(egui::Stroke::new(2.0_f32, theme::BRONZE))
            .corner_radius(4.0)
            .inner_margin(egui::Margin::same(margin))
            .show(ui, |ui| {
                ui.set_width(width);
                let visuals = ui.visuals_mut();
                visuals.override_text_color = Some(theme::INK);
                visuals.widgets.noninteractive.bg_stroke.color = theme::INK;
                ui.with_layout(egui::Layout::top_down(egui::Align::Min), add);
            });
    });
}

/// A section of the newspaper: its name in red ink, in capitals, over a rule.
fn paper_section(ui: &mut egui::Ui, name: &str) {
    ui.add_space(10.0);
    let name = egui::RichText::new(name.to_uppercase())
        .strong()
        .extra_letter_spacing(2.0)
        .color(theme::RED_INK);
    ui.label(name);
    ui.separator();
}

/// An article of the newspaper: its headline, in bold if `major`, and then who it is
/// about in small print; one about the player's club in red ink.
fn article_view(ui: &mut egui::Ui, article: &Article, major: bool) {
    let mut headline = egui::RichText::new(&article.headline);
    if major {
        headline = headline.strong();
    }
    if article.ours {
        headline = headline.color(theme::RED_INK);
    }
    ui.label(headline);
    if let (true, Some(byline)) = (major, &article.byline) {
        ui.label(egui::RichText::new(byline).small().weak());
    }
}

/// The week's fights to watch, in club order, each once, though several of the club's
/// fighters shared a melee: the fight before its first tick, the club's fighters named
/// and ringed, the others by name and class, everyone's Elo there, and those who fall
/// at 0 HP but survive the fatal blow.
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
                    .map_or_else(|| "?".into(), |o| outsider_name(o, data))
            })
            .collect();
        let elos = fight
            .setup
            .fighters
            .iter()
            .map(|f| elo_in(state, report.colosseum, f))
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
                elos,
            },
        });
    }
    watched
}

/// "Flamma (Augure)": a fighter of a fight not of the club, as it went in; by its class
/// if it has no name.
fn outsider_name(fighter: &CareerFighter, data: &DesignData) -> String {
    let class = class_name(fighter, data);
    if fighter.name.is_empty() {
        class
    } else {
        tr!(
            "fighter-name",
            name = stage_name(&fighter.name),
            class = class
        )
    }
}

/// `fighter`'s Elo in `colosseum` now, found by its number among those ranked there;
/// `None` if unranked.
fn elo_in(state: &GameState, colosseum: ColosseumId, fighter: &CareerFighter) -> Option<i32> {
    let rosters = state.rosters();
    state
        .colosseums
        .get(usize::try_from(colosseum.0).ok()?)?
        .ranking
        .iter()
        .find(|r| {
            rosters
                .fighter(r.contender)
                .is_some_and(|f| f.id == fighter.id)
        })
        .map(|r| r.elo)
}

/// Turns to rest a week the club cannot pay (`next_week` would drop it) or a special
/// fight no longer offered (not in `special`), and drops care and refused armour that
/// no longer apply: run every frame, the sheet open or not.
fn settle_plan(
    plan: &mut WeekPlan,
    fighter: &CareerFighter,
    special: &[(Activity, String)],
    (state, care): (&GameState, &CareRules),
    data: &DesignData,
) {
    if let Activity::Fight(id) = plan.activity
        && i64::from(state.entry_fee(id, data).unwrap_or(0)) > state.money
    {
        plan.activity = Activity::Rest;
    }
    if matches!(plan.activity, Activity::TitleFight(_) | Activity::Melee(_))
        && !special.iter().any(|(a, _)| *a == plan.activity)
    {
        plan.activity = Activity::Rest;
    }
    let cost = fighter.care_cost(care);
    if cost == 0 || i64::from(cost) > state.money {
        plan.care = false;
    }
    if !plan.activity.fights() {
        plan.refused = Refused::NONE;
    }
}

/// A fighter's week in a few words, for the roster: "Repos, Double glaive, avec soins".
/// A fighter's week: rest, training, a fight at home or, in a list, abroad, a title
/// fight or a melee (in `special`, with their names), and care at the
/// price of the club's `care`, all within the money of the club's `state`.
fn plan_editor(
    ui: &mut egui::Ui,
    plan: &mut WeekPlan,
    fighter: &CareerFighter,
    special: &[(Activity, String)],
    club: (&GameState, &CareRules),
    data: &DesignData,
) {
    settle_plan(plan, fighter, special, club, data);
    let (state, care) = club;
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
        let affordable = i64::from(fee) <= money;
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
    for (activity, name) in special {
        ui.selectable_value(&mut plan.activity, *activity, name);
    }
    let cost = fighter.care_cost(care);
    if cost == 0 {
        // Nothing to heal.
    } else if i64::from(cost) > money {
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
                Activity::Challenge(at, rival) if at == id => {
                    challenger_text(id, rival, state, data)
                }
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
                            let grudge = state.rival_fighter(r).is_some_and(|o| {
                                state
                                    .fighters
                                    .get(i)
                                    .is_some_and(|me| me.bears_grudge(o.id))
                            });
                            let text = tr!(
                                "plan-challenge-rival",
                                name = name,
                                elo = rating.elo,
                                grudge = if grudge { "true" } else { "false" },
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

/// A fighter's sheet, drawn more than written: tags for who it is, bars for its
/// gauges, stats, ease and personality, coloured tags for its record and health.
fn sheet(ui: &mut egui::Ui, index: usize, f: &CareerFighter, best: Stats, data: &DesignData) {
    ui.heading(tr!(
        "sheet-title",
        fighter = fighter_name(index, f, data),
        age = f.age
    ));
    identity(ui, f, data);
    egui::ScrollArea::vertical()
        .id_salt("sheet")
        .show(ui, |ui| {
            section(ui, &tr!("sheet-condition"));
            egui::Grid::new("condition").show(ui, |ui| {
                for (name, value, max, color) in [
                    (tr!("roster-form"), f.form.value(), Form::MAX, theme::LAUREL),
                    (
                        tr!("roster-morale"),
                        f.morale.value(),
                        Morale::MAX,
                        theme::SEA,
                    ),
                    (tr!("roster-fame"), f.fame.value(), Fame::MAX, theme::BRONZE),
                ] {
                    ui.label(name);
                    meter(ui, value, max, color, None);
                    ui.label(value.to_string());
                    ui.end_row();
                }
                let tie = f.tie(Club::Player);
                ui.label(tr!("sheet-tie"))
                    .on_hover_text(tr!("sheet-tie-hint"));
                let filled = u32::try_from(tie + MAX_SCORE).unwrap_or(0);
                meter(ui, filled, 2 * MAX_SCORE.unsigned_abs(), theme::SEA, None);
                ui.label(tie.to_string());
                ui.end_row();
            });
            section(ui, &tr!("record-title"));
            egui::Grid::new("record").show(ui, |ui| {
                for (name, record) in [
                    (tr!("sheet-career"), f.record),
                    (tr!("sheet-season"), f.season_record),
                ] {
                    ui.label(name);
                    record_chips(ui, record);
                    ui.end_row();
                }
            });
            section(ui, &tr!("sheet-stats"));
            stat_meters(ui, f, best, data);
            section(ui, &tr!("sheet-ease-title"));
            egui::Grid::new("ease").show(ui, |ui| {
                let loadouts = data.class(f.class).map_or(0, |c| c.loadouts.len());
                for l in 0..loadouts {
                    let name = egui::RichText::new(loadout_name(f, l, data));
                    ui.label(if l == f.loadout { name.strong() } else { name });
                    meter(ui, f.ease_with(l), 100, theme::BRONZE, None);
                    ui.label(f.ease_with(l).to_string());
                    ui.end_row();
                }
            });
            section(ui, &tr!("personality-title"));
            let temperament = f.personality.temperament;
            ui.label(egui::RichText::new(temperament_name(temperament)).strong());
            ui.label(temperament_text(temperament));
            for &t in &f.traits {
                ui.label(egui::RichText::new(trait_name(t)).strong());
                ui.label(trait_text(t));
                if let Some(memory) = trait_memory(f, t) {
                    ui.label(memory);
                }
            }
            section(ui, &tr!("sheet-health"));
            health(ui, f, data);
        });
}

/// Where a fighter comes from, its hand, its loadout and, once its career is over,
/// why: one tag each.
fn identity(ui: &mut egui::Ui, f: &CareerFighter, data: &DesignData) {
    let rules = &data.fighter;
    let plain = ui.visuals().widgets.inactive.bg_fill;
    ui.horizontal_wrapped(|ui| {
        if let Some(origin) = origin_place(f, data) {
            chip(ui, origin, plain);
        }
        let hand = match f.dominant_hand {
            _ if f.ambidextrous => tr!("sheet-ambidextrous"),
            Side::Right => tr!("sheet-right-handed"),
            Side::Left => tr!("sheet-left-handed"),
        };
        chip(ui, hand, plain);
        chip(
            ui,
            loadout_name(f, f.loadout, data),
            theme::BRONZE.gamma_multiply(0.6),
        )
        .on_hover_text(tr!("sheet-loadout"));
        let status = match f.status {
            Status::Active => None,
            Status::Retired if too_maimed(&f.injuries, &rules.retirement) => {
                Some((tr!("sheet-retired-maimed"), theme::BRONZE))
            }
            Status::Retired => Some((tr!("sheet-retired"), theme::BRONZE)),
            Status::Dead => Some((tr!("sheet-dead"), theme::BLOOD)),
            Status::Sold => Some((tr!("sheet-sold"), plain)),
            Status::Released => Some((tr!("sheet-released"), plain)),
        };
        if let Some((text, fill)) = status {
            chip(ui, text, fill);
        }
    });
}

/// The best trained value of each stat among the world's active fighters: the club's,
/// the rival clubs' and the free agents.
fn best_stats(state: &GameState) -> Stats {
    state
        .fighters
        .iter()
        .chain(state.rivals.iter().flat_map(|r| &r.fighters))
        .chain(&state.free_agents)
        .filter(|f| f.status == Status::Active)
        .fold(Stats::default(), |best, f| best.zip(f.stats, u32::max))
}

/// Each stat as a bar: faded up to its trained value, full up to what it fights with,
/// full against `best`, the world's best trained value; then the value, and what
/// injuries or form take off it in red.
fn stat_meters(ui: &mut egui::Ui, f: &CareerFighter, best: Stats, data: &DesignData) {
    let fighting = f.fighting_stats(&data.fighter).to_array();
    egui::Grid::new("stats").show(ui, |ui| {
        for (((name, trained), now), best) in stat_names()
            .into_iter()
            .zip(f.stats.to_array())
            .zip(fighting)
            .zip(best.to_array())
        {
            ui.label(name);
            meter(ui, now, best, theme::BRONZE, Some(trained)).on_hover_text(tr!(
                "sheet-stat-hover",
                trained = trained,
                fighting = now,
                best = best
            ));
            ui.horizontal(|ui| {
                ui.label(trained.to_string());
                if now != trained {
                    let change = i64::from(now) - i64::from(trained);
                    let color = if change < 0 {
                        theme::BLOOD
                    } else {
                        theme::LAUREL
                    };
                    ui.label(
                        egui::RichText::new(format!("{change:+}"))
                            .small()
                            .color(color),
                    );
                }
            });
            ui.end_row();
        }
    });
}

/// Wins, losses and draws as green, red and plain tags.
fn record_chips(ui: &mut egui::Ui, record: Record) {
    let plain = ui.visuals().widgets.inactive.bg_fill;
    ui.horizontal(|ui| {
        chip(ui, tr!("record-wins", count = record.wins), theme::LAUREL);
        chip(
            ui,
            tr!("record-losses", count = record.losses),
            theme::BLOOD,
        );
        chip(ui, tr!("record-draws", count = record.draws), plain);
    });
}

/// A green « En bonne santé », or a red tag for each injury and a bronze one for an
/// illness, what each takes off the stats on hover.
fn health(ui: &mut egui::Ui, f: &CareerFighter, data: &DesignData) {
    ui.horizontal_wrapped(|ui| {
        if f.injuries.is_empty() && f.illness.is_none() {
            chip(ui, tr!("sheet-healthy"), theme::LAUREL);
        }
        for &injury in &f.injuries {
            chip(ui, injury_text(injury, f.dominant_hand), theme::BLOOD)
                .on_hover_text(injury_effect(injury, f.dominant_hand, data));
        }
        if let Some(illness) = f.illness {
            chip(ui, illness_text(illness), theme::BRONZE.gamma_multiply(0.6))
                .on_hover_text(illness_effect(illness, data));
        }
    });
}

/// One fight of the summary in a frame: who fought where and how it ended, the two
/// sides face to face stat by stat, then what it brought the club; returns whether
/// « Revoir le combat » was clicked.
fn fight_card(
    ui: &mut egui::Ui,
    report: &FightReport,
    state: &GameState,
    data: &DesignData,
) -> bool {
    let fight = &report.fight;
    let fighter = &state.fighters[report.fighter];
    let mut again = false;
    egui::Frame::group(ui.style())
        .fill(ui.visuals().extreme_bg_color)
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal(|ui| {
                let name = fighter_name(report.fighter, fighter, data);
                ui.label(egui::RichText::new(name).strong().color(theme::BRONZE));
                let place = colosseum_name(report.colosseum, data);
                ui.label(egui::RichText::new(tr!("summary-at", place = place)).weak());
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    chip(
                        ui,
                        result_text(&report.line),
                        result_color(ui, &report.line),
                    );
                });
            });
            ui.horizontal(|ui| {
                ui.label(against_text(&report.line, data));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let seconds = fight.ticks / lanista_core::combat::TICKS_PER_SECOND;
                    ui.label(egui::RichText::new(tr!("summary-time", seconds = seconds)).weak())
                        .on_hover_text(tr!("summary-seed", seed = fight.seed.to_string()));
                });
            });
            ui.add_space(4.0);
            face_to_face(ui, report, state, data);
            ui.add_space(4.0);
            ui.horizontal_wrapped(|ui| {
                let money = format!("{:+}", report.money);
                chip(
                    ui,
                    tr!("summary-money-chip", money = money),
                    signed_color(ui, report.money),
                );
                let fame = format!("{:+}", fight.fame_change);
                let fame_color = signed_color(ui, i64::from(fight.fame_change));
                chip(ui, tr!("summary-fame-chip", fame = fame), fame_color);
                let (before, after) = report.form;
                let form_change = i64::from(after) - i64::from(before);
                let form = change(&tr!("roster-form"), before, after);
                chip(ui, form, signed_color(ui, form_change));
                if let Some(injury) = report.injury {
                    chip(ui, injury_text(injury, fighter.dominant_hand), theme::BLOOD);
                }
            });
            ui.add_space(2.0);
            again = ui.button(tr!("summary-watch-again")).clicked();
        });
    again
}

/// A fight's sides stat by stat. A duel puts the club's fighter on the left and its
/// opponent on the right, a bar each growing out from the stat's name, the longer one
/// the bigger value; a melee shows the club's fighter alone.
fn face_to_face(ui: &mut egui::Ui, report: &FightReport, state: &GameState, data: &DesignData) {
    let fight = &report.fight;
    let names = [
        tr!("summary-damage-dealt"),
        tr!("summary-damage-taken"),
        tr!("summary-spells-cast"),
        tr!("summary-misses"),
        tr!("summary-stuns"),
        tr!("summary-hp-left"),
    ];
    let values = |f: &lanista_core::log::FighterReport| {
        [
            f.damage_dealt,
            f.damage_taken,
            f.spells_cast,
            f.misses,
            f.stuns,
            f.hp,
        ]
    };
    let opponent = fight.opponent().filter(|_| !fight.is_melee());
    let sides: Vec<[u32; 6]> = if opponent.is_some() {
        fight.report.fighters.iter().map(values).collect()
    } else {
        fight.mine().into_iter().map(values).collect()
    };
    let plain = ui.visuals().widgets.active.bg_fill;
    let [mine, theirs] = sides.as_slice() else {
        egui::Grid::new(("face_to_face", fight.seed, report.fighter)).show(ui, |ui| {
            for (row, name) in names.into_iter().enumerate() {
                ui.label(egui::RichText::new(name).small());
                ui.label(sides.first().map_or(0, |mine| mine[row]).to_string());
                ui.end_row();
            }
        });
        return;
    };
    let them = opponent.map_or_else(String::new, |o| {
        let name = outsider_name(o, data);
        elo_in(state, report.colosseum, o).map_or_else(
            || name.clone(),
            |elo| tr!("fighter-elo", name = name.as_str(), elo = elo),
        )
    });
    face_row(
        ui,
        |ui| {
            ui.label(
                egui::RichText::new(tr!("summary-yours"))
                    .small()
                    .color(theme::BRONZE),
            );
        },
        "",
        |ui| {
            ui.label(
                egui::RichText::new(tr!("summary-opponent", them = them))
                    .small()
                    .weak(),
            );
        },
    );
    for (row, name) in names.iter().enumerate() {
        let max = mine[row].max(theirs[row]);
        face_row(
            ui,
            |ui| {
                ui.label(mine[row].to_string());
                half_bar(ui, mine[row], max, theme::BRONZE, true);
            },
            name,
            |ui| {
                half_bar(ui, theirs[row], max, plain, false);
                ui.label(theirs[row].to_string());
            },
        );
    }
}

/// Width of a side of the face to face: its bar and its value.
const SIDE_WIDTH: f32 = HALF_BAR_WIDTH + 50.0;
/// Width of the stat's name between the two sides, room for « Dégâts infligés ».
const STAT_NAME_WIDTH: f32 = 130.0;

/// One line of the face to face: the club's side right-aligned, the stat's `name`
/// centred on one line, the opponent's side left-aligned, each its fixed width.
fn face_row(
    ui: &mut egui::Ui,
    left: impl FnOnce(&mut egui::Ui),
    name: &str,
    right: impl FnOnce(&mut egui::Ui),
) {
    let height = ui.spacing().interact_size.y * 0.75;
    ui.horizontal(|ui| {
        let layout = egui::Layout::right_to_left(egui::Align::Center);
        ui.allocate_ui_with_layout(egui::vec2(SIDE_WIDTH, height), layout, left);
        let centred = egui::Layout::centered_and_justified(egui::Direction::LeftToRight);
        ui.allocate_ui_with_layout(egui::vec2(STAT_NAME_WIDTH, height), centred, |ui| {
            ui.add(egui::Label::new(egui::RichText::new(name).small()).extend());
        });
        let layout = egui::Layout::left_to_right(egui::Align::Center);
        ui.allocate_ui_with_layout(egui::vec2(SIDE_WIDTH, height), layout, right);
    });
}

/// Width of one side's bar in the fight summary.
const HALF_BAR_WIDTH: f32 = 90.0;

/// A thin bar filled to `value` out of `max`, from its right end if `leftward`.
fn half_bar(ui: &mut egui::Ui, value: u32, max: u32, color: egui::Color32, leftward: bool) {
    let size = egui::vec2(HALF_BAR_WIDTH, 10.0);
    let (rect, _) = ui.allocate_exact_size(size, egui::Sense::hover());
    let width = rect.width() * fraction(value, max);
    let filled = if leftward {
        egui::Rect::from_min_max(egui::pos2(rect.right() - width, rect.top()), rect.max)
    } else {
        egui::Rect::from_min_size(rect.min, egui::vec2(width, rect.height()))
    };
    ui.painter().rect_filled(filled, 3.0, color);
}

/// Green for a win, red for a loss or a death, bronze for giving up, plain for no winner.
fn result_color(ui: &egui::Ui, line: &FightLine) -> egui::Color32 {
    match line.result {
        _ if line.died => theme::BLOOD,
        FightResult::Won => theme::LAUREL,
        FightResult::Lost => theme::BLOOD,
        FightResult::GaveUp => theme::BRONZE.gamma_multiply(0.6),
        FightResult::Unfinished => ui.visuals().widgets.inactive.bg_fill,
    }
}

/// Green for a gain, red for a loss, plain for nothing.
fn signed_color(ui: &egui::Ui, change: i64) -> egui::Color32 {
    match change.signum() {
        1 => theme::LAUREL,
        -1 => theme::BLOOD,
        _ => ui.visuals().widgets.inactive.bg_fill,
    }
}

/// Horizontal margin inside a tag.
const CHIP_MARGIN: i8 = 10;

/// A small rounded tag on `fill`, its text on one line.
fn chip(
    ui: &mut egui::Ui,
    text: impl Into<egui::WidgetText>,
    fill: egui::Color32,
) -> egui::Response {
    let galley = chip_galley(ui, text);
    chip_of(ui, galley, fill)
}

/// A tag as `chip` draws it, first going to the next row of a wrapping layout when it
/// does not fit on this one. Not for a grid's cell: a new row there is the grid's.
fn wrapped_chip(
    ui: &mut egui::Ui,
    text: impl Into<egui::WidgetText>,
    fill: egui::Color32,
) -> egui::Response {
    let galley = chip_galley(ui, text);
    let width = galley.size().x + 2.0 * f32::from(CHIP_MARGIN);
    let row_started = ui.cursor().min.x > ui.max_rect().min.x;
    // `available_width` is a whole row's in a wrapping layout, not what is left of it.
    if row_started && width > ui.available_size_before_wrap().x {
        ui.end_row();
    }
    chip_of(ui, galley, fill)
}

/// A tag's text laid out on one line.
fn chip_galley(ui: &egui::Ui, text: impl Into<egui::WidgetText>) -> std::sync::Arc<egui::Galley> {
    text.into().into_galley(
        ui,
        Some(egui::TextWrapMode::Extend),
        f32::INFINITY,
        egui::TextStyle::Body,
    )
}

fn chip_of(
    ui: &mut egui::Ui,
    galley: std::sync::Arc<egui::Galley>,
    fill: egui::Color32,
) -> egui::Response {
    egui::Frame::new()
        .fill(fill)
        .corner_radius(10.0)
        .inner_margin(egui::Margin::symmetric(CHIP_MARGIN, 2))
        .show(ui, |ui| ui.label(galley))
        .response
}

/// Width of the sheet's bars: room for the name and value beside them on the Steam Deck.
const METER_WIDTH: f32 = 200.0;

/// A thin bar filled to `value` out of `max` in `color`; with `behind`, faded up to it
/// first, for a value `value` falls short of.
fn meter(
    ui: &mut egui::Ui,
    value: u32,
    max: u32,
    color: egui::Color32,
    behind: Option<u32>,
) -> egui::Response {
    let size = egui::vec2(METER_WIDTH, 12.0);
    let (rect, response) = ui.allocate_exact_size(size, egui::Sense::hover());
    let painter = ui.painter();
    painter.rect_filled(rect, 4.0, ui.visuals().extreme_bg_color);
    let filled = |share: f32| {
        let mut part = rect;
        part.set_width(rect.width() * share);
        part
    };
    if let Some(behind) = behind {
        painter.rect_filled(
            filled(fraction(behind, max)),
            4.0,
            color.gamma_multiply(0.35),
        );
    }
    painter.rect_filled(filled(fraction(value, max)), 4.0, color);
    response
}

/// `value` out of `max`, at most 1.
fn fraction(value: u32, max: u32) -> f32 {
    let share = f32::from(u16::try_from(value).unwrap_or(u16::MAX))
        / f32::from(u16::try_from(max.max(1)).unwrap_or(u16::MAX));
    share.min(1.0)
}

pub(crate) fn gauge(ui: &mut egui::Ui, name: &str, value: u32, max: u32) {
    let fraction = fraction(value, max);
    ui.add(egui::ProgressBar::new(fraction).text(format!("{name} {value}")));
}

#[cfg(test)]
mod tests {
    use lanista_core::combat::Vec2;
    use lanista_core::duel::play;
    use lanista_core::log::Report;
    use lanista_core::market::Move;
    use lanista_core::relations::{Change, Reason, Standing};
    use lanista_core::staff::{Candidate, Person, Role, Scouting, Staff, StaffMove};
    use lanista_core::state::News;
    use rand_chacha::ChaCha8Rng;
    use rand_chacha::rand_core::SeedableRng;

    use super::*;
    use crate::texts::condition_text;
    use crate::texts::{
        club_name as rival_club_name, date_in_sentence, ease_text, free_agent_name, named_club,
        outside_ease_text, outside_personality_text, potential_text, range_texts, staff_text,
        stage_name, unsellable_text, weeks_left_text,
    };
    use crate::title_screen::TitleScreen;
    use colosseums::reign_text;
    use lanista_core::colosseum::{Reign, ReignEnd, TitleLoss, TitleNews};

    fn screen(seed: u64) -> ClubScreen {
        let data = Rc::new(DesignData::load().unwrap());
        let state = GameState::new(seed, &data);
        ClubScreen::new(data, state, None, Bindings::new(Vec::new()))
    }

    /// The game of seed 7, saved in `folder`.
    fn saved_screen(data: &Rc<DesignData>, folder: &std::path::Path) -> ClubScreen {
        let state = GameState::new(7, data);
        let folder = Some(folder.to_path_buf());
        ClubScreen::new(Rc::clone(data), state, folder, Bindings::new(Vec::new()))
    }

    /// What « Continuer » opens at the next launch.
    fn resume(data: &Rc<DesignData>, folder: &std::path::Path) -> Option<ClubScreen> {
        let folder = Some(folder.to_path_buf());
        TitleScreen::new(Rc::clone(data), folder, Bindings::new(Vec::new())).resume()
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
        // It challenges a rival fighter whose club accepts: a fight against that
        // fighter.
        let state = &screen.state;
        let home = ColosseumId(0);
        let rival = state.colosseums[0]
            .active_ranking(state.rosters())
            .find_map(|r| match r.contender {
                Contender::Rival(r) if state.may_challenge(0, home, r, &screen.data) => Some(r),
                _ => None,
            })
            .expect("a rival fighter accepts");
        let resident = state.rival_fighter(rival).unwrap().clone();
        screen.decisions.plans[0].activity = Activity::Challenge(home, rival);
        screen.next_week();
        assert_eq!(screen.fights[0].fight.opponent(), Some(&resident));
        // Watched, that fighter is told by its name, its class and its Elo there.
        let cast = &screen.to_watch[0].cast;
        let class = class_name(&resident, &screen.data);
        let named = format!("{} ({class})", stage_name(&resident.name));
        assert_eq!(cast.names[1], named);
        // Found by its number: it may have changed club or left the ranking since.
        let rosters = screen.state.rosters();
        let elo = screen.state.colosseums[0]
            .ranking
            .iter()
            .find(|r| {
                rosters
                    .fighter(r.contender)
                    .is_some_and(|f| f.id == resident.id)
            })
            .map(|r| r.elo);
        assert_eq!(cast.elos[1], elo);
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
        assert!(menu[0].contains("salaires -95, "), "{menu:?}");
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
        assert_eq!(
            outside_personality_text(observed, &scouting),
            crate::texts::personality_text(observed)
        );
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
            alert_texts(&screen),
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
        screen.decisions.moves = vec![Move::Sign(
            a,
            GameState::asked_terms(&agents[a], &screen.data),
        )];
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
        assert!(alert_texts(&screen).contains(&format!(
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
        let mut screen = saved_screen(&data, &folder);
        assert!(resume(&data, &folder).is_none());
        screen.next_week();
        screen.next_week();
        let resumed = resume(&data, &folder).unwrap();
        assert_eq!(resumed.state, screen.state);
        assert_eq!(resumed.message.as_deref(), Some("Partie reprise"));
        // A manual save is written to its slot.
        screen.save_slot(0);
        let slot = saves::read(&folder, saves::SLOTS[0], &data).unwrap();
        assert_eq!(slot.as_ref(), Some(&screen.state));
        std::fs::remove_dir_all(folder).unwrap();
    }

    #[test]
    fn saving_over_a_game_waits_for_confirmation() {
        let data = Rc::new(DesignData::load().unwrap());
        let folder = std::env::temp_dir().join(format!("lanista-slots-{}", std::process::id()));
        let mut screen = saved_screen(&data, &folder);
        screen.next_week();
        // Saving to an empty slot loses nothing: it is done at once.
        screen.ask(1);
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
        // Saving over a slot waits for the player.
        screen.ask(1);
        assert_eq!(screen.confirm, Some(1));
        let slot = saves::read(&folder, saves::SLOTS[1], &data).unwrap();
        assert_eq!(slot, Some(saved));
        screen.save_slot(1);
        let slot = saves::read(&folder, saves::SLOTS[1], &data).unwrap();
        assert_eq!((slot.as_ref(), screen.confirm), (Some(&screen.state), None));
        std::fs::remove_dir_all(folder).unwrap();
    }

    /// The milestone of phase 4: a whole season with one club, saved and reloaded
    /// halfway, plays on exactly as if it had never stopped.
    #[test]
    fn a_whole_season_is_played_saved_and_reloaded() {
        let data = Rc::new(DesignData::load().unwrap());
        let folder = std::env::temp_dir().join(format!("lanista-season-{}", std::process::id()));
        let mut screen = saved_screen(&data, &folder);
        // Halfway, on a week the club fought, so that there is a fight to watch again.
        let mut half = 0;
        while half < u64::from(data.calendar.season_weeks() / 2) || screen.fights.is_empty() {
            screen.next_week();
            half += 1;
        }
        let mut reloaded = resume(&data, &folder).unwrap();
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
            alert_texts(&screen),
            [format!(
                "{name} : titre de Rome à défendre au plus tard en saison 1, semaine 5"
            )]
        );
        // The rival champions defend in week 5, without the player.
        let mut screen = self::screen(7);
        for _ in 0..5 {
            screen.next_week();
        }
        let titles = &screen.week_titles;
        let clubs = &screen.state.rivals;
        assert!(
            titles
                .iter()
                .any(|(_, article)| article.byline.as_ref().is_some_and(|b| {
                    b.contains(" contre ")
                        && clubs
                            .iter()
                            .any(|c| b.contains(&format!(", {})", named_club(&c.name))))
                })),
            "{titles:?}"
        );
    }

    #[test]
    fn the_rival_clubs_are_listed() {
        let screen = screen(7);
        let rows = screen.rival_rows();
        assert_eq!(rows.len(), screen.state.rivals.len());
        let cells: Vec<_> = rows[..3]
            .iter()
            .map(|r| {
                (
                    r.name.as_str(),
                    r.culture.as_str(),
                    r.profile.as_str(),
                    r.wealth.as_str(),
                    r.fighters,
                    r.relation.as_str(),
                )
            })
            .collect();
        assert_eq!(
            cells,
            [
                (
                    "Familia Maximus",
                    "Rome",
                    "spectaculaire",
                    "pauvre",
                    10,
                    "+0, neutre"
                ),
                (
                    "École Guan Yu",
                    "Asie de l'Est",
                    "formateur",
                    "modeste",
                    10,
                    "+0, neutre"
                ),
                (
                    "Clan de Tashunka",
                    "Tribus amérindiennes",
                    "vétéran",
                    "pauvre",
                    10,
                    "+0, neutre"
                ),
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
        assert_eq!(screen.rival_rows()[0].relation, "-25, tendu");
        assert_eq!(screen.relation_lines(1), Vec::<String>::new());
    }

    #[test]
    fn the_world_s_news_are_told_by_week_newest_first() {
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
            News {
                week: 31,
                what: WorldNews::Title(1, TitleNews::Vacated { to: None }),
            },
        ];
        let founded = "Un nouveau ludus ouvre ses portes : Familia Maximus (Rome), en Europe";
        let vacated = "Le champion de Rome n'est plus, son trône reste vide";
        let closed = "Faillite : École Guan Yu ferme ses portes, ses combattants partent au marché";
        assert_eq!(
            headlines(&screen),
            [
                (31, vec![vacated.to_owned(), founded.to_owned()]),
                (30, vec![closed.to_owned()]),
            ]
        );
        // The week just played leaves its titles to the week's title fights.
        screen.last_week = Some((31, Vec::new()));
        assert_eq!(headlines(&screen)[0], (31, vec![founded.to_owned()]));
    }

    /// Each week of the world's news with its headlines.
    fn alert_texts(screen: &ClubScreen) -> Vec<String> {
        screen.alerts().into_iter().map(|(text, _)| text).collect()
    }

    fn headlines(screen: &ClubScreen) -> Vec<(u64, Vec<String>)> {
        let pages = screen.news_pages().into_iter();
        pages
            .map(|(week, articles)| (week, articles.into_iter().map(|a| a.headline).collect()))
            .collect()
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
            headlines(&screen),
            [(
                4,
                vec!["École Guan Yu se déclare votre rival : vos combats contre lui attireront les foules".to_owned()]
            )]
        );
        assert!(alert_texts(&screen).contains(
            &"École Guan Yu est votre rival depuis la saison 1, semaine 4 : recettes à 150 %, renommée +3 par combat"
                .to_owned()
        ));
        assert_eq!(screen.rival_rows()[1].standing, Standing::Rival);
    }

    /// Phase 6 bis milestone: over three seasons on screen, a player who fills the club
    /// and challenges, week after week, the club it gets along with least makes it a
    /// declared rival, whose page tells the fights that did it; meanwhile a club closes
    /// and another is founded, and every club's relation tells its reasons.
    #[test]
    fn three_seasons_where_a_club_becomes_a_rival_one_closes_and_one_is_founded() {
        let data = Rc::new(DesignData::load().unwrap());
        let mut screen = screen(2);
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
                let terms = GameState::asked_terms(&state.free_agents[a], &data);
                screen.decisions.moves.push(Move::Sign(a, terms));
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
        // « Retour » goes back page by page, to the colosseums, then no further.
        screen.back();
        assert_eq!(screen.view, View::Outside(Target::Rival(id)));
        screen.back();
        screen.back();
        assert_eq!(screen.view, View::Colosseums);
        // « Suivant » opens them again, until another view is opened.
        screen.next();
        assert_eq!(screen.view, View::Outside(Target::Rival(id)));
        screen.go_to(View::Staff);
        screen.next();
        assert_eq!(screen.view, View::Staff);
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
    fn the_recap_sets_the_ranking_after_the_week_against_the_one_before() {
        let mut screen = screen(7);
        // Past the season's opening and its melees.
        screen.state.week = 2;
        let rome = ColosseumId(1);
        screen.decisions.plans[0].activity = Activity::Fight(rome);
        screen.next_week();
        let summary = &screen.state.journal.last.as_ref().unwrap().summary;
        let [(id, before)] = summary.rankings_before.as_slice() else {
            panic!("{:?}", summary.rankings_before);
        };
        let rows = colosseums::recap_rows(*id, before, &screen.state, &screen.data);
        let ours = rows
            .iter()
            .find(|r| r.contender == Contender::Club(0))
            .expect("the club's fighter in the recap");
        let fight = &screen.fights[0].fight;
        assert_eq!(ours.won > 0, fight.result == FightResult::Won);
        // The first ranks, then the club's fighter and those around it.
        assert_eq!(rows[0].rank, 0);
        assert!(rows.windows(2).all(|w| w[0].rank < w[1].rank));
        assert!(
            rows.iter()
                .all(|r| r.gap == (r.rank > 0 && rows.iter().all(|o| o.rank != r.rank - 1)))
        );
        // One signed during the week was a free agent before it: not new to the ranking.
        let signed = rows
            .iter()
            .find(|r| r.contender != Contender::Club(0))
            .unwrap();
        screen.state.free_agents[0].moved_to = Some(signed.contender);
        let mut before = before.clone();
        for r in before
            .iter_mut()
            .filter(|r| r.contender == signed.contender)
        {
            r.contender = Contender::FreeAgent(0);
        }
        let again = colosseums::recap_rows(*id, &before, &screen.state, &screen.data);
        let row = again.iter().find(|r| r.contender == signed.contender);
        assert_eq!(row, Some(signed));
    }

    #[test]
    fn the_newspaper_opens_on_the_last_week_played() {
        let mut screen = screen(7);
        assert_eq!(screen.issues(), Vec::<u64>::new());
        for _ in 0..3 {
            screen.next_week();
        }
        let issues = screen.issues();
        assert_eq!(issues.first(), Some(&3));
        // Newest first, each week once.
        assert!(issues.windows(2).all(|w| w[0] > w[1]));
        screen.shown_issue = issues.last().copied();
        screen.next_week();
        assert_eq!(screen.shown_issue, None);
    }

    #[test]
    fn the_market_filters_by_age_fame_and_record() {
        let screen = screen(7);
        let mut f = screen.state.free_agents[0].clone();
        f.age = 25;
        f.record = lanista_core::fighter::career::Record {
            wins: 3,
            losses: 1,
            draws: 0,
        };
        let shows = |filter: MarketFilter| filter.shows(&f);
        assert!(shows(MarketFilter::default()));
        assert!(!shows(MarketFilter::new((26, 99), 0, 0, 0)));
        assert!(!shows(MarketFilter::new((0, 24), 0, 0, 0)));
        assert!(!shows(MarketFilter::new((0, 99), f.fame.value() + 1, 0, 0)));
        assert!(shows(MarketFilter::new((25, 25), 0, 3, 75)));
        assert!(!shows(MarketFilter::new((0, 99), 0, 4, 0)));
        assert!(!shows(MarketFilter::new((0, 99), 0, 0, 76)));
    }

    #[test]
    fn the_market_sorts_by_the_column_clicked() {
        let screen = screen(7);
        let data = &screen.data;
        let ages = |sort: MarketSort| {
            let mut rows: Vec<_> = screen.state.free_agents.iter().collect();
            sort.apply(&mut rows, |f| f, data);
            rows.iter().map(|f| f.age).collect::<Vec<_>>()
        };
        let down = ages(MarketSort::new(SortBy::Age, false));
        assert!(down.is_sorted_by(|a, b| a >= b), "{down:?}");
        let up = ages(MarketSort::new(SortBy::Age, true));
        assert!(up.is_sorted(), "{up:?}");
        assert_ne!(up.first(), up.last(), "the free agents are all of one age");
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
            alert_texts(&screen),
            [format!(
                "{} : blessure, {arm}",
                fighter_name(0, &screen.state.fighters[0], &screen.data)
            )]
        );
        assert!(arm.starts_with("Bras droit"), "{arm}");
        // The alert leads to the fighter's sheet.
        assert_eq!(screen.alerts()[0].1, View::Sheet(0));
    }

    /// Every view drawn once, headless, on a screen wide enough for three cards a row,
    /// in a world a few weeks on: none panics.
    #[test]
    fn every_view_is_drawn() {
        let mut screen = screen(7);
        for _ in 0..12 {
            screen.next_week();
        }
        screen.to_watch.clear();
        let ui = egui::Context::default();
        crate::theme::apply(&ui);
        let input = || egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(1600.0, 900.0),
            )),
            ..Default::default()
        };
        let draw = |screen: &mut ClubScreen| {
            let _ = ui.run(input(), |ui| {
                egui::TopBottomPanel::top("club").show(ui, |ui| screen.top_bar(ui));
                egui::CentralPanel::default().show(ui, |ui| match screen.view {
                    View::Menu => screen.menu(ui),
                    View::Roster => screen.roster(ui),
                    View::Sheet(i) => screen.sheet(ui, i),
                    View::Fights => drop(screen.fight_summary(ui)),
                    View::Paper => screen.paper(ui),
                    View::Colosseums => screen.colosseums(ui),
                    View::Market => screen.market(ui),
                    View::Staff => screen.staff(ui),
                    View::Outside(target) => screen.outside(ui, target),
                    View::Rival(club) => screen.rival_club(ui, club),
                    View::Wiki(entry) => screen.wiki(ui, entry),
                });
            });
        };
        let views = [
            View::Menu,
            View::Roster,
            View::Sheet(0),
            View::Fights,
            View::Paper,
            View::Colosseums,
            View::Market,
            View::Rival(0),
        ];
        let pages = wiki::Entry::CATEGORIES
            .into_iter()
            .chain(wiki::entries(&screen.data));
        let views: Vec<View> = views.into_iter().chain(pages.map(View::Wiki)).collect();
        for view in views {
            screen.view = view;
            draw(&mut screen);
        }
        screen.view = View::Staff;
        for role in Role::ALL {
            screen.shown_role = role;
            draw(&mut screen);
        }
    }
}
