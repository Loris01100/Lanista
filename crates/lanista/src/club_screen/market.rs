//! The market view and the sheet of a fighter out of the club.

use lanista_core::colosseum::{ColosseumId, Contender};
use lanista_core::design_data::DesignData;
use lanista_core::fighter::career::{CareerFighter, Status};
use lanista_core::market::{Move, SaleOffer, Target, stat_ranges, value};
use lanista_core::rivals::RivalId;
use lanista_core::staff::Scouting;
use lanista_core::state::GameState;
use reesch::egui;

use super::colosseums::{arrow, not_ranked, ranked_name};
use super::{ClubScreen, View, chip, face_row, half_bar};
use crate::lang::tr;
use crate::texts::{
    club_name, colosseum_name, condition_text, fighter_name, free_agent_name, move_text,
    origin_text, outside_ease_text, outside_personality_text, potential_text, range_texts,
    record_text, rival_fighter_name, stat_names, unsellable_text, weeks,
};
use crate::theme;

/// Which fighters out of the club the market lists, kept while the game runs.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct MarketFilter {
    /// The youngest and oldest ages shown.
    age: (u32, u32),
    /// The least fame shown.
    fame: u32,
    /// The fewest wins shown.
    wins: u32,
    /// The least share of fights won shown, in percent.
    win_rate: u32,
}

impl Default for MarketFilter {
    fn default() -> Self {
        Self {
            age: (0, 99),
            fame: 0,
            wins: 0,
            win_rate: 0,
        }
    }
}

impl MarketFilter {
    #[cfg(test)]
    pub(super) fn new(age: (u32, u32), fame: u32, wins: u32, win_rate: u32) -> Self {
        Self {
            age,
            fame,
            wins,
            win_rate,
        }
    }

    /// Whether the market lists `fighter`.
    pub(super) fn shows(&self, fighter: &CareerFighter) -> bool {
        let r = fighter.record;
        let fights = r.wins + r.losses + r.draws;
        let rate = (r.wins * 100).checked_div(fights).unwrap_or(0);
        (self.age.0..=self.age.1).contains(&fighter.age)
            && fighter.fame.value() >= self.fame
            && r.wins >= self.wins
            && rate >= self.win_rate
    }

    /// The filters' row: age, fame, wins, share of wins, and a button clearing them.
    fn row(&mut self, ui: &mut egui::Ui) {
        ui.horizontal_wrapped(|ui| {
            ui.label(tr!("filter-age"));
            ui.add(egui::DragValue::new(&mut self.age.0).range(0..=self.age.1));
            ui.label(tr!("filter-to"));
            ui.add(egui::DragValue::new(&mut self.age.1).range(self.age.0..=99));
            ui.separator();
            ui.label(tr!("filter-fame"));
            ui.add(egui::DragValue::new(&mut self.fame).range(0..=100));
            ui.separator();
            ui.label(tr!("filter-wins"));
            ui.add(egui::DragValue::new(&mut self.wins).range(0..=999));
            ui.separator();
            ui.label(tr!("filter-win-rate"));
            ui.add(
                egui::DragValue::new(&mut self.win_rate)
                    .range(0..=100)
                    .suffix(" %"),
            );
            let default = Self::default();
            if ui
                .add_enabled(*self != default, egui::Button::new(tr!("filter-clear")))
                .clicked()
            {
                *self = default;
            }
        });
    }
}

/// A column the market's lists sort by.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum SortBy {
    Age,
    Fame,
    Wins,
    Value,
    Wage,
}

impl SortBy {
    fn key(self, fighter: &CareerFighter, data: &DesignData) -> u32 {
        let economy = &data.economy;
        match self {
            Self::Age => fighter.age,
            Self::Fame => fighter.fame.value(),
            Self::Wins => fighter.record.wins,
            Self::Value => value(fighter, &economy.market),
            Self::Wage => economy.contracts.asked_wage(fighter, &economy.market),
        }
    }
}

