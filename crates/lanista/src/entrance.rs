//! The fighters' entrance, before the first tick of the fight shown big: they walk in
//! from the nearest door, then the camera goes to each one presented in turn while its
//! card shows, and back to the whole arena. In real time: the fight speed leaves it be.

use lanista_core::fighter::FighterId;
use macroquad::math::{Rect, Vec2};
use reesch::blend_views;

/// Seconds the walk in from the door takes.
const WALK_SECONDS: f32 = 2.0;
/// Seconds the camera takes to go from one framing to the next.
const MOVE_SECONDS: f32 = 0.7;
/// Seconds the camera stays on a fighter presented, its card shown.
const CARD_SECONDS: f32 = 2.5;
/// The world area around a fighter presented, in meters: the fighter left of its
/// middle, the card on the right.
const CLOSE_UP: (f32, f32) = (8.0, 5.0);
const CLOSE_UP_SHIFT: f32 = 1.5;

#[derive(Debug, Clone, PartialEq)]
pub struct Entrance {
    /// Seconds since it began.
    elapsed: f32,
    /// The fighters presented, in turn.
    presented: Vec<FighterId>,
}

impl Entrance {
    #[must_use]
    pub fn new(presented: Vec<FighterId>) -> Self {
        Self {
            elapsed: 0.0,
            presented,
        }
    }

    pub fn advance(&mut self, dt: f32) {
        self.elapsed += dt.max(0.0);
    }

    /// Share of the walk in done, from 0 to 1.
    #[must_use]
    pub fn walked(&self) -> f32 {
        (self.elapsed / WALK_SECONDS).min(1.0)
    }

    #[must_use]
    pub fn is_over(&self) -> bool {
        let turns = self.presented.iter().map(|_| MOVE_SECONDS + CARD_SECONDS);
        let back = if self.presented.is_empty() {
            0.0
        } else {
            MOVE_SECONDS
        };
        self.elapsed >= WALK_SECONDS + turns.sum::<f32>() + back
    }

    /// The area the camera shows now: `whole` during the walk, then going to each
    /// fighter's `close_up` and staying there, then back to `whole`.
    #[must_use]
    pub fn view(&self, whole: Rect, close_up: impl Fn(FighterId) -> Rect) -> Rect {
        let mut t = self.elapsed - WALK_SECONDS;
        if t < 0.0 {
            return whole;
        }
        let mut from = whole;
        for &id in &self.presented {
            let to = close_up(id);
            if t < MOVE_SECONDS + CARD_SECONDS {
                return blend_views(from, to, t / MOVE_SECONDS);
            }
            t -= MOVE_SECONDS + CARD_SECONDS;
            from = to;
        }
        blend_views(from, whole, t / MOVE_SECONDS)
    }

    /// The fighter whose card shows: the one the camera has stopped on.
    #[must_use]
    pub fn card(&self) -> Option<FighterId> {
        let mut t = self.elapsed - WALK_SECONDS - MOVE_SECONDS;
        for &id in &self.presented {
            if t < 0.0 {
                return None;
            }
            if t < CARD_SECONDS {
                return Some(id);
            }
            t -= MOVE_SECONDS + CARD_SECONDS;
        }
        None
    }
}

/// The world area shown around a fighter presented standing `at`.
#[must_use]
pub fn close_up(at: Vec2) -> Rect {
    let (w, h) = CLOSE_UP;
    Rect::new(at.x + CLOSE_UP_SHIFT - w / 2.0, at.y - h / 2.0, w, h)
}

#[cfg(test)]
mod tests {
    use super::*;

    const WHOLE: Rect = Rect {
        x: -1.0,
        y: -1.0,
        w: 22.0,
        h: 12.0,
    };

    /// Fighter n's close-up: one meter wide at n.
    fn near(id: FighterId) -> Rect {
        #[expect(clippy::cast_precision_loss, reason = "a test's small ids")]
        let x = id.0 as f32;
        Rect::new(x, 0.0, 1.0, 1.0)
    }

    fn at(seconds: f32, presented: &[u32]) -> Entrance {
        let mut entrance = Entrance::new(presented.iter().map(|&k| FighterId(k)).collect());
        entrance.advance(seconds);
        entrance
    }

    #[test]
    fn the_fighters_walk_in_then_each_is_shown_in_turn() {
        let walk = at(1.0, &[0, 1]);
        assert!((walk.walked() - 0.5).abs() < 1e-6);
        assert_eq!(walk.view(WHOLE, near), WHOLE);
        assert_eq!(walk.card(), None);
        // On its way to the first: no card yet.
        let moving = at(WALK_SECONDS + MOVE_SECONDS / 2.0, &[0, 1]);
        assert_eq!(moving.card(), None);
        assert_ne!(moving.view(WHOLE, near), WHOLE);
        let first = at(WALK_SECONDS + MOVE_SECONDS + 1.0, &[0, 1]);
        assert_eq!(first.walked(), 1.0);
        assert_eq!(first.card(), Some(FighterId(0)));
        assert_eq!(first.view(WHOLE, near), near(FighterId(0)));
        let second = at(
            WALK_SECONDS + 2.0 * MOVE_SECONDS + CARD_SECONDS + 1.0,
            &[0, 1],
        );
        assert_eq!(second.card(), Some(FighterId(1)));
        assert_eq!(second.view(WHOLE, near), near(FighterId(1)));
        assert!(!second.is_over());
        // Back to the whole arena, then over.
        let end = at(
            WALK_SECONDS + 3.0 * MOVE_SECONDS + 2.0 * CARD_SECONDS,
            &[0, 1],
        );
        assert_eq!(end.card(), None);
        assert_eq!(end.view(WHOLE, near), WHOLE);
        assert!(end.is_over());
    }

    #[test]
    fn with_nobody_to_present_only_the_walk_is_left() {
        assert!(!at(WALK_SECONDS - 0.1, &[]).is_over());
        let over = at(WALK_SECONDS, &[]);
        assert!(over.is_over());
        assert_eq!(over.view(WHOLE, near), WHOLE);
    }

    #[test]
    fn a_close_up_leaves_room_on_the_right() {
        let shown = close_up(Vec2::new(10.0, 5.0));
        assert!(shown.contains(Vec2::new(10.0, 5.0)));
        assert!(shown.center().x > 10.0);
    }
}
