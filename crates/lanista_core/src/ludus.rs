//! The ludus: a plan of plots, those the club owns each free or holding a building,
//! built or being built; it buys the parcels next to them.
//! Its rules ([`LudusRules`]) are in `data/ludus.ron`; the week plays the player's
//! [`LudusMove`]s, then pays the upkeep of the buildings standing and moves the
//! constructions on ([`Ludus::week`]).

use serde::{Deserialize, Serialize};

/// The ludus's rules, from `data/ludus.ron`.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LudusRules {
    /// Plots in a row of the plan.
    pub columns: u32,
    pub rows: u32,
    /// The plot a club starts with, row by row from the top left.
    pub start: u32,
    /// The price of each parcel in turn, one more plot each; past the last, none is sold.
    pub parcel_prices: Vec<u32>,
    /// The share of its effect each copy of a building gives, in percent, the first
    /// copy first: as many copies at most as shares.
    pub copy_percents: Vec<u32>,
    pub families: Vec<Family>,
    /// `Site::building` is an index here.
    pub buildings: Vec<BuildingData>,
}

impl LudusRules {
    /// Plots in the plan, owned or not.
    #[must_use]
    pub fn plot_count(&self) -> usize {
        usize::try_from(self.columns * self.rows).unwrap_or(0)
    }
}

/// A family of buildings; three of them give the club its specialty.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Family {
    /// Names it in the language files (`family-<key>`).
    pub key: String,
    /// Names the club's title in the language files (`specialty-<key>`).
    pub specialty: String,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BuildingData {
    /// Names it in the language files (`building-<key>`).
    pub key: String,
    /// The key of its family, in `families`.
    pub family: String,
    /// Paid when the player decides to build it.
    pub price: u32,
    /// Paid each week once it stands, even into debt.
    pub upkeep: u32,
    /// Weeks of construction before it stands.
    pub weeks: u32,
    /// What its first copy does.
    pub effect: Effect,
}

/// What a building does, at the full share of its first copy; each acts on a rule
/// that already exists.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
pub enum Effect {
    /// Percent added to each stat's chance to improve at a training session.
    Training(u32),
    /// Percent added to the progression of fighters under 23.
    Youth(u32),
    /// Percentage points taken off the weeks left care keeps, with a doctor only.
    Healing(u32),
    /// Percent added to the form regained at rest.
    Rest(u32),
    /// Morale points gained each week.
    Morale(u32),
    /// Percent off the fighters' upkeep.
    Upkeep(u32),
    /// Morale points before a final or a title defense.
    BigFights(u32),
}

/// Buildings of a family standing that give the club its specialty.
pub const SPECIALTY_BUILDINGS: usize = 3;

/// The club's ludus.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Ludus {
    /// Each plot of the plan, row by row from the top left; `None` on one free or not owned.
    pub plots: Vec<Option<Site>>,
    /// Whether the club owns each plot: its starting one, then the parcels bought.
    #[serde(default)]
    pub owned: Vec<bool>,
}

/// A building on a plot.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Site {
    /// Its index in `LudusRules::buildings`.
    pub building: usize,
    /// Weeks of construction left; it stands, and acts, at 0.
    pub weeks_left: u32,
}

/// A decision of the week on the ludus, paid when made.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LudusMove {
    /// Starts building this building, by its index, on this free plot.
    Build { plot: usize, building: usize },
    /// Frees this plot, refunding nothing, a construction stopped as well.
    Demolish(usize),
    /// Buys this plot, next to one owned.
    BuyParcel(usize),
}

impl Ludus {
    /// A new club's ludus: its starting plot, free.
    #[must_use]
    pub fn new(rules: &LudusRules) -> Self {
        let cells = rules.plot_count();
        let mut owned = vec![false; cells];
        if let Some(start) = owned.get_mut(usize::try_from(rules.start).unwrap_or(usize::MAX)) {
            *start = true;
        }
        Self {
            plots: vec![None; cells],
            owned,
        }
    }

    /// The price of the next parcel; `None` once all are bought.
    #[must_use]
    pub fn parcel_price(&self, rules: &LudusRules) -> Option<u32> {
        let bought = self.owned.iter().filter(|&&o| o).count().saturating_sub(1);
        rules.parcel_prices.get(bought).copied()
    }

