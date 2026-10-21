//! The market view and the sheet of a fighter out of the club.

use lanista_core::colosseum::{ColosseumId, Contender};
use lanista_core::design_data::DesignData;
use lanista_core::fighter::career::{CareerFighter, Status};
use lanista_core::market::{Move, Target, value};
use lanista_core::rivals::RivalId;
use lanista_core::staff::Scouting;
use lanista_core::state::GameState;
use reesch::egui;

use super::colosseums::not_ranked;
use super::{ClubScreen, View};
use crate::lang::tr;
use crate::texts::{
    club_name, colosseum_name, colosseum_rules_name, condition_text, fighter_name, free_agent_name,
    move_text, outside_personality_text, potential_text, range_texts, record_text,
    rival_fighter_name, stat_names, unsellable_text, weeks,
};

/// What a click on the market asks for.
enum Pick {
    /// Open the sheet of this fighter.
    Open(Target),
    /// Write an offer for this rival fighter, starting from this price.
    Negotiate(RivalId, u32),
}

impl ClubScreen {
    /// The market: this week's moves, the answer to the last offer and the offer being
    /// written, the free agents, the rival fighters to observe or negotiate for, and the
    /// club's fighters to sell or release. Stats out of the club are hidden or seen as
    /// ranges until observed; a name opens what the club sees of the fighter.
    pub(super) fn market(&mut self, ui: &mut egui::Ui) {
        // In the colosseums of the club's continent.
        let home: Vec<usize> = (0..self.data.world.colosseums.len())
            .filter(|&c| self.data.world.colosseums[c].continent == self.state.continent)
            .collect();
        // Only those on the market: the others stay listed, and are many.
        let free_standings: Vec<Vec<String>> = (0..)
            .zip(&self.state.free_agents)
            .map(|(a, f)| {
                if f.status != Status::Active {
                    return Vec::new();
                }
                let all = self.standings(Contender::FreeAgent(a));
                home.iter().map(|&c| all[c].clone()).collect()
            })
            .collect();
        let data = &self.data;
        let state = &self.state;
        let moves = &mut self.decisions.moves;
        ui.heading(tr!("view-market"));
        ui.label(tr!(
            "market-intro",
            active = state.active_fighters(),
            max = data.economy.max_fighters
        ));
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
        let mut close = false;
        if let Some((id, price)) = &mut self.offer_draft {
            ui.horizontal(|ui| {
                ui.label(tr!(
                    "market-offer-for",
                    fighter = rival_fighter_name(*id, state, data)
                ));
                ui.add(egui::DragValue::new(price).speed(5));
                if let Some(text) = state
                    .rival_fighter(*id)
                    .and_then(|f| unsellable_text(f, state, data))
                {
                    ui.label(text);
                }
                let standing = state.standing(id.club, data);
                if !data.world.relations.standings.get(standing).deals {
                    ui.label(tr!(
                        "market-rival-refuses",
                        club = club_name(id.club, state)
                    ));
                }
                let allowed = state.may_move(Move::Offer(*id, *price), data);
                if ui
                    .add_enabled(allowed, egui::Button::new(tr!("market-make-offer")))
                    .clicked()
                {
                    set_offer(moves, *id, *price);
                    close = true;
                }
                close |= ui.button(tr!("cancel")).clicked();
            });
        }
        if close {
            self.offer_draft = None;
        }
        let mut cancel = None;
        for (n, &m) in moves.iter().enumerate() {
            ui.horizontal(|ui| {
                ui.strong(tr!("this-week", what = move_text(m, state, data)));
                if ui.button(tr!("cancel")).clicked() {
                    cancel = Some(n);
                }
            });
        }
        if let Some(n) = cancel {
            moves.remove(n);
        }
        let pick = egui::ScrollArea::vertical().show(ui, |ui| {
            let free = free_agent_grid(ui, state, &free_standings, moves, data);
            let rival = rival_grid(ui, state, moves, data);
            sale_grid(ui, state, moves, data);
            rival.or(free)
        });
        match pick.inner {
            Some(Pick::Open(target)) => self.go_to(View::Outside(target)),
            Some(Pick::Negotiate(id, price)) => self.offer_draft = Some((id, price)),
            None => {}
        }
    }

    /// What the club sees of a free agent or a rival club's fighter: age, fame, record,
    /// the stats seen, the potential and personality the recruiter sees, its standing in
    /// each colosseum, and the market moves it allows.
    pub(super) fn outside(&mut self, ui: &mut egui::Ui, target: Target) {
        self.back_button(ui);
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
            });
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
                        let sign = Move::Sign(a);
                        toggle(ui, moves, sign, &tr!("sign"), state.may_move(sign, data));
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
            self.view = View::Market;
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

/// Replaces the week's offer with one of `price` for rival fighter `id`.
fn set_offer(moves: &mut Vec<Move>, id: RivalId, price: u32) {
    moves.retain(|m| !matches!(m, Move::Offer(..)));
    moves.push(Move::Offer(id, price));
}

/// A button adding `m` to the week's moves, or taking it out when chosen already; only a
/// chosen one may be clicked when not `enabled`. An observation replaces another.
fn toggle(ui: &mut egui::Ui, moves: &mut Vec<Move>, m: Move, label: &str, enabled: bool) {
    let chosen = moves.contains(&m);
    let button = egui::SelectableLabel::new(chosen, label);
    if ui.add_enabled(enabled || chosen, button).clicked() {
        if chosen {
            moves.retain(|&other| other != m);
        } else {
            if matches!(m, Move::Observe(_)) {
                moves.retain(|other| !matches!(other, Move::Observe(_)));
            }
            moves.push(m);
        }
    }
}

