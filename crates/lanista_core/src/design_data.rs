//! Design data (classes, spells, combat rules, fighter rules, calendar, economy, colosseums, cultures,
//! statistical model) read from the RON files of `data/`.

use std::collections::BTreeMap;

use serde::Deserialize;

use crate::calendar::CalendarRules;
use crate::colosseum::{Continent, WorldRules};
use crate::combat::{
    ArenaKind, ArenaLayout, CombatRules, DamageKind, Element, Pillar, RADIUS, Range,
};
use crate::economy::EconomyRules;
use crate::fighter::ClassId;
use crate::fighter::appearance::AppearanceRules;
use crate::fighter::care::CareRules;
use crate::fighter::career::CareerRules;
use crate::fighter::equipment::{EaseRules, EquipmentRules, OffHand};
use crate::fighter::fame::FameRules;
use crate::fighter::form::FormRules;
use crate::fighter::illness::IllnessRules;
use crate::fighter::injury::InjuryRules;
use crate::fighter::morale::MoraleRules;
use crate::fighter::personality::PersonalityRules;
use crate::fighter::progression::ProgressionRules;
use crate::fighter::retirement::RetirementRules;
use crate::fighter::traits::TraitRules;
use crate::learned::{ClassModels, MODELS_DIR};
use crate::melee::MeleeRules;
use crate::names::Culture;
use crate::relations::{MAX_SCORE, Standing};
use crate::stat_model::StatModels;

/// Everything read from `data/`. `ClassId(i)` is the class at index `i` of `classes`.
#[derive(Debug, Clone, PartialEq)]
pub struct DesignData {
    pub classes: Vec<ClassData>,
    pub combat: CombatRules,
    pub fighter: FighterRules,
    pub calendar: CalendarRules,
    pub economy: EconomyRules,
    pub world: WorldRules,
    /// The world's cultures, from `data/names.ron`.
    pub cultures: Vec<Culture>,
    pub stat_model: StatModels,
    /// Each class's learned models, in the order of `classes`, read by
    /// [`DesignData::load`] from `assets/ai/`; none from [`DesignData::from_ron`].
    pub models: Vec<ClassModels>,
}

/// Rules of the fighter's physical state, from `data/fighter.ron`.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FighterRules {
    pub form: FormRules,
    pub injury: InjuryRules,
    pub illness: IllnessRules,
    pub care: CareRules,
    pub personality: PersonalityRules,
    pub traits: TraitRules,
    pub morale: MoraleRules,
    pub fame: FameRules,
    pub progression: ProgressionRules,
    pub retirement: RetirementRules,
    pub career: CareerRules,
    pub equipment: EquipmentRules,
    pub ease: EaseRules,
    pub appearance: AppearanceRules,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ClassData {
    /// Names it in the language files (`class-<key>`) and the tools.
    pub key: String,
    pub max_mana: u32,
    pub max_endurance: u32,
    /// Mana, then endurance, recovered each second of a fight.
    pub mana_per_second: u32,
    pub endurance_per_second: u32,
    /// Elements of the floor it carries at most (`SpellData::gathers`); none if left out.
    #[serde(default)]
    pub max_elements: u32,
    /// The spells of the first and third slots; the second comes from the loadout.
    pub spells: Vec<SpellData>,
    /// Its loadouts, two for each class; none for a class built by hand.
    pub loadouts: Vec<LoadoutData>,
}

impl ClassData {
    /// Loadout `i`, or the first one if there is no such loadout.
    #[must_use]
    pub fn loadout(&self, i: usize) -> Option<&LoadoutData> {
        self.loadouts.get(i).or(self.loadouts.first())
    }

    /// A fighter's spell slots in loadout `i` (see [`ClassData::loadout`]): the class's
    /// spells, the loadout's in the second slot if it brings one, its mine spells laying
    /// its `mines_per_cast`, its shots as its weapon fires them (`LoadoutData::shot`).
    #[must_use]
    pub fn spells_in(&self, i: usize) -> Vec<SpellData> {
        let mut spells = self.spells.clone();
        if let Some(loadout) = self.loadout(i) {
            if let Some(spell) = &loadout.spell {
                spells.insert(spells.len().min(1), spell.clone());
            }
            for spell in spells.iter_mut().filter(|s| s.mine) {
                spell.blows = loadout.mines_per_cast;
            }
            spells = spells.into_iter().map(|s| loadout.shot(s)).collect();
        }
        spells
    }
}

/// One of a class's equipments, chosen for each fighter.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LoadoutData {
    /// Names it in the language files (`loadout-<key>`).
    pub key: String,
    /// Its weapon type, a key of `weapons` in `data/combat.ron`.
    pub weapon: String,
    pub off_hand: OffHand,
    /// The spell of the second slot; none if left out, for a class whose loadouts share
    /// all its spells.
    #[serde(default)]
    pub spell: Option<SpellData>,
    /// Mines each of the fighter's mine spells lays per cast (`SpellData::blows`); 1 if
    /// left out.
    #[serde(default = "one")]
    pub mines_per_cast: u32,
    /// Percentage of its speed the fighter keeps in it (a heavy bow slows it down); 100
    /// if left out.
    #[serde(default = "full")]
    pub speed_percent: u32,
    /// Percentages of the power and the cooldown of its shots, the ranged spells that
    /// hit, auto attack included (`LoadoutData::shot`); 100 if left out.
    #[serde(default = "full")]
    pub shot_power_percent: u32,
    #[serde(default = "full")]
    pub shot_cooldown_percent: u32,
    /// Its weapon's auto attack, set at loading from [`CombatRules::weapons`] (through
    /// [`LoadoutData::shot`]); none for a loadout built by hand.
    #[serde(skip)]
    pub auto_attack: Option<SpellData>,
}

