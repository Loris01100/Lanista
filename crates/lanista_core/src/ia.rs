use crate::combat::{Arene, Vec2};
use crate::combattant::FighterId;

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub enum Action {
    #[default]
    Attendre,
    /// Move at full speed in this direction; its length is ignored.
    Deplacer(Vec2),
}

/// Decides for one fighter; called every `TICKS_PAR_DECISION` ticks.
pub trait CombatAi {
    fn decider(&mut self, arene: &Arene, moi: FighterId) -> Action;
}
