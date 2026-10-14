//! Colosseums: each has its arena type, prestige and purses (from `data/world.ron`),
//! and in the game its residents, its Elo ranking and its champion.

use std::cmp::{Ordering, Reverse};

use rand::{Rng, RngExt};
use serde::{Deserialize, Serialize};

use crate::design_data::DesignData;
use crate::economy::Purses;
use crate::fighter::career::{CareerFighter, Status};
use crate::fighter::fame::FightResult;
use crate::fighter::percent_of;

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
    /// Holds the title, which is not the first rank: it is won by beating the champion.
    pub champion: Option<Contender>,
}

impl Colosseum {
    /// A colosseum's opening: residents as strong as its prestige, all at the starting
    /// Elo, the strongest one champion.
    pub fn open<R: Rng + ?Sized>(rng: &mut R, rules: &ColosseumData, data: &DesignData) -> Self {
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
        }
    }

    /// Elo of `contender`, the starting one for a first fight here.
    #[must_use]
    pub fn elo(&self, contender: Contender, rules: &WorldRules) -> i32 {
        self.ranking
            .iter()
            .find(|r| r.contender == contender)
            .map_or(rules.elo_start, |r| r.elo)
    }

    /// Records a duel between `a` and `b`, `result` being `a`'s: both Elo move,
    /// and beating the champion takes the title.
    pub fn record(&mut self, a: Contender, b: Contender, result: FightResult, rules: &WorldRules) {
        let places = match result {
            FightResult::Won => [(a, 1), (b, 2)],
            FightResult::Lost | FightResult::GaveUp => [(a, 2), (b, 1)],
            FightResult::Unfinished => [(a, 1), (b, 1)],
        };
        self.record_places(&places, rules);
        match result {
            FightResult::Won if self.champion == Some(b) => self.champion = Some(a),
            FightResult::Lost | FightResult::GaveUp if self.champion == Some(a) => {
                self.champion = Some(b);
            }
            _ => {}
        }
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

    /// End of the week: dead residents give way to new ones, who start again at the
    /// starting Elo, and a title whose holder is gone goes to the best ranked.
    /// `club_active(i)` tells whether the club's fighter `i` still fights.
    pub fn end_week<R: Rng + ?Sized>(
        &mut self,
        rng: &mut R,
        rules: &ColosseumData,
        data: &DesignData,
        club_active: impl Fn(usize) -> bool,
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
        let active = |c: &Contender| match *c {
            Contender::Club(i) => club_active(i),
            Contender::Resident(_) => true,
        };
        if !self.champion.as_ref().is_some_and(active) {
            self.champion = self.ranking.iter().map(|r| r.contender).find(active);
        }
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
        )
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
    fn beating_the_champion_takes_the_title_and_the_ranking_follows_elo() {
        let data = data();
        let rules = &data.world;
        let mut rome = rome(&data);
        let champion = rome.champion.unwrap();
        let club = Contender::Club(0);
        // A draw changes neither Elo nor title.
        rome.record(club, champion, FightResult::Unfinished, rules);
        assert_eq!(
            (rome.elo(club, rules), rome.champion),
            (1000, Some(champion))
        );
        rome.record(club, champion, FightResult::Won, rules);
        assert_eq!(rome.champion, Some(club));
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
        // Giving up against a resident loses the title to them.
        rome.record(club, Contender::Resident(3), FightResult::GaveUp, rules);
        assert_eq!(rome.champion, Some(Contender::Resident(3)));
    }

    #[test]
    fn the_dead_are_replaced_and_a_vacant_title_goes_to_the_best_ranked() {
        let data = data();
        let rules = &data.world;
        let mut rng = ChaCha8Rng::seed_from_u64(2);
        let mut rome = rome(&data);
        let club = Contender::Club(0);
        rome.record(club, rome.champion.unwrap(), FightResult::Won, rules);
        rome.record(
            Contender::Resident(5),
            Contender::Resident(6),
            FightResult::Won,
            rules,
        );
        let dead = rome.residents[5].clone();
        rome.residents[5].status = Status::Dead;
        rome.end_week(&mut rng, &rules.colosseums[1], &data, |_| true);
        assert_ne!(rome.residents[5], dead);
        assert_eq!(rome.residents[5].status, Status::Active);
        assert_eq!(rome.elo(Contender::Resident(5), rules), rules.elo_start);
        assert_eq!(rome.champion, Some(club));
        // The club's champion retires: the title goes to the first rank.
        rome.end_week(&mut rng, &rules.colosseums[1], &data, |_| false);
        assert_eq!(rome.champion, Some(rome.ranking[1].contender));
        assert_eq!(rome.ranking[0].contender, club);
    }
}
