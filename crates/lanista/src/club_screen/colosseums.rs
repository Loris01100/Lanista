//! The colosseums view and the page of a rival club.

use lanista_core::colosseum::{ColosseumId, Contender};
use lanista_core::fighter::career::Status;
use lanista_core::market::Target;
use lanista_core::rivals::RivalId;
use reesch::egui;

use super::{ClubScreen, RANKS_SHOWN, View};
use crate::texts::{
    arena_name, club_name, colosseum_name, condition_text, contender_name, profile_name,
    record_text, rival_fighter_name,
};

impl ClubScreen {
    /// The rival clubs, then each colosseum side by side. Every name opens its page.
    pub(super) fn colosseums(&mut self, ui: &mut egui::Ui) {
        let mut goto = None;
        ui.heading("Clubs rivaux");
        for (club, line) in self.rival_lines().into_iter().enumerate() {
            if ui.link(line).clicked() {
                goto = Some(View::Rival(club));
            }
        }
        ui.heading("Colisées");
        let data = &self.data;
        let state = &self.state;
        let colosseums = state.colosseums.iter().zip(&data.world.colosseums);
        ui.columns(data.world.colosseums.len().max(1), |columns| {
            for (n, ((colosseum, rules), ui)) in colosseums.zip(columns).enumerate() {
                ui.strong(&rules.name);
                ui.label(format!(
                    "Arène : {}, prestige {} / 5",
                    arena_name(rules.arena),
                    rules.prestige
                ));
                ui.label(format!(
                    "Inscription {}, prime de victoire {}, bonus champion {}",
                    rules.purses.entry_fee, rules.purses.win_prize, rules.purses.champion_bonus
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
                                            if ui.link(club_name(id.club, data)).clicked() {
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

    /// "Ludus Magnus, prudent : 10 combattants, argent 500" for each rival club.
    pub(super) fn rival_lines(&self) -> Vec<String> {
        self.data
            .world
            .rivals
            .iter()
            .zip(&self.state.rivals)
            .map(|(rules, rival)| {
                format!(
                    "{}, {} : {} combattants, argent {}",
                    rules.name,
                    profile_name(rules.profile),
                    rival.active(),
                    rival.money
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
                    || "non classé".to_owned(),
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

    /// A rival club: profile, money, the titles it holds, then its fighters in activity
    /// with their standing in each colosseum.
    pub(super) fn rival_club(&mut self, ui: &mut egui::Ui, club: usize) {
        self.back_button(ui);
        let data = &self.data;
        let state = &self.state;
        let (Some(rules), Some(rival)) = (data.world.rivals.get(club), state.rivals.get(club))
        else {
            return;
        };
        ui.heading(&rules.name);
        ui.label(format!(
            "Club {}, argent {}, {} combattants en activité (au plus {})",
            profile_name(rules.profile),
            rival.money,
            rival.active(),
            data.world.roster.max
        ));
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
                    for rules in &data.world.colosseums {
                        ui.strong(&rules.name);
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
                        for standing in self.standings(Contender::Rival(id)) {
                            ui.label(standing);
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
