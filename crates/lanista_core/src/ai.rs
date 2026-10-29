use crate::combat::{
    AccuracyRules, Arena, Mine, RADIUS, Range, Vec2, can_cast_on, edge_distance, of_use,
    path_direction, shot_blocked, spell_damage, spell_hit_percent,
};
use crate::design_data::SpellData;
use crate::fighter::form::Form;
use crate::fighter::illness::IllnessLevel;
use crate::fighter::morale::Morale;
use crate::fighter::personality::{Personality, Temperament};
use crate::fighter::{Condition, Fighter, FighterId};
use crate::melee::{in_danger, in_final};

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub enum Action {
    #[default]
    Wait,
    /// Move at full speed in this direction; its length is ignored.
    Move(Vec2),
    /// Cast spell `spell` (index in the fighter's `spells`) on `target`, as soon as possible
    /// until the next decision.
    Cast { spell: usize, target: FighterId },
}

/// Decides for one fighter; called every `TICKS_PER_DECISION` ticks.
pub trait CombatAi {
    fn decide(&mut self, arena: &Arena, me: FighterId) -> Action;
}

/// Score of closing in on an enemy no spell can reach. Any positive value works:
/// moving is only offered when no cast is possible.
const MOVE_SCORE: f64 = 1.0;
// ponytail: AI weights in code like `MOVE_SCORE`; move them to `data/` once balancing needs it.
/// Retreat score of a fighter with fear 1 at 0 HP; casts score the share of the
/// target's HP they take, so 1 is a killing blow.
const RETREAT_WEIGHT: f64 = 2.0;
/// Below this retreat score a fighter still closes in and fights normally.
const RETREAT_MIN: f64 = 0.1;
/// An enemy threatens a fighter within its longest reach plus this margin, in meters.
const THREAT_MARGIN: f64 = 1.0;
/// Score of stepping in on an enemy in reach while no spell is ready, at boldness 100.
const PRESS_SCORE: f64 = 0.05;
/// Closer than this, in meters, there is nothing left to press.
const PRESS_MIN_DISTANCE: f64 = 0.1;
/// Ticks a showman at ego 100 waits for its strongest spell instead of a weaker hit.
const SHOW_WAIT_TICKS: f64 = 60.0;
/// Share of the strongest spell's value a showman gives to waiting for it.
const SHOW_WAIT_VALUE: f64 = 0.9;
/// Share of its best throw's value a fighter gives to gathering elements for it: below
/// the throw, so it throws whenever it can pay for it.
const GATHER_VALUE: f64 = 0.9;

/// Utility AI: gives every possible action a score and plays the best one.
/// Its condition (personality, morale, form, injuries, illness) changes the scores,
/// so an injured or fearful fighter fights differently; a neutral one ignores it.
// ponytail: aims at the nearest enemy only, and ranged fighters do not keep their distance.
#[derive(Debug, Default)]
pub struct UtilityAi;

impl CombatAi for UtilityAi {
    fn decide(&mut self, arena: &Arena, me: FighterId) -> Action {
        best(&score_actions(arena, me))
    }
}

/// How far a trait or gauge is above its neutral value, from -1 to 1.
fn offset(value: u32, neutral: u32) -> f64 {
    (f64::from(value) - f64::from(neutral)) / f64::from(neutral)
}

/// How much a fighter wants out of the fight once hurt, from 0 (neutral or bolder) to 1.
/// Up with timidity, injuries, illness, low form and low morale; down with boldness.
/// Always 0 for a berserker.
#[must_use]
pub fn fear(condition: &Condition) -> f64 {
    let p = &condition.personality;
    if p.temperament == Temperament::Berserker {
        return 0.0;
    }
    let illness = condition.illness.map_or(0.0, |ill| match ill.level {
        IllnessLevel::Mild => 0.1,
        IllnessLevel::Moderate => 0.2,
        IllnessLevel::Severe => 0.3,
    });
    let injuries = u32::try_from(condition.injuries.len()).unwrap_or(u32::MAX);
    let fear = -0.75 * offset(p.boldness.value(), Personality::NEUTRAL)
        - 0.2 * offset(condition.morale.value(), Morale::NEUTRAL)
        + 0.2 * (1.0 - f64::from(condition.form.value()) / f64::from(Form::MAX))
        + 0.15 * f64::from(injuries)
        + illness;
    fear.clamp(0.0, 1.0)
}

