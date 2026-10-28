//! The encyclopedia, laid out as a civilopedia: its categories over the page, the list of
//! a category's pages (or of those a search finds) on the left, the page's rules and
//! history in the middle, its card on the right. Every figure comes from the design
//! data, the formulas of the Calculs pages too; only the prose is in the language files
//! (`wiki-<kind>-<key>-rules` and `-history`), a page without it saying so.

use std::rc::Rc;

use lanista_core::combat::{DamageKind, Range};
use lanista_core::design_data::{DesignData, LoadoutData, SpellData};
use lanista_core::fighter::Stats;
use lanista_core::fighter::equipment::OffHand;
use lanista_core::fighter::illness::IllnessLevel;
use reesch::egui;

use super::{ClubScreen, View, section};
use crate::combat_screen::{cost_text, tenths};
use crate::lang::tr;
use crate::texts::{arena_effect, arena_name, continent_name, stat_names};

/// A page of the encyclopedia.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Entry {
    /// The class at this index of `DesignData::classes`.
    Class(usize),
    /// The weapon type at this index of `CombatRules::weapons`, in key order.
    Weapon(usize),
    /// The shield an off hand may hold (`OffHand::Shield`).
    Shield,
    /// The colosseum at this index of `WorldRules::colosseums`.
    Colosseum(usize),
    /// A formula of the game.
    Calc(Calc),
}

/// The formulas the Calculs pages lay out, each with the figures of the design data.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Calc {
    Damage,
    Defense,
    Hit,
    Stun,
    Cut,
    Bleed,
    Form,
    Injury,
    Illness,
}

impl Calc {
    const ALL: [Self; 9] = [
        Self::Damage,
        Self::Defense,
        Self::Hit,
        Self::Stun,
        Self::Cut,
        Self::Bleed,
        Self::Form,
        Self::Injury,
        Self::Illness,
    ];

    /// Names it in the language files (`calc-<key>`).
    fn key(self) -> &'static str {
        match self {
            Self::Damage => "damage",
            Self::Defense => "defense",
            Self::Hit => "hit",
            Self::Stun => "stun",
            Self::Cut => "cut",
            Self::Bleed => "bleed",
            Self::Form => "form",
            Self::Injury => "injury",
            Self::Illness => "illness",
        }
    }
}

impl Entry {
    /// The first page of each category, in the order of the tabs.
    pub(super) const CATEGORIES: [Self; 4] = [
        Self::Class(0),
        Self::Weapon(0),
        Self::Colosseum(0),
        Self::Calc(Calc::Damage),
    ];

    /// Its category's name key.
    fn category(self) -> &'static str {
        match self {
            Self::Class(_) => "wiki-classes",
            Self::Weapon(_) | Self::Shield => "wiki-weapons",
            Self::Colosseum(_) => "wiki-colosseums",
            Self::Calc(_) => "wiki-calcs",
        }
    }

    /// Its kind and key in the language files, `class` and `mirmillon` for
    /// `class-mirmillon`; none if it is out of the design data.
    fn key(self, data: &DesignData) -> Option<(&'static str, &str)> {
        match self {
            Self::Class(i) => data.classes.get(i).map(|c| ("class", c.key.as_str())),
            Self::Weapon(i) => data
                .combat
                .weapons
                .keys()
                .nth(i)
                .map(|k| ("weapon", k.as_str())),
            Self::Shield => Some(("weapon", "shield")),
            Self::Colosseum(i) => data
                .world
                .colosseums
                .get(i)
                .map(|c| ("colosseum", c.key.as_str())),
            Self::Calc(calc) => Some(("calc", calc.key())),
        }
    }

    fn name(self, data: &DesignData) -> String {
        self.key(data)
            .map_or_else(|| "?".into(), |(kind, key)| tr!(&format!("{kind}-{key}")))
    }
}

/// Every page, the classes, the weapons and the shield, the colosseums, the formulas.
pub(super) fn entries(data: &DesignData) -> impl Iterator<Item = Entry> {
    let classes = (0..data.classes.len()).map(Entry::Class);
    let weapons = (0..data.combat.weapons.len()).map(Entry::Weapon);
    let colosseums = (0..data.world.colosseums.len()).map(Entry::Colosseum);
    let calcs = Calc::ALL.into_iter().map(Entry::Calc);
    classes
        .chain(weapons)
        .chain([Entry::Shield])
        .chain(colosseums)
        .chain(calcs)
}

