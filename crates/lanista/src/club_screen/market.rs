//! The market view and the sheet of a fighter out of the club.

use lanista_core::colosseum::{ColosseumId, Contender};
use lanista_core::design_data::DesignData;
use lanista_core::economy::Terms;
use lanista_core::fighter::career::{CareerFighter, Status};
use lanista_core::market::{Move, SaleOffer, Target, personality_seen, stat_ranges, value};
use lanista_core::rivals::RivalId;
use lanista_core::staff::Scouting;
use lanista_core::state::GameState;
use reesch::egui;

use super::colosseums::{arrow, not_ranked, ranked_name};
use super::{ClubScreen, View, chip, face_row, half_bar};
use crate::lang::tr;
use crate::texts::{
    class_name, club_name, colosseum_name, fighter_name, free_agent_name, move_text,
    potential_text, range_texts, record_text, rival_fighter_name, stat_names, temperament_name,
    terms_text, unsellable_text, weeks,
};
use crate::theme::{self, Icon};

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

    /// The filters, one under the other: age, fame, wins, share of wins, and a button
    /// clearing them.
    fn column(&mut self, ui: &mut egui::Ui) {
        egui::Grid::new("market filters").show(ui, |ui| {
            ui.label(tr!("filter-age"));
            ui.horizontal(|ui| {
                ui.add(egui::DragValue::new(&mut self.age.0).range(0..=self.age.1));
                ui.label(tr!("filter-to"));
                ui.add(egui::DragValue::new(&mut self.age.1).range(self.age.0..=99));
            });
            ui.end_row();
            ui.label(tr!("filter-fame"));
            ui.add(egui::DragValue::new(&mut self.fame).range(0..=100));
            ui.end_row();
            ui.label(tr!("filter-wins"));
            ui.add(egui::DragValue::new(&mut self.wins).range(0..=999));
            ui.end_row();
            ui.label(tr!("filter-win-rate"));
            ui.add(
                egui::DragValue::new(&mut self.win_rate)
                    .range(0..=100)
                    .suffix(" %"),
            );
            ui.end_row();
        });
        let default = Self::default();
        if ui
            .add_enabled(*self != default, egui::Button::new(tr!("filter-clear")))
            .clicked()
        {
            *self = default;
        }
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
    /// Show this fighter in the market's card.
    Show(Contender),
    /// Open the sheet of this fighter.
    Open(Target),
    /// Open the sheet of the club's fighter.
    Sheet(usize),
    /// Write an offer for this rival fighter, starting from this price.
    Negotiate(RivalId, u32),
    /// Open the page of this rival club.
    Club(usize),
    /// Turn down this rival club's offer for one of the club's fighters.
    Refuse(SaleOffer),
    /// Negotiate this signing or renewal.
    Terms(Move),
}

