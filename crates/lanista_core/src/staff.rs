//! The club's staff, paid every week: a trainer, a doctor and a recruiter, each from
//! level 1 to 5, a better one doing more for a higher salary. The trainer raises the
//! training gains and spares form, the doctor makes care shorten recoveries more and
//! spares after-effects, the recruiter shows more of the fighters out of the club.

use serde::{Deserialize, Serialize};

use crate::fighter::care::CareRules;
use crate::fighter::illness::ByLevel;
use crate::fighter::injury::BySeverity;
use crate::fighter::percent_of;

/// The staff levels on offer, from `data/economy.ron`; each list starts at level 1.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StaffRules {
    pub trainers: Vec<TrainerRules>,
    pub doctors: Vec<DoctorRules>,
    pub recruiters: Vec<RecruiterRules>,
}

/// Index of `level`, from 1, in a list of levels.
fn index(level: u32) -> Option<usize> {
    usize::try_from(level.checked_sub(1)?).ok()
}

impl StaffRules {
    /// The rules of trainer level `level`, from 1.
    #[must_use]
    pub fn trainer(&self, level: u32) -> Option<&TrainerRules> {
        self.trainers.get(index(level)?)
    }

    /// The rules of doctor level `level`, from 1.
    #[must_use]
    pub fn doctor(&self, level: u32) -> Option<&DoctorRules> {
        self.doctors.get(index(level)?)
    }

    /// The rules of recruiter level `level`, from 1.
    #[must_use]
    pub fn recruiter(&self, level: u32) -> Option<&RecruiterRules> {
        self.recruiters.get(index(level)?)
    }

    /// The weekly salary of `role` at `level`; `None` for a level not on offer.
    #[must_use]
    pub fn salary(&self, role: Role, level: u32) -> Option<u32> {
        match role {
            Role::Trainer => self.trainer(level).map(|t| t.salary),
            Role::Doctor => self.doctor(level).map(|d| d.salary),
            Role::Recruiter => self.recruiter(level).map(|r| r.salary),
        }
    }

    /// How many levels of `role` are on offer.
    #[must_use]
    pub fn levels(&self, role: Role) -> usize {
        match role {
            Role::Trainer => self.trainers.len(),
            Role::Doctor => self.doctors.len(),
            Role::Recruiter => self.recruiters.len(),
        }
    }
}

/// One trainer level.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TrainerRules {
    /// Paid every week, even into debt.
    pub salary: u32,
    /// Paid for each training session, instead of `EconomyRules::training_cost`.
    pub training_cost: u32,
    pub coaching: Coaching,
}

/// One doctor level.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DoctorRules {
    /// Paid every week, even into debt.
    pub salary: u32,
    pub medicine: Medicine,
}

/// One recruiter level.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RecruiterRules {
    /// Paid every week, even into debt.
    pub salary: u32,
    pub scouting: Scouting,
}

/// What a trainer brings to each training session; nothing without one.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Coaching {
    /// Added to each stat's chance to improve, in percent of that chance.
    pub progress_percent: u32,
    /// Form a session costs less, never below nothing.
    pub form_spared: u32,
}

/// What a doctor brings to care and injuries; nothing without one. A permanent
/// injury already there stays: no doctor heals a lost arm.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Medicine {
    /// Percentage points taken off the weeks left that care keeps.
    pub care_weeks_off_percent: u32,
    /// Price of care, in percent of the usual price.
    pub care_cost_percent: u32,
    /// Chance, in percent, that an injury that would leave an after-effect is only
    /// severe instead.
    pub after_effect_spared_percent: u32,
}

impl Default for Medicine {
    fn default() -> Self {
        Self {
            care_weeks_off_percent: 0,
            care_cost_percent: 100,
            after_effect_spared_percent: 0,
        }
    }
}

impl Medicine {
    /// `rules` as this doctor gives care: fewer weeks kept, at its price.
    #[must_use]
    pub fn care_rules(&self, rules: &CareRules) -> CareRules {
        let price = |cost| percent_of(cost, self.care_cost_percent);
        let injury = &rules.injury_cost;
        let illness = &rules.illness_cost;
        CareRules {
            injury_cost: BySeverity {
                light: price(injury.light),
                medium: price(injury.medium),
                severe: price(injury.severe),
            },
            illness_cost: ByLevel {
                mild: price(illness.mild),
                moderate: price(illness.moderate),
                severe: price(illness.severe),
            },
            weeks_kept_percent: rules
                .weeks_kept_percent
                .saturating_sub(self.care_weeks_off_percent),
        }
    }
}

/// What a recruiter shows of the fighters out of the club; nothing more without one.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Scouting {
    /// Stats revealed each week of observation, on top of `MarketRules::revealed_per_week`.
    pub revealed_per_week: u32,
    /// Width of the stat ranges, in percent of the usual width.
    pub range_width_percent: u32,
    /// Width of the estimate of the potential, in percent of it, once observed;
    /// 0 gives no estimate.
    pub potential_range_percent: u32,
    /// Whether the personality of a fighter observed is seen.
    pub personality: bool,
}

impl Default for Scouting {
    fn default() -> Self {
        Self {
            revealed_per_week: 0,
            range_width_percent: 100,
            potential_range_percent: 0,
            personality: false,
        }
    }
}

/// What the club's staff brings to a fighter's week.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct StaffEffects {
    pub coaching: Coaching,
    pub medicine: Medicine,
}

/// A post of the staff.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Role {
    Trainer,
    Doctor,
    Recruiter,
}

impl Role {
    pub const ALL: [Self; 3] = [Self::Trainer, Self::Doctor, Self::Recruiter];
}

/// The club's staff: each post's level, from 1, `None` when no one holds it.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Staff {
    pub trainer: Option<u32>,
    pub doctor: Option<u32>,
    pub recruiter: Option<u32>,
}