impl LoadoutData {
    /// `spell` as this loadout fires it: a shot, a ranged spell that hits (neither
    /// gathering nor laying a mine), with `shot_power_percent` of its power and
    /// `shot_cooldown_percent` of its cooldown, rounded up; any other spell unchanged.
    #[must_use]
    pub fn shot(&self, spell: SpellData) -> SpellData {
        if spell.range != Range::Ranged || spell.mine || spell.gathers > 0 {
            return spell;
        }
        let scale = |value: u32, percent: u32| {
            u32::try_from((u64::from(value) * u64::from(percent)).div_ceil(100)).unwrap_or(u32::MAX)
        };
        SpellData {
            power: scale(spell.power, self.shot_power_percent),
            cooldown: scale(spell.cooldown, self.shot_cooldown_percent),
            ..spell
        }
    }
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SpellData {
    /// Names it in the fight logs and in the spells the combat screen lists.
    pub name: String,
    /// Melee uses melee attack against melee defense, ranged the ranged ones.
    pub range: Range,
    /// Farthest distance between the two fighters' edges, in meters.
    pub reach: f64,
    /// Damage, as a percentage of the `damage` formula's result.
    pub power: u32,
    /// Ticks before the spell can be cast again (30 ticks per second).
    pub cooldown: u32,
    pub cost: Cost,
    /// What it hits with, which sets how often it stuns.
    pub damage_kind: DamageKind,
    /// Percentage of the hit chance it keeps (`combat::spell_hit_percent`): below 100 for
    /// a clumsy blow; 100 if left out.
    #[serde(default = "full_accuracy")]
    pub accuracy_percent: u32,
    /// From its how-manyth hit on the same target a cutting hit of this spell severs the
    /// part it lands on for sure, if it can be (`combat::severs`); never if left out.
    #[serde(default)]
    pub sure_cut_hits: Option<u32>,
    /// Chance, in percent, that a hit of it makes the part it lands on bleed, if that
    /// part does not already (`combat::BleedRules`); never if left out.
    #[serde(default)]
    pub bleed_percent: u32,
    /// The accuracy a hit of it takes off its target for a while, adding up with the
    /// others; none if left out.
    #[serde(default)]
    pub accuracy_drop: Option<AccuracyDrop>,
    /// Elements of the arena's floor it adds to its caster's reserve, up to the class's
    /// `max_elements`: a spell without target nor damage, cast on its caster (`combat`'s
    /// `gather`); 0, a spell like the others, if left out.
    #[serde(default)]
    pub gathers: u32,
    /// What it hits like when it throws each element, in place of its own `power`,
    /// `damage_kind` and `accuracy_drop`, set by `Arena::with_layout`; nothing if left out.
    #[serde(default)]
    pub thrown: BTreeMap<Element, Thrown>,
    /// Lays a mine at its target's feet instead of hitting it: the mine blasts with this
    /// spell's power, range and damage kind (`combat::MineRules`); false if left out.
    #[serde(default)]
    pub mine: bool,
    /// Blows of one cast, all landing or none on a single hit roll, each with its own
    /// part, effects and the same damage; for a mine spell, the mines it lays, set by
    /// its loadout (`LoadoutData::mines_per_cast`). 1 if left out.
    #[serde(default = "one")]
    pub blows: u32,
    /// Never misses, whatever the target's speed or the caster's lost accuracy
    /// (`combat::spell_hit_percent`); false if left out.
    #[serde(default)]
    pub sure_hit: bool,
    /// A ranged spell that falls from above: pillars do not block it
    /// (`combat::shot_blocked`); false if left out.
    #[serde(default)]
    pub over_obstacles: bool,
    /// In meters: it also hits every other fighter standing this close to its target's
    /// center, but its caster and its caster's ally, each on its own hit roll; 0, its
    /// target alone, if left out.
    #[serde(default)]
    pub area: f64,
    /// For a mine spell: its mine is a trap, unseen by the caster's opponents, that
    /// catches the first fighter touching it instead of blasting (`combat::Trap`); none,
    /// a mine that blasts, if left out.
    #[serde(default)]
    pub trap: Option<TrapData>,
}

/// What a trap does to the fighter it catches (`SpellData::trap`): it holds it, neither
/// acting nor moving but still hit, for `ticks`, its hits taking `damage_percent` more;
/// the first hit that takes HP frees it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TrapData {
    /// 30 per second.
    pub ticks: u32,
    pub damage_percent: u32,
}

/// How a spell hits with an element of the floor (`SpellData::thrown`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Thrown {
    pub power: u32,
    pub damage_kind: DamageKind,
    #[serde(default)]
    pub accuracy_drop: Option<AccuracyDrop>,
}

/// A share of a fighter's accuracy lost for a while (`fighter::Effect::Blinded`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AccuracyDrop {
    pub percent: u32,
    /// 30 per second.
    pub ticks: u32,
}

const fn full_accuracy() -> u32 {
    100
}

const fn one() -> u32 {
    1
}

const fn full() -> u32 {
    100
}

/// What casting a spell spends, all of it needed; what is left out costs nothing.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Cost {
    pub mana: u32,
    pub endurance: u32,
    /// Elements of the floor, from the caster's reserve (`Fighter::elements`).
    pub elements: u32,
}

