//! The ludus view: the plan of the plots on parchment, then the plot chosen, the
//! specialties and this week's moves beside it.

use lanista_core::design_data::DesignData;
use lanista_core::ludus::{Ludus, LudusMove, LudusRules, SPECIALTY_BUILDINGS, Site};
use reesch::egui::{self, Align, Align2, Color32, Layout, Margin, RichText, Stroke, vec2};

use super::ClubScreen;
use crate::lang::tr;
use crate::texts::{building_name, effect_text, ludus_move_text, school_name};
use crate::theme::{self, BRONZE, INK, LAUREL};

/// A building's tile, darker than the plan's parchment.
const TILE: Color32 = Color32::from_rgb(0xDC, 0xC9, 0xA6);
/// The dashed outline of a free plot or a parcel.
const DASH: Color32 = Color32::from_rgb(0xA8, 0x91, 0x6C);
/// The text of the button demolishing, on the wood.
const DEMOLISH: Color32 = Color32::from_rgb(0xE8, 0x83, 0x6F);
/// Plots in a row of the plan.
const COLUMNS: u8 = 4;
const TILE_HEIGHT: f32 = 120.0;
const GAP: f32 = 10.0;

/// What a tile of the plan shows.
#[derive(Clone, Copy)]
enum Tile {
    /// A building, and which copy of it it is, from 0.
    Built(Site, usize),
    Free,
    /// A neighboring parcel, still to buy.
    Parcel,
}

impl ClubScreen {
    /// The plan, as it will be once this week's moves are made, then the card of the
    /// tile chosen: its building, the buildings a free plot may take, or the parcel to
    /// buy; the specialties and this week's moves under it.
    pub(super) fn ludus(&mut self, ui: &mut egui::Ui) {
        let data = &self.data;
        let rules = &data.ludus;
        let chosen = &mut self.decisions.ludus;
        let (planned, costs, money) = planned(&self.state.ludus, chosen, self.state.money, data);
        let start = usize::try_from(rules.plots).unwrap_or(0);
        let tiles = (start + rules.parcel_prices.len()).max(planned.plots.len());
        let mut shown = self.shown_plot.min(tiles.saturating_sub(1));
        let mut cancel = None;
        ui.horizontal_top(|ui| {
            let left = ui.available_width() * 0.6;
            ui.vertical(|ui| {
                ui.set_width(left);
                ui.heading(RichText::new(school_name(&self.state, data)).color(BRONZE));
                ui.label(RichText::new(tr!("ludus-intro")).small().weak());
                let board = egui::Frame::default()
                    .fill(theme::PARCHMENT)
                    .stroke(Stroke::new(3.0_f32, theme::WOOD_LIGHTEST))
                    .corner_radius(6)
                    .inner_margin(Margin::same(16));
                board.show(ui, |ui| {
                    ui.spacing_mut().item_spacing = vec2(GAP, GAP);
                    let gaps = GAP * f32::from(COLUMNS - 1);
                    let width = (ui.available_width() - gaps) / f32::from(COLUMNS);
                    let all: Vec<usize> = (0..tiles).collect();
                    for row in all.chunks(usize::from(COLUMNS)) {
                        ui.horizontal(|ui| {
                            for &i in row {
                                let tile = match planned.plots.get(i) {
                                    Some(Some(site)) => {
                                        Tile::Built(*site, planned.copy_rank(i).unwrap_or(0))
                                    }
                                    Some(None) => Tile::Free,
                                    None => Tile::Parcel,
                                };
                                let size = vec2(width, TILE_HEIGHT);
                                if tile_button(ui, size, tile, i == shown, data).clicked() {
                                    shown = i;
                                }
                            }
                        });
                    }
                    entrance(ui);
                });
            });
            ui.vertical(|ui| {
                egui::ScrollArea::vertical().show(ui, |ui| {
                    egui::Frame::default()
                        .fill(theme::WOOD_LIGHT)
                        .stroke(Stroke::new(2.0_f32, theme::WOOD_LIGHTEST))
                        .corner_radius(6)
                        .inner_margin(Margin::symmetric(20, 16))
                        .show(ui, |ui| {
                            ui.set_width(ui.available_width());
                            match planned.plots.get(shown) {
                                Some(Some(site)) => {
                                    site_card(ui, (&planned, shown, *site), chosen, data);
                                }
                                Some(None) => catalog(ui, (&planned, shown, money), chosen, data),
                                None => parcel_card(ui, (&planned, money), chosen, rules),
                            }
                        });
                    specialties(ui, &planned, data);
                    let ludus = (&self.state.ludus, &planned);
                    cancel = week(ui, ludus, (chosen, &costs, money), data);
                });
            });
        });
        self.shown_plot = shown;
        if let Some(n) = cancel {
            self.decisions.ludus.remove(n);
        }
    }
}

