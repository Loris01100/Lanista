//! A fighter drawn from its look (`Appearance`): layers drawn in light grey, each
//! multiplied by a tint, on the same spritesheet grid, all at the same animation image;
//! the head and hair are one image each, moved and turned image by image to follow the
//! body. Formats: `docs/Formats_graphiques.md`.

use lanista_core::fighter::Fighter;
use lanista_core::fighter::appearance::{
    Appearance, AppearanceRules, LayerRule, Paint, Part, Tint,
};
use lanista_core::fighter::equipment::OffHand;
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

/// Each layer of a look as (name, variant): a Hair layer once for each hair shape.
fn files<'a>(
    rules: &'a AppearanceRules,
    (variant, layers): (&'a str, &'a [LayerRule]),
) -> Vec<(&'a str, &'a str)> {
    let mut files = Vec::new();
    for layer in layers {
        if layer.paint == Paint::Hair {
            files.extend(rules.hair.iter().map(|s| (layer.name.as_str(), s.as_str())));
        } else {
            files.push((layer.name.as_str(), variant));
        }
    }
    files
}

/// The drawings of the common look, which must be loaded, then those which may be
/// missing: the classes' own looks (a class is drawn with the common look until all of
/// its are there) and every layer's details.
#[must_use]
pub fn texture_paths(rules: &AppearanceRules) -> (Vec<String>, Vec<String>) {
    let common = files(rules, rules.default_look());
    let own: Vec<(&str, &str)> = rules
        .classes
        .keys()
        .flat_map(|class| files(rules, rules.look(class)))
        .collect();
    let required = common.iter().map(|&(l, v)| layer_path(l, v)).collect();
    let optional = own
        .iter()
        .map(|&(l, v)| layer_path(l, v))
        .chain(common.iter().chain(&own).map(|&(l, v)| detail_path(l, v)))
        .collect();
    (required, optional)
}

/// The variant and the layers a fighter of `class` with `off_hand` is drawn with: its
/// class's look if all of its drawings are loaded, otherwise the common one.
fn look_of<'a>(
    assets: &Assets,
    rules: &'a AppearanceRules,
    class: &str,
    off_hand: OffHand,
) -> (&'a str, &'a [LayerRule]) {
    let (variant, layers) = rules.look(class);
    let loaded = layers
        .iter()
        .filter(|l| l.drawn_with(off_hand) && l.paint != Paint::Hair)
        .all(|l| assets.try_texture(&layer_path(&l.name, variant)).is_some());
    if loaded {
        (variant, layers)
    } else {
        rules.default_look()
    }
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

    /// Whether the layers of `part` are gone.
    fn hides(self, part: Part) -> bool {
        match part {
            Part::Head => self.head,
            Part::ArmFront => self.arm_front,
            Part::ArmBack => self.arm_back,
            Part::LegFront => self.leg_front,
            Part::LegBack => self.leg_back,
            Part::Body => false,
        }
    }
}

/// The tints of a look: skin, hair and tunic.
struct Tints {
    skin: Tint,
    hair: Tint,
    tunic: Tint,
}

impl Tints {
    /// What a layer of `paint` is multiplied by.
    fn of(&self, paint: Paint) -> Tint {
        match paint {
            Paint::Skin => self.skin,
            Paint::Hair => self.hair,
            Paint::Tunic => self.tunic,
            Paint::Plain => (255, 255, 255),
        }
    }
}

/// The hair shape of a layered `look` and its tints; the first of each for a unique
/// one, or an index out of `rules`.
fn colors<'a>(rules: &'a AppearanceRules, look: &Appearance) -> (&'a str, Tints) {
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
    let tints = Tints {
        skin: pick(&rules.skins, skin),
        hair: pick(&rules.hair_tints, hair_tint),
        tunic: pick(&rules.tunics, tunic),
    };
    (shape, tints)
}

/// Who is drawn: its look, the key of its class and what its off hand holds.
pub struct Dress<'a> {
    pub look: &'a Appearance,
    pub class: &'a str,
    pub off_hand: OffHand,
}

