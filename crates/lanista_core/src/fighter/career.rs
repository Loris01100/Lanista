//! The career fighter: everything about a fighter that lasts from week to week,
//! and one week of their life (care, rest, training or a fight, healing, ageing).

use rand::{Rng, RngExt};
use serde::{Deserialize, Serialize};

use crate::colosseum::{ColosseumId, Contender};
use crate::combat::{Arena, ArenaKind, Vec2};
use crate::design_data::{DesignData, FighterRules};
use crate::duel::{STATS, play};
use crate::fighter::appearance::Appearance;
use crate::fighter::care::CareRules;
use crate::fighter::equipment::Refused;
use crate::fighter::fame::{Fame, FightOutcome, FightResult};
use crate::fighter::form::{Form, FormEvent};
use crate::fighter::illness::{Illness, IllnessLevel, roll_illness};
use crate::fighter::injury::{
    Injury, Location, Severity, Side, bleeding_wound, fatal_blow_wound, has_lost, heal_week,
    injured_stats, roll_injury, severed_limb_wound, spare_after_effect, stun_wound,
};
use crate::fighter::melee::{Entrant, melee_start};
use crate::fighter::morale::{Morale, MoraleEvent};
use crate::fighter::personality::{Personality, Temperament, TemperamentRules};
use crate::fighter::progression::{
    ProgressEvent, age_decline, progress, random_potential, season_gains, season_progress,
};
use crate::fighter::retirement::{decides_to_retire, too_maimed};
use crate::fighter::traits::{Trait, random_traits};
use crate::fighter::{ClassId, Condition, Fighter, FighterId, Stats, percent_of};
use crate::log::{Report, places};
use crate::names::FighterName;
use crate::rivals::RivalId;
use crate::staff::{Medicine, StaffEffects};
use crate::stat_model::Ending;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Status {
    Active,
    /// Stopped: decided by the fighter only, or forced by too many limbs lost
    /// (`retirement::too_maimed`).
    Retired,
    Dead,
    /// Sold to another club: the fighter goes on there, and this entry is its past here.
    Sold,
    /// Let go by its club: a free agent now.
    Released,
}

/// Fights won, lost (given up included) and drawn.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Record {
    pub wins: u32,
    pub losses: u32,
    pub draws: u32,
}

