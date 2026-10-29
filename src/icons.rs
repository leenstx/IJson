// Icon definitions from Lucide (ISC): https://lucide.dev/icons/
// Outline icons, viewBox 0 0 24 24. Drawn as strokes so they stay sharp at toolbar size.
use eframe::egui::{self, Color32, Painter, Pos2, Rect, Shape, Stroke};

#[allow(dead_code)]
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum LucideIcon {
    Plus,
    WandSparkles,
    ArrowUpDown,
    ArrowDownAZ,
    ArrowDownZA,
    Undo,
    Redo,
    Minimize,
    FoldVertical,
    Eraser,
    Copy,
    Trash,
    Columns2,
    MoonStar,
    SunMoon,
    Sun,
    ChevronsUpDown,
    ChevronsDownUp,
    Search,
    Key,
    Braces,
    Type,
    Route,
    ChevronsDown,
    ChevronsUp,
    Pencil,
    X,
    ChevronRight,
    ChevronDown,
    Pin,
    PinOff,
    Bookmark,
    BookmarkOff,
    Download,
}

#[derive(Clone, Copy)]
pub(crate) enum LucideElem {
    Path(&'static str),
    Circle(f32, f32, f32),
    Rect(f32, f32, f32, f32, f32),
}

fn icon_elements(icon: LucideIcon) -> &'static [LucideElem] {
    match icon {
        LucideIcon::Plus => &[
            LucideElem::Path("M5 12h14"),
            LucideElem::Path("M12 5v14"),
        ],
        LucideIcon::WandSparkles => &[
            LucideElem::Path("m21.64 3.64-1.28-1.28a1.21 1.21 0 0 0-1.72 0L2.36 18.64a1.21 1.21 0 0 0 0 1.72l1.28 1.28a1.2 1.2 0 0 0 1.72 0L21.64 5.36a1.2 1.2 0 0 0 0-1.72"),
            LucideElem::Path("m14 7 3 3"),
            LucideElem::Path("M5 6v4"),
            LucideElem::Path("M19 14v4"),
            LucideElem::Path("M10 2v2"),
            LucideElem::Path("M7 8H3"),
            LucideElem::Path("M21 16h-4"),
            LucideElem::Path("M11 3H9"),
        ],
        LucideIcon::ArrowUpDown => &[
            LucideElem::Path("m21 16-4 4-4-4"),
            LucideElem::Path("M17 20V4"),
            LucideElem::Path("m3 8 4-4 4 4"),
            LucideElem::Path("M7 4v16"),
        ],
        LucideIcon::ArrowDownAZ => &[
            LucideElem::Path("m3 16 4 4 4-4"),
            LucideElem::Path("M7 20V4"),
            LucideElem::Path("M20 8h-5"),
            LucideElem::Path("M15 10V6.5a2.5 2.5 0 0 1 5 0V10"),
            LucideElem::Path("M15 14h5l-5 6h5"),
        ],
        LucideIcon::ArrowDownZA => &[
            LucideElem::Path("m3 16 4 4 4-4"),
            LucideElem::Path("M7 4v16"),
            LucideElem::Path("M15 4h5l-5 6h5"),
            LucideElem::Path("M15 20v-3.5a2.5 2.5 0 0 1 5 0V20"),
            LucideElem::Path("M20 18h-5"),
        ],
        LucideIcon::Undo => &[
            LucideElem::Path("M9 14 4 9l5-5"),
            LucideElem::Path("M4 9h10.5a5.5 5.5 0 0 1 5.5 5.5a5.5 5.5 0 0 1-5.5 5.5H11"),
        ],
        LucideIcon::Redo => &[
            LucideElem::Path("m15 14 5-5-5-5"),
            LucideElem::Path("M20 9H9.5A5.5 5.5 0 0 0 4 14.5A5.5 5.5 0 0 0 9.5 20H13"),
        ],
        LucideIcon::Minimize => &[
            LucideElem::Path("m14 10 7-7"),
            LucideElem::Path("M20 10h-6V4"),
            LucideElem::Path("m3 21 7-7"),
            LucideElem::Path("M4 14h6v6"),
        ],
        LucideIcon::FoldVertical => &[
            LucideElem::Path("M12 22v-6"),
            LucideElem::Path("M12 8V2"),
            LucideElem::Path("M4 12H2"),
            LucideElem::Path("M10 12H8"),
            LucideElem::Path("M16 12h-2"),
            LucideElem::Path("M22 12h-2"),
            LucideElem::Path("m15 19-3-3-3 3"),
            LucideElem::Path("m15 5-3 3-3-3"),
        ],
        LucideIcon::Eraser => &[
            LucideElem::Path("M21 21H8a2 2 0 0 1-1.42-.587l-3.994-3.999a2 2 0 0 1 0-2.828l10-10a2 2 0 0 1 2.829 0l5.999 6a2 2 0 0 1 0 2.828L12.834 21"),
            LucideElem::Path("m5.082 11.09 8.828 8.828"),
        ],
        LucideIcon::Copy => &[
            LucideElem::Rect(8.0, 8.0, 14.0, 14.0, 2.0),
            LucideElem::Path("M4 16c-1.1 0-2-.9-2-2V4c0-1.1.9-2 2-2h10c1.1 0 2 .9 2 2"),
        ],
        LucideIcon::Trash => &[
            LucideElem::Path("M10 11v6"),
            LucideElem::Path("M14 11v6"),
            LucideElem::Path("M19 6v14a2 2 0 0 1-2 2H7a2 2 0 0 1-2-2V6"),
            LucideElem::Path("M3 6h18"),
            LucideElem::Path("M8 6V4a2 2 0 0 1 2-2h4a2 2 0 0 1 2 2v2"),
        ],
        LucideIcon::MoonStar => &[
            LucideElem::Path("M18 5h4"),
            LucideElem::Path("M20 3v4"),
            LucideElem::Path("M20.985 12.486a9 9 0 1 1-9.473-9.472c.405-.022.617.46.402.803a6 6 0 0 0 8.268 8.268c.344-.215.825-.004.803.401"),
        ],
        LucideIcon::SunMoon => &[
            LucideElem::Path("M12 2v2"),
            LucideElem::Path("M14.837 16.385a6 6 0 1 1-7.223-7.222c.624-.147.97.66.715 1.248a4 4 0 0 0 5.26 5.259c.589-.255 1.396.09 1.248.715"),
            LucideElem::Path("M16 12a4 4 0 0 0-4-4"),
            LucideElem::Path("m19 5-1.256 1.256"),
            LucideElem::Path("M20 12h2"),
        ],
        LucideIcon::Sun => &[
            LucideElem::Circle(12.0, 12.0, 4.0),
            LucideElem::Path("M12 2v2"),
            LucideElem::Path("M12 20v2"),
            LucideElem::Path("m4.93 4.93 1.41 1.41"),
            LucideElem::Path("m17.66 17.66 1.41 1.41"),
            LucideElem::Path("M2 12h2"),
            LucideElem::Path("M20 12h2"),
            LucideElem::Path("m6.34 17.66-1.41 1.41"),
            LucideElem::Path("m19.07 4.93-1.41 1.41"),
        ],
        LucideIcon::Columns2 => &[
            LucideElem::Rect(3.0, 3.0, 18.0, 18.0, 2.0),
            LucideElem::Path("M12 3v18"),
        ],
        LucideIcon::ChevronsUpDown => &[
            LucideElem::Path("m7 15 5 5 5-5"),
            LucideElem::Path("m7 9 5-5 5 5"),
        ],
        LucideIcon::ChevronsDownUp => &[
            LucideElem::Path("m7 20 5-5 5 5"),
            LucideElem::Path("m7 4 5 5 5-5"),
        ],
        LucideIcon::Search => &[
            LucideElem::Path("m21 21-4.34-4.34"),
            LucideElem::Circle(11.0, 11.0, 8.0),
        ],
        LucideIcon::Key => &[
            LucideElem::Path("m2 21 9.6-9.6"),
            LucideElem::Path("m7.5 15.5 2.3 2.3a1 1 0 0 1 0 1.4l-2.1 2.1a1 1 0 0 1-1.4 0L4 19"),
            LucideElem::Circle(15.5, 7.5, 5.5),
        ],
        LucideIcon::Braces => &[
            LucideElem::Path("M8 3H7a2 2 0 0 0-2 2v5a2 2 0 0 1-2 2 2 2 0 0 1 2 2v5c0 1.1.9 2 2 2h1"),
            LucideElem::Path("M16 21h1a2 2 0 0 0 2-2v-5c0-1.1.9-2 2-2a2 2 0 0 1-2-2V5a2 2 0 0 0-2-2h-1"),
        ],
        LucideIcon::Type => &[
            LucideElem::Path("M12 4v16"),
            LucideElem::Path("M4 7V5a1 1 0 0 1 1-1h14a1 1 0 0 1 1 1v2"),
            LucideElem::Path("M9 20h6"),
        ],
        LucideIcon::Route => &[
            LucideElem::Circle(6.0, 19.0, 3.0),
            LucideElem::Path("M9 19h8.5a3.5 3.5 0 0 0 0-7h-11a3.5 3.5 0 0 1 0-7H15"),
            LucideElem::Circle(18.0, 5.0, 3.0),
        ],
        LucideIcon::ChevronsDown => &[
            LucideElem::Path("m7 6 5 5 5-5"),
            LucideElem::Path("m7 13 5 5 5-5"),
        ],
        LucideIcon::ChevronsUp => &[
            LucideElem::Path("m17 11-5-5-5 5"),
            LucideElem::Path("m17 18-5-5-5 5"),
        ],
        LucideIcon::Pencil => &[
            LucideElem::Path("M21.174 6.812a1 1 0 0 0-3.986-3.987L3.842 16.174a2 2 0 0 0-.5.83l-1.321 4.352a.5.5 0 0 0 .623.622l4.353-1.32a2 2 0 0 0 .83-.497z"),
            LucideElem::Path("m15 5 4 4"),
        ],
        LucideIcon::X => &[
            LucideElem::Path("M18 6 6 18"),
            LucideElem::Path("m6 6 12 12"),
        ],
        LucideIcon::ChevronRight => &[
            LucideElem::Path("m9 18 6-6-6-6"),
        ],
        LucideIcon::ChevronDown => &[
            LucideElem::Path("m6 9 6 6 6-6"),
        ],
        LucideIcon::Pin => &[
            LucideElem::Path("M12 17v5"),
            LucideElem::Path("M9 10.76a2 2 0 0 1-1.11 1.79l-1.78.9A2 2 0 0 0 5 15.24V16a1 1 0 0 0 1 1h12a1 1 0 0 0 1-1v-.76a2 2 0 0 0-1.11-1.79l-1.78-.9A2 2 0 0 1 15 10.76V7a1 1 0 0 1 1-1 2 2 0 0 0 0-4H8a2 2 0 0 0 0 4 1 1 0 0 1 1 1z"),
        ],
        LucideIcon::PinOff => &[
            LucideElem::Path("M12 17v5"),
            LucideElem::Path("M15 9.34V7a1 1 0 0 1 1-1 2 2 0 0 0 0-4H7.89"),
            LucideElem::Path("m2 2 20 20"),
            LucideElem::Path("M9 9v1.76a2 2 0 0 1-1.11 1.79l-1.78.9A2 2 0 0 0 5 15.24V16a1 1 0 0 0 1 1h11"),
        ],
        LucideIcon::Bookmark => &[
            LucideElem::Path("M17 3a2 2 0 0 1 2 2v15a1 1 0 0 1-1.496 0.868l-4.512-2.578a2 2 0 0 0-1.984 0l-4.512 2.578A1 1 0 0 1 5 20V5a2 2 0 0 1 2-2z"),
        ],
        LucideIcon::BookmarkOff => &[
            LucideElem::Path("M19 19v1a1 1 0 0 1-1.496 0.868l-4.512-2.578a2 2 0 0 0-1.984 0l-4.512 2.578A1 1 0 0 1 5 20V5"),
            LucideElem::Path("m2 2 20 20"),
            LucideElem::Path("M8.656 3H17a2 2 0 0 1 2 2v8.344"),
        ],
        LucideIcon::Download => &[
            LucideElem::Path("M21 15v4a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2v-4"),
            LucideElem::Path("m7 10 5 5 5-5"),
            LucideElem::Path("M12 15V3"),
        ],
    }
}