/// The pages of `shown`'s category, or of all whose name holds `query`, whatever the case.
fn listed(data: &DesignData, shown: Entry, query: &str) -> Vec<Entry> {
    let query = query.trim().to_lowercase();
    entries(data)
        .filter(|e| {
            if query.is_empty() {
                e.category() == shown.category()
            } else {
                e.name(data).to_lowercase().contains(&query)
            }
        })
        .collect()
}

/// The classes with loadouts that `holds`: the class's and the loadout's indices.
fn users(data: &DesignData, holds: impl Fn(&LoadoutData) -> bool) -> Vec<(usize, usize)> {
    let mut users = Vec::new();
    for (c, class) in data.classes.iter().enumerate() {
        for (l, loadout) in class.loadouts.iter().enumerate() {
            if holds(loadout) {
                users.push((c, l));
            }
        }
    }
    users
}

impl ClubScreen {
    /// The encyclopedia open on `shown`.
    pub(super) fn wiki(&mut self, ui: &mut egui::Ui, shown: Entry) {
        let data = Rc::clone(&self.data);
        let mut open = None;
        ui.horizontal(|ui| {
            for first in Entry::CATEGORIES {
                let selected = first.category() == shown.category();
                if ui
                    .selectable_label(selected, tr!(first.category()))
                    .clicked()
                {
                    open = Some(first);
                }
            }
        });
        ui.separator();
        egui::SidePanel::left("wiki-list")
            .resizable(false)
            .default_width(180.0)
            .show_inside(ui, |ui| {
                let search =
                    egui::TextEdit::singleline(&mut self.wiki_search).hint_text(tr!("wiki-search"));
                ui.add(search);
                ui.add_space(4.0);
                egui::ScrollArea::vertical().show(ui, |ui| {
                    for entry in listed(&data, shown, &self.wiki_search) {
                        if ui
                            .selectable_label(entry == shown, entry.name(&data))
                            .clicked()
                        {
                            open = Some(entry);
                        }
                    }
                });
            });
        egui::SidePanel::right("wiki-card")
            .resizable(false)
            .default_width(220.0)
            .show_inside(ui, |ui| card(ui, shown, &data, &mut open));
        egui::CentralPanel::default().show_inside(ui, |ui| {
            egui::ScrollArea::vertical().show(ui, |ui| page(ui, shown, &data));
        });
        if let Some(entry) = open {
            self.go_to(View::Wiki(entry));
        }
    }
}

/// The middle of the page: its rules, what it brings to a fight, its history.
fn page(ui: &mut egui::Ui, entry: Entry, data: &DesignData) {
    ui.heading(entry.name(data));
    section(ui, &tr!("wiki-rules"));
    prose(ui, entry, data, "rules");
    match entry {
        Entry::Class(i) => {
            let Some(class) = data.classes.get(i) else {
                return;
            };
            section(ui, &tr!("wiki-spells"));
            for spell in &class.spells {
                spell_line(ui, spell);
            }
            section(ui, &tr!("wiki-configurations"));
            let id = loadout_id(i);
            let mut chosen = ui.data(|d| d.get_temp(id)).unwrap_or(0);
            ui.horizontal(|ui| {
                for (l, loadout) in class.loadouts.iter().enumerate() {
                    let name = tr!(&format!("loadout-{}", loadout.key));
                    if ui.selectable_label(l == chosen, name).clicked() {
                        chosen = l;
                        ui.data_mut(|d| d.insert_temp(id, l));
                    }
                }
            });
            if let Some(loadout) = class.loadouts.get(chosen) {
                let weapon = tr!(&format!("weapon-{}", loadout.weapon));
                let off_hand = match loadout.off_hand {
                    OffHand::Free => tr!("wiki-off-hand-free"),
                    OffHand::Shield => tr!("wiki-off-hand-shield"),
                    OffHand::Weapon => tr!("wiki-off-hand-weapon"),
                };
                ui.label(tr!("wiki-loadout", weapon = weapon, off_hand = off_hand));
                if loadout.speed_percent != 100 {
                    ui.label(tr!("wiki-loadout-speed", percent = loadout.speed_percent));
                }
                if (loadout.shot_power_percent, loadout.shot_cooldown_percent) != (100, 100) {
                    ui.label(tr!(
                        "wiki-loadout-shots",
                        power = loadout.shot_power_percent,
                        cooldown = loadout.shot_cooldown_percent
                    ));
                }
                if let Some(spell) = &loadout.spell {
                    spell_line(ui, spell);
                }
            }
        }
        Entry::Weapon(i) => {
            if let Some(attack) = data.combat.weapons.values().nth(i) {
                section(ui, &tr!("wiki-auto-attack"));
                spell_line(ui, attack);
            }
        }
        Entry::Shield => {
            section(ui, &tr!("wiki-effect"));
            let percent = data.fighter.equipment.shield_defense_percent;
            ui.label(tr!("wiki-shield-effect", percent = percent));
        }
        Entry::Colosseum(i) => {
            let Some(colosseum) = data.world.colosseums.get(i) else {
                return;
            };
            section(ui, &tr!("wiki-arena"));
            let layout = data.combat.arenas.get(colosseum.arena);
            ui.label(egui::RichText::new(arena_name(colosseum.arena)).strong());
            ui.label(arena_effect(colosseum.arena));
            let floor = tr!(
                "wiki-arena-floor",
                width = decimal(layout.width),
                height = decimal(layout.height),
                speed = layout.speed_percent,
                pillars = layout.pillars.len()
            );
            ui.label(egui::RichText::new(floor).small().weak());
        }
        // A formula has no history.
        Entry::Calc(calc) => return calc_page(ui, calc, data),
    }
    section(ui, &tr!("wiki-history"));
    prose(ui, entry, data, "history");
}

