//! The colosseums view and the page of a rival club.

use lanista_core::colosseum::{
    Colosseum, ColosseumData, ColosseumId, Contender, Continent, Reign, ReignEnd, TitleLoss,
};
use lanista_core::design_data::DesignData;
use lanista_core::fighter::career::Status;
use lanista_core::market::Target;
use lanista_core::relations::Standing;
use lanista_core::rivals::RivalId;
use lanista_core::state::GameState;
use reesch::egui;

use super::market::estimate;
use super::{ClubScreen, RANKS_SHOWN, View, chip};
use crate::lang::tr;
use crate::texts::{
    arena_effect, arena_name, class_name, club_name, colosseum_name, colosseum_rules_name,
    condition_text, contender_name, continent_name, culture_name, culture_text, date_in_sentence,
    date_text, named_club, place_name, profile_name, record_text, relation_change_text,
    relation_text, rival_fighter_name, staff_text, stage_name, wealth_name,
};
use crate::theme;

impl ClubScreen {
    /// One continent at a time, the club's first, in two tabs: its colosseums side by
    /// side, each in a card, or its rival clubs in a table. Every name opens its page.
    pub(super) fn colosseums(&mut self, ui: &mut egui::Ui) {
        self.continent_tabs(ui);
        let shown = self.shown_continent;
        let clubs = self
            .state
            .rivals
            .iter()
            .filter(|r| r.is_open() && r.continent == shown)
            .count();
        let colosseums = self
            .data
            .world
            .colosseums
            .iter()
            .filter(|c| c.continent == shown)
            .count();
        ui.horizontal(|ui| {
            let part = &mut self.shown_part;
            let name = tr!("continent-colosseums", count = colosseums);
            ui.selectable_value(part, ContinentPart::Colosseums, name);
            ui.selectable_value(
                part,
                ContinentPart::Clubs,
                tr!("continent-clubs", count = clubs),
            );
            if *part == ContinentPart::Colosseums {
                ui.separator();
                let prestige = &mut self.shown_prestige;
                ui.selectable_value(prestige, None, tr!("prestige-all"));
                for p in PRESTIGES {
                    ui.selectable_value(prestige, Some(p), prestige_name(p));
                }
            }
        });
        ui.add_space(4.0);
        let goto = match self.shown_part {
            ContinentPart::Colosseums => self.continent_colosseums(ui),
            ContinentPart::Clubs => self.continent_clubs(ui),
        };
        if let Some(view) = goto {
            self.go_to(view);
        }
    }

