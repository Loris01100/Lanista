//! Watching the club's fights of the week, one shown big and the others in clickable thumbnails drawn into render targets. Speed and « Passer
//! au résultat » are shared; each fight goes at its own pace and stops on its own.

use std::path::PathBuf;
use std::rc::Rc;

use lanista_core::combat::TICKS_PER_SECOND;
use lanista_core::design_data::{Cost, DesignData};
use lanista_core::fighter::injury::{Location, Side};
use lanista_core::fighter::{Effect, Fighter, FighterId};
use lanista_core::log::{Event, EventKind};
use macroquad::prelude::*;
use reesch::{Assets, Audio, Bindings, SceneTarget, Screen, Transition, egui, pixel_camera};

use crate::Action;
use crate::club_screen::gauge;
use crate::fight_view::{Cast, FightView, STANDS, Tone, Verdict};
use crate::fighter_sprite::PIXELS_PER_METRE;
use crate::lang::tr;
use crate::options;
use crate::texts::{part_injury_text, part_name};
use crate::theme;

pub use crate::fight_view::{DEATH_SOUND, HIT_SOUND};
// ponytail: placeholder music.
pub const MUSIC: &str = "placeholder/music.wav";
/// Fight speeds offered.
const SPEEDS: [f32; 3] = [0.5, 1.0, 1.5];
/// A thumbnail's width, a share of the screen's; 16:10, like its render target.
const THUMBNAIL_SHARE: f32 = 0.24;
/// Pixels of a thumbnail's render target.
const THUMBNAIL_PIXELS: (u32, u32) = (384, 240);
/// Gap around the thumbnails, in screen pixels.
const MARGIN: f32 = 12.0;
const THUMBNAIL_FRAME: Color = Color::new(0.9, 0.85, 0.7, 1.0);
/// Over a thumbnail whose fight is over.
const FINISHED: Color = Color::new(0.55, 0.55, 0.55, 1.0);
/// Width of the pause menu.
const PAUSE_WIDTH: f32 = 220.0;
/// Lines of the fight's feed, and its width.
const FEED_LINES: usize = 6;
const FEED_WIDTH: f32 = 340.0;
/// Size of the words over the fighters, in points.
const POPUP_SIZE: f32 = 22.0;
/// Damage taken by the club's fighters, a lighter blood to read on the sand.
const CLUB_HURT: egui::Color32 = egui::Color32::from_rgb(0xF0, 0x5A, 0x48);

/// A fight to watch: before its first tick, with who is who.
#[derive(Debug, Clone, PartialEq)]
pub struct Watched {
    pub arena: lanista_core::combat::Arena,
    pub cast: Cast,
}

pub struct CombatScreen {
    data: Rc<DesignData>,
    views: Vec<FightView>,
    /// The fight shown big.
    focus: usize,
    /// One per thumbnail, made at the first update: tests never draw.
    targets: Vec<SceneTarget>,
    /// The name under the pointer, shown at the next frame.
    hovered: Option<String>,
    bindings: Bindings<Action>,
    /// Fight speed, a factor of real time: one of `SPEEDS`.
    speed: f32,
    /// Folder where the options chosen are kept; `None` keeps nothing.
    settings: Option<PathBuf>,
    /// Whether the pause menu, opened by Échap, is laid over the screen; the fights
    /// wait meanwhile.
    paused: bool,
    /// Whether the options, opened from the pause menu, are laid over it.
    options_open: bool,
}

impl CombatScreen {
    /// Watches `fights` from their first tick, the first one big. Once all are over,
    /// « Continuer » goes back.
    pub fn watch(data: Rc<DesignData>, fights: Vec<Watched>, bindings: Bindings<Action>) -> Self {
        let views = fights
            .into_iter()
            .map(|f| FightView::new(f.arena, f.cast))
            .collect();
        Self {
            data,
            views,
            focus: 0,
            targets: Vec::new(),
            hovered: None,
            bindings,
            speed: 1.0,
            settings: None,
            paused: false,
            options_open: false,
        }
    }

    /// Keeps the options chosen on this screen in `folder`.
    #[must_use]
    pub fn keeping_settings_in(mut self, folder: Option<PathBuf>) -> Self {
        self.settings = folder;
        self
    }

    /// The fight shown big.
    fn fight(&self) -> &FightView {
        &self.views[self.focus]
    }

    /// The fights on screen, the one shown big among them.
    #[cfg(test)]
    pub(crate) fn views(&self) -> &[FightView] {
        &self.views
    }

    /// Whether any fight is still going.
    pub(crate) fn running(&self) -> bool {
        self.views.iter().any(FightView::running)
    }

    /// Plays the rest of every fight at once, without drawing them.
    fn skip(&mut self) {
        for view in &mut self.views {
            view.skip();
        }
    }