pub(crate) fn draw_lucide_icon(painter: &Painter, rect: Rect, icon: LucideIcon, color: Color32) {
    let size = rect.width().min(rect.height());
    if size < 2.0 {
        return;
    }
    let origin = rect.center() - egui::vec2(size, size) * 0.5;
    let scale = size / 24.0;
    // Lucide default stroke is 2px inside a 24px viewBox.
    let stroke = Stroke::new((2.0 * scale).max(1.15), color);
    for elem in icon_elements(icon) {
        match *elem {
            LucideElem::Path(d) => {
                for (points, closed) in flatten_path(d, origin, scale) {
                    if points.len() < 2 {
                        continue;
                    }
                    if closed {
                        painter.add(Shape::closed_line(points, stroke));
                    } else {
                        painter.line(points, stroke);
                    }
                }
            }
            LucideElem::Circle(cx, cy, r) => {
                let center = map_pt(cx, cy, origin, scale);
                painter.circle_stroke(center, r * scale, stroke);
            }
            LucideElem::Rect(x, y, w, h, rx) => {
                let min = map_pt(x, y, origin, scale);
                let rect = Rect::from_min_size(min, egui::vec2(w * scale, h * scale));
                let radius = (rx * scale).round().max(0.0) as u8;
                painter.rect_stroke(
                    rect,
                    egui::CornerRadius::same(radius),
                    stroke,
                    egui::StrokeKind::Middle,
                );
            }
        }
    }
}

