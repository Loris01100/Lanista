//! Watching a fight, a standard duel or a career fight: the logic runs at its
//! fixed tick rate, fighters are drawn as animated sprites at positions
//! interpolated between the last two ticks.

use std::rc::Rc;

use lanista_core::ai::{self, CombatAi};
use lanista_core::combat::{self, Arena};
use lanista_core::design_data::DesignData;
use lanista_core::duel::{MAX_TICKS, random_duel, utility_ais};
use lanista_core::fighter::FighterId;
use lanista_core::fighter::injury::Location;
use lanista_core::log::EventKind;
use macroquad::prelude::*;
use reesch::{
    Animation, AnimationEvent, AnimationPlayer, Assets, Audio, Bindings, FixedStep, Screen,
    SpriteSheet, Transition, egui, fit_camera,
};

use crate::Action;

pub const FIGHTER_SHEET: &str = "placeholder/fighter.png";
// ponytail: placeholder sounds; per-spell and per-class sounds in `data/` later.
pub const HIT_SOUND: &str = "placeholder/hit.wav";
pub const DEATH_SOUND: &str = "placeholder/death.wav";
pub const MUSIC: &str = "placeholder/music.wav";
// ponytail: one placeholder sheet for every class; per-class sheets
// and animations in `data/` (RON) with the real sprites.
const SHEET: SpriteSheet = SpriteSheet {
    columns: 4,
    frame_width: 16.0,
    frame_height: 16.0,
};
/// Drawn size of a fighter, in meters (its body is 1 m wide).
const SPRITE_SIZE: f32 = 1.6;
/// Tint per fighter, in `FighterId` order, to tell them apart in a mirror match.
const FIGHTER_TINTS: [Color; 2] = [
    Color::new(1.0, 0.75, 0.55, 1.0),
    Color::new(0.6, 0.75, 1.0, 1.0),
];
/// Fight speeds offered, with their labels.
const SPEEDS: [(f32, &str); 3] = [(0.5, "x0,5"), (1.0, "x1"), (1.5, "x1,5")];
const SAND: Color = Color::new(0.76, 0.66, 0.48, 1.0);
const STANDS: Color = Color::new(0.16, 0.13, 0.11, 1.0);
// ponytail: blood drawn with shapes until the real sprites bring a stain.
// Opaque, so overlapping blots make one even stain.
const BLOOD: Color = Color::new(0.45, 0.03, 0.03, 1.0);
/// Seconds a blood stain takes to spread, at fight speed.
const BLOOD_SPREAD_SECONDS: f32 = 1.2;
/// Radius of a fully spread stain, in meters.
const BLOOD_RADIUS: f32 = 0.7;
/// The stain sits this far below the fighter's center, under the body lying down, in meters.
const BLOOD_DROP: f32 = 0.25;
/// Smaller blots around the stain: offset from its center and radius, as shares of
/// `BLOOD_RADIUS`. Mirrored for every other fighter so two stains differ.
const BLOOD_BLOTS: [(f32, f32, f32); 4] = [
    (0.0, 0.0, 1.0),
    (0.7, 0.35, 0.55),
    (-0.55, 0.5, 0.45),
    (0.2, -0.7, 0.4),
];
/// Brightness of a dead fighter's sprite once it lies down, a share of its tint.
const DEAD_SHADE: f32 = 0.6;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Pose {
    Idle,
    Move,
    Cast,
    Death,
    /// Stunned or knocked out: lying down, alive.
    Down,
}

/// What a fighter is showing: its pose and how far into its animation.
struct Look {
    pose: Pose,
    player: AnimationPlayer,
    /// Seconds since the death was shown, for the blood stain; `None` while alive.
    dead_for: Option<f32>,
    /// The cast being shown missed: no hit sound at its impact.
    missed: bool,
}

impl Look {
    fn show(&mut self, pose: Pose) {
        if self.pose != pose {
            self.pose = pose;
            self.player = AnimationPlayer::default();
        }
    }
}

/// What the fight on screen is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Mode {
    /// A standard duel; another seed can be picked.
    Free {
        /// Seed of the next fight, edited in the controls.
        next_seed: u64,
    },
    /// A career fight, watched once, fighter 0 being the player's.
    Watch,
}

