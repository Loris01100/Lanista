//! What each rival club thinks of the player's club: a score from −100 to +100,
//! moved by what happens between them and coming back slowly to 0, each change kept
//! with its reason, and the standing it gives, which sets prices and challenges.

use serde::{Deserialize, Serialize};

use crate::rivals::{ProfileRules, RivalId};

/// The score's bounds.
pub const MAX_SCORE: i32 = 100;

/// What a club's relation with the player's club comes to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum Standing {
    Ally,
    Friend,
    Neutral,
    Tense,
    Rival,
}

impl Standing {
    pub const ALL: [Self; 5] = [
        Self::Ally,
        Self::Friend,
        Self::Neutral,
        Self::Tense,
        Self::Rival,
    ];
}

/// Why a relation moved; club fighters by their place in the player's club.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Reason {
    /// The club's `fighter` killed `victim`, of this club.
    Killed { fighter: usize, victim: RivalId },
    /// The club's `fighter` beat `rival`, of this club.
    Beaten { fighter: usize, rival: RivalId },
    /// The club's `fighter` took this club's title in the colosseum at this index.
    TitleTaken { fighter: usize, colosseum: usize },
    /// The player sold its `fighter` to this club.
    Sold { fighter: usize },
    /// The player bought from this club the fighter now its `fighter`.
    Bought { fighter: usize },
    /// The club's `fighter` kept its pact with `rival`, of this club, in a melee.
    PactKept { fighter: usize, rival: RivalId },
    /// The club's `fighter` betrayed its pact with `rival`, of this club, in a melee.
    Betrayed { fighter: usize, rival: RivalId },
}

/// One change of a relation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Change {
    pub week: u64,
    pub points: i32,
    pub why: Reason,
}

/// A club's relation with the player's club.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Relation {
    /// From −100 to +100.
    pub score: i32,
    /// The last changes, oldest first, at most `RelationRules::history`.
    pub changes: Vec<Change>,
    /// The week the club declared itself the player's rival, while it lasts.
    pub rival_since: Option<u64>,
}

impl Relation {
    /// Moves the score by `why`'s points from `rules`, weighed by the club's
    /// `profile`: a fall by its `anger_percent`, a deal by its `deal_percent`. A change
    /// of no points is not kept.
    pub fn change(
        &mut self,
        (week, why): (u64, Reason),
        profile: &ProfileRules,
        rules: &RelationRules,
    ) {
        let base = rules.points(why);
        let weight = match why {
            Reason::Sold { .. } | Reason::Bought { .. } => profile.deal_percent,
            _ if base < 0 => profile.anger_percent,
            _ => 100,
        };
        let points = i32::try_from(i64::from(base) * i64::from(weight) / 100).unwrap_or(0);
        if points == 0 {
            return;
        }
        self.score = (self.score + points).clamp(-MAX_SCORE, MAX_SCORE);
        self.changes.push(Change { week, points, why });
        let excess = self.changes.len().saturating_sub(rules.history);
        self.changes.drain(..excess);
    }

    /// A week goes by: the score comes back `RelationRules::decay` points towards 0.
    pub fn fade(&mut self, rules: &RelationRules) {
        let decay = i32::try_from(rules.decay).unwrap_or(MAX_SCORE);
        self.score -= self.score.signum() * decay.min(self.score.abs());
    }

    /// At the end of week `week`, the club declares itself the player's rival once its
    /// score is a rival's, and ends the rivalry once it climbs back to
    /// `RivalryRules::ends_from`. Returns `Some(true)` for a declaration, `Some(false)`
    /// for an end.
    pub fn declare(&mut self, week: u64, rules: &RelationRules) -> Option<bool> {
        match self.rival_since {
            None if rules.standings.of(self.score) == Standing::Rival => {
                self.rival_since = Some(week);
                Some(true)
            }
            Some(_) if self.score >= rules.rivalry.ends_from => {
                self.rival_since = None;
                Some(false)
            }
            _ => None,
        }
    }
}

/// What a declared rivalry does.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RivalryRules {
    /// The score from which a declared rivalry ends.
    pub ends_from: i32,
    /// A fight between the rivals' fighters brings this percentage of its receipts.
    pub receipts_percent: u32,
    /// And this much fame to each of them, whatever the result.
    pub fame: u32,
}

/// What a standing does.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StandingRules {
    /// The lowest score of this standing.
    pub from: i32,
    /// Whether the club trades with the player at all.
    pub deals: bool,
    /// The club asks the player this percentage of its price, and offers its price
    /// divided by it for the player's fighters.
    pub price_percent: u32,
    /// Taken from the chance its fighter needs to accept the player's challenges, in
    /// points.
    pub challenge_points: u32,
    /// Added to the chance, in percent at each decision of a melee, that its fighters
    /// make a pact with the player's.
    pub pact_points: i32,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Standings {
    pub ally: StandingRules,
    pub friend: StandingRules,
    pub neutral: StandingRules,
    pub tense: StandingRules,
    pub rival: StandingRules,
}