fn flatten_path(d: &str, origin: Pos2, scale: f32) -> Vec<(Vec<Pos2>, bool)> {
    let bytes = d.as_bytes();
    let mut i = 0;
    let mut cmd = b'M';
    let mut cx = 0.0;
    let mut cy = 0.0;
    let mut sx = 0.0;
    let mut sy = 0.0;
    let mut sub = Vec::new();
    let mut closed = false;
    let mut out = Vec::new();

    let finish = |sub: &mut Vec<Pos2>, closed: &mut bool, out: &mut Vec<(Vec<Pos2>, bool)>| {
        if sub.len() >= 2 {
            out.push((std::mem::take(sub), *closed));
        } else {
            sub.clear();
        }
        *closed = false;
    };

    while i < bytes.len() {
        while i < bytes.len() && (bytes[i].is_ascii_whitespace() || bytes[i] == b',') {
            i += 1;
        }
        if i >= bytes.len() {
            break;
        }
        if bytes[i].is_ascii_alphabetic() {
            cmd = bytes[i];
            i += 1;
            if cmd == b'Z' || cmd == b'z' {
                if let Some(first) = sub.first().copied() {
                    sub.push(first);
                }
                closed = true;
                finish(&mut sub, &mut closed, &mut out);
                cx = sx;
                cy = sy;
                continue;
            }
        }
        let rel = cmd.is_ascii_lowercase();
        match cmd.to_ascii_uppercase() {
            b'M' => {
                if !sub.is_empty() {
                    finish(&mut sub, &mut closed, &mut out);
                }
                let (x, y) = (read_num(bytes, &mut i), read_num(bytes, &mut i));
                if rel {
                    cx += x;
                    cy += y;
                } else {
                    cx = x;
                    cy = y;
                }
                sx = cx;
                sy = cy;
                sub.push(map_pt(cx, cy, origin, scale));
                cmd = if rel { b'l' } else { b'L' };
            }
            b'L' => {
                let (x, y) = (read_num(bytes, &mut i), read_num(bytes, &mut i));
                if rel {
                    cx += x;
                    cy += y;
                } else {
                    cx = x;
                    cy = y;
                }
                sub.push(map_pt(cx, cy, origin, scale));
            }
            b'H' => {
                let x = read_num(bytes, &mut i);
                cx = if rel { cx + x } else { x };
                sub.push(map_pt(cx, cy, origin, scale));
            }
            b'V' => {
                let y = read_num(bytes, &mut i);
                cy = if rel { cy + y } else { y };
                sub.push(map_pt(cx, cy, origin, scale));
            }
            b'C' => {
                let x1 = read_num(bytes, &mut i);
                let y1 = read_num(bytes, &mut i);
                let x2 = read_num(bytes, &mut i);
                let y2 = read_num(bytes, &mut i);
                let x = read_num(bytes, &mut i);
                let y = read_num(bytes, &mut i);
                let (mut p1x, mut p1y, mut p2x, mut p2y, mut px, mut py) = (x1, y1, x2, y2, x, y);
                if rel {
                    p1x += cx;
                    p1y += cy;
                    p2x += cx;
                    p2y += cy;
                    px += cx;
                    py += cy;
                }
                flatten_cubic(cx, cy, p1x, p1y, p2x, p2y, px, py, origin, scale, &mut sub);
                cx = px;
                cy = py;
            }
            b'A' => {
                let rx = read_num(bytes, &mut i);
                let ry = read_num(bytes, &mut i);
                let angle_deg = read_num(bytes, &mut i);
                let large_arc = read_num(bytes, &mut i) != 0.0;
                let sweep = read_num(bytes, &mut i) != 0.0;
                let x = read_num(bytes, &mut i);
                let y = read_num(bytes, &mut i);
                let (px, py) = if rel { (cx + x, cy + y) } else { (x, y) };
                flatten_arc(
                    cx, cy, rx, ry, angle_deg, large_arc, sweep, px, py, origin, scale, &mut sub,
                );
                cx = px;
                cy = py;
            }
            _ => break,
        }
    }
    finish(&mut sub, &mut closed, &mut out);
    out
}

