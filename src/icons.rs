//! Toolbar icons drawn with simple shapes.

use egui::{pos2, vec2, Color32, Painter, Pos2, Rect, Shape, Stroke};

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Icon {
    Play,
    Pause,
    Stop,
    Prev,
    Next,
    Fullscreen,
    Extended,
    Playlist,
    LoopOff,
    LoopAll,
    LoopOne,
    Random,
    Speaker,
    SpeakerMuted,
    Frame,
    Snapshot,
    AbLoop,
}

pub fn draw(p: &Painter, r: Rect, icon: Icon, color: Color32) {
    let c = r.center();
    let s = r.width().min(r.height());
    let u = s / 16.0; // one unit on a 16 x 16 grid
    let at = |x: f32, y: f32| pos2(c.x + (x - 8.0) * u, c.y + (y - 8.0) * u);
    let stroke = Stroke::new((1.6 * u).max(1.2), color);
    match icon {
        Icon::Play => {
            p.add(Shape::convex_polygon(vec![at(4.0, 2.5), at(13.5, 8.0), at(4.0, 13.5)], color, Stroke::NONE));
        }
        Icon::Pause => {
            p.rect_filled(Rect::from_min_max(at(4.0, 3.0), at(7.0, 13.0)), 0.5 * u, color);
            p.rect_filled(Rect::from_min_max(at(9.0, 3.0), at(12.0, 13.0)), 0.5 * u, color);
        }
        Icon::Stop => {
            p.rect_filled(Rect::from_min_max(at(4.0, 4.0), at(12.0, 12.0)), 0.6 * u, color);
        }
        Icon::Prev => {
            p.rect_filled(Rect::from_min_max(at(3.0, 4.0), at(4.8, 12.0)), 0.0, color);
            p.add(Shape::convex_polygon(vec![at(4.8, 8.0), at(9.0, 4.0), at(9.0, 12.0)], color, Stroke::NONE));
            p.add(Shape::convex_polygon(vec![at(8.8, 8.0), at(13.0, 4.0), at(13.0, 12.0)], color, Stroke::NONE));
        }
        Icon::Next => {
            p.add(Shape::convex_polygon(vec![at(3.0, 4.0), at(7.2, 8.0), at(3.0, 12.0)], color, Stroke::NONE));
            p.add(Shape::convex_polygon(vec![at(7.0, 4.0), at(11.2, 8.0), at(7.0, 12.0)], color, Stroke::NONE));
            p.rect_filled(Rect::from_min_max(at(11.2, 4.0), at(13.0, 12.0)), 0.0, color);
        }
        Icon::Fullscreen => {
            let l = 3.5;
            for (x, y, dx, dy) in [(2.5, 2.5, 1.0, 1.0), (13.5, 2.5, -1.0, 1.0), (2.5, 13.5, 1.0, -1.0), (13.5, 13.5, -1.0, -1.0)] {
                p.line_segment([at(x, y), at(x + dx * l, y)], stroke);
                p.line_segment([at(x, y), at(x, y + dy * l)], stroke);
            }
            p.rect_stroke(Rect::from_min_max(at(5.5, 5.5), at(10.5, 10.5)), 0.5 * u, Stroke::new(stroke.width * 0.8, color), egui::StrokeKind::Middle);
        }
        Icon::Extended => {
            // three sliders
            for (i, knob) in [(0, 10.5), (1, 5.0), (2, 8.5)] {
                let y = 4.0 + i as f32 * 4.0;
                p.line_segment([at(2.5, y), at(13.5, y)], Stroke::new(stroke.width * 0.8, color));
                p.circle_filled(at(knob, y), 1.8 * u, color);
            }
        }
        Icon::Playlist => {
            for i in 0..3 {
                let y = 4.0 + i as f32 * 4.0;
                p.circle_filled(at(3.0, y), 1.1 * u, color);
                p.line_segment([at(5.5, y), at(13.5, y)], stroke);
            }
        }
        Icon::LoopOff | Icon::LoopAll | Icon::LoopOne => {
            loop_arrows(p, &at, u, stroke, color);
            if icon == Icon::LoopOne {
                p.text(at(8.0, 8.2), egui::Align2::CENTER_CENTER, "1", egui::FontId::proportional(7.5 * u), color);
            }
        }
        Icon::Random => {
            let pts1 = curve(at(2.0, 4.5), at(7.0, 4.5), at(9.0, 11.5), at(12.5, 11.5));
            let pts2 = curve(at(2.0, 11.5), at(7.0, 11.5), at(9.0, 4.5), at(12.5, 4.5));
            p.add(Shape::line(pts1, stroke));
            p.add(Shape::line(pts2, stroke));
            p.add(Shape::convex_polygon(vec![at(12.0, 2.3), at(14.6, 4.5), at(12.0, 6.7)], color, Stroke::NONE));
            p.add(Shape::convex_polygon(vec![at(12.0, 9.3), at(14.6, 11.5), at(12.0, 13.7)], color, Stroke::NONE));
        }
        Icon::Speaker | Icon::SpeakerMuted => {
            p.add(Shape::convex_polygon(
                vec![at(2.0, 6.0), at(4.5, 6.0), at(8.0, 3.0), at(8.0, 13.0), at(4.5, 10.0), at(2.0, 10.0)],
                color,
                Stroke::NONE,
            ));
            if icon == Icon::SpeakerMuted {
                p.line_segment([at(10.0, 5.5), at(14.0, 10.5)], stroke);
                p.line_segment([at(14.0, 5.5), at(10.0, 10.5)], stroke);
            } else {
                arc(p, at(8.0, 8.0), 3.2 * u, -0.9, 0.9, Stroke::new(stroke.width * 0.85, color));
                arc(p, at(8.0, 8.0), 5.8 * u, -0.9, 0.9, Stroke::new(stroke.width * 0.85, color));
            }
        }
        Icon::Frame => {
            p.rect_stroke(Rect::from_min_max(at(2.5, 3.5), at(13.5, 12.5)), 0.5 * u, stroke, egui::StrokeKind::Middle);
            p.add(Shape::convex_polygon(vec![at(6.0, 5.5), at(10.5, 8.0), at(6.0, 10.5)], color, Stroke::NONE));
        }
        Icon::AbLoop => {
            let f = egui::FontId::new(10.5 * u, egui::FontFamily::Proportional);
            p.text(at(3.0, 8.3), egui::Align2::CENTER_CENTER, "A", f.clone(), color);
            p.text(at(13.0, 8.3), egui::Align2::CENTER_CENTER, "B", f, color);
            p.line_segment([at(5.6, 8.0), at(9.6, 8.0)], Stroke::new(stroke.width * 0.9, color));
            p.add(Shape::convex_polygon(vec![at(9.2, 6.0), at(11.0, 8.0), at(9.2, 10.0)], color, Stroke::NONE));
        }
        Icon::Snapshot => {
            p.rect_stroke(Rect::from_min_max(at(2.0, 5.0), at(14.0, 13.0)), 1.0 * u, stroke, egui::StrokeKind::Middle);
            p.rect_filled(Rect::from_min_max(at(5.5, 3.0), at(10.5, 5.0)), 0.5 * u, color);
            p.circle_stroke(at(8.0, 9.0), 2.4 * u, stroke);
        }
    }
}