/// The formula of `calc`, with the design data's figures.
fn calc_page(ui: &mut egui::Ui, calc: Calc, data: &DesignData) {
    section(ui, &tr!("wiki-formula"));
    for line in calc_lines(calc, data) {
        ui.label(line);
    }
    match calc {
        Calc::Injury => {
            severity_table(ui, data);
            parts_table(ui, data);
        }
        Calc::Illness => illness_table(ui, data),
        _ => {}
    }
}

/// The lines of `calc`'s formula, those of the fight; the others in `health_lines`.
fn calc_lines(calc: Calc, data: &DesignData) -> Vec<String> {
    let combat = &data.combat;
    let fighter = &data.fighter;
    match calc {
        Calc::Damage => vec![
            tr!("wiki-calc-damage-formula"),
            tr!("wiki-calc-damage-stats"),
            tr!(
                "wiki-calc-damage-off-hand",
                percent = fighter.equipment.off_hand_attack_percent
            ),
        ],
        Calc::Defense => vec![
            tr!("wiki-calc-defense-formula"),
            tr!(
                "wiki-calc-defense-shield",
                percent = fighter.equipment.shield_defense_percent
            ),
        ],
        Calc::Hit => {
            let hits = &combat.hits;
            let total = hits.head + hits.body + hits.arm + hits.leg;
            vec![
                tr!(
                    "wiki-calc-hit-formula",
                    even = combat.accuracy.even_percent,
                    min = combat.accuracy.min_percent,
                    max = combat.accuracy.max_percent
                ),
                tr!(
                    "wiki-calc-hit-location",
                    head = share(hits.head, total),
                    body = share(hits.body, total),
                    arm = share(hits.arm, total),
                    leg = share(hits.leg, total)
                ),
            ]
        }
        Calc::Stun => vec![
            tr!(
                "wiki-calc-stun-chance",
                blunt = hundredths(combat.stun.blunt_hundredths),
                cutting = hundredths(combat.stun.cutting_hundredths),
                head = combat.stun.head_injury_percent
            ),
            tr!(
                "wiki-calc-stun-rise",
                seconds = tenths(combat.stun.ticks),
                timid = combat.stun.rise_percent_most_timid,
                bold = combat.stun.rise_percent_boldest,
                lost = combat.stun.rise_percent_lost_per_stun
            ),
        ],
        Calc::Cut => vec![tr!(
            "wiki-calc-cut",
            limb = hundredths(combat.cut.limb_hundredths),
            head = hundredths(combat.cut.head_hundredths),
            fatal = combat.cut.fatal_head_percent
        )],
        Calc::Bleed => vec![tr!(
            "wiki-calc-bleed",
            percent = combat.bleed.hp_percent_per_second,
            head = combat.bleed.head_seconds,
            body = combat.bleed.body_seconds,
            arm = combat.bleed.arm_seconds,
            leg = combat.bleed.leg_seconds
        )],
        Calc::Form | Calc::Injury | Calc::Illness => health_lines(calc, data),
    }
}