/// How the market's lists are sorted, kept while the game runs: by the column whose
/// title was clicked, highest first, lowest first after a second click; none keeps
/// their order.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) struct MarketSort {
    by: Option<SortBy>,
    ascending: bool,
}

impl MarketSort {
    #[cfg(test)]
    pub(super) fn new(by: SortBy, ascending: bool) -> Self {
        Self {
            by: Some(by),
            ascending,
        }
    }

    /// Sorts `rows`, `fighter` giving each one's fighter.
    pub(super) fn apply<T>(
        self,
        rows: &mut [T],
        fighter: impl Fn(&T) -> &CareerFighter,
        data: &DesignData,
    ) {
        let Some(by) = self.by else { return };
        rows.sort_by(|a, b| {
            let order = by.key(fighter(a), data).cmp(&by.key(fighter(b), data));
            if self.ascending {
                order
            } else {
                order.reverse()
            }
        });
    }

    /// A column's title, clicked to sort by it, with an arrow when the lists are.
    fn title(&mut self, ui: &mut egui::Ui, title: &str, by: SortBy) {
        ui.horizontal(|ui| {
            let label =
                egui::Label::new(egui::RichText::new(title).strong()).sense(egui::Sense::click());
            let mut clicked = ui
                .add(label)
                .on_hover_cursor(egui::CursorIcon::PointingHand)
                .clicked();
            if self.by == Some(by) {
                let dir = if self.ascending {
                    egui::Vec2::UP
                } else {
                    egui::Vec2::DOWN
                };
                clicked |= arrow(ui, dir, theme::BRONZE).clicked();
            }
            if clicked {
                self.ascending = self.by == Some(by) && !self.ascending;
                self.by = Some(by);
            }
        });
    }
}

/// What a click on the market asks for.
enum Pick {
    /// Open the sheet of this fighter.
    Open(Target),
    /// Write an offer for this rival fighter, starting from this price.
    Negotiate(RivalId, u32),
    /// Open the page of this rival club.
    Club(usize),
    /// Turn down this rival club's offer for one of the club's fighters.
    Refuse(SaleOffer),
}

impl ClubScreen {
    /// The market: the roster's room and the rules, this week's moves with the answer
    /// to the last offer and the offer being written, then one tab at a time the free
    /// agents, the rival fighters to observe or negotiate for, or the club's fighters to
    /// sell or release. A fighter out of the club shows only its age, fame and record,
    /// if it passes the filters; its name opens what the club sees of it.
    pub(super) fn market(&mut self, ui: &mut egui::Ui) {
        let active = self.state.active_fighters();
        let max = self.data.economy.max_fighters;
        ui.horizontal(|ui| {
            ui.heading(tr!("view-market"));
            let full = u32::try_from(active).is_ok_and(|n| n >= max);
            let color = if full { theme::BLOOD } else { theme::LAUREL };
            chip(ui, tr!("market-roster", active = active, max = max), color);
        });
        ui.label(egui::RichText::new(tr!("market-rules")).small().weak());
        ui.add_space(4.0);
        self.week_moves(ui);
        ui.add_space(4.0);
        let filter = self.market_filter;
        let sort = &mut self.market_sort;
        let counts = market_counts(&self.state, filter);
        ui.horizontal(|ui| {
            let tab = &mut self.market_tab;
            let [free, rivals, yours] = counts;
            ui.selectable_value(
                tab,
                MarketTab::FreeAgents,
                tr!("market-tab-free", count = free),
            );
            ui.selectable_value(
                tab,
                MarketTab::Rivals,
                tr!("market-tab-rivals", count = rivals),
            );
            ui.selectable_value(
                tab,
                MarketTab::Yours,
                tr!("market-tab-yours", count = yours),
            );
        });
        if self.market_tab != MarketTab::Yours {
            self.market_filter.row(ui);
        }
        ui.separator();
        let data = &self.data;
        let state = &self.state;
        let moves = &mut self.decisions.moves;
        let tab = self.market_tab;
        let pick = egui::ScrollArea::vertical().show(ui, |ui| match tab {
            MarketTab::FreeAgents => free_agent_grid(ui, state, moves, (filter, sort), data),
            MarketTab::Rivals => rival_grid(ui, state, moves, (filter, sort), data),
            MarketTab::Yours => sale_grid(ui, state, moves, data),
        });
        match pick.inner {
            Some(Pick::Open(target)) => self.go_to(View::Outside(target)),
            Some(Pick::Negotiate(id, price)) => self.offer_draft = Some((id, price)),
            Some(Pick::Club(club)) => self.go_to(View::Rival(club)),
            Some(Pick::Refuse(offer)) => {
                let accept = Move::AcceptOffer {
                    fighter: offer.fighter,
                    rival: offer.rival,
                };
                self.decisions.moves.retain(|m| *m != accept);
                self.state.sale_offers.retain(|o| *o != offer);
            }
            None => {}
        }
    }

