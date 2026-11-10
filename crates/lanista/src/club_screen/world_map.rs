//! The world map, over the whole view: the land drawn from `assets/ui/world_*.png`, every
//! colosseum where it stands, the club's fighters on their continents and their
//! journeys; a colosseum chosen on it, a panel beside it sends each fighter there.

use lanista_core::colosseum::{ColosseumId, Continent};
use lanista_core::fighter::career::{Activity, Duty, Status, WeekPlan};
use lanista_core::state::located;
use reesch::egui;
use reesch::load_smooth_texture;

use super::{ClubScreen, View, here};
use crate::lang::tr;
use crate::texts::{activity_name, colosseum_name, continent_name, stage_name};
use crate::theme;

/// The longitude and latitude at the middle of the view: the 150th meridian west, at
/// the edges, crosses the Pacific and no continent.
const CENTRE: (f32, f32) = (30.0, 8.5);
/// Degrees of longitude and latitude always in view, whatever the window's shape: every
/// colosseum, from Rapa Nui to Ngāpuhi, and the land from 75° N to 58° S.
const SPAN: (f32, f32) = (310.0, 133.0);
/// Degrees between two meridians or two parallels drawn.
const GRATICULE: i32 = 15;
/// How far from a colosseum's dot, in points, a click still picks it.
const PICK_RADIUS: f32 = 14.0;
/// Space around the title and the panel, from the map's edges.
const INSET: f32 = 14.0;
/// The chosen colosseum's panel's width, at most.
const PANEL_WIDTH: f32 = 420.0;
/// The sea, darker than the wood so the land stands out.
const SEA: egui::Color32 = egui::Color32::from_rgb(0x1F, 0x1A, 0x16);

/// Where each continent's name is written, on its land: longitude, latitude.
fn anchor(continent: Continent) -> (f32, f32) {
    match continent {
        Continent::Europe => (18.0, 55.0),
        Continent::Africa => (19.0, 15.0),
        Continent::Asia => (88.0, 60.0),
        Continent::Americas => (-102.0, 52.0),
        Continent::Oceania => (134.0, -24.0),
    }
}

/// Places on the map: an equirectangular projection, as many points to a degree of
/// longitude as of latitude, `CENTRE` at the middle of `rect` and `SPAN` within it.
struct Projection {
    rect: egui::Rect,
    /// Points to a degree.
    scale: f32,
}

impl Projection {
    fn new(rect: egui::Rect) -> Self {
        let scale = (rect.width() / SPAN.0).min(rect.height() / SPAN.1);
        Self { rect, scale }
    }

    /// Where longitude `lon` and latitude `lat` are drawn, the longitude brought within
    /// half a turn of the centre's.
    fn at(&self, (lon, lat): (f32, f32)) -> egui::Pos2 {
        let east = (lon - CENTRE.0 + 180.0).rem_euclid(360.0) - 180.0;
        self.unwrapped((CENTRE.0 + east, lat))
    }

    /// Where `lon` and `lat` are drawn, the longitude taken as it is.
    fn unwrapped(&self, (lon, lat): (f32, f32)) -> egui::Pos2 {
        let middle = self.rect.center();
        egui::pos2(
            middle.x + (lon - CENTRE.0) * self.scale,
            middle.y - (lat - CENTRE.1) * self.scale,
        )
    }
}

/// A colosseum's place in degrees.
fn degrees((lon, lat): (i32, i32)) -> (f32, f32) {
    let degrees = |v: i32| i16::try_from(v).map_or(0.0, f32::from);
    (degrees(lon), degrees(lat))
}