/// The lines of the formulas of form, injuries and illnesses.
fn health_lines(calc: Calc, data: &DesignData) -> Vec<String> {
    let fighter = &data.fighter;
    match calc {
        Calc::Form => {
            let form = &fighter.form;
            vec![
                tr!(
                    "wiki-calc-form-risk",
                    k = decimal(form.risk_factor),
                    max = decimal(1.0 + form.risk_factor)
                ),
                tr!(
                    "wiki-calc-form-events",
                    fight = form.fight,
                    title = form.title_defense,
                    melee = form.melee,
                    training = form.training,
                    rest = form.rest
                ),
            ]
        }
        Calc::Injury => {
            let injury = &fighter.injury;
            vec![
                tr!(
                    "wiki-calc-injury-chance",
                    percent = percent(injury.chance_per_fight),
                    survival = percent(injury.fatal_blow_survival),
                    stun = percent(injury.stun_wound_chance)
                ),
                tr!("wiki-calc-care", percent = fighter.care.weeks_kept_percent),
                tr!("wiki-calc-max-malus", max = injury.max_malus),
            ]
        }
        Calc::Illness => {
            let illness = &fighter.illness;
            vec![
                tr!(
                    "wiki-calc-illness-chance",
                    percent = percent(illness.chance_per_week),
                    chance = percent(illness.worsening_chance),
                    min = illness.worsening_weeks.0,
                    max = illness.worsening_weeks.1
                ),
                tr!("wiki-calc-care", percent = fighter.care.weeks_kept_percent),
            ]
        }
        _ => Vec::new(),
    }
}

/// The injuries' severities: chance, weeks, share of the maluses, care.
fn severity_table(ui: &mut egui::Ui, data: &DesignData) {
    let rules = &data.fighter.injury;
    let (weights, weeks, malus) = (
        &rules.severity_weights,
        &rules.healing_weeks,
        &rules.severity_percent,
    );
    let cost = &data.fighter.care.injury_cost;
    section(ui, &tr!("wiki-severities"));
    egui::Grid::new("wiki-severities")
        .striped(true)
        .show(ui, |ui| {
            header_row(
                ui,
                &[
                    "wiki-level",
                    "wiki-chance",
                    "wiki-healing",
                    "wiki-maluses",
                    "wiki-care",
                ],
            );
            let severities = [
                (
                    "wiki-light",
                    weights.light,
                    Some((weeks.light, malus.light, cost.light)),
                ),
                (
                    "wiki-medium",
                    weights.medium,
                    Some((weeks.medium, malus.medium, cost.medium)),
                ),
                (
                    "wiki-severe",
                    weights.severe,
                    Some((weeks.severe, malus.severe, cost.severe)),
                ),
                ("wiki-permanent", weights.permanent, None),
            ];
            let total = weights.light + weights.medium + weights.severe + weights.permanent;
            for (name, weight, heals) in severities {
                ui.label(tr!(name));
                ui.label(format!("{} %", share(weight, total)));
                if let Some(((min, max), malus, cost)) = heals {
                    ui.label(tr!("wiki-weeks-range", min = min, max = max));
                    ui.label(format!("{malus} %"));
                    ui.label(cost.to_string());
                } else {
                    ui.label(tr!("wiki-never"));
                    ui.label(tr!("wiki-after-effect"));
                    ui.label("—");
                }
                ui.end_row();
            }
        });
}