    /// A card with this week's moves, each to cancel, the price a rival club asked for
    /// its fighter and the offer being written; nothing when there is none of them.
    fn week_moves(&mut self, ui: &mut egui::Ui) {
        let data = &self.data;
        let state = &self.state;
        let moves = &mut self.decisions.moves;
        if moves.is_empty() && state.counter_offer.is_none() && self.offer_draft.is_none() {
            return;
        }
        let mut close = false;
        let mut cancel = None;
        egui::Frame::group(ui.style())
            .fill(ui.visuals().extreme_bg_color)
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                let title = egui::RichText::new(tr!("market-this-week"));
                ui.label(title.strong().color(theme::BRONZE));
                if let Some((id, price)) = state.counter_offer {
                    ui.horizontal(|ui| {
                        ui.label(tr!(
                            "market-counter-offer",
                            club = club_name(id.club, state),
                            price = price,
                            fighter = rival_fighter_name(id, state, data)
                        ));
                        if ui.button(tr!("market-offer-this-price")).clicked() {
                            set_offer(moves, id, price);
                        }
                    });
                }
                if let Some((id, price)) = &mut self.offer_draft {
                    ui.horizontal_wrapped(|ui| {
                        ui.label(tr!(
                            "market-offer-for",
                            fighter = rival_fighter_name(*id, state, data)
                        ));
                        ui.add(egui::DragValue::new(price).speed(5));
                        let allowed = state.may_move(Move::Offer(*id, *price), data);
                        if ui
                            .add_enabled(allowed, egui::Button::new(tr!("market-make-offer")))
                            .clicked()
                        {
                            set_offer(moves, *id, *price);
                            close = true;
                        }
                        close |= ui.button(tr!("cancel")).clicked();
                        if let Some(text) = state
                            .rival_fighter(*id)
                            .and_then(|f| unsellable_text(f, state, data))
                        {
                            ui.colored_label(theme::BLOOD, text);
                        }
                        let standing = state.standing(id.club, data);
                        if !data.world.relations.standings.get(standing).deals {
                            let refuses =
                                tr!("market-rival-refuses", club = club_name(id.club, state));
                            ui.colored_label(theme::BLOOD, refuses);
                        }
                    });
                }
                for (n, &m) in moves.iter().enumerate() {
                    ui.horizontal(|ui| {
                        ui.label(move_text(m, state, data));
                        if ui.small_button(tr!("cancel")).clicked() {
                            cancel = Some(n);
                        }
                    });
                }
            });
        if close {
            self.offer_draft = None;
        }
        if let Some(n) = cancel {
            self.decisions.moves.remove(n);
        }
    }

    /// What the club sees of a free agent or a rival club's fighter: age, fame, record,
    /// the stats seen, the potential and personality the recruiter sees, its standing in
    /// each colosseum, and the market moves it allows.
    pub(super) fn outside(&mut self, ui: &mut egui::Ui, target: Target) {
        let data = &self.data;
        let state = &self.state;
        let (fighter, name) = match target {
            Target::FreeAgent(a) => (
                state.free_agents.get(a),
                state
                    .free_agents
                    .get(a)
                    .map(|f| free_agent_name(a, f, data)),
            ),
            Target::Rival(id) => (
                state.rival_fighter(id),
                Some(rival_fighter_name(id, state, data)),
            ),
        };
        let (Some(f), Some(name)) = (fighter, name) else {
            return;
        };
        let scouting = state.scouting();
        ui.heading(tr!("sheet-title", fighter = name, age = f.age));
        if let Some(origin) = origin_text(f, data) {
            ui.label(origin);
        }
        let mut goto = None;
        if let Target::Rival(id) = target
            && ui
                .link(tr!("outside-club", club = club_name(id.club, state)))
                .clicked()
        {
            goto = Some(View::Rival(id.club));
        }
        ui.label(tr!(
            "outside-summary",
            condition = condition_text(f),
            fame = f.fame.value(),
            record = record_text(f.record)
        ));
        ui.label(observed_text(f));
        ui.add_space(8.0);
        egui::Grid::new("outside stats")
            .striped(true)
            .show(ui, |ui| {
                for (stat, seen) in stat_names()
                    .into_iter()
                    .zip(range_texts(f, &scouting, data))
                {
                    ui.label(stat);
                    ui.label(seen);
                    ui.end_row();
                }
                ui.label(tr!("potential"));
                ui.label(potential_text(f, &scouting));
                ui.end_row();
                ui.label(tr!("personality-title"));
                ui.label(outside_personality_text(f, &scouting));
                ui.end_row();
                ui.label(tr!("ease-title"));
                ui.label(outside_ease_text(f, &scouting, data));
                ui.end_row();
            });
        ui.add_space(8.0);
        comparison(ui, &mut self.compared, f, (state, &scouting), data);
        let contender = match target {
            Target::FreeAgent(a) => Contender::FreeAgent(a),
            Target::Rival(id) => Contender::Rival(id),
        };
        ui.add_space(8.0);
        self.outside_standings(ui, contender);
        ui.add_space(8.0);
        let moves = &mut self.decisions.moves;
        let mut draft = None;
        if f.status == Status::Active {
            ui.horizontal(|ui| {
                toggle(ui, moves, Move::Observe(target), &observe_label(f), true);
                match target {
                    Target::FreeAgent(a) => {
                        let sign = Move::Sign(a, GameState::asked_terms(f, data));
                        let allowed = state.may_move(sign, data);
                        terms_toggle(ui, moves, sign, &tr!("sign"), allowed, data);
                    }
                    Target::Rival(id) => {
                        let estimate = estimate(f, data);
                        ui.label(tr!("estimated-value-of", value = estimate));
                        if ui.button(tr!("negotiate")).clicked() {
                            draft = Some((id, estimate));
                        }
                    }
                }
            });
        }
        if let Some(draft) = draft {
            self.offer_draft = Some(draft);
            self.go_to(View::Market);
        } else if let Some(view) = goto {
            self.go_to(view);
        }
    }
}