    /// Plays `dt` seconds of fight time in every fight; returns the sounds of the one
    /// shown big. No macroquad call, so tests can drive it.
    pub(crate) fn advance(&mut self, dt: f32) -> Vec<&'static str> {
        let mut heard = Vec::new();
        for (k, view) in self.views.iter_mut().enumerate() {
            let sounds = view.advance(dt);
            if k == self.focus {
                heard = sounds;
            }
        }
        heard
    }

    /// The fights shown as thumbnails, in order: all but the one shown big.
    fn thumbnailed(&self) -> Vec<usize> {
        (0..self.views.len()).filter(|&k| k != self.focus).collect()
    }

    /// The world area the big view shows: its fight, with room on the left for the
    /// thumbnails when there are any.
    /// The camera of the fight shown big: its art's pixels enlarged by a whole factor.
    fn main_camera(&self) -> Camera2D {
        pixel_camera(self.main_view(), PIXELS_PER_METRE)
    }

    fn main_view(&self) -> Rect {
        let world = self.fight().world();
        if self.views.len() < 2 {
            return world;
        }
        let room = THUMBNAIL_SHARE + 0.02;
        let width = world.w / (1.0 - room);
        Rect::new(world.x - width * room, world.y, width, world.h)
    }

    /// Speed buttons, then the result of the fight shown big once it is over; the
    /// « Continuer » once every fight is over. Returns whether to go back.
    fn controls(&mut self, ui: &egui::Context) -> bool {
        let mut done = false;
        egui::Window::new(tr!("controls"))
            .title_bar(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_BOTTOM, [0.0, -12.0])
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    if self.running() {
                        let labels = [tr!("speed-slow"), tr!("speed-normal"), tr!("speed-fast")];
                        for (speed, label) in SPEEDS.into_iter().zip(labels) {
                            ui.selectable_value(&mut self.speed, speed, label);
                        }
                        if ui.button(tr!("skip-to-result")).clicked() {
                            self.skip();
                        }
                    }
                    if !self.fight().running() {
                        result_view(ui, self.fight());
                    }
                    if !self.running() {
                        done = ui.button(tr!("continue")).clicked();
                    }
                });
            });
        done
    }

    /// The pause menu in the middle of the screen while `paused` and the options are
    /// closed: « Reprendre », « Options », « Quitter le combat »; returns whether to
    /// leave the fights.
    fn pause_menu(&mut self, ui: &egui::Context) -> bool {
        if !self.paused || self.options_open {
            return false;
        }
        let mut leave = false;
        let modal = egui::Modal::new(egui::Id::new("pause")).show(ui, |ui| {
            ui.set_width(PAUSE_WIDTH);
            ui.vertical_centered_justified(|ui| {
                ui.heading(tr!("pause"));
                ui.add_space(6.0);
                if ui.button(tr!("resume")).clicked() {
                    self.paused = false;
                }
                if ui.button(tr!("options")).clicked() {
                    self.options_open = true;
                }
                leave = ui.button(tr!("leave-fight")).clicked();
            });
        });
        // Not `should_close`: the Échap that opened the menu would close it at once.
        if modal.backdrop_response.clicked() {
            self.paused = false;
        }
        leave
    }

    /// A click on a thumbnail shows its fight big; the name of the fighter under the
    /// pointer in the big view is shown next to it.
    fn point(&mut self, ui: &egui::Context) {
        self.hovered = None;
        if ui.is_pointer_over_area() {
            return;
        }
        let mouse = Vec2::from(mouse_position());
        let rects = thumbnail_rects(self.views.len() - 1, screen_width());
        if let Some(slot) = rects.iter().position(|r| r.contains(mouse)) {
            if is_mouse_button_pressed(MouseButton::Left) {
                self.focus = self.thumbnailed()[slot];
            }
            return;
        }
        let world = self.main_camera().screen_to_world(mouse);
        self.hovered = self
            .fight()
            .fighter_at(world)
            .map(|id| self.fight().name(id));
    }
}

impl CombatScreen {
    /// The time left before the time limit of the fight shown big, top right.
    fn timer(&self, ui: &egui::Context) {
        let arena = &self.fight().arena;
        egui::Window::new(tr!("timer"))
            .title_bar(false)
            .resizable(false)
            .anchor(egui::Align2::RIGHT_TOP, [-12.0, 12.0])
            .show(ui, |ui| {
                ui.heading(clock(arena.max_ticks.saturating_sub(arena.tick)));
            });
    }

    /// The next elimination of the melee shown big, top middle.
    fn elimination(&self, ui: &egui::Context) {
        let lines = self.fight().elimination_lines();
        if lines.is_empty() {
            return;
        }
        egui::Window::new(tr!("elimination"))
            .title_bar(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_TOP, [0.0, 12.0])
            .show(ui, |ui| {
                for line in &lines {
                    ui.label(line);
                }
            });
    }

    /// The last moments of the fight shown big, under the timer: who hit whom, for how
    /// much and where, the wounds and falls.
    fn feed(&self, ui: &egui::Context) {
        let lines = self.fight().feed(FEED_LINES);
        if lines.is_empty() {
            return;
        }
        egui::Window::new(tr!("feed"))
            .title_bar(false)
            .resizable(false)
            .anchor(egui::Align2::RIGHT_TOP, [-12.0, 72.0])
            .show(ui, |ui| {
                ui.set_width(FEED_WIDTH);
                let weak = ui.visuals().weak_text_color();
                let last = lines.len() - 1;
                for (k, line) in lines.iter().enumerate() {
                    let text = egui::RichText::new(line).small();
                    ui.label(if k == last { text } else { text.color(weak) });
                }
            });
    }

