//! The career fighter: everything about a fighter that lasts from week to week,
//! and one week of their life (care, rest, training or a fight, healing, ageing).

use rand::{Rng, RngExt};
use serde::{Deserialize, Serialize};

use crate::colosseum::ColosseumId;
use crate::combat::{Arena, Vec2};
use crate::design_data::{DesignData, FighterRules};
use crate::duel::{ARENA_SIZE, STATS, play};
use crate::fighter::fame::{Fame, FightOutcome, FightResult};
use crate::fighter::form::{Form, FormEvent};
use crate::fighter::illness::{Illness, IllnessLevel, roll_illness};
use crate::fighter::injury::{Injury, Severity, Side, heal_week, injured_stats, roll_injury};
use crate::fighter::morale::{Morale, MoraleEvent};
use crate::fighter::personality::Personality;
use crate::fighter::progression::{ProgressEvent, age_decline, progress, random_potential};
use crate::fighter::retirement::decides_to_retire;
use crate::fighter::{ClassId, Condition, Fighter, FighterId, Stats};
use crate::log::Report;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Status {
    Active,
    /// Decided to stop; only the fighter decides.
    Retired,
    Dead,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CareerFighter {
    pub class: ClassId,
    /// Trained stats, before injuries, illness and morale.
    pub stats: Stats,
    /// Hidden: the highest each stat can reach by progressing.
    pub potential: Stats,
    pub dominant_hand: Side,
    /// In years.
    pub age: u32,
    /// Weeks played since the last birthday; a year is a season of `data/calendar.ron`.
    // ponytail: own count, in step with the calendar for fighters there from week 1;
    // age everyone in the off-season once the market brings fighters mid-season.
    pub weeks_this_year: u32,
    pub personality: Personality,
    pub morale: Morale,
    pub fame: Fame,
    pub form: Form,
    pub injuries: Vec<Injury>,
    pub illness: Option<Illness>,
    pub status: Status,
}

impl CareerFighter {
    /// A new fighter: random class, stats within `rules.career.stats_spread_percent` of the
    /// standard duel's, hidden potential, dominant hand, age and personality; neutral morale,
    /// no fame, full form, healthy.
    ///
    /// # Panics
    /// If `data` has no class, which [`DesignData::from_ron`] rejects: a bug, not a data error.
    pub fn random<R: Rng + ?Sized>(rng: &mut R, data: &DesignData) -> Self {
        assert!(!data.classes.is_empty(), "no class in the design data");
        let rules = &data.fighter;
        let class_count = u32::try_from(data.classes.len()).unwrap_or(u32::MAX);
        let class = ClassId(rng.random_range(0..class_count));
        let spread = rules.career.stats_spread_percent.min(100);
        let stats = Stats::from_array(STATS.to_array().map(|value| {
            let percent = 100 - spread + rng.random_range(0..=2 * spread);
            crate::fighter::percent_of(value, percent).max(1)
        }));
        let potential = random_potential(rng, stats, &rules.progression);
        let dominant_hand = if rng.random_range(0..100) < rules.career.right_handed_percent {
            Side::Right
        } else {
            Side::Left
        };
        let (youngest, oldest) = rules.career.start_age;
        let age = rng.random_range(youngest..=oldest);
        Self {
            class,
            stats,
            potential,
            dominant_hand,
            age,
            weeks_this_year: 0,
            personality: Personality::random(rng),
            morale: Morale::neutral(),
            fame: Fame::default(),
            form: Form::full(),
            injuries: Vec::new(),
            illness: None,
            status: Status::Active,
        }
    }

    /// Stats in a fight: the trained ones lowered by injuries and illness,
    /// then with morale's bonus or malus.
    #[must_use]
    pub fn fighting_stats(&self, rules: &FighterRules) -> Stats {
        let mut stats = injured_stats(
            self.stats,
            &self.injuries,
            self.dominant_hand,
            &rules.injury,
        );
        if let Some(illness) = &self.illness {
            stats = illness.ill_stats(stats, &rules.illness);
        }
        self.morale.combat_stats(stats, &rules.morale)
    }

    #[must_use]
    pub fn permanent_injuries(&self) -> u32 {
        let count = self.injuries.iter().filter(|i| i.is_permanent()).count();
        u32::try_from(count).unwrap_or(u32::MAX)
    }

    /// This fighter ready to fight in an arena, as fighter `id` at `position`.
    ///
    /// # Panics
    /// If its class is not in `data`.
    #[must_use]
    pub fn combat_fighter(&self, id: FighterId, data: &DesignData, position: Vec2) -> Fighter {
        let class_data = data.class(self.class).expect("a class of the design data");
        let rules = &data.fighter;
        let stats = self.fighting_stats(rules);
        let mut fighter = Fighter::new(id, self.class, class_data, stats, position);
        fighter.surrender_hp = self.personality.surrender_hp(stats.hp, &rules.personality);
        fighter.condition = Condition {
            personality: self.personality,
            morale: self.morale,
            form: self.form,
            injuries: self.injuries.clone(),
            illness: self.illness,
        };
        fighter
    }

    /// Plays one week following `plan`; `None`, and nothing changes, if the fighter
    /// is retired or dead. In order: care, the week's activity, then healing,
    /// the illness draw and, once a year, ageing and the retirement decision.
    /// A fight is against `opponent`, or a new random fighter without one.
    pub fn play_week<R: Rng + ?Sized>(
        &mut self,
        plan: WeekPlan,
        opponent: Option<&mut Self>,
        rng: &mut R,
        data: &DesignData,
    ) -> Option<WeekReport> {
        if self.status != Status::Active {
            return None;
        }
        let rules = &data.fighter;
        let mut report = WeekReport {
            plan,
            care_cost: if plan.care { self.care(rules) } else { 0 },
            fight: None,
            new_injury: None,
            healed_injuries: Vec::new(),
            new_illness: None,
            illness_healed: false,
            birthday: false,
        };
        match plan.activity {
            Activity::Rest => {
                self.form = match &self.illness {
                    Some(illness) => illness.rest(self.form, &rules.illness, &rules.form),
                    None => self.form.after(FormEvent::Rest, &rules.form),
                };
            }
            Activity::Train => {
                self.form = self.form.after(FormEvent::Training, &rules.form);
                self.progress(ProgressEvent::Training, rng, rules);
            }
            Activity::Fight(_) | Activity::TitleFight(_) | Activity::Challenge(..) => {
                let form = if matches!(plan.activity, Activity::TitleFight(_)) {
                    FormEvent::TitleDefense
                } else {
                    FormEvent::Fight
                };
                let mut stranger;
                let opponent = if let Some(opponent) = opponent {
                    opponent
                } else {
                    stranger = Self::random(rng, data);
                    &mut stranger
                };
                let (fight, injury) = self.fight(opponent, form, rng, data);
                report.fight = Some(fight);
                report.new_injury = injury;
                if self.status == Status::Dead {
                    return Some(report);
                }
            }
        }
        self.end_week(&mut report, rng, data);
        Some(report)
    }

    /// Price of caring for every injury and illness where care helps, without caring.
    #[must_use]
    pub fn care_cost(&self, rules: &FighterRules) -> u32 {
        let injuries = self
            .injuries
            .iter()
            .filter_map(|i| i.care_cost(&rules.care));
        let illness = self.illness.and_then(|i| i.care_cost(&rules.care));
        injuries.chain(illness).sum()
    }

    /// Cares for every injury and illness where care helps; returns the total price.
    fn care(&mut self, rules: &FighterRules) -> u32 {
        let mut cost = 0_u32;
        for injury in &mut self.injuries {
            cost = cost.saturating_add(injury.treat(&rules.care).unwrap_or(0));
        }
        if let Some(illness) = &mut self.illness {
            cost = cost.saturating_add(illness.treat(&rules.care).unwrap_or(0));
        }
        cost
    }

    fn progress<R: Rng + ?Sized>(
        &mut self,
        event: ProgressEvent,
        rng: &mut R,
        rules: &FighterRules,
    ) {
        self.stats = progress(
            rng,
            self.stats,
            self.potential,
            event,
            self.age,
            self.morale,
            &rules.progression,
        );
    }

    /// A duel between this fighter, `FighterId(0)`, and `opponent`, `FighterId(1)`,
    /// played to its end: its seed, and the arena before its first tick and after its last.
    pub fn duel<R: Rng + ?Sized>(
        &self,
        opponent: &Self,
        rng: &mut R,
        data: &DesignData,
    ) -> (u64, Arena, Arena) {
        let seed = rng.random::<u64>();
        let middle = ARENA_SIZE / 2.0;
        let fighters = vec![
            self.combat_fighter(FighterId(0), data, Vec2::new(3.0, middle)),
            opponent.combat_fighter(FighterId(1), data, Vec2::new(ARENA_SIZE - 3.0, middle)),
        ];
        let start = Arena::new(ARENA_SIZE, ARENA_SIZE, fighters, seed);
        let mut arena = start.clone();
        play(&mut arena);
        (seed, start, arena)
    }

    /// Whether this fighter's fighting stats add up to more than `other`'s.
    #[must_use]
    pub fn stronger_than(&self, other: &Self, rules: &FighterRules) -> bool {
        let total = |f: &Self| {
            let stats = f.fighting_stats(rules).to_array();
            stats.iter().map(|&v| u64::from(v)).sum::<u64>()
        };
        total(self) > total(other)
    }

    /// The fame and death the finished fight in `arena`, where this fighter was `id`,
    /// brings: all a resident takes from a fight. Returns the outcome and the fame change.
    pub fn take_fame_and_death(
        &mut self,
        arena: &Arena,
        id: FighterId,
        against_stronger: bool,
        rules: &FighterRules,
    ) -> (FightOutcome, i32) {
        let outcome = FightOutcome::from_arena(arena, id, against_stronger, &rules.fame);
        let fame_change = outcome.fame_change(&rules.fame);
        self.fame = self.fame.changed_by(fame_change);
        if arena.fighter(id).is_some_and(|f| !f.is_alive()) {
            self.status = Status::Dead;
        }
        (outcome, fame_change)
    }

    /// A duel against `opponent`, then its effects: fame, morale, form (`form`'s cost),
    /// illness worsening, a possible injury and progression. The opponent only takes
    /// fame and death. Returns the new injury.
    fn fight<R: Rng + ?Sized>(
        &mut self,
        opponent: &mut Self,
        form: FormEvent,
        rng: &mut R,
        data: &DesignData,
    ) -> (FightSummary, Option<Injury>) {
        let rules = &data.fighter;
        let (seed, start, arena) = self.duel(opponent, rng, data);
        let stronger = opponent.stronger_than(self, rules);
        let weaker = self.stronger_than(opponent, rules);
        let summary_opponent = opponent.clone();
        opponent.take_fame_and_death(&arena, FighterId(1), weaker, rules);
        let (outcome, fame_change) =
            self.take_fame_and_death(&arena, FighterId(0), stronger, rules);
        let died = self.status == Status::Dead;
        let summary = FightSummary {
            seed,
            start,
            report: Report::new(&arena),
            opponent: summary_opponent,
            against_stronger: stronger,
            result: outcome.result,
            died,
            fame_change,
            ticks: arena.tick,
        };
        if died {
            return (summary, None);
        }
        match outcome.result {
            FightResult::Won => {
                self.morale = self.morale.after(MoraleEvent::Victory, &rules.morale);
            }
            FightResult::Lost | FightResult::GaveUp => {
                self.morale = self.morale.after(MoraleEvent::Defeat, &rules.morale);
            }
            FightResult::Unfinished => {}
        }
        if let Ok(gained @ 1..) = u32::try_from(fame_change) {
            self.morale = self
                .morale
                .after(MoraleEvent::FameGained(gained), &rules.morale);
        }
        // The risk follows the form the fighter went in with.
        let injury = roll_injury(rng, self.form, &rules.injury, &rules.form);
        if let Some(injury) = injury {
            self.injuries.push(injury);
            self.morale = self.morale.after(MoraleEvent::Injury, &rules.morale);
        }
        self.form = self.form.after(form, &rules.form);
        self.illness = self
            .illness
            .map(|illness| illness.after_fight(rng, &rules.illness));
        self.progress(ProgressEvent::Fight, rng, rules);
        (summary, injury)
    }

    /// Healing, the illness draw for a healthy fighter, and the yearly birthday.
    fn end_week<R: Rng + ?Sized>(
        &mut self,
        report: &mut WeekReport,
        rng: &mut R,
        data: &DesignData,
    ) {
        let rules = &data.fighter;
        report.healed_injuries = self
            .injuries
            .iter()
            .filter(|i| !i.is_permanent() && i.weeks_left <= 1)
            .copied()
            .collect();
        heal_week(&mut self.injuries);
        if let Some(illness) = self.illness {
            self.illness = illness.heal_week();
            report.illness_healed = self.illness.is_none();
        } else {
            self.illness = roll_illness(rng, self.form, &rules.illness, &rules.form);
            report.new_illness = self.illness;
            if self.illness.is_some() {
                self.morale = self.morale.after(MoraleEvent::Illness, &rules.morale);
            }
        }
        self.weeks_this_year += 1;
        if self.weeks_this_year >= data.calendar.season_weeks() {
            self.weeks_this_year = 0;
            self.age += 1;
            report.birthday = true;
            self.stats = age_decline(self.stats, self.age, &rules.progression);
            let permanent = self.permanent_injuries();
            if decides_to_retire(
                rng,
                self.age,
                &self.personality,
                permanent,
                &rules.retirement,
            ) {
                self.status = Status::Retired;
            }
        }
    }
}

/// What a fighter does in a week.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Activity {
    Rest,
    Train,
    /// A duel in this colosseum.
    Fight(ColosseumId),
    /// A duel for this colosseum's title, as its champion or as a challenger from
    /// the top of its ranking; costs the form of a title defense.
    TitleFight(ColosseumId),
    /// A duel in this colosseum against the resident at this index, who challenged the
    /// fighter or accepts its challenge.
    Challenge(ColosseumId, usize),
}

