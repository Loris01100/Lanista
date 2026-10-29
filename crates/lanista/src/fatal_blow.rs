//! The fatal blow of a duel shown big, if on: shortly before the death the
//! fight slows down and the camera closes in on both fighters; once the victim has
//! fallen, the blow is played again from a copy of the arena taken just before it,
//! close on the killer, then close on the victim, mirrored. The AIs keep no memory, so a
//! copy of the arena replays the blow as it went.

use lanista_core::combat::{self, Arena, TICKS_PER_SECOND};
use lanista_core::duel::ais;
use lanista_core::fighter::FighterId;
use lanista_core::log::EventKind;
use macroquad::math::{Rect, Vec2, vec2};
use reesch::blend_views;

use crate::fight_view::{FightView, to_f32};

/// Fight time a real second plays during the blow.
const SLOW: f32 = 0.2;
/// Ticks before the death the slow motion begins, and the replays.
const LEAD_TICKS: u32 = TICKS_PER_SECOND * 3 / 2;
const REPLAY_LEAD_TICKS: u32 = TICKS_PER_SECOND / 2;
/// Fight seconds a shot goes on once the fight is over: the fall.
const AFTER_SECONDS: f32 = 0.5;
/// Real seconds the camera takes to close in.
const MOVE_SECONDS: f32 = 0.5;
/// The smallest world area a shot shows, and the room around the fighters, in meters.
const CLOSE: (f32, f32) = (5.0, 3.0);
const MARGIN: f32 = 1.0;
/// The live shot, then a replay on the killer, then one on the victim.
const SHOTS: usize = 3;

pub struct FatalBlow {
    /// The tick the victim dies on.
    tick: u32,
    /// Where the killer and the victim stand then.
    killer: Vec2,
    victim: Vec2,
    /// The arena just before the blow, the replays starting from it.
    before: Option<Arena>,
    /// The shot playing, `None` before the slow motion.
    shot: Option<usize>,
    /// Real seconds since the shot began, and since its fight was over.
    elapsed: f32,
    over_for: f32,
    replay: Option<FightView>,
}

impl FatalBlow {
    /// The blow that kills in the duel `arena`, played on a copy; none in a melee, nor
    /// in a duel nobody dies in.
    #[must_use]
    pub fn find(arena: &Arena) -> Option<Self> {
        if arena.fighters.len() != 2 {
            return None;
        }
        let mut copy = arena.clone();
        let mut ais = ais(&copy);
        let ticks = copy.max_ticks;
        combat::run(&mut copy, &mut ais, ticks);
        let (tick, victim) = copy.log.iter().find_map(|e| match e.kind {
            EventKind::Death { fighter } => Some((e.tick, fighter)),
            _ => None,
        })?;
        let place = |id: FighterId| {
            let at = copy.fighter(id)?.position;
            Some(vec2(to_f32(at.x), to_f32(at.y)))
        };
        let killer = copy.fighters.iter().find(|f| f.id != victim)?.id;
        Some(Self {
            tick,
            killer: place(killer)?,
            victim: place(victim)?,
            before: None,
            shot: None,
            elapsed: 0.0,
            over_for: 0.0,
            replay: None,
        })
    }

    /// Whether `live` is past the death without the blow shown: skipped, or not shown big.
    #[must_use]
    pub fn missed(&self, live: &FightView) -> bool {
        self.shot.is_none() && live.arena.tick > self.tick
    }

    /// Whether it is playing, the slow motion begun once `live` comes near the death.
    pub fn begins(&mut self, live: &FightView) -> bool {
        if self.shot.is_none() && live.arena.tick + LEAD_TICKS >= self.tick {
            self.shot = Some(0);
        }
        self.shot.is_some()
    }

    #[must_use]
    pub fn playing(&self) -> bool {
        self.shot.is_some_and(|shot| shot < SHOTS)
    }

    #[must_use]
    pub fn is_over(&self) -> bool {
        self.shot.is_some_and(|shot| shot >= SHOTS)
    }

    /// The fight a replay shows instead of `live`.
    #[must_use]
    pub fn replay(&self) -> Option<&FightView> {
        self.replay.as_ref()
    }

    /// Whether the shot shows the world mirrored.
    #[must_use]
    pub fn mirrored(&self) -> bool {
        self.shot == Some(2)
    }

    /// Plays `dt` real seconds of the shot, slowed down, on `live` or its replay;
    /// returns the sounds to play.
    pub fn play(&mut self, live: &mut FightView, dt: f32) -> Vec<&'static str> {
        let Some(shot) = self.shot.filter(|&shot| shot < SHOTS) else {
            return Vec::new();
        };
        if self.before.is_none() && live.arena.tick + REPLAY_LEAD_TICKS >= self.tick {
            self.before = Some(live.arena.clone());
        }
        let view = self.replay.as_mut().unwrap_or(live);
        let sounds = view.advance(dt * SLOW);
        self.elapsed += dt;
        if !view.running() {
            self.over_for += dt;
        }
        if self.over_for >= AFTER_SECONDS / SLOW {
            let cast = live.cast.clone();
            self.replay = self
                .before
                .clone()
                .filter(|_| shot + 1 < SHOTS)
                .map(|arena| FightView::new(arena, cast));
            // No copy to replay from: over.
            self.shot = Some(if self.replay.is_some() {
                shot + 1
            } else {
                SHOTS
            });
            self.elapsed = 0.0;
            self.over_for = 0.0;
        }
        sounds
    }

    /// The area the camera shows, through `shown`: from `whole` closing in on both
    /// fighters live, then on one of them each replay.
    #[must_use]
    pub fn view(&self, whole: Rect, shown: impl Fn(Rect) -> Rect) -> Rect {
        match self.shot {
            None => shown(whole),
            Some(0) => blend_views(
                shown(whole),
                shown(around(&[self.killer, self.victim])),
                self.elapsed / MOVE_SECONDS,
            ),
            Some(1) => shown(around(&[self.killer])),
            Some(_) => shown(around(&[self.victim])),
        }
    }
}

/// The world area around `points`, at least `CLOSE`.
fn around(points: &[Vec2]) -> Rect {
    let low = points.iter().fold(Vec2::INFINITY, |a, &p| a.min(p));
    let high = points.iter().fold(Vec2::NEG_INFINITY, |a, &p| a.max(p));
    let size = (high - low + Vec2::splat(2.0 * MARGIN)).max(CLOSE.into());
    let centre = (low + high) / 2.0;
    Rect::new(
        centre.x - size.x / 2.0,
        centre.y - size.y / 2.0,
        size.x,
        size.y,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_shot_frames_its_fighters_with_room_around() {
        let one = around(&[vec2(10.0, 5.0)]);
        assert_eq!(one.size(), Vec2::from(CLOSE));
        assert_eq!(one.center(), vec2(10.0, 5.0));
        let both = around(&[vec2(0.0, 0.0), vec2(8.0, 1.0)]);
        assert!(both.contains(vec2(-0.9, 0.0)) && both.contains(vec2(8.9, 1.0)));
    }
}
