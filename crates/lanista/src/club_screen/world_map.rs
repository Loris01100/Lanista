//! The world map: every colosseum where it stands, the club's fighters on their
//! continents; a colosseum chosen on it, each fighter sent there.

use lanista_core::colosseum::{ColosseumId, Continent};
use lanista_core::fighter::career::{Activity, Duty, Status, WeekPlan};
use lanista_core::state::located;
use reesch::egui;

use super::{ClubScreen, View, here};
use crate::lang::tr;
use crate::texts::{activity_name, colosseum_name, continent_name, stage_name};
use crate::theme;

/// The map's height for its width: a world twice as wide as high.
const ASPECT: f32 = 0.5;
/// Share of the view's height the map may take, the chosen colosseum's panel below.
const MAP_SHARE: f32 = 0.6;
/// How far from a colosseum's dot, in points, a click still picks it.
const PICK_RADIUS: f32 = 14.0;

impl ClubScreen {
    /// The world map, each colosseum a dot as large as its prestige, the club's own
    /// continent named as home, and under each continent's name the club's fighters on
    /// it; clicking a dot chooses it, and the panel below sends the club's fighters
    /// there (`send`).
    pub(super) fn world_map(&mut self, ui: &mut egui::Ui) {
        ui.heading(tr!("view-map"));
        ui.label(egui::RichText::new(tr!("map-intro")).small().weak());
        ui.add_space(4.0);
        let width = ui.available_width();
        let height = (width * ASPECT).min(ui.available_height() * MAP_SHARE);
        let (rect, response) =
            ui.allocate_exact_size(egui::vec2(height / ASPECT, height), egui::Sense::click());
        let data = &self.data;
        let at = |id: ColosseumId| {
            let (x, y) = data.world.colosseum(id).map_or((0, 0), |c| c.map);
            rect.min + egui::vec2(percent(x) * rect.width(), percent(y) * rect.height())
        };
        let painter = ui.painter_at(rect);
        painter.rect_filled(rect, 4.0, theme::WOOD_DARK);
        painter.rect_stroke(
            rect,
            4.0,
            egui::Stroke::new(1.0_f32, theme::WOOD_LIGHTEST),
            egui::StrokeKind::Inside,
        );
        let small = egui::TextStyle::Small.resolve(ui.style());
        let body = egui::TextStyle::Body.resolve(ui.style());
        self.continent_names(&painter, &at, (&body, &small));
        let hovered = response.hover_pos();
        let mut pointed = None;
        for (id, rules) in (0..).map(ColosseumId).zip(&data.world.colosseums) {
            let centre = at(id);
            #[expect(clippy::cast_precision_loss, reason = "a prestige from 1 to 5")]
            let radius = 3.0 + 2.0 * rules.prestige as f32;
            let chosen = self.map_chosen == Some(id);
            let near = hovered.is_some_and(|p| p.distance(centre) <= PICK_RADIUS);
            if near {
                pointed = Some(id);
            }
            let fill = if chosen { theme::BRONZE } else { theme::SEA };
            painter.circle_filled(centre, radius, fill);
            if chosen || near {
                painter.circle_stroke(
                    centre,
                    radius + 3.0,
                    egui::Stroke::new(1.5_f32, theme::PARCHMENT),
                );
            }
            let label = egui::pos2(centre.x, centre.y + radius + 2.0);
            let name = colosseum_name(id, data);
            painter.text(
                label,
                egui::Align2::CENTER_TOP,
                name,
                small.clone(),
                theme::PARCHMENT,
            );
        }
        if response.clicked()
            && let Some(id) = pointed
        {
            self.map_chosen = Some(id);
        }
        ui.add_space(8.0);
        if let Some(id) = self.map_chosen {
            egui::ScrollArea::vertical()
                .id_salt("map-panel")
                .show(ui, |ui| self.send(ui, id));
        } else {
            ui.label(egui::RichText::new(tr!("map-choose")).weak());
        }
    }