/// Every action worth considering for `me`, with its score; higher is better.
/// A cast scores the share of the target's HP it is expected to take, given its chance
/// to hit; waiting scores 0, or the
/// value a showman sees in its strongest spell about to be ready; closing in scores
/// `MOVE_SCORE`; gathering elements scores `GATHER_VALUE` of the best spell they pay for,
/// while the reserve is not full; a fearful, hurt fighter scores backing away from a
/// threatening enemy while it has room to get out of its reach (`can_back_off`), and
/// otherwise fights as if unafraid.
/// A pillar between the two makes ranged spells useless: the fighter walks around it,
/// as it closes in around any pillar or mine in its way (`Arena::obstacles`). A mine
/// not yet armed whose blast would reach it scores stepping away (`escape_mine`); a mine
/// spell is not cast on a target within its own blast. In a melee, a fighter the next check
/// would put out (`in_danger`), or in the final (`in_final`), neither backs away nor
/// waits: it goes for a hit.
#[must_use]
pub fn score_actions(arena: &Arena, me: FighterId) -> Vec<(Action, f64)> {
    let mut scored = vec![(Action::Wait, 0.0)];
    let Some(me) = arena.fighter(me) else {
        return scored;
    };
    let Some(target) = nearest_enemy(arena, me) else {
        return scored;
    };
    let distance = edge_distance(me, target);
    let blocked = shot_blocked(&arena.pillars, me.position, target.position);
    let usable =
        |data: &SpellData| !(blocked && data.range == Range::Ranged && !data.over_obstacles);
    for (spell, data) in me.spells.iter().enumerate() {
        if !(can_cast_on(arena, me, spell, target) && of_use(arena, me, spell, target)) {
            continue;
        }
        let (on, value) = if data.gathers > 0 {
            (me.id, gather_value(me, target, &arena.accuracy))
        } else {
            (target.id, cast_value(me, target, data, &arena.accuracy))
        };
        scored.push((Action::Cast { spell, target: on }, value));
    }
    let can_cast = scored.len() > 1;
    let endangered = in_danger(arena, me) || in_final(arena);
    if !endangered {
        scored[0].1 = show_wait_value(me, target, distance, &arena.accuracy);
    }

    let retreat = if endangered {
        0.0
    } else {
        fear(&me.condition) * (1.0 - share(me.hp, me.stats.hp)) * RETREAT_WEIGHT
    };
    let toward = path_direction(&arena.obstacles(me.id), me.position, target.position);
    let longest_reach = |f: &Fighter| f.spells.iter().map(|s| s.reach).fold(0.0, f64::max);
    let usable_reach = me
        .spells
        .iter()
        .filter(|s| usable(s))
        .map(|s| s.reach)
        .fold(0.0, f64::max);
    let away = Vec2::new(
        me.position.x - target.position.x,
        me.position.y - target.position.y,
    );
    // Out of the target's reach once backed away by this much, in meters; 0 or less
    // when not threatened.
    let escape = longest_reach(target) + THREAT_MARGIN - distance;
    if retreat > RETREAT_MIN && escape >= 0.0 && can_back_off(arena, me, away, escape) {
        scored.push((Action::Move(away), retreat));
    } else if distance > usable_reach {
        scored.push((Action::Move(toward), MOVE_SCORE));
    } else if !can_cast && distance > PRESS_MIN_DISTANCE {
        let press = if endangered {
            1.0
        } else {
            let personality = &me.condition.personality;
            // A berserker presses harder the more it is hurt.
            let rage = if personality.temperament == Temperament::Berserker {
                1.0 - share(me.hp, me.stats.hp)
            } else {
                0.0
            };
            offset(personality.boldness.value(), Personality::NEUTRAL).max(rage)
        };
        if press > 0.0 {
            scored.push((Action::Move(toward), press * PRESS_SCORE));
        }
    }
    scored.extend(escape_mine(arena, me, target));
    scored
}

