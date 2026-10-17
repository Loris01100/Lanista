//! The club's staff, paid every week: a trainer, a doctor and a recruiter, each from
//! level 1 to 5, a better one doing more for a higher salary. The trainer raises the
//! training gains and spares form, the doctor makes care shorten recoveries more and
//! spares after-effects, the recruiter shows more of the fighters out of the club.
//! They are hired among the candidates on the market, each with its own stats and
//! salary drawn around those of its level.

use rand::{Rng, RngExt};
use serde::{Deserialize, Serialize};

use crate::fighter::care::CareRules;
use crate::fighter::illness::ByLevel;
use crate::fighter::injury::BySeverity;
use crate::fighter::{percent_of, pick};

/// The staff levels, from `data/economy.ron`; each list starts at level 1. A candidate's
/// stats and salary are drawn around those of its level.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StaffRules {
    /// Each salary is paid once for every so many fighters in activity, at least once.
    pub fighters_per_salary: u32,
    pub trainers: Vec<TrainerRules>,
    pub doctors: Vec<DoctorRules>,
    pub recruiters: Vec<RecruiterRules>,
    pub market: StaffMarketRules,
}

/// How candidates come to the market and leave it.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StaffMarketRules {
    /// Candidates for each post at the start of a week; missing ones arrive at the
    /// end of the week.
    pub candidates: u32,
    /// Weeks a candidate stays on the market, drawn between the two, unless hired.
    pub min_weeks: u32,
    pub max_weeks: u32,
    /// Chance of each level for a new candidate, from level 1.
    pub level_weights: Vec<u32>,
    /// Each stat and the salary of a candidate are those of its level, give or take up
    /// to this percentage, each drawn on its own.
    pub spread_percent: u32,
}

/// Index of `level`, from 1, in a list of levels.
fn index(level: u32) -> Option<usize> {
    usize::try_from(level.checked_sub(1)?).ok()
}

impl StaffRules {
    /// How many times a club with `active` fighters in activity pays each salary: once
    /// for every `fighters_per_salary` of them, at least once.
    #[must_use]
    pub fn salary_shares(&self, active: usize) -> u32 {
        let per = usize::try_from(self.fighters_per_salary.max(1)).unwrap_or(usize::MAX);
        u32::try_from(active.div_ceil(per).max(1)).unwrap_or(u32::MAX)
    }

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
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TrainerRules {
    /// Paid every week, even into debt.
    pub salary: u32,
    /// Paid for each training session, instead of `EconomyRules::training_cost`.
    pub training_cost: u32,
    pub coaching: Coaching,
}

/// One doctor level.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DoctorRules {
    /// Paid every week, even into debt.
    pub salary: u32,
    pub medicine: Medicine,
}

/// One recruiter level.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RecruiterRules {
    /// Paid every week, even into debt.
    pub salary: u32,
    pub scouting: Scouting,
}

/// What a trainer brings to each training session; nothing without one.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Coaching {
    /// Added to each stat's chance to improve, in percent of that chance.
    pub progress_percent: u32,
    /// Form a session costs less, never below nothing.
    pub form_spared: u32,
}

/// What a doctor brings to care and injuries; nothing without one. A permanent
/// injury already there stays: no doctor heals a lost arm.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
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
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
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

/// `value` give or take up to `spread` percent, rounded to the nearest.
fn varied<R: Rng + ?Sized>(value: u32, spread: u32, rng: &mut R) -> u32 {
    let percent = rng.random_range(100u32.saturating_sub(spread)..=100 + spread);
    u32::try_from((u64::from(value) * u64::from(percent) + 50) / 100).unwrap_or(u32::MAX)
}

impl TrainerRules {
    /// A trainer of this level, each stat and the salary varied (`varied`).
    fn varied<R: Rng + ?Sized>(&self, spread: u32, rng: &mut R) -> Self {
        Self {
            salary: varied(self.salary, spread, rng),
            training_cost: varied(self.training_cost, spread, rng),
            coaching: Coaching {
                progress_percent: varied(self.coaching.progress_percent, spread, rng),
                form_spared: varied(self.coaching.form_spared, spread, rng),
            },
        }
    }
}