impl ClubScreen {
    /// Where `contender`, out of the club, stands in each colosseum of the club's
    /// continent, and abroad where it is ranked.
    fn outside_standings(&self, ui: &mut egui::Ui, contender: Contender) {
        let data = &self.data;
        ui.strong(tr!("view-colosseums"));
        let mut ranked = false;
        let not_ranked = not_ranked();
        for (c, standing) in (0..).zip(self.standings(contender)) {
            let id = ColosseumId(c);
            let home = data
                .world
                .colosseum(id)
                .is_some_and(|r| r.continent == self.state.continent);
            // Abroad, only where it is ranked.
            if home || standing != not_ranked {
                ranked = true;
                ui.label(tr!(
                    "labelled",
                    label = colosseum_name(id, data),
                    text = standing
                ));
            }
        }
        if !ranked {
            ui.label(not_ranked);
        }
    }
}

/// A list to pick one of the club's fighters in activity, then its trained stats face to
/// face with what the recruiter sees of `outsider`: a bar to the middle of each range.
fn comparison(
    ui: &mut egui::Ui,
    compared: &mut Option<usize>,
    outsider: &CareerFighter,
    (state, scouting): (&GameState, &Scouting),
    data: &DesignData,
) {
    let names: Vec<_> = (0..)
        .zip(&state.fighters)
        .filter(|(_, f)| f.status == Status::Active)
        .map(|(i, f)| (i, fighter_name(i, f, data)))
        .collect();
    let shown = names
        .iter()
        .find(|(i, _)| Some(*i) == *compared)
        .map_or_else(|| tr!("compare-none"), |(_, name)| name.clone());
    egui::ComboBox::from_label(tr!("compare-with"))
        .selected_text(shown)
        .show_ui(ui, |ui| {
            ui.selectable_value(compared, None, tr!("compare-none"));
            for (i, name) in &names {
                ui.selectable_value(compared, Some(*i), name.as_str());
            }
        });
    let Some(mine) = compared
        .and_then(|i| state.fighters.get(i))
        .filter(|f| f.status == Status::Active)
    else {
        return;
    };
    let middles = stat_ranges(outsider, &data.economy.market, scouting)
        .map(|seen| seen.map_or(0, |(low, high)| low.midpoint(high)));
    let plain = ui.visuals().widgets.active.bg_fill;
    for (((name, mine), theirs), seen) in stat_names()
        .iter()
        .zip(mine.stats.to_array())
        .zip(middles)
        .zip(range_texts(outsider, scouting, data))
    {
        let max = mine.max(theirs);
        face_row(
            ui,
            |ui| {
                ui.label(mine.to_string());
                half_bar(ui, mine, max, theme::BRONZE, true);
            },
            name,
            |ui| {
                half_bar(ui, theirs, max, plain, false);
                ui.label(seen);
            },
        );
    }
}