pub struct CombatScreen {
    data: Rc<DesignData>,
    mode: Mode,
    arena: Arena,
    ais: Vec<Box<dyn CombatAi>>,
    step: FixedStep,
    /// Fighter positions before the last tick, to interpolate from.
    previous: Vec<combat::Vec2>,
    /// Log events already turned into animations.
    seen_events: usize,
    looks: Vec<Look>,
    animations: [Animation; 5],
    bindings: Bindings<Action>,
    /// Fight speed, a factor of real time: one of `SPEEDS`.
    speed: f32,
}

impl CombatScreen {
    /// The standard duel of `seed`, the same as `simulate --seed <seed>`; another
    /// seed can be picked on screen.
    pub fn new(data: Rc<DesignData>, seed: u64, bindings: Bindings<Action>) -> Self {
        let arena = random_duel(&data, seed);
        Self::with(data, arena, Mode::Free { next_seed: seed }, bindings)
    }

    /// Watches the fight `arena`, from its first tick: a career fight, fighter 0
    /// being the player's. Once it is over, « Continuer » goes back.
    pub fn watch(data: Rc<DesignData>, arena: Arena, bindings: Bindings<Action>) -> Self {
        Self::with(data, arena, Mode::Watch, bindings)
    }

    fn with(data: Rc<DesignData>, arena: Arena, mode: Mode, bindings: Bindings<Action>) -> Self {
        let animation = |row: u16, frame_time, looping| Animation {
            first: row * SHEET.columns,
            frames: 4,
            frame_time,
            looping,
            markers: Vec::new(),
        };
        let mut screen = Self {
            data,
            mode,
            ais: Vec::new(),
            previous: Vec::new(),
            arena: arena.clone(),
            step: FixedStep::new(combat::TICKS_PER_SECOND),
            seen_events: 0,
            looks: Vec::new(),
            // Same order as `Pose`.
            animations: [
                animation(0, 0.25, true),
                animation(1, 0.1, true),
                Animation {
                    markers: vec![(2, "impact".into())],
                    ..animation(2, 0.06, false)
                },
                animation(3, 0.15, false),
                // ponytail: the death row until the real sprites bring a stunned pose.
                animation(3, 0.1, false),
            ],
            bindings,
            speed: 1.0,
        };
        screen.show(arena);
        screen
    }

    /// Shows `arena` from where it is; the speed is kept.
    fn show(&mut self, arena: Arena) {
        self.arena = arena;
        self.ais = utility_ais(&self.arena);
        self.previous = positions(&self.arena);
        self.step = FixedStep::new(combat::TICKS_PER_SECOND);
        self.seen_events = 0;
        self.looks = self
            .arena
            .fighters
            .iter()
            .map(|_| Look {
                pose: Pose::Idle,
                player: AnimationPlayer::default(),
                dead_for: None,
                missed: false,
            })
            .collect();
    }

    /// Plays the rest of the fight at once, without drawing it.
    fn skip(&mut self) {
        combat::run(&mut self.arena, &mut self.ais, MAX_TICKS);
        self.previous = positions(&self.arena);
        // No cast animation for hits nobody saw.
        self.seen_events = self.arena.log.len();
    }