    /// The continents to choose from, the club's marked.
    fn continent_tabs(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            for continent in Continent::ALL {
                let mut name = continent_name(continent);
                if continent == self.state.continent {
                    name = tr!("continent-home", continent = name);
                }
                ui.selectable_value(&mut self.shown_continent, continent, name);
            }
        });
    }

    /// The colosseums of the continent shown at the prestige shown, a card each side
    /// by side; returns the page clicked.
    fn continent_colosseums(&self, ui: &mut egui::Ui) -> Option<View> {
        let shown = self.shown_continent;
        let prestige = self.shown_prestige;
        let data = &self.data;
        let state = &self.state;
        let colosseums: Vec<_> = (0..)
            .map(ColosseumId)
            .zip(state.colosseums.iter().zip(&data.world.colosseums))
            .filter(|(_, (_, rules))| {
                rules.continent == shown && prestige.is_none_or(|p| rules.prestige == p)
            })
            .collect();
        if colosseums.is_empty() {
            ui.label(tr!("colosseums-none"));
        }
        let mut goto = None;
        ui.columns(colosseums.len().max(1), |columns| {
            for ((id, (colosseum, rules)), ui) in colosseums.into_iter().zip(columns) {
                goto = colosseum_column(ui, id, colosseum, rules, state, data).or(goto);
            }
        });
        goto
    }

    /// The rival clubs of the continent shown, a row each; returns the page of the club
    /// clicked.
    fn continent_clubs(&self, ui: &mut egui::Ui) -> Option<View> {
        let shown = self.shown_continent;
        let mut goto = None;
        let rows = self.rival_rows();
        egui::ScrollArea::vertical().show(ui, |ui| {
            egui::Grid::new("rival clubs")
                .striped(true)
                .spacing([16.0, 6.0])
                .show(ui, |ui| {
                    for title in [
                        tr!("ranking-club"),
                        tr!("clubs-culture"),
                        tr!("clubs-profile"),
                        tr!("clubs-wealth"),
                        tr!("clubs-fighters"),
                        tr!("clubs-relation"),
                        tr!("clubs-titles"),
                    ] {
                        ui.strong(title);
                    }
                    ui.end_row();
                    for row in rows {
                        let rival = &self.state.rivals[row.club];
                        if !rival.is_open() || rival.continent != shown {
                            continue;
                        }
                        let name = egui::RichText::new(&row.name).strong();
                        if ui.link(name.color(theme::BRONZE)).clicked() {
                            goto = Some(View::Rival(row.club));
                        }
                        ui.label(&row.culture);
                        ui.label(&row.profile);
                        ui.label(&row.wealth);
                        ui.label(row.fighters.to_string());
                        chip(ui, &row.relation, standing_color(ui, row.standing));
                        ui.horizontal(|ui| {
                            for title in &row.titles {
                                chip(ui, title, theme::BRONZE.gamma_multiply(0.6));
                            }
                        });
                        ui.end_row();
                    }
                });
        });
        goto
    }

    /// Each rival club as its row in the table of clubs, in club order; its money stays
    /// hidden.
    pub(super) fn rival_rows(&self) -> Vec<RivalRow> {
        let data = &self.data;
        let state = &self.state;
        (0..)
            .zip(&state.rivals)
            .map(|(club, rival)| {
                RivalRow {
                club,
                name: named_club(&rival.name),
                culture: culture_name(&rival.culture),
                profile: profile_name(rival.profile),
                wealth: wealth_name(rival.wealth),
                fighters: rival.active(),
                relation: relation_text(club, state, data),
                standing: state.standing(club, data),
                titles: (0..)
                    .zip(&state.colosseums)
                    .filter(|(_, c)| {
                        matches!(c.champion, Some(Contender::Rival(id)) if id.club == club)
                    })
                    .map(|(c, _)| colosseum_name(ColosseumId(c), data))
                    .collect(),
            }
            })
            .collect()
    }

    /// Each change of rival club `club`'s relation with the player's club, newest
    /// first (see `relation_change_text`).
    pub(super) fn relation_lines(&self, club: usize) -> Vec<String> {
        let changes = self.state.relations.get(&club);
        changes
            .into_iter()
            .flat_map(|r| r.changes.iter().rev())
            .map(|&c| relation_change_text(c, &self.state, &self.data))
            .collect()
    }

    /// "Champion de Capoue : Augure n° 3 (Ludus Magnus)" for each title rival club
    /// `club` holds.
    pub(super) fn titles_of(&self, club: usize) -> Vec<String> {
        (0..)
            .zip(&self.state.colosseums)
            .filter_map(|(c, colosseum)| match colosseum.champion {
                Some(Contender::Rival(id)) if id.club == club => Some(tr!(
                    "champion-of",
                    place = colosseum_name(ColosseumId(c), &self.data),
                    fighter = rival_fighter_name(id, &self.state, &self.data)
                )),
                _ => None,
            })
            .collect()
    }

    /// Where `contender` stands in each colosseum, in colosseum order: "3e, Elo 1032",
    /// "champion, 5e, Elo 1010", or "non classé" past the ranks shown, its Elo hidden.
    pub(super) fn standings(&self, contender: Contender) -> Vec<String> {
        let rosters = self.state.rosters();
        self.state
            .colosseums
            .iter()
            .map(|colosseum| {
                let rank = colosseum
                    .active_ranking(rosters)
                    .position(|r| r.contender == contender)
                    .filter(|&rank| rank < RANKS_SHOWN);
                let mut text = rank.map_or_else(not_ranked, |rank| {
                    let elo = colosseum.elo(contender, &self.data.world);
                    let place = u32::try_from(rank + 1).unwrap_or(u32::MAX);
                    tr!("standing-ranked", place = place_name(place), elo = elo)
                });
                if colosseum.champion == Some(contender) {
                    text = tr!("standing-champion", standing = text);
                }
                text
            })
            .collect()
    }

    /// A rival club: when it closed, profile, wealth (its money stays hidden), culture,
    /// continent, staff, the titles it holds, its relation with the player's club and
    /// why, then its fighters in activity with their standing in each colosseum of its
    /// continent and their estimated value, each to negotiate for on the market.
    pub(super) fn rival_club(&mut self, ui: &mut egui::Ui, club: usize) {
        self.back_button(ui);
        if club >= self.state.rivals.len() {
            return;
        }
        self.rival_header(ui, club);
        let (goto, draft) = self.rival_fighters(ui, club);
        if let Some(draft) = draft {
            self.offer_draft = Some(draft);
            self.view = View::Market;
        } else if let Some(view) = goto {
            self.go_to(view);
        }
    }

    /// A rival club's description, titles and relation with the player's club.
    fn rival_header(&self, ui: &mut egui::Ui, club: usize) {
        let data = &self.data;
        let state = &self.state;
        let rival = &state.rivals[club];
        ui.heading(named_club(&rival.name));
        if let Some(week) = rival.closed {
            let closed = tr!("rival-closed", date = date_text(week, data));
            ui.colored_label(egui::Color32::LIGHT_RED, closed);
        }
        ui.label(tr!(
            "rival-description",
            profile = profile_name(rival.profile),
            wealth = wealth_name(rival.wealth),
            culture = culture_text(&rival.culture),
            continent = continent_name(rival.continent),
            fighters = rival.active(),
            max = data.world.roster.max
        ));
        ui.label(staff_text(&rival.staff));
        for line in self.titles_of(club) {
            ui.colored_label(egui::Color32::GOLD, line);
        }
        ui.strong(tr!(
            "rival-relation",
            club = named_club(&state.name),
            relation = relation_text(club, state, data)
        ));
        let lines = self.relation_lines(club);
        if !lines.is_empty() {
            egui::CollapsingHeader::new(tr!("rival-history"))
                .id_salt(("relation", club))
                .show(ui, |ui| {
                    for line in lines {
                        ui.label(line);
                    }
                });
        }
        let gone = rival.fighters.len() - rival.active();
        if gone > 0 {
            ui.label(tr!("rival-gone", gone = gone));
        }
    }

    /// A rival club's fighters in activity; returns the page clicked and the offer to
    /// negotiate.
    fn rival_fighters(
        &self,
        ui: &mut egui::Ui,
        club: usize,
    ) -> (Option<View>, Option<(RivalId, u32)>) {
        let data = &self.data;
        let state = &self.state;
        let rival = &state.rivals[club];
        let mut goto = None;
        let mut draft = None;
        egui::ScrollArea::vertical().show(ui, |ui| {
            egui::Grid::new(("rival club", club))
                .striped(true)
                .show(ui, |ui| {
                    for title in [
                        tr!("roster-fighter"),
                        tr!("roster-age"),
                        tr!("roster-fame"),
                        tr!("record-title"),
                        tr!("roster-condition"),
                    ] {
                        ui.strong(title);
                    }
                    let home: Vec<usize> = (0..data.world.colosseums.len())
                        .filter(|&c| data.world.colosseums[c].continent == rival.continent)
                        .collect();
                    for &c in &home {
                        ui.strong(colosseum_rules_name(&data.world.colosseums[c]));
                    }
                    ui.strong(tr!("estimated-value"));
                    ui.end_row();
                    for (fighter, f) in rival.fighters.iter().enumerate() {
                        if f.status != Status::Active {
                            continue;
                        }
                        let id = RivalId { club, fighter };
                        if ui.link(rival_fighter_name(id, state, data)).clicked() {
                            goto = Some(View::Outside(Target::Rival(id)));
                        }
                        ui.label(f.age.to_string());
                        ui.label(f.fame.value().to_string());
                        ui.label(record_text(f.record));
                        ui.label(condition_text(f));
                        let standings = self.standings(Contender::Rival(id));
                        for &c in &home {
                            ui.label(&standings[c]);
                        }
                        let estimate = estimate(f, data);
                        ui.label(tr!("estimate", value = estimate));
                        if ui.button(tr!("negotiate")).clicked() {
                            draft = Some((id, estimate));
                        }
                        ui.end_row();
                    }
                });
        });
        (goto, draft)
    }
}