const CLASSES_FILE: &str = "data/classes.ron";
/// Images of a fighter's spritesheet: 4 animations of 4 (`docs/Formats_graphiques.md`).
pub const APPEARANCE_FRAMES: usize = 16;
const COMBAT_FILE: &str = "data/combat.ron";
const FIGHTER_FILE: &str = "data/fighter.ron";
const CALENDAR_FILE: &str = "data/calendar.ron";
const ECONOMY_FILE: &str = "data/economy.ron";
const WORLD_FILE: &str = "data/world.ron";
const NAMES_FILE: &str = "data/names.ron";
/// Written by the `calibrate` tool.
pub const STAT_MODEL_FILE: &str = "data/stat_model.ron";

/// Why the design data could not be loaded; `file` is the file at fault.
#[derive(Debug)]
pub enum LoadError {
    Io {
        file: &'static str,
        source: std::io::Error,
    },
    /// Not valid RON, or not the expected fields.
    Ron {
        file: &'static str,
        source: ron::error::SpannedError,
    },
    /// Parsed, but a value would break the game, e.g. a range whose lowest is above its highest.
    Invalid {
        file: &'static str,
        field: &'static str,
        problem: &'static str,
    },
}

impl std::fmt::Display for LoadError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io { file, source } => write!(f, "{file}: {source}"),
            Self::Ron { file, source } => write!(f, "{file}: {source}"),
            Self::Invalid {
                file,
                field,
                problem,
            } => write!(f, "{file}: {field}: {problem}"),
        }
    }
}

impl std::error::Error for LoadError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io { source, .. } => Some(source),
            Self::Ron { source, .. } => Some(source),
            Self::Invalid { .. } => None,
        }
    }
}

impl DesignData {
    /// Reads the content of `data/classes.ron`, `data/combat.ron`, `data/fighter.ron`,
    /// `data/calendar.ron`, `data/economy.ron`, `data/world.ron`, `data/names.ron` and
    /// `data/stat_model.ron`.
    ///
    /// # Errors
    /// If a text is not valid RON or does not match the expected fields, or if a value
    /// would make the game panic later (no class, a range upside down, a chance outside 0 to 1,
    /// a season of no week, no free colosseum, a culture without names, a model's chance outside
    /// 0 to 1).
    #[expect(clippy::too_many_arguments, reason = "one text for each file of data/")]
    pub fn from_ron(
        classes: &str,
        combat: &str,
        fighter: &str,
        calendar: &str,
        economy: &str,
        world: &str,
        names: &str,
        stat_model: &str,
    ) -> Result<Self, LoadError> {
        let mut data = Self {
            classes: ron::from_str(classes).map_err(|source| LoadError::Ron {
                file: CLASSES_FILE,
                source,
            })?,
            combat: ron::from_str(combat).map_err(|source| LoadError::Ron {
                file: COMBAT_FILE,
                source,
            })?,
            fighter: ron::from_str(fighter).map_err(|source| LoadError::Ron {
                file: FIGHTER_FILE,
                source,
            })?,
            calendar: ron::from_str(calendar).map_err(|source| LoadError::Ron {
                file: CALENDAR_FILE,
                source,
            })?,
            economy: ron::from_str(economy).map_err(|source| LoadError::Ron {
                file: ECONOMY_FILE,
                source,
            })?,
            world: ron::from_str(world).map_err(|source| LoadError::Ron {
                file: WORLD_FILE,
                source,
            })?,
            cultures: ron::from_str(names).map_err(|source| LoadError::Ron {
                file: NAMES_FILE,
                source,
            })?,
            stat_model: ron::from_str(stat_model).map_err(|source| LoadError::Ron {
                file: STAT_MODEL_FILE,
                source,
            })?,
            models: Vec::new(),
        };
        for loadout in data.classes.iter_mut().flat_map(|c| &mut c.loadouts) {
            let attack = data.combat.weapons.get(&loadout.weapon);
            let attack = attack.cloned().ok_or(LoadError::Invalid {
                file: CLASSES_FILE,
                field: "weapon",
                problem: "a weapon not in the weapons of data/combat.ron",
            })?;
            loadout.auto_attack = Some(loadout.shot(attack));
        }
        data.validate()?;
        if let Some(field) = data.stat_model.invalid_field() {
            return Err(LoadError::Invalid {
                file: STAT_MODEL_FILE,
                field,
                problem: "a chance outside 0 to 1, a weight not a number or an empty table",
            });
        }
        Ok(data)
    }