impl Record {
    fn add(&mut self, result: FightResult) {
        match result {
            FightResult::Won => self.wins += 1,
            FightResult::Lost | FightResult::GaveUp => self.losses += 1,
            FightResult::Unfinished => self.draws += 1,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CareerFighter {
    pub class: ClassId,
    /// Its stage name, given as it comes into the world (`GameState::named`); empty
    /// for a fighter out of a game.
    #[serde(default)]
    pub name: FighterName,
    /// The key of its culture in `data/names.ron`, which named it: its continent is
    /// the culture's.
    #[serde(default)]
    pub culture: String,
    /// Trained stats, before injuries, illness and morale.
    pub stats: Stats,
    /// Hidden: the highest each stat can reach by progressing.
    pub potential: Stats,
    pub dominant_hand: Side,
    /// Strikes as well with both hands: no malus with two weapons.
    #[serde(default)]
    pub ambidextrous: bool,
    /// Its loadout, an index of its class's `loadouts`: the last one its club chose
    /// (`WeekPlan::loadout`).
    #[serde(default)]
    pub loadout: usize,
    /// Its ease with each loadout of its class, from 0 to 100 (`EaseRules`): drawn as a
    /// recruit, raised by each fight in it.
    #[serde(default)]
    pub ease: Vec<u32>,
    /// In years.
    pub age: u32,
    pub personality: Personality,
    /// Its traits, 0 to 2, for life.
    #[serde(default)]
    pub traits: Vec<Trait>,
    pub morale: Morale,
    pub fame: Fame,
    pub form: Form,
    pub injuries: Vec<Injury>,
    pub illness: Option<Illness>,
    pub status: Status,
    #[serde(default)]
    pub record: Record,
    /// The record since the last birthday, the start of the last off-season: worth
    /// stats at the next one.
    #[serde(default)]
    pub season_record: Record,
    /// Weeks the player's club observed it: the more, the more of its stats are seen,
    /// and the narrower (`market::stat_ranges`).
    #[serde(default)]
    pub observed_weeks: u32,
    /// How it looks, for life.
    pub appearance: Appearance,
    /// The week it joined its club by a signing or a sale; `None` for one there from
    /// the start. It may not be sold before `GameState::sellable_from`.
    pub joined: Option<u64>,
    /// Where it went, for the entry a rival club's fighter or a free agent left behind:
    /// the fighter lives on there, and this entry keeps only its class and status
    /// (`CareerFighter::moved`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub moved_to: Option<Contender>,
}

impl CareerFighter {
    /// A new fighter: random class, stats within `rules.career.stats_spread_percent` of the
    /// standard duel's, hidden potential, dominant hand, age, personality, look and ease
    /// with each loadout, in the one it is most at ease with;
    /// neutral morale, no fame, full form, healthy.
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
        let hand = rng.random_range(0..100);
        let ambidextrous = hand < rules.career.ambidextrous_percent;
        let dominant_hand = if hand < rules.career.right_handed_percent {
            Side::Right
        } else {
            Side::Left
        };
        let (youngest, oldest) = rules.career.start_age;
        let age = rng.random_range(youngest..=oldest);
        let personality = Personality::random(rng, &rules.personality);
        let traits = random_traits(rng, &rules.traits);
        let appearance = Appearance::random(rng, &rules.appearance);
        let loadouts = data.class(class).map_or(0, |c| c.loadouts.len());
        let ease = rules.ease.random(rng, loadouts);
        let mut fighter = Self {
            class,
            name: FighterName::default(),
            culture: String::new(),
            stats,
            potential,
            dominant_hand,
            ambidextrous,
            loadout: 0,
            ease,
            age,
            personality,
            traits,
            morale: Morale::neutral(),
            fame: Fame::default(),
            form: Form::full(),
            injuries: Vec::new(),
            illness: None,
            status: Status::Active,
            record: Record::default(),
            season_record: Record::default(),
            observed_weeks: 0,
            appearance,
            joined: None,
            moved_to: None,
        };
        fighter.loadout = fighter.preferred_loadout();
        fighter
    }

    /// The loadout its club picks for it: the one it is most at ease with, the first
    /// of them on a tie.
    #[must_use]
    pub fn preferred_loadout(&self) -> usize {
        let best = self.ease.iter().max().copied().unwrap_or(0);
        self.ease.iter().position(|&e| e == best).unwrap_or(0)
    }

    /// Its ease with `loadout`; full for a loadout it has none for.
    #[must_use]
    pub fn ease_with(&self, loadout: usize) -> u32 {
        self.ease.get(loadout).copied().unwrap_or(100)
    }

    /// The entry it leaves behind on going to `to`: its class, name and culture, and
    /// its status; the rest is blank, the fighter living on at `to`.
    #[must_use]
    pub fn moved(&self, to: Contender) -> Self {
        Self {
            name: self.name.clone(),
            culture: self.culture.clone(),
            ..Self::moved_entry(self.class, self.status, to)
        }
    }

    fn moved_entry(class: ClassId, status: Status, to: Contender) -> Self {
        Self {
            class,
            name: FighterName::default(),
            culture: String::new(),
            stats: Stats::default(),
            potential: Stats::default(),
            dominant_hand: Side::Right,
            ambidextrous: false,
            loadout: 0,
            ease: Vec::new(),
            age: 0,
            personality: Personality::neutral(),
            traits: Vec::new(),
            morale: Morale::neutral(),
            fame: Fame::default(),
            form: Form::full(),
            injuries: Vec::new(),
            illness: None,
            status,
            record: Record::default(),
            season_record: Record::default(),
            observed_weeks: 0,
            appearance: Appearance::default(),
            joined: None,
            moved_to: Some(to),
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

    /// This fighter ready to fight in an arena, as fighter `id` at `position`, in its
    /// loadout, with what its off hand brings, and without the `refused` pieces of
    /// equipment: their maluses on its stats, its cutting hits blunt without weapon.
    ///
    /// # Panics
    /// If its class is not in `data`.
    #[must_use]
    pub fn combat_fighter(
        &self,
        id: FighterId,
        refused: Refused,
        data: &DesignData,
        position: Vec2,
    ) -> Fighter {
        let class_data = data.class(self.class).expect("a class of the design data");
        let rules = &data.fighter;
        let equipment = &rules.equipment;
        let mut stats = self.fighting_stats(rules);
        if let Some(loadout) = class_data.loadout(self.loadout) {
            stats = loadout.off_hand.stats(stats, self.ambidextrous, equipment);
        }
        let stats = rules.ease.stats(stats, self.ease_with(self.loadout));
        let stats = refused.stats(stats, equipment);
        let class = (class_data, self.loadout);
        let mut fighter = Fighter::in_loadout(id, self.class, class, stats, position);
        fighter.surrender_hp = self.personality.surrender_hp(stats.hp, &rules.personality);
        fighter.appearance = self.appearance.clone();
        fighter.refused = refused;
        let models = usize::try_from(self.class.0)
            .ok()
            .and_then(|c| data.models.get(c));
        let record = self.record;
        fighter.model = models.and_then(|m| {
            if record.wins + record.losses + record.draws >= rules.career.veteran_fights {
                m.veteran.clone()
            } else {
                m.recruit.clone()
            }
        });
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
    /// is retired or dead. In order: care, the week's activity, then healing and
    /// the illness draw; ageing is the off-season's ([`CareerFighter::birthday`]).
    /// A fight is against `opponent`, or a new random fighter without one, in the
    /// colosseum's arena (see [`WorldRules::arena`](crate::colosseum::WorldRules::arena)). The club's
    /// `staff` helps: training gains more and costs less form with the trainer's
    /// coaching, care and injuries follow the doctor's medicine. A melee, which the
    /// world plays with everyone in it ([`CareerFighter::begin_melee_week`]), is a rest
    /// here.
    pub fn play_week<R: Rng + ?Sized>(
        &mut self,
        plan: WeekPlan,
        opponent: Option<(&mut Self, Medicine)>,
        staff: StaffEffects,
        rng: &mut R,
        data: &DesignData,
    ) -> Option<WeekReport> {
        let mut report = self.begin_melee_week(plan, staff, data)?;
        let rules = &data.fighter;
        match plan.activity {
            Activity::Rest | Activity::Melee(_) => {
                self.form = match &self.illness {
                    Some(illness) => illness.rest(self.form, &rules.illness, &rules.form),
                    None => self.form.after(FormEvent::Rest, &rules.form),
                };
                if plan.activity == Activity::Rest {
                    self.take_rest(rules);
                }
            }
            Activity::Train => {
                let coaching = staff.coaching;
                let mut lost = rules.form.training.saturating_sub(coaching.form_spared);
                let mut coaching_percent = coaching.progress_percent;
                let traits = &rules.traits;
                if self.has(Trait::Workaholic) {
                    coaching_percent += traits.workaholic.training_percent;
                }
                if self.has(Trait::Nonchalant) {
                    lost = percent_of(lost, traits.nonchalant.training_form_percent);
                    self.feel(
                        MoraleEvent::Lowered(traits.nonchalant.training_morale),
                        rules,
                    );
                }
                self.form = Form::new(self.form.value().saturating_sub(lost));
                self.progress(ProgressEvent::Training { coaching_percent }, rng, rules);
            }
            Activity::Fight(id) | Activity::TitleFight(id) | Activity::Challenge(id, _) => {
                let form = if matches!(plan.activity, Activity::TitleFight(_)) {
                    FormEvent::TitleDefense
                } else {
                    FormEvent::Fight
                };
                let mut stranger;
                let (opponent, theirs) = if let Some(opponent) = opponent {
                    opponent
                } else {
                    stranger = Self::random(rng, data);
                    (&mut stranger, Medicine::default())
                };
                let medicine = [staff.medicine, theirs];
                let arena = data.world.arena(id);
                let refused = plan.refused;
                let (fight, injury, spared) =
                    self.fight(opponent, (arena, refused), form, medicine, rng, data);
                let prestige = data.world.colosseum(id).map_or(0, |c| c.prestige);
                let title = matches!(plan.activity, Activity::TitleFight(_));
                self.on_stage(prestige, title, rules);
                opponent.on_stage(prestige, title, rules);
                report.fight = Some(fight);
                report.new_injury = injury;
                report.after_effect_spared = spared;
                if self.status == Status::Dead {
                    return Some(report);
                }
            }
        }
        self.end_week(&mut report, rng, data);
        Some(report)
    }

    /// The start of a week whose melee the world plays with everyone in it
    /// (`fighter::melee::play_melee`), and of every week: `plan`'s loadout, then care
    /// following `plan`, at the prices of `staff`'s doctor. `None`, and nothing changes,
    /// if the fighter is retired or dead.
    pub fn begin_melee_week(
        &mut self,
        plan: WeekPlan,
        staff: StaffEffects,
        data: &DesignData,
    ) -> Option<WeekReport> {
        if self.status != Status::Active {
            return None;
        }
        self.loadout = plan.loadout;
        let care = staff.medicine.care_rules(&data.fighter.care);
        let care_cost = if plan.care { self.care(&care) } else { 0 };
        Some(WeekReport::new(plan, care_cost))
    }

    /// The end of a week of melee begun with [`CareerFighter::begin_melee_week`]: the
    /// melee as this fighter lived it (`fight`, with its new injury and whether its
    /// doctor spared it an after-effect) told in `report`, then, unless it died,
    /// healing and the illness draw.
    pub fn end_melee_week<R: Rng + ?Sized>(
        &mut self,
        report: &mut WeekReport,
        (fight, injury, spared): (FightSummary, Option<Injury>, bool),
        rng: &mut R,
        data: &DesignData,
    ) {
        report.fight = Some(fight);
        report.new_injury = injury;
        report.after_effect_spared = spared;
        if self.status != Status::Dead {
            self.end_week(report, rng, data);
        }
    }

    /// Price of caring for every injury and illness where `care` helps, without caring.
    #[must_use]
    pub fn care_cost(&self, care: &CareRules) -> u32 {
        let injuries = self.injuries.iter().filter_map(|i| i.care_cost(care));
        let illness = self.illness.and_then(|i| i.care_cost(care));
        injuries.chain(illness).sum()
    }

    /// Cares for every injury and illness where `care` helps; returns the total price.
    fn care(&mut self, care: &CareRules) -> u32 {
        let mut cost = 0_u32;
        for injury in &mut self.injuries {
            cost = cost.saturating_add(injury.treat(care).unwrap_or(0));
        }
        if let Some(illness) = &mut self.illness {
            cost = cost.saturating_add(illness.treat(care).unwrap_or(0));
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

    /// A duel between this fighter, `FighterId(0)`, without the `refused` pieces of
    /// equipment, and `opponent`, `FighterId(1)`, fully equipped, on the floor of `kind`,
    /// played to its end: its seed, and the arena before its first tick and after its
    /// last. They start at 15 % and 85 % of the width, mid-height.
    pub fn duel<R: Rng + ?Sized>(
        &self,
        opponent: &Self,
        (kind, refused): (ArenaKind, Refused),
        rng: &mut R,
        data: &DesignData,
    ) -> (u64, Arena, Arena) {
        let seed = rng.random::<u64>();
        let start = self.duel_arena(opponent, (kind, refused), seed, data);
        let mut arena = start.clone();
        play(&mut arena);
        (seed, start, arena)
    }

    /// The arena of this fighter's duel against `opponent` on `kind`'s floor, before the
    /// first tick: this fighter is `FighterId(0)`, without the `refused` pieces of
    /// equipment.
    #[must_use]
    pub fn duel_arena(
        &self,
        opponent: &Self,
        (kind, refused): (ArenaKind, Refused),
        seed: u64,
        data: &DesignData,
    ) -> Arena {
        let layout = data.combat.arenas.get(kind);
        let (width, middle) = (layout.width, layout.height / 2.0);
        let (left, right) = (
            Vec2::new(width * 0.15, middle),
            Vec2::new(width * 0.85, middle),
        );
        let fighters = vec![
            self.duel_fighter(opponent, (FighterId(0), refused), data, left),
            opponent.duel_fighter(self, (FighterId(1), Refused::NONE), data, right),
        ];
        Arena::new(width, layout.height, fighters, seed)
            .with_rules(&data.combat)
            .with_layout(layout)
    }

    /// This fighter ready for a duel against `opponent`, as fighter `id` without the
    /// `refused` pieces of equipment, at `position`: as [`CareerFighter::combat_fighter`],
    /// but a proud one keeps only `proud_surrender_percent` of its surrender threshold
    /// against a weaker opponent.
    fn duel_fighter(
        &self,
        opponent: &Self,
        (id, refused): (FighterId, Refused),
        data: &DesignData,
        position: Vec2,
    ) -> Fighter {
        let mut fighter = self.combat_fighter(id, refused, data, position);
        if self.personality.temperament == Temperament::Proud
            && self.stronger_than(opponent, &data.fighter)
        {
            let percent = data.fighter.personality.proud_surrender_percent;
            fighter.surrender_hp = percent_of(fighter.surrender_hp, percent);
        }
        fighter
    }

    #[must_use]
    pub fn has(&self, t: Trait) -> bool {
        self.traits.contains(&t)
    }

    /// What a week of rest does to its morale: the workaholic sulks, the nonchalant
    /// enjoys it.
    fn take_rest(&mut self, rules: &FighterRules) {
        if self.has(Trait::Workaholic) {
            self.feel(
                MoraleEvent::Lowered(rules.traits.workaholic.rest_morale),
                rules,
            );
        }
        if self.has(Trait::Nonchalant) {
            self.feel(
                MoraleEvent::Lifted(rules.traits.nonchalant.rest_morale),
                rules,
            );
        }
    }

    /// What a duel in a colosseum of `prestige`, for a title or not, does to the morale
    /// of a vain survivor: lifted on a great stage, lowered on a small one.
    pub fn on_stage(&mut self, prestige: u32, title: bool, rules: &FighterRules) {
        if self.status == Status::Dead || !self.has(Trait::Vain) {
            return;
        }
        let vain = &rules.traits.vain;
        if title || prestige >= vain.great_prestige {
            self.feel(MoraleEvent::Lifted(vain.great_morale), rules);
        } else if prestige <= vain.small_prestige {
            self.feel(MoraleEvent::Lowered(vain.small_morale), rules);
        }
    }

    /// Its temperament's trade-off.
    fn temperament<'a>(&self, rules: &'a FighterRules) -> &'a TemperamentRules {
        rules
            .personality
            .temperaments
            .get(self.personality.temperament)
    }

    /// Its morale after `event`, the change scaled by its temperament's `morale_percent`.
    fn feel(&mut self, event: MoraleEvent, rules: &FighterRules) {
        let before = self.morale.value();
        let after = self.morale.after(event, &rules.morale).value();
        let percent = self.temperament(rules).morale_percent;
        let value = if after < before {
            before.saturating_sub(percent_of(before - after, percent))
        } else {
            before.saturating_add(percent_of(after - before, percent))
        };
        self.morale = Morale::new(value);
    }

    /// Whether this fighter's fighting stats add up to more than `other`'s.
    #[must_use]
    pub fn stronger_than(&self, other: &Self, rules: &FighterRules) -> bool {
        self.strength(rules) > other.strength(rules)
    }

    /// Its fighting stats added up.
    pub(crate) fn strength(&self, rules: &FighterRules) -> u64 {
        let stats = self.fighting_stats(rules).to_array();
        stats.iter().map(|&v| u64::from(v)).sum()
    }

    /// The fame, record and death the finished fight in `arena`, where this fighter was
    /// `id` without the `refused` pieces of equipment, brings (see `take_side`).
    fn take_fame_and_death<R: Rng + ?Sized>(
        &mut self,
        arena: &Arena,
        (id, refused): (FighterId, Refused),
        against_stronger: bool,
        rng: &mut R,
        rules: &FighterRules,
    ) -> Taken {
        let side = Ending::from_arena(arena, id, &rules.fame);
        let bonus = refused.fame_bonus(&rules.fame);
        self.take_side(side, against_stronger, bonus, rng, rules)
    }

    /// The fame, record and death `side` brings: a beheaded fighter dies; one brought
    /// to 0 HP otherwise survives with a chance of `InjuryRules::fatal_blow_survival`,
    /// and a severe wound; one that lost a limb keeps it lost (a permanent injury); a
    /// stunned one keeps a severe head wound with `InjuryRules::stun_wound_chance`; one
    /// that bled, a light or medium wound where it first bled (`bleeding_wound`).
    /// `bonus` fame comes whatever the result: refused equipment, a place in a melee.
    /// The wound is returned with the outcome and the fame change, not yet taken.
    pub(crate) fn take_side<R: Rng + ?Sized>(
        &mut self,
        side: Ending,
        against_stronger: bool,
        bonus: u32,
        rng: &mut R,
        rules: &FighterRules,
    ) -> Taken {
        let outcome = FightOutcome {
            result: side.result,
            against_stronger,
            spectacular_hits: side.spectacular_hits,
        };
        let bonus = i32::try_from(bonus).unwrap_or(i32::MAX);
        let fame_change = outcome.fame_change(&rules.fame).saturating_add(bonus);
        self.take_fame_and_side(side, (against_stronger, fame_change), rng, rules)
    }

    /// As [`CareerFighter::take_side`], but `fame_change` is all the fame the fight
    /// brings, whatever its result: a melee's. Its temperament's `fame_percent` scales it.
    pub(crate) fn take_fame_and_side<R: Rng + ?Sized>(
        &mut self,
        side: Ending,
        (against_stronger, fame_change): (bool, i32),
        rng: &mut R,
        rules: &FighterRules,
    ) -> Taken {
        let outcome = FightOutcome {
            result: side.result,
            against_stronger,
            spectacular_hits: side.spectacular_hits,
        };
        let percent = i64::from(self.temperament(rules).fame_percent);
        let fame_change = i32::try_from(i64::from(fame_change) * percent / 100).unwrap_or(i32::MAX);
        self.fame = self.fame.changed_by(fame_change);
        self.record.add(outcome.result);
        self.season_record.add(outcome.result);
        let mut wound = None;
        if side.severed == Some(Location::Head) {
            self.status = Status::Dead;
        } else if side.died {
            if rng.random_bool(rules.injury.fatal_blow_survival) {
                wound = Some(fatal_blow_wound(rng, &rules.injury));
            } else {
                self.status = Status::Dead;
            }
        } else if let Some(limb) = side.severed {
            // A stump: nothing more to lose.
            wound = (!has_lost(&self.injuries, limb)).then(|| severed_limb_wound(limb));
        } else if side.stunned && rng.random_bool(rules.injury.stun_wound_chance) {
            wound = Some(stun_wound(rng, &rules.injury));
        } else if let Some(part) = side.bled {
            wound = Some(bleeding_wound(rng, part, &rules.injury));
        }
        Taken {
            outcome,
            fame_change,
            wound,
        }
    }

    fn take_injury(&mut self, injury: Injury, rules: &FighterRules) {
        self.injuries.push(injury);
        self.feel(MoraleEvent::Injury, rules);
    }

    /// A fight against `opponent` that the statistical model resolves instead of a
    /// duel, taking only fame, record and death to both: all a title defense between
    /// rival fighters leaves, in an arena of `kind`. Returns the outcome and fame change
    /// of each, this fighter's first.
    pub fn modelled_fame_and_death<R: Rng + ?Sized>(
        &mut self,
        opponent: &mut Self,
        kind: ArenaKind,
        rng: &mut R,
        data: &DesignData,
    ) -> [(FightOutcome, i32); 2] {
        let rules = &data.fighter;
        let [mine, theirs] = self.modelled_sides(opponent, kind, rng, data);
        for (fighter, taken) in [(&mut *self, mine), (&mut *opponent, theirs)] {
            if let Some(wound) = taken.wound {
                fighter.take_injury(wound, rules);
            }
        }
        [
            (mine.outcome, mine.fame_change),
            (theirs.outcome, theirs.fame_change),
        ]
    }

    /// The fight against `opponent` in an arena of `kind` as the statistical model
    /// resolves it, taken by both (see `take_side`), this fighter's first.
    fn modelled_sides<R: Rng + ?Sized>(
        &mut self,
        opponent: &mut Self,
        kind: ArenaKind,
        rng: &mut R,
        data: &DesignData,
    ) -> [Taken; 2] {
        let rules = &data.fighter;
        let origin = Vec2::new(0.0, 0.0);
        let [mine, theirs] = data.stat_model.get(kind).resolve(
            &self.duel_fighter(opponent, (FighterId(0), Refused::NONE), data, origin),
            &opponent.duel_fighter(self, (FighterId(1), Refused::NONE), data, origin),
            &data.combat.hits,
            rng,
        );
        let stronger = opponent.stronger_than(self, rules);
        let weaker = self.stronger_than(opponent, rules);
        [
            self.take_side(mine, stronger, 0, rng, rules),
            opponent.take_side(theirs, weaker, 0, rng, rules),
        ]
    }

    /// A fight between rival fighters against `opponent` in an arena of `kind`, resolved
    /// by the statistical model: fame, record and death, then what the fight leaves both
    /// (`after_fight`, with `form`'s cost and each one's doctor's `medicine`, this
    /// fighter's first). Neither ends its week. Returns this fighter's result.
    pub fn modelled_fight<R: Rng + ?Sized>(
        &mut self,
        opponent: &mut Self,
        kind: ArenaKind,
        form: FormEvent,
        medicine: [Medicine; 2],
        rng: &mut R,
        data: &DesignData,
    ) -> FightResult {
        let [mine, theirs] = self.modelled_sides(opponent, kind, rng, data);
        let rules = &data.fighter;
        self.after_fight(mine, form, medicine[0], rng, rules);
        opponent.after_fight(theirs, form, medicine[1], rng, rules);
        mine.outcome.result
    }

    /// The same as [`CareerFighter::modelled_fight`], but played as a real duel: what
    /// the `compare` tool sets the model against. Returns this fighter's result.
    pub fn dueled_fight<R: Rng + ?Sized>(
        &mut self,
        opponent: &mut Self,
        kind: ArenaKind,
        form: FormEvent,
        rng: &mut R,
        data: &DesignData,
    ) -> FightResult {
        let alone = [Medicine::default(); 2];
        let equipped = (kind, Refused::NONE);
        let (summary, _, _) = self.fight(opponent, equipped, form, alone, rng, data);
        summary.result
    }

    /// A duel against `opponent` in an arena of `kind`, without the `refused` pieces of
    /// equipment, then its effects on both: fame, death, morale, form (`form`'s cost),
    /// illness worsening, a possible injury and progression. Returns this fighter's new
    /// injury, and whether its doctor's `medicine` (the first, the opponent's doctor's
    /// second) spared it an after-effect.
    fn fight<R: Rng + ?Sized>(
        &mut self,
        opponent: &mut Self,
        (kind, refused): (ArenaKind, Refused),
        form: FormEvent,
        medicine: [Medicine; 2],
        rng: &mut R,
        data: &DesignData,
    ) -> (FightSummary, Option<Injury>, bool) {
        let rules = &data.fighter;
        let (seed, _, arena) = self.duel(opponent, (kind, refused), rng, data);
        let stronger = opponent.stronger_than(self, rules);
        let weaker = self.stronger_than(opponent, rules);
        let setup = FightSetup {
            kind,
            fighters: vec![self.clone(), opponent.clone()],
            entrants: vec![
                Entrant {
                    refused,
                    ..Entrant::default()
                },
                Entrant::default(),
            ],
            melee: false,
        };
        let equipped = (FighterId(1), Refused::NONE);
        let theirs = opponent.take_fame_and_death(&arena, equipped, weaker, rng, rules);
        let mine = self.take_fame_and_death(&arena, (FighterId(0), refused), stronger, rng, rules);
        let died = self.status == Status::Dead;
        // Fallen at 0 HP in the arena, yet alive after the fight.
        let survived = |id: usize, fighter: &Self| {
            arena.fighters.get(id).is_some_and(|f| !f.is_alive()) && fighter.status != Status::Dead
        };
        let summary = FightSummary {
            seed,
            setup,
            report: Report::new(&arena),
            me: FighterId(0),
            place: places(&arena).first().copied().unwrap_or(1),
            against_stronger: stronger,
            refused,
            result: mine.outcome.result,
            died,
            survived_fatal_blow: vec![survived(0, self), survived(1, opponent)],
            fame_change: mine.fame_change,
            ticks: arena.tick,
        };
        let (injury, spared) = self.after_fight(mine, form, medicine[0], rng, rules);
        opponent.after_fight(theirs, form, medicine[1], rng, rules);
        (summary, injury, spared)
    }

    /// What a fight `taken` leaves a survivor: morale, an injury (returned, with whether
    /// `medicine` kept it from leaving an after-effect): the wound of a fatal blow
    /// survived or of a stun, else a possible one, retiring at once if it leaves too many
    /// limbs lost (`too_maimed`); form (`form`'s cost), illness worsening, progression,
    /// and ease with the loadout fought in.
    pub(crate) fn after_fight<R: Rng + ?Sized>(
        &mut self,
        taken: Taken,
        form: FormEvent,
        medicine: Medicine,
        rng: &mut R,
        rules: &FighterRules,
    ) -> (Option<Injury>, bool) {
        if self.status == Status::Dead {
            return (None, false);
        }
        match taken.outcome.result {
            FightResult::Won => {
                self.feel(MoraleEvent::Victory, rules);
            }
            FightResult::Lost | FightResult::GaveUp => {
                self.feel(MoraleEvent::Defeat, rules);
            }
            FightResult::Unfinished => {}
        }
        if let Ok(gained @ 1..) = u32::try_from(taken.fame_change) {
            self.feel(MoraleEvent::FameGained(gained), rules);
        }
        let (injury, spared) = if taken.wound.is_some() {
            (taken.wound, false)
        } else {
            // The risk follows the form the fighter went in with.
            let percent = self.temperament(rules).injury_percent;
            let rolled = roll_injury(rng, (self.form, percent), &rules.injury, &rules.form);
            let spared_percent = medicine.after_effect_spared_percent;
            let injury = rolled.map(|i| spare_after_effect(rng, i, spared_percent, &rules.injury));
            (
                injury,
                rolled.is_some_and(|i| i.is_permanent()) && injury != rolled,
            )
        };
        if let Some(injury) = injury {
            self.take_injury(injury, rules);
            if too_maimed(&self.injuries, &rules.retirement) {
                self.status = Status::Retired;
            }
        }
        self.form = self.form.after(form, &rules.form);
        self.illness = self
            .illness
            .map(|illness| illness.after_fight(rng, &rules.illness));
        self.progress(ProgressEvent::Fight, rng, rules);
        if let Some(ease) = self.ease.get_mut(self.loadout) {
            *ease = (*ease + rules.ease.per_fight).min(100);
        }
        (injury, spared)
    }

    /// The end of a week whose fight was played as someone's opponent: healing and the
    /// illness draw.
    pub fn end_fought_week<R: Rng + ?Sized>(&mut self, rng: &mut R, data: &DesignData) {
        if self.status == Status::Active {
            let plan = WeekPlan {
                activity: Activity::Rest,
                care: false,
                refused: Refused::NONE,
                loadout: self.loadout,
            };
            self.end_week(&mut WeekReport::new(plan, 0), rng, data);
        }
    }

    /// Healing and the illness draw for a healthy fighter.
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
                self.feel(MoraleEvent::Illness, rules);
            }
        }
    }

    /// A year older, at the start of the off-season for everyone (`state::next_week`):
    /// the year's record brings stats (returned), then the age's decline and the
    /// decision to retire.
    pub fn birthday<R: Rng + ?Sized>(&mut self, rng: &mut R, data: &DesignData) -> u32 {
        let rules = &data.fighter;
        self.age += 1;
        let progression = &rules.progression;
        let gains = season_gains(self.season_record, &progression.season);
        self.stats = season_progress(rng, self.stats, self.potential, gains, progression);
        self.season_record = Record::default();
        self.stats = age_decline(self.stats, self.age, progression);
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
        gains
    }
}

/// What a fighter does in a week.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Activity {
    Rest,
    Train,
    /// A duel in this colosseum.
    Fight(ColosseumId),
    /// A duel for this colosseum's title, as its champion or as a challenger from
    /// the top of its ranking; costs the form of a title defense.
    TitleFight(ColosseumId),
    /// A duel in this colosseum against this rival fighter, who challenged the fighter
    /// or accepts its challenge.
    Challenge(ColosseumId, RivalId),
    /// The season-opening melee of this colosseum, in the season's first week only.
    Melee(ColosseumId),
}

impl Activity {
    /// Whether the fighter fights this week: a duel or a melee.
    #[must_use]
    pub fn fights(self) -> bool {
        !matches!(self, Self::Rest | Self::Train)
    }

