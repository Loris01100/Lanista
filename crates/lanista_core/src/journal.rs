//! The club's journal, kept in the save: each fighter's weeks, short enough to keep
//! them all, and the last week played whole, its fights with what plays them again
//! from their seed (`FightSummary::start`).

use serde::{Deserialize, Serialize};

use crate::design_data::DesignData;
use crate::fighter::career::{Activity, CareerFighter, FightSummary, Status, WeekReport};
use crate::fighter::equipment::Refused;
use crate::fighter::fame::FightResult;
use crate::fighter::illness::Illness;
use crate::fighter::injury::Injury;
use crate::fighter::retirement::too_maimed;
use crate::fighter::traits::Trait;
use crate::fighter::{ClassId, Stats};
use crate::log::FighterReport;
use crate::state::WeekSummary;

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Journal {
    /// Each club fighter's weeks, newest first, in club order.
    pub fighters: Vec<Vec<FighterWeek>>,
    /// The last week played, whole.
    pub last: Option<PlayedWeek>,
}

/// A week the club played.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PlayedWeek {
    pub week: u64,
    /// The club's money before and after it.
    pub money: (i64, i64),
    pub summary: WeekSummary,
}

/// What a week did to one of the club's fighters.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FighterWeek {
    pub week: u64,
    pub activity: Activity,
    pub fight: Option<FightLine>,
    pub new_injury: Option<Injury>,
    /// The doctor kept the new injury from leaving an after-effect.
    pub after_effect_spared: bool,
    pub healed_injuries: Vec<Injury>,
    pub new_illness: Option<Illness>,
    pub illness_healed: bool,
    /// The traits it earned.
    #[serde(default)]
    pub new_traits: Vec<Trait>,
    /// A year went by: the fighter is now `age`.
    pub birthday: bool,
    pub season_gains: u32,
    /// Form, morale and fame, before and after the week.
    pub form: (u32, u32),
    pub morale: (u32, u32),
    pub fame: (u32, u32),
    pub stats: (Stats, Stats),
    /// Its age after the week.
    pub age: u32,
    pub retired: Option<Retired>,
}

/// Why a fighter retired.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Retired {
    /// Its age, or its decline.
    OfAge,
    /// Too many limbs lost (`too_maimed`).
    Maimed,
}

/// A fight, as the journal tells it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FightLine {
    pub result: FightResult,
    pub died: bool,
    /// The fighter's line of the fight's report.
    pub mine: Option<FighterReport>,
    /// Ticks the fight could last.
    pub max_ticks: u32,
    pub against_stronger: bool,
    pub refused: Refused,
    /// Fighters in the fight, the fighter included: 2 in a duel.
    pub size: usize,
    /// Its place, 1 the best.
    pub place: u32,
    /// The class of the opponent of a duel, the first other fighter of a melee.
    pub opponent: Option<ClassId>,
    pub fame_change: i32,
}

impl FightLine {
    #[must_use]
    pub fn new(fight: &FightSummary, data: &DesignData) -> Self {
        Self {
            result: fight.result,
            died: fight.died,
            mine: fight.mine().cloned(),
            max_ticks: fight.start(data).max_ticks,
            against_stronger: fight.against_stronger,
            refused: fight.refused,
            size: fight.setup.fighters.len(),
            place: fight.place,
            opponent: fight.opponent().map(|o| o.class),
            fame_change: fight.fame_change,
        }
    }

    /// Whether more than two fought.
    #[must_use]
    pub fn is_melee(&self) -> bool {
        self.size > 2
    }
}

impl FighterWeek {
    /// The week `report` of a fighter, `before` it and `after`.
    #[must_use]
    pub fn new(
        week: u64,
        (before, after): (&CareerFighter, &CareerFighter),
        report: &WeekReport,
        data: &DesignData,
    ) -> Self {
        Self {
            week,
            activity: report.plan.activity,
            fight: report.fight.as_ref().map(|f| FightLine::new(f, data)),
            new_injury: report.new_injury,
            after_effect_spared: report.after_effect_spared,
            healed_injuries: report.healed_injuries.clone(),
            new_illness: report.new_illness,
            illness_healed: report.illness_healed,
            new_traits: after
                .traits
                .iter()
                .filter(|t| !before.traits.contains(t))
                .copied()
                .collect(),
            birthday: report.birthday,
            season_gains: report.season_gains,
            form: (before.form.value(), after.form.value()),
            morale: (before.morale.value(), after.morale.value()),
            fame: (before.fame.value(), after.fame.value()),
            stats: (before.stats, after.stats),
            age: after.age,
            retired: (after.status == Status::Retired).then(|| {
                if too_maimed(&after.injuries, &data.fighter.retirement) {
                    Retired::Maimed
                } else {
                    Retired::OfAge
                }
            }),
        }
    }
}

impl Journal {
    /// Keeps week `week`, played from the club's fighters `before` and money
    /// `money.0` to `fighters` and `money.1`.
    pub fn record(
        &mut self,
        week: u64,
        (before, fighters): (&[CareerFighter], &[CareerFighter]),
        money: (i64, i64),
        summary: &WeekSummary,
        data: &DesignData,
    ) {
        self.fighters.resize_with(fighters.len(), Vec::new);
        for (i, report) in summary.reports.iter().enumerate() {
            let (Some(report), Some(after)) = (report, fighters.get(i)) else {
                continue;
            };
            // A free agent signed this week was not there before it.
            let before = before.get(i).unwrap_or(after);
            let entry = FighterWeek::new(week, (before, after), report, data);
            self.fighters[i].insert(0, entry);
        }
        self.last = Some(PlayedWeek {
            week,
            money,
            summary: summary.clone(),
        });
    }

    /// Every fighter of the club's last week's fights, for checking a save.
    pub fn fought(&self) -> impl Iterator<Item = &CareerFighter> {
        let reports = self.last.iter().flat_map(|w| &w.summary.reports);
        let fights = reports.flatten().filter_map(|r| r.fight.as_ref());
        fights.flat_map(|f| &f.setup.fighters)
    }
}