    /// Reads the files of `data/` at run time, so balancing needs no rebuild.
    ///
    /// # Errors
    /// If a file cannot be read, or [`DesignData::from_ron`] rejects it.
    pub fn load() -> Result<Self, LoadError> {
        let read = |file: &'static str| {
            std::fs::read_to_string(Self::path(file))
                .map_err(|source| LoadError::Io { file, source })
        };
        let mut data = Self::from_ron(
            &read(CLASSES_FILE)?,
            &read(COMBAT_FILE)?,
            &read(FIGHTER_FILE)?,
            &read(CALENDAR_FILE)?,
            &read(ECONOMY_FILE)?,
            &read(WORLD_FILE)?,
            &read(NAMES_FILE)?,
            &read(STAT_MODEL_FILE)?,
        )?;
        data.models = ClassModels::load(MODELS_DIR, &data);
        Ok(data)
    }

    /// Where [`DesignData::load`] reads `file`, one of the `data/` files: in the
    /// working directory (the game's folder, or the workspace root), else in the
    /// source tree, for the tests run from their crate's folder.
    #[must_use]
    pub fn path(file: &str) -> String {
        if std::path::Path::new(file).exists() {
            return file.to_owned();
        }
        format!("{}/../../{file}", env!("CARGO_MANIFEST_DIR"))
    }

    /// Rejects the values the rest of the game would panic on.
    fn validate(&self) -> Result<(), LoadError> {
        if self.classes.is_empty() {
            return Err(LoadError::Invalid {
                file: CLASSES_FILE,
                field: "classes",
                problem: "no class",
            });
        }
        if self.economy.max_fighters == 0 {
            return Err(LoadError::Invalid {
                file: ECONOMY_FILE,
                field: "max_fighters",
                problem: "a club of no fighter",
            });
        }
        self.validate_world()?;
        self.validate_relations()?;
        self.validate_melee_prizes()?;
        let market = &self.economy.market;
        if market.max_free_agents < market.min_free_agents {
            return Err(LoadError::Invalid {
                file: ECONOMY_FILE,
                field: "market",
                problem: "fewer free agents at most than at least",
            });
        }
        if self.calendar.season_weeks() == 0 {
            return Err(LoadError::Invalid {
                file: CALENDAR_FILE,
                field: "season",
                problem: "no week",
            });
        }
        self.validate_combat()?;
        self.validate_fighter()
    }

    /// The world's colosseums and clubs, and the cultures that name them.
    /// The standings run from the highest down to −100, each with a price.
    fn validate_relations(&self) -> Result<(), LoadError> {
        let invalid = |problem| LoadError::Invalid {
            file: WORLD_FILE,
            field: "relations.standings",
            problem,
        };
        let standings = &self.world.relations.standings;
        let froms = Standing::ALL.map(|s| standings.get(s).from);
        if froms.windows(2).any(|w| w[0] <= w[1]) || froms[4] > -MAX_SCORE {
            return Err(invalid("not from the highest down to -100"));
        }
        if Standing::ALL
            .iter()
            .any(|&s| standings.get(s).price_percent == 0)
        {
            return Err(invalid("a price of nothing"));
        }
        if self.world.relations.rivalry.ends_from < standings.tense.from {
            return Err(invalid("a rivalry ending among the rival scores"));
        }
        Ok(())
    }

    /// The last paid place of a melee pays each colosseum's entry fee, the journey apart.
    fn validate_melee_prizes(&self) -> Result<(), LoadError> {
        let paid = (usize::MAX, self.combat.melee.paid_places);
        let last_paid = |c: &crate::colosseum::ColosseumData| c.purses.melee_receipts(paid.1, paid);
        if self
            .world
            .colosseums
            .iter()
            .any(|c| last_paid(c) < c.purses.entry_fee)
        {
            return Err(LoadError::Invalid {
                file: WORLD_FILE,
                field: "melee_prize",
                problem: "a last paid place of a melee that does not pay the entry fee",
            });
        }
        Ok(())
    }

    fn validate_world(&self) -> Result<(), LoadError> {
        let world = |field, problem| LoadError::Invalid {
            file: WORLD_FILE,
            field,
            problem,
        };
        let clubs = &self.world.clubs;
        let homes: Vec<Continent> = clubs
            .continents
            .iter()
            .filter(|(w, _)| *w > 0)
            .map(|&(_, c)| c)
            .collect();
        let free_at = |home: Continent| {
            self.world
                .colosseums
                .iter()
                .any(|c| c.continent == home && c.purses.entry_fee == 0)
        };
        if !homes.iter().all(|&home| free_at(home)) {
            return Err(world(
                "colosseums",
                "a continent of clubs without a colosseum free to enter",
            ));
        }
        if self.world.colosseums.iter().any(|c| c.residents == 0) {
            return Err(world("residents", "a colosseum without opponents"));
        }
        if clubs.count == 0 {
            return Err(world("clubs.count", "no rival club to own the residents"));
        }
        if clubs.max < clubs.count {
            return Err(world(
                "clubs.max",
                "fewer than the clubs the world opens with",
            ));
        }
        if clubs.ruined_seasons == 0 {
            return Err(world(
                "clubs.ruined_seasons",
                "clubs closed before being ruined",
            ));
        }
        let weighed = |weights: &mut dyn Iterator<Item = u32>| weights.sum::<u32>() > 0;
        if !weighed(&mut clubs.profiles.iter().map(|(w, _)| *w)) {
            return Err(world("clubs.profiles", "no weight"));
        }
        if !weighed(&mut clubs.wealth.iter().map(|(w, _)| *w)) {
            return Err(world("clubs.wealth", "no weight"));
        }
        if homes.is_empty() {
            return Err(world("clubs.continents", "no weight"));
        }
        if clubs
            .cultures
            .iter()
            .any(|(_, key)| self.culture(key).is_none())
        {
            return Err(world(
                "clubs.cultures",
                "a culture missing from data/names.ron",
            ));
        }
        let cultured = |home: Continent| {
            clubs
                .cultures
                .iter()
                .any(|(w, key)| *w > 0 && self.culture(key).is_some_and(|c| c.continent == home))
        };
        if !homes.iter().all(|&home| cultured(home)) {
            return Err(world(
                "clubs.cultures",
                "a continent of clubs without a culture",
            ));
        }
        let unnamed =
            |c: &Culture| c.names.is_empty() || c.clubs.is_empty() || c.fighters.is_empty();
        if self.cultures.iter().any(unnamed) {
            return Err(LoadError::Invalid {
                file: NAMES_FILE,
                field: "cultures",
                problem: "no name, pattern or fighter's name",
            });
        }
        if self.world.roster.max < self.world.roster.wanted {
            return Err(world("roster", "fewer places than fighters wanted"));
        }
        Ok(())
    }

    fn validate_appearance(&self) -> Result<(), LoadError> {
        let looks = &self.fighter.appearance;
        let empty = [
            ("appearance.hair", looks.hair.is_empty()),
            ("appearance.skins", looks.skins.is_empty()),
            ("appearance.hair_tints", looks.hair_tints.is_empty()),
            ("appearance.tunics", looks.tunics.is_empty()),
        ];
        if let Some(&(field, _)) = empty.iter().find(|(_, empty)| *empty) {
            return Err(LoadError::Invalid {
                file: FIGHTER_FILE,
                field,
                problem: "empty",
            });
        }
        if looks.layers.is_empty() || looks.classes.values().any(Vec::is_empty) {
            return Err(LoadError::Invalid {
                file: FIGHTER_FILE,
                field: "appearance.layers",
                problem: "a look without layers",
            });
        }
        let is_class = |key: &String| self.classes.iter().any(|c| &c.key == key);
        if !looks.classes.keys().all(is_class) {
            return Err(LoadError::Invalid {
                file: FIGHTER_FILE,
                field: "appearance.classes",
                problem: "not the key of a class",
            });
        }
        if looks.head_frames.len() != APPEARANCE_FRAMES {
            return Err(LoadError::Invalid {
                file: FIGHTER_FILE,
                field: "appearance.head_frames",
                problem: "not one for each of the spritesheet's 16 images",
            });
        }
        Ok(())
    }

    fn validate_fighter(&self) -> Result<(), LoadError> {
        self.validate_appearance()?;
        let rules = &self.fighter;
        let equipment = &rules.equipment;
        // A refused armour, a second weapon or a loadout one is not at ease with must not
        // make the fighter stronger.
        for (field, percent) in [
            ("ease.malus_percent", rules.ease.malus_percent),
            (
                "equipment.off_hand_attack_percent",
                equipment.off_hand_attack_percent,
            ),
            (
                "equipment.armourless_defense_percent",
                equipment.armourless_defense_percent,
            ),
        ] {
            if percent > 100 {
                return Err(LoadError::Invalid {
                    file: FIGHTER_FILE,
                    field,
                    problem: "above 100",
                });
            }
        }
        let ranges = [
            ("career.start_age", rules.career.start_age),
            (
                "injury.healing_weeks.light",
                rules.injury.healing_weeks.light,
            ),
            (
                "injury.healing_weeks.medium",
                rules.injury.healing_weeks.medium,
            ),
            (
                "injury.healing_weeks.severe",
                rules.injury.healing_weeks.severe,
            ),
            ("illness.weeks.mild", rules.illness.weeks.mild),
            ("illness.weeks.moderate", rules.illness.weeks.moderate),
            ("illness.weeks.severe", rules.illness.weeks.severe),
            ("illness.worsening_weeks", rules.illness.worsening_weeks),
            (
                "progression.potential_headroom_percent",
                rules.progression.potential_headroom_percent,
            ),
        ];
        let chances = [
            ("injury.chance_per_fight", rules.injury.chance_per_fight),
            ("illness.chance_per_week", rules.illness.chance_per_week),
            ("illness.worsening_chance", rules.illness.worsening_chance),
            (
                "injury.fatal_blow_survival",
                rules.injury.fatal_blow_survival,
            ),
            ("injury.stun_wound_chance", rules.injury.stun_wound_chance),
        ];
        let invalid = |field, problem| LoadError::Invalid {
            file: FIGHTER_FILE,
            field,
            problem,
        };
        if let Some(&(field, _)) = ranges.iter().find(|(_, (min, max))| min > max) {
            return Err(invalid(field, "lowest above highest"));
        }
        if let Some(&(field, _)) = chances
            .iter()
            .find(|(_, chance)| !(0.0..=1.0).contains(chance))
        {
            return Err(invalid(field, "not a chance from 0 to 1"));
        }
        // A negative factor would turn a tired fighter's chances negative.
        if rules.form.risk_factor.is_nan() || rules.form.risk_factor < 0.0 {
            return Err(invalid("form.risk_factor", "below 0"));
        }
        Ok(())
    }

    fn validate_combat(&self) -> Result<(), LoadError> {
        let accuracy = &self.combat.accuracy;
        if accuracy.min_percent > accuracy.max_percent {
            return Err(LoadError::Invalid {
                file: COMBAT_FILE,
                field: "accuracy",
                problem: "lowest above highest",
            });
        }
        let (cut, hits) = (&self.combat.cut, &self.combat.hits);
        if hits.arm.saturating_add(hits.leg) == 0 {
            return Err(LoadError::Invalid {
                file: COMBAT_FILE,
                field: "hits",
                problem: "no limb to sever",
            });
        }
        if cut.fatal_head_percent > 100 {
            return Err(LoadError::Invalid {
                file: COMBAT_FILE,
                field: "cut.fatal_head_percent",
                problem: "above 100",
            });
        }
        if let Some((field, problem)) = melee_problem(&self.combat.melee) {
            return Err(LoadError::Invalid {
                file: COMBAT_FILE,
                field,
                problem,
            });
        }
        if let Some((field, problem)) = self.classes.iter().find_map(class_problem) {
            return Err(LoadError::Invalid {
                file: CLASSES_FILE,
                field,
                problem,
            });
        }
        for kind in ArenaKind::ALL {
            if let Some(problem) = layout_problem(self.combat.arenas.get(kind)) {
                return Err(LoadError::Invalid {
                    file: COMBAT_FILE,
                    field: "arenas",
                    problem,
                });
            }
        }
        Ok(())
    }

    #[must_use]
    pub fn class(&self, id: ClassId) -> Option<&ClassData> {
        self.classes.get(usize::try_from(id.0).ok()?)
    }

    /// The culture `key` of `data/names.ron`.
    #[must_use]
    pub fn culture(&self, key: &str) -> Option<&Culture> {
        self.cultures.iter().find(|c| c.key == key)
    }
}