/// Below this form, `suggested_plan` rests.
pub const REST_BELOW_FORM: u32 = 50;

/// A simple stand-in for the player's decisions, shared by the `career` tool and the
/// game's default choice: care for anything medium or worse, rest when tired, badly
/// hurt or ill, otherwise fight on odd weeks, in the first colosseum (free to enter),
/// and train on even ones.
// ponytail: the rival managers' rules of phase 4 will replace it.
#[must_use]
pub fn suggested_plan(fighter: &CareerFighter, week: u64) -> WeekPlan {
    let badly_hurt = fighter
        .injuries
        .iter()
        .any(|i| matches!(i.severity, Severity::Medium | Severity::Severe));
    let ill = fighter
        .illness
        .is_some_and(|i| i.level != IllnessLevel::Mild);
    let activity = if badly_hurt || ill || fighter.form.value() < REST_BELOW_FORM {
        Activity::Rest
    } else if week % 2 == 1 {
        Activity::Fight(ColosseumId(0))
    } else {
        Activity::Train
    };
    WeekPlan {
        activity,
        care: badly_hurt || ill,
    }
}

/// The player's decisions for one fighter and one week.
// ponytail: care is all or nothing here; choosing injury by injury comes with
// the fighter screen in phase 4.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WeekPlan {
    pub activity: Activity,
    /// Care for every injury and illness where it helps, before the activity.
    pub care: bool,
}