    /// The colosseum the fighter fights in this week.
    #[must_use]
    pub fn colosseum(self) -> Option<ColosseumId> {
        match self {
            Self::Rest | Self::Train => None,
            Self::Fight(c) | Self::TitleFight(c) | Self::Challenge(c, _) | Self::Melee(c) => {
                Some(c)
            }
        }
    }
}

/// Below this form, `suggested_plan` rests.
pub const REST_BELOW_FORM: u32 = 50;

/// A simple stand-in for the player's decisions, shared by the `career` tool and the
/// game's default choice: care for anything medium or worse, rest when tired, badly
/// hurt or ill, otherwise fight on odd weeks, in the first colosseum (free to enter),
/// and train on even ones.
// ponytail: fixed rules; the rival managers' (`rivals::plan`) could suggest instead.
#[must_use]
pub fn suggested_plan(fighter: &CareerFighter, week: u64) -> WeekPlan {
    let hurt = badly_hurt_or_ill(fighter);
    let activity = if hurt || fighter.form.value() < REST_BELOW_FORM {
        Activity::Rest
    } else if week % 2 == 1 {
        Activity::Fight(ColosseumId(0))
    } else {
        Activity::Train
    };
    WeekPlan {
        activity,
        care: hurt,
        refused: Refused::NONE,
        loadout: fighter.loadout,
    }
}

/// A medium or severe injury, or an illness worse than mild: time to rest and care.
#[must_use]
pub fn badly_hurt_or_ill(fighter: &CareerFighter) -> bool {
    let badly_hurt = fighter
        .injuries
        .iter()
        .any(|i| matches!(i.severity, Severity::Medium | Severity::Severe));
    badly_hurt
        || fighter
            .illness
            .is_some_and(|i| i.level != IllnessLevel::Mild)
}

/// The player's decisions for one fighter and one week.
// ponytail: care is all or nothing here; choosing injury by injury comes with
// the fighter screen in phase 4.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct WeekPlan {
    pub activity: Activity,
    /// Care for every injury and illness where it helps, before the activity.
    pub care: bool,
    /// The pieces of the colosseum's equipment the fighter leaves, if it fights.
    pub refused: Refused,
    /// Its loadout from this week on (`CareerFighter::loadout`).
    pub loadout: usize,
}

