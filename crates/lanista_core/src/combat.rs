use crate::combattant::{Combattant, Stats};
use crate::ia::{Action, CombatAi};

/// Duration of a tick, in seconds: 30 ticks per second.
pub const DT: f64 = 1.0 / 30.0;
/// The AIs decide every 6 ticks, 5 times per second.
pub const TICKS_PAR_DECISION: u32 = 6;
// ponytail: same radius and speed scale for everyone, move to `data/` when classes are loaded.
/// Fighter radius, in meters.
pub const RAYON: f64 = 0.5;
/// Meters per second per point of VIT.
pub const VITESSE_PAR_VIT: f64 = 0.1;

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Vec2 {
    pub x: f64,
    pub y: f64,
}

impl Vec2 {
    #[must_use]
    pub const fn new(x: f64, y: f64) -> Self {
        Self { x, y }
    }

    #[must_use]
    pub fn longueur(self) -> f64 {
        libm::sqrt(self.x * self.x + self.y * self.y)
    }
}

/// A fight in progress. `FighterId(i)` is the fighter at index `i` of `combattants`.
/// The floor spans `0..largeur` × `0..hauteur`, in meters.
#[derive(Debug, Clone, PartialEq)]
pub struct Arene {
    pub largeur: f64,
    pub hauteur: f64,
    /// Ticks elapsed since the start of the fight.
    pub tick: u32,
    pub combattants: Vec<Combattant>,
}

impl Arene {
    #[must_use]
    pub fn new(largeur: f64, hauteur: f64, combattants: Vec<Combattant>) -> Self {
        Self {
            largeur,
            hauteur,
            tick: 0,
            combattants,
        }
    }
}

/// Advances the fight by one tick. `ias[i]` controls `combattants[i]`.
///
/// # Panics
/// If there is not exactly one AI per fighter.
pub fn tick(arene: &mut Arene, ias: &mut [Box<dyn CombatAi>]) {
    assert_eq!(arene.combattants.len(), ias.len(), "one AI per fighter");
    if arene.tick.is_multiple_of(TICKS_PAR_DECISION) {
        for (i, ia) in ias.iter_mut().enumerate() {
            let combattant = &arene.combattants[i];
            if combattant.est_vivant() {
                let action = ia.decider(arene, combattant.id);
                arene.combattants[i].action = action;
            }
        }
    }
    deplacer(arene);
    separer(arene);
    borner(arene);
    arene.tick += 1;
}

/// Whether a hit uses AC against DC or AL against DL.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Portee {
    CorpsACorps,
    Distance,
}

/// Damage of a hit: attack × 100 / (100 + defense), rounded up so that
/// a nonzero attack always deals at least 1, whatever the defense.
#[must_use]
pub fn degats(attaquant: &Stats, defenseur: &Stats, portee: Portee) -> u32 {
    let (attaque, defense) = match portee {
        Portee::CorpsACorps => (attaquant.ac, defenseur.dc),
        Portee::Distance => (attaquant.al, defenseur.dl),
    };
    // u64: attack × 100 overflows u32 above 42 million.
    let degats = (u64::from(attaque) * 100).div_ceil(100 + u64::from(defense));
    // Never above the attack, so it fits in a u32.
    u32::try_from(degats).unwrap_or(attaque)
}

fn deplacer(arene: &mut Arene) {
    for c in arene.combattants.iter_mut().filter(|c| c.est_vivant()) {
        if let Action::Deplacer(direction) = c.action {
            let longueur = direction.longueur();
            if longueur > 0.0 {
                let pas = f64::from(c.stats.vit) * VITESSE_PAR_VIT * DT / longueur;
                c.position.x += direction.x * pas;
                c.position.y += direction.y * pas;
            }
        }
    }
}

/// Pushes overlapping living fighters apart, each by half the overlap.
// ponytail: one pass in index order, a crowded melee may keep small overlaps; iterate if visible.
fn separer(arene: &mut Arene) {
    let combattants = &mut arene.combattants;
    for i in 0..combattants.len() {
        for j in i + 1..combattants.len() {
            let (a, b) = (&combattants[i], &combattants[j]);
            if !a.est_vivant() || !b.est_vivant() {
                continue;
            }
            let ecart = Vec2::new(b.position.x - a.position.x, b.position.y - a.position.y);
            let distance = ecart.longueur();
            let chevauchement = 2.0 * RAYON - distance;
            if chevauchement <= 0.0 {
                continue;
            }
            // Same center: no direction to push along, pick one.
            let normale = if distance > 0.0 {
                Vec2::new(ecart.x / distance, ecart.y / distance)
            } else {
                Vec2::new(1.0, 0.0)
            };
            let demi = chevauchement / 2.0;
            combattants[i].position.x -= normale.x * demi;
            combattants[i].position.y -= normale.y * demi;
            combattants[j].position.x += normale.x * demi;
            combattants[j].position.y += normale.y * demi;
        }
    }
}

