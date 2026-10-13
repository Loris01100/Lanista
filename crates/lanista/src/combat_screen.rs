//! Watching a fight: the logic runs at its fixed tick rate, fighters are drawn
//! as animated sprites at positions interpolated between the last two ticks.

use lanista_core::ai::{self, CombatAi};
use lanista_core::combat::{self, Arena};
use lanista_core::design_data::DesignData;
use lanista_core::duel::{MAX_TICKS, random_duel, utility_ais};
use lanista_core::log::EventKind;
use macroquad::prelude::*;
use reesch::{
    Animation, AnimationPlayer, Assets, Bindings, FixedStep, Screen, SpriteSheet, Transition, egui,
    fit_camera,
};

use crate::Action;

pub const FIGHTER_SHEET: &str = "placeholder/fighter.png";
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Pose {
    Idle,
    Move,
    Cast,
    Death,
}

/// What a fighter is showing: its pose and how far into its animation.
struct Look {
    pose: Pose,
    player: AnimationPlayer,
}

impl Look {
    fn show(&mut self, pose: Pose) {
        if self.pose != pose {
            *self = Self {
                pose,
                player: AnimationPlayer::default(),
            };
        }
    }
}

pub struct CombatScreen {
    data: DesignData,
    /// Seed of the next fight, edited in the controls.
    next_seed: u64,
    arena: Arena,
    ais: Vec<Box<dyn CombatAi>>,
    step: FixedStep,
    /// Fighter positions before the last tick, to interpolate from.
    previous: Vec<combat::Vec2>,
    /// Log events already turned into animations.
    seen_events: usize,
    looks: Vec<Look>,
    animations: [Animation; 4],
    bindings: Bindings<Action>,
    /// Fight speed, a factor of real time: one of `SPEEDS`.
    speed: f32,
}

impl CombatScreen {
    /// The standard duel of `seed`, the same as `simulate --seed <seed>`.
    pub fn new(data: DesignData, seed: u64, bindings: Bindings<Action>) -> Self {
        let arena = random_duel(&data, seed);
        let animation = |row: u16, frame_time, looping| Animation {
            first: row * SHEET.columns,
            frames: 4,
            frame_time,
            looping,
            markers: Vec::new(),
        };
        let mut screen = Self {
            data,
            next_seed: seed,
            ais: utility_ais(&arena),
            previous: positions(&arena),
            arena,
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
            ],
            bindings,
            speed: 1.0,
        };
        screen.start(seed);
        screen
    }

    /// Starts the duel of `seed` from the beginning; the speed is kept.
    fn start(&mut self, seed: u64) {
        self.arena = random_duel(&self.data, seed);
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
    /// next fight.
    fn controls(&mut self, ui: &egui::Context) {
        let mut start = false;
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
                    ui.separator();
                    ui.label(format!("Graine {}", self.arena.seed));
                    ui.add(egui::DragValue::new(&mut self.next_seed));
                    start = ui.button("Lancer").clicked();
                });
            });
        if start {
            self.start(self.next_seed);
        }
    }

    fn result(&self) -> String {
        if !self.arena.is_over() {
            return "Temps écoulé, pas de vainqueur".into();
        }
        match self.arena.fighters.iter().find(|f| f.is_alive()) {
            Some(winner) => format!("Vainqueur : combattant {}", winner.id.0),
            None => "Aucun survivant".into(),
        }
    }

    fn running(&self) -> bool {
        !self.arena.is_over() && self.arena.tick < MAX_TICKS
    }

    fn animate(&mut self, dt: f32) {
        // A cast resolves within one tick: the log, not `action`, shows it.
        for event in &self.arena.log[self.seen_events..] {
            if let EventKind::Cast { caster, .. } = event.kind
                && let Some(look) = index(caster.0).and_then(|i| self.looks.get_mut(i))
            {
                look.pose = Pose::Cast;
                look.player = AnimationPlayer::default();
            }
        }
        self.seen_events = self.arena.log.len();

        for (fighter, look) in self.arena.fighters.iter().zip(&mut self.looks) {
            let casting = look.pose == Pose::Cast && !look.player.is_finished(&self.animations[2]);
            let pose = if !fighter.is_alive() {
                Pose::Death
            } else if casting {
                Pose::Cast
            } else if matches!(fighter.action, ai::Action::Move(_)) {
                Pose::Move
            } else {
                Pose::Idle
            };
            look.show(pose);
            look.player.advance(&self.animations[pose as usize], dt);
        }
    }
}

impl Screen for CombatScreen {
    fn update(&mut self, ui: &egui::Context, _assets: &Assets) -> Transition {
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
        self.controls(ui);

        // Speed scales time: more or fewer ticks per frame, animations alike.
        let dt = get_frame_time() * self.speed;
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
        self.animate(dt);
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
            let tint = index(fighter.id.0)
                .and_then(|i| FIGHTER_TINTS.get(i))
                .copied()
                .unwrap_or(WHITE);
            let frame = look.player.frame(&self.animations[look.pose as usize]);
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
