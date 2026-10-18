//! One fight shown on screen, a duel or a melee: its logic runs at its fixed tick rate,
//! fighters are drawn as animated sprites at positions interpolated between the last
//! two ticks. The same drawing serves the big view and the thumbnails.

use lanista_core::ai::{self, CombatAi};
use lanista_core::combat::{self, Arena, RADIUS};
use lanista_core::design_data::DesignData;
use lanista_core::duel::utility_ais;
use lanista_core::fighter::FighterId;
use lanista_core::fighter::injury::Location;
use lanista_core::log::EventKind;
use lanista_core::melee::{in_danger, in_final, next_cut};
use macroquad::prelude::*;
use reesch::{Animation, AnimationEvent, AnimationPlayer, Assets, FixedStep};

use crate::fighter_sprite::{Placement, SHEET, SPRITE_SIZE, draw_fighter};

// ponytail: placeholder sounds; per-spell and per-class sounds in `data/` later.
pub const HIT_SOUND: &str = "placeholder/hit.wav";
pub const DEATH_SOUND: &str = "placeholder/death.wav";
const SAND: Color = Color::new(0.76, 0.66, 0.48, 1.0);
/// The floor of an arena that slows fighters down: deeper, darker sand.
const DEEP_SAND: Color = Color::new(0.66, 0.54, 0.36, 1.0);
// ponytail: pillars drawn with shapes until the arenas get their sprites.
const PILLAR: Color = Color::new(0.55, 0.53, 0.5, 1.0);
const PILLAR_TOP: Color = Color::new(0.72, 0.7, 0.66, 1.0);
/// Around the floor.
pub const STANDS: Color = Color::new(0.16, 0.13, 0.11, 1.0);
/// The ring under the club's fighters.
const CLUB: Color = Color::new(1.0, 0.82, 0.25, 0.9);
/// The line between two allies.
const PACT: Color = Color::new(0.55, 0.8, 1.0, 0.8);
/// The damage gauge over a melee fighter: safe, or out if the check came now.
const SAFE: Color = Color::new(0.35, 0.8, 0.35, 1.0);
const DANGER: Color = Color::new(0.9, 0.2, 0.15, 1.0);
const GAUGE_BACK: Color = Color::new(0.1, 0.08, 0.07, 0.8);
/// The damage gauge's size and its height over a fighter's center, in meters.
const GAUGE_SIZE: (f32, f32) = (0.9, 0.12);
const GAUGE_RISE: f32 = 0.95;
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
/// Brightness of a dead fighter's sprite once it lies down, a share of its tints.
const DEAD_SHADE: f32 = 0.6;
/// Brightness of a fighter out of the fight standing: it gave up or was eliminated.
const OUT_SHADE: f32 = 0.75;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Pose {
    Idle,
    Move,
    Cast,
    Death,
    /// Stunned, knocked out, maimed or left for dead: lying down, alive.
    Down,
}

/// What a fighter is showing: its pose and how far into its animation.
pub struct Look {
    pub pose: Pose,
    player: AnimationPlayer,
    /// Seconds since the death was shown, for the blood stain; `None` while alive.
    pub dead_for: Option<f32>,
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

/// Who is who in a watched fight.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Cast {
    /// Each fighter's name, in the arena's order; « combattant n » for one missing.
    pub names: Vec<String>,
    /// The club's fighters, ringed.
    pub club: Vec<FighterId>,
    /// Those the arena kills who survive the fatal blow, once the fight is over: they
    /// fall and lie down alive, without bleeding out.
    pub survivors: Vec<FighterId>,
}

pub struct FightView {
    pub arena: Arena,
    pub cast: Cast,
    ais: Vec<Box<dyn CombatAi>>,
    step: FixedStep,
    /// Fighter positions before the last tick, to interpolate from.
    previous: Vec<combat::Vec2>,
    /// Log events already turned into animations.
    seen_events: usize,
    pub looks: Vec<Look>,
    animations: [Animation; 5],
}

