//! Colosseums: each has its arena type, prestige and purses (from `data/world.ron`),
//! and in the game its residents, its Elo ranking and its champion.

use std::cmp::{Ordering, Reverse};

use rand::{Rng, RngExt};
use serde::{Deserialize, Serialize};

use crate::design_data::DesignData;
use crate::economy::Purses;
use crate::fighter::career::{CareerFighter, Status};
use crate::fighter::fame::FightResult;
use crate::fighter::{FighterId, percent_of};

/// The world's rules, from `data/world.ron`.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WorldRules {
    /// Elo of a fighter's first fight in a colosseum.
    pub elo_start: i32,
    /// Most Elo points one fight moves.
    pub elo_k: u32,
    /// Residents' stats grow by this percentage for each prestige level above 1.
    pub prestige_stats_percent: u32,
    /// A champion must defend the title at least once in this many weeks.
    pub title_defense_weeks: u32,
    /// How many of the first in a ranking may fight for the title.
    pub challengers: u32,
    /// `ColosseumId(i)` is the colosseum at index `i`.
    pub colosseums: Vec<ColosseumData>,
}

impl WorldRules {
    #[must_use]
    pub fn colosseum(&self, id: ColosseumId) -> Option<&ColosseumData> {
        self.colosseums.get(usize::try_from(id.0).ok()?)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ColosseumData {
    /// Shown to the player.
    pub name: String,
    pub arena: ArenaKind,
    /// From 1 to 5: the higher, the stronger the residents.
    pub prestige: u32,
    /// Number of regular fighters, the opponents of its fights.
    pub residents: u32,
    pub purses: Purses,
}

/// What the arena does to the fight.
// ponytail: no effect on the fight until phase 6.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
pub enum ArenaKind {
    /// Slows everyone.
    Sand,
    /// Pillars block ranged spells.
    Pillars,
    /// Favours melee.
    Narrow,
}

/// Index of a colosseum in `WorldRules::colosseums` and `GameState::colosseums`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct ColosseumId(pub u32);

/// A fighter with an Elo in a colosseum.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Contender {
    /// The club's fighter at this index.
    Club(usize),
    /// The colosseum's resident at this index.
    Resident(usize),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Rating {
    pub contender: Contender,
    pub elo: i32,
}

/// A colosseum in the game.
// ponytail: residents only take fame and death from their fights, and neither age
// nor tire; they become rival clubs' fighters later in phase 5.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Colosseum {
    /// Its regular fighters, the opponents of the club's fights; a dead one is
    /// replaced at the end of the week.
    pub residents: Vec<CareerFighter>,
    /// Everyone who fought here, best Elo first; ties keep their order. The club's
    /// fighters join with their first fight here.
    pub ranking: Vec<Rating>,
    /// Holds the title, which is not the first rank: it changes hands only in a
    /// title fight the champion loses.
    pub champion: Option<Contender>,
    /// Week the title was last won or defended.
    pub title_since: u64,
}

/// What happened to a title in a week.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TitleNews {
    /// A title fight; `result` is the champion's, who keeps the title unless beaten.
    Fight {
        champion: Contender,
        challenger: Contender,
        result: FightResult,
    },
    /// The champion did not defend in time: the title went to `to`.
    Stripped {
        former: Contender,
        to: Option<Contender>,
    },
    /// The champion died, retired or left: the title went to `to`.
    Vacated { to: Option<Contender> },
}

/// Whether the champion keeps the title after a title fight with this result.
#[must_use]
pub fn title_kept(result: FightResult) -> bool {
    matches!(result, FightResult::Won | FightResult::Unfinished)
}

impl Colosseum {
    /// A colosseum's opening in `week`: residents as strong as its prestige, all at the
    /// starting Elo, the strongest one champion.
    pub fn open<R: Rng + ?Sized>(
        rng: &mut R,
        rules: &ColosseumData,
        data: &DesignData,
        week: u64,
    ) -> Self {
        let residents: Vec<_> = (0..rules.residents)
            .map(|_| resident(rng, rules, data))
            .collect();
        let champion = (0..residents.len())
            .max_by_key(|&i| (total(&residents[i]), Reverse(i)))
            .map(Contender::Resident);
        let ranking = (0..residents.len())
            .map(|i| Rating {
                contender: Contender::Resident(i),
                elo: data.world.elo_start,
            })
            .collect();
        Self {
            residents,
            ranking,
            champion,
            title_since: week,
        }
    }

