//! The calendar: seasons of weeks, each split into phases. The game only counts
//! weeks from the start; seasons and phases are derived from that count.

use serde::Deserialize;

/// Lengths of a season's phases, in weeks, from `data/calendar.ron`.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CalendarRules {
    pub competition_weeks: u32,
    pub great_event_weeks: u32,
    pub off_season_weeks: u32,
}

impl CalendarRules {
    /// Weeks in a season, a year of the game.
    #[must_use]
    pub fn season_weeks(&self) -> u32 {
        self.competition_weeks
            .saturating_add(self.great_event_weeks)
            .saturating_add(self.off_season_weeks)
    }
}

/// The phases of a season, in order.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SeasonPhase {
    /// The Classic in each colosseum, challenges between clubs.
    Competition,
    /// World championship or Inter-Coliseum, depending on the season.
    GreatEvent,
    /// Ageing, retirements, rookies, open market.
    OffSeason,
}

/// A week of the game, as a season and a week within it, both from 1.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Date {
    pub season: u64,
    pub week: u32,
}

impl Date {
    /// The date of the game's `week`-th week, counting from 1.
    ///
    /// # Panics
    /// If the season has no week, which [`crate::design_data::DesignData::from_ron`] rejects.
    #[must_use]
    pub fn of(week: u64, rules: &CalendarRules) -> Self {
        let length = u64::from(rules.season_weeks());
        assert!(length > 0, "a season of no week");
        let index = week.saturating_sub(1);
        Self {
            season: index / length + 1,
            // Below `length`, itself a u32.
            week: u32::try_from(index % length).unwrap_or(u32::MAX) + 1,
        }
    }

    #[must_use]
    pub fn phase(self, rules: &CalendarRules) -> SeasonPhase {
        if self.week <= rules.competition_weeks {
            SeasonPhase::Competition
        } else if self.week <= rules.competition_weeks + rules.great_event_weeks {
            SeasonPhase::GreatEvent
        } else {
            SeasonPhase::OffSeason
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const RULES: CalendarRules = CalendarRules {
        competition_weeks: 22,
        great_event_weeks: 4,
        off_season_weeks: 4,
    };

    fn date(week: u64) -> Date {
        Date::of(week, &RULES)
    }

    #[test]
    fn a_season_is_thirty_weeks() {
        assert_eq!(RULES.season_weeks(), 30);
        assert_eq!(date(1), Date { season: 1, week: 1 });
        assert_eq!(
            date(30),
            Date {
                season: 1,
                week: 30
            }
        );
        assert_eq!(date(31), Date { season: 2, week: 1 });
        assert_eq!(date(95), Date { season: 4, week: 5 });
    }

    #[test]
    fn phases_follow_each_other() {
        let phase = |week| date(week).phase(&RULES);
        assert_eq!(phase(1), SeasonPhase::Competition);
        assert_eq!(phase(22), SeasonPhase::Competition);
        assert_eq!(phase(23), SeasonPhase::GreatEvent);
        assert_eq!(phase(26), SeasonPhase::GreatEvent);
        assert_eq!(phase(27), SeasonPhase::OffSeason);
        assert_eq!(phase(30), SeasonPhase::OffSeason);
        assert_eq!(phase(31), SeasonPhase::Competition);
    }
}