/// What happened in a week.
#[derive(Debug, Clone, PartialEq)]
pub struct WeekReport {
    pub plan: WeekPlan,
    /// Price of the care given; `state::next_week` takes it from the club's money.
    pub care_cost: u32,
    pub fight: Option<FightSummary>,
    pub new_injury: Option<Injury>,
    pub healed_injuries: Vec<Injury>,
    pub new_illness: Option<Illness>,
    pub illness_healed: bool,
    /// A year went by: one year older, decline from 40, retirement decision.
    pub birthday: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct FightSummary {
    /// Replaying the duel with this seed gives the same fight.
    pub seed: u64,
    /// The fight before its first tick, the career fighter as `FighterId(0)`:
    /// playing it with utility AIs gives this very fight, to watch it.
    pub start: Arena,
    /// Damage dealt and taken and spells cast by each side, the career fighter first.
    pub report: Report,
    /// The opponent before the fight.
    pub opponent: CareerFighter,
    pub against_stronger: bool,
    pub result: FightResult,
    pub died: bool,
    pub fame_change: i32,
    pub ticks: u32,
}

/// Starting values of generated fighters, from `data/fighter.ron`.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CareerRules {
    /// Youngest and oldest age of a generated fighter.
    pub start_age: (u32, u32),
    /// A generated fighter's stats sit within this percentage of the standard duel's.
    pub stats_spread_percent: u32,
    /// Chance, in percent, that a generated fighter is right-handed.
    pub right_handed_percent: u32,
}

