//! Colosseums: each has its continent, arena type, prestige and purses (from
//! `data/world.ron`), and in the game its Elo ranking and its champion. Its fighters
//! come from the clubs and the market.

use std::cmp::{Ordering, Reverse};
use std::collections::BTreeMap;

use rand::{Rng, RngExt};
use serde::{Deserialize, Serialize};

use crate::combat::ArenaKind;
use crate::economy::Purses;
use crate::fighter::career::{CareerFighter, Status};
use crate::fighter::fame::FightResult;
use crate::relations::RelationRules;
use crate::rivals::{ClubRules, Profile, Profiles, Rival, RivalId, RosterRules};

/// The world's rules, from `data/world.ron`.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WorldRules {
    /// Elo of a fighter's first fight in a colosseum.
    pub elo_start: i32,
    /// Most Elo points one fight moves.
    pub elo_k: u32,
    /// Stats of the fighters generated for a colosseum grow by this percentage for each
    /// prestige level above 1.
    pub prestige_stats_percent: u32,
    /// A champion must defend the title at least once in this many weeks.
    pub title_defense_weeks: u32,
    /// How many of the first in a ranking may fight for the title.
    pub challengers: u32,
    pub challenges: ChallengeRules,
    /// The rival managers' thresholds.
    pub profiles: Profiles,
    /// The thresholds free agents choose their fights by, as managers without money:
    /// only where entry is free, never trained nor cared for.
    pub free_agents: Profile,
    pub roster: RosterRules,
    /// How the rival clubs are drawn.
    pub clubs: ClubRules,
    /// Added to the entry fee of a colosseum on another continent than the club's.
    pub travel_fee: u32,
    /// How many weeks the world's news are kept (`GameState::news`).
    pub news_weeks: u32,
    /// What the rival clubs think of the player's club (`GameState::relations`).
    pub relations: RelationRules,
    /// `ColosseumId(i)` is the colosseum at index `i`.
    pub colosseums: Vec<ColosseumData>,
}

impl WorldRules {
    #[must_use]
    pub fn colosseum(&self, id: ColosseumId) -> Option<&ColosseumData> {
        self.colosseums.get(usize::try_from(id.0).ok()?)
    }

    /// The arena kind of colosseum `id`; sand, as in Capoue, for one missing from the
    /// data, which no plan may enter.
    #[must_use]
    pub fn arena(&self, id: ColosseumId) -> ArenaKind {
        self.colosseum(id).map_or(ArenaKind::Sand, |c| c.arena)
    }

    /// What a club of `home` pays to register a fighter in colosseum `id`: its entry
    /// fee, and the journey (`travel_fee`) if it is on another continent. `None` for a
    /// colosseum missing from the data.
    #[must_use]
    pub fn entry_fee(&self, id: ColosseumId, home: Continent) -> Option<u32> {
        let colosseum = self.colosseum(id)?;
        let travel = if colosseum.continent == home {
            0
        } else {
            self.travel_fee
        };
        Some(colosseum.purses.entry_fee.saturating_add(travel))
    }

    /// Where a club of `home` starts: the first colosseum of its continent free to
    /// enter, or else the first one.
    #[must_use]
    pub fn first_colosseum(&self, home: Continent) -> ColosseumId {
        (0..)
            .zip(&self.colosseums)
            .find(|(_, c)| c.continent == home && c.purses.entry_fee == 0)
            .map_or(ColosseumId(0), |(id, _)| ColosseumId(id))
    }

    /// The colosseums of `continent`.
    pub fn colosseums_of(&self, continent: Continent) -> impl Iterator<Item = ColosseumId> + '_ {
        (0..)
            .zip(&self.colosseums)
            .filter(move |(_, c)| c.continent == continent)
            .map(|(id, _)| ColosseumId(id))
    }
}

/// The continents of the world, each with its colosseums and its clubs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum Continent {
    Europe,
    Asia,
    Americas,
    Africa,
    Oceania,
}

impl Continent {
    pub const ALL: [Self; 5] = [
        Self::Europe,
        Self::Asia,
        Self::Americas,
        Self::Africa,
        Self::Oceania,
    ];
}

