//! Convex shape clipping of tessellated paint, including native text-edit paint.
use crate::model::{Background, Corners};
use eframe::egui::{
    epaint::{Mesh, Primitive, Vertex},
    *,
};
pub fn outline(r: Rect, corners: &Corners) -> Vec<Pos2> {
    let (rx, ry) = if corners.kind == "ellipse" {
        (r.width() / 2., r.height() / 2.)
    } else {
        let v = corners.radius.unwrap_or(0.) as f32;
        let v = v.min(r.width() / 2.).min(r.height() / 2.);
        (v, v)
    };
    if rx <= 0. || ry <= 0. {
        return vec![
            r.left_top(),
            r.right_top(),
            r.right_bottom(),
            r.left_bottom(),
        ];
    }
    let mut points = vec![];
    for (center, start) in [
        (pos2(r.right() - rx, r.top() + ry), -90.),
        (pos2(r.right() - rx, r.bottom() - ry), 0.),
        (pos2(r.left() + rx, r.bottom() - ry), 90.),
        (pos2(r.left() + rx, r.top() + ry), 180.),
    ] {
        for i in 0..=16 {
            let t = (start + i as f32 * 90. / 16.).to_radians();
            points.push(center + vec2(rx * t.cos(), ry * t.sin()));
        }
    }
    points
}
fn cross(a: Vec2, b: Vec2) -> f32 {
    a.x * b.y - a.y * b.x
}
fn mix(a: Vertex, b: Vertex, t: f32) -> Vertex {
    let ac = a.color.to_array();
    let bc = b.color.to_array();
    let mut c = [0; 4];
    for i in 0..4 {
        c[i] = (ac[i] as f32 + (bc[i] as f32 - ac[i] as f32) * t).round() as u8;
    }
    Vertex {
        pos: a.pos + (b.pos - a.pos) * t,
        uv: a.uv + (b.uv - a.uv) * t,
        color: Color32::from_rgba_premultiplied(c[0], c[1], c[2], c[3]),
    }
}
pub fn clip(mesh: &Mesh, polygon: &[Pos2]) -> Mesh {
    let mut out = Mesh::with_texture(mesh.texture_id);
    for tri in mesh.indices.as_chunks::<3>().0 {
        let mut p: Vec<_> = tri.iter().map(|i| mesh.vertices[*i as usize]).collect();
        for i in 0..polygon.len() {
            if p.is_empty() {
                break;
            }
            let a = polygon[i];
            let b = polygon[(i + 1) % polygon.len()];
            let mut next = vec![];
            let mut prev = *p.last().unwrap();
            let mut dp = cross(b - a, prev.pos - a);
            for v in p {
                let dv = cross(b - a, v.pos - a);
                if (dv >= 0.) != (dp >= 0.) {
                    next.push(mix(prev, v, dp / (dp - dv)));
                }
                if dv >= 0. {
                    next.push(v);
                }
                prev = v;
                dp = dv;
            }
            p = next;
        }
        if p.len() >= 3 {
            let base = out.vertices.len() as u32;
            for i in 1..p.len() - 1 {
                out.indices
                    .extend([base, base + i as u32, base + i as u32 + 1]);
            }
            out.vertices.extend(p);
        }
    }
    out
}
pub fn contains(p: Pos2, polygon: &[Pos2]) -> bool {
    (0..polygon.len()).all(|i| {
        cross(
            polygon[(i + 1) % polygon.len()] - polygon[i],
            p - polygon[i],
        ) >= -0.01
    })
}
pub fn clip_paint(ui: &Ui, start: usize, polygons: &[Vec<Pos2>]) {
    if polygons.is_empty() {
        return;
    }
    let layer = ui.layer_id();
    let shapes = ui.ctx().graphics_mut(|g| {
        let l = g.entry(layer);
        let shapes = l.all_entries().skip(start).cloned().collect::<Vec<_>>();
        for i in start..start + shapes.len() {
            l.reset_shape(eframe::egui::layers::ShapeIdx(i));
        }
        shapes
    });
    let primitives = ui.ctx().tessellate(shapes, ui.ctx().pixels_per_point());
    for p in primitives {
        if let Primitive::Mesh(mut m) = p.primitive {
            for polygon in polygons {
                m = clip(&m, polygon);
            }
            ui.painter().with_clip_rect(p.clip_rect).add(Shape::mesh(m));
        }
    }
}
pub fn gradient(r: Rect, b: &Background) -> Mesh {
    let angle = b.angle_degrees.unwrap().to_radians() as f32;
    let direction = vec2(angle.sin(), -angle.cos());
    let side = vec2(-direction.y, direction.x);
    let length = r.width() * direction.x.abs() + r.height() * direction.y.abs();
    let half = r.width() + r.height();
    let mut stops = vec![(0., crate::painter::color(&b.stops[0].color))];
    stops.extend(
        b.stops
            .iter()
            .map(|s| (s.offset as f32, crate::painter::color(&s.color))),
    );
    stops.push((1., crate::painter::color(&b.stops.last().unwrap().color)));
    let mut mesh = Mesh::default();
    for pair in stops.windows(2) {
        let [(a, ca), (z, cz)] = [pair[0], pair[1]];
        if z <= a {
            continue;
        }
        let i = mesh.vertices.len() as u32;
        for (t, c, sign) in [(a, ca, -1.), (a, ca, 1.), (z, cz, 1.), (z, cz, -1.)] {
            mesh.vertices.push(Vertex {
                pos: r.center() + direction * ((t - 0.5) * length) + side * half * sign,
                uv: epaint::WHITE_UV,
                color: c,
            });
        }
        mesh.indices.extend([i, i + 1, i + 2, i, i + 2, i + 3]);
    }
    clip(
        &mesh,
        &[
            r.left_top(),
            r.right_top(),
            r.right_bottom(),
            r.left_bottom(),
        ],
    )
}