impl ClubScreen {
    /// The market, in three columns over this week's moves: on the left the roster's
    /// room, the tabs and the filters; in the middle the free agents, the rival fighters
    /// or the club's fighters, a fighter out of the club only if it passes the filters;
    /// on the right the card of the fighter whose name was clicked, with its moves.
    pub(super) fn market(&mut self, ui: &mut egui::Ui) {
        egui::TopBottomPanel::bottom("market week").show_inside(ui, |ui| self.week_moves(ui));
        egui::SidePanel::left("market tabs")
            .resizable(false)
            .default_width(200.0)
            .show_inside(ui, |ui| self.market_side(ui));
        let mut pick = None;
        egui::SidePanel::right("market card")
            .resizable(false)
            .default_width(360.0)
            .show_inside(ui, |ui| pick = self.market_card(ui));
        egui::CentralPanel::default().show_inside(ui, |ui| {
            let filter = self.market_filter;
            let sort = &mut self.market_sort;
            let (data, state) = (&self.data, &self.state);
            let listed = (&self.decisions.moves[..], self.market_shown);
            let tab = self.market_tab;
            let grid = egui::ScrollArea::vertical().show(ui, |ui| match tab {
                MarketTab::FreeAgents => free_agent_grid(ui, state, listed, (filter, sort), data),
                MarketTab::Rivals => rival_grid(ui, state, listed, (filter, sort), data),
                MarketTab::Yours => sale_grid(ui, state, listed.1, data),
            });
            pick = pick.take().or(grid.inner);
        });
        match pick {
            Some(Pick::Show(shown)) => self.market_shown = Some(shown),
            Some(Pick::Open(target)) => self.go_to(View::Outside(target)),
            Some(Pick::Sheet(i)) => self.go_to(View::Sheet(i)),
            Some(Pick::Negotiate(id, price)) => self.offer_draft = Some((id, price)),
            Some(Pick::Club(club)) => self.go_to(View::Rival(club)),
            Some(Pick::Terms(m)) => self.terms_draft = Some(m),
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

    /// The bar under the market: this week's moves, each to cancel, the price a rival
    /// club asked for its fighter and the offer being written.
    fn week_moves(&mut self, ui: &mut egui::Ui) {
        let data = &self.data;
        let state = &self.state;
        let moves = &mut self.decisions.moves;
        let mut close = false;
        let mut cancel = None;
        ui.horizontal_wrapped(|ui| {
            let title = egui::RichText::new(tr!("market-this-week"));
            ui.label(title.strong().color(theme::BRONZE));
            if moves.is_empty() && state.counter_offer.is_none() && self.offer_draft.is_none() {
                ui.label(egui::RichText::new(tr!("market-no-move")).weak());
            }
            if let Some((id, price)) = state.counter_offer {
                ui.separator();
                ui.label(tr!(
                    "market-counter-offer",
                    club = club_name(id.club, state),
                    price = price,
                    fighter = rival_fighter_name(id, state, data)
                ));
                if ui.button(tr!("market-offer-this-price")).clicked() {
                    set_offer(moves, id, price);
                }
            }
            if let Some((id, price)) = &mut self.offer_draft {
                ui.separator();
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
                    let refuses = tr!("market-rival-refuses", club = club_name(id.club, state));
                    ui.colored_label(theme::BLOOD, refuses);
                }
            }
            for (n, &m) in moves.iter().enumerate() {
                ui.separator();
                ui.label(move_text(m, state, data));
                if ui.small_button(tr!("cancel")).clicked() {
                    cancel = Some(n);
                }
            }
        });
        if close {
            self.offer_draft = None;
        }
        if let Some(n) = cancel {
            self.decisions.moves.remove(n);
        }
    }

    /// The market's left column: the roster's room, the tabs, the filters but on the
    /// club's own tab, and the rules.
    fn market_side(&mut self, ui: &mut egui::Ui) {
        let active = self.state.active_fighters();
        let max = self.data.economy.max_fighters;
        ui.heading(tr!("view-market"));
        let full = u32::try_from(active).is_ok_and(|n| n >= max);
        let icon = if full { Icon::Full } else { Icon::Class };
        chip(ui, icon, tr!("market-roster", active = active, max = max));
        ui.add_space(8.0);
        let [free, rivals, yours] = market_counts(&self.state, self.market_filter);
        let tab = &mut self.market_tab;
        for (shown, text) in [
            (MarketTab::FreeAgents, tr!("market-tab-free", count = free)),
            (MarketTab::Rivals, tr!("market-tab-rivals", count = rivals)),
            (MarketTab::Yours, tr!("market-tab-yours", count = yours)),
        ] {
            ui.selectable_value(tab, shown, text);
        }
        ui.separator();
        if self.market_tab != MarketTab::Yours {
            self.market_filter.column(ui);
            ui.separator();
        }
        ui.label(egui::RichText::new(tr!("market-rules")).small().weak());
    }

    /// The market's right column: the card of the fighter picked in the list shown, or
    /// a word to pick one; returns what was clicked.
    fn market_card(&mut self, ui: &mut egui::Ui) -> Option<Pick> {
        let shown = self.market_shown.filter(|&c| {
            let tab = match c {
                Contender::FreeAgent(_) => MarketTab::FreeAgents,
                Contender::Rival(_) => MarketTab::Rivals,
                Contender::Club(_) => MarketTab::Yours,
            };
            tab == self.market_tab
        });
        let target = match shown {
            Some(Contender::FreeAgent(a)) => Target::FreeAgent(a),
            Some(Contender::Rival(id)) => Target::Rival(id),
            Some(Contender::Club(i)) => {
                let moves = &mut self.decisions.moves;
                return club_card(ui, i, &self.state, moves, &self.data);
            }
            None => {
                ui.label(egui::RichText::new(tr!("market-pick")).weak());
                return None;
            }
        };
        let mut moves = std::mem::take(&mut self.decisions.moves);
        let mut compared = self.compared;
        let pick = egui::ScrollArea::vertical()
            .id_salt("market card")
            .show(ui, |ui| {
                self.outside_card(ui, target, &mut moves, &mut compared)
            })
            .inner;
        self.decisions.moves = moves;
        self.compared = compared;
        pick
    }

    /// The card of a free agent or a rival club's fighter: its club, who it is, how
    /// long it was observed, what the recruiter sees of it face to face with one of the
    /// club's, its standings, then its moves; returns what was clicked.
    fn outside_card(
        &self,
        ui: &mut egui::Ui,
        target: Target,
        moves: &mut Vec<Move>,
        compared: &mut Option<usize>,
    ) -> Option<Pick> {
        let data = &self.data;
        let state = &self.state;
        let (f, name, contender) = match target {
            Target::FreeAgent(a) => {
                let f = state.free_agents.get(a)?;
                (f, free_agent_name(a, f, data), Contender::FreeAgent(a))
            }
            Target::Rival(id) => {
                let f = state.rival_fighter(id)?;
                (f, rival_fighter_name(id, state, data), Contender::Rival(id))
            }
        };
        if f.status != Status::Active {
            ui.label(egui::RichText::new(tr!("market-pick")).weak());
            return None;
        }
        let mut pick = None;
        match target {
            Target::FreeAgent(_) => {
                ui.label(
                    egui::RichText::new(tr!("market-free-agents"))
                        .small()
                        .weak(),
                );
            }
            Target::Rival(id) => {
                let club = tr!("outside-club", club = club_name(id.club, state));
                if ui.link(club).clicked() {
                    pick = Some(Pick::Club(id.club));
                }
            }
        }
        ui.heading(name);
        ui.label(tr!(
            "market-card-profile",
            class = class_name(f, data),
            age = f.age,
            fame = f.fame.value()
        ));
        super::record_chips(ui, f.record);
        ui.label(observed_text(f));
        let estimate = estimate(f, data);
        if let Target::Rival(_) = target {
            ui.colored_label(theme::BRONZE, tr!("estimated-value-of", value = estimate));
        }
        ui.add_space(8.0);
        let scouting = state.scouting();
        comparison(ui, compared, f, (state, &scouting), data);
        let temperament = if personality_seen(f, &scouting) {
            temperament_name(f.personality.temperament)
        } else {
            "?".into()
        };
        for (label, text) in [
            (tr!("potential"), potential_text(f, &scouting)),
            (tr!("personality-title"), temperament),
        ] {
            ui.label(tr!("labelled", label = label, text = text));
        }
        ui.add_space(8.0);
        self.outside_standings(ui, contender);
        ui.add_space(8.0);
        ui.horizontal_wrapped(|ui| {
            match target {
                Target::FreeAgent(a) => {
                    let sign = Move::Sign(a, GameState::asked_terms(f, data));
                    let allowed = state.may_move(sign, data) && state.can_hire(data);
                    let label = tr!("sign-for", wage = asked_wage_text(f, data));
                    if let Some(m) = terms_button(ui, moves, sign, &label, allowed) {
                        pick = Some(Pick::Terms(m));
                    }
                }
                Target::Rival(id) => {
                    if ui.button(tr!("negotiate")).clicked() {
                        pick = Some(Pick::Negotiate(id, estimate));
                    }
                }
            }
            toggle(ui, moves, Move::Observe(target), &observe_label(f), true);
            if ui.button(tr!("market-full-sheet")).clicked() {
                pick = Some(Pick::Open(target));
            }
        });
        pick
    }

    /// What the club sees of a free agent or a rival club's fighter: its sheet as the
    /// recruiter sees it, or whole for a starter, then its club, how long it was
    /// observed, the comparison with one of the club's, its standing in each colosseum,
    /// and the market moves it allows, or « Choisir » for a starter.
    pub(super) fn outside(&mut self, ui: &mut egui::Ui, target: Target) {
        let data = &self.data;
        let state = &self.state;
        let (Some(f), name) = (match target {
            Target::FreeAgent(a) => (state.free_agents.get(a), String::new()),
            Target::Rival(id) => (state.rival_fighter(id), rival_fighter_name(id, state, data)),
        }) else {
            return;
        };
        let name = match target {
            Target::FreeAgent(a) => free_agent_name(a, f, data),
            Target::Rival(_) => name,
        };
        let scouting = state.scouting();
        let starter = match target {
            Target::FreeAgent(a) => state.starters.contains(&a).then_some(a),
            Target::Rival(_) => None,
        };
        let seen = (
            super::best_stats(state),
            starter.is_none().then_some(&scouting),
        );
        let mut goto = None;
        let mut picked = None;
        let mut draft = None;
        let mut compared = self.compared;
        let mut terms_draft = self.terms_draft;
        let mut moves = std::mem::take(&mut self.decisions.moves);
        let left = |ui: &mut egui::Ui| super::sheet(ui, &name, f, seen, data);
        super::side_by_side(ui, left, |ui| {
            if let Some(a) = starter {
                ui.label(tr!("starter-text"));
                ui.horizontal(|ui| {
                    if ui.button(tr!("starter-pick")).clicked() {
                        picked = Some(a);
                    }
                    if ui.button(tr!("starter-back")).clicked() {
                        goto = Some(View::Menu);
                    }
                });
                return;
            }
            if let Target::Rival(id) = target
                && ui
                    .link(tr!("outside-club", club = club_name(id.club, state)))
                    .clicked()
            {
                goto = Some(View::Rival(id.club));
            }
            ui.label(observed_text(f));
            ui.add_space(8.0);
            comparison(ui, &mut compared, f, (state, &scouting), data);
            let contender = match target {
                Target::FreeAgent(a) => Contender::FreeAgent(a),
                Target::Rival(id) => Contender::Rival(id),
            };
            ui.add_space(8.0);
            self.outside_standings(ui, contender);
            ui.add_space(8.0);
            let moves = &mut moves;
            if f.status == Status::Active {
                ui.horizontal(|ui| {
                    toggle(ui, moves, Move::Observe(target), &observe_label(f), true);
                    match target {
                        Target::FreeAgent(a) => {
                            let sign = Move::Sign(a, GameState::asked_terms(f, data));
                            let allowed = state.may_move(sign, data);
                            if let Some(m) = terms_button(ui, moves, sign, &tr!("sign"), allowed) {
                                terms_draft = Some(m);
                            }
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
        });
        self.compared = compared;
        self.terms_draft = terms_draft;
        self.decisions.moves = moves;
        if let Some(a) = picked {
            self.pick_starter(a);
            return;
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
    /// The window laid over the screen while `terms_draft` is set, to negotiate a
    /// signing or a renewal: the fighter, what it asks, the terms offered, how it takes
    /// them and what they cost, then the offer to make, withdraw or leave.
    pub(super) fn terms_window(&mut self, ctx: &egui::Context) {
        let Some(draft) = self.terms_draft else {
            return;
        };
        let data = &self.data;
        let state = &self.state;
        let (fighter, name) = match draft {
            Move::Sign(a, _) => {
                let f = state.free_agents.get(a);
                (f, f.map(|f| free_agent_name(a, f, data)))
            }
            Move::Renew(i, _) => {
                let f = state.fighters.get(i);
                (f, f.map(|f| fighter_name(i, f, data)))
            }
            _ => (None, None),
        };
        let (Some(f), Some(name), Move::Sign(_, mut terms) | Move::Renew(_, mut terms)) =
            (fighter, name, draft)
        else {
            self.terms_draft = None;
            return;
        };
        let rules = &data.economy.contracts;
        let asked = GameState::asked_terms(f, data);
        let moves = &mut self.decisions.moves;
        let chosen = moves.iter().position(|&m| same_fighter(m, draft));
        let mut close = false;
        let modal = egui::Modal::new(egui::Id::new("terms")).show(ctx, |ui| {
            ui.set_width(TERMS_WIDTH);
            ui.heading(tr!("contract-title", fighter = name));
            ui.label(tr!(
                "contract-profile",
                class = class_name(f, data),
                age = f.age,
                record = record_text(f.record),
                morale = f.morale.value()
            ));
            ui.label(tr!("contract-asks", terms = terms_text(asked)));
            ui.separator();
            let top = asked.wage.saturating_mul(2).max(terms.wage);
            egui::Grid::new("terms").num_columns(2).show(ui, |ui| {
                ui.label(tr!("terms-wage"));
                ui.add(egui::Slider::new(&mut terms.wage, 1..=top));
                ui.end_row();
                ui.label(tr!("terms-bonus"));
                let top = asked.wage.saturating_mul(4).max(terms.win_bonus);
                ui.add(egui::Slider::new(&mut terms.win_bonus, 0..=top));
                ui.end_row();
                ui.label(tr!("terms-seasons"));
                ui.add(egui::Slider::new(&mut terms.seasons, 1..=rules.max_seasons));
                ui.end_row();
            });
            ui.label(egui::RichText::new(tr!("terms-hint")).small().weak());
            ui.separator();
            let worth = rules.worth(f, terms);
            let season = u64::from(data.calendar.season_weeks());
            ui.label(tr!(
                "contract-worth",
                worth = worth,
                season = worth.saturating_mul(season)
            ));
            let least = u64::from(rules.least_wage(f, &data.economy.market));
            // How the fighter takes the terms; below `renewal_morale`, it renews on none.
            let unhappy =
                matches!(draft, Move::Renew(..)) && f.morale.value() < rules.renewal_morale;
            let (key, color) = if unhappy {
                ("unhappy", theme::BLOOD)
            } else if worth >= least {
                ("signs", theme::LAUREL)
            } else if worth * 10 >= least * 9 {
                ("hesitates", theme::BRONZE)
            } else {
                ("refuses", theme::BLOOD)
            };
            ui.colored_label(color, tr!("contract-mood", mood = key));
            ui.add_space(4.0);
            ui.horizontal(|ui| {
                let offer = with_terms(draft, terms);
                if ui.button(tr!("contract-propose")).clicked() {
                    match chosen {
                        Some(k) => moves[k] = offer,
                        None => moves.push(offer),
                    }
                    close = true;
                }
                if let Some(k) = chosen
                    && ui.button(tr!("contract-withdraw")).clicked()
                {
                    moves.remove(k);
                    close = true;
                }
                close |= ui.button(tr!("cancel")).clicked();
            });
        });
        self.terms_draft = (!close && !modal.should_close()).then_some(with_terms(draft, terms));
    }
}

/// `m`, a signing or a renewal, on `terms`.
fn with_terms(m: Move, terms: Terms) -> Move {
    match m {
        Move::Sign(a, _) => Move::Sign(a, terms),
        Move::Renew(i, _) => Move::Renew(i, terms),
        other => other,
    }
}

/// The negotiation window's width.
const TERMS_WIDTH: f32 = 420.0;

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

/// Whether `a` and `b` sign or renew the same fighter, whatever the terms.
fn same_fighter(a: Move, b: Move) -> bool {
    match (a, b) {
        (Move::Sign(a, _), Move::Sign(b, _)) | (Move::Renew(a, _), Move::Renew(b, _)) => a == b,
        _ => false,
    }
}

/// A button opening the negotiation of `m`, a signing or a renewal on the fighter's
/// asked terms, or of the one chosen already, shown selected; returns the move to
/// negotiate. Only a chosen one may be clicked when not `enabled`.
fn terms_button(
    ui: &mut egui::Ui,
    moves: &[Move],
    m: Move,
    label: &str,
    enabled: bool,
) -> Option<Move> {
    let chosen = moves.iter().copied().find(|&other| same_fighter(other, m));
    let button = egui::SelectableLabel::new(chosen.is_some(), label);
    let clicked = ui
        .add_enabled(enabled || chosen.is_some(), button)
        .clicked();
    clicked.then_some(chosen.unwrap_or(m))
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

/// A fighter's name, in bronze when `picked` and selected when `shown`, its class in
/// small print beside it; returns whether the name was clicked.
fn name_cell(
    ui: &mut egui::Ui,
    name: String,
    class: String,
    (shown, picked): (bool, bool),
) -> bool {
    ui.horizontal(|ui| {
        let mut name = egui::RichText::new(name).strong();
        if picked {
            name = name.color(theme::BRONZE);
        }
        let clicked = ui.selectable_label(shown, name).clicked();
        ui.label(egui::RichText::new(class).small().weak());
        clicked
    })
    .inner
}

/// Whether one of the week's `moves` is on `target`.
fn picked(moves: &[Move], target: Target) -> bool {
    moves.iter().any(|&m| match (m, target) {
        (Move::Observe(t), _) => t == target,
        (Move::Sign(a, _), Target::FreeAgent(b)) => a == b,
        (Move::Offer(a, _), Target::Rival(b)) => a == b,
        _ => false,
    })
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

/// How many weeks the fighter was observed, faded when never.
fn seen_cell(ui: &mut egui::Ui, fighter: &CareerFighter) {
    match fighter.observed_weeks {
        0 => ui.label(egui::RichText::new("—").weak()),
        n => ui.colored_label(theme::BRONZE, weeks(n)),
    };
}

/// A grid's titles: those of a fighter out of the club, then `last`, each clicked to
/// sort by its column, then the weeks observed; `club` adds the club's column after
/// the name.
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
    ui.strong(tr!("market-seen"));
    ui.end_row();
}

/// The market's grids, striped and spaced alike.
fn market_grid(id: &str) -> egui::Grid {
    egui::Grid::new(id).striped(true).spacing([18.0, 6.0])
}

/// The free agents on the market `filter` shows, the one `shown` selected, those the
/// week's `moves` are on in bronze; returns the one clicked.
fn free_agent_grid(
    ui: &mut egui::Ui,
    state: &GameState,
    (moves, shown): (&[Move], Option<Contender>),
    (filter, sort): (MarketFilter, &mut MarketSort),
    data: &DesignData,
) -> Option<Pick> {
    let mut pick = None;
    market_grid("free agents").show(ui, |ui| {
        outside_header(ui, false, &[(tr!("market-asked-wage"), SortBy::Wage)], sort);
        let mut rows: Vec<_> = (state.free_agents.iter().enumerate())
            .filter(|(_, f)| f.status == Status::Active && filter.shows(f))
            .collect();
        sort.apply(&mut rows, |(_, f)| f, data);
        for (a, f) in rows {
            let contender = Contender::FreeAgent(a);
            let (name, class) = ranked_name(contender, state, data);
            let marks = (
                shown == Some(contender),
                picked(moves, Target::FreeAgent(a)),
            );
            if name_cell(ui, name, class, marks) {
                pick = Some(Pick::Show(contender));
            }
            record_cells(ui, f);
            ui.label(asked_wage_text(f, data));
            seen_cell(ui, f);
            ui.end_row();
        }
    });
    pick
}

/// The rival clubs' fighters in activity `filter` shows, the one `shown` selected,
/// those the week's `moves` are on in bronze; returns the one or the club clicked.
fn rival_grid(
    ui: &mut egui::Ui,
    state: &GameState,
    (moves, shown): (&[Move], Option<Contender>),
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
            let contender = Contender::Rival(id);
            let (name, class) = ranked_name(contender, state, data);
            let marks = (shown == Some(contender), picked(moves, Target::Rival(id)));
            if name_cell(ui, name, class, marks) {
                pick = Some(Pick::Show(contender));
            }
            if ui.link(club_name(id.club, state)).clicked() {
                pick = Some(Pick::Club(id.club));
            }
            record_cells(ui, f);
            ui.colored_label(theme::BRONZE, tr!("estimate", value = estimate(f, data)));
            ui.label(asked_wage_text(f, data));
            seen_cell(ui, f);
            ui.end_row();
        }
    });
    pick
}

/// The club's fighters in activity, the one `shown` selected, each with its contract
/// and whether it is for sale; returns the one clicked.
fn sale_grid(
    ui: &mut egui::Ui,
    state: &GameState,
    shown: Option<Contender>,
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
        ui.end_row();
        let season = state.date(data).season;
        for (i, f) in state.fighters.iter().enumerate() {
            if f.status != Status::Active {
                continue;
            }
            let contender = Contender::Club(i);
            let (name, class) = ranked_name(contender, state, data);
            if name_cell(ui, name, class, (shown == Some(contender), false)) {
                pick = Some(Pick::Show(contender));
            }
            record_cells(ui, f);
            contract_cell(ui, f, season);
            sale_status_cell(ui, i, unsellable_text(f, state, data).as_deref(), state);
            ui.end_row();
        }
    });
    pick
}

/// The card of the club's fighter `i`: who it is, its contract, whether it is for
/// sale, then its moves: to put on the transfer list or take off it, to make not for
/// sale or for sale again, the rival clubs' offers to accept or refuse, to renew in its
/// contract's last season on terms to offer, or to release; returns what was clicked.
fn club_card(
    ui: &mut egui::Ui,
    i: usize,
    state: &GameState,
    moves: &mut Vec<Move>,
    data: &DesignData,
) -> Option<Pick> {
    let Some(f) = state.fighters.get(i).filter(|f| f.status == Status::Active) else {
        ui.label(egui::RichText::new(tr!("market-pick")).weak());
        return None;
    };
    ui.heading(fighter_name(i, f, data));
    ui.label(tr!(
        "market-card-profile",
        class = class_name(f, data),
        age = f.age,
        fame = f.fame.value()
    ));
    super::record_chips(ui, f.record);
    ui.add_space(8.0);
    ui.strong(tr!("market-contract"));
    contract_cell(ui, f, state.date(data).season);
    let unsellable = unsellable_text(f, state, data);
    sale_status_cell(ui, i, unsellable.as_deref(), state);
    ui.add_space(8.0);
    let row = ui.horizontal_wrapped(|ui| {
        sale_actions(ui, (i, f), unsellable.is_none(), state, moves, data)
    });
    ui.add_space(8.0);
    if ui.button(tr!("market-full-sheet")).clicked() {
        return Some(Pick::Sheet(i));
    }
    row.inner
}

/// A fighter's contract, in blood in its last season.
fn contract_cell(ui: &mut egui::Ui, f: &CareerFighter, season: u64) {
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
}

/// Whether the club's fighter `i` is listed, kept or not for sale.
fn sale_status_cell(ui: &mut egui::Ui, i: usize, unsellable: Option<&str>, state: &GameState) {
    if state.transfer_list.contains(&i) {
        chip(ui, Icon::Listed, tr!("market-listed"));
    } else if state.kept.contains(&i) {
        chip(ui, Icon::Kept, tr!("keep"));
    } else if let Some(text) = unsellable {
        chip(ui, Icon::Recent, text);
    } else {
        ui.label("");
    }
}

/// The moves on the club's fighter `i`, and the offer refused if any.
fn sale_actions(
    ui: &mut egui::Ui,
    (i, f): (usize, &CareerFighter),
    sellable: bool,
    state: &GameState,
    moves: &mut Vec<Move>,
    data: &DesignData,
) -> Option<Pick> {
    let mut pick = None;
    if state.transfer_list.contains(&i) {
        toggle(ui, moves, Move::Unlist(i), &tr!("unlist"), true);
    } else if state.kept.contains(&i) {
        toggle(ui, moves, Move::Unkeep(i), &tr!("unkeep"), true);
    } else if sellable {
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
        if let Some(m) = terms_button(ui, moves, renew, &label, true) {
            pick = Some(Pick::Terms(m));
        }
    }
    toggle(ui, moves, Move::Release(i), &tr!("release"), true);
    pick
}