    /// Over the fighters of the fight shown big, the words of their last hits and
    /// wounds, rising and fading: the damage taken, red for the club's fighters, a miss
    /// in grey, a wound in blood, a fall in bronze.
    fn popups(&self, ui: &egui::Context) {
        let fight = self.fight();
        let camera = self.main_camera();
        let points = ui.pixels_per_point();
        let painter = ui.layer_painter(egui::LayerId::new(
            egui::Order::Background,
            egui::Id::new("popups"),
        ));
        let font = egui::FontId::proportional(POPUP_SIZE);
        for (at, popup) in fight.shown_popups() {
            let screen = camera.world_to_screen(at) / points;
            let pos = egui::pos2(screen.x, screen.y);
            let color = match popup.tone {
                Tone::Damage if fight.cast.club.contains(&popup.fighter) => CLUB_HURT,
                Tone::Damage => theme::PARCHMENT,
                Tone::Miss => egui::Color32::from_gray(0xB0),
                Tone::Wound => CLUB_HURT,
                Tone::Out => theme::BRONZE,
            };
            let fade = popup.fade();
            let shadow = egui::Color32::BLACK.gamma_multiply(fade);
            for offset in [egui::vec2(1.5, 1.5), egui::vec2(-1.0, 1.5)] {
                painter.text(
                    pos + offset,
                    egui::Align2::CENTER_BOTTOM,
                    &popup.text,
                    font.clone(),
                    shadow,
                );
            }
            painter.text(
                pos,
                egui::Align2::CENTER_BOTTOM,
                &popup.text,
                font.clone(),
                color.gamma_multiply(fade),
            );
        }
    }

    /// Hit points, mana, endurance and elements of the fight shown big, bottom left, the
    /// spells each fighter has, where it was hit and what still lasts on it: both fighters
    /// of a duel, the club's in a melee.
    fn gauges(&self, ui: &egui::Context) {
        let fight = self.fight();
        let fighters = &fight.arena.fighters;
        let shown: Vec<_> = fighters
            .iter()
            .filter(|f| fighters.len() <= 2 || fight.cast.club.contains(&f.id))
            .collect();
        if shown.is_empty() {
            return;
        }
        egui::Window::new(tr!("gauges"))
            .title_bar(false)
            .resizable(false)
            .anchor(egui::Align2::LEFT_BOTTOM, [12.0, -12.0])
            .show(ui, |ui| {
                ui.set_width(260.0);
                let height = ui.ctx().screen_rect().height() * 0.7;
                egui::ScrollArea::vertical()
                    .max_height(height)
                    .show(ui, |ui| {
                        for f in shown {
                            ui.strong(fight.name_and_elo(f.id));
                            gauge(ui, &tr!("stat-hp"), f.hp, f.stats.hp);
                            for (name, g) in [
                                (tr!("mana"), &f.mana),
                                (tr!("endurance"), &f.endurance),
                                (tr!("elements"), &f.elements),
                            ] {
                                if g.max > 0 {
                                    gauge(ui, &name, g.current, g.max);
                                }
                            }
                            spells(ui, f);
                            anatomy(ui, &fight.arena.log, f);
                            for (line, wound) in states(f) {
                                let color = if wound {
                                    ui.visuals().error_fg_color
                                } else {
                                    ui.visuals().warn_fg_color
                                };
                                ui.label(egui::RichText::new(line).small().color(color));
                            }
                        }
                    });
            });
    }
}

/// The result of `fight`: for the club, « Victoire ! » in green or « Défaite » in red,
/// big, then who won and how the others went out, a sentence each.
fn result_view(ui: &mut egui::Ui, fight: &FightView) {
    ui.vertical(|ui| {
        if let Some(verdict) = fight.verdict() {
            let (text, color) = match verdict {
                Verdict::Victory => (tr!("verdict-victory"), theme::LAUREL),
                Verdict::Defeat => (tr!("verdict-defeat"), theme::BLOOD),
                Verdict::NoWinner => (tr!("verdict-no-winner"), theme::BRONZE),
            };
            ui.label(egui::RichText::new(text).heading().strong().color(color));
        }
        for line in fight.result() {
            ui.label(line);
        }
    });
}

/// `f`'s spells, one line each with what it costs: ready, recharging (seconds left) or
/// too dear for what `f` has left, greyed out; the one it is set on casting in bronze.
/// Its weapon's auto attack, cast only once nothing else is affordable and of use, says so
/// on hover.
fn spells(ui: &mut egui::Ui, f: &Fighter) {
    let chosen = match f.action {
        lanista_core::ai::Action::Cast { spell, .. } => Some(spell),
        _ => None,
    };
    let text = ui.visuals().text_color();
    let weak = ui.visuals().weak_text_color();
    let bronze = ui.visuals().warn_fg_color;
    for (k, spell) in f.spells.iter().enumerate() {
        let cooldown = f.cooldowns.get(k).copied().unwrap_or(0);
        let status = if cooldown > 0 {
            tr!("spell-recharging", seconds = tenths(cooldown))
        } else if f.can_afford(k) {
            tr!("spell-ready")
        } else {
            tr!("spell-too-dear")
        };
        let ready = cooldown == 0 && f.can_afford(k);
        let mut name = egui::RichText::new(&spell.name).small();
        name = if chosen == Some(k) {
            name.strong().color(bronze)
        } else {
            name.color(if ready { text } else { weak })
        };
        ui.horizontal(|ui| {
            let label = ui.label(name);
            if f.auto_attack == Some(k) {
                label.on_hover_text(tr!("spell-auto-attack"));
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let color = if ready { text } else { weak };
                ui.label(egui::RichText::new(status).small().color(color));
                let cost = cost_text(&spell.cost);
                if !cost.is_empty() {
                    ui.label(egui::RichText::new(cost).small().color(weak));
                }
            });
        });
    }
}

