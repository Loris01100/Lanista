use crate::combattant::Combattant;

/// A fight in progress. `FighterId(i)` is the fighter at index `i` of `combattants`.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Arene {
    pub combattants: Vec<Combattant>,
}