impl DoctorRules {
    /// A doctor of this level, each stat and the salary varied (`varied`).
    fn varied<R: Rng + ?Sized>(&self, spread: u32, rng: &mut R) -> Self {
        let medicine = &self.medicine;
        Self {
            salary: varied(self.salary, spread, rng),
            medicine: Medicine {
                care_weeks_off_percent: varied(medicine.care_weeks_off_percent, spread, rng),
                care_cost_percent: varied(medicine.care_cost_percent, spread, rng),
                after_effect_spared_percent: varied(
                    medicine.after_effect_spared_percent,
                    spread,
                    rng,
                )
                .min(100),
            },
        }
    }
}

impl RecruiterRules {
    /// A recruiter of this level, each stat and the salary varied (`varied`); it sees
    /// the personality as its level does.
    fn varied<R: Rng + ?Sized>(&self, spread: u32, rng: &mut R) -> Self {
        let scouting = &self.scouting;
        Self {
            salary: varied(self.salary, spread, rng),
            scouting: Scouting {
                revealed_per_week: varied(scouting.revealed_per_week, spread, rng),
                range_width_percent: varied(scouting.range_width_percent, spread, rng),
                potential_range_percent: varied(scouting.potential_range_percent, spread, rng),
                personality: scouting.personality,
            },
        }
    }
}

/// Someone on a staff or on the market: its level and its own stats and salary.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Member<R> {
    pub level: u32,
    pub rules: R,
}

/// Someone for one post.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Person {
    Trainer(Member<TrainerRules>),
    Doctor(Member<DoctorRules>),
    Recruiter(Member<RecruiterRules>),
}

impl Person {
    #[must_use]
    pub fn role(&self) -> Role {
        match self {
            Self::Trainer(_) => Role::Trainer,
            Self::Doctor(_) => Role::Doctor,
            Self::Recruiter(_) => Role::Recruiter,
        }
    }

    #[must_use]
    pub fn level(&self) -> u32 {
        match self {
            Self::Trainer(m) => m.level,
            Self::Doctor(m) => m.level,
            Self::Recruiter(m) => m.level,
        }
    }

    /// Paid every week once hired.
    #[must_use]
    pub fn salary(&self) -> u32 {
        match self {
            Self::Trainer(m) => m.rules.salary,
            Self::Doctor(m) => m.rules.salary,
            Self::Recruiter(m) => m.rules.salary,
        }
    }

    /// Someone for `role` exactly as `level` of `rules` is; `None` for a level not on
    /// offer.
    #[must_use]
    pub fn of_level(role: Role, level: u32, rules: &StaffRules) -> Option<Self> {
        Some(match role {
            Role::Trainer => Self::Trainer(Member {
                level,
                rules: rules.trainer(level)?.clone(),
            }),
            Role::Doctor => Self::Doctor(Member {
                level,
                rules: rules.doctor(level)?.clone(),
            }),
            Role::Recruiter => Self::Recruiter(Member {
                level,
                rules: rules.recruiter(level)?.clone(),
            }),
        })
    }
}

/// A candidate on the market, hired for its salary alone.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Candidate {
    pub person: Person,
    /// Weeks before it leaves the market, this one included.
    pub weeks_left: u32,
}

/// A new candidate for `role`: a level drawn by `StaffMarketRules::level_weights`
/// among those of `rules`, its stats and salary around the level's, and its weeks on
/// the market. `None` when `rules` has no level for `role`.
pub fn candidate<R: Rng + ?Sized>(
    role: Role,
    rules: &StaffRules,
    rng: &mut R,
) -> Option<Candidate> {
    let market = &rules.market;
    let levels: Vec<(u32, u32)> = (1..)
        .take(rules.levels(role))
        .zip(market.level_weights.iter().chain(std::iter::repeat(&0)))
        .map(|(level, &weight)| (weight, level))
        .collect();
    if levels.is_empty() {
        return None;
    }
    let level = pick(rng, &levels);
    let spread = market.spread_percent;
    let person = match role {
        Role::Trainer => Person::Trainer(Member {
            level,
            rules: rules.trainer(level)?.varied(spread, rng),
        }),
        Role::Doctor => Person::Doctor(Member {
            level,
            rules: rules.doctor(level)?.varied(spread, rng),
        }),
        Role::Recruiter => Person::Recruiter(Member {
            level,
            rules: rules.recruiter(level)?.varied(spread, rng),
        }),
    };
    let fewest = market.min_weeks.max(1);
    let weeks_left = rng.random_range(fewest..=market.max_weeks.max(fewest));
    Some(Candidate { person, weeks_left })
}

/// A club's staff: whoever holds each post, `None` when no one does.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Staff {
    pub trainer: Option<Member<TrainerRules>>,
    pub doctor: Option<Member<DoctorRules>>,
    pub recruiter: Option<Member<RecruiterRules>>,
}