fn borner(arene: &mut Arene) {
    for c in &mut arene.combattants {
        c.position.x = c.position.x.clamp(RAYON, arene.largeur - RAYON);
        c.position.y = c.position.y.clamp(RAYON, arene.hauteur - RAYON);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::combattant::{ClasseId, FighterId, Stats};

    struct Fixe(Action);

    impl CombatAi for Fixe {
        fn decider(&mut self, _: &Arene, _: FighterId) -> Action {
            self.0
        }
    }

    fn combattant(id: u32, x: f64, y: f64) -> Combattant {
        let stats = Stats {
            pv: 100,
            ac: 10,
            al: 10,
            vit: 30,
            dc: 10,
            dl: 10,
        };
        Combattant::new(FighterId(id), ClasseId(0), stats, 50, 50, Vec2::new(x, y))
    }

    fn jouer(arene: &mut Arene, actions: &[Action], ticks: u32) {
        let mut ias: Vec<Box<dyn CombatAi>> = actions
            .iter()
            .map(|&a| Box::new(Fixe(a)) as Box<dyn CombatAi>)
            .collect();
        for _ in 0..ticks {
            tick(arene, &mut ias);
        }
    }

    #[test]
    fn avance_a_sa_vitesse() {
        let mut arene = Arene::new(20.0, 20.0, vec![combattant(0, 5.0, 5.0)]);
        // The direction's length must not change the speed.
        jouer(&mut arene, &[Action::Deplacer(Vec2::new(10.0, 0.0))], 30);
        // VIT 30 → 3 m/s, for one second.
        let position = arene.combattants[0].position;
        assert!((position.x - 8.0).abs() < 1e-9, "{position:?}");
        assert!((position.y - 5.0).abs() < 1e-9, "{position:?}");
    }

    #[test]
    fn deux_combattants_ne_se_chevauchent_pas() {
        let mut arene = Arene::new(
            20.0,
            20.0,
            vec![combattant(0, 5.0, 5.0), combattant(1, 8.0, 5.0)],
        );
        let actions = [
            Action::Deplacer(Vec2::new(1.0, 0.0)),
            Action::Deplacer(Vec2::new(-1.0, 0.0)),
        ];
        jouer(&mut arene, &actions, 60);
        let (a, b) = (arene.combattants[0].position, arene.combattants[1].position);
        let distance = Vec2::new(b.x - a.x, b.y - a.y).longueur();
        assert!(distance >= 2.0 * RAYON - 1e-9, "{distance}");
    }

    fn stats(ac: u32, al: u32, dc: u32, dl: u32) -> Stats {
        Stats {
            pv: 100,
            ac,
            al,
            vit: 30,
            dc,
            dl,
        }
    }

    #[test]
    fn la_defense_reduit_les_degats() {
        let attaquant = stats(100, 0, 0, 0);
        assert_eq!(
            degats(&attaquant, &stats(0, 0, 0, 0), Portee::CorpsACorps),
            100
        );
        assert_eq!(
            degats(&attaquant, &stats(0, 0, 100, 0), Portee::CorpsACorps),
            50
        );
        assert_eq!(
            degats(&attaquant, &stats(0, 0, 300, 0), Portee::CorpsACorps),
            25
        );
    }

    #[test]
    fn ac_contre_dc_et_al_contre_dl() {
        let attaquant = stats(100, 60, 0, 0);
        let defenseur = stats(0, 0, 100, 200);
        assert_eq!(degats(&attaquant, &defenseur, Portee::CorpsACorps), 50);
        assert_eq!(degats(&attaquant, &defenseur, Portee::Distance), 20);
    }

    #[test]
    fn la_defense_n_annule_jamais_les_degats() {
        let attaquant = stats(1, 0, 0, 0);
        let defenseur = stats(0, 0, u32::MAX, 0);
        assert_eq!(degats(&attaquant, &defenseur, Portee::CorpsACorps), 1);
        assert_eq!(degats(&attaquant, &defenseur, Portee::Distance), 0);
    }

    #[test]
    fn pas_de_debordement() {
        let attaquant = stats(u32::MAX, 0, 0, 0);
        assert_eq!(
            degats(&attaquant, &stats(0, 0, 0, 0), Portee::CorpsACorps),
            u32::MAX
        );
    }

    #[test]
    fn les_pv_s_arretent_a_zero() {
        let mut c = combattant(0, 5.0, 5.0);
        c.subir(30);
        assert_eq!(c.pv, 70);
        c.subir(1000);
        assert_eq!(c.pv, 0);
        assert!(!c.est_vivant());
    }

    #[test]
    fn reste_dans_l_arene() {
        let mut arene = Arene::new(10.0, 10.0, vec![combattant(0, 5.0, 5.0)]);
        jouer(&mut arene, &[Action::Deplacer(Vec2::new(-1.0, 1.0))], 300);
        assert_eq!(
            arene.combattants[0].position,
            Vec2::new(RAYON, 10.0 - RAYON)
        );
    }
}
