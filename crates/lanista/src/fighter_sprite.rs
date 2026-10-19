//! A fighter drawn from its look (`Appearance`): layers drawn in light grey, each
//! multiplied by a tint, on the same spritesheet grid, all at the same animation image;
//! the head and hair are one image each, moved and turned image by image to follow the
//! body. Formats: `docs/Formats_graphiques.md`.

use lanista_core::fighter::Fighter;
use lanista_core::fighter::appearance::{Appearance, AppearanceRules, Tint};
use lanista_core::fighter::injury::{Location, Side};
use macroquad::prelude::*;
use reesch::{Assets, SpriteSheet};

/// The spritesheet grid of every layer: 4 animations of 4 images of 32 × 32 px.
pub const SHEET: SpriteSheet = SpriteSheet {
    columns: 4,
    frame_width: 32.0,
    frame_height: 32.0,
};
/// The art's scale: pixels a meter (`docs/Formats_graphiques.md`).
pub const PIXELS_PER_METRE: f32 = 20.0;
/// Drawn size of a fighter, in meters (its body is 1 m wide): 32 px.
pub const SPRITE_SIZE: f32 = 1.6;
/// Pixels on a side of a fighter's image.
const PIXELS: f32 = 32.0;

/// What a layer is tinted with, in the order of `colors`.
#[derive(Clone, Copy)]
enum Paint {
    Skin,
    Hair,
    Tunic,
}

/// The layers drawn from the spritesheet, in drawing order, before the head.
const BEHIND_HEAD: [(&str, Paint); 4] = [
    ("leg_back", Paint::Skin),
    ("arm_back", Paint::Skin),
    ("body", Paint::Tunic),
    ("leg_front", Paint::Skin),
];
/// The layer drawn last, over the head.
const OVER_HEAD: (&str, Paint) = ("arm_front", Paint::Skin);

/// A layer's drawing: `fighters/<layer>/<variant>.png`.
fn layer_path(layer: &str, variant: &str) -> String {
    format!("fighters/{layer}/{variant}.png")
}

/// Its untinted details, if any: `fighters/<layer>/<variant>_detail.png`.
fn detail_path(layer: &str, variant: &str) -> String {
    format!("fighters/{layer}/{variant}_detail.png")
}

/// A unique character's whole spritesheet.
fn unique_path(name: &str) -> String {
    format!("uniques/{name}.png")
}

/// Every layer's drawing `rules` name, which must be loaded, then their details, which
/// may be missing.
#[must_use]
pub fn texture_paths(rules: &AppearanceRules) -> (Vec<String>, Vec<String>) {
    let build = rules.build.as_str();
    let mut layers: Vec<(&str, &str)> = BEHIND_HEAD
        .iter()
        .chain([&OVER_HEAD])
        .map(|&(layer, _)| (layer, build))
        .collect();
    layers.push(("head", build));
    layers.extend(rules.hair.iter().map(|shape| ("hair", shape.as_str())));
    let required = layers.iter().map(|&(l, v)| layer_path(l, v)).collect();
    let details = layers.iter().map(|&(l, v)| detail_path(l, v)).collect();
    (required, details)
}

/// `tint` darkened by `shade`, from 0 (black) to 1.
fn color(tint: Tint, shade: f32) -> Color {
    let channel = |c: u8| f32::from(c) / 255.0 * shade;
    Color::new(channel(tint.0), channel(tint.1), channel(tint.2), 1.0)
}

/// Where and how to draw one fighter.
pub struct Placement {
    /// Its center, in meters.
    pub x: f32,
    pub y: f32,
    /// The spritesheet image to show.
    pub frame: u16,
    /// Facing left.
    pub flip_x: bool,
    /// Brightness, from 0 to 1: dimmer once dead.
    pub shade: f32,
    /// The parts it no longer has, left undrawn.
    pub missing: Missing,
}

/// The parts of a fighter not drawn, cut off in the fight or lost before: the head
/// takes the hair with it, and the front limbs are the right ones of a fighter facing
/// right, the left ones once it faces left.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
#[expect(
    clippy::struct_excessive_bools,
    reason = "one flag per part a fighter may lose"
)]
pub struct Missing {
    pub head: bool,
    pub arm_front: bool,
    pub arm_back: bool,
    pub leg_front: bool,
    pub leg_back: bool,
}