impl Staff {
    /// A staff holding `posts`, each exactly as its level of `rules` is; a level not
    /// on offer leaves the post empty.
    #[must_use]
    pub fn of_levels(posts: &[(Role, u32)], rules: &StaffRules) -> Self {
        let mut staff = Self::default();
        for &(role, level) in posts {
            if let Some(person) = Person::of_level(role, level, rules) {
                staff.hire(person);
            }
        }
        staff
    }

    /// The level of whoever holds `role`.
    #[must_use]
    pub fn level(&self, role: Role) -> Option<u32> {
        match role {
            Role::Trainer => self.trainer.as_ref().map(|m| m.level),
            Role::Doctor => self.doctor.as_ref().map(|m| m.level),
            Role::Recruiter => self.recruiter.as_ref().map(|m| m.level),
        }
    }

    /// The weekly salary of whoever holds `role`.
    #[must_use]
    pub fn salary(&self, role: Role) -> Option<u32> {
        match role {
            Role::Trainer => self.trainer().map(|t| t.salary),
            Role::Doctor => self.doctor().map(|d| d.salary),
            Role::Recruiter => self.recruiter().map(|r| r.salary),
        }
    }

    /// `person` holds its post now, in place of whoever did.
    pub fn hire(&mut self, person: Person) {
        match person {
            Person::Trainer(m) => self.trainer = Some(m),
            Person::Doctor(m) => self.doctor = Some(m),
            Person::Recruiter(m) => self.recruiter = Some(m),
        }
    }

    /// No one holds `role` now.
    pub fn dismiss(&mut self, role: Role) {
        match role {
            Role::Trainer => self.trainer = None,
            Role::Doctor => self.doctor = None,
            Role::Recruiter => self.recruiter = None,
        }
    }

    #[must_use]
    pub fn trainer(&self) -> Option<&TrainerRules> {
        self.trainer.as_ref().map(|m| &m.rules)
    }

    #[must_use]
    pub fn doctor(&self) -> Option<&DoctorRules> {
        self.doctor.as_ref().map(|m| &m.rules)
    }

    #[must_use]
    pub fn recruiter(&self) -> Option<&RecruiterRules> {
        self.recruiter.as_ref().map(|m| &m.rules)
    }

    /// What the trainer and the doctor bring to the club's fighters' weeks.
    #[must_use]
    pub fn effects(&self) -> StaffEffects {
        StaffEffects {
            coaching: self.trainer().map(|t| t.coaching).unwrap_or_default(),
            medicine: self.doctor().map(|d| d.medicine).unwrap_or_default(),
        }
    }

    /// The price of a training session: the trainer's, `without` a trainer.
    #[must_use]
    pub fn training_cost(&self, without: u32) -> u32 {
        self.trainer().map_or(without, |t| t.training_cost)
    }

    /// Every post's weekly salary, in `Role::ALL` order, for the posts held.
    pub fn salaries(&self) -> impl Iterator<Item = u32> {
        Role::ALL.into_iter().filter_map(|role| self.salary(role))
    }
}

/// A change of staff, made before the week.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StaffMove {
    /// Hires the candidate at this place on the market, in place of whoever holds its
    /// post.
    Hire(usize),
    Dismiss(Role),
}

impl StaffMove {
    /// The post `self` changes; `None` for a candidate missing from `market`.
    #[must_use]
    pub fn role(self, market: &[Candidate]) -> Option<Role> {
        match self {
            Self::Hire(c) => market.get(c).map(|c| c.person.role()),
            Self::Dismiss(role) => Some(role),
        }
    }
}

/// A change of staff, once made.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StaffNews {
    /// Someone of this level hired for the post.
    Hired(Role, u32),
    Dismissed(Role),
}

/// Whether `m` may be made now by a club with `staff` and `money`: a candidate on
/// `market` hired with the money for a first salary, or a post held emptied.
#[must_use]
pub fn may_make(staff: &Staff, market: &[Candidate], money: i64, m: StaffMove) -> bool {
    match m {
        StaffMove::Hire(c) => market
            .get(c)
            .is_some_and(|c| i64::from(c.person.salary()) <= money),
        StaffMove::Dismiss(role) => staff.level(role).is_some(),
    }
}