/// The field and the problem of melee rules that would break a melee: fewer than two
/// fighters, no room or time, a chance above 100, or pacts kept by the last two, who
/// would then never fight.
fn melee_problem(rules: &MeleeRules) -> Option<(&'static str, &'static str)> {
    let pacts = &rules.pacts;
    if rules.max_fighters < 2 {
        Some(("melee.max_fighters", "below 2"))
    } else if rules.square_meters_per_fighter.is_nan() || rules.square_meters_per_fighter <= 0.0 {
        Some(("melee.square_meters_per_fighter", "not above 0"))
    } else if rules.max_ticks == 0 {
        Some(("melee.max_ticks", "0"))
    } else if pacts.distance.is_nan() {
        Some(("melee.pacts.distance", "not a number"))
    } else if [
        pacts.form_percent_most_timid,
        pacts.form_percent_boldest,
        pacts.betrayal_percent_boldest,
    ]
    .iter()
    .any(|&p| p > 100)
    {
        Some(("melee.pacts", "a chance above 100"))
    } else if pacts.break_fighting < 2 {
        Some(("melee.pacts.break_fighting", "below 2"))
    } else if rules.elimination.every_ticks == 0 {
        Some(("melee.elimination.every_ticks", "0"))
    } else if rules.elimination.final_fighters < 1 {
        Some(("melee.elimination.final_fighters", "below 1"))
    } else {
        None
    }
}