impl Standings {
    #[must_use]
    pub fn get(&self, standing: Standing) -> &StandingRules {
        match standing {
            Standing::Ally => &self.ally,
            Standing::Friend => &self.friend,
            Standing::Neutral => &self.neutral,
            Standing::Tense => &self.tense,
            Standing::Rival => &self.rival,
        }
    }

    /// The standing of `score`: the first, from the ally down, it reaches.
    #[must_use]
    pub fn of(&self, score: i32) -> Standing {
        Standing::ALL
            .into_iter()
            .find(|&s| score >= self.get(s).from)
            .unwrap_or(Standing::Rival)
    }
}

/// The relations' rules, from `data/world.ron`: points of each event, before the
/// club's profile weighs them.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RelationRules {
    pub killed: i32,
    pub beaten: i32,
    pub title_taken: i32,
    /// A sale or a purchase concluded, whichever way.
    pub deal: i32,
    pub pact_kept: i32,
    pub betrayed: i32,
    /// Points a score comes back towards 0 each week.
    pub decay: u32,
    /// Changes kept for each club.
    pub history: usize,
    pub standings: Standings,
    pub rivalry: RivalryRules,
}

impl RelationRules {
    #[must_use]
    pub fn points(&self, why: Reason) -> i32 {
        match why {
            Reason::Killed { .. } => self.killed,
            Reason::Beaten { .. } => self.beaten,
            Reason::TitleTaken { .. } => self.title_taken,
            Reason::Sold { .. } | Reason::Bought { .. } => self.deal,
            Reason::PactKept { .. } => self.pact_kept,
            Reason::Betrayed { .. } => self.betrayed,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::design_data::DesignData;

    #[test]
    fn a_change_is_weighed_by_the_profile_kept_and_fades() {
        let data = DesignData::load().unwrap();
        let rules = &data.world.relations;
        let aggressive = &data.world.profiles.aggressive;
        let merchant = &data.world.profiles.merchant;
        let rival = RivalId {
            club: 0,
            fighter: 0,
        };
        let killed = Reason::Killed {
            fighter: 0,
            victim: rival,
        };
        let mut relation = Relation::default();
        relation.change((3, killed), aggressive, rules);
        let points = rules.killed * i32::try_from(aggressive.anger_percent).unwrap() / 100;
        assert!(points < rules.killed);
        assert_eq!(relation.score, points);
        assert_eq!(
            relation.changes,
            vec![Change {
                week: 3,
                points,
                why: killed
            }]
        );
        relation.fade(rules);
        assert_eq!(relation.score, points + i32::try_from(rules.decay).unwrap());

        let mut deal = Relation::default();
        deal.change((1, Reason::Sold { fighter: 0 }), merchant, rules);
        assert!(deal.score > rules.deal);
        for _ in 0..100 {
            deal.fade(rules);
        }
        assert_eq!(deal.score, 0);
    }

    #[test]
    fn the_score_stays_in_bounds_and_the_history_short() {
        let data = DesignData::load().unwrap();
        let rules = &data.world.relations;
        let profile = &data.world.profiles.aggressive;
        let why = Reason::Betrayed {
            fighter: 0,
            rival: RivalId {
                club: 0,
                fighter: 0,
            },
        };
        let mut relation = Relation::default();
        for week in 0..100 {
            relation.change((week, why), profile, rules);
        }
        assert_eq!(relation.score, -MAX_SCORE);
        assert_eq!(relation.changes.len(), rules.history);
        assert_eq!(relation.changes.last().unwrap().week, 99);
    }

    #[test]
    fn a_rivalry_is_declared_below_tense_and_ends_well_above() {
        let data = DesignData::load().unwrap();
        let rules = &data.world.relations;
        let mut relation = Relation {
            score: rules.standings.tense.from,
            ..Relation::default()
        };
        assert_eq!(relation.declare(1, rules), None);
        relation.score -= 1;
        assert_eq!(relation.declare(2, rules), Some(true));
        assert_eq!(relation.declare(3, rules), None);
        relation.score = rules.rivalry.ends_from - 1;
        assert_eq!(relation.declare(4, rules), None);
        assert_eq!(relation.rival_since, Some(2));
        relation.score += 1;
        assert_eq!(relation.declare(5, rules), Some(false));
        assert_eq!(relation.rival_since, None);
    }

    #[test]
    fn each_score_has_its_standing() {
        let data = DesignData::load().unwrap();
        let standings = &data.world.relations.standings;
        assert_eq!(standings.of(MAX_SCORE), Standing::Ally);
        assert_eq!(standings.of(0), Standing::Neutral);
        assert_eq!(standings.of(-MAX_SCORE), Standing::Rival);
        for s in Standing::ALL {
            assert_eq!(standings.of(standings.get(s).from), s);
        }
    }
}