/// The maluses of an injury on each part of the body, medium, permanent, the limb lost.
fn parts_table(ui: &mut egui::Ui, data: &DesignData) {
    let rules = &data.fighter.injury;
    section(ui, &tr!("wiki-parts"));
    let (temporary, permanent, lost) = (&rules.temporary, &rules.permanent, &rules.lost);
    let at = &rules.location_weights;
    let total = at.head + at.body + at.arm + at.leg;
    // An arm wound falls on either arm, one time in two.
    let one_arm = |weight: u32| share(weight, total * 2);
    let parts = [
        (
            "location-head",
            share(at.head, total),
            temporary.head,
            permanent.head,
            None,
        ),
        (
            "location-body",
            share(at.body, total),
            temporary.body,
            permanent.body,
            None,
        ),
        (
            "wiki-dominant-arm",
            one_arm(at.arm),
            temporary.dominant_arm,
            permanent.dominant_arm,
            Some(lost.dominant_arm),
        ),
        (
            "wiki-off-arm",
            one_arm(at.arm),
            temporary.off_arm,
            permanent.off_arm,
            Some(lost.off_arm),
        ),
        (
            "wiki-leg",
            share(at.leg, total),
            temporary.leg,
            permanent.leg,
            Some(lost.leg),
        ),
    ];
    egui::Grid::new("wiki-parts").striped(true).show(ui, |ui| {
        header_row(
            ui,
            &[
                "wiki-part",
                "wiki-chance",
                "wiki-medium",
                "wiki-permanent",
                "wiki-lost",
            ],
        );
        for (name, weight, temporary, permanent, lost) in parts {
            ui.label(tr!(name));
            ui.label(format!("{weight} %"));
            ui.label(maluses(temporary));
            ui.label(maluses(permanent));
            ui.label(lost.map_or_else(|| "—".into(), maluses));
            ui.end_row();
        }
    });
}

/// The illnesses' levels: chance, weeks, malus on every stat, rest, care.
fn illness_table(ui: &mut egui::Ui, data: &DesignData) {
    let rules = &data.fighter.illness;
    let cost = &data.fighter.care.illness_cost;
    let levels = [
        ("wiki-mild", IllnessLevel::Mild, cost.mild),
        ("wiki-moderate", IllnessLevel::Moderate, cost.moderate),
        ("wiki-severe-illness", IllnessLevel::Severe, cost.severe),
    ];
    let weights = &rules.level_weights;
    let total = weights.mild + weights.moderate + weights.severe;
    section(ui, &tr!("wiki-levels"));
    egui::Grid::new("wiki-levels").striped(true).show(ui, |ui| {
        header_row(
            ui,
            &[
                "wiki-level",
                "wiki-chance",
                "wiki-healing",
                "wiki-all-stats",
                "wiki-rest",
                "wiki-care",
            ],
        );
        for (name, level, cost) in levels {
            let (min, max) = rules.weeks.get(level);
            ui.label(tr!(name));
            ui.label(format!("{} %", share(weights.get(level), total)));
            ui.label(tr!("wiki-weeks-range", min = min, max = max));
            ui.label(format!("-{} %", rules.malus.get(level)));
            ui.label(format!("{} %", rules.rest_percent.get(level)));
            ui.label(cost.to_string());
            ui.end_row();
        }
    });
}

/// A grid's first row, its columns' names.
fn header_row(ui: &mut egui::Ui, keys: &[&str]) {
    for key in keys {
        ui.label(egui::RichText::new(tr!(key)).strong());
    }
    ui.end_row();
}

/// Stat maluses in percent, « PRÉ -10 %, PV -10 % », or a dash for none.
fn maluses(stats: Stats) -> String {
    let maluses: Vec<String> = stat_names()
        .into_iter()
        .zip(stats.to_array())
        .filter(|&(_, percent)| percent > 0)
        .map(|(stat, percent)| tr!("health-malus", stat = stat, percent = percent))
        .collect();
    if maluses.is_empty() {
        "—".into()
    } else {
        maluses.join(", ")
    }
}

/// The chance, in percent, of drawing `weight` out of weights adding up to `total`: « 33,33 ».
fn share(weight: u32, total: u32) -> String {
    let hundredths_of_percent =
        (u64::from(weight) * 10_000 + u64::from(total) / 2) / u64::from(total.max(1));
    hundredths(u32::try_from(hundredths_of_percent).unwrap_or(u32::MAX))
}

/// Hundredths of a percent as a percent: « 1,67 » for 167, « 5 » for 500.
fn hundredths(n: u32) -> String {
    match n % 100 {
        0 => (n / 100).to_string(),
        rest => format!("{},{rest:02}", n / 100)
            .trim_end_matches('0')
            .into(),
    }
}

/// A chance from 0 to 1 in percent, rounded: « 15 » for 0.15.
fn percent(chance: f64) -> String {
    (chance * 100.0).round().to_string()
}