    /// The ranking without the club's fighters who no longer fight.
    pub fn active_ranking<'a>(
        &'a self,
        club: &'a [CareerFighter],
    ) -> impl Iterator<Item = &'a Rating> + 'a {
        self.ranking.iter().filter(|r| match r.contender {
            Contender::Club(i) => club.get(i).is_some_and(|f| f.status == Status::Active),
            Contender::Resident(_) => true,
        })
    }

    /// Last week the champion may defend the title in before losing it.
    #[must_use]
    pub fn defense_deadline(&self, rules: &WorldRules) -> u64 {
        self.title_since + u64::from(rules.title_defense_weeks)
    }

    /// The resident the club's fighter `i` meets if it fights for the title here: as
    /// champion, the best-ranked resident; from the first `challengers` of the ranking,
    /// a resident champion. `None` if it may not fight for the title.
    // ponytail: a resident champion only meets residents in its own defenses; club
    // fighters ask for their title fight, until challenges.
    #[must_use]
    pub fn title_opponent(
        &self,
        i: usize,
        club: &[CareerFighter],
        rules: &WorldRules,
    ) -> Option<usize> {
        let me = Contender::Club(i);
        if self.champion == Some(me) {
            return self.best_resident(None);
        }
        let Some(Contender::Resident(champion)) = self.champion else {
            return None;
        };
        let top = usize::try_from(rules.challengers).unwrap_or(usize::MAX);
        self.active_ranking(club)
            .take(top)
            .any(|r| r.contender == me)
            .then_some(champion)
    }

    /// The best-ranked resident other than `except`.
    fn best_resident(&self, except: Option<usize>) -> Option<usize> {
        self.ranking.iter().find_map(|r| match r.contender {
            Contender::Resident(i) if Some(i) != except => Some(i),
            _ => None,
        })
    }

    /// Records a title fight in `week`, `result` being the champion's: both Elo move,
    /// the challenger takes the title by winning, and the title counts as defended.
    pub fn record_title_fight(
        &mut self,
        champion: Contender,
        challenger: Contender,
        result: FightResult,
        week: u64,
        rules: &WorldRules,
    ) {
        self.record(champion, challenger, result, rules);
        if !title_kept(result) {
            self.champion = Some(challenger);
        }
        self.title_since = week;
    }

    /// Elo of `contender`, the starting one for a first fight here.
    #[must_use]
    pub fn elo(&self, contender: Contender, rules: &WorldRules) -> i32 {
        self.ranking
            .iter()
            .find(|r| r.contender == contender)
            .map_or(rules.elo_start, |r| r.elo)
    }

    /// Records a duel between `a` and `b`, `result` being `a`'s: both Elo move.
    pub fn record(&mut self, a: Contender, b: Contender, result: FightResult, rules: &WorldRules) {
        let places = match result {
            FightResult::Won => [(a, 1), (b, 2)],
            FightResult::Lost | FightResult::GaveUp => [(a, 2), (b, 1)],
            FightResult::Unfinished => [(a, 1), (b, 1)],
        };
        self.record_places(&places, rules);
    }

    /// Records a fight of any size from each contender's place (1 the best, from
    /// `log::places`; a shared place is a draw). Everyone is compared with everyone
    /// else, a duel being the case of two, and each comparison weighs K / (n - 1), so
    /// a melee moves one's Elo no more than a duel. Titles only change hands in duels.
    pub fn record_places(&mut self, places: &[(Contender, u32)], rules: &WorldRules) {
        let Some(others) = places.len().checked_sub(1).filter(|&n| n > 0) else {
            return;
        };
        let k = f64::from(rules.elo_k) / f64::from(u32::try_from(others).unwrap_or(u32::MAX));
        let elos: Vec<i32> = places.iter().map(|&(c, _)| self.elo(c, rules)).collect();
        let changes: Vec<i32> = places
            .iter()
            .zip(&elos)
            .enumerate()
            .map(|(i, (&(_, place), &elo))| {
                let points: f64 = places
                    .iter()
                    .zip(&elos)
                    .enumerate()
                    .filter(|&(j, _)| j != i)
                    .map(|(_, (&(_, other), &theirs))| {
                        let score = match place.cmp(&other) {
                            Ordering::Less => 1.0,
                            Ordering::Equal => 0.5,
                            Ordering::Greater => 0.0,
                        };
                        score - expected(elo, theirs)
                    })
                    .sum();
                rounded(k * points)
            })
            .collect();
        for (&(contender, _), change) in places.iter().zip(changes) {
            self.add_elo(contender, change, rules);
        }
        self.ranking.sort_by_key(|r| Reverse(r.elo));
    }

    fn add_elo(&mut self, contender: Contender, change: i32, rules: &WorldRules) {
        match self.ranking.iter_mut().find(|r| r.contender == contender) {
            Some(rating) => rating.elo = rating.elo.saturating_add(change),
            None => self.ranking.push(Rating {
                contender,
                elo: rules.elo_start.saturating_add(change),
            }),
        }
    }

    /// End of `week`: dead residents give way to new ones at the starting Elo, a title
    /// whose holder is gone goes to the best ranked still fighting, then a resident
    /// champion at its deadline defends the title against the best-ranked other
    /// resident and a club champion past it loses the title. Returns what happened to
    /// the title.
    pub fn end_week<R: Rng + ?Sized>(
        &mut self,
        week: u64,
        rng: &mut R,
        rules: &ColosseumData,
        data: &DesignData,
        club: &[CareerFighter],
    ) -> Vec<TitleNews> {
        let mut news = Vec::new();
        self.replace_the_gone(week, rng, rules, data, club, &mut news);
        if week < self.defense_deadline(&data.world) {
            return news;
        }
        match self.champion {
            Some(Contender::Resident(c)) => {
                if let Some(r) = self.best_resident(Some(c)) {
                    let result = self.resident_duel(c, r, rng, data);
                    let (champion, challenger) = (Contender::Resident(c), Contender::Resident(r));
                    self.record_title_fight(champion, challenger, result, week, &data.world);
                    news.push(TitleNews::Fight {
                        champion,
                        challenger,
                        result,
                    });
                    // The duel may have killed one of them.
                    self.replace_the_gone(week, rng, rules, data, club, &mut news);
                }
            }
            Some(former @ Contender::Club(_)) => {
                let to = self
                    .active_ranking(club)
                    .map(|r| r.contender)
                    .find(|&c| c != former);
                self.champion = to;
                self.title_since = week;
                news.push(TitleNews::Stripped { former, to });
            }
            None => {}
        }
        news
    }

    /// Replaces dead residents, and gives a title whose holder is gone to the best
    /// ranked still fighting.
    fn replace_the_gone<R: Rng + ?Sized>(
        &mut self,
        week: u64,
        rng: &mut R,
        rules: &ColosseumData,
        data: &DesignData,
        club: &[CareerFighter],
        news: &mut Vec<TitleNews>,
    ) {
        for i in 0..self.residents.len() {
            if self.residents[i].status == Status::Active {
                continue;
            }
            self.residents[i] = resident(rng, rules, data);
            let contender = Contender::Resident(i);
            self.ranking.retain(|r| r.contender != contender);
            self.ranking.push(Rating {
                contender,
                elo: data.world.elo_start,
            });
            if self.champion == Some(contender) {
                self.champion = None;
            }
        }
        self.ranking.sort_by_key(|r| Reverse(r.elo));
        let holder_fights = self
            .champion
            .is_some_and(|c| self.active_ranking(club).any(|r| r.contender == c));
        if !holder_fights {
            let to = self.active_ranking(club).map(|r| r.contender).next();
            self.champion = to;
            self.title_since = week;
            news.push(TitleNews::Vacated { to });
        }
    }

    /// A duel between residents `a` and `b`, who take fame and death from it;
    /// returns `a`'s result.
    fn resident_duel<R: Rng + ?Sized>(
        &mut self,
        a: usize,
        b: usize,
        rng: &mut R,
        data: &DesignData,
    ) -> FightResult {
        let rules = &data.fighter;
        let (mut first, mut second) = (self.residents[a].clone(), self.residents[b].clone());
        let (_, _, arena) = first.duel(&second, rng, data);
        let (stronger, weaker) = (
            second.stronger_than(&first, rules),
            first.stronger_than(&second, rules),
        );
        let (outcome, _) = first.take_fame_and_death(&arena, FighterId(0), stronger, rules);
        second.take_fame_and_death(&arena, FighterId(1), weaker, rules);
        self.residents[a] = first;
        self.residents[b] = second;
        outcome.result
    }
}