/// The field and the problem of a class that would break fights: a loadout of no speed,
/// a spell's area not a number or below 0, or a trap on a spell that lays no mine.
fn class_problem(class: &ClassData) -> Option<(&'static str, &'static str)> {
    let spells = class
        .spells
        .iter()
        .chain(class.loadouts.iter().filter_map(|l| l.spell.as_ref()));
    if class.loadouts.iter().any(|l| l.speed_percent == 0) {
        return Some(("speed_percent", "0"));
    }
    for spell in spells {
        if spell.area.is_nan() || spell.area < 0.0 {
            return Some(("area", "not a number or below 0"));
        }
        if spell.trap.is_some() && !spell.mine {
            return Some(("trap", "on a spell that lays no mine"));
        }
    }
    None
}

/// What would break fights on `layout`: a floor too small for two fighters, no speed,
/// or a pillar that is not round or leaves no way between it and a wall.
fn layout_problem(layout: &ArenaLayout) -> Option<&'static str> {
    // Two fighters side by side, with room to move.
    let least = 8.0 * RADIUS;
    if layout.width.is_nan() || layout.height.is_nan() {
        return Some("a size not a number");
    }
    if layout.width < least || layout.height < least {
        return Some("a floor smaller than 4 m");
    }
    if layout.speed_percent == 0 {
        return Some("a speed percent of 0");
    }
    let gap = 2.0 * RADIUS;
    let walled_in = |p: &Pillar| {
        !(p.radius > 0.0
            && p.x - p.radius >= gap
            && p.x + p.radius <= layout.width - gap
            && p.y - p.radius >= gap
            && p.y + p.radius <= layout.height - gap)
    };
    if layout.pillars.iter().any(walled_in) {
        return Some("a pillar without radius or less than a fighter's width from a wall");
    }
    let door = layout.door;
    let inside = (0.0..=layout.width).contains(&door.x) && (0.0..=layout.height).contains(&door.y);
    let on_wall = [
        door.x,
        layout.width - door.x,
        door.y,
        layout.height - door.y,
    ]
    .iter()
    .any(|&d| d.abs() < 1e-9);
    (!inside || !on_wall).then_some("a door off the walls")
}

#[cfg(test)]
mod tests {
    use super::*;

    const CLASSES: &str = include_str!("../../../data/classes.ron");
    const COMBAT: &str = include_str!("../../../data/combat.ron");
    const FIGHTER: &str = include_str!("../../../data/fighter.ron");
    const CALENDAR: &str = include_str!("../../../data/calendar.ron");
    const ECONOMY: &str = include_str!("../../../data/economy.ron");
    const WORLD: &str = include_str!("../../../data/world.ron");
    const NAMES: &str = include_str!("../../../data/names.ron");
    const STAT_MODEL: &str = include_str!("../../../data/stat_model.ron");