impl ClubScreen {
    /// The world map over the whole view, each colosseum a dot as large as its prestige,
    /// the highest of each continent ringed; under each continent's name, the club's own
    /// named as home, the club's fighters on it, and a dotted line for each one
    /// travelling this week. Clicking a dot chooses it and opens its panel (`send`), on
    /// the side away from it.
    pub(super) fn world_map(&mut self, ui: &mut egui::Ui) {
        let rect = ui.max_rect();
        let response = ui.allocate_rect(rect, egui::Sense::click());
        let map = Projection::new(rect);
        let painter = ui.painter_at(rect);
        painter.rect_filled(rect, 0.0, SEA);
        graticule(&painter, &map);
        land(ui.ctx(), &painter, &map);
        let small = egui::TextStyle::Small.resolve(ui.style());
        let body = egui::TextStyle::Body.resolve(ui.style());
        self.journeys(&painter, &map, &small);
        self.continent_names(&painter, &map, &small);
        let pointed = self.colosseums_on(&painter, &map, response.hover_pos(), &small);
        let panel = self.map_chosen.map(|id| {
            let x = self
                .data
                .world
                .colosseum(id)
                .map_or(rect.center().x, |c| map.at(degrees(c.place)).x);
            let width = PANEL_WIDTH.min(rect.width() - 2.0 * INSET);
            let shrunk = rect.shrink(INSET);
            let left = if x > rect.center().x {
                shrunk.left()
            } else {
                shrunk.right() - width
            };
            egui::Rect::from_x_y_ranges(left..=left + width, shrunk.y_range())
        });
        let on_panel = |p: egui::Pos2| panel.is_some_and(|r| r.contains(p));
        if let Some(id) = pointed
            && response.hover_pos().is_some_and(|p| !on_panel(p))
        {
            response
                .clone()
                .on_hover_ui_at_pointer(|ui| self.details(ui, id));
        }
        if response.clicked()
            && response
                .interact_pointer_pos()
                .is_some_and(|p| !on_panel(p))
        {
            self.map_chosen = pointed;
        }
        title(ui, rect, &body);
        if let (Some(id), Some(panel)) = (self.map_chosen, panel) {
            self.panel(ui, panel, id);
        }
    }

    /// Each colosseum's dot and name; returns the one under `hovered`, if any.
    fn colosseums_on(
        &self,
        painter: &egui::Painter,
        map: &Projection,
        hovered: Option<egui::Pos2>,
        small: &egui::FontId,
    ) -> Option<ColosseumId> {
        let data = &self.data;
        let dots: Vec<(ColosseumId, egui::Pos2, f32)> = (0..)
            .map(ColosseumId)
            .zip(&data.world.colosseums)
            .map(|(id, c)| {
                #[expect(clippy::cast_precision_loss, reason = "a prestige from 1 to 5")]
                let radius = 2.5 + 2.0 * c.prestige as f32;
                (id, map.at(degrees(c.place)), radius)
            })
            .collect();
        let pointed = hovered.and_then(|p| {
            dots.iter()
                .map(|&(id, centre, _)| (id, centre.distance(p)))
                .filter(|&(_, d)| d <= PICK_RADIUS)
                .min_by(|a, b| a.1.total_cmp(&b.1))
                .map(|(id, _)| id)
        });
        let mut taken: Vec<egui::Rect> = dots
            .iter()
            .map(|&(_, centre, r)| egui::Rect::from_center_size(centre, egui::Vec2::splat(2.0 * r)))
            .collect();
        for &(id, centre, radius) in &dots {
            let Some(rules) = data.world.colosseum(id) else {
                continue;
            };
            let major = data
                .world
                .colosseums_of(rules.continent)
                .filter_map(|c| data.world.colosseum(c))
                .all(|c| c.prestige <= rules.prestige);
            let chosen = self.map_chosen == Some(id);
            let colour = if chosen { theme::BRONZE } else { theme::SEA };
            if major {
                painter.circle_stroke(centre, radius + 3.5, egui::Stroke::new(1.0_f32, colour));
            }
            let outline = if pointed == Some(id) {
                egui::Stroke::new(1.6_f32, theme::PARCHMENT)
            } else {
                egui::Stroke::new(1.2_f32, SEA)
            };
            painter.circle(centre, radius, colour, outline);
            let gap = radius + if major { 6.0 } else { 3.0 };
            let name = painter.layout_no_wrap(
                colosseum_name(id, data),
                small.clone(),
                egui::Color32::PLACEHOLDER,
            );
            let at = label_place(centre, gap, name.size(), &taken);
            taken.push(egui::Rect::from_min_size(at, name.size()));
            let text = if chosen {
                theme::BRONZE
            } else {
                theme::PARCHMENT
            };
            haloed(painter, at, &name, text);
        }
        pointed
    }