/// "Observé 2 semaines" or "Jamais observé".
pub(super) fn observed_text(fighter: &CareerFighter) -> String {
    match fighter.observed_weeks {
        0 => tr!("never-observed"),
        n => tr!("observed", weeks = weeks(n)),
    }
}

/// The value the market shows of a rival fighter.
// ponytail: the value rounded, until the recruiter estimates it.
pub(super) fn estimate(fighter: &CareerFighter, data: &DesignData) -> u32 {
    value(fighter, &data.economy.market) / 10 * 10
}

/// The weekly wage `fighter` would ask to join the club.
fn asked_wage_text(fighter: &CareerFighter, data: &DesignData) -> String {
    let economy = &data.economy;
    let wage = economy.contracts.asked_wage(fighter, &economy.market);
    tr!("wage", wage = wage)
}

/// Replaces the week's offer with one of `price` for rival fighter `id`.
fn set_offer(moves: &mut Vec<Move>, id: RivalId, price: u32) {
    moves.retain(|m| !matches!(m, Move::Offer(..)));
    moves.push(Move::Offer(id, price));
}

/// A button adding `m` to the week's moves, or taking it out when chosen already; only a
/// chosen one may be clicked when not `enabled`. An observation replaces another.
fn toggle(
    ui: &mut egui::Ui,
    moves: &mut Vec<Move>,
    m: Move,
    label: &str,
    enabled: bool,
) -> egui::Response {
    let chosen = moves.contains(&m);
    let button = egui::SelectableLabel::new(chosen, label);
    let response = ui.add_enabled(enabled || chosen, button);
    if response.clicked() {
        if chosen {
            moves.retain(|&other| other != m);
        } else {
            if matches!(m, Move::Observe(_)) {
                moves.retain(|other| !matches!(other, Move::Observe(_)));
            }
            moves.push(m);
        }
    }
    response
}