/// What happened in a week.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WeekReport {
    pub plan: WeekPlan,
    /// Price of the care given; `state::next_week` takes it from the club's money.
    pub care_cost: u32,
    pub fight: Option<FightSummary>,
    pub new_injury: Option<Injury>,
    /// The doctor kept the new injury from leaving an after-effect: it is only severe.
    pub after_effect_spared: bool,
    pub healed_injuries: Vec<Injury>,
    pub new_illness: Option<Illness>,
    pub illness_healed: bool,
    /// A year went by: the season's gains, one year older, decline from 40,
    /// retirement decision.
    pub birthday: bool,
    /// Stat gains the year's record brought at the birthday.
    pub season_gains: u32,
}

impl WeekReport {
    fn new(plan: WeekPlan, care_cost: u32) -> Self {
        Self {
            plan,
            care_cost,
            fight: None,
            new_injury: None,
            after_effect_spared: false,
            healed_injuries: Vec::new(),
            new_illness: None,
            illness_healed: false,
            birthday: false,
            season_gains: 0,
        }
    }
}

/// A fight as one of its fighters, the career fighter, lived it: a duel, or a melee
/// that several of the club's fighters may share, each with its own summary.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FightSummary {
    /// Replaying the fight with this seed gives the same fight.
    pub seed: u64,
    /// What the fight was played from: with the seed, its arena before the first tick
    /// (`FightSummary::start`).
    pub setup: FightSetup,
    /// Damage dealt and taken and spells cast by each fighter, in the arena's order.
    pub report: Report,
    /// The career fighter in the arena: `FighterId(0)` in a duel.
    pub me: FighterId,
    /// The career fighter's place, 1 the best (see `log::places`).
    pub place: u32,
    /// Stronger than this fighter, or than its average in a melee.
    pub against_stronger: bool,
    /// The pieces of equipment the career fighter fought without.
    pub refused: Refused,
    pub result: FightResult,
    pub died: bool,
    /// Whether each fighter, in the arena's order, fell at 0 HP in the arena and
    /// survived it (`InjuryRules::fatal_blow_survival`): the arena shows it dead, the
    /// fight leaves it alive and badly wounded.
    pub survived_fatal_blow: Vec<bool>,
    pub fame_change: i32,
    pub ticks: u32,
}

