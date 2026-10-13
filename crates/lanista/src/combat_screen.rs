//! Watching a fight: the logic runs at its fixed tick rate, fighters are drawn
//! as animated sprites at positions interpolated between the last two ticks.

use lanista_core::ai::{self, CombatAi, UtilityAi};
use lanista_core::combat::{self, Arena};
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
    arena: Arena,
    ais: Vec<Box<dyn CombatAi>>,
    max_ticks: u32,
    step: FixedStep,
    /// Fighter positions before the last tick, to interpolate from.
    previous: Vec<combat::Vec2>,
    /// Log events already turned into animations.
    seen_events: usize,
    looks: Vec<Look>,
    animations: [Animation; 4],
    bindings: Bindings<Action>,
}

impl CombatScreen {
    pub fn new(arena: Arena, max_ticks: u32, bindings: Bindings<Action>) -> Self {
        let ais = arena
            .fighters
            .iter()
            .map(|_| Box::new(UtilityAi) as Box<dyn CombatAi>)
            .collect();
        let looks = arena
            .fighters
            .iter()
            .map(|_| Look {
                pose: Pose::Idle,
                player: AnimationPlayer::default(),
            })
            .collect();
        let animation = |row: u16, frame_time, looping| Animation {
            first: row * SHEET.columns,
            frames: 4,
            frame_time,
            looping,
            markers: Vec::new(),
        };
        Self {
            previous: positions(&arena),
            arena,
            ais,
            max_ticks,
            step: FixedStep::new(combat::TICKS_PER_SECOND),
            seen_events: 0,
            looks,
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
        }
    }

    fn running(&self) -> bool {
        !self.arena.is_over() && self.arena.tick < self.max_ticks
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
        let dt = get_frame_time();
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