/// A button adding `m`, a signing or a renewal on the fighter's asked terms, to the
/// week's moves, or taking it out when chosen already; chosen, the terms offered are
/// changed beside it. Only a chosen one may be clicked when not `enabled`.
fn terms_toggle(
    ui: &mut egui::Ui,
    moves: &mut Vec<Move>,
    m: Move,
    label: &str,
    enabled: bool,
    data: &DesignData,
) -> egui::Response {
    let same = |other: &Move| match (*other, m) {
        (Move::Sign(a, _), Move::Sign(b, _)) | (Move::Renew(a, _), Move::Renew(b, _)) => a == b,
        _ => false,
    };
    let chosen = moves.iter().position(same);
    let button = egui::SelectableLabel::new(chosen.is_some(), label);
    let response = ui
        .add_enabled(enabled || chosen.is_some(), button)
        .on_hover_text(tr!("terms-hint"));
    match chosen {
        Some(k) if response.clicked() => {
            moves.remove(k);
        }
        None if response.clicked() => moves.push(m),
        Some(k) => {
            if let Move::Sign(_, terms) | Move::Renew(_, terms) = &mut moves[k] {
                let max_seasons = data.economy.contracts.max_seasons;
                ui.label(tr!("terms-wage"));
                ui.add(egui::DragValue::new(&mut terms.wage).range(1..=99_999));
                ui.label(tr!("terms-bonus"));
                ui.add(egui::DragValue::new(&mut terms.win_bonus).range(0..=99_999));
                ui.label(tr!("terms-seasons"));
                ui.add(egui::DragValue::new(&mut terms.seasons).range(1..=max_seasons));
            }
        }
        None => {}
    }
    response
}

fn observe_label(fighter: &CareerFighter) -> String {
    match fighter.observed_weeks {
        0 => tr!("observe"),
        weeks => tr!("observe-again", weeks = weeks),
    }
}

/// The market's tabs.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum MarketTab {
    FreeAgents,
    Rivals,
    Yours,
}

/// How many fighters each tab lists: the free agents and rival fighters `filter` shows,
/// and the club's fighters in activity.
fn market_counts(state: &GameState, filter: MarketFilter) -> [usize; 3] {
    let listed = |f: &&CareerFighter| f.status == Status::Active && filter.shows(f);
    let free = state.free_agents.iter().filter(listed).count();
    let rivals = state
        .rivals
        .iter()
        .flat_map(|r| &r.fighters)
        .filter(listed)
        .count();
    [free, rivals, state.active_fighters()]
}

/// A fighter's name, its class in small print beside it; returns whether the name was
/// clicked.
fn name_cell(ui: &mut egui::Ui, name: egui::RichText, class: String) -> bool {
    ui.horizontal(|ui| {
        let clicked = ui.link(name).clicked();
        ui.label(egui::RichText::new(class).small().weak());
        clicked
    })
    .inner
}

/// Age, fame and record: wins in green, losses in red, draws faded.
fn record_cells(ui: &mut egui::Ui, fighter: &CareerFighter) {
    ui.label(fighter.age.to_string());
    ui.label(fighter.fame.value().to_string());
    let r = fighter.record;
    ui.horizontal(|ui| {
        ui.colored_label(theme::LAUREL, tr!("record-wins", count = r.wins));
        ui.colored_label(theme::BLOOD, tr!("record-losses", count = r.losses));
        ui.label(egui::RichText::new(tr!("record-draws", count = r.draws)).weak());
    });
}

/// A grid's titles: those of a fighter out of the club, then `last`, each clicked to
/// sort by its column; `club` adds the club's column after the name.
fn outside_header(ui: &mut egui::Ui, club: bool, last: &[(String, SortBy)], sort: &mut MarketSort) {
    ui.strong(tr!("roster-fighter"));
    if club {
        ui.strong(tr!("ranking-club"));
    }
    let titles = [
        (tr!("roster-age"), SortBy::Age),
        (tr!("roster-fame"), SortBy::Fame),
        (tr!("record-title"), SortBy::Wins),
    ];
    for (title, by) in titles.iter().chain(last) {
        sort.title(ui, title, *by);
    }
    ui.end_row();
}