impl FightSummary {
    /// The fight before its first tick: playing it with utility AIs gives this very
    /// fight, to watch it.
    #[must_use]
    pub fn start(&self, data: &DesignData) -> Arena {
        self.setup.arena(self.seed, data)
    }

    /// The others before the fight, in the arena's order: one in a duel.
    pub fn opponents(&self) -> impl Iterator<Item = &CareerFighter> {
        let me = usize::try_from(self.me.0).unwrap_or(usize::MAX);
        let fighters = self.setup.fighters.iter().enumerate();
        fighters.filter(move |&(i, _)| i != me).map(|(_, f)| f)
    }

    /// The opponent of a duel, the first other fighter of a melee.
    #[must_use]
    pub fn opponent(&self) -> Option<&CareerFighter> {
        self.opponents().next()
    }

    /// Whether more than two fought.
    #[must_use]
    pub fn is_melee(&self) -> bool {
        self.setup.fighters.len() > 2
    }

    /// The career fighter's line of the report.
    #[must_use]
    pub fn mine(&self) -> Option<&crate::log::FighterReport> {
        self.report.fighters.get(usize::try_from(self.me.0).ok()?)
    }
}

/// What a fight is played from, so that its seed plays it again without keeping the
/// fight itself: its floor, and its fighters before it, in the arena's order, each
/// as it went in.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FightSetup {
    pub kind: ArenaKind,
    pub fighters: Vec<CareerFighter>,
    /// How each fighter went in, in the same order; only the refused equipment counts
    /// in a duel.
    pub entrants: Vec<Entrant>,
    /// A melee (`melee::melee_start`); else a duel between the first two fighters
    /// (`CareerFighter::duel_arena`).
    pub melee: bool,
}

impl FightSetup {
    /// The arena of the fight before its first tick, seeded with `seed`.
    #[must_use]
    pub fn arena(&self, seed: u64, data: &DesignData) -> Arena {
        match self.fighters.as_slice() {
            [me, opponent] if !self.melee => {
                let refused = self.entrants.first().map_or(Refused::NONE, |e| e.refused);
                me.duel_arena(opponent, (self.kind, refused), seed, data)
            }
            fighters => melee_start(fighters, &self.entrants, self.kind, seed, data),
        }
    }
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
    /// Chance, in percent, that a generated fighter is ambidextrous: the lowest rolls
    /// of the right-handed ones.
    pub ambidextrous_percent: u32,
    /// Fights from which a fighter is a veteran, played by its class's most advanced
    /// learned model rather than the recruits' one.
    pub veteran_fights: u32,
}

/// What a fight brings a fighter before its after-effects (see `CareerFighter::take_side`).
#[derive(Debug, Clone, Copy)]
pub(crate) struct Taken {
    pub(crate) outcome: FightOutcome,
    pub(crate) fame_change: i32,
    /// The severe wound of a fatal blow survived.
    wound: Option<Injury>,
}

/// A roster saved with its moved entries as their class, status and new place alone
/// (`#[serde(with = "entries")]`), which keeps the saves from swelling with the copies
/// of fighters living on elsewhere.
pub mod entries {
    use serde::{Deserialize, Deserializer, Serialize, Serializer};

    use super::{CareerFighter, Status};
    use crate::colosseum::Contender;
    use crate::fighter::ClassId;
    use crate::names::FighterName;