#[cfg(test)]
mod tests {
    use super::*;
    use rand::SeedableRng;
    use rand_chacha::ChaCha8Rng;

    fn data() -> DesignData {
        DesignData::load().unwrap()
    }

    fn fighter(seed: u64) -> CareerFighter {
        CareerFighter::random(&mut ChaCha8Rng::seed_from_u64(seed), &data())
    }

    fn plan(activity: Activity) -> WeekPlan {
        WeekPlan {
            activity,
            care: false,
        }
    }

    #[test]
    fn generated_fighters_are_reproducible_and_within_rules() {
        assert_eq!(fighter(7), fighter(7));
        assert_ne!(fighter(7), fighter(8));
        let f = fighter(7);
        assert!((20..=30).contains(&f.age));
        for ((stat, standard), cap) in f
            .stats
            .to_array()
            .into_iter()
            .zip(STATS.to_array())
            .zip(f.potential.to_array())
        {
            assert!(stat * 100 >= standard * 80 && stat * 100 <= standard * 120);
            assert!(cap >= stat);
        }
    }

    #[test]
    fn rest_restores_form_and_training_costs_some() {
        let data = data();
        let mut rng = ChaCha8Rng::seed_from_u64(0);
        let mut f = fighter(7);
        f.form = Form::new(50);
        f.play_week(plan(Activity::Rest), None, &mut rng, &data)
            .unwrap();
        assert_eq!(f.form.value(), 65);
        f.play_week(plan(Activity::Train), None, &mut rng, &data)
            .unwrap();
        assert_eq!(f.form.value(), 60);
        assert_eq!(f.weeks_this_year, 2);
    }