/// The club's `ludus` and `money` once `moves` are made, each in turn if it may be,
/// and what each one cost; `None` for one refused.
fn planned(
    ludus: &Ludus,
    moves: &[LudusMove],
    money: i64,
    data: &DesignData,
) -> (Ludus, Vec<Option<u32>>, i64) {
    let mut ludus = ludus.clone();
    let mut money = money;
    let costs = moves
        .iter()
        .map(|&m| {
            let cost = ludus.make(m, money, &data.ludus);
            money -= cost.map_or(0, i64::from);
            cost
        })
        .collect();
    (ludus, costs, money)
}

/// The share of its effect the first `copies` copies of a building give together.
fn total_share(rules: &LudusRules, copies: usize) -> u32 {
    rules.copy_percents.iter().take(copies).sum()
}

fn panel() -> egui::Frame {
    egui::Frame::default()
        .fill(theme::WOOD_DARK)
        .stroke(Stroke::new(1.0_f32, theme::WOOD_LIGHTER))
        .corner_radius(6)
        .inner_margin(Margin::symmetric(20, 12))
}

fn title(ui: &mut egui::Ui, text: String) {
    ui.label(RichText::new(text).heading().size(22.0).color(BRONZE));
}

fn bronze_button(text: String) -> egui::Button<'static> {
    egui::Button::new(RichText::new(text).strong().color(INK)).fill(BRONZE)
}

/// A dashed outline around `rect`.
fn dashed(painter: &egui::Painter, rect: egui::Rect, stroke: Stroke) {
    let corners = [
        rect.left_top(),
        rect.right_top(),
        rect.right_bottom(),
        rect.left_bottom(),
        rect.left_top(),
    ];
    painter.extend(egui::Shape::dashed_line(&corners, stroke, 6.0, 4.0));
}

/// A tile of the plan, framed in bronze when chosen.
fn tile_button(
    ui: &mut egui::Ui,
    area: egui::Vec2,
    tile: Tile,
    chosen: bool,
    data: &DesignData,
) -> egui::Response {
    let (rect, response) = ui.allocate_exact_size(area, egui::Sense::click());
    let painter = ui.painter().clone();
    let faded = if matches!(tile, Tile::Parcel) && !chosen {
        0.6
    } else {
        1.0
    };
    let ink = theme::WOOD_LIGHTER.gamma_multiply(faded);
    let line = Stroke::new(2.0_f32, ink);
    match tile {
        Tile::Built(site, rank) => {
            painter.rect_filled(rect, 6.0_f32, TILE);
            let border = Stroke::new(2.0_f32, theme::WOOD_LIGHTEST);
            painter.rect_stroke(rect, 6.0_f32, border, egui::StrokeKind::Inside);
            let mut inner = ui.new_child(
                egui::UiBuilder::new()
                    .max_rect(rect.shrink(10.0))
                    .layout(Layout::top_down(Align::Min)),
            );
            // Its labels let the click through to the tile.
            inner.style_mut().interaction.selectable_labels = false;
            building_tile(&mut inner, site, rank, data);
        }
        Tile::Free => {
            dashed(&painter, rect.shrink(1.0), Stroke::new(2.0_f32, DASH));
            let c = rect.center() - vec2(0.0, 12.0);
            painter.line_segment([c - vec2(10.0, 0.0), c + vec2(10.0, 0.0)], line);
            painter.line_segment([c - vec2(0.0, 10.0), c + vec2(0.0, 10.0)], line);
            tile_caption(&painter, rect, tr!("ludus-free-plot"), ink);
        }
        Tile::Parcel => {
            dashed(
                &painter,
                rect.shrink(1.0),
                Stroke::new(2.0_f32, DASH.gamma_multiply(faded)),
            );
            let c = rect.center() - vec2(0.0, 12.0);
            painter.circle_stroke(c - vec2(0.0, 3.0), 5.0, line);
            let body = egui::Rect::from_center_size(c + vec2(0.0, 3.0), vec2(14.0, 9.0));
            painter.rect_filled(body, 1.0_f32, ink);
            tile_caption(&painter, rect, tr!("ludus-parcel"), ink);
        }
    }
    if chosen {
        let ring = Stroke::new(3.0_f32, BRONZE);
        painter.rect_stroke(rect, 6.0_f32, ring, egui::StrokeKind::Inside);
    }
    response.on_hover_cursor(egui::CursorIcon::PointingHand)
}