/// Challenges between the club's fighters and the rival fighters; how often they are
/// sent and accepted depends on the rival clubs (`ProfileRules`).
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ChallengeRules {
    /// What a fighter who refuses a challenge and does not fight that week loses: fame,
    /// and Elo in the challenger's colosseum.
    pub refusal_fame: u32,
    pub refusal_elo: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ColosseumData {
    /// Names it in the language files (`colosseum-<key>`) and the tools.
    pub key: String,
    pub continent: Continent,
    pub arena: ArenaKind,
    /// From 1 to 5: the higher, the stronger the fighters generated for it.
    pub prestige: u32,
    /// Rival fighters ranked here at its opening, generated as strong as its prestige.
    pub residents: u32,
    pub purses: Purses,
}

/// Index of a colosseum in `WorldRules::colosseums` and `GameState::colosseums`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct ColosseumId(pub u32);

/// A rival fighter's challenge to a club's fighter, to accept or refuse this week.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Challenge {
    /// The club's fighter at this index.
    pub fighter: usize,
    pub colosseum: ColosseumId,
    pub rival: RivalId,
}

/// A fighter with an Elo in a colosseum. Its Elo and title follow it to another club
/// or to the market (`Colosseum::transfer`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum Contender {
    /// The club's fighter at this index.
    Club(usize),
    Rival(RivalId),
    /// The free agent at this index in `GameState::free_agents`.
    FreeAgent(usize),
}

impl Contender {
    /// Whether it is one of the player's fighters; the others, rival fighters and free
    /// agents, are the outsiders.
    #[must_use]
    pub fn is_club(self) -> bool {
        matches!(self, Self::Club(_))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Rating {
    pub contender: Contender,
    pub elo: i32,
}

/// Every fighter in the game: the player's club, the rival clubs and the market.
#[derive(Debug, Clone, Copy)]
pub struct Rosters<'a> {
    pub club: &'a [CareerFighter],
    pub rivals: &'a [Rival],
    pub free_agents: &'a [CareerFighter],
}

impl<'a> Rosters<'a> {
    #[must_use]
    pub fn fighter(self, contender: Contender) -> Option<&'a CareerFighter> {
        match contender {
            Contender::Club(i) => self.club.get(i),
            Contender::Rival(id) => self.rivals.get(id.club)?.fighters.get(id.fighter),
            Contender::FreeAgent(a) => self.free_agents.get(a),
        }
    }

    /// Whether `contender` still fights: neither retired, dead nor gone.
    #[must_use]
    pub fn active(self, contender: Contender) -> bool {
        self.fighter(contender)
            .is_some_and(|f| f.status == Status::Active)
    }
}

/// A colosseum in the game.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Colosseum {
    /// Everyone who fought here, best Elo first; ties keep their order. Fighters join
    /// with their first fight here; those who stopped fighting stay, unshown.
    pub ranking: Vec<Rating>,
    /// Holds the title, which is not the first rank: it changes hands only in a
    /// title fight the champion loses.
    pub champion: Option<Contender>,
    /// Week the title was last won or defended.
    pub title_since: u64,
}

