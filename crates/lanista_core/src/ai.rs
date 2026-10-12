use crate::combat::{Arena, Vec2};
use crate::fighter::FighterId;

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub enum Action {
    #[default]
    Wait,
    /// Move at full speed in this direction; its length is ignored.
    Move(Vec2),
}

/// Decides for one fighter; called every `TICKS_PER_DECISION` ticks.
pub trait CombatAi {
    fn decide(&mut self, arena: &Arena, me: FighterId) -> Action;
}
