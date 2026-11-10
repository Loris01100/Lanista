//! The market view and the sheet of a fighter out of the club.

use lanista_core::colosseum::{ColosseumId, Contender};
use lanista_core::design_data::DesignData;
use lanista_core::economy::Terms;
use lanista_core::fighter::ClassId;
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
    class_key_name, class_name, club_name, colosseum_name, fighter_name, free_agent_name,
    move_text, potential_text, range_texts, record_text, rival_fighter_name, stage_name,
    stat_names, temperament_name, terms_text, unsellable_text, weeks,
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
    /// The only class shown, if one is chosen.
    pub(super) class: Option<ClassId>,
    /// Whether only the fighters observed at least once are shown.
    pub(super) seen: bool,
}

impl Default for MarketFilter {
    fn default() -> Self {
        Self {
            age: (0, 99),
            fame: 0,
            wins: 0,
            win_rate: 0,
            class: None,
            seen: false,
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
            ..Self::default()
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
            && self.class.is_none_or(|c| c == fighter.class)
            && (!self.seen || fighter.observed_weeks > 0)
    }

    /// The filters, one under the other, each under its name: class, age, fame, wins,
    /// share of wins, then whether only the fighters observed are shown.
    fn column(&mut self, ui: &mut egui::Ui, data: &DesignData) {
        ui.label(tr!("filter-class"));
        let name = |c: Option<ClassId>| {
            c.and_then(|c| data.class(c))
                .map_or_else(|| tr!("filter-every-class"), |c| class_key_name(&c.key))
        };
        egui::ComboBox::from_id_salt("market class")
            .width(ui.available_width())
            .selected_text(name(self.class))
            .show_ui(ui, |ui| {
                ui.selectable_value(&mut self.class, None, name(None));
                for c in (0..).zip(&data.classes).map(|(i, _)| Some(ClassId(i))) {
                    ui.selectable_value(&mut self.class, c, name(c));
                }
            });
        ui.add_space(4.0);
        ui.label(tr!("filter-age"));
        ui.horizontal(|ui| {
            ui.add(egui::DragValue::new(&mut self.age.0).range(0..=self.age.1));
            ui.label(tr!("filter-to"));
            ui.add(egui::DragValue::new(&mut self.age.1).range(self.age.0..=99));
        });
        ui.add_space(4.0);
        ui.spacing_mut().slider_width = ui.available_width();
        ui.label(format!("{} · {}", tr!("filter-fame"), self.fame));
        ui.add(egui::Slider::new(&mut self.fame, 0..=100).show_value(false));
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            ui.label(tr!("filter-wins"));
            ui.add(egui::DragValue::new(&mut self.wins).range(0..=999));
        });
        ui.add_space(4.0);
        ui.label(format!("{} · {} %", tr!("filter-win-rate"), self.win_rate));
        ui.add(egui::Slider::new(&mut self.win_rate, 0..=100).show_value(false));
        ui.add_space(4.0);
        ui.checkbox(&mut self.seen, tr!("filter-seen"));
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
            let label = egui::Label::new(header_text(title)).sense(egui::Sense::click());
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
    /// room, the tabs, the search and the filters; in the middle the free agents, the
    /// rival fighters or the club's fighters, a fighter out of the club only if it passes
    /// the search and the filters; on the right the card of the fighter whose row was
    /// clicked, the list's first until one is.
    pub(super) fn market(&mut self, ui: &mut egui::Ui) {
        dense(ui.style_mut());
        let panel = |fill| {
            egui::Frame::new()
                .fill(fill)
                .inner_margin(egui::Margin::symmetric(18, 16))
        };
        let bar = egui::Frame::new()
            .fill(theme::INK)
            .stroke(egui::Stroke::new(1.0_f32, theme::WOOD_LIGHTER))
            .inner_margin(egui::Margin::symmetric(18, 8));
        egui::TopBottomPanel::bottom("market week")
            .frame(bar)
            .show_inside(ui, |ui| self.week_moves(ui));
        egui::SidePanel::left("market tabs")
            .resizable(false)
            .exact_width(SIDE_WIDTH)
            .frame(panel(theme::WOOD))
            .show_inside(ui, |ui| self.market_side(ui));
        let mut pick = None;
        egui::SidePanel::right("market card")
            .resizable(false)
            .exact_width(CARD_WIDTH)
            .frame(panel(theme::WOOD))
            .show_inside(ui, |ui| pick = self.market_card(ui));
        let list = egui::Frame::new().inner_margin(egui::Margin {
            left: 18,
            right: 14,
            top: 14,
            bottom: 0,
        });
        let mut first = None;
        egui::CentralPanel::default()
            .frame(list)
            .show_inside(ui, |ui| {
                let filter = (self.market_filter, self.market_search.as_str());
                let sort = &mut self.market_sort;
                let (data, state) = (&self.data, &self.state);
                let listed = (&self.decisions.moves[..], self.market_shown);
                let tab = self.market_tab;
                let list =
                    egui::ScrollArea::vertical()
                        .auto_shrink(false)
                        .show(ui, |ui| match tab {
                            MarketTab::FreeAgents => {
                                free_agent_list(ui, state, listed, (filter, sort), data)
                            }
                            MarketTab::Rivals => {
                                rival_list(ui, state, listed, (filter, sort), data)
                            }
                            MarketTab::Yours => sale_list(ui, state, listed.1, data),
                        });
                let (picked, top) = list.inner;
                pick = pick.take().or(picked);
                first = top;
            });
        if self
            .market_shown
            .is_none_or(|c| tab_of(c) != self.market_tab)
        {
            self.market_shown = first;
        }
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

    /// The bar under the market: this week's moves, each in its plaque to cancel, the
    /// price a rival club asked for its fighter and the offer being written; the
    /// market's rules on its title.
    fn week_moves(&mut self, ui: &mut egui::Ui) {
        let data = &self.data;
        let state = &self.state;
        let moves = &mut self.decisions.moves;
        let mut close = false;
        let mut cancel = None;
        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing.x = 12.0;
            let title = display(ui, tr!("market-this-week"), 17.0);
            ui.label(title.color(theme::BRONZE))
                .on_hover_text(tr!("market-rules"));
            if moves.is_empty() && state.counter_offer.is_none() && self.offer_draft.is_none() {
                ui.label(egui::RichText::new(tr!("market-no-move")).weak());
            }
            if let Some((id, price)) = state.counter_offer {
                plaque(ui, |ui| {
                    ui.label(tr!(
                        "market-counter-offer",
                        club = club_name(id.club, state),
                        price = price,
                        fighter = rival_fighter_name(id, state, data)
                    ));
                    if ui.small_button(tr!("market-offer-this-price")).clicked() {
                        set_offer(moves, id, price);
                    }
                });
            }
            if let Some((id, price)) = &mut self.offer_draft {
                plaque(ui, |ui| {
                    ui.label(tr!(
                        "market-offer-for",
                        fighter = rival_fighter_name(*id, state, data)
                    ));
                    ui.add(egui::DragValue::new(price).speed(5));
                    let allowed = state.may_move(Move::Offer(*id, *price), data);
                    let make = egui::Button::new(tr!("market-make-offer"));
                    if ui.add_enabled(allowed, make).clicked() {
                        set_offer(moves, *id, *price);
                        close = true;
                    }
                    close |= ui.small_button(tr!("cancel")).clicked();
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
                });
            }
            for (n, &m) in moves.iter().enumerate() {
                plaque(ui, |ui| {
                    ui.label(move_text(m, state, data));
                    let button = egui::Button::new(egui::RichText::new("×").weak()).frame(false);
                    if ui.add(button).on_hover_text(tr!("cancel")).clicked() {
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

    /// The market's left column: the roster's room, the tabs with how many each lists,
    /// the search and the filters but on the club's own tab, then the button clearing
    /// them at the bottom.
    fn market_side(&mut self, ui: &mut egui::Ui) {
        let active = self.state.active_fighters();
        let max = self.data.economy.max_fighters;
        let free = u32::try_from(active).map_or(0, |a| max.saturating_sub(a));
        ui.heading(tr!("view-market"));
        let room = tr!("market-roster", active = active, max = max, free = free);
        if free == 0 {
            chip(ui, Icon::Full, room);
        } else {
            ui.label(egui::RichText::new(room).weak());
        }
        ui.add_space(14.0);
        let search = self.market_search.as_str();
        let [free, rivals, yours] = market_counts(&self.state, (self.market_filter, search));
        for (tab, text, count) in [
            (MarketTab::FreeAgents, tr!("market-tab-free"), free),
            (MarketTab::Rivals, tr!("market-tab-rivals"), rivals),
            (MarketTab::Yours, tr!("market-tab-yours"), yours),
        ] {
            if market_tab(ui, text, count, self.market_tab == tab) {
                self.market_tab = tab;
            }
        }
        ui.add_space(8.0);
        ui.separator();
        ui.add_space(8.0);
        let filters = self.market_tab != MarketTab::Yours;
        if filters {
            ui.label(tr!("market-search"));
            let field = egui::TextEdit::singleline(&mut self.market_search)
                .hint_text(tr!("market-search-hint"))
                .desired_width(f32::INFINITY);
            ui.add(field);
            ui.add_space(4.0);
            self.market_filter.column(ui, &self.data);
            ui.with_layout(egui::Layout::bottom_up(egui::Align::Min), |ui| {
                let default = MarketFilter::default();
                let clear = egui::Button::new(tr!("filter-clear"))
                    .min_size(egui::vec2(ui.available_width(), 32.0));
                let cleared = self.market_filter == default && self.market_search.is_empty();
                if ui.add_enabled(!cleared, clear).clicked() {
                    self.market_filter = default;
                    self.market_search.clear();
                }
            });
        }
    }

    /// The market's right column: the card of the fighter picked in the list shown, or
    /// a word to pick one; returns what was clicked.
    fn market_card(&mut self, ui: &mut egui::Ui) -> Option<Pick> {
        let shown = self.market_shown.filter(|&c| tab_of(c) == self.market_tab);
        let target = match shown {
            Some(Contender::FreeAgent(a)) => Target::FreeAgent(a),
            Some(Contender::Rival(id)) => Target::Rival(id),
            Some(Contender::Club(i)) => {
                let moves = &mut self.decisions.moves;
                return club_card(ui, i, &self.state, moves, &self.data);
            }
            None => {
                pick_hint(ui);
                return None;
            }
        };
        let mut moves = std::mem::take(&mut self.decisions.moves);
        let mut compared = self.compared;
        let pick = self.outside_card(ui, target, &mut moves, &mut compared);
        self.decisions.moves = moves;
        self.compared = compared;
        pick
    }

    /// The card of a free agent or a rival club's fighter: its club, who it is, how
    /// long it was observed, its estimated value, what the recruiter sees of it face to
    /// face with one of the club's, its potential, temperament and wage, its standings,
    /// then its moves at the bottom; returns what was clicked.
    fn outside_card(
        &self,
        ui: &mut egui::Ui,
        target: Target,
        moves: &mut Vec<Move>,
        compared: &mut Option<usize>,
    ) -> Option<Pick> {
        let data = &self.data;
        let state = &self.state;
        let (f, contender) = match target {
            Target::FreeAgent(a) => (state.free_agents.get(a), Contender::FreeAgent(a)),
            Target::Rival(id) => (state.rival_fighter(id), Contender::Rival(id)),
        };
        let Some(f) = f.filter(|f| f.status == Status::Active) else {
            pick_hint(ui);
            return None;
        };
        let estimate = estimate(f, data);
        let mut pick = None;
        egui::TopBottomPanel::bottom("market card moves")
            .show_separator_line(false)
            .show_inside(ui, |ui| {
                pick = outside_moves(ui, (target, f), (state, moves), estimate, data);
            });
        let scouting = state.scouting();
        egui::ScrollArea::vertical()
            .id_salt("market card")
            .show(ui, |ui| {
                ui.add_space(6.0);
                match target {
                    Target::FreeAgent(_) => {
                        ui.label(header_text(&tr!("market-free-agent")));
                    }
                    Target::Rival(id) => {
                        let club = club_name(id.club, state).to_uppercase();
                        if ui.link(egui::RichText::new(club).small()).clicked() {
                            pick = Some(Pick::Club(id.club));
                        }
                    }
                }
                ui.heading(stage_name(&f.name));
                card_profile(ui, f, data);
                ui.horizontal_wrapped(|ui| {
                    let seen = f.observed_weeks > 0;
                    pill(ui, observed_text(f), seen.then_some(theme::BRONZE));
                    if let Target::Rival(_) = target {
                        let value = tr!("estimated-value-of", value = estimate);
                        pill(ui, value, Some(theme::BRONZE));
                    }
                });
                super::record_chips(ui, f.record);
                ui.add_space(10.0);
                let mine = scouted_title(ui, compared, state);
                range_bands(ui, f, mine, (state, &scouting), data);
                if let Some(mine) = mine {
                    let name = stage_name(&mine.name);
                    let hint = tr!("market-band-hint", fighter = name);
                    ui.label(egui::RichText::new(hint).small().weak());
                }
                ui.add_space(8.0);
                let temperament = if personality_seen(f, &scouting) {
                    temperament_name(f.personality.temperament)
                } else {
                    "?".into()
                };
                boxes(
                    ui,
                    [
                        (tr!("potential"), potential_text(f, &scouting)),
                        (tr!("market-temperament"), temperament),
                        (tr!("terms-wage"), asked_wage_text(f, data)),
                    ],
                );
                ui.add_space(8.0);
                self.outside_standings(ui, contender);
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
        flip(moves, m);
    }
    response
}

/// Adds `m` to the week's moves, or takes it out when chosen already. An observation
/// replaces another.
fn flip(moves: &mut Vec<Move>, m: Move) {
    if moves.contains(&m) {
        moves.retain(|&other| other != m);
    } else {
        if matches!(m, Move::Observe(_)) {
            moves.retain(|other| !matches!(other, Move::Observe(_)));
        }
        moves.push(m);
    }
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

/// The tab of the market a fighter is listed on.
fn tab_of(contender: Contender) -> MarketTab {
    match contender {
        Contender::FreeAgent(_) => MarketTab::FreeAgents,
        Contender::Rival(_) => MarketTab::Rivals,
        Contender::Club(_) => MarketTab::Yours,
    }
}

/// Whether `search` is empty or found, whatever the case, in one of `texts`.
fn searched(search: &str, texts: &[&str]) -> bool {
    let search = search.trim().to_lowercase();
    search.is_empty() || texts.iter().any(|t| t.to_lowercase().contains(&search))
}

/// How many fighters each tab lists: the free agents and rival fighters the filter
/// and the search show, and the club's fighters in activity.
fn market_counts(state: &GameState, (filter, search): (MarketFilter, &str)) -> [usize; 3] {
    let listed = |f: &CareerFighter| f.status == Status::Active && filter.shows(f);
    let free = (state.free_agents.iter())
        .filter(|f| listed(f) && searched(search, &[&stage_name(&f.name)]))
        .count();
    let rivals = (0..state.rivals.len())
        .flat_map(|club| {
            let name = club_name(club, state);
            (state.rivals[club].fighters.iter())
                .filter(move |f| listed(f) && searched(search, &[&stage_name(&f.name), &name]))
        })
        .count();
    [free, rivals, state.active_fighters()]
}

/// One of the market's tabs, a full-width row with how many it lists on the right,
/// on a lighter wood when `shown`; whether it was clicked.
fn market_tab(ui: &mut egui::Ui, text: String, count: usize, shown: bool) -> bool {
    let size = egui::vec2(ui.available_width(), 36.0);
    let (rect, response) = ui.allocate_exact_size(size, egui::Sense::click());
    let fill = if shown {
        theme::WOOD_LIGHT
    } else if response.hovered() {
        ROW_HOVER
    } else {
        egui::Color32::TRANSPARENT
    };
    let painter = ui.painter();
    painter.rect_filled(rect, 4.0, fill);
    let font = egui::TextStyle::Body.resolve(ui.style());
    let inner = rect.shrink2(egui::vec2(10.0, 0.0));
    let left = egui::Align2::LEFT_CENTER;
    painter.text(
        inner.left_center(),
        left,
        text,
        font.clone(),
        theme::PARCHMENT,
    );
    let color = if shown {
        LIGHT_BRONZE
    } else {
        ui.visuals().weak_text_color()
    };
    let right = egui::Align2::RIGHT_CENTER;
    painter.text(inner.right_center(), right, count.to_string(), font, color);
    response
        .on_hover_cursor(egui::CursorIcon::PointingHand)
        .clicked()
}

/// `text` in the display font at `size`.
fn display(ui: &egui::Ui, text: String, size: f32) -> egui::RichText {
    let mut font = egui::TextStyle::Heading.resolve(ui.style());
    font.size = size;
    egui::RichText::new(text).font(font)
}

/// Width of the market's left column.
const SIDE_WIDTH: f32 = 236.0;

/// Width of the market's card.
const CARD_WIDTH: f32 = 440.0;

/// Width of the bar under a record in the lists.
const RECORD_BAR_WIDTH: f32 = 132.0;

/// Size of a record's figures in the lists, between the small print and the body.
const RECORD_TEXT_SIZE: f32 = 13.0;

/// A record's wins in figures, lighter than the bar's to read on wood.
const LIGHT_LAUREL: egui::Color32 = egui::Color32::from_rgb(0x9D, 0xBA, 0x62);

/// A record's losses in figures, lighter than the bar's to read on wood.
const LIGHT_BLOOD: egui::Color32 = egui::Color32::from_rgb(0xE0, 0x67, 0x5C);

/// Width of a stat's name on the card.
const BAND_LABEL_WIDTH: f32 = 44.0;

/// Width of a stat's range in figures on the card.
const BAND_TEXT_WIDTH: f32 = 60.0;

/// The band of a range on the card: a darker bronze, under the pale mark.
const BAND_FILL: egui::Color32 = egui::Color32::from_rgb(0x8C, 0x6A, 0x33);

/// Height of a row of the market's lists: a name over its class.
const ROW_HEIGHT: f32 = 42.0;

/// Height of the titles' row of the market's lists.
const HEADER_HEIGHT: f32 = 26.0;

/// Shrinks the text and the widgets of the market to the mockup's sizes, smaller
/// than the other screens' so that its list, filters and card hold more.
fn dense(style: &mut egui::Style) {
    for (text, size) in [
        (egui::TextStyle::Small, 12.0),
        (egui::TextStyle::Body, 15.0),
        (egui::TextStyle::Button, 15.0),
    ] {
        if let Some(font) = style.text_styles.get_mut(&text) {
            font.size = size;
        }
    }
    style.spacing.interact_size.y = 26.0;
    style.spacing.item_spacing.y = 6.0;
    style.spacing.button_padding.y = 4.0;
}

/// Every other row of the market's lists.
const ROW_STRIPE: egui::Color32 = egui::Color32::from_rgb(0x29, 0x22, 0x1C);

/// A row of the market's lists or a tab under the pointer.
const ROW_HOVER: egui::Color32 = egui::Color32::from_rgb(0x3A, 0x2F, 0x26);

/// The figures that stand out on the market: values, weeks observed, counts.
const LIGHT_BRONZE: egui::Color32 = egui::Color32::from_rgb(0xE4, 0xB8, 0x67);

/// Room between two columns of the market's lists.
const COLUMN_GAP: f32 = 10.0;

/// Room at each end of a row of the market's lists.
const ROW_PADDING: f32 = 8.0;

/// The least width of the names' column, however narrow the window.
const NAME_MIN_WIDTH: f32 = 150.0;

/// The most width of the names' column: past it, the other columns share the room.
const NAME_MAX_WIDTH: f32 = 300.0;

/// A column of a market list after the name's: its width, and whether its figures are
/// aligned right.
type Column = (f32, bool);

/// The free agents' columns after the name: age, fame, record, wage, weeks observed.
const FREE_COLUMNS: [Column; 5] = [
    (44.0, true),
    (60.0, true),
    (150.0, false),
    (80.0, true),
    (70.0, true),
];

/// The rival fighters' columns after the name: age, fame, record, value, wage, weeks
/// observed.
const RIVAL_COLUMNS: [Column; 6] = [
    (44.0, true),
    (60.0, true),
    (150.0, false),
    (80.0, true),
    (80.0, true),
    (70.0, true),
];

/// The club's fighters' columns after the name: age, fame, record, contract, sale.
const SALE_COLUMNS: [Column; 5] = [
    (44.0, true),
    (60.0, true),
    (150.0, false),
    (240.0, false),
    (140.0, false),
];

/// A row of a market list across the whole width, `height` high, its name's cell
/// taking what `columns` leave up to `NAME_MAX_WIDTH`, the columns widened alike to
/// share the rest; striped, framed in bronze when `selected`, clicked
/// anywhere when `sense` says so, a line under it. Returns its response, the name's
/// cell and the others; a text too long for its cell is cut.
fn list_row<const N: usize>(
    ui: &mut egui::Ui,
    columns: [Column; N],
    (stripe, selected): (bool, bool),
    (sense, height): (egui::Sense, f32),
) -> (egui::Response, egui::Ui, [egui::Ui; N]) {
    let width = ui.available_width();
    let (rect, response) = ui.allocate_exact_size(egui::vec2(width, height), sense);
    let hovered = sense.senses_click() && ui.rect_contains_pointer(rect);
    let fill = if selected {
        theme::WOOD_LIGHT
    } else if hovered {
        ROW_HOVER
    } else if stripe {
        ROW_STRIPE
    } else {
        egui::Color32::TRANSPARENT
    };
    let painter = ui.painter();
    painter.rect_filled(rect, 0.0, fill);
    let line = egui::Stroke::new(1.0_f32, theme::WOOD);
    painter.hline(rect.x_range(), rect.bottom() - 0.5, line);
    if selected {
        let stroke = egui::Stroke::new(1.0_f32, theme::BRONZE);
        painter.rect_stroke(rect, 0.0, stroke, egui::StrokeKind::Inside);
    }
    let gaps = COLUMN_GAP * f32::from(u8::try_from(N).unwrap_or(u8::MAX));
    let widths: f32 = columns.iter().map(|c| c.0).sum();
    let room = width - widths - gaps - 2.0 * ROW_PADDING;
    let first = room.clamp(NAME_MIN_WIDTH, NAME_MAX_WIDTH);
    let stretch = 1.0 + (room - first).max(0.0) / widths;
    let columns = columns.map(|(w, right)| (w * stretch, right));
    let mut x = rect.left() + ROW_PADDING;
    let mut cell = |ui: &mut egui::Ui, w: f32, layout: egui::Layout| {
        let rect = egui::Rect::from_x_y_ranges(x..=x + w, rect.y_range());
        x += w + COLUMN_GAP;
        let mut child = ui.new_child(egui::UiBuilder::new().max_rect(rect).layout(layout));
        child.set_clip_rect(rect.intersect(ui.clip_rect()));
        child.spacing_mut().item_spacing = egui::vec2(0.0, 0.0);
        child.style_mut().interaction.selectable_labels = false;
        child.style_mut().wrap_mode = Some(egui::TextWrapMode::Truncate);
        child
    };
    let name = cell(ui, first, egui::Layout::left_to_right(egui::Align::Center));
    let cells = columns.map(|(w, right)| {
        let layout = if right {
            egui::Layout::right_to_left(egui::Align::Center)
        } else {
            egui::Layout::left_to_right(egui::Align::Center)
        };
        cell(ui, w, layout)
    });
    (response, name, cells)
}

/// How many fighters a market list shows, of `total` when a search or a filter
/// leaves some out.
fn list_count(ui: &mut egui::Ui, shown: usize, total: Option<usize>) {
    ui.horizontal(|ui| {
        let count = tr!("market-count", count = shown);
        ui.label(egui::RichText::new(count).strong());
        if let Some(total) = total.filter(|&t| t != shown) {
            let of = tr!("market-count-of", total = total);
            ui.label(egui::RichText::new(of).small().weak());
        }
    });
    ui.add_space(4.0);
}

/// The titles' row of a market list: `name` over the names, then each column's
/// title, clicked to sort by it when it has a `SortBy`.
fn list_header<const N: usize>(
    ui: &mut egui::Ui,
    columns: [Column; N],
    titles: [(String, Option<SortBy>); N],
    sort: &mut MarketSort,
) {
    let size = (egui::Sense::hover(), HEADER_HEIGHT);
    let (_, mut name, mut cells) = list_row(ui, columns, (false, false), size);
    name.label(header_text(&tr!("market-col-fighter")));
    for (cell, (title, by)) in cells.iter_mut().zip(titles) {
        match by {
            Some(by) => sort.title(cell, &title, by),
            None => {
                cell.label(header_text(&title));
            }
        }
    }
}

/// A column's title of a market list: small, faded, in capitals.
fn header_text(title: &str) -> egui::RichText {
    egui::RichText::new(title.to_uppercase()).small().weak()
}

/// A fighter's name, in bronze when `picked`, just over the line `below` draws in
/// small print, the two in the middle of the row.
fn name_lines(ui: &mut egui::Ui, name: String, picked: bool, below: impl FnOnce(&mut egui::Ui)) {
    let rect = ui.max_rect();
    let middle = rect.center().y;
    let mut name = egui::RichText::new(name).strong();
    if picked {
        name = name.color(theme::BRONZE);
    }
    let line = |ui: &mut egui::Ui, y: std::ops::RangeInclusive<f32>, align: egui::Align| {
        let rect = egui::Rect::from_x_y_ranges(rect.x_range(), y);
        let layout = egui::Layout::left_to_right(align);
        let mut child = ui.new_child(egui::UiBuilder::new().max_rect(rect).layout(layout));
        child.set_clip_rect(rect.intersect(ui.clip_rect()));
        child
    };
    let overlap = 3.0;
    line(ui, rect.top()..=middle + overlap, egui::Align::Max).label(name);
    below(&mut line(
        ui,
        middle - overlap..=rect.bottom(),
        egui::Align::Min,
    ));
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

/// A record in figures, wins in green, losses in red, draws faded, with the share of
/// fights won, just over a bar of its wins and losses, the two in the middle of the
/// cell.
fn record_cell(ui: &mut egui::Ui, fighter: &CareerFighter) {
    let r = fighter.record;
    let fights = r.wins + r.losses + r.draws;
    let rate = (r.wins * 100).checked_div(fights).unwrap_or(0);
    let font = egui::FontId::proportional(RECORD_TEXT_SIZE);
    let plain = theme::PARCHMENT;
    let mut job = egui::text::LayoutJob::default();
    for (text, color) in [
        (r.wins.to_string(), LIGHT_LAUREL),
        (" – ".into(), plain),
        (r.losses.to_string(), LIGHT_BLOOD),
        (" – ".into(), plain),
        (r.draws.to_string(), ui.visuals().weak_text_color()),
        (format!(" · {rate} %"), plain),
    ] {
        job.append(&text, 0.0, egui::TextFormat::simple(font.clone(), color));
    }
    let galley = ui.fonts(|f| f.layout_job(job));
    let rect = ui.max_rect();
    let middle = rect.center().y;
    let painter = ui.painter();
    let top = egui::pos2(rect.left(), middle + 2.0 - galley.size().y);
    painter.galley(top, galley, plain);
    let size = egui::vec2(RECORD_BAR_WIDTH.min(rect.width()), 4.0);
    let rect = egui::Rect::from_min_size(egui::pos2(rect.left(), middle + 4.0), size);
    painter.rect_filled(rect, 2.0, theme::INK);
    let won = rect.width() * super::fraction(r.wins, fights);
    let lost = rect.width() * super::fraction(r.losses, fights);
    let left = rect.left();
    let bar = |from: f32, to: f32| egui::Rect::from_x_y_ranges(from..=to, rect.y_range());
    painter.rect_filled(bar(left, left + won), 2.0, theme::LAUREL);
    painter.rect_filled(bar(left + won, left + won + lost), 2.0, theme::BLOOD);
}

/// Age and fame, aligned right.
fn age_fame_cells(age: &mut egui::Ui, fame: &mut egui::Ui, fighter: &CareerFighter) {
    age.label(fighter.age.to_string());
    fame.label(egui::RichText::new(fighter.fame.value().to_string()).strong());
}

/// How many weeks the fighter was observed, faded when never.
fn seen_cell(ui: &mut egui::Ui, fighter: &CareerFighter) {
    match fighter.observed_weeks {
        0 => ui.label(egui::RichText::new("—").weak()),
        n => ui.colored_label(LIGHT_BRONZE, tr!("market-seen-weeks", weeks = n)),
    };
}

/// The free agents on the market `filter` shows, the one `shown` selected, those the
/// week's `moves` are on in bronze; returns the one clicked.
fn free_agent_list(
    ui: &mut egui::Ui,
    state: &GameState,
    (moves, shown): (&[Move], Option<Contender>),
    ((filter, search), sort): ((MarketFilter, &str), &mut MarketSort),
    data: &DesignData,
) -> (Option<Pick>, Option<Contender>) {
    let titles = [
        (tr!("roster-age"), Some(SortBy::Age)),
        (tr!("market-col-fame"), Some(SortBy::Fame)),
        (tr!("record-title"), Some(SortBy::Wins)),
        (tr!("market-col-wage"), Some(SortBy::Wage)),
        (tr!("market-seen"), None),
    ];
    let active = (state.free_agents.iter().enumerate()).filter(|(_, f)| f.status == Status::Active);
    let total = active.clone().count();
    let mut rows: Vec<_> = active
        .filter(|(_, f)| filter.shows(f) && searched(search, &[&stage_name(&f.name)]))
        .collect();
    sort.apply(&mut rows, |(_, f)| f, data);
    list_count(ui, rows.len(), Some(total));
    list_header(ui, FREE_COLUMNS, titles, sort);
    ui.spacing_mut().item_spacing.y = 0.0;
    let first = rows.first().map(|&(a, _)| Contender::FreeAgent(a));
    let mut pick = None;
    for (n, (a, f)) in rows.into_iter().enumerate() {
        let contender = Contender::FreeAgent(a);
        let marks = (n % 2 == 1, shown == Some(contender));
        let (row, mut name, [mut age, mut fame, mut record, mut wage, mut seen]) =
            list_row(ui, FREE_COLUMNS, marks, (egui::Sense::click(), ROW_HEIGHT));
        let (shown_name, class) = ranked_name(contender, state, data);
        name_lines(
            &mut name,
            shown_name,
            picked(moves, Target::FreeAgent(a)),
            |ui| {
                ui.label(egui::RichText::new(class).small().weak());
            },
        );
        age_fame_cells(&mut age, &mut fame, f);
        record_cell(&mut record, f);
        wage.label(asked_wage_text(f, data));
        seen_cell(&mut seen, f);
        if row.clicked() {
            pick = Some(Pick::Show(contender));
        }
    }
    (pick, first)
}

/// The rival clubs' fighters in activity `filter` shows, the one `shown` selected,
/// those the week's `moves` are on in bronze, each club under its fighter's name;
/// returns the fighter or the club clicked.
fn rival_list(
    ui: &mut egui::Ui,
    state: &GameState,
    (moves, shown): (&[Move], Option<Contender>),
    ((filter, search), sort): ((MarketFilter, &str), &mut MarketSort),
    data: &DesignData,
) -> (Option<Pick>, Option<Contender>) {
    let titles = [
        (tr!("roster-age"), Some(SortBy::Age)),
        (tr!("market-col-fame"), Some(SortBy::Fame)),
        (tr!("record-title"), Some(SortBy::Wins)),
        (tr!("market-col-value"), Some(SortBy::Value)),
        (tr!("market-col-wage"), Some(SortBy::Wage)),
        (tr!("market-seen"), None),
    ];
    let active = (state.rivals.iter().enumerate())
        .flat_map(|(club, r)| {
            let fighters = r.fighters.iter().enumerate();
            fighters.map(move |(fighter, f)| (RivalId { club, fighter }, f))
        })
        .filter(|(_, f)| f.status == Status::Active);
    let total = active.clone().count();
    let mut rows: Vec<_> = active
        .filter(|(id, f)| {
            let club = club_name(id.club, state);
            filter.shows(f) && searched(search, &[&stage_name(&f.name), &club])
        })
        .collect();
    sort.apply(&mut rows, |(_, f)| f, data);
    list_count(ui, rows.len(), Some(total));
    list_header(ui, RIVAL_COLUMNS, titles, sort);
    ui.spacing_mut().item_spacing.y = 0.0;
    let first = rows.first().map(|&(id, _)| Contender::Rival(id));
    let mut pick = None;
    for (n, (id, f)) in rows.into_iter().enumerate() {
        let contender = Contender::Rival(id);
        let marks = (n % 2 == 1, shown == Some(contender));
        let (row, mut name, [mut age, mut fame, mut record, mut value, mut wage, mut seen]) =
            list_row(ui, RIVAL_COLUMNS, marks, (egui::Sense::click(), ROW_HEIGHT));
        let (shown_name, class) = ranked_name(contender, state, data);
        name_lines(
            &mut name,
            shown_name,
            picked(moves, Target::Rival(id)),
            |ui| {
                ui.label(egui::RichText::new(format!("{class} · ")).small().weak());
                let club = egui::RichText::new(club_name(id.club, state)).small();
                if ui.link(club).clicked() {
                    pick = Some(Pick::Club(id.club));
                }
            },
        );
        age_fame_cells(&mut age, &mut fame, f);
        record_cell(&mut record, f);
        value.colored_label(LIGHT_BRONZE, tr!("estimate", value = estimate(f, data)));
        wage.label(asked_wage_text(f, data));
        seen_cell(&mut seen, f);
        if row.clicked() && pick.is_none() {
            pick = Some(Pick::Show(contender));
        }
    }
    (pick, first)
}

/// The club's fighters in activity, the one `shown` selected, each with its contract
/// and whether it is for sale; returns the one clicked.
fn sale_list(
    ui: &mut egui::Ui,
    state: &GameState,
    shown: Option<Contender>,
    data: &DesignData,
) -> (Option<Pick>, Option<Contender>) {
    let mut sort = MarketSort::default();
    let titles = [
        (tr!("roster-age"), None),
        (tr!("market-col-fame"), None),
        (tr!("record-title"), None),
        (tr!("market-contract"), None),
        (tr!("market-sale-status"), None),
    ];
    let active = (state.fighters.iter().enumerate()).filter(|(_, f)| f.status == Status::Active);
    list_count(ui, active.clone().count(), None);
    list_header(ui, SALE_COLUMNS, titles, &mut sort);
    ui.spacing_mut().item_spacing.y = 0.0;
    let season = state.date(data).season;
    let mut pick = None;
    let first = active.clone().next().map(|(i, _)| Contender::Club(i));
    for (n, (i, f)) in active.enumerate() {
        let contender = Contender::Club(i);
        let marks = (n % 2 == 1, shown == Some(contender));
        let (row, mut name, [mut age, mut fame, mut record, mut contract, mut sale]) =
            list_row(ui, SALE_COLUMNS, marks, (egui::Sense::click(), ROW_HEIGHT));
        let (shown_name, class) = ranked_name(contender, state, data);
        name_lines(&mut name, shown_name, false, |ui| {
            ui.label(egui::RichText::new(class).small().weak());
        });
        age_fame_cells(&mut age, &mut fame, f);
        record_cell(&mut record, f);
        contract_cell(&mut contract, f, season);
        sale_status_cell(
            &mut sale,
            i,
            unsellable_text(f, state, data).as_deref(),
            state,
        );
        if row.clicked() {
            pick = Some(Pick::Show(contender));
        }
    }
    (pick, first)
}

/// A word to pick a fighter in the list, where its card would be.
fn pick_hint(ui: &mut egui::Ui) {
    ui.add_space(12.0);
    ui.label(egui::RichText::new(tr!("market-pick")).weak());
}

/// A fighter's class, age and fame, faded, under its name on the card.
fn card_profile(ui: &mut egui::Ui, f: &CareerFighter, data: &DesignData) {
    let profile = tr!(
        "market-card-profile",
        class = class_name(f, data),
        age = f.age,
        fame = f.fame.value()
    );
    ui.label(egui::RichText::new(profile).weak());
}

/// A rounded tag: in `color` on a lighter wood, or faded on ink without one.
fn pill(ui: &mut egui::Ui, text: String, color: Option<egui::Color32>) {
    let fill = if color.is_some() {
        theme::WOOD_LIGHT
    } else {
        theme::INK
    };
    egui::Frame::new()
        .fill(fill)
        .corner_radius(10)
        .inner_margin(egui::Margin::symmetric(10, 3))
        .show(ui, |ui| {
            let text = egui::RichText::new(text);
            ui.label(match color {
                Some(c) => text.color(c),
                None => text.weak(),
            });
        });
}

/// A plaque of the bar under the market, its content in a row.
fn plaque(ui: &mut egui::Ui, add: impl FnOnce(&mut egui::Ui)) {
    egui::Frame::new()
        .fill(theme::WOOD)
        .corner_radius(4)
        .inner_margin(egui::Margin::symmetric(8, 3))
        .show(ui, |ui| ui.horizontal(add));
}

/// Three boxes side by side, each a faded label over its value.
fn boxes(ui: &mut egui::Ui, items: [(String, String); 3]) {
    ui.columns(3, |columns| {
        for (column, (label, value)) in columns.iter_mut().zip(items) {
            egui::Frame::new()
                .fill(theme::INK)
                .corner_radius(6)
                .inner_margin(8)
                .show(column, |ui| {
                    ui.set_width(ui.available_width());
                    ui.label(egui::RichText::new(label).small().weak());
                    ui.label(value);
                });
        }
    });
}

/// The title over the ranges the recruiter sees, with a list of the club's fighters
/// in activity to face them with, the first one until another is chosen; returns the
/// one faced, none when the club has nobody.
fn scouted_title<'a>(
    ui: &mut egui::Ui,
    compared: &mut Option<usize>,
    state: &'a GameState,
) -> Option<&'a CareerFighter> {
    let active: Vec<(usize, &CareerFighter)> = (0..)
        .zip(&state.fighters)
        .filter(|(_, f)| f.status == Status::Active)
        .collect();
    let mine = active
        .iter()
        .find(|(i, _)| Some(*i) == *compared)
        .or(active.first())
        .copied();
    ui.horizontal(|ui| {
        let title = display(ui, tr!("market-scouted"), 16.0);
        ui.label(title.color(theme::BRONZE));
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            let Some((faced, f)) = mine else { return };
            egui::ComboBox::from_id_salt("market compared")
                .selected_text(stage_name(&f.name))
                .show_ui(ui, |ui| {
                    for &(i, f) in &active {
                        if ui
                            .selectable_label(i == faced, stage_name(&f.name))
                            .clicked()
                        {
                            *compared = Some(i);
                        }
                    }
                });
            ui.label(egui::RichText::new(tr!("market-faced-with")).small().weak());
        });
    });
    mine.map(|(_, f)| f)
}

/// Each stat the recruiter sees of `f` as a band over its range, on a scale of the
/// world's best, `mine`'s value as a pale mark, the range in figures beside it; a
/// « ? » for a stat not seen.
fn range_bands(
    ui: &mut egui::Ui,
    f: &CareerFighter,
    mine: Option<&CareerFighter>,
    (state, scouting): (&GameState, &Scouting),
    data: &DesignData,
) {
    let best = super::best_stats(state).to_array();
    let ranges = stat_ranges(f, &data.economy.market, scouting);
    let mine = mine.map(|m| m.stats.to_array());
    let rows = stat_names()
        .into_iter()
        .zip(ranges)
        .zip(range_texts(f, scouting, data))
        .zip(best)
        .enumerate();
    ui.spacing_mut().item_spacing.y = 9.0;
    let label = |ui: &mut egui::Ui, w: f32, text: egui::RichText, align: egui::Align| {
        let size = egui::vec2(w, ui.spacing().interact_size.y);
        let layout = egui::Layout::left_to_right(egui::Align::Center).with_main_align(align);
        let (rect, _) = ui.allocate_exact_size(size, egui::Sense::hover());
        ui.new_child(egui::UiBuilder::new().max_rect(rect).layout(layout))
            .label(text);
    };
    for (k, (((name, range), text), best)) in rows {
        ui.horizontal(|ui| {
            label(
                ui,
                BAND_LABEL_WIDTH,
                egui::RichText::new(name),
                egui::Align::Min,
            );
            let width = ui.available_width() - BAND_TEXT_WIDTH - ui.spacing().item_spacing.x;
            let size = egui::vec2(width, 10.0);
            let (rect, _) = ui.allocate_exact_size(size, egui::Sense::hover());
            let painter = ui.painter();
            painter.rect_filled(rect, 5.0, theme::INK);
            let own = mine.and_then(|m| m.get(k).copied());
            let high = range.map_or(0, |(_, high)| high);
            let max = best.max(high).max(own.unwrap_or(0)).max(1);
            let x = |v: u32| rect.left() + rect.width() * super::fraction(v, max);
            if let Some((low, high)) = range {
                let to = x(high).max(x(low) + 4.0);
                let band = egui::Rect::from_x_y_ranges(x(low)..=to, rect.y_range());
                painter.rect_filled(band, 5.0, BAND_FILL);
            }
            if let Some(own) = own {
                let center = egui::pos2(x(own), rect.center().y);
                let mark = egui::Rect::from_center_size(center, egui::vec2(3.0, 18.0));
                painter.rect_filled(mark, 1.0, theme::PARCHMENT);
            }
            let text = egui::RichText::new(text).weak();
            label(ui, BAND_TEXT_WIDTH, text, egui::Align::Max);
        });
    }
}

/// A wide button for the card's main move, in bronze, or shown chosen once it is;
/// whether it was clicked. Only a chosen one may be clicked when not `enabled`.
fn primary_button(ui: &mut egui::Ui, text: String, chosen: bool, enabled: bool) -> bool {
    let (color, fill) = if chosen {
        (theme::BRONZE, theme::WOOD_LIGHT)
    } else {
        (theme::INK, theme::BRONZE)
    };
    let text = egui::RichText::new(text).strong().color(color);
    let button = egui::Button::new(text)
        .fill(fill)
        .selected(chosen)
        .min_size(egui::vec2(ui.available_width(), 40.0));
    ui.add_enabled(enabled || chosen, button).clicked()
}

/// The moves at the bottom of the card of a fighter out of the club: to recruit it or
/// negotiate for it, to observe it, to open its whole sheet; returns what was clicked.
fn outside_moves(
    ui: &mut egui::Ui,
    (target, f): (Target, &CareerFighter),
    (state, moves): (&GameState, &mut Vec<Move>),
    estimate: u32,
    data: &DesignData,
) -> Option<Pick> {
    let mut pick = None;
    ui.add_space(6.0);
    match target {
        Target::FreeAgent(a) => {
            let sign = Move::Sign(a, GameState::asked_terms(f, data));
            let chosen = moves.iter().copied().find(|&m| same_fighter(m, sign));
            let allowed = state.may_move(sign, data) && state.can_hire(data);
            let label = tr!("sign-for", wage = asked_wage_text(f, data));
            if primary_button(ui, label, chosen.is_some(), allowed) {
                pick = Some(Pick::Terms(chosen.unwrap_or(sign)));
            }
        }
        Target::Rival(id) => {
            let offered = moves
                .iter()
                .any(|m| matches!(m, Move::Offer(o, _) if *o == id));
            let label = tr!("negotiate-from", value = estimate);
            if primary_button(ui, label, offered, true) {
                pick = Some(Pick::Negotiate(id, estimate));
            }
        }
    }
    ui.columns(2, |columns| {
        let observe = Move::Observe(target);
        let size = egui::vec2(columns[0].available_width(), 30.0);
        let button = egui::Button::new(observe_label(f))
            .selected(moves.contains(&observe))
            .min_size(size);
        if columns[0].add(button).clicked() {
            flip(moves, observe);
        }
        let sheet = egui::Button::new(tr!("market-full-sheet")).min_size(size);
        if columns[1].add(sheet).clicked() {
            pick = Some(Pick::Open(target));
        }
    });
    let note = match target {
        Target::FreeAgent(_) => tr!("market-free-agents"),
        Target::Rival(_) => tr!("market-offer-answer"),
    };
    ui.label(egui::RichText::new(note).small().weak());
    ui.add_space(6.0);
    pick
}

/// The card of the club's fighter `i`: who it is, its contract, whether it is for
/// sale, then its moves: to put on the transfer list or take off it, to make not for
/// sale or for sale again, the rival clubs' offers to accept or refuse, to renew in its
/// contract's last season on terms to offer, or to release, and its whole sheet at the
/// bottom; returns what was clicked.
fn club_card(
    ui: &mut egui::Ui,
    i: usize,
    state: &GameState,
    moves: &mut Vec<Move>,
    data: &DesignData,
) -> Option<Pick> {
    let Some(f) = state.fighters.get(i).filter(|f| f.status == Status::Active) else {
        pick_hint(ui);
        return None;
    };
    let mut pick = None;
    egui::TopBottomPanel::bottom("market club card moves")
        .show_separator_line(false)
        .show_inside(ui, |ui| {
            ui.add_space(6.0);
            let size = egui::vec2(ui.available_width(), 30.0);
            let sheet = egui::Button::new(tr!("market-full-sheet")).min_size(size);
            if ui.add(sheet).clicked() {
                pick = Some(Pick::Sheet(i));
            }
            ui.add_space(6.0);
        });
    egui::ScrollArea::vertical()
        .id_salt("market club card")
        .show(ui, |ui| {
            ui.add_space(6.0);
            ui.heading(stage_name(&f.name));
            card_profile(ui, f, data);
            super::record_chips(ui, f.record);
            ui.add_space(10.0);
            let title = |text: String| egui::RichText::new(text).strong().color(theme::BRONZE);
            ui.label(title(tr!("market-contract")));
            contract_cell(ui, f, state.date(data).season);
            let unsellable = unsellable_text(f, state, data);
            sale_status_cell(ui, i, unsellable.as_deref(), state);
            ui.add_space(10.0);
            ui.label(title(tr!("market-actions")));
            let row = ui.horizontal_wrapped(|ui| {
                sale_actions(ui, (i, f), unsellable.is_none(), state, moves, data)
            });
            if row.inner.is_some() {
                pick = row.inner;
            }
        });
    pick
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