/// Draws a fighter as `at` says, with its class's layers and those of its off hand,
/// without the parts it lost. A unique character missing from the assets is drawn as
/// the default layered look, and whole.
pub fn draw_fighter(assets: &Assets, rules: &AppearanceRules, who: &Dress, at: &Placement) {
    let look = who.look;
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
    let untinted = color((255, 255, 255), at.shade);
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
    let (variant, layers) = look_of(assets, rules, who.class, who.off_hand);
    for layer in layers {
        if !layer.drawn_with(who.off_hand) || at.missing.hides(layer.part) {
            continue;
        }
        let variant = if layer.paint == Paint::Hair {
            shape
        } else {
            variant
        };
        let draw = |texture: &Texture2D, color: Color| {
            if layer.part == Part::Head {
                head(texture, color);
            } else {
                sheet(texture, color);
            }
        };
        if let Some(texture) = assets.try_texture(&layer_path(&layer.name, variant)) {
            draw(texture, color(tints.of(layer.paint), at.shade));
        }
        if let Some(texture) = assets.try_texture(&detail_path(&layer.name, variant)) {
            draw(texture, untinted);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lanista_core::design_data::DesignData;

    #[test]
    fn every_drawing_the_common_look_names_is_in_the_assets() {
        let rules = DesignData::load().unwrap().fighter.appearance;
        let (required, optional) = texture_paths(&rules);
        // Six layers of the build and each hair shape.
        assert_eq!(required.len(), 6 + rules.hair.len());
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets");
        for path in &required {
            assert!(root.join(path).is_file(), "missing {path}");
        }
        assert!(required.contains(&"fighters/hair/long.png".to_owned()));
        assert!(optional.contains(&"fighters/head/average_detail.png".to_owned()));
        // A class's own look may wait for its drawings.
        assert!(optional.contains(&"fighters/shield/gladiateur.png".to_owned()));
        assert!(optional.contains(&"fighters/crest/gladiateur_detail.png".to_owned()));
    }

    #[test]
    fn a_class_waits_for_all_its_drawings_and_its_off_hand_picks_layers() {
        let rules = DesignData::load().unwrap().fighter.appearance;
        // Nothing loaded: the common look, whatever the class.
        let empty = Assets::default();
        assert_eq!(
            look_of(&empty, &rules, "gladiateur", OffHand::Shield),
            rules.default_look()
        );
        let (_, layers) = rules.look("gladiateur");
        let drawn = |off_hand| {
            layers
                .iter()
                .filter(|l| l.drawn_with(off_hand))
                .map(|l| l.name.as_str())
                .collect::<Vec<_>>()
        };
        assert!(drawn(OffHand::Shield).contains(&"shield"));
        assert!(!drawn(OffHand::Weapon).contains(&"shield"));
        assert!(drawn(OffHand::Weapon).contains(&"weapon_back"));
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
        assert!(right.hides(Part::ArmFront) && !right.hides(Part::Body));
        assert!(!right.hides(Part::Head));
        let left = Missing::of(&fighter, true);
        assert!(left.arm_back && left.leg_front && !left.arm_front && !left.leg_back);
        // Beheaded: no head, nor what goes with it.
        fighter.severed = Some(Location::Head);
        let beheaded = Missing::of(&fighter, false);
        assert!(beheaded.head && beheaded.hides(Part::Head));
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
        assert_eq!(tints.of(Paint::Skin), rules.skins[2]);
        assert_eq!(tints.of(Paint::Hair), rules.hair_tints[0]);
        assert_eq!(tints.of(Paint::Tunic), rules.tunics[1]);
        assert_eq!(tints.of(Paint::Plain), (255, 255, 255));
        // Out of the data or unique: the first shape and tints.
        let unknown = Appearance::Layered {
            hair: 9,
            skin: 9,
            hair_tint: 9,
            tunic: 9,
        };
        let (shape, tints) = colors(&rules, &unknown);
        assert_eq!(shape, rules.hair[0]);
        assert_eq!(
            (tints.skin, tints.hair, tints.tunic),
            (rules.skins[0], rules.hair_tints[0], rules.tunics[0])
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