fn tile_caption(painter: &egui::Painter, rect: egui::Rect, text: String, ink: Color32) {
    let at = rect.center() + vec2(0.0, 14.0);
    let font = egui::FontId::proportional(15.0);
    painter.text(at, Align2::CENTER_CENTER, text, font, ink);
}

/// A building's tile: its name and family, then which copy it is and its works.
fn building_tile(ui: &mut egui::Ui, site: Site, rank: usize, data: &DesignData) {
    let rules = &data.ludus;
    let name = building_name(site.building, data);
    ui.label(RichText::new(name).heading().size(16.0).color(INK));
    if let Some(building) = rules.buildings.get(site.building) {
        let family = tr!(&format!("family-{}", building.family));
        let family = RichText::new(family).small().italics();
        ui.label(family.color(theme::WOOD_LIGHTER));
    }
    ui.with_layout(Layout::bottom_up(Align::Min), |ui| {
        ui.spacing_mut().item_spacing.y = 2.0;
        if site.weeks_left > 0 {
            egui::Frame::default()
                .fill(theme::RED_INK)
                .corner_radius(8)
                .inner_margin(Margin::symmetric(8, 1))
                .show(ui, |ui| {
                    let works = RichText::new(tr!("ludus-works")).small().strong();
                    ui.label(works.color(theme::PARCHMENT));
                });
        }
        let share = rules.copy_percents.get(rank).copied().unwrap_or(0);
        let copy = tr!("ludus-copy-share", rank = rank + 1, share = share);
        ui.label(RichText::new(copy).small().strong().color(INK));
    });
}

/// The line « Entrée » closing the plan.
fn entrance(ui: &mut egui::Ui) {
    let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), 28.0), egui::Sense::hover());
    let painter = ui.painter();
    dashed(
        painter,
        egui::Rect::from_min_max(rect.left_top(), rect.right_top()),
        Stroke::new(2.0_f32, DASH),
    );
    let font = egui::FontId::new(14.0, egui::TextStyle::Heading.resolve(ui.style()).family);
    let text = tr!("ludus-entrance");
    painter.text(
        rect.center(),
        Align2::CENTER_CENTER,
        text,
        font,
        theme::WOOD_LIGHTER,
    );
}