    /// Speed buttons, then the result once the fight is over; the seed of the
    /// next fight and « Retour » in the standard duel, or « Continuer » for a watched
    /// fight. Returns whether to go back.
    fn controls(&mut self, ui: &egui::Context) -> bool {
        let mut start = None;
        let mut done = false;
        egui::Window::new("Contrôles")
            .title_bar(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_BOTTOM, [0.0, -12.0])
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    if self.running() {
                        for (speed, label) in SPEEDS {
                            ui.selectable_value(&mut self.speed, speed, label);
                        }
                        if ui.button("Passer au résultat").clicked() {
                            self.skip();
                        }
                    } else {
                        ui.label(self.result());
                    }
                    match &mut self.mode {
                        Mode::Free { next_seed } => {
                            ui.separator();
                            ui.label(format!("Graine {}", self.arena.seed));
                            ui.add(egui::DragValue::new(next_seed));
                            if ui.button("Lancer").clicked() {
                                start = Some(*next_seed);
                            }
                            ui.separator();
                            done = ui.button("Retour (Échap)").clicked();
                        }
                        Mode::Watch => {
                            let finished = !self.running();
                            done = finished && ui.button("Continuer").clicked();
                        }
                    }
                });
            });
        if let Some(seed) = start {
            self.show(random_duel(&self.data, seed));
        }
        done
    }

    /// How the result names fighter `id`.
    fn name(&self, id: FighterId) -> String {
        match (self.mode, id.0) {
            (Mode::Watch, 0) => "votre combattant".into(),
            (Mode::Watch, _) => "l'adversaire".into(),
            (Mode::Free { .. }, n) => format!("combattant {n}"),
        }
    }

    fn result(&self) -> String {
        if !self.arena.is_over() {
            return "Temps écoulé, pas de vainqueur".into();
        }
        let winner = match self.arena.fighters.iter().find(|f| f.is_fighting()) {
            Some(winner) => format!("Vainqueur : {}", self.name(winner.id)),
            None => "Aucun vainqueur".into(),
        };
        let out = |how: &str, out: fn(&lanista_core::fighter::Fighter) -> bool| {
            let names: Vec<String> = self
                .arena
                .fighters
                .iter()
                .filter(|f| out(f))
                .map(|f| self.name(f.id))
                .collect();
            (!names.is_empty()).then(|| format!("{how} : {}", names.join(", ")))
        };
        let reasons: Vec<String> = [
            out("abandon", |f| f.surrendered),
            out("K.-O.", |f| f.knocked_out),
            out("décapité", |f| f.severed == Some(Location::Head)),
            out("membre tranché", |f| {
                f.severed.is_some_and(|part| part != Location::Head)
            }),
        ]
        .into_iter()
        .flatten()
        .collect();
        if reasons.is_empty() {
            winner
        } else {
            format!("{winner} ({})", reasons.join(", "))
        }
    }

    fn running(&self) -> bool {
        !self.arena.is_over() && self.arena.tick < MAX_TICKS
    }

    /// Plays the ticks due after `dt` seconds of fight time, then animates;
    /// returns the sounds to play. No macroquad call, so tests can drive it.
    fn advance(&mut self, dt: f32) -> Vec<&'static str> {
        for _ in 0..self.step.advance(f64::from(dt)) {
            if !self.running() {
                break;
            }
            self.previous = positions(&self.arena);
            combat::tick(&mut self.arena, &mut self.ais);
        }
        if !self.running() {
            // Nothing moves any more: stop interpolating.
            self.previous = positions(&self.arena);
        }
        self.animate(dt)
    }

    /// Returns the sounds to play: a hit at each impact frame of a cast that hit,
    /// a cry at each death.
    fn animate(&mut self, dt: f32) -> Vec<&'static str> {
        let mut sounds = Vec::new();
        // A cast resolves within one tick: the log, not `action`, shows it.
        for event in &self.arena.log[self.seen_events..] {
            if let EventKind::Cast { caster, missed, .. } = event.kind
                && let Some(look) = index(caster.0).and_then(|i| self.looks.get_mut(i))
            {
                look.pose = Pose::Cast;
                look.player = AnimationPlayer::default();
                look.missed = missed;
            }
        }
        self.seen_events = self.arena.log.len();

        for (fighter, look) in self.arena.fighters.iter().zip(&mut self.looks) {
            let casting = look.pose == Pose::Cast && !look.player.is_finished(&self.animations[2]);
            let pose = if !fighter.is_alive() {
                Pose::Death
            } else if !fighter.is_standing() && !fighter.surrendered {
                Pose::Down
            } else if casting {
                Pose::Cast
            } else if matches!(fighter.action, ai::Action::Move(_)) {
                Pose::Move
            } else {
                Pose::Idle
            };
            if pose == Pose::Death && look.pose != Pose::Death {
                sounds.push(DEATH_SOUND);
            }
            if pose == Pose::Death {
                look.dead_for = Some(look.dead_for.map_or(0.0, |t| t + dt));
            }
            look.show(pose);
            let events = look.player.advance(&self.animations[pose as usize], dt);
            if events.contains(&AnimationEvent::Marker("impact")) && !look.missed {
                sounds.push(HIT_SOUND);
            }
        }
        sounds
    }
}