/// Makes `moves` on `staff`, the first for each post only, each if `may_make` allows
/// it with `money`; the candidates hired leave `market`. Returns the changes made.
pub fn make_moves(
    staff: &mut Staff,
    market: &mut Vec<Candidate>,
    moves: &[StaffMove],
    money: i64,
) -> Vec<StaffNews> {
    let mut made = Vec::new();
    let mut seen = Vec::new();
    let mut hired = Vec::new();
    for &m in moves {
        let Some(role) = m.role(market) else {
            continue;
        };
        if seen.contains(&role) {
            continue;
        }
        seen.push(role);
        if !may_make(staff, market, money, m) {
            continue;
        }
        match m {
            StaffMove::Hire(c) => {
                let person = market[c].person.clone();
                made.push(StaffNews::Hired(role, person.level()));
                staff.hire(person);
                hired.push(c);
            }
            StaffMove::Dismiss(role) => {
                staff.dismiss(role);
                made.push(StaffNews::Dismissed(role));
            }
        }
    }
    hired.sort_unstable();
    for c in hired.into_iter().rev() {
        market.remove(c);
    }
    made
}

/// The market's end of week: each candidate has a week less, and those with none
/// left leave.
pub fn age_market(market: &mut Vec<Candidate>) {
    for candidate in market.iter_mut() {
        candidate.weeks_left = candidate.weeks_left.saturating_sub(1);
    }
    market.retain(|c| c.weeks_left > 0);
}

/// Brings each post's candidates on `market` up to `StaffMarketRules::candidates`.
pub fn fill_market<R: Rng + ?Sized>(market: &mut Vec<Candidate>, rules: &StaffRules, rng: &mut R) {
    let wanted = usize::try_from(rules.market.candidates).unwrap_or(usize::MAX);
    for role in Role::ALL {
        while market.iter().filter(|c| c.person.role() == role).count() < wanted {
            let Some(candidate) = candidate(role, rules, rng) else {
                break;
            };
            market.push(candidate);
        }
    }
}

#[cfg(test)]
mod tests {
    use rand::SeedableRng;
    use rand_chacha::ChaCha8Rng;

    use super::*;

    fn trainer(salary: u32) -> TrainerRules {
        TrainerRules {
            salary,
            training_cost: 5,
            coaching: Coaching::default(),
        }
    }

    fn market(candidates: u32, level_weights: Vec<u32>, spread_percent: u32) -> StaffMarketRules {
        StaffMarketRules {
            candidates,
            min_weeks: 2,
            max_weeks: 4,
            level_weights,
            spread_percent,
        }
    }

    fn trainers(trainers: Vec<TrainerRules>) -> StaffRules {
        StaffRules {
            fighters_per_salary: 3,
            trainers,
            doctors: Vec::new(),
            recruiters: Vec::new(),
            market: market(2, vec![1, 1], 20),
        }
    }

    #[test]
    fn a_salary_is_paid_once_for_every_three_fighters_at_least_once() {
        let rules = trainers(Vec::new());
        let shares: Vec<u32> = [0, 1, 3, 4, 10, 12]
            .into_iter()
            .map(|n| rules.salary_shares(n))
            .collect();
        assert_eq!(shares, [1, 1, 1, 2, 4, 4]);
    }

    fn hired_trainer(level: u32, rules: TrainerRules) -> Candidate {
        Candidate {
            person: Person::Trainer(Member { level, rules }),
            weeks_left: 1,
        }
    }