/// Which half of a continent the colosseums view shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum ContinentPart {
    Colosseums,
    Clubs,
}

/// A rival club as the table of clubs shows it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct RivalRow {
    /// Its index among the rival clubs.
    pub(super) club: usize,
    pub(super) name: String,
    pub(super) culture: String,
    pub(super) profile: String,
    pub(super) wealth: String,
    /// Its fighters in activity.
    pub(super) fighters: usize,
    /// "+12, ami": its relation with the player's club.
    pub(super) relation: String,
    pub(super) standing: Standing,
    /// The colosseums one of its fighters is champion of.
    pub(super) titles: Vec<String>,
}

/// The colosseums' levels, by prestige: major, middling, weak.
const PRESTIGES: [u32; 3] = [3, 2, 1];

fn prestige_name(prestige: u32) -> String {
    match prestige {
        3 => tr!("prestige-major"),
        2 => tr!("prestige-t1"),
        _ => tr!("prestige-t2"),
    }
}

/// Green for a friend, red for a rival, bronze for a tense relation.
fn standing_color(ui: &egui::Ui, standing: Standing) -> egui::Color32 {
    match standing {
        Standing::Ally | Standing::Friend => theme::LAUREL,
        Standing::Neutral => ui.visuals().widgets.inactive.bg_fill,
        Standing::Tense => theme::BRONZE.gamma_multiply(0.6),
        Standing::Rival => theme::BLOOD,
    }
}