/// What a spell costs, its mana, endurance and elements, the free ones left out.
pub(crate) fn cost_text(cost: &Cost) -> String {
    [
        (cost.mana, "cost-mana"),
        (cost.endurance, "cost-endurance"),
        (cost.elements, "cost-elements"),
    ]
    .into_iter()
    .filter(|&(amount, _)| amount > 0)
    .map(|(amount, key)| tr!(key, amount = amount))
    .collect::<Vec<_>>()
    .join(" ")
}

/// `ticks` in seconds with one decimal, rounded up, a comma before it: « 1,4 ».
pub(crate) fn tenths(ticks: u32) -> String {
    let tenths = (u64::from(ticks) * 10).div_ceil(u64::from(TICKS_PER_SECOND));
    format!("{},{}", tenths / 10, tenths % 10)
}

/// `ticks` in minutes and seconds, the seconds rounded up: « 0:59 ».
fn clock(ticks: u32) -> String {
    let seconds = ticks.div_ceil(TICKS_PER_SECOND);
    format!("{}:{:02}", seconds / 60, seconds % 60)
}

/// Where `f` was hit this fight, laid out like a body: the hit points lost on each
/// part, redder the closer to half its max they are; a severed part struck through, an
/// injured one, from before the fight, on bronze (red if lasting), its injuries on hover.
fn anatomy(ui: &mut egui::Ui, log: &[Event], f: &Fighter) {
    use Location::{Arm, Body, Head, Leg};
    let rows = [
        [None, Some(Head), None],
        [Some(Arm(Side::Left)), Some(Body), Some(Arm(Side::Right))],
        [Some(Leg(Side::Left)), None, Some(Leg(Side::Right))],
    ];
    let red = ui.visuals().error_fg_color;
    let text = ui.visuals().text_color();
    let bronze = ui.visuals().warn_fg_color;
    let dark = ui.visuals().extreme_bg_color;
    egui::Grid::new(("anatomy", f.id.0)).show(ui, |ui| {
        for row in rows {
            for part in row {
                let Some(part) = part else {
                    ui.label("");
                    continue;
                };
                let damage = part_damage(log, f.id, part);
                let percent = u64::from(damage) * 200 / u64::from(f.stats.hp.max(1));
                let share = f32::from(u8::try_from(percent.min(100)).unwrap_or(100)) / 100.0;
                let injuries: Vec<_> = f
                    .condition
                    .injuries
                    .iter()
                    .filter(|i| i.location == part && !i.is_lost())
                    .collect();
                let mut name = egui::RichText::new(part_name(part)).small();
                if f.has_lost(part) {
                    name = name.strikethrough().color(red);
                } else if !injuries.is_empty() {
                    let lasting = injuries.iter().any(|i| i.is_permanent());
                    let back = if lasting { red } else { bronze };
                    name = name.background_color(back).color(dark);
                }
                ui.vertical_centered(|ui| {
                    let label = ui.label(name);
                    if !injuries.is_empty() {
                        let texts: Vec<_> =
                            injuries.iter().map(|&&i| part_injury_text(i)).collect();
                        label.on_hover_text(texts.join(
                            "
",
                        ));
                    }
                    ui.label(
                        egui::RichText::new(damage.to_string())
                            .color(text.lerp_to_gamma(red, share)),
                    );
                });
            }
            ui.end_row();
        }
    });
}

/// What lasts on `f` now, a line each with the seconds left, and whether it is a wound:
/// down stunned, each part bleeding, its accuracy lost.
fn states(f: &Fighter) -> Vec<(String, bool)> {
    let mut lines = Vec::new();
    if !f.is_fighting() {
        return lines;
    }
    if f.stunned_ticks > 0 {
        lines.push((
            tr!("state-stunned", seconds = tenths(f.stunned_ticks)),
            false,
        ));
    }
    for lasting in &f.effects {
        let seconds = tenths(lasting.ticks_left);
        lines.push(match lasting.effect {
            Effect::Bleeding { part, .. } => (
                tr!(
                    "state-bleeding",
                    part = part_name(part).to_lowercase(),
                    seconds = seconds
                ),
                true,
            ),
            Effect::Blinded { percent } => (
                tr!("state-blinded", percent = percent, seconds = seconds),
                false,
            ),
        });
    }
    lines
}