/// What happened to a title in a week.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
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
    /// A colosseum's opening in `week`: `fighters` all ranked at the starting Elo, the
    /// strongest one champion.
    #[must_use]
    pub fn open(fighters: &[(Contender, &CareerFighter)], week: u64, rules: &WorldRules) -> Self {
        let champion = (0..fighters.len())
            .max_by_key(|&i| (total(fighters[i].1), Reverse(i)))
            .map(|i| fighters[i].0);
        let ranking = fighters
            .iter()
            .map(|&(contender, _)| Rating {
                contender,
                elo: rules.elo_start,
            })
            .collect();
        Self {
            ranking,
            champion,
            title_since: week,
        }
    }

    /// The ranking without those who no longer fight.
    pub fn active_ranking<'a>(
        &'a self,
        rosters: Rosters<'a>,
    ) -> impl Iterator<Item = &'a Rating> + 'a {
        self.ranking
            .iter()
            .filter(move |r| rosters.active(r.contender))
    }

    /// Last week the champion may defend the title in before losing it.
    #[must_use]
    pub fn defense_deadline(&self, rules: &WorldRules) -> u64 {
        self.title_since + u64::from(rules.title_defense_weeks)
    }

    /// The outsider the club's fighter `i` meets if it fights for the title here: as
    /// champion, the best-ranked outsider; from the first `challengers` of the ranking,
    /// an outsider champion. `None` if it may not fight for the title.
    // ponytail: an outsider champion only meets outsiders in its own defenses; club
    // fighters ask for their title fight.
    #[must_use]
    pub fn title_opponent(
        &self,
        i: usize,
        rosters: Rosters,
        rules: &WorldRules,
    ) -> Option<Contender> {
        let me = Contender::Club(i);
        if !rosters.active(me) {
            return None;
        }
        if self.champion == Some(me) {
            return self.best_outsider(rosters, None);
        }
        let champion = self.champion.filter(|c| !c.is_club())?;
        let top = usize::try_from(rules.challengers).unwrap_or(usize::MAX);
        self.active_ranking(rosters)
            .take(top)
            .any(|r| r.contender == me)
            .then_some(champion)
    }

    /// Whether `rival` accepts the challenge of the club's fighter `i`: Elo gives it
    /// at least `accept_percent` chances of winning.
    #[must_use]
    pub fn accepts_challenge(
        &self,
        rival: RivalId,
        i: usize,
        accept_percent: u32,
        rules: &WorldRules,
    ) -> bool {
        let theirs = self.elo(Contender::Rival(rival), rules);
        let mine = self.elo(Contender::Club(i), rules);
        expected(theirs, mine) * 100.0 >= f64::from(accept_percent)
    }

    /// The chance, from 0 to 1, Elo gives `contender` against the average of the other
    /// outsiders still fighting here; even when there is none.
    #[must_use]
    pub fn chance(&self, contender: Contender, rosters: Rosters, rules: &WorldRules) -> f64 {
        self.odds(rosters, rules).chance(contender)
    }

    /// What [`Colosseum::chance`] reads of this colosseum, gathered once to give the
    /// chance of many contenders.
    #[must_use]
    pub fn odds(&self, rosters: Rosters, rules: &WorldRules) -> Odds {
        let mut odds = Odds {
            ratings: BTreeMap::new(),
            sum: 0,
            count: 0,
            elo_start: rules.elo_start,
        };
        for r in &self.ranking {
            let counted = !r.contender.is_club() && rosters.active(r.contender);
            let entry = odds
                .ratings
                .entry(r.contender)
                .or_insert(Odds::entry(r.elo));
            if counted {
                entry.1 += i64::from(r.elo);
                entry.2 += 1;
                odds.sum += i64::from(r.elo);
                odds.count += 1;
            }
        }
        odds
    }

    /// The rival fighter still fighting closest in Elo to the club's fighter `i`, who
    /// challenges it; `None` if it has never fought here.
    #[must_use]
    pub fn challenger(&self, i: usize, rosters: Rosters) -> Option<RivalId> {
        let mine = self
            .ranking
            .iter()
            .find(|r| r.contender == Contender::Club(i))?;
        self.active_ranking(rosters)
            .filter_map(|r| match r.contender {
                Contender::Rival(id) => Some((id, r.elo.abs_diff(mine.elo))),
                Contender::Club(_) | Contender::FreeAgent(_) => None,
            })
            .min_by_key(|&(_, gap)| gap)
            .map(|(id, _)| id)
    }

    /// The best-ranked outsider still fighting, other than `except`.
    #[must_use]
    pub fn best_outsider(&self, rosters: Rosters, except: Option<Contender>) -> Option<Contender> {
        self.active_ranking(rosters)
            .map(|r| r.contender)
            .find(|&c| !c.is_club() && Some(c) != except)
    }

    /// Gives the Elo and the title of `from` to `to`: the same fighter, gone to another
    /// club or to the market.
    pub fn transfer(&mut self, from: Contender, to: Contender) {
        for rating in &mut self.ranking {
            if rating.contender == from {
                rating.contender = to;
            }
        }
        if self.champion == Some(from) {
            self.champion = Some(to);
        }
    }

    /// Gives a title whose holder no longer fights to the best ranked still fighting,
    /// in `week`.
    pub fn vacate_if_gone(&mut self, week: u64, rosters: Rosters) -> Option<TitleNews> {
        if self.champion.is_some_and(|c| rosters.active(c)) {
            return None;
        }
        let to = self.active_ranking(rosters).map(|r| r.contender).next();
        if to.is_none() && self.champion.is_none() {
            return None;
        }
        self.champion = to;
        self.title_since = week;
        Some(TitleNews::Vacated { to })
    }

    /// A club champion past its deadline in `week` loses the title to the best ranked
    /// still fighting.
    pub fn strip(&mut self, week: u64, rosters: Rosters, rules: &WorldRules) -> Option<TitleNews> {
        let former @ Contender::Club(_) = self.champion? else {
            return None;
        };
        if week < self.defense_deadline(rules) {
            return None;
        }
        let to = self
            .active_ranking(rosters)
            .map(|r| r.contender)
            .find(|&c| c != former);
        self.champion = to;
        self.title_since = week;
        Some(TitleNews::Stripped { former, to })
    }

    /// Takes `points` of Elo from `contender`, a fighter who refused a challenge.
    pub fn lose_elo(&mut self, contender: Contender, points: u32, rules: &WorldRules) {
        self.add_elo(contender, -i32::try_from(points).unwrap_or(i32::MAX), rules);
        self.ranking.sort_by_key(|r| Reverse(r.elo));
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
}