    /// Whether `plot` is a parcel to buy: not owned, next to a plot owned, side by side
    /// or one above the other.
    #[must_use]
    pub fn buyable(&self, plot: usize, rules: &LudusRules) -> bool {
        let columns = usize::try_from(rules.columns).unwrap_or(1).max(1);
        let owned = |p: usize| self.owned.get(p).copied().unwrap_or(false);
        let column = plot % columns;
        let above = plot.checked_sub(columns);
        let left = plot.checked_sub(1).filter(|_| column > 0);
        let right = (column + 1 < columns).then_some(plot + 1);
        let next = [above, Some(plot + columns), left, right];
        matches!(self.owned.get(plot), Some(false)) && next.into_iter().flatten().any(owned)
    }

    /// Copies of `building`, built or being built.
    #[must_use]
    pub fn copies(&self, building: usize) -> usize {
        self.plots
            .iter()
            .flatten()
            .filter(|s| s.building == building)
            .count()
    }

    /// Buildings of the family `family` (its key) standing, copies included; one
    /// being built does not count.
    #[must_use]
    pub fn family_standing(&self, family: &str, rules: &LudusRules) -> usize {
        self.plots
            .iter()
            .flatten()
            .filter(|s| s.weeks_left == 0)
            .filter(|s| {
                rules
                    .buildings
                    .get(s.building)
                    .is_some_and(|b| b.family == family)
            })
            .count()
    }

    /// Which copy of its building the one on `plot` is, from 0, in the order of the
    /// plots; `None` on a free plot. Its share of the effect is
    /// `LudusRules::copy_percents` at that rank.
    #[must_use]
    pub fn copy_rank(&self, plot: usize) -> Option<usize> {
        let site = self.plots.get(plot).copied().flatten()?;
        let before = self.plots[..plot].iter().flatten();
        Some(before.filter(|s| s.building == site.building).count())
    }

    /// What `m` costs; `None` if it may not be made now: a building on a plot not
    /// owned or not free, past its most copies, a parcel not next to the club's plots
    /// or when none is left, a plot to demolish with nothing on it.
    #[must_use]
    pub fn cost(&self, m: LudusMove, rules: &LudusRules) -> Option<u32> {
        match m {
            LudusMove::Build { plot, building } => {
                let data = rules.buildings.get(building)?;
                let owned = self.owned.get(plot).copied().unwrap_or(false);
                let free = owned && matches!(self.plots.get(plot), Some(None));
                (free && self.copies(building) < rules.copy_percents.len()).then_some(data.price)
            }
            LudusMove::Demolish(plot) => self.plots.get(plot)?.map(|_| 0),
            LudusMove::BuyParcel(plot) => self
                .buyable(plot, rules)
                .then(|| self.parcel_price(rules))
                .flatten(),
        }
    }

    /// Makes `m` if it may be made with `money`; returns what it cost.
    pub fn make(&mut self, m: LudusMove, money: i64, rules: &LudusRules) -> Option<u32> {
        let cost = self.cost(m, rules).filter(|&c| i64::from(c) <= money)?;
        match m {
            LudusMove::Build { plot, building } => {
                self.plots[plot] = Some(Site {
                    building,
                    weeks_left: rules.buildings[building].weeks,
                });
            }
            LudusMove::Demolish(plot) => self.plots[plot] = None,
            LudusMove::BuyParcel(plot) => self.owned[plot] = true,
        }
        Some(cost)
    }