/// Hit points `id` lost on `part` in `log`: hits and mine blasts landing on it, and its
/// bleeding.
fn part_damage(log: &[Event], id: FighterId, part: Location) -> u32 {
    log.iter()
        .filter_map(|e| match e.kind {
            EventKind::Cast {
                target,
                damage,
                part: Some(p),
                ..
            }
            | EventKind::Blast {
                target,
                damage,
                part: Some(p),
                ..
            }
            | EventKind::Bled {
                fighter: target,
                damage,
                part: p,
                ..
            } if target == id && p == part => Some(damage),
            _ => None,
        })
        .sum()
}

/// Where the `count` thumbnails go on a screen `width` pixels wide: down the left side,
/// each `THUMBNAIL_SHARE` of the width, 16:10.
#[must_use]
fn thumbnail_rects(count: usize, width: f32) -> Vec<Rect> {
    let w = width * THUMBNAIL_SHARE;
    let h = w * 10.0 / 16.0;
    (0..count)
        .map(|i| {
            #[expect(clippy::cast_precision_loss, reason = "three thumbnails at most")]
            let i = i as f32;
            Rect::new(MARGIN, MARGIN + i * (h + MARGIN), w, h)
        })
        .collect()
}

impl Screen for CombatScreen {
    fn update(&mut self, ui: &egui::Context, assets: &Assets, audio: &mut Audio) -> Transition {
        // Échap opens or closes the pause menu; the options take it while open.
        if !self.options_open && self.bindings.pressed(ui, &Action::Quit) {
            self.paused = !self.paused;
        }
        let keys = !self.paused;
        for (action, speed) in [
            (Action::SlowSpeed, SPEEDS[0]),
            (Action::NormalSpeed, SPEEDS[1]),
            (Action::FastSpeed, SPEEDS[2]),
        ] {
            if keys && self.bindings.pressed(ui, &action) {
                self.speed = speed;
            }
        }
        if keys && self.running() && self.bindings.pressed(ui, &Action::Skip) {
            self.skip();
        }
        if self.controls(ui) || self.pause_menu(ui) {
            return Transition::Pop;
        }
        while self.targets.len() + 1 < self.views.len() {
            let (w, h) = THUMBNAIL_PIXELS;
            self.targets.push(SceneTarget::new(w, h));
        }
        self.point(ui);
        if let Some(name) = &self.hovered
            && let Some(at) = ui.pointer_hover_pos()
        {
            egui::Area::new(egui::Id::new("fighter_name"))
                .fixed_pos(at + egui::vec2(14.0, 14.0))
                .interactable(false)
                .show(ui, |ui| {
                    egui::Frame::popup(ui.style()).show(ui, |ui| ui.label(name));
                });
        }
        self.timer(ui);
        self.feed(ui);
        self.elimination(ui);
        self.gauges(ui);
        self.popups(ui);
        // ponytail: no screen to tell a failure on; the options then last until the
        // game closes.
        let _ = options::window(ui, audio, self.settings.as_deref(), &mut self.options_open);
        audio.play_music(assets, MUSIC);
        if self.paused {
            return Transition::None;
        }
        // Speed scales time: more or fewer ticks per frame, animations alike.
        for sound in self.advance(get_frame_time() * self.speed) {
            audio.play_effect(assets, sound);
        }
        Transition::None
    }

    fn draw(&self, assets: &Assets) {
        // The thumbnails first: drawing into a target changes the camera.
        let shown = self.thumbnailed();
        for (target, &k) in self.targets.iter().zip(&shown) {
            let view = &self.views[k];
            target.draw(view.world(), STANDS, || view.draw(assets, &self.data));
        }
        clear_background(STANDS);
        set_camera(&self.main_camera());
        self.fight().draw(assets, &self.data);
        set_default_camera();
        let rects = thumbnail_rects(shown.len(), screen_width());
        for ((target, &k), rect) in self.targets.iter().zip(&shown).zip(rects) {
            let over = !self.views[k].running();
            target.show(rect, if over { FINISHED } else { WHITE });
            draw_rectangle_lines(rect.x, rect.y, rect.w, rect.h, 2.0, THUMBNAIL_FRAME);
        }
    }
}

#[cfg(test)]
mod tests {
    use lanista_core::combat::Arena;
    use lanista_core::duel::{play, random_duel};
    use lanista_core::fighter::equipment::Refused;
    use lanista_core::log::{Report, log_text};

    use super::*;
    use crate::fight_view::Pose;

    /// Seed 15 has stuns and a knock-out.
    const SEEDS: [u64; 5] = [42, 0, 1, 7, 15];

    /// A career duel against an opponent, as the club screen names them.
    fn named() -> Cast {
        Cast {
            names: vec!["votre combattant".into(), "l'adversaire".into()],
            club: vec![FighterId(0)],
            ..Cast::default()
        }
    }