/// The market's grids, striped and spaced alike.
fn market_grid(id: &str) -> egui::Grid {
    egui::Grid::new(id).striped(true).spacing([18.0, 6.0])
}

/// The free agents on the market `filter` shows; returns the one to open.
fn free_agent_grid(
    ui: &mut egui::Ui,
    state: &GameState,
    moves: &mut Vec<Move>,
    (filter, sort): (MarketFilter, &mut MarketSort),
    data: &DesignData,
) -> Option<Pick> {
    let mut pick = None;
    ui.label(
        egui::RichText::new(tr!("market-free-agents"))
            .small()
            .weak(),
    );
    let room = state.can_hire(data);
    market_grid("free agents").show(ui, |ui| {
        outside_header(ui, false, &[(tr!("market-asked-wage"), SortBy::Wage)], sort);
        let mut rows: Vec<_> = (state.free_agents.iter().enumerate())
            .filter(|(_, f)| f.status == Status::Active && filter.shows(f))
            .collect();
        sort.apply(&mut rows, |(_, f)| f, data);
        for (a, f) in rows {
            let (name, class) = ranked_name(Contender::FreeAgent(a), state, data);
            if name_cell(ui, egui::RichText::new(name).strong(), class) {
                pick = Some(Pick::Open(Target::FreeAgent(a)));
            }
            record_cells(ui, f);
            ui.label(asked_wage_text(f, data));
            ui.horizontal(|ui| {
                let observe = Move::Observe(Target::FreeAgent(a));
                toggle(ui, moves, observe, &observe_label(f), true);
                let sign = Move::Sign(a, GameState::asked_terms(f, data));
                let allowed = state.may_move(sign, data);
                let button = terms_toggle(ui, moves, sign, &tr!("sign"), allowed, data);
                if !room {
                    button.on_disabled_hover_text(tr!("market-full"));
                }
            });
            ui.end_row();
        }
    });
    pick
}

/// The rival clubs' fighters in activity `filter` shows; returns the one to open or to
/// negotiate for, with a first price.
fn rival_grid(
    ui: &mut egui::Ui,
    state: &GameState,
    moves: &mut Vec<Move>,
    (filter, sort): (MarketFilter, &mut MarketSort),
    data: &DesignData,
) -> Option<Pick> {
    let mut pick = None;
    market_grid("rival fighters").show(ui, |ui| {
        let titles = [
            (tr!("estimated-value"), SortBy::Value),
            (tr!("market-asked-wage"), SortBy::Wage),
        ];
        outside_header(ui, true, &titles, sort);
        let mut rows: Vec<_> = (state.rivals.iter().enumerate())
            .flat_map(|(club, r)| {
                let fighters = r.fighters.iter().enumerate();
                fighters.map(move |(fighter, f)| (RivalId { club, fighter }, f))
            })
            .filter(|(_, f)| f.status == Status::Active && filter.shows(f))
            .collect();
        sort.apply(&mut rows, |(_, f)| f, data);
        for (id, f) in rows {
            let (name, class) = ranked_name(Contender::Rival(id), state, data);
            if name_cell(ui, egui::RichText::new(name).strong(), class) {
                pick = Some(Pick::Open(Target::Rival(id)));
            }
            if ui.link(club_name(id.club, state)).clicked() {
                pick = Some(Pick::Club(id.club));
            }
            record_cells(ui, f);
            let estimate = estimate(f, data);
            ui.colored_label(theme::BRONZE, tr!("estimate", value = estimate));
            ui.label(asked_wage_text(f, data));
            ui.horizontal(|ui| {
                let observe = Move::Observe(Target::Rival(id));
                toggle(ui, moves, observe, &observe_label(f), true);
                if ui.button(tr!("negotiate")).clicked() {
                    pick = Some(Pick::Negotiate(id, estimate));
                }
            });
            ui.end_row();
        }
    });
    pick
}