impl FightView {
    /// Shows `arena` from where it is, with `cast`.
    #[must_use]
    pub fn new(arena: Arena, cast: Cast) -> Self {
        let animation = |row: u16, frame_time, looping| Animation {
            first: row * SHEET.columns,
            frames: 4,
            frame_time,
            looping,
            markers: Vec::new(),
        };
        let looks = arena
            .fighters
            .iter()
            .map(|_| Look {
                pose: Pose::Idle,
                player: AnimationPlayer::default(),
                dead_for: None,
                missed: false,
            })
            .collect();
        Self {
            ais: utility_ais(&arena),
            previous: positions(&arena),
            arena,
            cast,
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
                // ponytail: the death row until the real sprites bring a stunned pose.
                animation(3, 0.1, false),
            ],
        }
    }

    #[must_use]
    pub fn running(&self) -> bool {
        !self.arena.is_over() && self.arena.tick < self.arena.max_ticks
    }

    /// Plays the rest of the fight at once, without drawing it.
    pub fn skip(&mut self) {
        let ticks = self.arena.max_ticks;
        combat::run(&mut self.arena, &mut self.ais, ticks);
        self.previous = positions(&self.arena);
        // No cast animation for hits nobody saw.
        self.seen_events = self.arena.log.len();
    }

    /// Plays the ticks due after `dt` seconds of fight time, then animates;
    /// returns the sounds to play. No macroquad call, so tests can drive it.
    pub fn advance(&mut self, dt: f32) -> Vec<&'static str> {
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
    /// a cry at each death. A survivor of the fatal blow falls without one.
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
            let survives = self.cast.survivors.contains(&fighter.id);
            let pose = if !fighter.is_alive() && !survives {
                Pose::Death
            } else if !fighter.is_alive()
                || (!fighter.is_standing() && !fighter.surrendered && !fighter.eliminated)
            {
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

    /// How the result names fighter `id`.
    #[must_use]
    pub fn name(&self, id: FighterId) -> String {
        index(id.0)
            .and_then(|i| self.cast.names.get(i))
            .cloned()
            .unwrap_or_else(|| format!("combattant {}", id.0))
    }

    /// The fight's outcome, in words: its winner and how the others went out, named in
    /// a duel, counted in a melee.
    #[must_use]
    pub fn result(&self) -> String {
        if !self.arena.is_over() {
            return "Temps écoulé, pas de vainqueur".into();
        }
        let winner = match self.arena.fighters.iter().find(|f| f.is_fighting()) {
            Some(winner) => format!("Vainqueur : {}", self.name(winner.id)),
            None => "Aucun vainqueur".into(),
        };
        let melee = self.arena.fighters.len() > 2;
        let out = |how: &str, out: &dyn Fn(&lanista_core::fighter::Fighter) -> bool| {
            let names: Vec<String> = self
                .arena
                .fighters
                .iter()
                .filter(|f| out(f))
                .map(|f| self.name(f.id))
                .collect();
            let told = if melee {
                names.len().to_string()
            } else {
                names.join(", ")
            };
            (!names.is_empty()).then(|| format!("{how} : {told}"))
        };
        let reasons: Vec<String> = [
            out("mort", &|f| {
                melee && !f.is_alive() && !self.cast.survivors.contains(&f.id)
            }),
            out("abandon", &|f| f.surrendered),
            out("éliminé faute de coups", &|f| f.eliminated),
            out("K.-O.", &|f| f.knocked_out),
            out("laissé pour mort, survit", &|f| {
                !f.is_alive() && self.cast.survivors.contains(&f.id)
            }),
            out("décapité", &|f| f.severed == Some(Location::Head)),
            out("membre tranché", &|f| {
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

    /// What the next check of a melee holds, in words, for the player: when it comes,
    /// how many it puts out and the damage it takes to stay, then where each of the
    /// club's fighters stands; « Duel final » once the checks are over. Nothing in a
    /// duel, nor once the fight is over.
    #[must_use]
    pub fn elimination_lines(&self) -> Vec<String> {
        if !self.running() {
            return Vec::new();
        }
        let Some(cut) = next_cut(&self.arena) else {
            return if in_final(&self.arena) {
                vec!["Duel final".into()]
            } else {
                Vec::new()
            };
        };
        let seconds = cut.ticks_left.div_ceil(combat::TICKS_PER_SECOND);
        let out = match cut.out {
            1 => "1 combattant sort".into(),
            n => format!("{n} combattants sortent"),
        };
        let mut lines = vec![
            format!("Élimination dans {seconds} s : {out}"),
            format!(
                "Pour rester, plus de {} dégâts depuis la dernière élimination",
                cut.threshold
            ),
        ];
        for &id in &self.cast.club {
            let Some(f) = self.arena.fighter(id).filter(|f| f.is_fighting()) else {
                continue;
            };
            let standing = if in_danger(&self.arena, f) {
                "éliminé si rien ne change"
            } else {
                "à l'abri"
            };
            lines.push(format!(
                "{} : {} dégâts, {standing}",
                self.name(id),
                f.round_damage
            ));
        }
        lines
    }

    /// The world area to show: the floor and a meter around it.
    #[must_use]
    pub fn world(&self) -> Rect {
        let (width, height) = (to_f32(self.arena.width), to_f32(self.arena.height));
        Rect::new(-1.0, -1.0, width + 2.0, height + 2.0)
    }

    /// Where each fighter is drawn now, between its last two positions.
    fn drawn_positions(&self) -> Vec<Vec2> {
        let alpha = self.step.alpha();
        self.previous
            .iter()
            .zip(&self.arena.fighters)
            .map(|(from, f)| {
                let to = f.position;
                vec2(
                    to_f32(from.x + (to.x - from.x) * alpha),
                    to_f32(from.y + (to.y - from.y) * alpha),
                )
            })
            .collect()
    }

    /// The fighter drawn at `point` of the world, the one standing on top first.
    #[must_use]
    pub fn fighter_at(&self, point: Vec2) -> Option<FighterId> {
        let reach = SPRITE_SIZE / 2.0;
        self.drawn_positions()
            .iter()
            .zip(&self.arena.fighters)
            .rev()
            .find(|(at, _)| (point.x - at.x).abs() <= reach && (point.y - at.y).abs() <= reach)
            .map(|(_, f)| f.id)
    }

    /// Draws the fight in world coordinates, a camera on it: the floor of its arena kind
    /// (deep sand, pillars), blood, the lines between allies, the rings under the club's
    /// fighters, then the fighters, the dead first, those out of the fight dimmed.
    pub fn draw(&self, assets: &Assets, data: &DesignData) {
        let (width, height) = (to_f32(self.arena.width), to_f32(self.arena.height));
        let floor = if self.arena.speed_percent < 100 {
            DEEP_SAND
        } else {
            SAND
        };
        draw_rectangle(0.0, 0.0, width, height, floor);
        for pillar in &self.arena.pillars {
            let (x, y, radius) = (to_f32(pillar.x), to_f32(pillar.y), to_f32(pillar.radius));
            draw_circle(x, y, radius, PILLAR);
            draw_circle(x, y, radius * 0.75, PILLAR_TOP);
        }
        // Blood under everyone.
        for (fighter, look) in self.arena.fighters.iter().zip(&self.looks) {
            if let Some(dead_for) = look.dead_for {
                let x = to_f32(fighter.position.x);
                let y = to_f32(fighter.position.y) + BLOOD_DROP;
                draw_blood(x, y, blood_spread(dead_for), fighter.id.0 % 2 == 1);
            }
        }
        let at = self.drawn_positions();
        let place = |id: FighterId| index(id.0).and_then(|i| at.get(i)).copied();
        for &(a, b) in &self.arena.pacts {
            if let (Some(a), Some(b)) = (place(a), place(b)) {
                draw_line(a.x, a.y, b.x, b.y, 0.06, PACT);
            }
        }
        let radius = to_f32(RADIUS) * 1.2;
        for &id in &self.cast.club {
            if let (Some(p), Some(f)) = (place(id), self.arena.fighter(id))
                && f.is_alive()
            {
                draw_circle_lines(p.x, p.y + radius * 0.5, radius, 0.08, CLUB);
            }
        }
        self.draw_gauges(&at);
        let rules = &data.fighter.appearance;
        // The dead first, so the living walk over them.
        let mut order: Vec<usize> = (0..self.arena.fighters.len()).collect();
        order.sort_by_key(|&i| self.arena.fighters[i].is_alive());
        for i in order {
            let fighter = &self.arena.fighters[i];
            let look = &self.looks[i];
            let animation = &self.animations[look.pose as usize];
            let lying = look.pose == Pose::Death && look.player.is_finished(animation);
            let aside = fighter.surrendered || fighter.eliminated;
            let placement = Placement {
                x: at[i].x,
                y: at[i].y,
                frame: look.player.frame(animation),
                flip_x: self.faces_left(i),
                shade: if lying {
                    DEAD_SHADE
                } else if aside {
                    OUT_SHADE
                } else {
                    1.0
                },
            };
            draw_fighter(assets, rules, &fighter.appearance, &placement);
        }
    }

    /// Over each fighter still in a melee, before a check: the damage it dealt since the
    /// last one, full past what it takes to stay, red if the check would put it out.
    fn draw_gauges(&self, at: &[Vec2]) {
        let Some(cut) = next_cut(&self.arena).filter(|_| self.running()) else {
            return;
        };
        let (width, height) = GAUGE_SIZE;
        let needed = f64::from(cut.threshold) + 1.0;
        for (fighter, p) in self.arena.fighters.iter().zip(at) {
            if !fighter.is_fighting() {
                continue;
            }
            let share = to_f32((f64::from(fighter.round_damage) / needed).min(1.0));
            let color = if in_danger(&self.arena, fighter) {
                DANGER
            } else {
                SAFE
            };
            let (x, y) = (p.x - width / 2.0, p.y - GAUGE_RISE);
            draw_rectangle(x, y, width, height, GAUGE_BACK);
            draw_rectangle(x, y, width * share.max(0.08), height, color);
        }
    }

    /// Sprites face right: fighter `i` turns toward its nearest opponent still standing.
    fn faces_left(&self, i: usize) -> bool {
        let me = &self.arena.fighters[i];
        let nearest = self
            .arena
            .fighters
            .iter()
            .filter(|f| f.id != me.id && f.is_standing())
            .min_by(|a, b| {
                let d = |f: &lanista_core::fighter::Fighter| {
                    (f.position.x - me.position.x).abs() + (f.position.y - me.position.y).abs()
                };
                d(a).total_cmp(&d(b))
            });
        nearest.is_some_and(|f| f.position.x < me.position.x)
    }
}

/// How far a blood stain has spread `dead_for` seconds after the death, from 0 to 1:
/// fast at first, then slowing down (ease-out).
#[must_use]
pub fn blood_spread(dead_for: f32) -> f32 {
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
    use super::*;
    use lanista_core::duel::random_duel;

    #[test]
    fn blood_spreads_fast_then_slows_and_stops() {
        assert!(blood_spread(0.0).abs() < f32::EPSILON);
        let (early, late) = (blood_spread(0.3), blood_spread(0.9));
        assert!(early > 0.3 && late > early && late < 1.0, "{early} {late}");
        assert!((blood_spread(BLOOD_SPREAD_SECONDS) - 1.0).abs() < f32::EPSILON);
        assert!((blood_spread(10.0) - 1.0).abs() < f32::EPSILON);
    }

    #[test]
    fn a_melee_tells_its_next_elimination_and_where_the_club_stands() {
        use lanista_core::combat::ArenaKind;
        use lanista_core::melee::melee_arena;

        let data = DesignData::load().unwrap();
        let fighters = (0..20)
            .flat_map(|seed| random_duel(&data, seed).fighters)
            .take(20)
            .enumerate()
            .map(|(i, mut f)| {
                f.id = FighterId(u32::try_from(i).unwrap());
                f
            })
            .collect();
        let arena = melee_arena(fighters, ArenaKind::Sand, 1, &data.combat);
        let cast = Cast {
            names: vec!["Mirmillon n° 1".into()],
            club: vec![FighterId(0)],
            ..Cast::default()
        };
        let mut view = FightView::new(arena, cast);
        let lines = view.elimination_lines();
        assert_eq!(lines[0], "Élimination dans 15 s : 3 combattants sortent");
        assert!(lines[1].starts_with("Pour rester, plus de 0 dégâts"));
        assert!(
            lines[2].starts_with("Mirmillon n° 1 : 0 dégâts, "),
            "{lines:?}"
        );
        view.skip();
        assert_eq!(view.elimination_lines(), Vec::<String>::new());
        // A duel tells nothing.
        let duel = FightView::new(random_duel(&data, 1), Cast::default());
        assert_eq!(duel.elimination_lines(), Vec::<String>::new());
    }

    #[test]
    fn a_fighter_is_found_under_the_pointer_and_named() {
        let data = DesignData::load().unwrap();
        let arena = random_duel(&data, 42);
        let first = arena.fighters[0].position;
        let cast = Cast {
            names: vec!["Mirmillon n° 1".into()],
            ..Cast::default()
        };
        let view = FightView::new(arena, cast);
        let on = vec2(to_f32(first.x) + 0.3, to_f32(first.y) - 0.3);
        assert_eq!(view.fighter_at(on), Some(FighterId(0)));
        assert_eq!(view.fighter_at(vec2(-5.0, -5.0)), None);
        assert_eq!(view.name(FighterId(0)), "Mirmillon n° 1");
        assert_eq!(view.name(FighterId(1)), "combattant 1");
        assert_eq!(view.world(), Rect::new(-1.0, -1.0, 22.0, 22.0));
    }
}
