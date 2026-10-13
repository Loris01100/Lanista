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

/// Smallest area with the screen's aspect ratio that contains `view`, centred
/// on it: the extra space shows more of the world instead of black bars.
#[must_use]
pub fn fit_view(view: Rect, screen: Vec2) -> Rect {
    if screen.x <= 0.0 || screen.y <= 0.0 {
        return view; // minimised window
    }
    let scale = (screen.x / view.w).min(screen.y / view.h);
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
    }
}