/// Stepping away from the mine not yet armed, among those `me` sees (`Arena::sees`),
/// whose blast would hurt it most, or a trap of its own that would catch it, worth the
/// share of its HP that blast or bite would take; straight away from `target` when the
/// mine lies right under it. Nothing without such a mine.
fn escape_mine(arena: &Arena, me: &Fighter, target: &Fighter) -> Option<(Action, f64)> {
    let gap = |from: Vec2| Vec2::new(me.position.x - from.x, me.position.y - from.y);
    let reach = |m: &Mine| {
        if arena.trap_of(m).is_some() {
            RADIUS + arena.mine_rules.radius
        } else {
            arena.mine_rules.blast_radius
        }
    };
    arena
        .mines
        .iter()
        .filter(|m| {
            arena.sees(me.id, m) && m.armed_at > arena.tick && gap(m.position).length() <= reach(m)
        })
        .filter_map(|m| {
            let owner = arena.fighter(m.owner)?;
            let spell = owner.spells.get(m.spell)?;
            let value = share(spell_damage(&owner.stats, &me.stats, spell), me.hp);
            let away = gap(m.position);
            let away = if away.length() > 0.0 {
                away
            } else {
                gap(target.position)
            };
            Some((Action::Move(away), value))
        })
        .max_by(|a, b| a.1.total_cmp(&b.1))
}

/// `part` / `whole`, 0 when `whole` is 0.
fn share(part: u32, whole: u32) -> f64 {
    if whole == 0 {
        0.0
    } else {
        f64::from(part) / f64::from(whole)
    }
}

/// Share of the target's current HP a spell is expected to take: what it takes on a hit,
/// above 1 for overkill, times the chance to hit.
fn cast_value(me: &Fighter, target: &Fighter, spell: &SpellData, rules: &AccuracyRules) -> f64 {
    let hit = f64::from(spell_hit_percent(me, &target.stats, spell, rules)) / 100.0;
    let blows = if spell.mine { 1 } else { spell.blows };
    share(
        spell_damage(&me.stats, &target.stats_against(spell), spell) * blows,
        target.hp,
    ) * hit
}

/// What gathering elements is worth: `GATHER_VALUE` of the best spell that spends some,
/// on the target, whatever its reach or cooldown.
fn gather_value(me: &Fighter, target: &Fighter, rules: &AccuracyRules) -> f64 {
    me.spells
        .iter()
        .filter(|data| data.cost.elements > 0)
        .map(|data| cast_value(me, target, data, rules))
        .fold(0.0, f64::max)
        * GATHER_VALUE
}

/// What waiting is worth to a showman: a share of its strongest spell's value when
/// that spell reaches the target, is affordable and will be ready soon; 0 otherwise.
fn show_wait_value(me: &Fighter, target: &Fighter, distance: f64, rules: &AccuracyRules) -> f64 {
    let flair = offset(me.condition.personality.ego.value(), Personality::NEUTRAL);
    if flair <= 0.0 {
        return 0.0;
    }
    let Some((spell, data)) = me
        .spells
        .iter()
        .enumerate()
        .filter(|(_, data)| distance <= data.reach)
        .max_by_key(|(_, data)| spell_damage(&me.stats, &target.stats, data))
    else {
        return 0.0;
    };
    let cooldown = f64::from(me.cooldowns[spell]);
    if cooldown > 0.0 && cooldown <= flair * SHOW_WAIT_TICKS && me.can_afford(spell) {
        cast_value(me, target, data, rules) * SHOW_WAIT_VALUE
    } else {
        0.0
    }
}

