//! One fight shown on screen, a duel or a melee: its logic runs at its fixed tick rate,
//! fighters are drawn as animated sprites at positions interpolated between the last
//! two ticks. The same drawing serves the big view and the thumbnails.

use lanista_core::ai::{self, CombatAi};
use lanista_core::combat::{self, Arena, RADIUS};
use lanista_core::design_data::DesignData;
use lanista_core::duel::ais;
use lanista_core::fighter::injury::Location;
use lanista_core::fighter::{Effect, Fighter, FighterId};
use lanista_core::log::EventKind;
use lanista_core::melee::{in_danger, in_final, next_cut};
use macroquad::prelude::*;
use reesch::{Animation, AnimationEvent, AnimationPlayer, Assets, FixedStep};

use crate::fighter_sprite::{Dress, Missing, Placement, SHEET, SPRITE_SIZE, draw_fighter};
use crate::lang::tr;
use crate::texts::part_name;

// ponytail: placeholder sounds; per-spell and per-class sounds in `data/` later.
pub const HIT_SOUND: &str = "placeholder/hit.wav";
pub const DEATH_SOUND: &str = "placeholder/death.wav";
const SAND: Color = Color::new(0.76, 0.66, 0.48, 1.0);
/// The floor of an arena that slows fighters down: deeper, darker sand.
const DEEP_SAND: Color = Color::new(0.66, 0.54, 0.36, 1.0);
// ponytail: pillars drawn with shapes until the arenas get their sprites.
const PILLAR: Color = Color::new(0.55, 0.53, 0.5, 1.0);
const PILLAR_TOP: Color = Color::new(0.72, 0.7, 0.66, 1.0);
/// A mine on the floor, and its heart, red once armed.
const MINE: Color = Color::new(0.25, 0.22, 0.2, 1.0);
const MINE_ARMED: Color = Color::new(0.85, 0.15, 0.1, 1.0);
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
/// Share of a full stain under a fighter still bleeding.
const BLEEDING_SPREAD: f32 = 0.3;
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
/// Seconds a word over a fighter lasts, at fight speed.
pub const POPUP_SECONDS: f32 = 1.2;
/// A hit's words wait for the impact frame of its cast: two frames of 0.06 s.
const IMPACT_DELAY: f32 = 0.12;
/// Words stacked over a fighter at once; the next one takes the lowest place again.
const POPUP_ROWS: usize = 3;
/// Where a word appears over a fighter's center, how far it rises in its life, and
/// the gap between two stacked, in meters.
const POPUP_START: f32 = 1.15;
const POPUP_CLIMB: f32 = 0.6;
const POPUP_GAP: f32 = 0.4;

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
    /// Each fighter's Elo in the fight's colosseum, in the arena's order; `None` for one
    /// unranked.
    pub elos: Vec<Option<i32>>,
}

/// How a fight ended for the club.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Verdict {
    /// One of its fighters won.
    Victory,
    /// An opponent won.
    Defeat,
    /// Nobody won: all out, or the time ran out.
    NoWinner,
}

/// How a fighter went out of a fight.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum WayOut {
    Dead,
    Beheaded,
    Severed,
    LeftForDead,
    KnockedOut,
    GaveUp,
    Eliminated,
}

impl WayOut {
    /// In the order a melee's result counts them.
    const ALL: [Self; 7] = [
        Self::Dead,
        Self::Beheaded,
        Self::Severed,
        Self::LeftForDead,
        Self::KnockedOut,
        Self::GaveUp,
        Self::Eliminated,
    ];

    /// The key of the sentence telling a fighter went out this way.
    fn told(self) -> &'static str {
        match self {
            Self::Dead => "out-dead",
            Self::Beheaded => "out-beheaded",
            Self::Severed => "out-severed",
            Self::LeftForDead => "out-left-for-dead",
            Self::KnockedOut => "out-knocked-out",
            Self::GaveUp => "out-gave-up",
            Self::Eliminated => "out-eliminated",
        }
    }

    /// The key of how many went out this way.
    fn counted(self) -> &'static str {
        match self {
            Self::Dead => "out-count-dead",
            Self::Beheaded => "out-count-beheaded",
            Self::Severed => "out-count-severed",
            Self::LeftForDead => "out-count-left-for-dead",
            Self::KnockedOut => "out-count-knocked-out",
            Self::GaveUp => "out-count-gave-up",
            Self::Eliminated => "out-count-eliminated",
        }
    }
}

