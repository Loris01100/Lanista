//! 2D camera that fits a world area to any screen size and aspect ratio
//! (16:9 monitors, 16:10 Steam Deck, resized windows).

use macroquad::camera::Camera2D;
use macroquad::math::{Rect, Vec2, vec2};
use macroquad::window::{screen_height, screen_width};

/// Camera showing all of `view` (world units, y down), as large as possible
/// and centred. Call it every frame: it follows window resizes.
#[must_use]
pub fn fit_camera(view: Rect) -> Camera2D {
    let mut camera =
        Camera2D::from_display_rect(fit_view(view, vec2(screen_width(), screen_height())));
    // `from_display_rect` gives y up on screen (textures upside down); flip it.
    camera.zoom.y = -camera.zoom.y;
    camera
}

/// Like `fit_camera`, for pixel art drawn `pixels_per_unit` pixels a world unit: see
/// `pixel_view`.
#[must_use]
pub fn pixel_camera(view: Rect, pixels_per_unit: f32) -> Camera2D {
    let shown = pixel_view(view, vec2(screen_width(), screen_height()), pixels_per_unit);
    let mut camera = Camera2D::from_display_rect(shown);
    camera.zoom.y = -camera.zoom.y;
    camera
}

/// The area shown `t` of the way from `from` to `to` (0 to 1, clamped), eased in and
/// out: a camera moving from one framing to another. Two areas of the screen's aspect
/// ratio (`fit_view`, `pixel_view`) blend into one of that ratio, which `fit_camera`
/// shows as it is.
#[must_use]
pub fn blend_views(from: Rect, to: Rect, t: f32) -> Rect {
    let t = t.clamp(0.0, 1.0);
    let eased = t * t * (3.0 - 2.0 * t);
    let mix = |a: f32, b: f32| a + (b - a) * eased;
    Rect::new(
        mix(from.x, to.x),
        mix(from.y, to.y),
        mix(from.w, to.w),
        mix(from.h, to.h),
    )
}

/// Smallest area with the screen's aspect ratio that contains `view`, centred
/// on it: the extra space shows more of the world instead of black bars.
#[must_use]
pub fn fit_view(view: Rect, screen: Vec2) -> Rect {
    if screen.x <= 0.0 || screen.y <= 0.0 {
        return view; // minimised window
    }
    centred(view, screen, fit_scale(view, screen))
}

/// The area of the world shown when pixel art drawn `pixels_per_unit` pixels a world
/// unit is enlarged by the largest whole factor that still shows all of `view`, so that
/// every pixel of the art takes as many pixels of the screen; more of the world shows
/// around it. A screen too small to show `view` unenlarged shrinks it as `fit_view`.
#[must_use]
pub fn pixel_view(view: Rect, screen: Vec2, pixels_per_unit: f32) -> Rect {
    if screen.x <= 0.0 || screen.y <= 0.0 {
        return view;
    }
    let fit = fit_scale(view, screen);
    let factor = (fit / pixels_per_unit).floor();
    let scale = if factor >= 1.0 {
        factor * pixels_per_unit
    } else {
        fit
    };
    centred(view, screen, scale)
}

/// Screen pixels per world unit that show all of `view`, as large as possible.
fn fit_scale(view: Rect, screen: Vec2) -> f32 {
    (screen.x / view.w).min(screen.y / view.h)
}

/// The area of `screen` pixels, `scale` pixels a world unit, centred on `view`.
fn centred(view: Rect, screen: Vec2, scale: f32) -> Rect {
    let size = screen / scale;
    Rect::new(
        view.center().x - size.x / 2.0,
        view.center().y - size.y / 2.0,
        size.x,
        size.y,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    const ARENA: Rect = Rect {
        x: 0.0,
        y: 0.0,
        w: 20.0,
        h: 20.0,
    };

    #[test]
    fn steam_deck_widens_a_square_view() {
        // 1280 × 800: 40 px per metre, 32 m wide.
        assert_eq!(
            fit_view(ARENA, vec2(1280.0, 800.0)),
            Rect::new(-6.0, 0.0, 32.0, 20.0)
        );
    }

    #[test]
    fn portrait_window_heightens_it() {
        assert_eq!(
            fit_view(ARENA, vec2(400.0, 800.0)),
            Rect::new(0.0, -10.0, 20.0, 40.0)
        );
    }

    #[test]
    fn minimised_window_keeps_the_view() {
        assert_eq!(fit_view(ARENA, Vec2::ZERO), ARENA);
        assert_eq!(pixel_view(ARENA, Vec2::ZERO, 20.0), ARENA);
    }

    /// 20 m of arena and 1 m on each side, drawn 20 px a metre.
    const WITH_MARGIN: Rect = Rect {
        x: -1.0,
        y: -1.0,
        w: 22.0,
        h: 22.0,
    };

    #[test]
    fn pixel_art_is_enlarged_by_a_whole_factor() {
        // 800 px high: 36 px a metre would fit, ×1 (20 px) is shown, 64 × 40 m.
        assert_eq!(
            pixel_view(WITH_MARGIN, vec2(1280.0, 800.0), 20.0),
            Rect::new(-22.0, -10.0, 64.0, 40.0)
        );
        // 1080 px high: ×2, 40 px a metre, 48 × 27 m.
        assert_eq!(
            pixel_view(WITH_MARGIN, vec2(1920.0, 1080.0), 20.0),
            Rect::new(-14.0, -3.5, 48.0, 27.0)
        );
        // 1440 px high: ×3.
        let shown = pixel_view(WITH_MARGIN, vec2(2560.0, 1440.0), 20.0);
        assert!((shown.h - 24.0).abs() < 1e-4);
    }

    #[test]
    fn a_blend_eases_from_one_framing_to_the_other() {
        let close = Rect::new(8.0, 9.0, 4.0, 2.5);
        let whole = fit_view(ARENA, vec2(1280.0, 800.0));
        assert_eq!(blend_views(whole, close, -1.0), whole);
        assert_eq!(blend_views(whole, close, 2.0), close);
        let half = blend_views(whole, close, 0.5);
        assert_eq!(half, Rect::new(1.0, 4.5, 18.0, 11.25));
        // Slow at both ends, and the screen's aspect ratio all the way.
        let early = blend_views(whole, close, 0.1);
        assert!(early.x - whole.x < (close.x - whole.x) * 0.1);
        assert!((early.w / early.h - 1.6).abs() < 1e-5);
    }

    #[test]
    fn too_small_a_screen_shrinks_the_art_to_fit() {
        let screen = vec2(400.0, 300.0);
        assert_eq!(
            pixel_view(WITH_MARGIN, screen, 20.0),
            fit_view(WITH_MARGIN, screen)
        );
    }
}