    #[test]
    fn shipped_data_loads() {
        let data = DesignData::from_ron(
            CLASSES, COMBAT, FIGHTER, CALENDAR, ECONOMY, WORLD, NAMES, STAT_MODEL,
        )
        .unwrap();
        assert_eq!(data.classes.len(), 5);
        for class in &data.classes {
            assert_eq!(class.loadouts.len(), 2, "{}", class.key);
            for (i, loadout) in class.loadouts.iter().enumerate() {
                assert_eq!(class.spells_in(i).len(), 3, "{}", class.key);
                if let Some(spell) = &loadout.spell {
                    assert_eq!(&class.spells_in(i)[1], spell, "{}", loadout.key);
                }
                let weapon = data.combat.weapons.get(&loadout.weapon).cloned();
                assert_eq!(
                    loadout.auto_attack,
                    weapon.map(|w| loadout.shot(w)),
                    "{}",
                    loadout.key
                );
            }
            assert_eq!(class.loadout(9), class.loadouts.first());
        }
        // The double dague lays two mines a cast, the dague and bouclier one.
        let mines = |loadout| data.classes[3].spells_in(loadout)[2].blows;
        assert_eq!((mines(0), mines(1)), (1, 2));
        // The hunter's heavy bow shoots harder and slower than the light one, its trap
        // alike in both, and slows it down.
        let hunter = &data.classes[4];
        assert_eq!(hunter.key, "chasseur");
        let (heavy, light) = (hunter.spells_in(0), hunter.spells_in(1));
        for (h, l) in heavy.iter().zip(&light) {
            if h.mine {
                assert_eq!(h, l);
            } else {
                assert!(h.power > l.power && h.cooldown > l.cooldown, "{}", h.name);
            }
        }
        let attacks = hunter
            .loadouts
            .iter()
            .map(|l| l.auto_attack.clone().unwrap());
        let [heavy_attack, light_attack]: [SpellData; 2] =
            attacks.collect::<Vec<_>>().try_into().unwrap();
        assert!(heavy_attack.power > light_attack.power);
        assert!(hunter.loadouts[0].speed_percent < hunter.loadouts[1].speed_percent);
        assert_eq!(data.calendar.season_weeks(), 30);
        // Five continents, each with its major colosseum and one or two minor ones, one
        // of them free to enter: the safety net; Capoue first, where one starts.
        let world = &data.world;
        for continent in Continent::ALL {
            let here: Vec<_> = world.colosseums_of(continent).collect();
            assert!((2..=3).contains(&here.len()), "{continent:?}");
            let first = world.colosseum(world.first_colosseum(continent)).unwrap();
            assert_eq!((first.continent, first.purses.entry_fee), (continent, 0));
        }
        assert_eq!(world.colosseums[0].key, "capoue");
        assert_eq!(world.first_colosseum(Continent::Europe).0, 0);
    }

    #[test]
    fn finds_a_class_by_id() {
        let data = DesignData::from_ron(
            CLASSES, COMBAT, FIGHTER, CALENDAR, ECONOMY, WORLD, NAMES, STAT_MODEL,
        )
        .unwrap();
        assert_eq!(data.class(ClassId(4)), data.classes.get(4));
        assert_eq!(data.class(ClassId(5)), None);
    }

    #[test]
    fn rejects_unknown_fields() {
        let typo = r#"[(name: "X", max_mana: 0, max_endurance: 0, mana_per_second: 0, endurance_per_second: 0, spels: [])]"#;
        assert!(matches!(
            DesignData::from_ron(
                typo, COMBAT, FIGHTER, CALENDAR, ECONOMY, WORLD, NAMES, STAT_MODEL
            ),
            Err(LoadError::Ron {
                file: CLASSES_FILE,
                ..
            })
        ));
    }