    /// The upkeep of the buildings standing, then a week of work on each construction;
    /// returns the upkeep and the plots whose construction ended this week.
    pub fn week(&mut self, rules: &LudusRules) -> (u32, Vec<usize>) {
        let mut upkeep = 0;
        let mut finished = Vec::new();
        for (i, site) in self.plots.iter_mut().enumerate() {
            let Some(site) = site else { continue };
            if site.weeks_left == 0 {
                let cost = rules.buildings.get(site.building).map_or(0, |b| b.upkeep);
                upkeep += cost;
            } else {
                site.weeks_left -= 1;
                if site.weeks_left == 0 {
                    finished.push(i);
                }
            }
        }
        (upkeep, finished)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A plan of two rows of two, starting at the top left.
    fn rules() -> LudusRules {
        let building = |key: &str, price, weeks| BuildingData {
            key: key.to_owned(),
            family: "entrainement".to_owned(),
            price,
            upkeep: 10,
            weeks,
            effect: Effect::Training(10),
        };
        LudusRules {
            columns: 2,
            rows: 2,
            start: 0,
            parcel_prices: vec![100, 300],
            copy_percents: vec![100, 60],
            families: Vec::new(),
            buildings: vec![building("a", 50, 2), building("b", 80, 0)],
        }
    }

    #[test]
    fn builds_pays_and_stands_after_its_weeks() {
        let rules = rules();
        let mut ludus = Ludus::new(&rules);
        let build = |plot| LudusMove::Build { plot, building: 0 };
        assert_eq!(ludus.make(build(0), 49, &rules), None);
        assert_eq!(ludus.make(build(0), 50, &rules), Some(50));
        // Not on a plot taken, nor one not owned, nor one missing.
        assert_eq!(ludus.make(build(0), 1000, &rules), None);
        assert_eq!(ludus.make(build(1), 1000, &rules), None);
        assert_eq!(ludus.make(build(4), 1000, &rules), None);
        // No upkeep while it is built.
        assert_eq!(ludus.week(&rules), (0, vec![]));
        assert_eq!(ludus.family_standing("entrainement", &rules), 0);
        assert_eq!(ludus.week(&rules), (0, vec![0]));
        assert_eq!(ludus.week(&rules), (10, vec![]));
        assert_eq!(ludus.family_standing("entrainement", &rules), 1);
        assert_eq!(ludus.family_standing("soins", &rules), 0);
        // A building of no weeks stands at once.
        ludus.make(LudusMove::Demolish(0), 0, &rules).unwrap();
        ludus
            .make(
                LudusMove::Build {
                    plot: 0,
                    building: 1,
                },
                80,
                &rules,
            )
            .unwrap();
        assert_eq!(ludus.week(&rules), (10, vec![]));
    }

    #[test]
    fn parcels_next_to_the_plots_owned_are_sold_dearer_each() {
        let rules = rules();
        let mut ludus = Ludus::new(&rules);
        let buy = LudusMove::BuyParcel;
        // Not the plot owned, nor the corner facing it.
        assert!(!ludus.buyable(0, &rules) && !ludus.buyable(3, &rules));
        assert!(ludus.buyable(1, &rules) && ludus.buyable(2, &rules));
        assert_eq!(ludus.make(buy(3), 1000, &rules), None);
        assert_eq!(ludus.make(buy(1), 99, &rules), None);
        assert_eq!(ludus.make(buy(1), 100, &rules), Some(100));
        assert!(ludus.buyable(3, &rules));
        assert_eq!(ludus.make(buy(3), 300, &rules), Some(300));
        // No price left.
        assert_eq!(ludus.parcel_price(&rules), None);
        assert_eq!(ludus.make(buy(2), 1000, &rules), None);
    }

    #[test]
    fn copies_are_capped_and_ranked_in_plan_order() {
        let mut rules = rules();
        rules.parcel_prices = vec![0, 0, 0];
        let mut ludus = Ludus::new(&rules);
        for plot in 1..4 {
            ludus.make(LudusMove::BuyParcel(plot), 0, &rules).unwrap();
        }
        let build = |plot| LudusMove::Build { plot, building: 0 };
        ludus.make(build(0), 1000, &rules).unwrap();
        ludus.make(build(1), 1000, &rules).unwrap();
        assert_eq!(ludus.make(build(2), 1000, &rules), None);
        // Demolishing frees a copy, for nothing back.
        assert_eq!(ludus.make(LudusMove::Demolish(1), 0, &rules), Some(0));
        assert_eq!(ludus.make(LudusMove::Demolish(1), 0, &rules), None);
        assert_eq!(ludus.make(build(2), 1000, &rules), Some(50));
        assert_eq!(ludus.copy_rank(0), Some(0));
        assert_eq!(ludus.copy_rank(1), None);
        assert_eq!(ludus.copy_rank(2), Some(1));
    }
}