    #[test]
    fn levels_count_from_1() {
        let rules = trainers(vec![trainer(10), trainer(20)]);
        assert_eq!(rules.trainer(0), None);
        assert_eq!(rules.salary(Role::Trainer, 1), Some(10));
        assert_eq!(rules.salary(Role::Trainer, 2), Some(20));
        assert_eq!(rules.salary(Role::Trainer, 3), None);
        assert_eq!(rules.salary(Role::Doctor, 1), None);
        assert_eq!(rules.levels(Role::Trainer), 2);
        let second = Person::of_level(Role::Trainer, 2, &rules).unwrap();
        assert_eq!(
            (second.role(), second.level(), second.salary()),
            (Role::Trainer, 2, 20)
        );
        assert_eq!(Person::of_level(Role::Doctor, 1, &rules), None);
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
    fn the_staff_sets_salaries_effects_and_the_session_s_price() {
        let coaching = Coaching {
            progress_percent: 20,
            form_spared: 1,
        };
        let rules = TrainerRules {
            salary: 10,
            training_cost: 7,
            coaching,
        };
        let mut staff = Staff::default();
        assert_eq!(staff.training_cost(5), 5);
        assert_eq!(staff.effects(), StaffEffects::default());
        assert_eq!(staff.salaries().count(), 0);
        staff.hire(hired_trainer(1, rules).person);
        assert_eq!(staff.level(Role::Trainer), Some(1));
        assert_eq!(staff.training_cost(5), 7);
        assert_eq!(staff.effects().coaching, coaching);
        assert_eq!(staff.effects().medicine, Medicine::default());
        assert_eq!(staff.salaries().collect::<Vec<_>>(), [10]);
        staff.dismiss(Role::Trainer);
        assert_eq!(staff, Staff::default());
    }

    #[test]
    fn a_candidate_s_stats_and_salary_are_drawn_around_its_level_s() {
        let level = TrainerRules {
            salary: 100,
            training_cost: 10,
            coaching: Coaching {
                progress_percent: 50,
                form_spared: 1,
            },
        };
        // Only level 2 is drawn.
        let rules = StaffRules {
            market: market(2, vec![0, 1], 20),
            ..trainers(vec![trainer(10), level])
        };
        let mut rng = ChaCha8Rng::seed_from_u64(1);
        let mut salaries = Vec::new();
        for _ in 0..200 {
            let candidate = candidate(Role::Trainer, &rules, &mut rng).unwrap();
            assert!((2..=4).contains(&candidate.weeks_left));
            let Person::Trainer(m) = candidate.person else {
                panic!("not a trainer");
            };
            assert_eq!(m.level, 2);
            assert!((80..=120).contains(&m.rules.salary));
            assert!((8..=12).contains(&m.rules.training_cost));
            assert!((40..=60).contains(&m.rules.coaching.progress_percent));
            // Rounded to the nearest: 1 stays 1.
            assert_eq!(m.rules.coaching.form_spared, 1);
            salaries.push(m.rules.salary);
        }
        salaries.sort_unstable();
        salaries.dedup();
        assert!(salaries.len() > 20, "{salaries:?}");
        assert_eq!(candidate(Role::Doctor, &rules, &mut rng), None);
    }

    #[test]
    fn candidates_come_and_go() {
        let rules = trainers(vec![trainer(10), trainer(20)]);
        let mut rng = ChaCha8Rng::seed_from_u64(1);
        let mut market = Vec::new();
        fill_market(&mut market, &rules, &mut rng);
        // Two trainers, and no other post has levels.
        assert_eq!(market.len(), 2);
        let mut last = hired_trainer(1, trainer(10));
        last.weeks_left = 1;
        market[0].weeks_left = 3;
        market.push(last);
        age_market(&mut market);
        assert_eq!(market.len(), 2);
        assert_eq!(market[0].weeks_left, 2);
        fill_market(&mut market, &rules, &mut rng);
        assert_eq!(market.len(), 2);
    }

    #[test]
    fn moves_hire_from_the_market_one_per_post() {
        let mut market = vec![
            hired_trainer(1, trainer(10)),
            hired_trainer(2, trainer(20)),
            hired_trainer(3, trainer(30)),
        ];
        let mut staff = Staff::default();
        assert_eq!(StaffMove::Hire(2).role(&market), Some(Role::Trainer));
        assert_eq!(StaffMove::Hire(3).role(&market), None);
        // Without a doctor, none to dismiss; without the money for a first salary,
        // no one hired.
        assert!(!may_make(
            &staff,
            &market,
            100,
            StaffMove::Dismiss(Role::Doctor)
        ));
        assert!(!may_make(&staff, &market, 19, StaffMove::Hire(1)));
        assert!(may_make(&staff, &market, 20, StaffMove::Hire(1)));
        let moves = [
            StaffMove::Hire(5),
            StaffMove::Hire(1),
            StaffMove::Hire(2),
            StaffMove::Dismiss(Role::Doctor),
        ];
        let made = make_moves(&mut staff, &mut market, &moves, 20);
        assert_eq!(made, [StaffNews::Hired(Role::Trainer, 2)]);
        assert_eq!(staff.salaries().collect::<Vec<_>>(), [20]);
        assert_eq!(
            market.iter().map(|c| c.person.level()).collect::<Vec<_>>(),
            [1, 3]
        );
        let made = make_moves(
            &mut staff,
            &mut market,
            &[StaffMove::Dismiss(Role::Trainer)],
            0,
        );
        assert_eq!(made, [StaffNews::Dismissed(Role::Trainer)]);
        assert_eq!(staff, Staff::default());
    }
}