    #[test]
    fn rejects_values_the_game_would_panic_on() {
        let field = |classes: &str, fighter: &str| match DesignData::from_ron(
            classes, COMBAT, fighter, CALENDAR, ECONOMY, WORLD, NAMES, STAT_MODEL,
        ) {
            Err(LoadError::Invalid { field, .. }) => field,
            other => panic!("expected an invalid field, got {other:?}"),
        };
        assert_eq!(field("[]", FIGHTER), "classes");
        let unarmed = CLASSES.replacen("weapon: \"glaive\"", "weapon: \"fork\"", 1);
        assert_eq!(field(&unarmed, FIGHTER), "weapon");
        let nowhere = CLASSES.replacen("area: 2.0", "area: -1.0", 1);
        assert_eq!(field(&nowhere, FIGHTER), "area");
        let unlaid = CLASSES.replacen(
            "mine: true,
                trap:",
            "trap:",
            1,
        );
        assert_eq!(field(&unlaid, FIGHTER), "trap");
        let rooted = CLASSES.replacen("speed_percent: 85", "speed_percent: 0", 1);
        assert_eq!(field(&rooted, FIGHTER), "speed_percent");
        let upside_down = FIGHTER.replacen("start_age: (20, 30)", "start_age: (30, 20)", 1);
        assert_eq!(field(CLASSES, &upside_down), "career.start_age");
        let stronger = FIGHTER.replacen(
            "off_hand_attack_percent: 85",
            "off_hand_attack_percent: 185",
            1,
        );
        assert_eq!(
            field(CLASSES, &stronger),
            "equipment.off_hand_attack_percent"
        );
        let bald = FIGHTER.replacen(r#"hair: ["short", "long"]"#, "hair: []", 1);
        assert_eq!(field(CLASSES, &bald), "appearance.hair");
        let still = FIGHTER.replacen("(-1.0, 0.0, 0.0), ", "", 1);
        assert_eq!(field(CLASSES, &still), "appearance.head_frames");
        let foreign_look = FIGHTER.replacen(r#""gladiateur": ["#, r#""spartiate": ["#, 1);
        assert_eq!(field(CLASSES, &foreign_look), "appearance.classes");
        let not_a_chance = FIGHTER.replacen("worsening_chance: 0.5", "worsening_chance: 1.5", 1);
        assert_eq!(field(CLASSES, &not_a_chance), "illness.worsening_chance");
        let no_week =
            "(competition_weeks: 0, great_event_weeks: 0, off_season_weeks: 0, first_year: 680)";
        assert!(matches!(
            DesignData::from_ron(
                CLASSES, COMBAT, FIGHTER, no_week, ECONOMY, WORLD, NAMES, STAT_MODEL
            ),
            Err(LoadError::Invalid {
                field: "season",
                ..
            })
        ));
        let upside_down = COMBAT.replacen("min_percent: 25", "min_percent: 99", 1);
        assert!(matches!(
            DesignData::from_ron(
                CLASSES,
                &upside_down,
                FIGHTER,
                CALENDAR,
                ECONOMY,
                WORLD,
                NAMES,
                STAT_MODEL
            ),
            Err(LoadError::Invalid {
                field: "accuracy",
                ..
            })
        ));
        let combat_field = |combat: &str| match DesignData::from_ron(
            CLASSES, combat, FIGHTER, CALENDAR, ECONOMY, WORLD, NAMES, STAT_MODEL,
        ) {
            Err(LoadError::Invalid { field, .. }) => field,
            other => panic!("expected an invalid field, got {other:?}"),
        };
        let faithful = COMBAT.replacen("break_fighting: 3", "break_fighting: 1", 1);
        assert_eq!(combat_field(&faithful), "melee.pacts.break_fighting");
        let alone = COMBAT.replacen("max_fighters: 20", "max_fighters: 1", 1);
        assert_eq!(combat_field(&alone), "melee.max_fighters");
        let stuck = COMBAT.replacen("speed_percent: 70", "speed_percent: 0", 1);
        assert_eq!(combat_field(&stuck), "arenas");
        let tiny = COMBAT.replacen("height: 6.0,", "height: 3.0,", 1);
        assert_eq!(combat_field(&tiny), "arenas");
        let against_the_wall = COMBAT.replacen("(x: 10.0, y: 4.0,", "(x: 10.0, y: 1.5,", 1);
        assert_eq!(combat_field(&against_the_wall), "arenas");
        let no_way_out = COMBAT.replacen("door: (x: 4.0, y: 0.0)", "door: (x: 4.0, y: 3.0)", 1);
        assert_eq!(combat_field(&no_way_out), "arenas");
    }

    #[test]
    fn rejects_a_world_the_game_could_not_play() {
        let not_free = WORLD.replacen("entry_fee: 0", "entry_fee: 5", 1);
        assert!(matches!(
            DesignData::from_ron(
                CLASSES, COMBAT, FIGHTER, CALENDAR, ECONOMY, &not_free, NAMES, STAT_MODEL
            ),
            Err(LoadError::Invalid {
                field: "colosseums",
                ..
            })
        ));
        let world_field = |world: &str, names: &str| match DesignData::from_ron(
            CLASSES, COMBAT, FIGHTER, CALENDAR, ECONOMY, world, names, STAT_MODEL,
        ) {
            Err(LoadError::Invalid { field, .. }) => field,
            other => panic!("expected an invalid field, got {other:?}"),
        };
        let unknown = WORLD.replacen(r#"(2, "oceanie")"#, r#"(2, "atlantide")"#, 1);
        assert_eq!(world_field(&unknown, NAMES), "clubs.cultures");
        let nameless = NAMES.replacen(r#"clubs: ["whare", "pa"]"#, "clubs: []", 1);
        assert_eq!(world_field(WORLD, &nameless), "cultures");
        let empty = WORLD.replacen("count: 40", "count: 0", 1);
        assert_eq!(world_field(&empty, NAMES), "clubs.count");
        let crowded = WORLD.replacen("max: 100", "max: 39", 1);
        assert_eq!(world_field(&crowded, NAMES), "clubs.max");
        let doomed = WORLD.replacen("ruined_seasons: 2", "ruined_seasons: 0", 1);
        assert_eq!(world_field(&doomed, NAMES), "clubs.ruined_seasons");
        let muddled = WORLD.replacen("tense: (from: -59", "tense: (from: 30", 1);
        assert_eq!(world_field(&muddled, NAMES), "relations.standings");
        let free = WORLD.replacen("price_percent: 85", "price_percent: 0", 1);
        assert_eq!(world_field(&free, NAMES), "relations.standings");
        // Oceania's clubs with no culture of their own, or no free colosseum.
        let uprooted = NAMES.replacen("continent: Oceania", "continent: Europe", 1);
        assert_eq!(world_field(WORLD, &uprooted), "clubs.cultures");
        let dear = ["nan_madol", "rapa_nui"]
            .iter()
            .fold(WORLD.to_owned(), |world, key| {
                let free = format!(
                    r#"key: "{key}",
            continent: Oceania,
            arena: Sand,
            prestige: 1,
            residents: 8,
            purses: (entry_fee: 0"#
                );
                world.replacen(&free, &free.replace("entry_fee: 0", "entry_fee: 5"), 1)
            });
        assert_ne!(dear, WORLD);
        assert_eq!(world_field(&dear, NAMES), "colosseums");
        let nowhere = WORLD.replacen("(1, Oceania)", "(0, Oceania)", 1);
        assert!(
            DesignData::from_ron(
                CLASSES, COMBAT, FIGHTER, CALENDAR, ECONOMY, &nowhere, &uprooted, STAT_MODEL,
            )
            .is_ok()
        );
    }

    #[test]
    fn shipped_form_rules_triple_risk_at_form_0() {
        let data = DesignData::from_ron(
            CLASSES, COMBAT, FIGHTER, CALENDAR, ECONOMY, WORLD, NAMES, STAT_MODEL,
        )
        .unwrap();
        assert!((data.fighter.form.risk_factor - 2.0).abs() < f64::EPSILON);
    }
}