/// What a word over a fighter tells, for its color.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tone {
    /// Hit points lost to a hit or a mine.
    Damage,
    Miss,
    /// A wound that lasts, or the hit points it takes.
    Wound,
    /// Down or out of the fight.
    Out,
}

/// A word over a fighter for a moment: the damage of a hit, a miss, a wound.
#[derive(Debug, Clone, PartialEq)]
pub struct Popup {
    pub fighter: FighterId,
    pub text: String,
    pub tone: Tone,
    /// Seconds since it appeared, at fight speed; below 0 until its hit's impact.
    pub age: f32,
    /// Its place in the stack over the fighter, 0 the lowest.
    row: usize,
}

impl Popup {
    /// How opaque it is: fully, then fading out over the last third of its life.
    #[must_use]
    pub fn fade(&self) -> f32 {
        let left = 1.0 - self.age / POPUP_SECONDS;
        (left * 3.0).clamp(0.0, 1.0)
    }
}

pub struct FightView {
    pub arena: Arena,
    pub cast: Cast,
    /// The words over the fighters, the oldest first.
    pub popups: Vec<Popup>,
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
            ais: ais(&arena),
            previous: positions(&arena),
            arena,
            cast,
            popups: Vec::new(),
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
        // No cast animation nor word for hits nobody saw.
        self.seen_events = self.arena.log.len();
        self.popups.clear();
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
    /// Each new hit, wound or fall puts its word over the fighter.
    fn animate(&mut self, dt: f32) -> Vec<&'static str> {
        let mut sounds = Vec::new();
        for popup in &mut self.popups {
            popup.age += dt;
        }
        self.popups.retain(|p| p.age < POPUP_SECONDS);
        // A cast resolves within one tick: the log, not `action`, shows it.
        for event in &self.arena.log[self.seen_events..] {
            if let EventKind::Cast { caster, missed, .. } = event.kind
                && let Some(look) = index(caster.0).and_then(|i| self.looks.get_mut(i))
            {
                look.pose = Pose::Cast;
                look.player = AnimationPlayer::default();
                look.missed = missed;
            }
            if let Some((fighter, text, tone, delay)) = popup(event.kind) {
                let row = self.popups.iter().filter(|p| p.fighter == fighter).count() % POPUP_ROWS;
                self.popups.push(Popup {
                    fighter,
                    text,
                    tone,
                    age: -delay,
                    row,
                });
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
            .unwrap_or_else(|| tr!("fighter-number", number = id.0))
    }

    /// "Flamma (Augure) · Elo 1520": the fighter's name with its Elo, when ranked.
    pub fn name_and_elo(&self, id: FighterId) -> String {
        let name = self.name(id);
        match index(id.0).and_then(|i| self.cast.elos.get(i).copied().flatten()) {
            Some(elo) => tr!("fighter-elo", name = name.as_str(), elo = elo),
            None => name,
        }
    }

    /// The fighter still fighting once the fight is over.
    fn winner(&self) -> Option<&Fighter> {
        if !self.arena.is_over() {
            return None;
        }
        self.arena.fighters.iter().find(|f| f.is_fighting())
    }

    /// How the fight ended for the club: `None` when none of its fighters is in it.
    #[must_use]
    pub fn verdict(&self) -> Option<Verdict> {
        if self.cast.club.is_empty() {
            return None;
        }
        Some(match self.winner() {
            Some(winner) if self.cast.club.contains(&winner.id) => Verdict::Victory,
            Some(_) => Verdict::Defeat,
            None => Verdict::NoWinner,
        })
    }

    /// How fighter `f` went out of the fight, the gravest way first; `None` while it
    /// fights.
    fn way_out(&self, f: &Fighter) -> Option<WayOut> {
        let survived = self.cast.survivors.contains(&f.id);
        Some(match f.severed {
            Some(Location::Head) => WayOut::Beheaded,
            Some(_) => WayOut::Severed,
            None if !f.is_alive() && survived => WayOut::LeftForDead,
            None if !f.is_alive() => WayOut::Dead,
            None if f.knocked_out => WayOut::KnockedOut,
            None if f.surrendered => WayOut::GaveUp,
            None if f.eliminated => WayOut::Eliminated,
            None => return None,
        })
    }

    /// The fight's outcome, in sentences: who won, the opponent told as such when the
    /// club fights, then how each of the others went out in a duel, how many went out
    /// each way in a melee.
    #[must_use]
    pub fn result(&self) -> Vec<String> {
        if !self.arena.is_over() {
            return vec![tr!("result-time-up")];
        }
        let winner = self.winner().map_or_else(
            || tr!("result-no-winner"),
            |winner| {
                let name = self.name(winner.id);
                let opponent = !self.cast.club.is_empty() && !self.cast.club.contains(&winner.id);
                let name = if opponent {
                    tr!("result-opponent", name = name)
                } else {
                    name
                };
                tr!("result-winner", winner = name)
            },
        );
        let out: Vec<(&Fighter, WayOut)> = self
            .arena
            .fighters
            .iter()
            .filter_map(|f| self.way_out(f).map(|way| (f, way)))
            .collect();
        let mut lines = vec![winner];
        if self.arena.fighters.len() > 2 {
            let counts: Vec<String> = WayOut::ALL
                .into_iter()
                .filter_map(|way| {
                    let count = out.iter().filter(|(_, w)| *w == way).count();
                    (count > 0).then(|| tr!(way.counted(), count = count))
                })
                .collect();
            if !counts.is_empty() {
                lines.push(tr!("result-out", ways = counts.join(", ")));
            }
        } else {
            let told = out
                .iter()
                .map(|&(f, way)| tr!(way.told(), who = self.name(f.id)));
            lines.extend(told);
        }
        lines
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
                vec![tr!("final-duel")]
            } else {
                Vec::new()
            };
        };
        let seconds = cut.ticks_left.div_ceil(combat::TICKS_PER_SECOND);
        let mut lines = vec![
            tr!("elimination-next", seconds = seconds, out = cut.out),
            tr!("elimination-threshold", threshold = cut.threshold),
        ];
        for &id in &self.cast.club {
            let Some(f) = self.arena.fighter(id).filter(|f| f.is_fighting()) else {
                continue;
            };
            lines.push(tr!(
                "elimination-standing",
                fighter = self.name(id),
                damage = f.round_damage,
                danger = if in_danger(&self.arena, f) {
                    "true"
                } else {
                    "false"
                }
            ));
        }
        lines
    }

    /// The last `count` moments of the fight, in sentences, the latest last: who hit
    /// whom with what, for how much and where, the misses, wounds, falls and pacts. In a
    /// melee the club fights, only those its fighters take part in.
    #[must_use]
    pub fn feed(&self, count: usize) -> Vec<String> {
        let melee = self.arena.fighters.len() > 2 && !self.cast.club.is_empty();
        let mut lines: Vec<String> = self
            .arena
            .log
            .iter()
            .rev()
            .filter_map(|e| self.told(e.kind))
            .filter(|(ids, _)| !melee || ids.iter().any(|id| self.cast.club.contains(id)))
            .map(|(_, line)| line)
            .take(count)
            .collect();
        lines.reverse();
        lines
    }

    /// The sentence the feed tells `kind` in and the fighters it is about; `None` for
    /// what it leaves out: each second of bleeding, the floor's elements, a mine laid,
    /// the death of one just beheaded.
    fn told(&self, kind: EventKind) -> Option<([FighterId; 2], String)> {
        let part = |p: Location| part_name(p).to_lowercase();
        Some(match kind {
            EventKind::Cast { caster, target, .. } => ([caster, target], self.told_cast(kind)),
            EventKind::Blast {
                owner,
                target,
                damage,
                ..
            } => (
                [owner, target],
                tr!("feed-blast", target = self.name(target), damage = damage),
            ),
            EventKind::Afflicted {
                fighter,
                effect: Effect::Bleeding { part: at, .. },
            } => (
                [fighter; 2],
                tr!("feed-bleeding", who = self.name(fighter), part = part(at)),
            ),
            EventKind::Afflicted {
                fighter,
                effect: Effect::Blinded { percent },
            } => (
                [fighter; 2],
                tr!("feed-blinded", who = self.name(fighter), percent = percent),
            ),
            EventKind::Stun { fighter } => {
                ([fighter; 2], tr!("feed-stunned", who = self.name(fighter)))
            }
            EventKind::Rise { fighter } => {
                ([fighter; 2], tr!("feed-rise", who = self.name(fighter)))
            }
            EventKind::Sever {
                fighter,
                part: Location::Head,
            } => ([fighter; 2], tr!("out-beheaded", who = self.name(fighter))),
            EventKind::Sever { fighter, part: at } => (
                [fighter; 2],
                tr!("feed-severed", who = self.name(fighter), part = part(at)),
            ),
            EventKind::Death { fighter } => {
                let f = self.arena.fighter(fighter)?;
                if f.severed == Some(Location::Head) {
                    return None;
                }
                let way = if self.cast.survivors.contains(&fighter) {
                    WayOut::LeftForDead
                } else {
                    WayOut::Dead
                };
                ([fighter; 2], tr!(way.told(), who = self.name(fighter)))
            }
            EventKind::Surrender { fighter } => (
                [fighter; 2],
                tr!(WayOut::GaveUp.told(), who = self.name(fighter)),
            ),
            EventKind::KnockedOut { fighter } => (
                [fighter; 2],
                tr!(WayOut::KnockedOut.told(), who = self.name(fighter)),
            ),
            EventKind::Eliminated { fighter } => (
                [fighter; 2],
                tr!(WayOut::Eliminated.told(), who = self.name(fighter)),
            ),
            EventKind::Pact { a, b } => {
                ([a, b], tr!("feed-pact", a = self.name(a), b = self.name(b)))
            }
            EventKind::PactBroken {
                a,
                b,
                betrayer: Some(betrayer),
            } => ([a, b], tr!("feed-betrayal", betrayer = self.name(betrayer))),
            EventKind::PactBroken {
                a,
                b,
                betrayer: None,
            } => (
                [a, b],
                tr!("feed-pact-ended", a = self.name(a), b = self.name(b)),
            ),
            EventKind::Bled { .. } | EventKind::Gather { .. } | EventKind::Lay { .. } => {
                return None;
            }
        })
    }

    /// The feed's sentence for cast `kind`: who cast what on whom, missed or for how
    /// many HP, and where it landed in an arena with rules.
    fn told_cast(&self, kind: EventKind) -> String {
        let EventKind::Cast {
            caster,
            target,
            spell,
            damage,
            missed,
            part,
        } = kind
        else {
            return String::new();
        };
        let spell = self
            .arena
            .fighter(caster)
            .and_then(|f| f.spells.get(spell))
            .map_or_else(String::new, |s| s.name.clone());
        let (caster, target) = (self.name(caster), self.name(target));
        match (missed, part) {
            (true, _) => tr!("feed-miss", caster = caster, spell = spell, target = target),
            (false, Some(part)) => tr!(
                "feed-hit-part",
                caster = caster,
                spell = spell,
                target = target,
                damage = damage,
                part = part_name(part).to_lowercase()
            ),
            (false, None) => tr!(
                "feed-hit",
                caster = caster,
                spell = spell,
                target = target,
                damage = damage
            ),
        }
    }

    /// The words over the fighters whose hit has landed, each with the point of the
    /// world its text stands on: over its fighter, stacked, rising as it ages.
    #[must_use]
    pub fn shown_popups(&self) -> Vec<(Vec2, &Popup)> {
        let at = self.drawn_positions();
        self.popups
            .iter()
            .filter(|p| p.age >= 0.0)
            .filter_map(|p| {
                let place = index(p.fighter.0).and_then(|i| at.get(i))?;
                #[expect(clippy::cast_precision_loss, reason = "three rows at most")]
                let row = p.row as f32;
                let rise = POPUP_START + POPUP_CLIMB * p.age / POPUP_SECONDS + POPUP_GAP * row;
                Some((vec2(place.x, place.y - rise), p))
            })
            .collect()
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
    /// (deep sand, pillars), the mines, blood under the dead and those bleeding, the lines between allies, the rings under the club's
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
        for mine in &self.arena.mines {
            let (x, y) = (to_f32(mine.position.x), to_f32(mine.position.y));
            let radius = to_f32(self.arena.mine_rules.radius);
            draw_circle(x, y, radius, MINE);
            if self.arena.tick >= mine.armed_at {
                draw_circle(x, y, radius * 0.4, MINE_ARMED);
            }
        }
        // Blood under everyone.
        for (fighter, look) in self.arena.fighters.iter().zip(&self.looks) {
            if let Some(dead_for) = look.dead_for {
                let x = to_f32(fighter.position.x);
                let y = to_f32(fighter.position.y) + BLOOD_DROP;
                draw_blood(x, y, blood_spread(dead_for), fighter.id.0 % 2 == 1);
            } else if fighter.is_fighting()
                && fighter
                    .effects
                    .iter()
                    .any(|l| matches!(l.effect, Effect::Bleeding { .. }))
            {
                let x = to_f32(fighter.position.x);
                let y = to_f32(fighter.position.y) + BLOOD_DROP;
                draw_blood(x, y, BLEEDING_SPREAD, fighter.id.0 % 2 == 1);
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
            let flip_x = self.faces_left(i);
            let placement = Placement {
                x: at[i].x,
                y: at[i].y,
                frame: look.player.frame(animation),
                flip_x,
                shade: if lying {
                    DEAD_SHADE
                } else if aside {
                    OUT_SHADE
                } else {
                    1.0
                },
                missing: Missing::of(fighter, flip_x),
            };
            let dress = Dress {
                look: &fighter.appearance,
                class: data.class(fighter.class).map_or("", |c| c.key.as_str()),
                off_hand: fighter.off_hand,
            };
            draw_fighter(assets, rules, &dress, &placement);
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

/// The word `kind` puts over a fighter, its tone and how long it waits for its hit's
/// impact; `None` for what shows no word: a death has its cry, the rest its feed line.
fn popup(kind: EventKind) -> Option<(FighterId, String, Tone, f32)> {
    Some(match kind {
        EventKind::Cast {
            target,
            missed: true,
            ..
        } => (target, tr!("popup-miss"), Tone::Miss, IMPACT_DELAY),
        EventKind::Cast { target, damage, .. } => (
            target,
            tr!("popup-damage", damage = damage),
            Tone::Damage,
            IMPACT_DELAY,
        ),
        EventKind::Blast { target, damage, .. } => (
            target,
            tr!("popup-damage", damage = damage),
            Tone::Damage,
            0.0,
        ),
        EventKind::Bled {
            fighter, damage, ..
        } if damage > 0 => (
            fighter,
            tr!("popup-damage", damage = damage),
            Tone::Wound,
            0.0,
        ),
        EventKind::Afflicted { fighter, effect } => {
            let key = match effect {
                Effect::Bleeding { .. } => "popup-bleeding",
                Effect::Blinded { .. } => "popup-blinded",
            };
            (fighter, tr!(key), Tone::Wound, IMPACT_DELAY)
        }
        EventKind::Stun { fighter } => (fighter, tr!("popup-stunned"), Tone::Out, IMPACT_DELAY),
        EventKind::KnockedOut { fighter } => (fighter, tr!("popup-knocked-out"), Tone::Out, 0.0),
        EventKind::Surrender { fighter } => (fighter, tr!("popup-gave-up"), Tone::Out, 0.0),
        EventKind::Sever { fighter, part } => {
            let key = if part == Location::Head {
                "popup-beheaded"
            } else {
                "popup-severed"
            };
            (fighter, tr!(key), Tone::Wound, IMPACT_DELAY)
        }
        EventKind::Bled { .. }
        | EventKind::Death { .. }
        | EventKind::Rise { .. }
        | EventKind::Eliminated { .. }
        | EventKind::Pact { .. }
        | EventKind::PactBroken { .. }
        | EventKind::Gather { .. }
        | EventKind::Lay { .. } => return None,
    })
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