impl Screen for CombatScreen {
    fn update(&mut self, ui: &egui::Context, assets: &Assets, audio: &mut Audio) -> Transition {
        if self.bindings.pressed(ui, &Action::Quit) {
            return Transition::Pop;
        }
        for (action, speed) in [
            (Action::SlowSpeed, SPEEDS[0].0),
            (Action::NormalSpeed, SPEEDS[1].0),
            (Action::FastSpeed, SPEEDS[2].0),
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
        volumes(ui, audio);
        audio.play_music(assets, MUSIC);
        // Speed scales time: more or fewer ticks per frame, animations alike.
        for sound in self.advance(get_frame_time() * self.speed) {
            audio.play_effect(assets, sound);
        }
        Transition::None
    }

    fn draw(&self, assets: &Assets) {
        clear_background(STANDS);
        let (width, height) = (to_f32(self.arena.width), to_f32(self.arena.height));
        set_camera(&fit_camera(Rect::new(
            -1.0,
            -1.0,
            width + 2.0,
            height + 2.0,
        )));
        draw_rectangle(0.0, 0.0, width, height, SAND);
        // Blood under everyone.
        for (fighter, look) in self.arena.fighters.iter().zip(&self.looks) {
            if let Some(dead_for) = look.dead_for {
                let x = to_f32(fighter.position.x);
                let y = to_f32(fighter.position.y) + BLOOD_DROP;
                draw_blood(x, y, blood_spread(dead_for), fighter.id.0 % 2 == 1);
            }
        }

        let texture = assets.texture(FIGHTER_SHEET);
        let alpha = self.step.alpha();
        // The dead first, so the living walk over them.
        let mut order: Vec<usize> = (0..self.arena.fighters.len()).collect();
        order.sort_by_key(|&i| self.arena.fighters[i].is_alive());
        for i in order {
            let fighter = &self.arena.fighters[i];
            let look = &self.looks[i];
            let (from, to) = (self.previous[i], fighter.position);
            let x = to_f32(from.x + (to.x - from.x) * alpha);
            let y = to_f32(from.y + (to.y - from.y) * alpha);
            // Sprites face right: turn toward the other fighters' side.
            let others = self.arena.fighters.iter().filter(|f| f.id != fighter.id);
            let flip_x = others.map(|f| f.position.x - to.x).sum::<f64>() < 0.0;
            let mut tint = index(fighter.id.0)
                .and_then(|i| FIGHTER_TINTS.get(i))
                .copied()
                .unwrap_or(WHITE);
            let animation = &self.animations[look.pose as usize];
            if look.pose == Pose::Death && look.player.is_finished(animation) {
                tint = Color::new(
                    tint.r * DEAD_SHADE,
                    tint.g * DEAD_SHADE,
                    tint.b * DEAD_SHADE,
                    tint.a,
                );
            }
            let frame = look.player.frame(animation);
            draw_texture_ex(
                texture,
                x - SPRITE_SIZE / 2.0,
                y - SPRITE_SIZE / 2.0,
                tint,
                DrawTextureParams {
                    dest_size: Some(vec2(SPRITE_SIZE, SPRITE_SIZE)),
                    source: Some(SHEET.frame_rect(frame)),
                    flip_x,
                    ..Default::default()
                },
            );
        }
        set_default_camera();
    }
}

/// Music and effects volume sliders, top right.
fn volumes(ui: &egui::Context, audio: &mut Audio) {
    egui::Window::new("Son")
        .resizable(false)
        .default_open(false)
        .anchor(egui::Align2::RIGHT_TOP, [-12.0, 12.0])
        .show(ui, |ui| {
            let mut music = audio.music_volume();
            if ui
                .add(egui::Slider::new(&mut music, 0.0..=1.0).text("Musique"))
                .changed()
            {
                audio.set_music_volume(music);
            }
            let mut effects = audio.effects_volume();
            if ui
                .add(egui::Slider::new(&mut effects, 0.0..=1.0).text("Effets"))
                .changed()
            {
                audio.set_effects_volume(effects);
            }
        });
}

/// How far a blood stain has spread `dead_for` seconds after the death, from 0 to 1:
/// fast at first, then slowing down (ease-out).
fn blood_spread(dead_for: f32) -> f32 {
    let t = (dead_for / BLOOD_SPREAD_SECONDS).clamp(0.0, 1.0);
    1.0 - (1.0 - t) * (1.0 - t)
}

/// A blood stain centered on (`x`, `y`), `spread` of its full size, mirrored if `mirror`.
fn draw_blood(x: f32, y: f32, spread: f32, mirror: bool) {
    let side = if mirror { -1.0 } else { 1.0 };
    let radius = BLOOD_RADIUS * spread;
    for (dx, dy, size) in BLOOD_BLOTS {
        draw_circle(
            x + side * dx * radius,
            y + dy * radius,
            size * radius,
            BLOOD,
        );
    }
}

fn positions(arena: &Arena) -> Vec<combat::Vec2> {
    arena.fighters.iter().map(|f| f.position).collect()
}

fn index(id: u32) -> Option<usize> {
    usize::try_from(id).ok()
}

#[expect(
    clippy::cast_possible_truncation,
    reason = "screen coordinates, in meters: f32 is plenty"
)]
fn to_f32(meters: f64) -> f32 {
    meters as f32
}