/// A fighter's standing in a colosseum where it is not among the ranks shown.
pub(super) fn not_ranked() -> String {
    tr!("not-ranked")
}

/// One colosseum's column: a card with its name, level, arena and purses, its
/// champion and when the title is to be defended, then its honours and its ranking;
/// returns the page clicked.
fn colosseum_column(
    ui: &mut egui::Ui,
    id: ColosseumId,
    colosseum: &Colosseum,
    rules: &ColosseumData,
    state: &GameState,
    data: &DesignData,
) -> Option<View> {
    let mut goto = None;
    egui::Frame::group(ui.style())
        .fill(ui.visuals().extreme_bg_color)
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal(|ui| {
                let name = egui::RichText::new(colosseum_rules_name(rules));
                ui.label(name.strong().color(theme::BRONZE));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let plain = ui.visuals().widgets.inactive.bg_fill;
                    chip(
                        ui,
                        tr!("colosseum-prestige", prestige = rules.prestige),
                        plain,
                    );
                });
            });
            purses(ui, id, rules, state, data);
            ui.horizontal_wrapped(|ui| {
                ui.colored_label(egui::Color32::GOLD, tr!("colosseum-champion"));
                match colosseum.champion {
                    Some(c) => {
                        let (name, _) = ranked_name(c, state, data);
                        let link = ui
                            .link(egui::RichText::new(name).strong())
                            .on_hover_text(contender_name(c, state, data));
                        if link.clicked() {
                            goto = Some(contender_view(c));
                        }
                        let deadline = colosseum.defense_deadline(&data.world);
                        let defend = tr!("colosseum-defend-by", deadline = deadline);
                        ui.label(egui::RichText::new(defend).weak())
                            .on_hover_text(tr!(
                                "colosseum-title-since",
                                since = colosseum.title_since
                            ));
                    }
                    None => {
                        ui.label(tr!("colosseum-no-champion"));
                    }
                }
            });
            egui::CollapsingHeader::new(tr!("colosseum-honours", reigns = colosseum.reigns.len()))
                .id_salt(("honours", id.0))
                .show(ui, |ui| {
                    for reign in colosseum.reigns.iter().rev() {
                        ui.horizontal_wrapped(|ui| {
                            if ui.link(contender_name(reign.holder, state, data)).clicked() {
                                goto = Some(contender_view(reign.holder));
                            }
                            ui.label(reign_text(reign, state, data));
                        });
                    }
                });
        });
    ui.add_space(4.0);
    ranking(ui, id, colosseum, state, data).or(goto)
}

/// A colosseum's arena and purses as tags, the arena's effect and the travel in a
/// tooltip.
fn purses(
    ui: &mut egui::Ui,
    id: ColosseumId,
    rules: &ColosseumData,
    state: &GameState,
    data: &DesignData,
) {
    let plain = ui.visuals().widgets.inactive.bg_fill;
    ui.horizontal_wrapped(|ui| {
        let arena = tr!("colosseum-arena-chip", arena = arena_name(rules.arena));
        chip(ui, arena, plain).on_hover_text(arena_effect(rules.arena));
        let fee = state.entry_fee(id, data).unwrap_or(0);
        let fee_chip = chip(ui, tr!("colosseum-fee-chip", fee = fee), plain);
        if fee > rules.purses.entry_fee {
            fee_chip.on_hover_text(tr!("colosseum-fee-travel"));
        }
        let prize = rules.purses.win_prize;
        chip(ui, tr!("colosseum-prize-chip", prize = prize), plain);
        let bonus = rules.purses.champion_bonus;
        chip(ui, tr!("colosseum-bonus-chip", bonus = bonus), plain);
    });
}

