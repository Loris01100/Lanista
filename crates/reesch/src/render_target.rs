//! Drawing a scene into a texture instead of the screen, then showing that texture
//! anywhere on screen, at any size: a picture in the picture.

use macroquad::camera::{Camera2D, set_camera, set_default_camera};
use macroquad::color::Color;
use macroquad::math::{Rect, Vec2, vec2};
use macroquad::texture::{
    self, DrawTextureParams, FilterMode, RenderTarget, Texture2D, draw_texture_ex,
};
use macroquad::window::clear_background;

use crate::camera::fit_view;

/// A texture of `size` pixels to draw a scene into.
pub struct SceneTarget {
    target: RenderTarget,
    size: Vec2,
}

impl SceneTarget {
    /// A new target of `width` × `height` pixels, shown without blur.
    #[must_use]
    pub fn new(width: u32, height: u32) -> Self {
        let target = texture::render_target(width, height);
        target.texture.set_filter(FilterMode::Nearest);
        #[expect(clippy::cast_precision_loss, reason = "a texture's pixels")]
        let size = vec2(width as f32, height as f32);
        Self { target, size }
    }

    /// Its size, in pixels.
    #[must_use]
    pub fn size(&self) -> Vec2 {
        self.size
    }

    /// Draws a scene into the target: cleared to `background`, then `draw` with a
    /// camera showing at least `view` of the world, like `fit_camera` on screen. The
    /// screen's camera is back afterwards.
    pub fn draw(&self, view: Rect, background: Color, draw: impl FnOnce()) {
        let mut camera = target_camera(view, self.size);
        camera.render_target = Some(self.target.clone());
        set_camera(&camera);
        clear_background(background);
        draw();
        set_default_camera();
    }

    /// The scene last drawn, shown on screen in `rect` (in pixels, the screen's camera
    /// set) and multiplied by `color`.
    pub fn show(&self, rect: Rect, color: Color) {
        draw_texture_ex(
            self.texture(),
            rect.x,
            rect.y,
            color,
            DrawTextureParams {
                dest_size: Some(vec2(rect.w, rect.h)),
                // A target's texture is stored upside down.
                flip_y: true,
                ..Default::default()
            },
        );
    }

    #[must_use]
    pub fn texture(&self) -> &Texture2D {
        &self.target.texture
    }
}

/// The camera that draws at least `view` of the world into a target of `size` pixels,
/// the world's y going down as on screen: the extra space shows more of the world.
#[must_use]
pub fn target_camera(view: Rect, size: Vec2) -> Camera2D {
    // Drawn to the screen with `flip_y`, the target's y-up camera shows y going down.
    Camera2D::from_display_rect(fit_view(view, size))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_thumbnail_camera_fits_the_view_into_the_target() {
        let arena = Rect::new(0.0, 0.0, 20.0, 20.0);
        // A 16:10 thumbnail of a square arena: wider on both sides, as on screen.
        let camera = target_camera(arena, vec2(320.0, 200.0));
        assert_eq!(camera.target, vec2(10.0, 10.0));
        let shown = fit_view(arena, vec2(320.0, 200.0));
        assert_eq!(shown, Rect::new(-6.0, 0.0, 32.0, 20.0));
        assert!((camera.zoom.x - 2.0 / 32.0).abs() < 1e-6);
        assert!((camera.zoom.y + 2.0 / 20.0).abs() < 1e-6);
    }
}