    /// Each continent's name on its land, the club's named as home, and under it the
    /// club's fighters on it.
    fn continent_names(&self, painter: &egui::Painter, map: &Projection, small: &egui::FontId) {
        let display = egui::FontId::new(20.0, theme::display_family());
        for continent in Continent::ALL {
            let at = map.at(anchor(continent));
            let home = continent == self.state.continent;
            let mut name = continent_name(continent).to_uppercase();
            if home {
                name = tr!("continent-home", continent = name);
            }
            let colour = if home {
                theme::BRONZE
            } else {
                theme::PARCHMENT.gamma_multiply(0.6)
            };
            let title = painter.layout_no_wrap(name, display.clone(), egui::Color32::PLACEHOLDER);
            let top = egui::pos2(at.x - title.size().x / 2.0, at.y - title.size().y);
            haloed(painter, top, &title, colour);
            let fighters: Vec<String> = self
                .state
                .fighters
                .iter()
                .zip(&self.decisions.plans)
                .filter(|(f, plan)| {
                    f.status == Status::Active
                        && here(f, &self.state) == continent
                        && !matches!(plan.activity, Activity::Travel(_))
                })
                .map(|(f, _)| stage_name(&f.name))
                .collect();
            if !fighters.is_empty() {
                let line = painter.layout(
                    fighters.join(", "),
                    small.clone(),
                    egui::Color32::PLACEHOLDER,
                    (map.scale * 40.0).max(160.0),
                );
                let under = egui::pos2(at.x - line.size().x / 2.0, at.y + 2.0);
                haloed(painter, under, &line, theme::BRONZE);
            }
        }
    }

    /// A dotted curve for each of the club's fighters travelling this week, from its
    /// continent's name to its destination's, its name beside it.
    fn journeys(&self, painter: &egui::Painter, map: &Projection, small: &egui::FontId) {
        let travels = self
            .state
            .fighters
            .iter()
            .zip(&self.decisions.plans)
            .filter(|(f, _)| f.status == Status::Active);
        for (f, plan) in travels {
            let Activity::Travel(to) = plan.activity else {
                continue;
            };
            let (from, to) = (map.at(anchor(here(f, &self.state))), map.at(anchor(to)));
            // Bent to the side, so two journeys back and forth do not overlap.
            let bend = (to - from).rot90() * 0.2;
            let middle = from + (to - from) / 2.0 + bend;
            let points: Vec<egui::Pos2> = (0..=24_u8)
                .map(|i| {
                    let t = f32::from(i) / 24.0;
                    let u = 1.0 - t;
                    (from.to_vec2() * u * u + middle.to_vec2() * 2.0 * u * t + to.to_vec2() * t * t)
                        .to_pos2()
                })
                .collect();
            let stroke = egui::Stroke::new(1.4_f32, theme::BRONZE);
            painter.extend(egui::Shape::dashed_line(&points, stroke, 6.0, 5.0));
            let label = painter.layout_no_wrap(
                tr!("map-travelling", fighter = stage_name(&f.name)),
                small.clone(),
                egui::Color32::PLACEHOLDER,
            );
            let at = from + (to - from) / 2.0 + bend / 2.0 + egui::vec2(8.0, 0.0);
            haloed(painter, at, &label, theme::BRONZE);
        }
    }

    /// Colosseum `id`'s name, continent, prestige and fee: a dot's tooltip.
    fn details(&self, ui: &mut egui::Ui, id: ColosseumId) {
        ui.strong(colosseum_name(id, &self.data));
        self.details_line(ui, id);
    }

