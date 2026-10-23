//! Watching fights: the standard duel, or the club's fights of the week, one shown big
//! and the others in clickable thumbnails drawn into render targets. Speed and « Passer
//! au résultat » are shared; each fight goes at its own pace and stops on its own.

use std::path::PathBuf;
use std::rc::Rc;

use lanista_core::design_data::DesignData;
use lanista_core::duel::random_duel;
use macroquad::prelude::*;
use reesch::{Assets, Audio, Bindings, SceneTarget, Screen, Transition, egui, pixel_camera};

use crate::Action;
use crate::club_screen::gauge;
use crate::fight_view::{Cast, FightView, STANDS};
use crate::fighter_sprite::PIXELS_PER_METRE;
use crate::lang::tr;
use crate::options;

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

/// What the fights on screen are.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Mode {
    /// A standard duel; another seed can be picked.
    Free {
        /// Seed of the next fight, edited in the controls.
        next_seed: u64,
    },
    /// The club's fights, watched once.
    Watch,
}

/// A fight to watch: before its first tick, with who is who.
#[derive(Debug, Clone, PartialEq)]
pub struct Watched {
    pub arena: lanista_core::combat::Arena,
    pub cast: Cast,
}

pub struct CombatScreen {
    data: Rc<DesignData>,
    mode: Mode,
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
}

impl CombatScreen {
    /// The standard duel of `seed`, the same as `simulate --seed <seed>`; another
    /// seed can be picked on screen.
    pub fn new(data: Rc<DesignData>, seed: u64, bindings: Bindings<Action>) -> Self {
        let duel = FightView::new(random_duel(&data, seed), Cast::default());
        Self::with(data, vec![duel], Mode::Free { next_seed: seed }, bindings)
    }

    /// Watches `fights` from their first tick, the first one big. Once all are over,
    /// « Continuer » goes back.
    pub fn watch(data: Rc<DesignData>, fights: Vec<Watched>, bindings: Bindings<Action>) -> Self {
        let views = fights
            .into_iter()
            .map(|f| FightView::new(f.arena, f.cast))
            .collect();
        Self::with(data, views, Mode::Watch, bindings)
    }

    fn with(
        data: Rc<DesignData>,
        views: Vec<FightView>,
        mode: Mode,
        bindings: Bindings<Action>,
    ) -> Self {
        Self {
            data,
            mode,
            views,
            focus: 0,
            targets: Vec::new(),
            hovered: None,
            bindings,
            speed: 1.0,
            settings: None,
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
    /// seed of the next fight and « Retour » in the standard duel, or « Continuer »
    /// once every watched fight is over. Returns whether to go back.
    fn controls(&mut self, ui: &egui::Context) -> bool {
        let mut start = None;
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
                        ui.label(self.fight().result());
                    }
                    match &mut self.mode {
                        Mode::Free { next_seed } => {
                            ui.separator();
                            ui.label(tr!("seed", seed = self.views[0].arena.seed.to_string()));
                            ui.add(egui::DragValue::new(next_seed));
                            if ui.button(tr!("start-fight")).clicked() {
                                start = Some(*next_seed);
                            }
                            ui.separator();
                            done = ui.button(tr!("back-escape")).clicked();
                        }
                        Mode::Watch => {
                            let finished = !self.running();
                            done = finished && ui.button(tr!("continue")).clicked();
                        }
                    }
                });
            });
        if let Some(seed) = start {
            let duel = FightView::new(random_duel(&self.data, seed), Cast::default());
            self.views = vec![duel];
            self.focus = 0;
        }
        done
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

    /// Hit points, mana and endurance of the fight shown big, bottom left: both
    /// fighters of a duel, the club's in a melee.
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
                ui.set_width(220.0);
                for f in shown {
                    ui.strong(fight.name(f.id));
                    gauge(ui, &tr!("stat-hp"), f.hp, f.stats.hp);
                    for (name, g) in [(tr!("mana"), &f.mana), (tr!("endurance"), &f.endurance)] {
                        if g.max > 0 {
                            gauge(ui, &name, g.current, g.max);
                        }
                    }
                }
            });
    }
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
        if self.bindings.pressed(ui, &Action::Quit) {
            return Transition::Pop;
        }
        for (action, speed) in [
            (Action::SlowSpeed, SPEEDS[0]),
            (Action::NormalSpeed, SPEEDS[1]),
            (Action::FastSpeed, SPEEDS[2]),
        ] {
            if self.bindings.pressed(ui, &action) {
                self.speed = speed;
            }
        }
        if self.running() && self.bindings.pressed(ui, &Action::Skip) {
            self.skip();
        }
        if self.controls(ui) {
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
        self.elimination(ui);
        self.gauges(ui);
        // ponytail: no screen to tell a failure on; the options then last until the
        // game closes.
        let _ = options::window(ui, audio, self.settings.as_deref());
        audio.play_music(assets, MUSIC);
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
    use lanista_core::duel::play;
    use lanista_core::fighter::FighterId;
    use lanista_core::fighter::equipment::Refused;
    use lanista_core::log::{EventKind, Report, log_text};

    use super::*;
    use crate::fight_view::Pose;

    /// Seed 15 has stuns and a knock-out.
    const SEEDS: [u64; 5] = [42, 0, 1, 7, 15];

    /// A career duel against an opponent, as the club screen names them.
    fn named() -> Cast {
        Cast {
            names: vec!["votre combattant".into(), "l'adversaire".into()],
            club: vec![FighterId(0)],
            survivors: Vec::new(),
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
        assert!(screen.fight().result().starts_with("Vainqueur : "));
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
            "Vainqueur : combattant 1 (K.-O. : combattant 0)"
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
        assert!(
            view.result()
                .contains(&format!("laissé pour mort, survit : {name}")),
            "{}",
            view.result()
        );
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
            "Vainqueur : combattant 0 (membre tranché : combattant 1)"
        );
    }

    fn screen(seed: u64) -> CombatScreen {
        CombatScreen::new(
            Rc::new(DesignData::load().unwrap()),
            seed,
            Bindings::new(Vec::new()),
        )
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