fn map_pt(x: f32, y: f32, origin: Pos2, scale: f32) -> Pos2 {
    egui::pos2(origin.x + x * scale, origin.y + y * scale)
}

fn flatten_cubic(
    x0: f32,
    y0: f32,
    x1: f32,
    y1: f32,
    x2: f32,
    y2: f32,
    x3: f32,
    y3: f32,
    origin: Pos2,
    scale: f32,
    out: &mut Vec<Pos2>,
) {
    const STEPS: i32 = 8;
    for step in 1..=STEPS {
        let t = step as f32 / STEPS as f32;
        let u = 1.0 - t;
        let x = u * u * u * x0 + 3.0 * u * u * t * x1 + 3.0 * u * t * t * x2 + t * t * t * x3;
        let y = u * u * u * y0 + 3.0 * u * u * t * y1 + 3.0 * u * t * t * y2 + t * t * t * y3;
        out.push(map_pt(x, y, origin, scale));
    }
}

fn flatten_arc(
    x0: f32,
    y0: f32,
    mut rx: f32,
    mut ry: f32,
    angle_deg: f32,
    large_arc: bool,
    sweep: bool,
    x: f32,
    y: f32,
    origin: Pos2,
    scale: f32,
    out: &mut Vec<Pos2>,
) {
    if rx == 0.0 || ry == 0.0 {
        out.push(map_pt(x, y, origin, scale));
        return;
    }
    rx = rx.abs();
    ry = ry.abs();
    let phi = angle_deg.to_radians();
    let cos_phi = phi.cos();
    let sin_phi = phi.sin();

    let dx = (x0 - x) * 0.5;
    let dy = (y0 - y) * 0.5;
    let x1p = cos_phi * dx + sin_phi * dy;
    let y1p = -sin_phi * dx + cos_phi * dy;

    let cr = (x1p * x1p) / (rx * rx) + (y1p * y1p) / (ry * ry);
    if cr > 1.0 {
        let s = cr.sqrt();
        rx *= s;
        ry *= s;
    }

    let sign = if large_arc == sweep { -1.0 } else { 1.0 };
    let num = (rx * rx * ry * ry - rx * rx * y1p * y1p - ry * ry * x1p * x1p).max(0.0);
    let den = rx * rx * y1p * y1p + ry * ry * x1p * x1p;
    let coef = if den == 0.0 {
        0.0
    } else {
        sign * (num / den).sqrt()
    };

    let cxp = coef * (rx * y1p / ry);
    let cyp = coef * -(ry * x1p / rx);

    let cx_val = cos_phi * cxp - sin_phi * cyp + (x0 + x) * 0.5;
    let cy_val = sin_phi * cxp + cos_phi * cyp + (y0 + y) * 0.5;

    let vx1 = (x1p - cxp) / rx;
    let vy1 = (y1p - cyp) / ry;
    let vx2 = (-x1p - cxp) / rx;
    let vy2 = (-y1p - cyp) / ry;

    let angle_fn = |ux: f32, uy: f32, vx: f32, vy: f32| -> f32 {
        let dot = ux * vx + uy * vy;
        let len = (ux * ux + uy * uy).sqrt() * (vx * vx + vy * vy).sqrt();
        if len == 0.0 {
            return 0.0;
        }
        let cos_val = (dot / len).clamp(-1.0, 1.0);
        let mut a = cos_val.acos();
        if ux * vy - uy * vx < 0.0 {
            a = -a;
        }
        a
    };

    let theta1 = angle_fn(1.0, 0.0, vx1, vy1);
    let mut dtheta = angle_fn(vx1, vy1, vx2, vy2);
    if !sweep && dtheta > 0.0 {
        dtheta -= std::f32::consts::TAU;
    } else if sweep && dtheta < 0.0 {
        dtheta += std::f32::consts::TAU;
    }

    const STEPS: usize = 8;
    for step in 1..=STEPS {
        let th = theta1 + (dtheta * step as f32) / STEPS as f32;
        let px = rx * th.cos();
        let py = ry * th.sin();
        let fx = cos_phi * px - sin_phi * py + cx_val;
        let fy = sin_phi * px + cos_phi * py + cy_val;
        out.push(map_pt(fx, fy, origin, scale));
    }
}

fn read_num(bytes: &[u8], i: &mut usize) -> f32 {
    while *i < bytes.len() {
        let c = bytes[*i];
        if c.is_ascii_whitespace() || c == b',' {
            *i += 1;
            continue;
        }
        break;
    }
    let start = *i;
    if *i < bytes.len() && (bytes[*i] == b'+' || bytes[*i] == b'-') {
        *i += 1;
    }
    let mut saw_dot = false;
    while *i < bytes.len() {
        let c = bytes[*i];
        if c.is_ascii_digit() {
            *i += 1;
        } else if c == b'.' && !saw_dot {
            saw_dot = true;
            *i += 1;
        } else {
            break;
        }
    }
    if start == *i {
        *i += 1;
        return 0.0;
    }
    std::str::from_utf8(&bytes[start..*i])
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(0.0)
}