    /// The chosen colosseum's panel in `rect`: its name and details, a button closing
    /// it, and the club's fighters to send there.
    fn panel(&mut self, ui: &mut egui::Ui, rect: egui::Rect, id: ColosseumId) {
        let mut child = ui.new_child(egui::UiBuilder::new().max_rect(rect));
        let mut close = false;
        egui::Frame::new()
            .fill(theme::WOOD)
            .stroke(egui::Stroke::new(1.0_f32, theme::WOOD_LIGHTEST))
            .corner_radius(3.0)
            .inner_margin(egui::Margin::same(14))
            .show(&mut child, |ui| {
                ui.set_width(rect.width() - 28.0);
                ui.horizontal(|ui| {
                    ui.heading(
                        egui::RichText::new(colosseum_name(id, &self.data)).color(theme::BRONZE),
                    );
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        close = ui.button(tr!("map-close")).clicked();
                    });
                });
                self.details_line(ui, id);
                ui.separator();
                egui::ScrollArea::vertical()
                    .id_salt("map-panel")
                    .show(ui, |ui| self.send(ui, id));
            });
        if close {
            self.map_chosen = None;
        }
    }

    /// Colosseum `id`'s continent, prestige and fee, under its name in the panel.
    fn details_line(&self, ui: &mut egui::Ui, id: ColosseumId) {
        let data = &self.data;
        if let Some(rules) = data.world.colosseum(id) {
            ui.label(
                egui::RichText::new(tr!(
                    "map-details",
                    continent = continent_name(rules.continent),
                    prestige = rules.prestige,
                    fee = self.state.entry_fee(id, data).unwrap_or(0)
                ))
                .weak(),
            );
        }
    }

    /// A row per fighter of the club in activity: its name, which opens its sheet, where
    /// it is and its week, and a button sending it to colosseum `id`. Sent, its orders
    /// fight there, every week if they already fought every week, else every other
    /// week, and its week is the fight there, or the journey first (`located`).
    fn send(&mut self, ui: &mut egui::Ui, id: ColosseumId) {
        let data = &self.data;
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
            .num_columns(2)
            .show(ui, |ui| {
                for (i, (f, (plan, orders))) in rows {
                    ui.vertical(|ui| {
                        if ui.link(stage_name(&f.name)).clicked() {
                            goto = Some(View::Sheet(i));
                        }
                        ui.label(
                            egui::RichText::new(tr!(
                                "map-fighter-week",
                                continent = continent_name(here(f, &self.state)),
                                activity = activity_name(plan.activity, data)
                            ))
                            .small()
                            .weak(),
                        );
                    });
                    let sent =
                        matches!(orders.activity, Duty::Fight(c) | Duty::Alternate(c) if c == id);
                    let button = egui::Button::new(tr!("map-send"));
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

/// The meridians and parallels every `GRATICULE` degrees, the equator dashed.
fn graticule(painter: &egui::Painter, map: &Projection) {
    let line = egui::Stroke::new(0.6_f32, theme::WOOD);
    let top = map.unwrapped((CENTRE.0, 90.0)).y;
    let bottom = map.unwrapped((CENTRE.0, -90.0)).y;
    for lon in (-180..180).step_by(usize::try_from(GRATICULE).unwrap_or(15)) {
        let x = map.at(degrees((lon, 0))).x;
        painter.line_segment([egui::pos2(x, top), egui::pos2(x, bottom)], line);
    }
    let (left, right) = (map.rect.left(), map.rect.right());
    for lat in (-75..=75).step_by(usize::try_from(GRATICULE).unwrap_or(15)) {
        let y = map.at(degrees((0, lat))).y;
        let ends = [egui::pos2(left, y), egui::pos2(right, y)];
        if lat == 0 {
            let stroke = egui::Stroke::new(0.6_f32, theme::WOOD_LIGHTER);
            painter.extend(egui::Shape::dashed_line(&ends, stroke, 2.0, 4.0));
        } else {
            painter.line_segment(ends, line);
        }
    }
}

/// The land, from the world's two halves, west and east of Greenwich, each drawn a turn
/// of the world on each side too: the view, centred away from Greenwich, shows a side
/// of the 180th meridian from each.
fn land(ui: &egui::Context, painter: &egui::Painter, map: &Projection) {
    let halves = [
        (-180.0, world_texture(ui, "world_west", WEST)),
        (0.0, world_texture(ui, "world_east", EAST)),
    ];
    let uv = egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0));
    for (west, texture) in &halves {
        for turn in [-360.0, 0.0, 360.0] {
            let corner = map.unwrapped((west + turn, 90.0));
            let rect = egui::Rect::from_min_size(corner, egui::Vec2::splat(180.0 * map.scale));
            if rect.intersects(map.rect) {
                painter.image(texture.id(), rect, uv, egui::Color32::WHITE);
            }
        }
    }
}

const WEST: &[u8] = include_bytes!("../../../../assets/ui/world_west.png");
const EAST: &[u8] = include_bytes!("../../../../assets/ui/world_east.png");

/// The texture of half the world named `name`, embedded like the frames, loaded once.
fn world_texture(ui: &egui::Context, name: &str, bytes: &[u8]) -> egui::TextureHandle {
    let id = egui::Id::new(name);
    if let Some(texture) = ui.data(|d| d.get_temp(id)) {
        return texture;
    }
    let texture = load_smooth_texture(ui, name, bytes).expect("the embedded map is a PNG");
    ui.data_mut(|d| d.insert_temp(id, texture.clone()));
    texture
}

/// Where a dot's name of `size` goes, `gap` from its `centre`: under it, else over,
/// right or left of it, the first side that overlaps none of `taken`; under it if all do.
fn label_place(centre: egui::Pos2, gap: f32, size: egui::Vec2, taken: &[egui::Rect]) -> egui::Pos2 {
    let sides = [
        egui::pos2(centre.x - size.x / 2.0, centre.y + gap),
        egui::pos2(centre.x - size.x / 2.0, centre.y - gap - size.y),
        egui::pos2(centre.x + gap, centre.y - size.y / 2.0),
        egui::pos2(centre.x - gap - size.x, centre.y - size.y / 2.0),
    ];
    sides
        .into_iter()
        .find(|&at| {
            let rect = egui::Rect::from_min_size(at, size);
            !taken.iter().any(|t| t.intersects(rect))
        })
        .unwrap_or(sides[0])
}

/// `text` at `at` in `colour`, outlined in the sea's colour so it reads over land and
/// lines alike.
fn haloed(
    painter: &egui::Painter,
    at: egui::Pos2,
    text: &std::sync::Arc<egui::Galley>,
    colour: egui::Color32,
) {
    for offset in [(-1.0, 0.0), (1.0, 0.0), (0.0, -1.0), (0.0, 1.0)] {
        painter.galley(at + egui::Vec2::from(offset), text.clone(), SEA);
    }
    painter.galley(at, text.clone(), colour);
}

/// The map's name and how it works, over its top left corner.
fn title(ui: &mut egui::Ui, rect: egui::Rect, body: &egui::FontId) {
    let width = (rect.width() - 2.0 * INSET).min(420.0);
    let area = egui::Rect::from_min_size(
        rect.min + egui::vec2(INSET, INSET),
        egui::vec2(width, rect.height() / 3.0),
    );
    let mut child = ui.new_child(egui::UiBuilder::new().max_rect(area));
    egui::Frame::new()
        .fill(SEA.gamma_multiply(0.85))
        .stroke(egui::Stroke::new(1.0_f32, theme::WOOD_LIGHTER))
        .corner_radius(3.0)
        .inner_margin(egui::Margin::symmetric(12, 8))
        .show(&mut child, |ui| {
            ui.set_max_width(width - 24.0);
            ui.heading(tr!("view-map"));
            ui.label(
                egui::RichText::new(tr!("map-intro"))
                    .font(egui::FontId::new(body.size * 0.8, body.family.clone()))
                    .weak(),
            );
        });
}
