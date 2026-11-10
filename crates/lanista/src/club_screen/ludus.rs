//! The ludus view.

use lanista_core::design_data::DesignData;
use lanista_core::ludus::{Ludus, LudusMove, Site};
use reesch::egui;

use super::{ClubScreen, wrapped_chip};
use crate::lang::tr;
use crate::texts::{building_name, effect_text, ludus_move_text, school_name};
use crate::theme;

impl ClubScreen {
    /// The ludus: this week's moves, the plots as they will be once made, then the card
    /// of the plot chosen: its building, or the buildings it may take.
    pub(super) fn ludus(&mut self, ui: &mut egui::Ui) {
        let data = &self.data;
        let chosen = &mut self.decisions.ludus;
        ui.heading(school_name(&self.state, data));
        ui.label(egui::RichText::new(tr!("ludus-intro")).small().weak());
        ui.add_space(4.0);
        let (planned, money) = planned(&self.state.ludus, chosen, self.state.money, data);
        let mut cancel = None;
        if !chosen.is_empty() {
            egui::Frame::group(ui.style())
                .fill(ui.visuals().extreme_bg_color)
                .show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    let title = egui::RichText::new(tr!("market-this-week"));
                    ui.label(title.strong().color(theme::BRONZE));
                    for (n, &m) in chosen.iter().enumerate() {
                        ui.horizontal(|ui| {
                            ui.label(ludus_move_text(m, data));
                            if ui.small_button(tr!("cancel")).clicked() {
                                cancel = Some(n);
                            }
                        });
                    }
                    ui.label(egui::RichText::new(tr!("ludus-left", money = money)).weak());
                });
            ui.add_space(4.0);
        }
        if let Some(n) = cancel {
            chosen.remove(n);
        }
        ui.horizontal_wrapped(|ui| {
            for (i, site) in planned.plots.iter().enumerate() {
                let name = plot_name(&planned, i, *site, data);
                ui.selectable_value(&mut self.shown_plot, i, name);
            }
            match planned.parcel_price(&data.ludus) {
                Some(price) => {
                    let allowed = i64::from(price) <= money;
                    let buy = egui::Button::new(tr!("ludus-buy-parcel", price = price));
                    if ui.add_enabled(allowed, buy).clicked() {
                        chosen.push(LudusMove::BuyParcel);
                    }
                }
                None => {
                    ui.label(egui::RichText::new(tr!("ludus-no-parcel")).weak());
                }
            }
        });
        ui.add_space(4.0);
        let plot = self.shown_plot.min(planned.plots.len().saturating_sub(1));
        egui::ScrollArea::vertical().show(ui, |ui| {
            egui::Frame::group(ui.style())
                .fill(ui.visuals().extreme_bg_color)
                .show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    match planned.plots.get(plot).copied().flatten() {
                        Some(site) => site_card(ui, (&planned, plot, site), chosen, data),
                        None => catalog(ui, (&planned, plot, money), chosen, data),
                    }
                });
        });
    }
}

/// The club's `ludus` and `money` once `moves` are made, each in turn if it may be.
fn planned(ludus: &Ludus, moves: &[LudusMove], money: i64, data: &DesignData) -> (Ludus, i64) {
    let mut ludus = ludus.clone();
    let mut money = money;
    for &m in moves {
        if let Some(cost) = ludus.make(m, money, &data.ludus) {
            money -= i64::from(cost);
        }
    }
    (ludus, money)
}

/// « Terrain 2 : Bains (2e), en travaux », or « Terrain 1 : libre ».
fn plot_name(ludus: &Ludus, plot: usize, site: Option<Site>, data: &DesignData) -> String {
    let what = site.map_or_else(
        || tr!("ludus-free"),
        |site| {
            let rank = ludus.copy_rank(plot).unwrap_or(0) + 1;
            let name = tr!(
                "ludus-copy",
                building = building_name(site.building, data),
                rank = rank
            );
            if site.weeks_left > 0 {
                tr!("ludus-in-works", what = name)
            } else {
                name
            }
        },
    );
    tr!(
        "labelled",
        label = tr!("ludus-plot", plot = plot + 1),
        text = what
    )
}

/// The building on `plot`: what it does at its copy's share, its upkeep, its works
/// left, and the button demolishing it, or taking back its construction decided this
/// week.
fn site_card(
    ui: &mut egui::Ui,
    (ludus, plot, site): (&Ludus, usize, Site),
    chosen: &mut Vec<LudusMove>,
    data: &DesignData,
) {
    let rules = &data.ludus;
    let Some(building) = rules.buildings.get(site.building) else {
        return;
    };
    let rank = ludus.copy_rank(plot).unwrap_or(0);
    let share = rules.copy_percents.get(rank).copied().unwrap_or(0);
    ui.horizontal(|ui| {
        let title = egui::RichText::new(building_name(site.building, data)).strong();
        ui.label(title.color(theme::BRONZE));
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if ui.button(tr!("ludus-demolish")).clicked() {
                let decided = chosen
                    .iter()
                    .rposition(|m| matches!(*m, LudusMove::Build { plot: p, .. } if p == plot));
                match decided {
                    Some(n) => {
                        chosen.remove(n);
                    }
                    None => chosen.push(LudusMove::Demolish(plot)),
                }
            }
        });
    });
    ui.horizontal_wrapped(|ui| {
        wrapped_chip(ui, None, tr!(&format!("family-{}", building.family)));
        wrapped_chip(ui, None, effect_text(building.effect, share));
        wrapped_chip(ui, None, tr!("ludus-share", share = share));
        wrapped_chip(ui, None, tr!("ludus-upkeep", upkeep = building.upkeep));
        let works = if site.weeks_left > 0 {
            tr!("ludus-works-left", weeks = site.weeks_left)
        } else {
            tr!("ludus-standing")
        };
        wrapped_chip(ui, None, works);
    });
}

/// The buildings free `plot` may take, each with what it costs and does as the next
/// copy, and the button building it, greyed out when the money or the copies run out.
fn catalog(
    ui: &mut egui::Ui,
    (ludus, plot, money): (&Ludus, usize, i64),
    chosen: &mut Vec<LudusMove>,
    data: &DesignData,
) {
    let rules = &data.ludus;
    ui.label(egui::RichText::new(tr!("ludus-catalog")).strong());
    for (b, building) in rules.buildings.iter().enumerate() {
        let copies = ludus.copies(b);
        let share = rules.copy_percents.get(copies).copied().unwrap_or(0);
        let m = LudusMove::Build { plot, building: b };
        let allowed = ludus.cost(m, rules).is_some_and(|c| i64::from(c) <= money);
        egui::Frame::group(ui.style()).show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal(|ui| {
                ui.label(egui::RichText::new(building_name(b, data)).strong());
                ui.label(egui::RichText::new(tr!(&format!("family-{}", building.family))).weak());
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let build = egui::Button::new(tr!("ludus-build"));
                    if ui.add_enabled(allowed, build).clicked() {
                        chosen.push(m);
                    }
                });
            });
            ui.horizontal_wrapped(|ui| {
                wrapped_chip(ui, None, effect_text(building.effect, share));
                wrapped_chip(ui, None, tr!("ludus-price", price = building.price));
                wrapped_chip(ui, None, tr!("ludus-upkeep", upkeep = building.upkeep));
                wrapped_chip(ui, None, tr!("ludus-weeks", weeks = building.weeks));
                let most = rules.copy_percents.len();
                wrapped_chip(ui, None, tr!("ludus-copies", copies = copies, most = most));
            });
        });
    }
}