fn loop_arrows(p: &Painter, at: &dyn Fn(f32, f32) -> Pos2, u: f32, stroke: Stroke, color: Color32) {
    // a rounded rectangle path with two arrow heads
    let top = vec![at(4.5, 10.5), at(3.0, 10.5), at(2.5, 10.0), at(2.5, 5.5), at(3.5, 4.5), at(12.0, 4.5)];
    let bottom = vec![at(11.5, 5.5), at(13.0, 5.5), at(13.5, 6.0), at(13.5, 10.5), at(12.5, 11.5), at(4.0, 11.5)];
    p.add(Shape::line(top, stroke));
    p.add(Shape::line(bottom, stroke));
    p.add(Shape::convex_polygon(vec![at(11.0, 2.3), at(13.8, 4.5), at(11.0, 6.7)], color, Stroke::NONE));
    p.add(Shape::convex_polygon(vec![at(5.0, 9.3), at(2.2, 11.5), at(5.0, 13.7)], color, Stroke::NONE));
    let _ = u;
}

fn curve(a: Pos2, b: Pos2, c: Pos2, d: Pos2) -> Vec<Pos2> {
    (0..=16)
        .map(|i| {
            let t = i as f32 / 16.0;
            let mt = 1.0 - t;
            let w = [mt * mt * mt, 3.0 * mt * mt * t, 3.0 * mt * t * t, t * t * t];
            pos2(
                a.x * w[0] + b.x * w[1] + c.x * w[2] + d.x * w[3],
                a.y * w[0] + b.y * w[1] + c.y * w[2] + d.y * w[3],
            )
        })
        .collect()
}

fn arc(p: &Painter, center: Pos2, radius: f32, a0: f32, a1: f32, stroke: Stroke) {
    let pts: Vec<Pos2> = (0..=12)
        .map(|i| {
            let a = a0 + (a1 - a0) * i as f32 / 12.0;
            center + vec2(a.cos(), a.sin()) * radius
        })
        .collect();
    p.add(Shape::line(pts, stroke));
}

/// The idle-screen badge: an original rounded play badge with soft rings.
pub fn draw_logo(p: &Painter, center: Pos2, size: f32) {
    let r = size / 2.0;
    for i in 0..3 {
        let rr = r * (1.0 + 0.18 * (i + 1) as f32);
        let a = (40 - i * 12) as u8;
        p.circle_stroke(center, rr, Stroke::new(2.0_f32, Color32::from_rgba_unmultiplied(255, 140, 40, a)));
    }
    // gradient disc made of rings
    let steps = 24;
    for i in 0..steps {
        let t = i as f32 / steps as f32;
        let rr = r * (1.0 - t);
        let col = lerp_color(Color32::from_rgb(232, 96, 20), Color32::from_rgb(255, 176, 64), t);
        p.circle_filled(center, rr, col);
    }
    let u = size / 16.0;
    let tri = vec![
        pos2(center.x - 2.6 * u, center.y - 4.2 * u),
        pos2(center.x + 4.6 * u, center.y),
        pos2(center.x - 2.6 * u, center.y + 4.2 * u),
    ];
    p.add(Shape::convex_polygon(tri, Color32::WHITE, Stroke::NONE));
}

pub fn lerp_color(a: Color32, b: Color32, t: f32) -> Color32 {
    let t = t.clamp(0.0, 1.0);
    let l = |x: u8, y: u8| (x as f32 + (y as f32 - x as f32) * t).round() as u8;
    Color32::from_rgb(l(a.r(), b.r()), l(a.g(), b.g()), l(a.b(), b.b()))
}