/// A new resident, stronger with the colosseum's prestige.
fn resident<R: Rng + ?Sized>(
    rng: &mut R,
    rules: &ColosseumData,
    data: &DesignData,
) -> CareerFighter {
    let percent = 100 + data.world.prestige_stats_percent * rules.prestige.saturating_sub(1);
    let mut fighter = CareerFighter::random(rng, data);
    fighter.stats = fighter.stats.map(|v| percent_of(v, percent).max(1));
    fighter.potential = fighter.potential.map(|v| percent_of(v, percent).max(1));
    fighter
}

fn total(fighter: &CareerFighter) -> u32 {
    fighter.stats.to_array().iter().sum()
}

/// Expected score of a player of Elo `a` against one of Elo `b`, from 0 to 1.
fn expected(a: i32, b: i32) -> f64 {
    1.0 / (1.0 + libm::pow(10.0, f64::from(b - a) / 400.0))
}

/// Elo points, rounded.
fn rounded(points: f64) -> i32 {
    #[expect(
        clippy::cast_possible_truncation,
        reason = "within K, a u32 turned into f64"
    )]
    let points = libm::round(points) as i32;
    points
}

/// Picks the resident a club's fighter meets.
// ponytail: anyone at random; matchmaking by Elo comes with challenges.
pub fn pick_opponent<R: Rng + ?Sized>(rng: &mut R, colosseum: &Colosseum) -> Option<usize> {
    (!colosseum.residents.is_empty()).then(|| rng.random_range(0..colosseum.residents.len()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use rand::SeedableRng;
    use rand_chacha::ChaCha8Rng;

    fn data() -> DesignData {
        DesignData::load().unwrap()
    }

    fn rome(data: &DesignData) -> Colosseum {
        Colosseum::open(
            &mut ChaCha8Rng::seed_from_u64(1),
            &data.world.colosseums[1],
            data,
            1,
        )
    }

    /// A club of one active fighter.
    fn club(data: &DesignData) -> Vec<CareerFighter> {
        vec![CareerFighter::random(
            &mut ChaCha8Rng::seed_from_u64(3),
            data,
        )]
    }

    /// Elo won by a fighter of Elo `a` placed `place_a` in a duel against one of Elo
    /// `b` placed `place_b`; the other loses as many.
    fn duel(a: i32, b: i32, place_a: u32, place_b: u32) -> i32 {
        let data = data();
        let rules = &data.world;
        let mut colosseum = rome(&data);
        let (x, y) = (Contender::Club(0), Contender::Club(1));
        colosseum.ranking = vec![
            Rating {
                contender: x,
                elo: a,
            },
            Rating {
                contender: y,
                elo: b,
            },
        ];
        colosseum.record_places(&[(x, place_a), (y, place_b)], rules);
        let won = colosseum.elo(x, rules) - a;
        assert_eq!(won, b - colosseum.elo(y, rules));
        won
    }

    #[test]
    fn elo_moves_more_for_an_upset() {
        assert_eq!(duel(1000, 1000, 1, 2), 16);
        assert_eq!(duel(1000, 1000, 1, 1), 0);
        assert_eq!(duel(1000, 1000, 2, 1), -16);
        assert!(duel(1000, 1400, 1, 2) > 16);
        assert!(duel(1400, 1000, 1, 2) < 16);
    }

    #[test]
    fn a_melee_compares_everyone_by_elimination_order() {
        let data = data();
        let rules = &data.world;
        let opened = rome(&data);
        let mut melee = opened.clone();
        // Twenty at the starting Elo, out in order: 0 is the last standing.
        let contenders: Vec<_> = (0..20).map(Contender::Resident).collect();
        let places: Vec<_> = contenders.iter().copied().zip(1..).collect();
        melee.record_places(&places, rules);
        let change = |i: usize| melee.elo(contenders[i], rules) - rules.elo_start;
        // The winner beats 19 and gains as much as a duel won, the first out loses as
        // much as a duel lost, the middle hardly moves, and the later out, the better.
        assert_eq!((change(0), change(19)), (16, -16));
        assert!(change(9).abs() <= 2 && change(10).abs() <= 2);
        assert!((1..20).all(|i| change(i) < change(i - 1)));
        // A melee leaves the title alone.
        assert_eq!(melee.champion, opened.champion);
        // Two out together draw with each other.
        let mut tie = opened;
        let three = [(contenders[0], 1), (contenders[1], 2), (contenders[2], 2)];
        tie.record_places(&three, rules);
        let change = |i: usize| tie.elo(contenders[i], rules) - rules.elo_start;
        assert_eq!((change(0), change(1), change(2)), (16, -8, -8));
    }

    #[test]
    fn a_colosseum_opens_with_its_residents_and_a_champion() {
        let data = data();
        let capua = Colosseum::open(
            &mut ChaCha8Rng::seed_from_u64(1),
            &data.world.colosseums[0],
            &data,
            1,
        );
        let rome = rome(&data);
        assert_eq!(rome.residents.len(), 15);
        assert_eq!(rome.ranking.len(), 15);
        assert!(rome.ranking.iter().all(|r| r.elo == data.world.elo_start));
        let Some(Contender::Resident(champion)) = rome.champion else {
            panic!("a resident holds the title");
        };
        assert!(
            rome.residents
                .iter()
                .all(|f| total(f) <= total(&rome.residents[champion]))
        );
        // Same draws, but Rome's prestige makes its residents stronger.
        assert!(total(&rome.residents[0]) > total(&capua.residents[0]));
    }

    #[test]
    fn only_a_title_fight_moves_the_title() {
        let data = data();
        let rules = &data.world;
        let mut rome = rome(&data);
        let champion = rome.champion.unwrap();
        let club = Contender::Club(0);
        // Beating the champion in an ordinary fight wins Elo, not the title.
        rome.record(club, champion, FightResult::Won, rules);
        assert_eq!(rome.champion, Some(champion));
        assert_eq!(
            rome.ranking[0],
            Rating {
                contender: club,
                elo: 1016
            }
        );
        assert_eq!(
            rome.ranking.last().unwrap(),
            &Rating {
                contender: champion,
                elo: 984
            }
        );
        // A draw in a title fight keeps it, but counts as a defense.
        rome.record_title_fight(champion, club, FightResult::Unfinished, 3, rules);
        assert_eq!((rome.champion, rome.title_since), (Some(champion), 3));
        assert_eq!(
            rome.defense_deadline(rules),
            3 + u64::from(rules.title_defense_weeks)
        );
        // A champion giving up loses it.
        rome.record_title_fight(champion, club, FightResult::GaveUp, 4, rules);
        assert_eq!((rome.champion, rome.title_since), (Some(club), 4));
    }

    #[test]
    fn the_champion_meets_the_best_resident_and_the_top_meets_the_champion() {
        let data = data();
        let rules = &data.world;
        let mut club = club(&data);
        let mut rome = rome(&data);
        let Some(Contender::Resident(champion)) = rome.champion else {
            panic!("a resident holds the title");
        };
        // Not ranked yet: no title fight.
        assert_eq!(rome.title_opponent(0, &club, rules), None);
        rome.record(
            Contender::Club(0),
            Contender::Resident(9),
            FightResult::Won,
            rules,
        );
        assert_eq!(rome.title_opponent(0, &club, rules), Some(champion));
        // Out of the top five: no title fight either.
        for r in 0..6 {
            rome.record(
                Contender::Resident(r),
                Contender::Resident(14),
                FightResult::Won,
                rules,
            );
            rome.record(
                Contender::Resident(r),
                Contender::Resident(13),
                FightResult::Won,
                rules,
            );
        }
        assert_eq!(rome.title_opponent(0, &club, rules), None);
        // As champion, against the best-ranked resident.
        rome.champion = Some(Contender::Club(0));
        let Contender::Resident(best) = rome.ranking[0].contender else {
            panic!("a resident leads");
        };
        assert_eq!(rome.title_opponent(0, &club, rules), Some(best));
        assert_eq!(rome.title_opponent(1, &club, rules), None);
        // A retired fighter fights no more.
        club[0].status = Status::Retired;
        rome.champion = Some(Contender::Resident(champion));
        rome.ranking.retain(|r| r.contender != Contender::Club(0));
        rome.ranking.insert(
            0,
            Rating {
                contender: Contender::Club(0),
                elo: 2000,
            },
        );
        assert_eq!(rome.title_opponent(0, &club, rules), None);
    }

    #[test]
    fn a_resident_champion_defends_at_its_deadline() {
        let data = data();
        let rules = &data.world;
        let club = club(&data);
        let mut rng = ChaCha8Rng::seed_from_u64(2);
        let mut rome = rome(&data);
        let deadline = rome.defense_deadline(rules);
        let before = rome.clone();
        assert_eq!(
            rome.end_week(deadline - 1, &mut rng, &rules.colosseums[1], &data, &club),
            []
        );
        assert_eq!(rome, before);
        let news = rome.end_week(deadline, &mut rng, &rules.colosseums[1], &data, &club);
        let Some(&TitleNews::Fight {
            champion,
            challenger,
            result,
        }) = news.first()
        else {
            panic!("a title fight, not {news:?}");
        };
        assert_eq!(Some(champion), before.champion);
        assert_eq!(
            challenger,
            Contender::Resident(
                before
                    .best_resident(Some(match champion {
                        Contender::Resident(c) => c,
                        Contender::Club(_) => unreachable!(),
                    }))
                    .unwrap()
            )
        );
        assert_eq!(rome.champion == Some(champion), title_kept(result));
        assert!(rome.defense_deadline(rules) > deadline);
    }

    #[test]
    fn a_club_champion_past_its_deadline_loses_the_title() {
        let data = data();
        let rules = &data.world;
        let club = club(&data);
        let mut rng = ChaCha8Rng::seed_from_u64(2);
        let mut rome = rome(&data);
        let club_champion = Contender::Club(0);
        rome.record_title_fight(
            rome.champion.unwrap(),
            club_champion,
            FightResult::Lost,
            1,
            rules,
        );
        let deadline = rome.defense_deadline(rules);
        rome.end_week(deadline - 1, &mut rng, &rules.colosseums[1], &data, &club);
        assert_eq!(rome.champion, Some(club_champion));
        let news = rome.end_week(deadline, &mut rng, &rules.colosseums[1], &data, &club);
        let to = Some(
            rome.ranking
                .iter()
                .map(|r| r.contender)
                .find(|&c| c != club_champion)
                .unwrap(),
        );
        assert_eq!(
            news,
            [TitleNews::Stripped {
                former: club_champion,
                to
            }]
        );
        assert_eq!((rome.champion, rome.title_since), (to, deadline));
    }

    #[test]
    fn the_dead_are_replaced_and_a_vacant_title_goes_to_the_best_ranked() {
        let data = data();
        let rules = &data.world;
        let mut club = club(&data);
        let mut rng = ChaCha8Rng::seed_from_u64(2);
        let mut rome = rome(&data);
        let champion = Contender::Club(0);
        rome.record_title_fight(
            rome.champion.unwrap(),
            champion,
            FightResult::Lost,
            1,
            rules,
        );
        rome.record(
            Contender::Resident(5),
            Contender::Resident(6),
            FightResult::Won,
            rules,
        );
        let dead = rome.residents[5].clone();
        rome.residents[5].status = Status::Dead;
        assert_eq!(
            rome.end_week(2, &mut rng, &rules.colosseums[1], &data, &club),
            []
        );
        assert_ne!(rome.residents[5], dead);
        assert_eq!(rome.residents[5].status, Status::Active);
        assert_eq!(rome.elo(Contender::Resident(5), rules), rules.elo_start);
        assert_eq!(rome.champion, Some(champion));
        // The club's champion retires: the title goes to the first rank still fighting.
        club[0].status = Status::Retired;
        let news = rome.end_week(3, &mut rng, &rules.colosseums[1], &data, &club);
        let to = Some(rome.ranking[1].contender);
        assert_eq!(rome.ranking[0].contender, champion);
        assert_eq!(news, [TitleNews::Vacated { to }]);
        assert_eq!((rome.champion, rome.title_since), (to, 3));
    }
}