    /// A career fight, watched frame by frame, is the very fight the career played.
    #[test]
    fn a_watched_career_fight_is_the_one_the_career_played() {
        use lanista_core::colosseum::ColosseumId;
        use lanista_core::fighter::career::{Activity, CareerFighter, WeekPlan};
        use lanista_core::staff::StaffEffects;
        use rand_chacha::ChaCha8Rng;
        use rand_chacha::rand_core::SeedableRng;

        let data = DesignData::load().unwrap();
        let mut rng = ChaCha8Rng::seed_from_u64(7);
        let mut fighter = CareerFighter::random(&mut rng, &data);
        let plan = WeekPlan {
            activity: Activity::Fight(ColosseumId(0)),
            care: false,
            refused: Refused::NONE,
            loadout: 0,
        };
        let fight = fighter
            .play_week(plan, None, StaffEffects::default(), &mut rng, &data)
            .unwrap()
            .fight
            .unwrap();
        let mut expected = fight.start(&data);
        let watched = Watched {
            arena: expected.clone(),
            cast: named(),
        };
        let mut screen =
            CombatScreen::watch(Rc::new(data), vec![watched], Bindings::new(Vec::new()));
        // Past the one-minute limit.
        for _ in 0..60 * 70 {
            screen.advance(1.0 / 60.0);
        }
        play(&mut expected);
        assert_eq!(log_text(&screen.fight().arena), log_text(&expected));
        assert_eq!(screen.fight().arena.tick, fight.ticks);
        assert!(screen.fight().result()[0].ends_with("remporte le combat."));
        assert!(!screen.running());
    }

    /// Three fights at once go each at its own pace, all as simulated, and stop on
    /// their own; a click focus aside, they all play on.
    #[test]
    fn three_fights_watched_together_are_the_simulated_ones() {
        let data = Rc::new(DesignData::load().unwrap());
        let fights: Vec<Watched> = [42, 0, 15]
            .map(|seed| Watched {
                arena: random_duel(&data, seed),
                cast: Cast::default(),
            })
            .into();
        let mut screen = CombatScreen::watch(Rc::clone(&data), fights, Bindings::new(Vec::new()));
        assert_eq!(screen.thumbnailed(), [1, 2]);
        screen.focus = 2;
        assert_eq!(screen.thumbnailed(), [0, 1]);
        while screen.running() {
            screen.advance(1.0 / 60.0);
        }
        for (view, seed) in screen.views.iter().zip([42, 0, 15]) {
            assert_eq!(
                log_text(&view.arena),
                log_text(&simulated(seed)),
                "seed {seed}"
            );
        }
        // The big view leaves room for the thumbnails on its left.
        let main = screen.main_view();
        let world = screen.fight().world();
        assert!(main.x < world.x && (main.right() - world.right()).abs() < 1e-4);
    }

    #[test]
    fn spell_costs_and_cooldowns_read_in_french() {
        assert_eq!(tenths(45), "1,5");
        assert_eq!(tenths(1), "0,1");
        assert_eq!(tenths(30), "1,0");
        let cost = Cost {
            mana: 12,
            endurance: 0,
            elements: 1,
        };
        assert_eq!(cost_text(&cost), "12 mana 1 élém.");
        assert_eq!(cost_text(&Cost::default()), "");
    }

    #[test]
    fn the_timer_counts_down_minutes_and_seconds() {
        assert_eq!(clock(lanista_core::duel::MAX_TICKS), "1:00");
        assert_eq!(clock(TICKS_PER_SECOND * 59 + 1), "1:00");
        assert_eq!(clock(TICKS_PER_SECOND * 5), "0:05");
        assert_eq!(clock(1), "0:01");
        assert_eq!(clock(0), "0:00");
    }

    #[test]
    fn thumbnails_go_down_the_left_side() {
        let rects = thumbnail_rects(2, 1280.0);
        assert_eq!(rects.len(), 2);
        assert!((rects[0].w - 1280.0 * THUMBNAIL_SHARE).abs() < 1e-3);
        assert!((rects[0].h * 16.0 / 10.0 - rects[0].w).abs() < 1e-3);
        assert!(rects[1].y > rects[0].bottom());
        assert!(rects.iter().all(|r| (r.x - MARGIN).abs() < 1e-6));
    }

    /// The loser of seed 13 dies: its stain starts at the death and keeps
    /// spreading after the fight, the winner never bleeds.
    #[test]
    fn only_the_dead_bleed_from_their_death_on() {
        let mut screen = screen(13);
        // The fight lasts 133 ticks, under 5 s.
        for _ in 0..60 * 5 {
            if screen
                .fight()
                .looks
                .iter()
                .any(|look| look.dead_for.is_some())
            {
                break;
            }
            screen.advance(1.0 / 60.0);
        }
        let dead = screen
            .fight()
            .arena
            .fighters
            .iter()
            .position(|f| !f.is_alive())
            .unwrap();
        assert_eq!(screen.fight().looks[dead].dead_for, Some(0.0));
        for _ in 0..60 {
            screen.advance(1.0 / 60.0);
        }
        assert!((screen.fight().looks[dead].dead_for.unwrap() - 1.0).abs() < 1e-3);
        assert!(screen.fight().looks[1 - dead].dead_for.is_none());
    }

    /// In the standard duel of seed 7, fighter 0 is knocked out: it lies down, alive,
    /// without blood.
    #[test]
    fn a_fighter_knocked_out_lies_down_without_bleeding() {
        let mut screen = screen(7);
        while screen.running() {
            screen.advance(1.0 / 60.0);
        }
        assert_eq!(screen.fight().looks[0].pose, Pose::Down);
        assert!(
            screen
                .fight()
                .looks
                .iter()
                .all(|look| look.dead_for.is_none())
        );
        assert_eq!(
            screen.fight().result(),
            [
                "combattant 1 remporte le combat.",
                "combattant 0 est mis K.-O."
            ]
        );
    }