impl Missing {
    /// What `fighter` has lost, drawn facing left if `flip_x`.
    #[must_use]
    pub fn of(fighter: &Fighter, flip_x: bool) -> Self {
        let (front, back) = if flip_x {
            (Side::Left, Side::Right)
        } else {
            (Side::Right, Side::Left)
        };
        Self {
            head: fighter.has_lost(Location::Head),
            arm_front: fighter.has_lost(Location::Arm(front)),
            arm_back: fighter.has_lost(Location::Arm(back)),
            leg_front: fighter.has_lost(Location::Leg(front)),
            leg_back: fighter.has_lost(Location::Leg(back)),
        }
    }

    /// Whether the layer `name` is gone.
    fn hides(self, name: &str) -> bool {
        match name {
            "head" | "hair" => self.head,
            "arm_front" => self.arm_front,
            "arm_back" => self.arm_back,
            "leg_front" => self.leg_front,
            "leg_back" => self.leg_back,
            _ => false,
        }
    }
}

/// The hair shape of a layered `look` and its tints, in `Paint` order; the first of
/// each for a unique one, or an index out of `rules`.
fn colors<'a>(rules: &'a AppearanceRules, look: &Appearance) -> (&'a str, [Tint; 3]) {
    let (hair, skin, hair_tint, tunic) = match *look {
        Appearance::Layered {
            hair,
            skin,
            hair_tint,
            tunic,
        } => (hair, skin, hair_tint, tunic),
        Appearance::Unique(_) => (0, 0, 0, 0),
    };
    let pick = |tints: &[Tint], i: u32| {
        let i = usize::try_from(i).unwrap_or(0);
        tints
            .get(i)
            .or(tints.first())
            .copied()
            .unwrap_or((255, 255, 255))
    };
    let shape = usize::try_from(hair)
        .ok()
        .and_then(|h| rules.hair.get(h))
        .or(rules.hair.first())
        .map_or("", String::as_str);
    let tints = [
        pick(&rules.skins, skin),
        pick(&rules.hair_tints, hair_tint),
        pick(&rules.tunics, tunic),
    ];
    (shape, tints)
}

/// Draws a fighter of `look` as `at` says, without the parts it lost. A unique
/// character missing from the assets is drawn as the default layered look, and whole.
pub fn draw_fighter(assets: &Assets, rules: &AppearanceRules, look: &Appearance, at: &Placement) {
    let dest = vec2(SPRITE_SIZE, SPRITE_SIZE);
    let (left, top) = (at.x - SPRITE_SIZE / 2.0, at.y - SPRITE_SIZE / 2.0);
    let sheet = |texture: &Texture2D, color: Color| {
        draw_texture_ex(
            texture,
            left,
            top,
            color,
            DrawTextureParams {
                dest_size: Some(dest),
                source: Some(SHEET.frame_rect(at.frame)),
                flip_x: at.flip_x,
                ..Default::default()
            },
        );
    };
    if let Appearance::Unique(name) = look
        && let Some(texture) = assets.try_texture(&unique_path(name))
    {
        sheet(texture, color((255, 255, 255), at.shade));
        return;
    }
    let (shape, tints) = colors(rules, look);
    let paint = |paint: Paint| tints[paint as usize];
    let untinted = color((255, 255, 255), at.shade);
    let build = rules.build.as_str();
    let layer = |(name, tint): (&str, Paint)| {
        if at.missing.hides(name) {
            return;
        }
        if let Some(texture) = assets.try_texture(&layer_path(name, build)) {
            sheet(texture, color(paint(tint), at.shade));
        }
        if let Some(texture) = assets.try_texture(&detail_path(name, build)) {
            sheet(texture, untinted);
        }
    };
    for part in BEHIND_HEAD {
        layer(part);
    }
    // The head and hair follow the neck: moved and turned for this image, mirrored
    // with the fighter.
    let scale = SPRITE_SIZE / PIXELS;
    let side = if at.flip_x { -1.0 } else { 1.0 };
    let frame = usize::from(at.frame);
    let (dx, dy, turn) = rules.head_frames.get(frame).copied().unwrap_or_default();
    #[expect(
        clippy::cast_possible_truncation,
        reason = "pixels and degrees of a 32 px image"
    )]
    let (dx, dy, turn) = (
        dx as f32 * scale * side,
        dy as f32 * scale,
        turn as f32 * side,
    );
    #[expect(clippy::cast_possible_truncation, reason = "a pixel of a 32 px image")]
    let (px, py) = (rules.head_pivot.0 as f32, rules.head_pivot.1 as f32);
    let px = if at.flip_x { PIXELS - px } else { px };
    let head = |texture: &Texture2D, color: Color| {
        draw_texture_ex(
            texture,
            left + dx,
            top + dy,
            color,
            DrawTextureParams {
                dest_size: Some(dest),
                rotation: turn.to_radians(),
                flip_x: at.flip_x,
                pivot: Some(vec2(left + dx + px * scale, top + dy + py * scale)),
                ..Default::default()
            },
        );
    };
    for (name, variant, tint) in [("head", build, Paint::Skin), ("hair", shape, Paint::Hair)] {
        if at.missing.hides(name) {
            continue;
        }
        if let Some(texture) = assets.try_texture(&layer_path(name, variant)) {
            head(texture, color(paint(tint), at.shade));
        }
        if let Some(texture) = assets.try_texture(&detail_path(name, variant)) {
            head(texture, untinted);
        }
    }
    layer(OVER_HEAD);
}