    #[derive(Serialize, Deserialize)]
    #[serde(untagged)]
    enum Entry<F> {
        Moved {
            class: ClassId,
            #[serde(default)]
            name: FighterName,
            #[serde(default)]
            culture: String,
            status: Status,
            moved_to: Contender,
        },
        Here(F),
    }

    /// # Errors
    /// If the serializer fails.
    pub fn serialize<S: Serializer>(
        fighters: &[CareerFighter],
        serializer: S,
    ) -> Result<S::Ok, S::Error> {
        serializer.collect_seq(fighters.iter().map(|f| match f.moved_to {
            Some(moved_to) => Entry::Moved {
                class: f.class,
                name: f.name.clone(),
                culture: f.culture.clone(),
                status: f.status,
                moved_to,
            },
            None => Entry::Here(f),
        }))
    }

    /// # Errors
    /// If an entry is neither a moved one nor a fighter.
    pub fn deserialize<'de, D: Deserializer<'de>>(
        deserializer: D,
    ) -> Result<Vec<CareerFighter>, D::Error> {
        let entries = Vec::<Entry<CareerFighter>>::deserialize(deserializer)?;
        Ok(entries
            .into_iter()
            .map(|entry| match entry {
                Entry::Moved {
                    class,
                    name,
                    culture,
                    status,
                    moved_to,
                } => CareerFighter {
                    name,
                    culture,
                    ..CareerFighter::moved_entry(class, status, moved_to)
                },
                Entry::Here(fighter) => fighter,
            })
            .collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::staff::Coaching;
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
            refused: Refused::NONE,
            loadout: 0,
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
        f.play_week(
            plan(Activity::Rest),
            None,
            StaffEffects::default(),
            &mut rng,
            &data,
        )
        .unwrap();
        assert_eq!(f.form.value(), 65);
        f.play_week(
            plan(Activity::Train),
            None,
            StaffEffects::default(),
            &mut rng,
            &data,
        )
        .unwrap();
        assert_eq!(f.form.value(), 60);
    }

    /// Fighter 7, balanced, at form 80 and neutral morale, with `traits` alone.
    fn with_traits(traits: &[Trait]) -> CareerFighter {
        CareerFighter {
            personality: Personality::neutral(),
            traits: traits.to_vec(),
            form: Form::new(80),
            ..fighter(7)
        }
    }

    fn week(f: &mut CareerFighter, activity: Activity, data: &DesignData) {
        let mut rng = ChaCha8Rng::seed_from_u64(0);
        f.play_week(
            plan(activity),
            None,
            StaffEffects::default(),
            &mut rng,
            data,
        )
        .unwrap();
    }

    #[test]
    fn the_workaholic_sulks_at_rest_and_the_nonchalant_at_training() {
        let data = data();
        let rules = &data.fighter.traits;
        let neutral = Morale::NEUTRAL;
        let mut workaholic = with_traits(&[Trait::Workaholic]);
        week(&mut workaholic, Activity::Rest, &data);
        assert_eq!(
            workaholic.morale.value(),
            neutral - rules.workaholic.rest_morale
        );
        let mut nonchalant = with_traits(&[Trait::Nonchalant]);
        week(&mut nonchalant, Activity::Rest, &data);
        assert_eq!(
            nonchalant.morale.value(),
            neutral + rules.nonchalant.rest_morale
        );
        let mut plain = with_traits(&[]);
        let mut nonchalant = with_traits(&[Trait::Nonchalant]);
        week(&mut plain, Activity::Train, &data);
        week(&mut nonchalant, Activity::Train, &data);
        assert_eq!(plain.morale.value(), neutral);
        assert_eq!(
            nonchalant.morale.value(),
            neutral - rules.nonchalant.training_morale
        );
        let cost = |f: &CareerFighter| 80 - f.form.value();
        assert_eq!(
            cost(&nonchalant),
            percent_of(cost(&plain), rules.nonchalant.training_form_percent)
        );
    }

    #[test]
    fn the_workaholic_gains_more_from_training() {
        let data = data();
        let trained = |traits: &[Trait]| {
            let mut f = with_traits(traits);
            f.age = 20;
            f.potential = f.stats.map(|v| v * 2);
            let mut rng = ChaCha8Rng::seed_from_u64(0);
            for _ in 0..100 {
                f.form = Form::full();
                f.morale = Morale::neutral();
                let staff = StaffEffects::default();
                f.play_week(plan(Activity::Train), None, staff, &mut rng, &data);
            }
            f.stats.to_array().iter().sum::<u32>()
        };
        assert!(trained(&[Trait::Workaholic]) > trained(&[]));
    }

    #[test]
    fn the_vain_likes_great_stages_and_scorns_small_ones() {
        let rules = &data().fighter;
        let vain = &rules.traits.vain;
        let staged = |prestige, title| {
            let mut f = with_traits(&[Trait::Vain]);
            f.on_stage(prestige, title, rules);
            f.morale.value()
        };
        let neutral = Morale::NEUTRAL;
        assert_eq!(
            staged(vain.great_prestige, false),
            neutral + vain.great_morale
        );
        assert_eq!(
            staged(vain.small_prestige, true),
            neutral + vain.great_morale
        );
        assert_eq!(
            staged(vain.small_prestige, false),
            neutral - vain.small_morale
        );
        assert_eq!(staged(vain.small_prestige + 1, false), neutral);
        let mut plain = with_traits(&[]);
        plain.on_stage(vain.small_prestige, false, rules);
        assert_eq!(plain.morale.value(), neutral);
    }

    #[test]
    fn a_trainer_spares_form() {
        let data = data();
        let mut rng = ChaCha8Rng::seed_from_u64(0);
        let mut f = fighter(7);
        f.form = Form::new(50);
        let coaching = Coaching {
            progress_percent: 50,
            form_spared: 3,
        };
        let staff = StaffEffects {
            coaching,
            ..StaffEffects::default()
        };
        f.play_week(plan(Activity::Train), None, staff, &mut rng, &data)
            .unwrap();
        assert_eq!(f.form.value(), 48);
        let spared_all = StaffEffects {
            coaching: Coaching {
                form_spared: 99,
                ..coaching
            },
            ..staff
        };
        f.play_week(plan(Activity::Train), None, spared_all, &mut rng, &data)
            .unwrap();
        assert_eq!(f.form.value(), 48);
    }

    #[test]
    fn a_doctor_s_care_shortens_recovery_more_at_its_price() {
        let data = data();
        let injury = Injury {
            location: Location::Body,
            severity: Severity::Severe,
            weeks_left: 8,
        };
        let medicine = Medicine {
            care_weeks_off_percent: 25,
            care_cost_percent: 150,
            after_effect_spared_percent: 0,
        };
        let staff = StaffEffects {
            medicine,
            ..StaffEffects::default()
        };
        let cared = |staff: StaffEffects| {
            let mut f = fighter(7);
            f.injuries = vec![injury];
            let mut rng = ChaCha8Rng::seed_from_u64(0);
            let rest = WeekPlan {
                activity: Activity::Rest,
                care: true,
                refused: Refused::NONE,
                loadout: 0,
            };
            let report = f.play_week(rest, None, staff, &mut rng, &data).unwrap();
            (report.care_cost, f.injuries[0].weeks_left)
        };
        let severe = data.fighter.care.injury_cost.severe;
        // Half the 8 weeks kept without a doctor, a quarter with it, then a week heals.
        assert_eq!(cared(StaffEffects::default()), (severe, 3));
        assert_eq!(cared(staff), (severe * 3 / 2, 1));
    }

    #[test]
    fn a_fight_costs_form_and_moves_fame() {
        let data = data();
        let mut rng = ChaCha8Rng::seed_from_u64(1);
        let mut f = fighter(7);
        let report = f
            .play_week(
                plan(Activity::Fight(ColosseumId(0))),
                None,
                StaffEffects::default(),
                &mut rng,
                &data,
            )
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
            .play_week(
                plan(Activity::Rest),
                None,
                StaffEffects::default(),
                &mut rng,
                &data,
            )
            .unwrap();
        assert_eq!(report.healed_injuries, vec![injury]);
        assert_eq!(f.injuries, []);
    }

    #[test]
    fn a_fighter_keeps_the_loadout_its_plan_chose_and_fights_in_it() {
        let data = data();
        let mut f = fighter(7);
        f.class = ClassId(0);
        f.ease = vec![40, 90];
        assert_eq!(f.preferred_loadout(), 1);
        f.ease = vec![70, 70];
        assert_eq!(f.preferred_loadout(), 0);
        let rest = WeekPlan {
            loadout: 1,
            ..plan(Activity::Rest)
        };
        let mut rng = ChaCha8Rng::seed_from_u64(1);
        f.play_week(rest, None, StaffEffects::default(), &mut rng, &data);
        assert_eq!(f.loadout, 1);
        assert_eq!(suggested_plan(&f, 2).loadout, 1);
        let at = Vec2::new(0.0, 0.0);
        let fighter = f.combat_fighter(FighterId(0), Refused::NONE, &data, at);
        assert_eq!(fighter.spells[1], data.classes[0].loadouts[1].spell);
    }

    #[test]
    fn a_loadout_one_is_not_at_ease_with_weakens_and_fighting_in_it_helps() {
        let data = data();
        let rules = &data.fighter;
        let mut f = fighter(7);
        f.class = ClassId(0);
        f.ease = vec![100, 0];
        f.loadout = 1;
        let at = Vec2::new(0.0, 0.0);
        let awkward = f.combat_fighter(FighterId(0), Refused::NONE, &data, at);
        f.ease = vec![100, 100];
        let easy = f.combat_fighter(FighterId(0), Refused::NONE, &data, at);
        assert!(awkward.stats.melee_defense < easy.stats.melee_defense);
        assert_eq!(awkward.stats.hp, easy.stats.hp);
        f.ease = vec![100, 0];
        // Timid, so that it gives up early and lives to learn from the fight.
        f.personality.boldness = crate::fighter::personality::TraitValue::new(0);
        let mut rng = ChaCha8Rng::seed_from_u64(1);
        let fight = plan(Activity::Fight(ColosseumId(0)));
        let fight = WeekPlan {
            loadout: 1,
            ..fight
        };
        f.play_week(fight, None, StaffEffects::default(), &mut rng, &data);
        assert_eq!(f.ease, [100, rules.ease.per_fight]);
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
        let data = data();
        let fighter = f.combat_fighter(FighterId(0), Refused::NONE, &data, Vec2::new(3.0, 10.0));
        let loadout = data.class(f.class).unwrap().loadout(f.loadout).unwrap();
        let equipped =
            loadout
                .off_hand
                .stats(f.fighting_stats(&rules), f.ambidextrous, &rules.equipment);
        let equipped = rules.ease.stats(equipped, f.ease_with(f.loadout));
        assert_eq!(fighter.stats, equipped);
        assert_eq!(fighter.condition.injuries, f.injuries);
    }

    #[test]
    fn a_head_wound_lowers_accuracy_not_attacks() {
        let rules = data().fighter;
        for severity in [
            crate::fighter::injury::Severity::Medium,
            crate::fighter::injury::Severity::Permanent,
        ] {
            let mut f = fighter(7);
            let healthy = f.fighting_stats(&rules);
            f.injuries.push(Injury {
                location: crate::fighter::injury::Location::Head,
                severity,
                weeks_left: 3,
            });
            let hurt = f.fighting_stats(&rules);
            assert!(hurt.accuracy < healthy.accuracy, "{severity:?}");
            assert_eq!(
                (hurt.melee_attack, hurt.ranged_attack),
                (healthy.melee_attack, healthy.ranged_attack)
            );
        }
    }

    #[test]
    fn a_good_season_brings_stats_at_the_birthday() {
        let data = data();
        let mut rng = ChaCha8Rng::seed_from_u64(3);
        let mut f = fighter(7);
        f.age = 25;
        f.potential = f.stats.map(|v| v * 2);
        let age = f.age;
        f.season_record = Record {
            wins: 8,
            losses: 2,
            draws: 0,
        };
        let before = f.stats;
        let gains = f.birthday(&mut rng, &data);
        assert_eq!(f.age, age + 1);
        assert!(gains > 0);
        let total = |s: Stats| s.to_array().iter().sum::<u32>();
        assert!(total(f.stats) >= total(before) + gains);
        assert_eq!(f.season_record, Record::default());
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
                f.play_week(
                    plan(Activity::Fight(ColosseumId(0))),
                    None,
                    StaffEffects::default(),
                    &mut rng,
                    &data
                ),
                None
            );
            assert_eq!(f, before);
        }
    }

    #[test]
    fn a_fatal_blow_kills_or_leaves_a_severe_wound() {
        let mut data = data();
        let fallen = Ending {
            result: FightResult::Lost,
            died: true,
            stunned: false,
            knocked_out: false,
            severed: None,
            bled: None,
            spectacular_hits: 0,
        };
        let mut rng = ChaCha8Rng::seed_from_u64(1);
        let rules = &mut data.fighter;
        rules.injury.fatal_blow_survival = 0.0;
        let mut dead = fighter(1);
        dead.take_side(fallen, false, 0, &mut rng, rules);
        assert_eq!(dead.status, Status::Dead);
        rules.injury.fatal_blow_survival = 1.0;
        let mut survivor = fighter(1);
        let taken = survivor.take_side(fallen, false, 0, &mut rng, rules);
        assert_eq!(survivor.status, Status::Active);
        let wound = taken.wound.unwrap();
        assert_eq!(wound.severity, Severity::Severe);
        // The wound replaces the usual injury draw, and is the one reported.
        let (injury, _) = survivor.after_fight(
            taken,
            FormEvent::Fight,
            Medicine::default(),
            &mut rng,
            rules,
        );
        assert_eq!(injury, Some(wound));
        assert_eq!(survivor.injuries, [wound]);
        assert_eq!(survivor.record.losses, 1);
    }

    #[test]
    fn a_fighter_without_its_armour_defends_weaker() {
        let data = data();
        let mut rng = ChaCha8Rng::seed_from_u64(4);
        let f = CareerFighter::random(&mut rng, &data);
        let at = Vec2::new(3.0, 10.0);
        let equipped = f.combat_fighter(FighterId(0), Refused::NONE, &data, at);
        let naked = Refused { armour: true };
        let bare = f.combat_fighter(FighterId(0), naked, &data, at);
        assert_eq!(
            bare.stats,
            naked.stats(equipped.stats, &data.fighter.equipment)
        );
        assert!(bare.stats.melee_defense < equipped.stats.melee_defense);
        assert_eq!(bare.stats.melee_attack, equipped.stats.melee_attack);
    }

    #[test]
    fn refused_equipment_brings_fame_whatever_the_result() {
        let data = data();
        let rules = &data.fighter;
        let lost = Ending {
            result: FightResult::GaveUp,
            died: false,
            stunned: false,
            knocked_out: false,
            severed: None,
            bled: None,
            spectacular_hits: 0,
        };
        let armour = Refused { armour: true };
        let mut rng = ChaCha8Rng::seed_from_u64(5);
        let mut f = CareerFighter::random(&mut rng, &data);
        let equipped = f.clone().take_side(lost, false, 0, &mut rng, rules);
        let bare = f.take_side(lost, false, armour.fame_bonus(&rules.fame), &mut rng, rules);
        let bonus = i32::try_from(rules.fame.refused_armour).unwrap();
        assert_eq!(bare.fame_change, equipped.fame_change + bonus);
    }

    #[test]
    fn a_week_s_fight_is_fought_without_what_the_plan_refuses() {
        let data = data();
        let mut rng = ChaCha8Rng::seed_from_u64(6);
        let mut f = CareerFighter::random(&mut rng, &data);
        let armour = Refused { armour: true };
        let week = WeekPlan {
            refused: armour,
            loadout: f.loadout,
            ..plan(Activity::Fight(ColosseumId(0)))
        };
        let expected = f.combat_fighter(FighterId(0), armour, &data, Vec2::new(0.0, 0.0));
        let report = f
            .play_week(week, None, StaffEffects::default(), &mut rng, &data)
            .unwrap();
        let fight = report.fight.unwrap();
        assert_eq!(fight.refused, armour);
        assert_eq!(fight.start(&data).fighters[0].stats, expected.stats);
    }

    #[test]
    fn a_duel_is_fought_on_the_floor_of_its_arena() {
        let data = data();
        let mut rng = ChaCha8Rng::seed_from_u64(1);
        let (a, b) = (
            CareerFighter::random(&mut rng, &data),
            CareerFighter::random(&mut rng, &data),
        );
        for kind in ArenaKind::ALL {
            let layout = data.combat.arenas.get(kind);
            let (_, start, end) = a.duel(&b, (kind, Refused::NONE), &mut rng, &data);
            assert_eq!((start.width, start.height), (layout.width, layout.height));
            assert_eq!(start.speed_percent, layout.speed_percent);
            assert_eq!(start.pillars, layout.pillars);
            assert!(start.fighters[0].position.x < start.fighters[1].position.x);
            assert!(end.fighters.iter().all(|f| f.position.x <= layout.width));
        }
    }

    #[test]
    fn a_fight_takes_the_arena_of_its_colosseum() {
        let data = data();
        let rome = data
            .world
            .colosseums
            .iter()
            .position(|c| c.arena == ArenaKind::Pillars)
            .expect("a colosseum with pillars");
        let rome = ColosseumId(u32::try_from(rome).unwrap());
        let mut rng = ChaCha8Rng::seed_from_u64(2);
        let mut f = CareerFighter::random(&mut rng, &data);
        let report = f
            .play_week(
                plan(Activity::Fight(rome)),
                None,
                StaffEffects::default(),
                &mut rng,
                &data,
            )
            .unwrap();
        let fight = report.fight.unwrap();
        assert_eq!(
            fight.start(&data).pillars,
            data.combat.arenas.pillars.pillars
        );
    }

    #[test]
    fn a_fight_tells_who_survived_a_fatal_blow() {
        let mut data = data();
        for survival in [0.0, 1.0] {
            data.fighter.injury.fatal_blow_survival = survival;
            let mut rng = ChaCha8Rng::seed_from_u64(3);
            let fallen = (0..500).find_map(|_| {
                let mut mine = CareerFighter::random(&mut rng, &data);
                let mut theirs = CareerFighter::random(&mut rng, &data);
                let alone = [Medicine::default(); 2];
                let sand = ArenaKind::Sand;
                let (summary, ..) = mine.fight(
                    &mut theirs,
                    (sand, Refused::NONE),
                    FormEvent::Fight,
                    alone,
                    &mut rng,
                    &data,
                );
                let mut arena = summary.start(&data);
                crate::duel::play(&mut arena);
                let i = arena.fighters.iter().position(|f| !f.is_alive())?;
                let beheaded = arena.fighters[i].severed == Some(Location::Head);
                (!beheaded).then_some((summary, i, [mine, theirs]))
            });
            let (summary, i, fighters) = fallen.expect("a fighter falls at 0 HP in 500 fights");
            let survived = survival > 0.5;
            assert_eq!(summary.survived_fatal_blow[i], survived);
            assert!(!summary.survived_fatal_blow[1 - i]);
            assert_eq!(fighters[i].status == Status::Dead, !survived);
        }
    }

    #[test]
    fn a_beheaded_fighter_dies_and_a_severed_limb_stays_lost() {
        let mut data = data();
        let cut = |severed| Ending {
            result: FightResult::Lost,
            died: severed == Location::Head,
            stunned: true,
            knocked_out: false,
            severed: Some(severed),
            bled: None,
            spectacular_hits: 0,
        };
        let mut rng = ChaCha8Rng::seed_from_u64(1);
        let rules = &mut data.fighter;
        // Even a fighter sure to survive a fatal blow, or keep a stun's wound.
        rules.injury.fatal_blow_survival = 1.0;
        rules.injury.stun_wound_chance = 1.0;
        let mut beheaded = fighter(1);
        let taken = beheaded.take_side(cut(Location::Head), false, 0, &mut rng, rules);
        assert_eq!((beheaded.status, taken.wound), (Status::Dead, None));
        let arm = Location::Arm(Side::Left);
        let mut maimed = fighter(1);
        let taken = maimed.take_side(cut(arm), false, 0, &mut rng, rules);
        assert_eq!(maimed.status, Status::Active);
        assert_eq!(taken.wound, Some(severed_limb_wound(arm)));
        // A lost limb is a defeat, not a surrender: no fame lost.
        assert_eq!(taken.fame_change, 0);
        // No doctor gives a limb back.
        let medicine = Medicine {
            after_effect_spared_percent: 100,
            ..Medicine::default()
        };
        let (injury, spared) =
            maimed.after_fight(taken, FormEvent::Fight, medicine, &mut rng, rules);
        assert_eq!((injury, spared), (Some(severed_limb_wound(arm)), false));
        assert!(maimed.injuries[0].is_lost());
        assert_eq!(maimed.status, Status::Active);
        // The model may cut the same arm again: a stump, no second lost arm.
        let taken = maimed.take_side(cut(arm), false, 0, &mut rng, rules);
        assert_eq!(taken.wound, None);
        // A leg too: it fights on. The other arm: it retires at once.
        for (limb, status) in [
            (Location::Leg(Side::Left), Status::Active),
            (Location::Arm(Side::Right), Status::Retired),
        ] {
            let taken = maimed.take_side(cut(limb), false, 0, &mut rng, rules);
            maimed.after_fight(taken, FormEvent::Fight, medicine, &mut rng, rules);
            assert_eq!(maimed.status, status);
        }
    }

    #[test]
    fn a_stun_may_leave_a_severe_head_wound() {
        let mut data = data();
        let knocked_out = Ending {
            result: FightResult::Lost,
            died: false,
            stunned: true,
            knocked_out: true,
            severed: None,
            bled: None,
            spectacular_hits: 0,
        };
        let mut rng = ChaCha8Rng::seed_from_u64(1);
        let rules = &mut data.fighter;
        rules.injury.stun_wound_chance = 1.0;
        let mut f = fighter(1);
        let taken = f.take_side(knocked_out, false, 0, &mut rng, rules);
        assert_eq!(f.status, Status::Active);
        let wound = taken.wound.unwrap();
        assert_eq!(
            (wound.location, wound.severity),
            (Location::Head, Severity::Severe)
        );
        // A knock-out is a defeat, not a surrender: no fame lost.
        assert_eq!(taken.fame_change, 0);
        rules.injury.stun_wound_chance = 0.0;
        let taken = fighter(1).take_side(knocked_out, false, 0, &mut rng, rules);
        assert_eq!(taken.wound, None);
    }

    #[test]
    fn a_bleeding_leaves_at_most_a_medium_wound_where_it_bled() {
        let data = data();
        let arm = Location::Arm(Side::Left);
        let bled = Ending {
            result: FightResult::GaveUp,
            died: false,
            stunned: false,
            knocked_out: false,
            severed: None,
            bled: Some(arm),
            spectacular_hits: 0,
        };
        let mut rng = ChaCha8Rng::seed_from_u64(2);
        for _ in 0..50 {
            let taken = fighter(1).take_side(bled, false, 0, &mut rng, &data.fighter);
            let wound = taken.wound.unwrap();
            assert_eq!(wound.location, arm);
            assert!(matches!(wound.severity, Severity::Light | Severity::Medium));
        }
    }

    #[test]
    fn the_start_of_a_fight_replays_it() {
        let data = data();
        let mut rng = ChaCha8Rng::seed_from_u64(5);
        let mut f = fighter(7);
        let report = f
            .play_week(
                plan(Activity::Fight(ColosseumId(0))),
                None,
                StaffEffects::default(),
                &mut rng,
                &data,
            )
            .unwrap();
        let fight = report.fight.unwrap();
        let mut replay = fight.start(&data);
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
                .play_week(
                    plan(fight),
                    Some((&mut opponent, Medicine::default())),
                    StaffEffects::default(),
                    &mut rng,
                    &data,
                )
                .unwrap();
            let summary = report.fight.unwrap();
            assert_eq!(summary.opponent(), Some(&fighter(seed + 100)));
            let mut replay = summary.start(&data);
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
                care: true,
                refused: Refused::NONE,
                loadout: f.loadout,
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
                    f.play_week(
                        plan(Activity::Fight(ColosseumId(0))),
                        None,
                        StaffEffects::default(),
                        &mut rng,
                        &data,
                    )
                })
                .collect();
            (f, reports)
        };
        assert_eq!(career(), career());
    }
}