    #[test]
    fn a_fight_costs_form_and_moves_fame() {
        let data = data();
        let mut rng = ChaCha8Rng::seed_from_u64(1);
        let mut f = fighter(7);
        let report = f
            .play_week(plan(Activity::Fight(ColosseumId(0))), None, &mut rng, &data)
            .unwrap();
        let fight = report.fight.unwrap();
        if f.status == Status::Active {
            assert_eq!(f.form.value(), 80);
        }
        assert_eq!(f.fame, Fame::default().changed_by(fight.fame_change));
    }

    #[test]
    fn injuries_heal_and_are_reported() {
        let data = data();
        let mut rng = ChaCha8Rng::seed_from_u64(2);
        let mut f = fighter(7);
        let injury = Injury {
            location: crate::fighter::injury::Location::Head,
            severity: crate::fighter::injury::Severity::Light,
            weeks_left: 1,
        };
        f.injuries.push(injury);
        let report = f
            .play_week(plan(Activity::Rest), None, &mut rng, &data)
            .unwrap();
        assert_eq!(report.healed_injuries, vec![injury]);
        assert_eq!(f.injuries, []);
    }

    #[test]
    fn injuries_illness_and_morale_shape_the_fighting_stats() {
        let rules = data().fighter;
        let mut f = fighter(7);
        let healthy = f.fighting_stats(&rules);
        assert_eq!(healthy, f.stats);
        f.injuries.push(Injury {
            location: crate::fighter::injury::Location::Leg(Side::Left),
            severity: crate::fighter::injury::Severity::Medium,
            weeks_left: 3,
        });
        assert!(f.fighting_stats(&rules).speed < healthy.speed);
        let fighter = f.combat_fighter(FighterId(0), &data(), Vec2::new(3.0, 10.0));
        assert_eq!(fighter.stats, f.fighting_stats(&rules));
        assert_eq!(fighter.condition.injuries, f.injuries);
    }

