//! Frames drawn from a 9-slice texture: corners kept at their size, edges and
//! centre stretched to fit.

use egui_macroquad::egui::{
    self, Color32, ColorImage, InnerResponse, Mesh, Rect, Shape, TextureHandle, TextureOptions,
    pos2,
};
use macroquad::Error;
use macroquad::texture::Image;

/// A 9-slice texture: an image whose `border` pixels on each side are its frame.
#[derive(Clone)]
pub struct NineSlice {
    pub texture: TextureHandle,
    /// In texture pixels, drawn one point each.
    pub border: f32,
}

impl NineSlice {
    /// Decodes `bytes` (PNG…) into an egui texture named `name`. Pixel art: no blur
    /// when scaled up.
    ///
    /// # Errors
    /// If the bytes are not an image.
    pub fn load(ui: &egui::Context, name: &str, bytes: &[u8], border: f32) -> Result<Self, Error> {
        let image = Image::from_file_with_format(bytes, None)?;
        let size = [usize::from(image.width), usize::from(image.height)];
        let image = ColorImage::from_rgba_unmultiplied(size, &image.bytes);
        let texture = ui.load_texture(name, image, TextureOptions::NEAREST);
        Ok(Self { texture, border })
    }

    /// The frame filling `rect`, as one shape.
    pub fn shape(&self, rect: Rect) -> Shape {
        let size = self.texture.size_vec2();
        // Three columns and rows: the border, the middle, the border.
        let cuts = |from: f32, to: f32, length: f32| {
            let border = self.border.min((to - from) / 2.0);
            let uv = self.border / length;
            [
                (from, from + border, 0.0, uv),
                (from + border, to - border, uv, 1.0 - uv),
                (to - border, to, 1.0 - uv, 1.0),
            ]
        };
        let mut mesh = Mesh::with_texture(self.texture.id());
        for (left, right, u0, u1) in cuts(rect.left(), rect.right(), size.x) {
            for (top, bottom, v0, v1) in cuts(rect.top(), rect.bottom(), size.y) {
                mesh.add_rect_with_uv(
                    Rect::from_min_max(pos2(left, top), pos2(right, bottom)),
                    Rect::from_min_max(pos2(u0, v0), pos2(u1, v1)),
                    Color32::WHITE,
                );
            }
        }
        Shape::mesh(mesh)
    }

    /// `add`'s contents inside the frame, `border` from its edges.
    pub fn show<R>(
        &self,
        ui: &mut egui::Ui,
        add: impl FnOnce(&mut egui::Ui) -> R,
    ) -> InnerResponse<R> {
        // Reserved first so that the frame is painted under the contents.
        let slot = ui.painter().add(Shape::Noop);
        let inner = egui::Frame::new().inner_margin(self.border).show(ui, add);
        ui.painter().set(slot, self.shape(inner.response.rect));
        inner
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_corners_keep_their_size_and_the_middle_stretches() {
        let ui = egui::Context::default();
        let image = ColorImage::new([24, 24], Color32::WHITE);
        let frame = NineSlice {
            texture: ui.load_texture("frame", image, TextureOptions::NEAREST),
            border: 8.0,
        };
        let Shape::Mesh(mesh) = frame.shape(Rect::from_min_max(pos2(0.0, 0.0), pos2(100.0, 40.0)))
        else {
            panic!("not a mesh");
        };
        // 9 rectangles of 4 vertices; the first is the top left corner.
        assert_eq!(mesh.vertices.len(), 36);
        let corner =
            Rect::from_points(&mesh.vertices[..4].iter().map(|v| v.pos).collect::<Vec<_>>());
        assert_eq!(corner, Rect::from_min_max(pos2(0.0, 0.0), pos2(8.0, 8.0)));
        let uv = Rect::from_points(&mesh.vertices[..4].iter().map(|v| v.uv).collect::<Vec<_>>());
        assert_eq!(uv.max, pos2(1.0 / 3.0, 1.0 / 3.0));
        // The middle stretches between the borders.
        let middle = Rect::from_points(
            &mesh.vertices[16..20]
                .iter()
                .map(|v| v.pos)
                .collect::<Vec<_>>(),
        );
        assert_eq!(middle, Rect::from_min_max(pos2(8.0, 8.0), pos2(92.0, 32.0)));
    }
}