/// The building on `plot`: what all its copies do together and what one more would
/// add, its upkeep and works left, and the button demolishing it, or taking back its
/// construction decided this week.
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
    let copies = ludus.copies(site.building);
    let most = rules.copy_percents.len();
    let family = tr!(&format!("family-{}", building.family));
    title(ui, building_name(site.building, data));
    let what = tr!(
        "ludus-family-copies",
        family = family,
        copies = copies,
        most = most
    );
    ui.label(RichText::new(what).italics().weak());
    ui.label(tr!(&format!("building-{}-desc", building.key)));
    ui.label(RichText::new(tr!("ludus-total")).small().weak());
    let total = effect_text(building.effect, total_share(rules, copies));
    ui.label(RichText::new(total).strong().color(LAUREL));
    let next = match rules.copy_percents.get(copies) {
        Some(&share) => tr!(
            "ludus-next",
            effect = effect_text(building.effect, total_share(rules, copies + 1)),
            share = share
        ),
        None => tr!("ludus-most", most = most),
    };
    ui.label(RichText::new(next).small().weak());
    if site.weeks_left > 0 {
        ui.label(tr!("ludus-works-left", weeks = site.weeks_left));
        let done = building.weeks.saturating_sub(site.weeks_left);
        #[expect(clippy::cast_precision_loss, reason = "a few weeks")]
        let part = done as f32 / building.weeks.max(1) as f32;
        ui.add(egui::ProgressBar::new(part).fill(BRONZE));
    }
    let decided = chosen
        .iter()
        .rposition(|m| matches!(*m, LudusMove::Build { plot: p, .. } if p == plot));
    ui.horizontal(|ui| {
        let upkeep = tr!("ludus-upkeep-week", upkeep = building.upkeep);
        ui.label(RichText::new(upkeep).weak());
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            let text = if decided.is_some() {
                tr!("ludus-cancel-build")
            } else {
                tr!("ludus-demolish")
            };
            let button = egui::Button::new(RichText::new(text).strong().color(DEMOLISH))
                .fill(Color32::TRANSPARENT)
                .stroke(Stroke::new(2.0_f32, theme::BLOOD));
            if ui.add(button).clicked() {
                match decided {
                    Some(n) => {
                        chosen.remove(n);
                    }
                    None => chosen.push(LudusMove::Demolish(plot)),
                }
            }
        });
    });
    ui.label(RichText::new(tr!("ludus-demolish-note")).small().weak());
}

/// The buildings free `plot` may take, each with what it would add as the next copy
/// and what it costs, and the button building it, greyed out when the money runs out.
fn catalog(
    ui: &mut egui::Ui,
    (ludus, plot, money): (&Ludus, usize, i64),
    chosen: &mut Vec<LudusMove>,
    data: &DesignData,
) {
    let rules = &data.ludus;
    title(ui, tr!("ludus-free-plot"));
    ui.label(RichText::new(tr!("ludus-catalog")).italics().weak());
    let most = rules.copy_percents.len();
    for (b, building) in rules.buildings.iter().enumerate() {
        let copies = ludus.copies(b);
        let Some(&share) = rules.copy_percents.get(copies) else {
            continue;
        };
        let m = LudusMove::Build { plot, building: b };
        let allowed = ludus.cost(m, rules).is_some_and(|c| i64::from(c) <= money);
        egui::Frame::default()
            .fill(theme::WOOD)
            .corner_radius(4)
            .inner_margin(Margin::symmetric(12, 8))
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if ui
                        .add_enabled(allowed, bronze_button(tr!("ludus-build")))
                        .clicked()
                    {
                        chosen.push(m);
                    }
                    ui.with_layout(Layout::top_down(Align::Min), |ui| {
                        ui.spacing_mut().item_spacing.y = 2.0;
                        let name = RichText::new(building_name(b, data)).heading().size(16.0);
                        ui.label(name);
                        let family = tr!(&format!("family-{}", building.family));
                        let what = tr!(
                            "ludus-family-copies",
                            family = family,
                            copies = copies,
                            most = most
                        );
                        ui.label(RichText::new(what).small().italics().weak());
                        let effect = effect_text(building.effect, share);
                        ui.label(RichText::new(effect).strong().color(LAUREL));
                        let terms = tr!(
                            "ludus-terms",
                            price = building.price,
                            upkeep = building.upkeep,
                            weeks = building.weeks
                        );
                        ui.label(RichText::new(terms).small().weak());
                    });
                });
            });
    }
}

/// The next parcel and the button buying it.
fn parcel_card(
    ui: &mut egui::Ui,
    (ludus, money): (&Ludus, i64),
    chosen: &mut Vec<LudusMove>,
    rules: &LudusRules,
) {
    title(ui, tr!("ludus-parcel"));
    ui.label(RichText::new(tr!("ludus-parcel-subtitle")).italics().weak());
    ui.label(tr!("ludus-parcel-text"));
    match ludus.parcel_price(rules) {
        Some(price) => {
            let buy = bronze_button(tr!("ludus-buy", price = price))
                .min_size(vec2(ui.available_width(), 44.0));
            if ui.add_enabled(i64::from(price) <= money, buy).clicked() {
                chosen.push(LudusMove::BuyParcel);
            }
        }
        None => {
            ui.label(RichText::new(tr!("ludus-no-parcel")).weak());
        }
    }
}