/// "Nakula" and "Augure": how a ranking names a contender, its club in a column of
/// its own.
pub(super) fn ranked_name(
    contender: Contender,
    state: &GameState,
    data: &DesignData,
) -> (String, String) {
    let number = match contender {
        Contender::Club(i) | Contender::FreeAgent(i) => i,
        Contender::Rival(id) => id.fighter,
    } + 1;
    state.rosters().fighter(contender).map_or_else(
        || ("?".into(), String::new()),
        |f| {
            let name = if f.name.is_empty() {
                tr!("ranked-unnamed", number = number)
            } else {
                stage_name(&f.name)
            };
            (name, class_name(f, data))
        },
    )
}

/// "pour Ludus Magnus, de saison 1, semaine 3 à saison 2, semaine 7, 2 défenses, battu".
pub(super) fn reign_text(reign: &Reign, state: &GameState, data: &DesignData) -> String {
    let club = match reign.won_as {
        Contender::Club(_) => tr!("reign-for", club = named_club(&state.name)),
        Contender::Rival(id) => tr!("reign-for", club = club_name(id.club, state)),
        Contender::FreeAgent(_) => tr!("reign-no-club"),
    };
    let status = state.rosters().fighter(reign.holder).map(|f| f.status);
    let (until, end) = match reign.end {
        None => (tr!("reign-now"), "current"),
        Some(ReignEnd { week, how }) => (
            date_in_sentence(week, data),
            match how {
                TitleLoss::Beaten => "beaten",
                TitleLoss::Stripped => "stripped",
                TitleLoss::Vacated if status == Some(Status::Dead) => "died",
                TitleLoss::Vacated if status == Some(Status::Retired) => "retired",
                TitleLoss::Vacated => "vacated",
            },
        ),
    };
    tr!(
        "reign",
        club = club,
        from = date_in_sentence(reign.from, data),
        until = until,
        defenses = reign.defenses,
        end = end
    )
}

/// A colosseum's first `RANKS_SHOWN` fighters, each with its class in small print,
/// the club's in bronze; returns the page clicked.
fn ranking(
    ui: &mut egui::Ui,
    id: ColosseumId,
    colosseum: &Colosseum,
    state: &GameState,
    data: &DesignData,
) -> Option<View> {
    let mut goto = None;
    let ranked = colosseum.active_ranking(state.rosters());
    egui::ScrollArea::vertical()
        .id_salt(("ranking", id.0))
        .show(ui, |ui| {
            egui::Grid::new(("ranking", id.0))
                .num_columns(4)
                .striped(true)
                .show(ui, |ui| {
                    for title in [
                        tr!("ranking-rank"),
                        tr!("roster-fighter"),
                        tr!("ranking-club"),
                        tr!("ranking-elo"),
                    ] {
                        ui.strong(title);
                    }
                    ui.end_row();
                    for (rank, rating) in ranked.take(RANKS_SHOWN).enumerate() {
                        let c = rating.contender;
                        ui.label((rank + 1).to_string());
                        ui.horizontal(|ui| {
                            let (name, class) = ranked_name(c, state, data);
                            let mut name = egui::RichText::new(name);
                            if matches!(c, Contender::Club(_)) {
                                name = name.strong().color(theme::BRONZE);
                            }
                            if ui.link(name).clicked() {
                                goto = Some(contender_view(c));
                            }
                            ui.label(egui::RichText::new(class).small().weak());
                        });
                        match c {
                            Contender::Rival(id) => {
                                if ui.link(club_name(id.club, state)).clicked() {
                                    goto = Some(View::Rival(id.club));
                                }
                            }
                            Contender::FreeAgent(_) => {
                                ui.label(egui::RichText::new(tr!("ranking-no-club")).weak());
                            }
                            Contender::Club(_) => {
                                let club = egui::RichText::new(named_club(&state.name));
                                ui.label(club.color(theme::BRONZE));
                            }
                        }
                        ui.label(rating.elo.to_string());
                        ui.end_row();
                    }
                });
        });
    goto
}

pub(super) fn contender_view(contender: Contender) -> View {
    match contender {
        Contender::Club(i) => View::Sheet(i),
        Contender::Rival(id) => View::Outside(Target::Rival(id)),
        Contender::FreeAgent(a) => View::Outside(Target::FreeAgent(a)),
    }
}
