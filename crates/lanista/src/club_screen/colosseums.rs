//! The colosseums view and the page of a rival club.

use lanista_core::colosseum::{ColosseumId, Contender, Continent};
use lanista_core::fighter::career::Status;
use lanista_core::market::Target;
use lanista_core::rivals::RivalId;
use reesch::egui;

use super::{ClubScreen, RANKS_SHOWN, View};
use crate::texts::{
    arena_effect, arena_name, club_name, colosseum_name, condition_text, contender_name,
    continent_name, culture_name, culture_text, profile_name, record_text, rival_fighter_name,
    staff_text, wealth_name,
};

impl ClubScreen {
    /// One continent at a time, the club's first: its rival clubs, then its colosseums
    /// side by side. Every name opens its page.
    pub(super) fn colosseums(&mut self, ui: &mut egui::Ui) {
        let mut goto = self.continent_clubs(ui);
        let shown = self.shown_continent;
        ui.heading(format!("Colisées ({})", continent_name(shown)));
        let data = &self.data;
        let state = &self.state;
        let colosseums: Vec<_> = (0..)
            .map(ColosseumId)
            .zip(state.colosseums.iter().zip(&data.world.colosseums))
            .filter(|(_, (_, rules))| rules.continent == shown)
            .collect();
        ui.columns(colosseums.len().max(1), |columns| {
            for ((id, (colosseum, rules)), ui) in colosseums.into_iter().zip(columns) {
                let n = id.0;
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
                let ranked = colosseum.active_ranking(state.rosters());
                egui::ScrollArea::vertical()
                    .id_salt(("ranking", n))
                    .show(ui, |ui| {
                        egui::Grid::new(("ranking", n))
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
            let here = self.state.rivals.get(club).map(|r| r.continent) == Some(shown);
            if here && ui.link(line).clicked() {
                goto = Some(View::Rival(club));
            }
        }
        goto
    }

    /// "Ludus Magnus (Rome), prudent et riche : 10 combattants" for each rival club, in
    /// club order; its money stays hidden.
    pub(super) fn rival_lines(&self) -> Vec<String> {
        self.state
            .rivals
            .iter()
            .map(|rival| {
                format!(
                    "{} ({}), {} et {} : {} combattants",
                    rival.name,
                    culture_name(&rival.culture, &self.data),
                    profile_name(rival.profile),
                    wealth_name(rival.wealth),
                    rival.active()
                )
            })
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

    /// A rival club: profile, wealth (its money stays hidden), culture, continent, staff,
    /// the titles it holds, then its fighters in activity with their standing in each
    /// colosseum of its continent.
    pub(super) fn rival_club(&mut self, ui: &mut egui::Ui, club: usize) {
        self.back_button(ui);
        let data = &self.data;
        let state = &self.state;
        let Some(rival) = state.rivals.get(club) else {
            return;
        };
        ui.heading(&rival.name);
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
        let gone = rival.fighters.len() - rival.active();
        if gone > 0 {
            ui.label(format!(
                "{gone} anciens combattants : retraités, morts, vendus ou libérés"
            ));
        }
        let mut goto = None;
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
                        ui.end_row();
                    }
                });
        });
        if let Some(view) = goto {
            self.go_to(view);
        }
    }
}

/// A fighter's standing in a colosseum where it is not among the ranks shown.
pub(super) const NOT_RANKED: &str = "non classé";

/// The page of a ranked fighter: the club's sheet, or what the club sees of an
/// outsider.
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