    /// The loser of seed 42 falls at 0 HP: watched as a survivor of the fatal blow, it
    /// lies down alive, without a cry, a stain or a darker body.
    #[test]
    fn a_survivor_of_the_fatal_blow_falls_and_lies_down_alive() {
        let mut fallen = simulated(42);
        let dead = fallen.fighters.iter().position(|f| !f.is_alive()).unwrap();
        let id = fallen.fighters[dead].id;
        fallen = random_duel(&DesignData::load().unwrap(), 42);
        let watched = Watched {
            arena: fallen,
            cast: Cast {
                survivors: vec![id],
                ..named()
            },
        };
        let mut screen = CombatScreen::watch(
            Rc::new(DesignData::load().unwrap()),
            vec![watched],
            Bindings::new(Vec::new()),
        );
        let mut sounds = Vec::new();
        while screen.running() {
            sounds.extend(screen.advance(1.0 / 60.0));
        }
        for _ in 0..60 {
            sounds.extend(screen.advance(1.0 / 60.0));
        }
        let view = screen.fight();
        assert!(!view.arena.fighters[dead].is_alive());
        assert_eq!(view.looks[dead].pose, Pose::Down);
        assert!(view.looks.iter().all(|look| look.dead_for.is_none()));
        assert!(!sounds.contains(&DEATH_SOUND));
        let name = view.name(id);
        let fell = format!("{name} tombe, laissé pour mort, mais survit.");
        assert!(view.result().contains(&fell), "{:?}", view.result());
    }

    /// In the standard duel of seed 65, fighter 1 loses an arm: it lies down, alive.
    #[test]
    fn a_fighter_that_lost_a_limb_lies_down_alive() {
        let mut screen = screen(65);
        while screen.running() {
            screen.advance(1.0 / 60.0);
        }
        assert_eq!(screen.fight().looks[1].pose, Pose::Down);
        assert!(
            screen
                .fight()
                .looks
                .iter()
                .all(|look| look.dead_for.is_none())
        );
        assert_eq!(
            screen.fight().result(),
            [
                "combattant 0 remporte le combat.",
                "combattant 1 a un membre tranché."
            ]
        );
    }

    /// In the standard duel of seed 7, fighter 1 knocks fighter 0 out: a defeat for the
    /// club of fighter 0, its opponent told as such; a victory for the club of fighter 1.
    #[test]
    fn the_result_tells_the_club_whether_it_won() {
        let data = Rc::new(DesignData::load().unwrap());
        let watch = |club: u32| {
            let duel = Watched {
                arena: random_duel(&data, 7),
                cast: Cast {
                    names: vec!["Datis".into(), "Augure".into()],
                    club: vec![FighterId(club)],
                    ..Cast::default()
                },
            };
            let mut screen =
                CombatScreen::watch(Rc::clone(&data), vec![duel], Bindings::new(Vec::new()));
            while screen.running() {
                screen.advance(1.0 / 60.0);
            }
            (screen.fight().verdict(), screen.fight().result())
        };
        assert_eq!(
            watch(0),
            (
                Some(Verdict::Defeat),
                vec![
                    "Augure, l'adversaire, remporte le combat.".to_owned(),
                    "Datis est mis K.-O.".to_owned()
                ]
            )
        );
        assert_eq!(watch(1).0, Some(Verdict::Victory));
        assert_eq!(watch(1).1[0], "Augure remporte le combat.");
    }

    /// The standard duel of `seed`, watched.
    fn screen(seed: u64) -> CombatScreen {
        let data = Rc::new(DesignData::load().unwrap());
        let duel = Watched {
            arena: random_duel(&data, seed),
            cast: Cast::default(),
        };
        CombatScreen::watch(data, vec![duel], Bindings::new(Vec::new()))
    }

    /// What `simulate --seed <seed>` plays and prints.
    fn simulated(seed: u64) -> Arena {
        let mut arena = random_duel(&DesignData::load().unwrap(), seed);
        play(&mut arena);
        arena
    }

    fn assert_same_fight(screen: &CombatScreen, seed: u64) {
        let expected = simulated(seed);
        let arena = &screen.fight().arena;
        assert_eq!(log_text(arena), log_text(&expected), "seed {seed}");
        assert_eq!(Report::new(arena), Report::new(&expected), "seed {seed}");
    }

    /// Phase 2 milestone: frame rate and speed change how the fight is shown,
    /// never how it goes.
    #[test]
    fn watched_fight_is_the_simulated_one() {
        let frame_times = [1.0 / 60.0, 1.0 / 144.0, 0.05, 0.3];
        for seed in SEEDS {
            let mut screen = screen(seed);
            let mut frame = 0;
            while screen.running() {
                let speed = SPEEDS[frame % SPEEDS.len()];
                screen.advance(frame_times[frame % frame_times.len()] * speed);
                frame += 1;
            }
            assert_same_fight(&screen, seed);
        }
    }