/// A colosseum's Elo as [`Colosseum::chance`] reads it (see [`Colosseum::odds`]).
#[derive(Debug, Clone, PartialEq)]
pub struct Odds {
    /// Each one ranked: its Elo, and the sum and number of its ratings among the
    /// outsiders still fighting.
    ratings: BTreeMap<Contender, (i32, i64, i64)>,
    /// Sum and number of the Elo of the outsiders still fighting.
    sum: i64,
    count: i64,
    elo_start: i32,
}

impl Odds {
    fn entry(elo: i32) -> (i32, i64, i64) {
        (elo, 0, 0)
    }

    /// The chance, from 0 to 1, Elo gives `contender` against the average of the other
    /// outsiders still fighting; even when there is none.
    #[must_use]
    pub fn chance(&self, contender: Contender) -> f64 {
        let (elo, own_sum, own_count) = self
            .ratings
            .get(&contender)
            .copied()
            .unwrap_or(Self::entry(self.elo_start));
        let average = (self.sum - own_sum)
            .checked_div(self.count - own_count)
            .and_then(|a| i32::try_from(a).ok());
        average.map_or(0.5, |a| expected(elo, a))
    }
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

/// Picks the outsider a club's fighter meets among `candidates`.
// ponytail: at random; a chosen opponent is a challenge.
pub fn pick_opponent<R: Rng + ?Sized>(rng: &mut R, candidates: &[Contender]) -> Option<Contender> {
    (!candidates.is_empty()).then(|| candidates[rng.random_range(0..candidates.len())])
}

fn total(fighter: &CareerFighter) -> u32 {
    fighter.stats.to_array().iter().sum()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::design_data::DesignData;
    use crate::economy::Wealth;
    use crate::market::with_prestige;
    use crate::staff::Staff;
    use rand::SeedableRng;
    use rand_chacha::ChaCha8Rng;

    fn data() -> DesignData {
        DesignData::load().unwrap()
    }

    /// A club of one active fighter, and one rival club of 15 fighters, all ranked in
    /// Rome.
    struct World {
        club: Vec<CareerFighter>,
        rivals: Vec<Rival>,
        rome: Colosseum,
    }

    impl World {
        fn rosters(&self) -> Rosters<'_> {
            Rosters {
                club: &self.club,
                rivals: &self.rivals,
                free_agents: &[],
            }
        }
    }

    fn rival(fighter: usize) -> Contender {
        Contender::Rival(RivalId { club: 0, fighter })
    }

    fn world(data: &DesignData) -> World {
        let mut rng = ChaCha8Rng::seed_from_u64(1);
        let fighters = (0..15)
            .map(|_| with_prestige(CareerFighter::random(&mut rng, data), 3, data))
            .collect();
        let rivals = vec![Rival {
            name: crate::names::ClubName::default(),
            culture: "rome".into(),
            continent: Continent::Europe,
            profile: Profile::Cautious,
            money: 0,
            fighters,
            staff: Staff::default(),
            wealth: Wealth::Modest,
            ruined_seasons: 0,
            closed: None,
        }];
        let ranked: Vec<_> = rivals[0]
            .fighters
            .iter()
            .enumerate()
            .map(|(j, f)| (rival(j), f))
            .collect();
        let rome = Colosseum::open(&ranked, 1, &data.world);
        let club = vec![CareerFighter::random(
            &mut ChaCha8Rng::seed_from_u64(3),
            data,
        )];
        World { club, rivals, rome }
    }