/// A number with a decimal comma: « 1,5 ».
fn decimal(x: f64) -> String {
    x.to_string().replace('.', ",")
}

/// Where the egui memory keeps the loadout shown on the page of the class at `class`.
fn loadout_id(class: usize) -> egui::Id {
    egui::Id::new(("wiki-loadout", class))
}

/// The text `wiki-<kind>-<key>-<part>`, or that it is still to write.
fn prose(ui: &mut egui::Ui, entry: Entry, data: &DesignData, part: &str) {
    let Some((kind, key)) = entry.key(data) else {
        return;
    };
    let id = format!("wiki-{kind}-{key}-{part}");
    let text = tr!(&id);
    // A missing key comes back as itself.
    if text == id {
        ui.label(egui::RichText::new(tr!("wiki-to-write")).italics().weak());
    } else {
        ui.label(text);
    }
}

/// A spell's name, then its figures on one line: « Mêlée, 1 m · puissance 150 % · … ».
fn spell_line(ui: &mut egui::Ui, spell: &SpellData) {
    ui.label(egui::RichText::new(&spell.name).strong());
    let range = match spell.range {
        Range::Melee => tr!("wiki-melee"),
        Range::Ranged => tr!("wiki-ranged"),
    };
    let reach = spell.reach.to_string().replace('.', ",");
    let kind = match spell.damage_kind {
        DamageKind::Blunt => tr!("wiki-blunt"),
        DamageKind::Cutting => tr!("wiki-cutting"),
        DamageKind::Neutral => tr!("wiki-neutral"),
    };
    let mut figures = vec![
        tr!("wiki-range", range = range, metres = reach),
        tr!("wiki-power", percent = spell.power),
        tr!("wiki-cooldown", seconds = tenths(spell.cooldown)),
        kind,
    ];
    let cost = cost_text(&spell.cost);
    if !cost.is_empty() {
        figures.push(cost);
    }
    if spell.blows > 1 {
        figures.push(tr!("wiki-blows", n = spell.blows));
    }
    if spell.sure_hit {
        figures.push(tr!("wiki-sure-hit"));
    } else if spell.accuracy_percent != 100 {
        figures.push(tr!("wiki-accuracy", percent = spell.accuracy_percent));
    }
    if spell.over_obstacles {
        figures.push(tr!("wiki-over-obstacles"));
    }
    if spell.area > 0.0 {
        let metres = spell.area.to_string().replace('.', ",");
        figures.push(tr!("wiki-area", metres = metres));
    }
    match spell.trap {
        Some(trap) => figures.push(tr!(
            "wiki-trap",
            seconds = tenths(trap.ticks),
            percent = trap.damage_percent
        )),
        None if spell.mine => figures.push(tr!("wiki-mine")),
        None => {}
    }
    if spell.gathers > 0 {
        figures.push(tr!("wiki-gathers", n = spell.gathers));
    }
    if spell.bleed_percent > 0 {
        figures.push(tr!("wiki-bleed", percent = spell.bleed_percent));
    }
    ui.label(egui::RichText::new(figures.join(" · ")).small().weak());
}