    /// One cry per death, one hit per cast that hit whose impact frame is shown:
    /// all the winner's hits, maybe not the loser's last one.
    #[test]
    fn sounds_follow_hits_and_deaths() {
        let mut screen = screen(42);
        let mut sounds = Vec::new();
        // 20 s, well past the end (133 ticks), so the last animations finish.
        for _ in 0..60 * 20 {
            sounds.extend(screen.advance(1.0 / 60.0));
        }
        let count = |sound| sounds.iter().filter(|&&s| s == sound).count();
        let arena = &screen.fight().arena;
        let casts = |winner_only: bool| {
            arena
                .log
                .iter()
                .filter(|e| match e.kind {
                    EventKind::Cast { caster, missed, .. } => {
                        !missed
                            && (!winner_only
                                || arena.fighters[usize::try_from(caster.0).unwrap()].is_fighting())
                    }
                    _ => false,
                })
                .count()
        };
        assert_eq!(count(DEATH_SOUND), 1);
        assert!((casts(true)..=casts(false)).contains(&count(HIT_SOUND)));
    }

    /// Every hit point lost lands on one of the six parts the anatomy shows, bleeding
    /// included.
    #[test]
    fn the_anatomy_counts_every_hit_point_lost() {
        use Location::{Arm, Body, Head, Leg};
        let parts = [
            Head,
            Body,
            Arm(Side::Left),
            Arm(Side::Right),
            Leg(Side::Left),
            Leg(Side::Right),
        ];
        let mut bled = false;
        for seed in 0..20 {
            let mut screen = screen(seed);
            screen.skip();
            let arena = &screen.fight().arena;
            bled |= arena
                .log
                .iter()
                .any(|e| matches!(e.kind, EventKind::Bled { damage, .. } if damage > 0));
            for f in &arena.fighters {
                let total: u32 = parts
                    .iter()
                    .map(|&p| part_damage(&arena.log, f.id, p))
                    .sum();
                assert_eq!(total, f.stats.hp - f.hp, "seed {seed}");
            }
        }
        assert!(bled, "no fight bled");
    }

    /// In the standard duel of seed 7, fighter 1's Guillotine takes 48 HP off fighter
    /// 0's head at tick 78 and stuns it: its damage and its stun rise over fighter 0
    /// once the blow lands, then fade away; the feed tells the blow, the stun and the
    /// knock-out.
    #[test]
    fn a_hit_shows_its_damage_over_its_target_and_in_the_feed() {
        use crate::fight_view::POPUP_SECONDS;

        let mut screen = screen(7);
        while screen.fight().arena.log.is_empty() {
            screen.advance(1.0 / 60.0);
        }
        let words = |screen: &CombatScreen| -> Vec<(FighterId, String, Tone)> {
            screen
                .fight()
                .shown_popups()
                .into_iter()
                .map(|(_, p)| (p.fighter, p.text.clone(), p.tone))
                .collect()
        };
        // Still before the impact.
        assert_eq!(screen.fight().popups.len(), 2);
        assert_eq!(words(&screen), Vec::new());
        for _ in 0..10 {
            screen.advance(1.0 / 60.0);
        }
        assert_eq!(
            words(&screen),
            [
                (FighterId(0), "-48".to_owned(), Tone::Damage),
                (FighterId(0), "étourdi".to_owned(), Tone::Out)
            ]
        );
        let shown = screen.fight().shown_popups();
        assert!(shown[1].0.y < shown[0].0.y, "stacked over the first");
        let lines = screen.fight().feed(6);
        assert_eq!(
            lines,
            [
                "combattant 1 : Guillotine sur combattant 0, -48 PV (tête)",
                "combattant 0 est étourdi."
            ]
        );
        for _ in 0..60 {
            screen.advance(POPUP_SECONDS / 60.0);
        }
        assert_eq!(screen.fight().popups, Vec::new());
        screen.skip();
        assert_eq!(screen.fight().feed(6)[2], "combattant 0 est mis K.-O.");
        assert_eq!(screen.fight().popups, Vec::new());
    }

    /// A stunned fighter tells how long it stays down, a bleeding part how long it bleeds.
    #[test]
    fn the_gauges_tell_what_lasts_on_a_fighter() {
        use lanista_core::fighter::Lasting;

        let mut f = random_duel(&DesignData::load().unwrap(), 7).fighters[0].clone();
        assert_eq!(states(&f), Vec::new());
        f.stunned_ticks = 45;
        f.effects.push(Lasting {
            effect: Effect::Bleeding {
                part: Location::Leg(Side::Left),
                source: FighterId(1),
            },
            ticks_left: 30,
        });
        assert_eq!(
            states(&f),
            [
                ("Étourdi, 1,5 s".to_owned(), false),
                ("Saigne : jambe gauche, 1,0 s".to_owned(), true)
            ]
        );
    }

    #[test]
    fn skipping_gives_the_same_result() {
        for seed in SEEDS {
            let mut screen = screen(seed);
            for _ in 0..30 {
                screen.advance(1.0 / 60.0);
            }
            screen.skip();
            assert_same_fight(&screen, seed);
        }
    }
}
