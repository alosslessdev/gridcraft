//! Painting a cell gradient with egui's mesh painter.

use egui::{Color32, Mesh, Painter, Rect, pos2};
use gridcraft_engine::model::{GradientFill, Theme};

/// Paints `gradient` across `rect`.
///
/// The mesh is sampled at a fixed grid. A flat triangle pair cannot represent a multi-stop
/// gradient, and egui has no gradient shader, so a 2-D grid is the smallest form that works for
/// both linear and path gradients.
pub fn paint_rect(painter: &Painter, rect: Rect, gradient: &GradientFill, theme: &Theme) {
    if !(rect.is_finite() && rect.is_positive()) {
        return;
    }
    let (cols, rows) = (16, 16);
    let mut mesh = Mesh::default();
    for y in 0..=rows {
        for x in 0..=cols {
            let u = x as f32 / cols as f32;
            let v = y as f32 / rows as f32;
            let Some(rgb) = gradient.sample(u as f64, v as f64, theme) else { return };
            let pos = pos2(rect.left() + rect.width() * u, rect.top() + rect.height() * v);
            mesh.colored_vertex(pos, Color32::from_rgb(rgb[0], rgb[1], rgb[2]));
        }
    }
    for y in 0..rows {
        for x in 0..cols {
            let row = cols + 1;
            let i = y * row + x;
            mesh.add_triangle(i, i + 1, i + row);
            mesh.add_triangle(i + 1, i + row + 1, i + row);
        }
    }
    painter.add(mesh);
}