fn observe_label(fighter: &CareerFighter) -> String {
    match fighter.observed_weeks {
        0 => tr!("observe"),
        weeks => tr!("observe-again", weeks = weeks),
    }
}

/// A grid's titles: `first`, the stats, then `last`.
fn market_header(ui: &mut egui::Ui, first: &[String], last: &[String]) {
    for title in first.iter().chain(&stat_names()).chain(last) {
        ui.strong(title);
    }
    ui.end_row();
}

/// What the club sees of a fighter out of it, with the recruiter's `scouting`: `name`,
/// age, fame, record, the stats seen, the potential estimated and the personality.
/// Returns whether the name was clicked.
fn outside_cells(
    ui: &mut egui::Ui,
    name: String,
    fighter: &CareerFighter,
    scouting: &Scouting,
    data: &DesignData,
) -> bool {
    let clicked = ui.link(name).clicked();
    ui.label(fighter.age.to_string());
    ui.label(fighter.fame.value().to_string());
    ui.label(record_text(fighter.record));
    for range in range_texts(fighter, scouting, data) {
        ui.label(range);
    }
    ui.label(potential_text(fighter, scouting));
    ui.label(outside_personality_text(fighter, scouting));
    clicked
}

/// The titles of a fighter out of the club, before its stats.
fn outside_titles() -> [String; 4] {
    [
        tr!("roster-fighter"),
        tr!("roster-age"),
        tr!("roster-fame"),
        tr!("record-title"),
    ]
}

/// The free agents on the market, with their standing in each colosseum of the club's
/// continent (`standings`, by index in `GameState::free_agents`); returns the one to
/// open.
fn free_agent_grid(
    ui: &mut egui::Ui,
    state: &GameState,
    standings: &[Vec<String>],
    moves: &mut Vec<Move>,
    data: &DesignData,
) -> Option<Pick> {
    let mut pick = None;
    ui.add_space(6.0);
    ui.strong(tr!("market-free-agents"));
    egui::Grid::new("free agents").striped(true).show(ui, |ui| {
        let mut last = vec![tr!("potential"), tr!("personality-title")];
        last.extend(
            data.world
                .colosseums
                .iter()
                .filter(|c| c.continent == state.continent)
                .map(colosseum_rules_name),
        );
        last.extend([String::new(), String::new()]);
        market_header(ui, &outside_titles(), &last);
        let scouting = state.scouting();
        for (a, f) in state.free_agents.iter().enumerate() {
            if f.status != Status::Active {
                continue;
            }
            if outside_cells(ui, free_agent_name(a, f, data), f, &scouting, data) {
                pick = Some(Pick::Open(Target::FreeAgent(a)));
            }
            for standing in standings.get(a).into_iter().flatten() {
                ui.label(standing);
            }
            let observe = Move::Observe(Target::FreeAgent(a));
            toggle(ui, moves, observe, &observe_label(f), true);
            let sign = Move::Sign(a);
            toggle(ui, moves, sign, &tr!("sign"), state.may_move(sign, data));
            ui.end_row();
        }
    });
    pick
}

/// The rival clubs' fighters in activity; returns the one to open or to negotiate for,
/// with a first price.
fn rival_grid(
    ui: &mut egui::Ui,
    state: &GameState,
    moves: &mut Vec<Move>,
    data: &DesignData,
) -> Option<Pick> {
    let mut pick = None;
    ui.add_space(6.0);
    ui.strong(tr!("market-rival-fighters"));
    egui::Grid::new("rival fighters")
        .striped(true)
        .show(ui, |ui| {
            market_header(
                ui,
                &outside_titles(),
                &[
                    tr!("potential"),
                    tr!("personality-title"),
                    tr!("estimated-value"),
                    String::new(),
                    String::new(),
                ],
            );
            let scouting = state.scouting();
            for (club, rival) in state.rivals.iter().enumerate() {
                for (fighter, f) in rival.fighters.iter().enumerate() {
                    if f.status != Status::Active {
                        continue;
                    }
                    let id = RivalId { club, fighter };
                    let name = rival_fighter_name(id, state, data);
                    if outside_cells(ui, name, f, &scouting, data) {
                        pick = Some(Pick::Open(Target::Rival(id)));
                    }
                    let estimate = estimate(f, data);
                    ui.label(tr!("estimate", value = estimate));
                    let observe = Move::Observe(Target::Rival(id));
                    toggle(ui, moves, observe, &observe_label(f), true);
                    if ui.button(tr!("negotiate")).clicked() {
                        pick = Some(Pick::Negotiate(id, estimate));
                    }
                    ui.end_row();
                }
            }
        });
    pick
}

/// The club's fighters in activity, each to put on the transfer list or take off it,
/// to make not for sale or for sale again, with the rival clubs' offers to accept, or
/// to release.
fn sale_grid(ui: &mut egui::Ui, state: &GameState, moves: &mut Vec<Move>, data: &DesignData) {
    ui.add_space(6.0);
    ui.strong(tr!("market-your-fighters"));
    egui::Grid::new("club market").striped(true).show(ui, |ui| {
        for (i, f) in state.fighters.iter().enumerate() {
            if f.status != Status::Active {
                continue;
            }
            ui.label(fighter_name(i, f, data));
            if state.transfer_list.contains(&i) {
                toggle(ui, moves, Move::Unlist(i), &tr!("unlist"), true);
            } else if state.kept.contains(&i) {
                toggle(ui, moves, Move::Unkeep(i), &tr!("unkeep"), true);
            } else if let Some(text) = unsellable_text(f, state, data) {
                ui.label(text);
            } else {
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
            }
            toggle(ui, moves, Move::Release(i), &tr!("release"), true);
            ui.end_row();
        }
    });
}