/// The club's fighters in activity, each with its contract and whether it is for sale,
/// to put on the transfer list or take off it, to make not for sale or for sale again,
/// with the rival clubs' offers to accept or refuse, to renew in its contract's last
/// season on terms to offer, or to release.
fn sale_grid(
    ui: &mut egui::Ui,
    state: &GameState,
    moves: &mut Vec<Move>,
    data: &DesignData,
) -> Option<Pick> {
    let mut pick = None;
    market_grid("club market").show(ui, |ui| {
        ui.strong(tr!("roster-fighter"));
        for title in [tr!("roster-age"), tr!("roster-fame"), tr!("record-title")] {
            ui.strong(title);
        }
        ui.strong(tr!("market-contract"));
        ui.strong(tr!("market-sale-status"));
        ui.strong(tr!("market-actions"));
        ui.end_row();
        let season = state.date(data).season;
        for (i, f) in state.fighters.iter().enumerate() {
            if f.status != Status::Active {
                continue;
            }
            let (name, class) = ranked_name(Contender::Club(i), state, data);
            ui.horizontal(|ui| {
                ui.label(egui::RichText::new(name).strong().color(theme::BRONZE));
                ui.label(egui::RichText::new(class).small().weak());
            });
            record_cells(ui, f);
            match f.contract {
                Some(c) if c.ends <= season => {
                    let text = tr!("contract-last-season", wage = c.wage, bonus = c.win_bonus);
                    ui.colored_label(theme::BLOOD, text)
                }
                Some(c) => ui.label(tr!(
                    "contract-until",
                    wage = c.wage,
                    bonus = c.win_bonus,
                    season = c.ends
                )),
                None => ui.label(""),
            };
            let plain = ui.visuals().widgets.inactive.bg_fill;
            let unsellable = unsellable_text(f, state, data);
            if state.transfer_list.contains(&i) {
                chip(ui, tr!("market-listed"), theme::BRONZE.gamma_multiply(0.6));
            } else if state.kept.contains(&i) {
                chip(ui, tr!("keep"), plain);
            } else if let Some(text) = &unsellable {
                chip(ui, text, plain);
            } else {
                ui.label("");
            }
            // Not wrapped: a grid's cell starts with no width, a wrapped row there would
            // break after each letter.
            ui.horizontal(|ui| {
                if state.transfer_list.contains(&i) {
                    toggle(ui, moves, Move::Unlist(i), &tr!("unlist"), true);
                } else if state.kept.contains(&i) {
                    toggle(ui, moves, Move::Unkeep(i), &tr!("unkeep"), true);
                } else if unsellable.is_none() {
                    let keep = Move::Keep(i);
                    toggle(ui, moves, keep, &tr!("keep"), state.may_move(keep, data));
                    let list = Move::List(i);
                    toggle(ui, moves, list, &tr!("list"), state.may_move(list, data));
                }
                for offer in state.sale_offers.iter().filter(|o| o.fighter == i) {
                    let accept = Move::AcceptOffer {
                        fighter: i,
                        rival: offer.rival,
                    };
                    let label = tr!(
                        "accept-offer",
                        club = club_name(offer.rival, state),
                        price = offer.price
                    );
                    toggle(ui, moves, accept, &label, state.may_move(accept, data));
                    if ui.button(tr!("refuse-offer")).clicked() {
                        pick = Some(Pick::Refuse(*offer));
                    }
                }
                let asked = GameState::asked_terms(f, data);
                let renew = Move::Renew(i, asked);
                if state.may_move(renew, data) {
                    let label = tr!("renew", wage = asked.wage);
                    terms_toggle(ui, moves, renew, &label, true, data);
                }
                toggle(ui, moves, Move::Release(i), &tr!("release"), true);
            });
            ui.end_row();
        }
    });
    pick
}
