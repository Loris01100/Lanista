//! The encyclopedia, laid out as a civilopedia: its categories over the page, the list of
//! a category's pages (or of those a search finds) on the left, the page's rules and
//! history in the middle, its card on the right. Every figure comes from the design
//! data; only the prose is in the language files (`wiki-<kind>-<key>-rules` and
//! `-history`), a page without it saying so.

use std::rc::Rc;

use lanista_core::combat::{DamageKind, Range};
use lanista_core::design_data::{DesignData, SpellData};
use lanista_core::fighter::equipment::OffHand;
use reesch::egui;

use super::{ClubScreen, View, section};
use crate::combat_screen::{cost_text, tenths};
use crate::lang::tr;

/// A page of the encyclopedia.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Entry {
    /// The class at this index of `DesignData::classes`.
    Class(usize),
    /// The weapon type at this index of `CombatRules::weapons`, in key order.
    Weapon(usize),
}

impl Entry {
    /// The first page of each category, in the order of the tabs.
    pub(super) const CATEGORIES: [Self; 2] = [Self::Class(0), Self::Weapon(0)];

    /// Its category's name key.
    fn category(self) -> &'static str {
        match self {
            Self::Class(_) => "wiki-classes",
            Self::Weapon(_) => "wiki-weapons",
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
        }
    }

    fn name(self, data: &DesignData) -> String {
        self.key(data)
            .map_or_else(|| "?".into(), |(kind, key)| tr!(&format!("{kind}-{key}")))
    }
}

/// Every page, the classes then the weapons.
pub(super) fn entries(data: &DesignData) -> impl Iterator<Item = Entry> {
    let classes = (0..data.classes.len()).map(Entry::Class);
    classes.chain((0..data.combat.weapons.len()).map(Entry::Weapon))
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

/// The classes whose loadouts hold weapon `key`: the class's and the loadout's indices.
fn users(data: &DesignData, key: &str) -> Vec<(usize, usize)> {
    let mut users = Vec::new();
    for (c, class) in data.classes.iter().enumerate() {
        for (l, loadout) in class.loadouts.iter().enumerate() {
            if loadout.weapon == key {
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
            section(ui, &tr!("wiki-loadouts"));
            for loadout in &class.loadouts {
                let weapon = tr!(&format!("weapon-{}", loadout.weapon));
                let off_hand = match loadout.off_hand {
                    OffHand::Free => tr!("wiki-off-hand-free"),
                    OffHand::Shield => tr!("wiki-off-hand-shield"),
                    OffHand::Weapon => tr!("wiki-off-hand-weapon"),
                };
                let name = tr!(&format!("loadout-{}", loadout.key));
                ui.label(egui::RichText::new(name).strong());
                ui.label(tr!("wiki-loadout", weapon = weapon, off_hand = off_hand));
                spell_line(ui, &loadout.spell);
                ui.add_space(4.0);
            }
        }
        Entry::Weapon(i) => {
            if let Some(attack) = data.combat.weapons.values().nth(i) {
                section(ui, &tr!("wiki-auto-attack"));
                spell_line(ui, attack);
            }
        }
    }
    section(ui, &tr!("wiki-history"));
    prose(ui, entry, data, "history");
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
    if spell.mine {
        figures.push(tr!("wiki-mine"));
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
        }
        Entry::Weapon(i) => {
            let Some(key) = data.combat.weapons.keys().nth(i) else {
                return;
            };
            section(ui, &tr!("wiki-used-by"));
            for (c, l) in users(data, key) {
                let class = Entry::Class(c);
                let loadout = tr!(&format!("loadout-{}", data.classes[c].loadouts[l].key));
                let text = tr!("wiki-user", class = class.name(data), loadout = loadout);
                if ui.link(text).clicked() {
                    *open = Some(class);
                }
            }
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
    fn every_weapon_is_held_by_a_class() {
        let data = DesignData::load().unwrap();
        for key in data.combat.weapons.keys() {
            assert!(!users(&data, key).is_empty(), "{key}");
        }
    }
}