impl Staff {
    /// The level of whoever holds `role`.
    #[must_use]
    pub fn level(&self, role: Role) -> Option<u32> {
        match role {
            Role::Trainer => self.trainer,
            Role::Doctor => self.doctor,
            Role::Recruiter => self.recruiter,
        }
    }

    /// Who holds `role` now.
    pub fn set(&mut self, role: Role, level: Option<u32>) {
        match role {
            Role::Trainer => self.trainer = level,
            Role::Doctor => self.doctor = level,
            Role::Recruiter => self.recruiter = level,
        }
    }

    /// The trainer, `None` without one or with a level missing from `rules`.
    #[must_use]
    pub fn trainer<'a>(&self, rules: &'a StaffRules) -> Option<&'a TrainerRules> {
        rules.trainer(self.trainer?)
    }

    /// The doctor, `None` without one or with a level missing from `rules`.
    #[must_use]
    pub fn doctor<'a>(&self, rules: &'a StaffRules) -> Option<&'a DoctorRules> {
        rules.doctor(self.doctor?)
    }

    /// The recruiter, `None` without one or with a level missing from `rules`.
    #[must_use]
    pub fn recruiter<'a>(&self, rules: &'a StaffRules) -> Option<&'a RecruiterRules> {
        rules.recruiter(self.recruiter?)
    }

    /// What the trainer and the doctor bring to the club's fighters' weeks.
    #[must_use]
    pub fn effects(&self, rules: &StaffRules) -> StaffEffects {
        StaffEffects {
            coaching: self.trainer(rules).map(|t| t.coaching).unwrap_or_default(),
            medicine: self.doctor(rules).map(|d| d.medicine).unwrap_or_default(),
        }
    }

    /// The price of a training session: the trainer's, `without` a trainer.
    #[must_use]
    pub fn training_cost(&self, rules: &StaffRules, without: u32) -> u32 {
        self.trainer(rules).map_or(without, |t| t.training_cost)
    }

    /// Every post's weekly salary, in `Role::ALL` order, for the posts held.
    pub fn salaries(&self, rules: &StaffRules) -> impl Iterator<Item = u32> {
        Role::ALL
            .into_iter()
            .filter_map(|role| rules.salary(role, self.level(role)?))
    }
}

/// A change of staff, made before the week.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StaffMove {
    /// Hires someone of this level for the post, in place of whoever holds it.
    Hire(Role, u32),
    Dismiss(Role),
}

impl StaffMove {
    #[must_use]
    pub fn role(self) -> Role {
        match self {
            Self::Hire(role, _) | Self::Dismiss(role) => role,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn trainer(salary: u32) -> TrainerRules {
        TrainerRules {
            salary,
            training_cost: 5,
            coaching: Coaching::default(),
        }
    }

    #[test]
    fn levels_count_from_1() {
        let rules = StaffRules {
            trainers: vec![trainer(10), trainer(20)],
            doctors: Vec::new(),
            recruiters: Vec::new(),
        };
        assert_eq!(rules.trainer(0), None);
        assert_eq!(rules.salary(Role::Trainer, 1), Some(10));
        assert_eq!(rules.salary(Role::Trainer, 2), Some(20));
        assert_eq!(rules.salary(Role::Trainer, 3), None);
        assert_eq!(rules.salary(Role::Doctor, 1), None);
        assert_eq!(rules.levels(Role::Trainer), 2);
    }

    #[test]
    fn a_doctor_cares_better_at_its_price() {
        let rules = CareRules {
            injury_cost: BySeverity {
                light: 20,
                medium: 60,
                severe: 150,
            },
            illness_cost: ByLevel {
                mild: 15,
                moderate: 50,
                severe: 120,
            },
            weeks_kept_percent: 50,
        };
        assert_eq!(Medicine::default().care_rules(&rules), rules);
        let doctor = Medicine {
            care_weeks_off_percent: 20,
            care_cost_percent: 150,
            after_effect_spared_percent: 0,
        };
        let care = doctor.care_rules(&rules);
        assert_eq!(care.weeks_kept_percent, 30);
        assert_eq!(
            (care.injury_cost.light, care.illness_cost.severe),
            (30, 180)
        );
        assert_eq!(care.weeks_after_care(8), 3);
    }

    #[test]
    fn each_post_has_its_own_level() {
        let mut staff = Staff::default();
        staff.set(Role::Doctor, Some(3));
        assert_eq!(Role::ALL.map(|r| staff.level(r)), [None, Some(3), None]);
        assert_eq!(StaffMove::Hire(Role::Recruiter, 2).role(), Role::Recruiter);
    }

    #[test]
    fn the_staff_sets_salaries_effects_and_the_session_s_price() {
        let coaching = Coaching {
            progress_percent: 20,
            form_spared: 1,
        };
        let rules = StaffRules {
            trainers: vec![TrainerRules {
                salary: 10,
                training_cost: 7,
                coaching,
            }],
            doctors: Vec::new(),
            recruiters: Vec::new(),
        };
        let mut staff = Staff::default();
        assert_eq!(staff.training_cost(&rules, 5), 5);
        assert_eq!(staff.effects(&rules), StaffEffects::default());
        assert_eq!(staff.salaries(&rules).count(), 0);
        staff.trainer = Some(1);
        // A doctor of a level not on offer is neither paid nor heard.
        staff.doctor = Some(3);
        assert_eq!(staff.training_cost(&rules, 5), 7);
        assert_eq!(staff.effects(&rules).coaching, coaching);
        assert_eq!(staff.effects(&rules).medicine, Medicine::default());
        assert_eq!(staff.salaries(&rules).collect::<Vec<_>>(), [10]);
    }
}