    /// Elo won by a fighter of Elo `a` placed `place_a` in a duel against one of Elo
    /// `b` placed `place_b`; the other loses as many.
    fn duel(a: i32, b: i32, place_a: u32, place_b: u32) -> i32 {
        let data = data();
        let rules = &data.world;
        let mut colosseum = world(&data).rome;
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
        let opened = world(&data).rome;
        let mut melee = opened.clone();
        // Twenty at the starting Elo, out in order: 0 is the last standing.
        let contenders: Vec<_> = (0..20).map(rival).collect();
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
    fn a_colosseum_opens_with_its_fighters_ranked_and_the_strongest_champion() {
        let data = data();
        let world = world(&data);
        let rome = &world.rome;
        assert_eq!(rome.ranking.len(), 15);
        assert!(rome.ranking.iter().all(|r| r.elo == data.world.elo_start));
        let champion = world.rosters().fighter(rome.champion.unwrap()).unwrap();
        assert!(
            world.rivals[0]
                .fighters
                .iter()
                .all(|f| total(f) <= total(champion))
        );
    }

    #[test]
    fn only_a_title_fight_moves_the_title() {
        let data = data();
        let rules = &data.world;
        let mut rome = world(&data).rome;
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
    fn the_champion_meets_the_best_rival_and_the_top_meets_the_champion() {
        let data = data();
        let rules = &data.world;
        let mut world = world(&data);
        let champion = world.rome.champion;
        assert!(matches!(champion, Some(Contender::Rival(_))));
        // Not ranked yet: no title fight.
        assert_eq!(world.rome.title_opponent(0, world.rosters(), rules), None);
        world
            .rome
            .record(Contender::Club(0), rival(9), FightResult::Won, rules);
        assert_eq!(
            world.rome.title_opponent(0, world.rosters(), rules),
            champion
        );
        // Out of the top five: no title fight either.
        for r in 0..6 {
            world
                .rome
                .record(rival(r), rival(14), FightResult::Won, rules);
            world
                .rome
                .record(rival(r), rival(13), FightResult::Won, rules);
        }
        assert_eq!(world.rome.title_opponent(0, world.rosters(), rules), None);
        // As champion, against the best-ranked rival fighter still fighting.
        world.rome.champion = Some(Contender::Club(0));
        let best @ Contender::Rival(id) = world.rome.ranking[0].contender else {
            panic!("a rival fighter leads");
        };
        assert_eq!(
            world.rome.title_opponent(0, world.rosters(), rules),
            Some(best)
        );
        world.rivals[0].fighters[id.fighter].status = Status::Dead;
        let second = world.rome.best_outsider(world.rosters(), None).unwrap();
        assert_ne!(second, best);
        assert_eq!(
            world.rome.title_opponent(0, world.rosters(), rules),
            Some(second)
        );
        assert_eq!(world.rome.title_opponent(1, world.rosters(), rules), None);
        // A retired fighter fights no more.
        world.club[0].status = Status::Retired;
        world.rome.champion = champion;
        world
            .rome
            .ranking
            .retain(|r| r.contender != Contender::Club(0));
        world.rome.ranking.insert(
            0,
            Rating {
                contender: Contender::Club(0),
                elo: 2000,
            },
        );
        assert_eq!(world.rome.title_opponent(0, world.rosters(), rules), None);
    }

    #[test]
    fn a_club_champion_past_its_deadline_loses_the_title() {
        let data = data();
        let rules = &data.world;
        let mut world = world(&data);
        let club_champion = Contender::Club(0);
        let holder = world.rome.champion.unwrap();
        world
            .rome
            .record_title_fight(holder, club_champion, FightResult::Lost, 1, rules);
        let deadline = world.rome.defense_deadline(rules);
        let rosters = Rosters {
            club: &world.club,
            rivals: &world.rivals,
            free_agents: &[],
        };
        assert_eq!(world.rome.strip(deadline - 1, rosters, rules), None);
        assert_eq!(world.rome.champion, Some(club_champion));
        let news = world.rome.strip(deadline, rosters, rules);
        let to = world
            .rome
            .ranking
            .iter()
            .map(|r| r.contender)
            .find(|&c| c != club_champion);
        assert_eq!(
            news,
            Some(TitleNews::Stripped {
                former: club_champion,
                to
            })
        );
        assert_eq!(
            (world.rome.champion, world.rome.title_since),
            (to, deadline)
        );
    }

    #[test]
    fn the_closest_rival_challenges_and_only_those_with_a_chance_accept() {
        let data = data();
        let rules = &data.world;
        let mut world = world(&data);
        let (zero, three) = (
            RivalId {
                club: 0,
                fighter: 0,
            },
            RivalId {
                club: 0,
                fighter: 3,
            },
        );
        let rosters = Rosters {
            club: &world.club,
            rivals: &world.rivals,
            free_agents: &[],
        };
        let rome = &mut world.rome;
        // Never fought here: no challenge, and everyone accepts the newcomer's.
        assert_eq!(rome.challenger(0, rosters), None);
        assert!(rome.accepts_challenge(zero, 0, 30, rules));
        rome.add_elo(Contender::Club(0), 100, rules);
        rome.add_elo(rival(3), 90, rules);
        rome.ranking.sort_by_key(|r| Reverse(r.elo));
        assert_eq!(rome.challenger(0, rosters), Some(three));
        // 100 points behind still leaves a rival fighter 36 % of chances.
        assert!(rome.accepts_challenge(zero, 0, 30, rules));
        assert!(!rome.accepts_challenge(zero, 0, 40, rules));
        rome.add_elo(Contender::Club(0), 200, rules);
        assert!(!rome.accepts_challenge(zero, 0, 30, rules));
        assert!(!rome.accepts_challenge(three, 0, 30, rules));
        assert!(rome.accepts_challenge(three, 0, 5, rules));
        // Rival 3, 90 points above the others, has 63 % of chances against them.
        assert!((rome.chance(rival(3), rosters, rules) - 0.627).abs() < 0.001);
        assert!((rome.chance(rival(4), rosters, rules) - 0.491).abs() < 0.001);
        rome.lose_elo(Contender::Club(0), 8, rules);
        assert_eq!(rome.elo(Contender::Club(0), rules), 1292);
        assert_eq!(rome.ranking[0].contender, Contender::Club(0));
    }

    #[test]
    fn free_agents_are_outsiders_and_elo_follows_a_fighter_to_its_club() {
        let data = data();
        let rules = &data.world;
        let mut world = world(&data);
        let agents = vec![CareerFighter::random(
            &mut ChaCha8Rng::seed_from_u64(5),
            &data,
        )];
        let agent = Contender::FreeAgent(0);
        world.rome.record(agent, rival(0), FightResult::Won, rules);
        let rosters = Rosters {
            club: &world.club,
            rivals: &world.rivals,
            free_agents: &agents,
        };
        // The best ranked outsider, the club's fighters' opponent for the title.
        assert_eq!(world.rome.best_outsider(rosters, None), Some(agent));
        world.rome.champion = Some(agent);
        assert_eq!(
            world.rome.best_outsider(rosters, Some(agent)),
            Some(rival(1))
        );
        world
            .rome
            .record(Contender::Club(0), rival(2), FightResult::Won, rules);
        assert_eq!(world.rome.title_opponent(0, rosters, rules), Some(agent));
        // Against the other outsiders, it is ahead.
        assert!(world.rome.chance(agent, rosters, rules) > 0.5);
        // Signed by the club, it takes its Elo and title along.
        let elo = world.rome.elo(agent, rules);
        let signed = Contender::Club(1);
        world.rome.transfer(agent, signed);
        assert_eq!(world.rome.elo(signed, rules), elo);
        assert_eq!(world.rome.champion, Some(signed));
        assert!(world.rome.ranking.iter().all(|r| r.contender != agent));
    }

    #[test]
    fn a_vacant_title_goes_to_the_best_ranked_still_fighting() {
        let data = data();
        let rules = &data.world;
        let mut world = world(&data);
        let champion = Contender::Club(0);
        let holder = world.rome.champion.unwrap();
        world
            .rome
            .record_title_fight(holder, champion, FightResult::Lost, 1, rules);
        world
            .rome
            .record(rival(5), rival(6), FightResult::Won, rules);
        let mut rome = world.rome.clone();
        assert_eq!(rome.vacate_if_gone(2, world.rosters()), None);
        // The club's champion retires, and the first rank is dead.
        world.club[0].status = Status::Retired;
        world.rivals[0].fighters[5].status = Status::Dead;
        let news = rome.vacate_if_gone(3, world.rosters());
        let to = Some(rome.ranking[2].contender);
        assert_eq!(rome.ranking[0].contender, champion);
        assert_eq!(rome.ranking[1].contender, rival(5));
        assert_eq!(news, Some(TitleNews::Vacated { to }));
        assert_eq!((rome.champion, rome.title_since), (to, 3));
    }
}