/// Each family's buildings standing, one square each, toward its specialty.
fn specialties(ui: &mut egui::Ui, ludus: &Ludus, data: &DesignData) {
    panel().show(ui, |ui| {
        ui.set_width(ui.available_width());
        ui.label(
            RichText::new(tr!("ludus-specialties"))
                .strong()
                .color(BRONZE),
        );
        egui::Grid::new("ludus-specialties").show(ui, |ui| {
            for family in &data.ludus.families {
                let standing = ludus.family_standing(&family.key, &data.ludus);
                ui.label(tr!(&format!("family-{}", family.key)));
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 4.0;
                    for k in 0..SPECIALTY_BUILDINGS {
                        let (rect, _) =
                            ui.allocate_exact_size(vec2(12.0, 12.0), egui::Sense::hover());
                        if k < standing {
                            ui.painter().rect_filled(rect, 2.0_f32, BRONZE);
                        }
                        let border = Stroke::new(2.0_f32, BRONZE);
                        ui.painter()
                            .rect_stroke(rect, 2.0_f32, border, egui::StrokeKind::Inside);
                    }
                });
                let specialty = tr!(&format!("specialty-{}", family.specialty));
                if standing >= SPECIALTY_BUILDINGS {
                    ui.label(RichText::new(specialty).strong().color(BRONZE));
                } else {
                    let at = tr!(
                        "ludus-specialty-at",
                        specialty = specialty,
                        needed = SPECIALTY_BUILDINGS
                    );
                    ui.label(RichText::new(at).weak());
                }
                ui.end_row();
            }
        });
    });
}

/// This week's moves, each with its cost and a button cancelling it, the works going
/// on, and the buildings' upkeep; the move to cancel, if any.
fn week(
    ui: &mut egui::Ui,
    (now, planned): (&Ludus, &Ludus),
    (chosen, costs, money): (&[LudusMove], &[Option<u32>], i64),
    data: &DesignData,
) -> Option<usize> {
    let mut cancel = None;
    panel().show(ui, |ui| {
        ui.set_width(ui.available_width());
        ui.label(
            RichText::new(tr!("market-this-week"))
                .strong()
                .color(BRONZE),
        );
        for (n, &m) in chosen.iter().enumerate() {
            ui.horizontal(|ui| {
                ui.label(ludus_move_text(m, data));
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if ui.small_button(tr!("cancel")).clicked() {
                        cancel = Some(n);
                    }
                    if let Some(cost) = costs.get(n).copied().flatten().filter(|&c| c > 0) {
                        ui.label(RichText::new(tr!("ludus-cost", cost = cost)).strong());
                    }
                });
            });
        }
        // The constructions under way, unless demolished this week.
        let works: Vec<Site> = now
            .plots
            .iter()
            .zip(&planned.plots)
            .filter_map(|(site, then)| site.filter(|s| s.weeks_left > 0 && *then == Some(*s)))
            .collect();
        for site in &works {
            let building = building_name(site.building, data);
            ui.label(tr!(
                "ludus-works-line",
                building = building,
                weeks = site.weeks_left
            ));
        }
        if chosen.is_empty() && works.is_empty() {
            ui.label(RichText::new(tr!("ludus-no-works")).weak());
        }
        ui.separator();
        let upkeep = planned.clone().week(&data.ludus).0;
        ui.horizontal(|ui| {
            ui.label(RichText::new(tr!("ludus-upkeep-total")).weak());
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                ui.label(RichText::new(tr!("ludus-per-week", upkeep = upkeep)).weak());
            });
        });
        if !chosen.is_empty() {
            ui.label(RichText::new(tr!("ludus-left", money = money)).weak());
        }
    });
    cancel
}
