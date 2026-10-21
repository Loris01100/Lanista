//! The colosseums view and the page of a rival club.

use lanista_core::colosseum::{Colosseum, ColosseumData, ColosseumId, Contender, Continent};
use lanista_core::design_data::DesignData;
use lanista_core::fighter::career::Status;
use lanista_core::market::Target;
use lanista_core::rivals::RivalId;
use lanista_core::state::GameState;
use reesch::egui;

use super::market::estimate;
use super::{ClubScreen, RANKS_SHOWN, View};
use crate::texts::{
    arena_effect, arena_name, club_name, colosseum_name, condition_text, contender_name,
    continent_name, culture_name, culture_text, date_text, profile_name, record_text,
    relation_change_text, relation_text, rival_fighter_name, staff_text, wealth_name,
};

impl ClubScreen {
    /// One continent at a time, the club's first: its rival clubs, then its colosseums
    /// side by side. Every name opens its page.
    pub(super) fn colosseums(&mut self, ui: &mut egui::Ui) {
        let mut goto = self.continent_clubs(ui);
        let shown = self.shown_continent;
        ui.heading(format!("Colisées ({})", continent_name(shown)));
        ui.horizontal(|ui| {
            ui.selectable_value(&mut self.shown_prestige, None, "Tous");
            for (prestige, name) in PRESTIGES {
                ui.selectable_value(&mut self.shown_prestige, Some(prestige), name);
            }
        });
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
            ui.label("Aucun colisée de ce niveau sur ce continent pour l'instant");
        }
        ui.columns(colosseums.len().max(1), |columns| {
            for ((id, (colosseum, rules)), ui) in colosseums.into_iter().zip(columns) {
                goto = colosseum_column(ui, id, colosseum, rules, state, data).or(goto);
            }
        });
        if let Some(view) = goto {
            self.go_to(view);
        }
    }

    /// The continents to choose from, then the rival clubs of the one shown; returns the
    /// page of the club clicked.
    fn continent_clubs(&mut self, ui: &mut egui::Ui) -> Option<View> {
        ui.horizontal(|ui| {
            for continent in Continent::ALL {
                let mut name = continent_name(continent).to_owned();
                if continent == self.state.continent {
                    name = format!("{name} (chez vous)");
                }
                ui.selectable_value(&mut self.shown_continent, continent, name);
            }
        });
        let shown = self.shown_continent;
        ui.heading(format!("Clubs rivaux ({})", continent_name(shown)));
        let mut goto = None;
        for (club, line) in self.rival_lines().into_iter().enumerate() {
            let here = self
                .state
                .rivals
                .get(club)
                .is_some_and(|r| r.is_open() && r.continent == shown);
            if here && ui.link(line).clicked() {
                goto = Some(View::Rival(club));
            }
        }
        goto
    }

    /// "Ludus Magnus (Rome), prudent et riche : 10 combattants ; relation +12, ami" for
    /// each rival club, in club order; its money stays hidden.
    pub(super) fn rival_lines(&self) -> Vec<String> {
        (0..)
            .zip(&self.state.rivals)
            .map(|(club, rival)| {
                format!(
                    "{} ({}), {} et {} : {} combattants ; relation {}",
                    rival.name,
                    culture_name(&rival.culture, &self.data),
                    profile_name(rival.profile),
                    wealth_name(rival.wealth),
                    rival.active(),
                    relation_text(club, &self.state, &self.data)
                )
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
                Some(Contender::Rival(id)) if id.club == club => Some(format!(
                    "Champion de {} : {}",
                    colosseum_name(ColosseumId(c), &self.data),
                    rival_fighter_name(id, &self.state, &self.data)
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
                let mut text = rank.map_or_else(
                    || NOT_RANKED.to_owned(),
                    |rank| {
                        let elo = colosseum.elo(contender, &self.data.world);
                        format!("{}, Elo {elo}", ordinal(rank + 1))
                    },
                );
                if colosseum.champion == Some(contender) {
                    text = format!("champion, {text}");
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
        let data = &self.data;
        let state = &self.state;
        let Some(rival) = state.rivals.get(club) else {
            return;
        };
        ui.heading(&rival.name);
        if let Some(week) = rival.closed {
            let closed = format!("Fermé, ruiné : {}", date_text(week, data));
            ui.colored_label(egui::Color32::LIGHT_RED, closed);
        }
        ui.label(format!(
            "Club {} et {}, {}, en {}, {} combattants en activité (au plus {})",
            profile_name(rival.profile),
            wealth_name(rival.wealth),
            culture_text(&rival.culture, data),
            continent_name(rival.continent),
            rival.active(),
            data.world.roster.max
        ));
        ui.label(staff_text(&rival.staff));
        for line in self.titles_of(club) {
            ui.colored_label(egui::Color32::GOLD, line);
        }
        ui.strong(format!(
            "Relation avec votre club : {}",
            relation_text(club, state, data)
        ));
        let lines = self.relation_lines(club);
        if !lines.is_empty() {
            egui::CollapsingHeader::new("Ce qui s'est passé entre vous")
                .id_salt(("relation", club))
                .show(ui, |ui| {
                    for line in lines {
                        ui.label(line);
                    }
                });
        }
        let gone = rival.fighters.len() - rival.active();
        if gone > 0 {
            ui.label(format!(
                "{gone} anciens combattants : retraités, morts, vendus ou libérés"
            ));
        }
        let mut goto = None;
        let mut draft = None;
        egui::ScrollArea::vertical().show(ui, |ui| {
            egui::Grid::new(("rival club", club))
                .striped(true)
                .show(ui, |ui| {
                    for title in ["Combattant", "Âge", "Renommée", "Bilan", "État"] {
                        ui.strong(title);
                    }
                    let home: Vec<usize> = (0..data.world.colosseums.len())
                        .filter(|&c| data.world.colosseums[c].continent == rival.continent)
                        .collect();
                    for &c in &home {
                        ui.strong(&data.world.colosseums[c].name);
                    }
                    ui.strong("Valeur estimée");
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
                        ui.label(format!("≈ {estimate}"));
                        if ui.button("Négocier").clicked() {
                            draft = Some((id, estimate));
                        }
                        ui.end_row();
                    }
                });
        });
        if let Some(draft) = draft {
            self.offer_draft = Some(draft);
            self.view = View::Market;
        } else if let Some(view) = goto {
            self.go_to(view);
        }
    }
}

/// The colosseums' levels, by prestige: major, middling, weak.
const PRESTIGES: [(u32, &str); 3] = [(3, "Majeurs"), (2, "T1"), (1, "T2")];

/// A fighter's standing in a colosseum where it is not among the ranks shown.
pub(super) const NOT_RANKED: &str = "non classé";

/// The page of a ranked fighter: the club's sheet, or what the club sees of an
/// outsider.
/// One colosseum's column: its arena, purses, champion and ranking; returns the page
/// clicked.
fn colosseum_column(
    ui: &mut egui::Ui,
    id: ColosseumId,
    colosseum: &Colosseum,
    rules: &ColosseumData,
    state: &GameState,
    data: &DesignData,
) -> Option<View> {
    let mut goto = None;
    ui.strong(&rules.name);
    ui.label(format!(
        "Arène : {}, prestige {} / 5",
        arena_name(rules.arena),
        rules.prestige
    ))
    .on_hover_text(arena_effect(rules.arena));
    let fee = state.entry_fee(id, data).unwrap_or(0);
    let travel = if fee > rules.purses.entry_fee {
        " (voyage compris)"
    } else {
        ""
    };
    ui.label(format!(
        "Inscription {fee}{travel}, prime de victoire {}, bonus champion {}",
        rules.purses.win_prize, rules.purses.champion_bonus
    ));
    ui.horizontal(|ui| {
        ui.colored_label(egui::Color32::GOLD, "Champion :");
        match colosseum.champion {
            Some(c) => {
                if ui.link(contender_name(c, state, data)).clicked() {
                    goto = Some(contender_view(c));
                }
            }
            None => {
                ui.label("aucun");
            }
        }
    });
    ui.label(format!(
        "Titre défendu ou pris en semaine {}, à défendre au plus tard en semaine {}",
        colosseum.title_since,
        colosseum.defense_deadline(&data.world)
    ));
    ranking(ui, id, colosseum, state, data).or(goto)
}

/// A colosseum's first `RANKS_SHOWN` fighters; returns the page clicked.
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
                .striped(true)
                .show(ui, |ui| {
                    for title in ["Rang", "Combattant", "Club", "Elo"] {
                        ui.strong(title);
                    }
                    ui.end_row();
                    for (rank, rating) in ranked.take(RANKS_SHOWN).enumerate() {
                        let c = rating.contender;
                        ui.label((rank + 1).to_string());
                        if ui.link(contender_name(c, state, data)).clicked() {
                            goto = Some(contender_view(c));
                        }
                        match c {
                            Contender::Rival(id) => {
                                if ui.link(club_name(id.club, state)).clicked() {
                                    goto = Some(View::Rival(id.club));
                                }
                            }
                            Contender::FreeAgent(_) => {
                                ui.label("sans club");
                            }
                            Contender::Club(_) => {
                                ui.label("");
                            }
                        }
                        ui.label(rating.elo.to_string());
                        ui.end_row();
                    }
                });
        });
    goto
}

fn contender_view(contender: Contender) -> View {
    match contender {
        Contender::Club(i) => View::Sheet(i),
        Contender::Rival(id) => View::Outside(Target::Rival(id)),
        Contender::FreeAgent(a) => View::Outside(Target::FreeAgent(a)),
    }
}

/// "1er", "2e", "3e"…
fn ordinal(n: usize) -> String {
    if n == 1 {
        "1er".into()
    } else {
        format!("{n}e")
    }
}