/// The right of the page: its figures, and links to the pages it leads to, the one
/// clicked in `open`.
fn card(ui: &mut egui::Ui, entry: Entry, data: &DesignData, open: &mut Option<Entry>) {
    // A formula's figures are all on its page.
    if matches!(entry, Entry::Calc(_)) {
        return;
    }
    section(ui, &tr!("wiki-card"));
    match entry {
        Entry::Class(i) => {
            let Some(class) = data.classes.get(i) else {
                return;
            };
            let resources = [
                ("mana", class.max_mana, class.mana_per_second),
                ("endurance", class.max_endurance, class.endurance_per_second),
            ];
            egui::Grid::new("wiki-resources").show(ui, |ui| {
                for (name, max, per_second) in resources.into_iter().filter(|r| r.1 > 0) {
                    ui.label(tr!(name));
                    ui.label(tr!("wiki-resource", max = max, per_second = per_second));
                    ui.end_row();
                }
                if class.max_elements > 0 {
                    ui.label(tr!("elements"));
                    ui.label(class.max_elements.to_string());
                    ui.end_row();
                }
            });
            section(ui, &tr!("wiki-weapons"));
            let keys: Vec<&String> = data.combat.weapons.keys().collect();
            let mut shown = Vec::new();
            for loadout in &class.loadouts {
                let Some(w) = keys.iter().position(|k| **k == loadout.weapon) else {
                    continue;
                };
                if !shown.contains(&w) {
                    shown.push(w);
                    let weapon = Entry::Weapon(w);
                    if ui.link(weapon.name(data)).clicked() {
                        *open = Some(weapon);
                    }
                }
            }
            let shield = class.loadouts.iter().any(|l| l.off_hand == OffHand::Shield);
            if shield && ui.link(Entry::Shield.name(data)).clicked() {
                *open = Some(Entry::Shield);
            }
        }
        Entry::Weapon(i) => {
            let Some(key) = data.combat.weapons.keys().nth(i) else {
                return;
            };
            users_links(ui, data, users(data, |l| l.weapon == *key), open);
        }
        Entry::Shield => {
            let percent = data.fighter.equipment.shield_defense_percent;
            egui::Grid::new("wiki-shield").show(ui, |ui| {
                ui.label(tr!("wiki-defenses"));
                ui.label(format!("{percent} %"));
                ui.end_row();
            });
            let users = users(data, |l| l.off_hand == OffHand::Shield);
            users_links(ui, data, users, open);
        }
        Entry::Colosseum(i) => {
            let Some(colosseum) = data.world.colosseums.get(i) else {
                return;
            };
            let purses = &colosseum.purses;
            let receipts = tr!(
                "wiki-receipts-value",
                base = purses.receipts_base,
                per_fame = purses.receipts_per_fame
            );
            let rows = [
                ("wiki-continent", continent_name(colosseum.continent)),
                ("wiki-prestige", format!("{} / 5", colosseum.prestige)),
                ("wiki-residents", colosseum.residents.to_string()),
                ("wiki-entry-fee", purses.entry_fee.to_string()),
                ("wiki-win-prize", purses.win_prize.to_string()),
                ("wiki-receipts", receipts),
                ("wiki-champion-bonus", purses.champion_bonus.to_string()),
                ("wiki-melee-prize", purses.melee_prize.to_string()),
            ];
            egui::Grid::new("wiki-colosseum").show(ui, |ui| {
                for (name, value) in rows {
                    ui.label(tr!(name));
                    ui.label(value);
                    ui.end_row();
                }
            });
        }
        Entry::Calc(_) => {}
    }
}

/// Links to the classes of `users`, each with its loadout's name, the class's page then
/// showing that loadout.
fn users_links(
    ui: &mut egui::Ui,
    data: &DesignData,
    users: Vec<(usize, usize)>,
    open: &mut Option<Entry>,
) {
    section(ui, &tr!("wiki-used-by"));
    for (c, l) in users {
        let class = Entry::Class(c);
        let loadout = tr!(&format!("loadout-{}", data.classes[c].loadouts[l].key));
        let text = tr!("wiki-user", class = class.name(data), loadout = loadout);
        if ui.link(text).clicked() {
            ui.data_mut(|d| d.insert_temp(loadout_id(c), l));
            *open = Some(class);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_search_finds_pages_of_every_category() {
        let data = DesignData::load().unwrap();
        let classes = listed(&data, Entry::Class(0), "");
        assert_eq!(classes.len(), data.classes.len());
        assert!(classes.iter().all(|e| matches!(e, Entry::Class(_))));
        let found: Vec<String> = listed(&data, Entry::Class(0), " GLAI")
            .into_iter()
            .map(|e| e.name(&data))
            .collect();
        assert_eq!(found, ["Glaive"]);
    }

    #[test]
    fn hundredths_read_as_percents() {
        assert_eq!(hundredths(500), "5");
        assert_eq!(hundredths(167), "1,67");
        assert_eq!(hundredths(17), "0,17");
        assert_eq!(hundredths(250), "2,5");
    }

    #[test]
    fn weights_read_as_chances() {
        assert_eq!(share(1, 10), "10");
        assert_eq!(share(1, 3), "33,33");
        assert_eq!(share(3, 40), "7,5");
    }

    #[test]
    fn every_weapon_is_held_by_a_class() {
        let data = DesignData::load().unwrap();
        for key in data.combat.weapons.keys() {
            assert!(!users(&data, |l| l.weapon == *key).is_empty(), "{key}");
        }
    }
}