    #[test]
    fn a_year_ages_the_fighter() {
        let data = data();
        let mut rng = ChaCha8Rng::seed_from_u64(3);
        let mut f = fighter(7);
        let age = f.age;
        let mut birthdays = 0;
        for _ in 0..data.calendar.season_weeks() {
            let report = f
                .play_week(plan(Activity::Rest), None, &mut rng, &data)
                .unwrap();
            birthdays += u32::from(report.birthday);
        }
        assert_eq!((birthdays, f.age, f.weeks_this_year), (1, age + 1, 0));
    }

    #[test]
    fn retired_and_dead_fighters_play_no_more_weeks() {
        let data = data();
        let mut rng = ChaCha8Rng::seed_from_u64(4);
        for status in [Status::Retired, Status::Dead] {
            let mut f = fighter(7);
            f.status = status;
            let before = f.clone();
            assert_eq!(
                f.play_week(plan(Activity::Fight(ColosseumId(0))), None, &mut rng, &data),
                None
            );
            assert_eq!(f, before);
        }
    }

    #[test]
    fn the_start_of_a_fight_replays_it() {
        let data = data();
        let mut rng = ChaCha8Rng::seed_from_u64(5);
        let mut f = fighter(7);
        let report = f
            .play_week(plan(Activity::Fight(ColosseumId(0))), None, &mut rng, &data)
            .unwrap();
        let fight = report.fight.unwrap();
        let mut replay = fight.start.clone();
        assert_eq!(replay.tick, 0);
        play(&mut replay);
        assert_eq!(replay.tick, fight.ticks);
        assert_eq!(Report::new(&replay), fight.report);
        assert_eq!(!replay.fighters[0].is_alive(), fight.died);
    }

    #[test]
    fn the_opponent_given_is_the_one_met_and_can_die() {
        let data = data();
        let fight = Activity::Fight(ColosseumId(0));
        for seed in 0..20 {
            let mut rng = ChaCha8Rng::seed_from_u64(seed);
            let mut f = fighter(7);
            let mut opponent = fighter(seed + 100);
            let report = f
                .play_week(plan(fight), Some(&mut opponent), &mut rng, &data)
                .unwrap();
            let summary = report.fight.unwrap();
            assert_eq!(summary.opponent, fighter(seed + 100));
            let mut replay = summary.start.clone();
            play(&mut replay);
            let died = !replay.fighters[1].is_alive();
            assert_eq!(opponent.status == Status::Dead, died, "seed {seed}");
        }
    }

    #[test]
    fn the_suggested_plan_rests_the_tired_and_the_hurt() {
        let mut f = fighter(7);
        assert_eq!(
            suggested_plan(&f, 1).activity,
            Activity::Fight(ColosseumId(0))
        );
        assert_eq!(suggested_plan(&f, 2).activity, Activity::Train);
        f.form = Form::new(REST_BELOW_FORM - 1);
        assert_eq!(suggested_plan(&f, 1).activity, Activity::Rest);
        f.form = Form::full();
        f.injuries.push(Injury {
            location: crate::fighter::injury::Location::Body,
            severity: Severity::Medium,
            weeks_left: 3,
        });
        assert_eq!(
            suggested_plan(&f, 1),
            WeekPlan {
                activity: Activity::Rest,
                care: true
            }
        );
    }

    #[test]
    fn same_seed_same_career() {
        let career = || {
            let data = data();
            let mut rng = ChaCha8Rng::seed_from_u64(9);
            let mut f = CareerFighter::random(&mut rng, &data);
            let reports: Vec<_> = (0..10)
                .filter_map(|_| {
                    f.play_week(plan(Activity::Fight(ColosseumId(0))), None, &mut rng, &data)
                })
                .collect();
            (f, reports)
        };
        assert_eq!(career(), career());
    }
}