#[cfg(test)]
mod tests {
    use lanista_core::duel::play;
    use lanista_core::log::{Report, log_text};

    use super::*;

    /// Seed 15 has stuns and a knock-out.
    const SEEDS: [u64; 5] = [42, 0, 1, 7, 15];

    /// The first week of career 7 is a fight its fighter gives up: watched frame
    /// by frame, it is the very fight the career played.
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
        };
        let fight = fighter
            .play_week(plan, None, StaffEffects::default(), &mut rng, &data)
            .unwrap()
            .fight
            .unwrap();
        let mut screen = CombatScreen::watch(
            Rc::new(data),
            fight.start.clone(),
            Bindings::new(Vec::new()),
        );
        // Past the one-minute limit.
        for _ in 0..60 * 70 {
            screen.advance(1.0 / 60.0);
        }
        let mut expected = fight.start;
        play(&mut expected);
        assert_eq!(log_text(&screen.arena), log_text(&expected));
        assert_eq!(screen.arena.tick, fight.ticks);
        assert_eq!(
            screen.result(),
            "Vainqueur : l'adversaire (abandon : votre combattant)"
        );
    }

    #[test]
    fn blood_spreads_fast_then_slows_and_stops() {
        assert!(blood_spread(0.0).abs() < f32::EPSILON);
        let (early, late) = (blood_spread(0.3), blood_spread(0.9));
        assert!(early > 0.3 && late > early && late < 1.0, "{early} {late}");
        assert!((blood_spread(BLOOD_SPREAD_SECONDS) - 1.0).abs() < f32::EPSILON);
        assert!((blood_spread(10.0) - 1.0).abs() < f32::EPSILON);
    }

    /// The loser of seed 42 dies: its stain starts at the death and keeps
    /// spreading after the fight, the winner never bleeds.
    #[test]
    fn only_the_dead_bleed_from_their_death_on() {
        let mut screen = screen(42);
        // The fight lasts 133 ticks, under 5 s.
        for _ in 0..60 * 5 {
            if screen.looks.iter().any(|look| look.dead_for.is_some()) {
                break;
            }
            screen.advance(1.0 / 60.0);
        }
        let dead = screen
            .arena
            .fighters
            .iter()
            .position(|f| !f.is_alive())
            .unwrap();
        assert_eq!(screen.looks[dead].dead_for, Some(0.0));
        for _ in 0..60 {
            screen.advance(1.0 / 60.0);
        }
        assert!((screen.looks[dead].dead_for.unwrap() - 1.0).abs() < 1e-3);
        assert!(screen.looks[1 - dead].dead_for.is_none());
    }

    /// In the standard duel of seed 15, fighter 0 is knocked out: it lies down, alive,
    /// without blood.
    #[test]
    fn a_fighter_knocked_out_lies_down_without_bleeding() {
        let mut screen = screen(15);
        while screen.running() {
            screen.advance(1.0 / 60.0);
        }
        assert_eq!(screen.looks[0].pose, Pose::Down);
        assert!(screen.looks.iter().all(|look| look.dead_for.is_none()));
        assert_eq!(
            screen.result(),
            "Vainqueur : combattant 1 (K.-O. : combattant 0)"
        );
    }

    /// In the standard duel of seed 62, fighter 1 loses a leg: it lies down, alive.
    #[test]
    fn a_fighter_that_lost_a_limb_lies_down_alive() {
        let mut screen = screen(62);
        while screen.running() {
            screen.advance(1.0 / 60.0);
        }
        assert_eq!(screen.looks[1].pose, Pose::Down);
        assert!(screen.looks.iter().all(|look| look.dead_for.is_none()));
        assert_eq!(
            screen.result(),
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
        assert_eq!(log_text(&screen.arena), log_text(&expected), "seed {seed}");
        assert_eq!(
            Report::new(&screen.arena),
            Report::new(&expected),
            "seed {seed}"
        );
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
                let speed = SPEEDS[frame % SPEEDS.len()].0;
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
        let casts = |winner_only: bool| {
            screen
                .arena
                .log
                .iter()
                .filter(|e| match e.kind {
                    EventKind::Cast { caster, missed, .. } => {
                        !missed
                            && (!winner_only
                                || screen.arena.fighters[index(caster.0).unwrap()].is_fighting())
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