    /// Each continent's name over its colosseums, the club's named as home, and under
    /// it the club's fighters on it; `at` places a colosseum on the map.
    fn continent_names(
        &self,
        painter: &egui::Painter,
        at: &dyn Fn(ColosseumId) -> egui::Pos2,
        (body, small): (&egui::FontId, &egui::FontId),
    ) {
        let data = &self.data;
        for continent in Continent::ALL {
            let dots: Vec<egui::Pos2> = data.world.colosseums_of(continent).map(at).collect();
            let Some(top) = dots.iter().map(|p| p.y).reduce(f32::min) else {
                continue;
            };
            #[expect(clippy::cast_precision_loss, reason = "a few colosseums")]
            let x = dots.iter().map(|p| p.x).sum::<f32>() / dots.len() as f32;
            let mut name = continent_name(continent);
            if continent == self.state.continent {
                name = tr!("continent-home", continent = name);
            }
            let title = egui::pos2(x, top - 28.0);
            painter.text(
                title,
                egui::Align2::CENTER_BOTTOM,
                name,
                body.clone(),
                theme::PARCHMENT,
            );
            let fighters: Vec<String> = self
                .state
                .fighters
                .iter()
                .filter(|f| f.status == Status::Active && here(f, &self.state) == continent)
                .map(|f| stage_name(&f.name))
                .collect();
            if !fighters.is_empty() {
                let line = fighters.join(", ");
                painter.text(
                    title,
                    egui::Align2::CENTER_TOP,
                    line,
                    small.clone(),
                    theme::BRONZE,
                );
            }
        }
    }

    /// Colosseum `id`'s name, then a row per fighter of the club in activity: its name,
    /// which opens its sheet, its week, and a button sending it there. Sent, its orders
    /// fight there, every week if they already fought every week, else every other
    /// week, and its week is the fight there, or the journey first (`located`).
    fn send(&mut self, ui: &mut egui::Ui, id: ColosseumId) {
        let data = &self.data;
        let Some(rules) = data.world.colosseum(id) else {
            return;
        };
        let place = colosseum_name(id, data);
        let fee = self.state.entry_fee(id, data).unwrap_or(0);
        ui.strong(tr!(
            "map-chosen",
            place = place.clone(),
            continent = continent_name(rules.continent),
            prestige = rules.prestige,
            fee = fee
        ));
        let mut goto = None;
        let decisions = &mut self.decisions;
        let rows = self
            .state
            .fighters
            .iter()
            .zip(decisions.plans.iter_mut().zip(&mut decisions.orders))
            .enumerate()
            .filter(|(_, (f, _))| f.status == Status::Active);
        egui::Grid::new("map-fighters")
            .striped(true)
            .show(ui, |ui| {
                for (i, (f, (plan, orders))) in rows {
                    if ui.link(stage_name(&f.name)).clicked() {
                        goto = Some(View::Sheet(i));
                    }
                    ui.label(continent_name(here(f, &self.state)));
                    ui.label(egui::RichText::new(activity_name(plan.activity, data)).weak());
                    let sent =
                        matches!(orders.activity, Duty::Fight(c) | Duty::Alternate(c) if c == id);
                    let button = egui::Button::new(tr!("map-send", place = place.clone()));
                    let response = ui
                        .add_enabled(!f.homebound && !sent, button)
                        .on_hover_text(tr!("map-send-hint", travel = data.world.travel_fee));
                    if response.clicked() {
                        orders.activity = match orders.activity {
                            Duty::Fight(_) => Duty::Fight(id),
                            _ => Duty::Alternate(id),
                        };
                        let there = WeekPlan {
                            activity: Activity::Fight(id),
                            ..*plan
                        };
                        *plan = located(there, f, self.state.continent, data);
                    }
                    ui.end_row();
                }
            });
        if let Some(view) = goto {
            self.go_to(view);
        }
    }
}

/// `value` percent as a share from 0 to 1.
fn percent(value: u32) -> f32 {
    u16::try_from(value.min(100)).map_or(1.0, |v| f32::from(v) / 100.0)
}