/// Whether `me` has room to back away along `direction` far enough to get `escape`
/// meters away, and at least half a meter, before a wall: a fighter that cannot get out
/// of reach fights instead, rather than wait against a wall to be hit.
fn can_back_off(arena: &Arena, me: &Fighter, direction: Vec2, escape: f64) -> bool {
    let length = direction.length();
    if length <= 0.0 {
        return false;
    }
    let (low, high) = arena.reachable();
    let room = |position: f64, step: f64, low: f64, high: f64| {
        if step > 0.0 {
            (high - position) / step
        } else if step < 0.0 {
            (low - position) / step
        } else {
            f64::INFINITY
        }
    };
    let (dx, dy) = (direction.x / length, direction.y / length);
    let room = room(me.position.x, dx, low.x, high.x).min(room(me.position.y, dy, low.y, high.y));
    room >= escape.max(0.5)
}

/// The highest-scored action; on a tie, the first one.
fn best(scored: &[(Action, f64)]) -> Action {
    let mut best = (Action::Wait, f64::NEG_INFINITY);
    for &(action, score) in scored {
        if score > best.1 {
            best = (action, score);
        }
    }
    best.0
}

/// The closest other fighter standing and not an ally, so a fighter down is left alone,
/// among its prey first (`Arena::hunts`); on a tie, the lowest ID.
// ponytail: a prey is chased across the arena past any closer enemy; weigh it by
// distance if hunters fare too badly.
fn nearest_enemy<'a>(arena: &'a Arena, me: &Fighter) -> Option<&'a Fighter> {
    let mut nearest: Option<(&Fighter, bool, f64)> = None;
    for f in arena
        .fighters
        .iter()
        .filter(|f| f.is_standing() && f.id != me.id && !arena.allied(me.id, f.id))
    {
        let prey = arena.hunts.contains(&(me.id, f.id));
        let distance = edge_distance(me, f);
        if nearest.is_none_or(|(_, p, d)| (prey, -distance) > (p, -d)) {
            nearest = Some((f, prey, distance));
        }
    }
    nearest.map(|(f, _, _)| f)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::combat::{RADIUS, tick};
    use crate::design_data::DesignData;
    use crate::duel::STATS;
    use crate::fighter::personality::TraitValue;
    use crate::fighter::{ClassId, test_fighter};

    #[test]
    fn waits_when_alone() {
        let arena = Arena::new(20.0, 20.0, vec![test_fighter(0, 5.0, 5.0)], 0);
        assert_eq!(UtilityAi.decide(&arena, FighterId(0)), Action::Wait);
    }

    #[test]
    fn moves_toward_an_enemy_out_of_reach() {
        let fighters = vec![test_fighter(0, 5.0, 5.0), test_fighter(1, 10.0, 5.0)];
        let arena = Arena::new(20.0, 20.0, fighters, 0);
        assert_eq!(
            UtilityAi.decide(&arena, FighterId(0)),
            Action::Move(Vec2::new(5.0, 0.0))
        );
    }

    #[test]
    fn casts_on_an_enemy_in_reach() {
        let fighters = vec![test_fighter(0, 5.0, 5.0), test_fighter(1, 6.5, 5.0)];
        let arena = Arena::new(20.0, 20.0, fighters, 0);
        assert_eq!(
            UtilityAi.decide(&arena, FighterId(0)),
            Action::Cast {
                spell: 0,
                target: FighterId(1)
            }
        );
    }

    #[test]
    fn goes_for_its_prey_first() {
        let fighters = vec![
            test_fighter(0, 5.0, 5.0),
            test_fighter(1, 6.5, 5.0),
            test_fighter(2, 15.0, 5.0),
        ];
        let mut arena = Arena::new(20.0, 20.0, fighters, 0);
        arena.hunts.push((FighterId(0), FighterId(2)));
        assert_eq!(
            UtilityAi.decide(&arena, FighterId(0)),
            Action::Move(Vec2::new(10.0, 0.0))
        );
        arena.fighters[2].stunned_ticks = 10;
        assert!(matches!(
            UtilityAi.decide(&arena, FighterId(0)),
            Action::Cast {
                target: FighterId(1),
                ..
            }
        ));
    }

    #[test]
    fn leaves_a_fighter_down_alone() {
        let fighters = vec![test_fighter(0, 5.0, 5.0), test_fighter(1, 6.5, 5.0)];
        let mut arena = Arena::new(20.0, 20.0, fighters, 0);
        arena.fighters[1].stunned_ticks = 10;
        assert_eq!(UtilityAi.decide(&arena, FighterId(0)), Action::Wait);
    }

    #[test]
    fn waits_in_reach_while_on_cooldown() {
        let fighters = vec![test_fighter(0, 5.0, 5.0), test_fighter(1, 6.5, 5.0)];
        let mut arena = Arena::new(20.0, 20.0, fighters, 0);
        arena.fighters[0].cooldowns[0] = 10;
        assert_eq!(UtilityAi.decide(&arena, FighterId(0)), Action::Wait);
    }

    #[test]
    fn steps_off_a_mine_before_it_is_armed_and_lays_none_on_a_target_too_close() {
        use crate::combat::{Mine, MineRules};
        let fighters = vec![test_fighter(0, 5.0, 5.0), test_fighter(1, 6.5, 5.0)];
        let mut arena = Arena::new(20.0, 20.0, fighters, 0);
        arena.mine_rules = MineRules {
            arm_ticks: 30,
            life_ticks: 900,
            radius: 0.3,
            blast_radius: 1.5,
            max_per_fighter: 3,
        };
        arena.fighters[1].spells[0].power = 300;
        arena.mines.push(Mine {
            position: Vec2::new(5.0, 5.0),
            owner: FighterId(1),
            spell: 0,
            armed_at: 30,
            gone_at: 900,
        });
        // Right under it: away from the enemy, rather than a weaker hit.
        assert_eq!(
            UtilityAi.decide(&arena, FighterId(0)),
            Action::Move(Vec2::new(-1.5, 0.0))
        );
        arena.mines.clear();
        let spell = &mut arena.fighters[0].spells[0];
        (spell.mine, spell.reach) = (true, 8.0);
        // 1.5 m between centers: its own mine would blast it too.
        assert_eq!(UtilityAi.decide(&arena, FighterId(0)), Action::Wait);
        arena.fighters[1].position = Vec2::new(10.0, 5.0);
        assert_eq!(
            UtilityAi.decide(&arena, FighterId(0)),
            Action::Cast {
                spell: 0,
                target: FighterId(1)
            }
        );
    }

    #[test]
    fn gathers_while_it_cannot_pay_for_a_throw_then_throws() {
        use crate::combat::{ArenaLayout, Element};
        use crate::fighter::test_thrower;

        let fighters = vec![test_thrower(0, 5.0, 5.0), test_fighter(1, 10.0, 5.0)];
        let floor = ArenaLayout {
            width: 20.0,
            height: 20.0,
            speed_percent: 100,
            pillars: Vec::new(),
            door: crate::combat::Vec2::new(10.0, 0.0),
        };
        let mut arena =
            Arena::new(20.0, 20.0, fighters.clone(), 0).with_layout(&floor, Element::Stone);
        let gather = Action::Cast {
            spell: 1,
            target: FighterId(0),
        };
        assert_eq!(UtilityAi.decide(&arena, FighterId(0)), gather);
        arena.fighters[0].elements.current = 2;
        assert_eq!(
            UtilityAi.decide(&arena, FighterId(0)),
            Action::Cast {
                spell: 2,
                target: FighterId(1)
            }
        );
        // A full reserve, the throw cooling down: nothing more to gather.
        arena.fighters[0].elements.current = 4;
        arena.fighters[0].cooldowns[2] = 10;
        assert_ne!(UtilityAi.decide(&arena, FighterId(0)), gather);
        // Nothing to gather on a floor without an element.
        let arena = Arena::new(20.0, 20.0, fighters, 0);
        assert_ne!(UtilityAi.decide(&arena, FighterId(0)), gather);
    }

    fn shipped_classes() -> DesignData {
        DesignData::load().unwrap()
    }

    #[test]
    fn picks_the_strongest_ready_spell() {
        let data = shipped_classes();
        let gladiator = &data.classes[0];
        let fighters = vec![
            Fighter::new(
                FighterId(0),
                ClassId(0),
                gladiator,
                STATS,
                Vec2::new(5.0, 5.0),
            ),
            Fighter::new(
                FighterId(1),
                ClassId(0),
                gladiator,
                STATS,
                Vec2::new(6.5, 5.0),
            ),
        ];
        let mut arena = Arena::new(20.0, 20.0, fighters, 0);
        // Spell 2 (power 250) beats spell 1 (130), which beats spell 0 (70).
        let cast = |spell| Action::Cast {
            spell,
            target: FighterId(1),
        };
        assert_eq!(UtilityAi.decide(&arena, FighterId(0)), cast(2));
        arena.fighters[0].cooldowns[2] = 1;
        assert_eq!(UtilityAi.decide(&arena, FighterId(0)), cast(1));
    }

    #[test]
    fn strikes_with_its_weapon_only_once_it_can_pay_for_no_spell() {
        let data = shipped_classes();
        let gladiator = &data.classes[0];
        let fighters = [5.0, 6.5]
            .into_iter()
            .zip(0..)
            .map(|(x, id)| {
                Fighter::new(
                    FighterId(id),
                    ClassId(0),
                    gladiator,
                    STATS,
                    Vec2::new(x, 5.0),
                )
            })
            .collect();
        let mut arena = Arena::new(20.0, 20.0, fighters, 0);
        let auto = arena.fighters[0].auto_attack.unwrap();
        assert_eq!(auto, 3);
        // Every spell on cooldown but affordable: it waits.
        arena.fighters[0].cooldowns[..3].fill(10);
        assert_eq!(UtilityAi.decide(&arena, FighterId(0)), Action::Wait);
        // Too tired for its cheapest spell (5 endurance): it strikes with its sword.
        arena.fighters[0].endurance.current = 4;
        let strike = Action::Cast {
            spell: auto,
            target: FighterId(1),
        };
        assert_eq!(UtilityAi.decide(&arena, FighterId(0)), strike);
    }

    #[test]
    fn strikes_with_its_weapon_when_no_affordable_spell_is_of_use() {
        let data = shipped_classes();
        let floor = data.combat.arenas.get(crate::combat::ArenaKind::Narrow);
        let class = &data.classes[1];
        let fighters = [2.0, 3.5]
            .into_iter()
            .zip(0..)
            .map(|(x, id)| Fighter::new(FighterId(id), ClassId(1), class, STATS, Vec2::new(x, 3.0)))
            .collect();
        let mut arena = Arena::new(8.0, 6.0, fighters, 0)
            .with_rules(&data.combat)
            .with_layout(floor, crate::combat::Element::Stone);
        let me = &mut arena.fighters[0];
        let auto = me.auto_attack.unwrap();
        // Too tired to gather or throw, its mine affordable but the target in its blast:
        // nothing else to do but strike with its dagger.
        me.endurance.current = 4;
        assert!(me.can_afford(2));
        let strike = Action::Cast {
            spell: auto,
            target: FighterId(1),
        };
        assert_eq!(UtilityAi.decide(&arena, FighterId(0)), strike);
        // Farther, the mine is of use again: it lays it rather than strike.
        arena.fighters[1].position = Vec2::new(6.0, 3.0);
        assert_eq!(
            UtilityAi.decide(&arena, FighterId(0)),
            Action::Cast {
                spell: 2,
                target: FighterId(1)
            }
        );
    }

    /// Fighter 0 at (`x`, 5) with `hp` left, against fighter 1 touching it on its right.
    fn face_to_face(x: f64, hp: u32, condition: Condition) -> Arena {
        let mut me = test_fighter(0, x, 5.0);
        me.hp = hp;
        me.condition = condition;
        let fighters = vec![me, test_fighter(1, x + 1.5, 5.0)];
        Arena::new(20.0, 20.0, fighters, 0)
    }

    fn fearful() -> Condition {
        Condition {
            personality: Personality {
                boldness: TraitValue::new(0),
                ..Personality::neutral()
            },
            ..Condition::default()
        }
    }

    fn hit() -> Action {
        Action::Cast {
            spell: 0,
            target: FighterId(1),
        }
    }

    #[test]
    fn fear_is_zero_when_neutral_and_grows_with_timidity_injuries_illness_and_fatigue() {
        use crate::fighter::illness::Illness;
        use crate::fighter::injury::{Injury, Location, Severity, Side};

        assert!(fear(&Condition::default()).abs() < f64::EPSILON);
        assert!((fear(&fearful()) - 0.75).abs() < 1e-9);
        let bold = Personality {
            boldness: TraitValue::new(80),
            ..Personality::neutral()
        };
        let worn = Condition {
            personality: bold,
            form: Form::new(0),
            injuries: vec![Injury {
                location: Location::Leg(Side::Left),
                severity: Severity::Medium,
                weeks_left: 3,
            }],
            illness: Some(Illness {
                level: IllnessLevel::Moderate,
                weeks_left: 3,
            }),
            ..Condition::default()
        };
        // 0.2 (form 0) + 0.15 (one injury) + 0.2 (moderate illness) - 0.45 (boldness).
        assert!((fear(&worn) - 0.1).abs() < 1e-9);
        assert!(
            fear(&Condition {
                morale: Morale::new(0),
                ..worn.clone()
            }) > fear(&worn)
        );
    }

    #[test]
    fn a_fearful_hurt_fighter_backs_away() {
        let arena = face_to_face(5.0, 30, fearful());
        assert_eq!(
            UtilityAi.decide(&arena, FighterId(0)),
            Action::Move(Vec2::new(-1.5, 0.0))
        );
        // Unhurt, or neutral, it fights.
        let arena = face_to_face(5.0, 100, fearful());
        assert_eq!(UtilityAi.decide(&arena, FighterId(0)), hit());
        let arena = face_to_face(5.0, 30, Condition::default());
        assert_eq!(UtilityAi.decide(&arena, FighterId(0)), hit());
    }

    #[test]
    fn an_injured_and_ill_fighter_backs_away_too() {
        use crate::fighter::illness::Illness;
        use crate::fighter::injury::{Injury, Location, Severity, Side};

        let injury = Injury {
            location: Location::Arm(Side::Right),
            severity: Severity::Severe,
            weeks_left: 6,
        };
        let condition = Condition {
            injuries: vec![injury; 3],
            illness: Some(Illness {
                level: IllnessLevel::Severe,
                weeks_left: 6,
            }),
            ..Condition::default()
        };
        let arena = face_to_face(5.0, 30, condition);
        assert!(matches!(
            UtilityAi.decide(&arena, FighterId(0)),
            Action::Move(_)
        ));
    }

    #[test]
    fn a_fearful_fighter_still_takes_a_killing_blow() {
        let mut arena = face_to_face(5.0, 30, fearful());
        arena.fighters[1].hp = 5;
        assert_eq!(UtilityAi.decide(&arena, FighterId(0)), hit());
    }

    #[test]
    fn a_cornered_fighter_fights() {
        let arena = face_to_face(RADIUS, 30, fearful());
        assert_eq!(UtilityAi.decide(&arena, FighterId(0)), hit());
    }

    #[test]
    fn a_fearful_fighter_that_cannot_get_out_of_reach_closes_in() {
        // Against a wall, a shooter 3.5 m away reaching 8 m: it closes in rather than
        // wait there to be shot.
        let mut arena = face_to_face(RADIUS, 30, fearful());
        arena.fighters[1].position = Vec2::new(5.0, 5.0);
        arena.fighters[1].spells[0].reach = 8.0;
        assert_eq!(
            UtilityAi.decide(&arena, FighterId(0)),
            Action::Move(Vec2::new(4.5, 0.0))
        );
        // Too little room behind to get 9 m away: the same.
        arena.fighters[0].position = Vec2::new(4.0, 5.0);
        arena.fighters[1].position = Vec2::new(8.0, 5.0);
        assert_eq!(
            UtilityAi.decide(&arena, FighterId(0)),
            Action::Move(Vec2::new(4.0, 0.0))
        );
        // Room enough: it backs away.
        arena.fighters[0].position = Vec2::new(16.0, 5.0);
        arena.fighters[1].position = Vec2::new(19.0, 5.0);
        assert_eq!(
            UtilityAi.decide(&arena, FighterId(0)),
            Action::Move(Vec2::new(-3.0, 0.0))
        );
    }

    #[test]
    fn a_bold_fighter_steps_in_while_its_spells_cool_down() {
        let aggressive = Condition {
            personality: Personality {
                boldness: TraitValue::new(100),
                ..Personality::neutral()
            },
            ..Condition::default()
        };
        let mut arena = face_to_face(5.0, 100, aggressive);
        arena.fighters[0].cooldowns[0] = 10;
        assert_eq!(
            UtilityAi.decide(&arena, FighterId(0)),
            Action::Move(Vec2::new(1.5, 0.0))
        );
    }

    #[test]
    fn a_berserker_fears_nothing_and_steps_in_once_hurt() {
        let berserker = Condition {
            personality: Personality {
                temperament: Temperament::Berserker,
                boldness: TraitValue::new(0),
                ..Personality::neutral()
            },
            ..Condition::default()
        };
        assert!(fear(&berserker).abs() < f64::EPSILON);
        // As timid as can be: unhurt it waits for its spell, hurt it steps in.
        let mut arena = face_to_face(5.0, 100, berserker.clone());
        arena.fighters[0].cooldowns[0] = 10;
        assert_eq!(UtilityAi.decide(&arena, FighterId(0)), Action::Wait);
        arena.fighters[0].hp = 10;
        assert_eq!(
            UtilityAi.decide(&arena, FighterId(0)),
            Action::Move(Vec2::new(1.5, 0.0))
        );
    }

    #[test]
    fn a_showman_waits_for_its_strongest_spell() {
        let data = shipped_classes();
        let gladiator = &data.classes[0];
        let fighter = |id, x| {
            Fighter::new(
                FighterId(id),
                ClassId(0),
                gladiator,
                STATS,
                Vec2::new(x, 5.0),
            )
        };
        let mut arena = Arena::new(20.0, 20.0, vec![fighter(0, 5.0), fighter(1, 6.5)], 0);
        arena.fighters[0].cooldowns[2] = 30;
        let cast = |spell| Action::Cast {
            spell,
            target: FighterId(1),
        };
        assert_eq!(UtilityAi.decide(&arena, FighterId(0)), cast(1));
        arena.fighters[0].condition.personality.ego = TraitValue::new(100);
        assert_eq!(UtilityAi.decide(&arena, FighterId(0)), Action::Wait);
        // Too long to wait for: the showman hits now.
        arena.fighters[0].cooldowns[2] = 100;
        assert_eq!(UtilityAi.decide(&arena, FighterId(0)), cast(1));
    }

    #[test]
    fn a_duel_of_utility_ais_ends_with_a_death() {
        let data = shipped_classes();
        let fighters = vec![
            Fighter::new(
                FighterId(0),
                ClassId(0),
                &data.classes[0],
                STATS,
                Vec2::new(3.0, 10.0),
            ),
            Fighter::new(
                FighterId(1),
                ClassId(1),
                &data.classes[1],
                STATS,
                Vec2::new(17.0, 10.0),
            ),
        ];
        let mut arena = Arena::new(20.0, 20.0, fighters, 0);
        let mut ais: Vec<Box<dyn CombatAi>> = vec![Box::new(UtilityAi), Box::new(UtilityAi)];
        // One minute of fight at most.
        while arena.fighters.iter().all(Fighter::is_alive) && arena.tick < 1800 {
            tick(&mut arena, &mut ais);
        }
        assert!(
            arena.fighters.iter().any(|f| !f.is_alive()),
            "{:?}",
            arena.fighters.iter().map(|f| f.hp).collect::<Vec<_>>()
        );
    }
}