#[cfg(test)]
mod tests {
    use super::*;
    use lanista_core::design_data::DesignData;

    #[test]
    fn every_drawing_the_looks_name_is_in_the_assets() {
        let rules = DesignData::load().unwrap().fighter.appearance;
        let (required, details) = texture_paths(&rules);
        // Six layers of the build and each hair shape.
        assert_eq!(required.len(), 6 + rules.hair.len());
        assert_eq!(details.len(), required.len());
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets");
        for path in &required {
            assert!(root.join(path).is_file(), "missing {path}");
        }
        assert!(required.contains(&"fighters/hair/long.png".to_owned()));
        assert!(details.contains(&"fighters/head/average_detail.png".to_owned()));
    }

    #[test]
    fn a_lost_part_is_left_undrawn_on_the_side_it_faces() {
        use lanista_core::fighter::injury::severed_limb_wound;

        let data = DesignData::load().unwrap();
        let standard = lanista_core::duel::random_duel(&data, 1);
        let mut fighter = standard.fighters[0].clone();
        assert_eq!(Missing::of(&fighter, false), Missing::default());
        // A right arm lost before, a left leg cut off now.
        fighter.condition.injuries = vec![severed_limb_wound(Location::Arm(Side::Right))];
        fighter.severed = Some(Location::Leg(Side::Left));
        let right = Missing::of(&fighter, false);
        assert!(right.arm_front && right.leg_back && !right.arm_back && !right.leg_front);
        assert!(right.hides("arm_front") && !right.hides("body") && !right.hides("hair"));
        let left = Missing::of(&fighter, true);
        assert!(left.arm_back && left.leg_front && !left.arm_front && !left.leg_back);
        // Beheaded: no head, no hair.
        fighter.severed = Some(Location::Head);
        let beheaded = Missing::of(&fighter, false);
        assert!(beheaded.head && beheaded.hides("head") && beheaded.hides("hair"));
    }

    #[test]
    fn a_look_picks_its_shape_and_tints() {
        let rules = DesignData::load().unwrap().fighter.appearance;
        let look = Appearance::Layered {
            hair: 1,
            skin: 2,
            hair_tint: 0,
            tunic: 1,
        };
        let (shape, tints) = colors(&rules, &look);
        assert_eq!(shape, rules.hair[1]);
        assert_eq!(tints[Paint::Skin as usize], rules.skins[2]);
        assert_eq!(tints[Paint::Hair as usize], rules.hair_tints[0]);
        assert_eq!(tints[Paint::Tunic as usize], rules.tunics[1]);
        // Out of the data or unique: the first shape and tints.
        let unknown = Appearance::Layered {
            hair: 9,
            skin: 9,
            hair_tint: 9,
            tunic: 9,
        };
        assert_eq!(
            colors(&rules, &unknown),
            (
                rules.hair[0].as_str(),
                [rules.skins[0], rules.hair_tints[0], rules.tunics[0]]
            )
        );
        assert_eq!(
            colors(&rules, &Appearance::Unique("x".into())).0,
            rules.hair[0]
        );
    }

    #[test]
    fn tints_are_multiplied_and_shaded() {
        let full = color((255, 128, 0), 1.0);
        assert!((full.r - 1.0).abs() < 1e-6 && full.b.abs() < 1e-6);
        let dim = color((255, 255, 255), 0.5);
        assert!((dim.g - 0.5).abs() < 1e-6 && (dim.a - 1.0).abs() < 1e-6);
    }
}
