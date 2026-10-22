//! Drawing of the controller with clickable buttons and live input.

use eframe::egui::{self, Align2, Color32, FontId, Pos2, Rect, Sense, Stroke, StrokeKind, Vec2};

use super::live::LiveState;
use super::theme::{ACCENT, BG, CARD, OK, TEXT_DIM, WARN};

/// Visual kind of a hotspot.
#[derive(Clone, Copy, PartialEq)]
enum Kind {
    Face(Color32),
    Small,
    Bumper,
    Stick,
    Dpad,
    Back,
    /// Front function key (on-board feature, not remappable).
    Func,
    Home,
}

/// Button hotspot: profile slot, centre (normalised, canvas 1.45:1), kind, label.
struct Spot {
    slot: usize,
    at: (f32, f32),
    kind: Kind,
    label: &'static str,
}

const FACE_A: Color32 = Color32::from_rgb(0x3d, 0xd6, 0x8c);
const FACE_B: Color32 = Color32::from_rgb(0xff, 0x5c, 0x6c);
const FACE_X: Color32 = Color32::from_rgb(0x4d, 0x8b, 0xff);
const FACE_Y: Color32 = Color32::from_rgb(0xff, 0xd8, 0x4d);

/// Layout traced from the official Rainbow 2 Pro product photo.
const SPOTS: &[Spot] = &[
    Spot {
        slot: 8,
        at: (0.22, 0.04),
        kind: Kind::Bumper,
        label: "LT",
    },
    Spot {
        slot: 6,
        at: (0.25, 0.12),
        kind: Kind::Bumper,
        label: "LB",
    },
    Spot {
        slot: 9,
        at: (0.78, 0.04),
        kind: Kind::Bumper,
        label: "RT",
    },
    Spot {
        slot: 7,
        at: (0.75, 0.12),
        kind: Kind::Bumper,
        label: "RB",
    },
    Spot {
        slot: 13,
        at: (0.24, 0.33),
        kind: Kind::Stick,
        label: "L3",
    },
    Spot {
        slot: 14,
        at: (0.63, 0.52),
        kind: Kind::Stick,
        label: "R3",
    },
    Spot {
        slot: 16,
        at: (0.37, 0.44),
        kind: Kind::Dpad,
        label: "▲",
    },
    Spot {
        slot: 17,
        at: (0.37, 0.60),
        kind: Kind::Dpad,
        label: "▼",
    },
    Spot {
        slot: 18,
        at: (0.32, 0.52),
        kind: Kind::Dpad,
        label: "◀",
    },
    Spot {
        slot: 19,
        at: (0.42, 0.52),
        kind: Kind::Dpad,
        label: "▶",
    },
    Spot {
        slot: 4,
        at: (0.73, 0.18),
        kind: Kind::Face(FACE_Y),
        label: "Y",
    },
    Spot {
        slot: 3,
        at: (0.67, 0.28),
        kind: Kind::Face(FACE_X),
        label: "X",
    },
    Spot {
        slot: 1,
        at: (0.79, 0.28),
        kind: Kind::Face(FACE_B),
        label: "B",
    },
    Spot {
        slot: 0,
        at: (0.73, 0.38),
        kind: Kind::Face(FACE_A),
        label: "A",
    },
    Spot {
        slot: 12,
        at: (0.50, 0.17),
        kind: Kind::Home,
        label: "⌂",
    },
    Spot {
        slot: 10,
        at: (0.44, 0.29),
        kind: Kind::Small,
        label: "View",
    },
    Spot {
        slot: 11,
        at: (0.56, 0.29),
        kind: Kind::Small,
        label: "Menu",
    },
    Spot {
        slot: 15,
        at: (0.50, 0.37),
        kind: Kind::Small,
        label: "Share",
    },
    Spot {
        slot: 30,
        at: (0.71, 0.63),
        kind: Kind::Func,
        label: "FN",
    },
    Spot {
        slot: 27,
        at: (0.405, 0.77),
        kind: Kind::Func,
        label: "Turbo",
    },
    Spot {
        slot: 29,
        at: (0.468, 0.77),
        kind: Kind::Func,
        label: "Map",
    },
    Spot {
        slot: 22,
        at: (0.532, 0.77),
        kind: Kind::Func,
        label: "Rec",
    },
    Spot {
        slot: 28,
        at: (0.595, 0.77),
        kind: Kind::Func,
        label: "Prof",
    },
    // M3/M4 sit on the top edge between the triggers (front view: M3 left,
    // M4 right, mirroring the manual's rear view "M4 … M3"; bits and
    // positions verified on hardware).
    Spot {
        slot: 25,
        at: (0.42, 0.05),
        kind: Kind::Back,
        label: "M3",
    },
    Spot {
        slot: 26,
        at: (0.58, 0.05),
        kind: Kind::Back,
        label: "M4",
    },
    Spot {
        slot: 23,
        at: (0.40, 0.95),
        kind: Kind::Back,
        label: "M1",
    },
    Spot {
        slot: 24,
        at: (0.60, 0.95),
        kind: Kind::Back,
        label: "M2",
    },
];

/// Per-button decoration supplied by the caller.
pub struct Marks {
    pub turbo: u32,
    pub remapped: u32,
    /// Light-strip brightness 0..=255 (None = unknown).
    pub strip: Option<u8>,
    /// Colour of the strip and Home light ("mode light": green in XInput,
    /// red in Switch mode, per the official manual).
    pub light: Color32,
    /// Custom strip colours (left, right) to preview instead of the mode light.
    pub strip_custom: Option<(Color32, Color32)>,
    /// Custom HOME logo colour (None = follows the mode colour).
    pub logo: Option<Color32>,
    /// Active on-board configuration 0..=3 (white LEDs), if known.
    pub profile: Option<usize>,
    /// Draw only the lights (Lights tab).
    pub lights_only: bool,
}

/// Draws the pad; returns the slot clicked this frame, if any.
pub fn pad(
    ui: &mut egui::Ui,
    selected: Option<usize>,
    marks: &Marks,
    live: &LiveState,
) -> Option<usize> {
    let w = ui.available_width().min(720.0);
    let (resp, painter) = ui.allocate_painter(Vec2::new(w, w / 1.45), Sense::click());
    let r = resp.rect;
    let at = |x: f32, y: f32| Pos2::new(r.left() + x * r.width(), r.top() + y * r.height());
    let s = r.width() / 640.0;

    // Light strip under the edge of the body, dimmed by the brightness.
    let alpha = f32::from(marks.strip.unwrap_or(255)) / 255.0;
    let (left_col, right_col) = marks.strip_custom.unwrap_or((marks.light, marks.light));
    let strip: Vec<Pos2> = [
        (0.14, 0.20),
        (0.09, 0.40),
        (0.10, 0.62),
        (0.20, 0.80),
        (0.36, 0.69),
        (0.50, 0.69),
        (0.64, 0.69),
        (0.80, 0.80),
        (0.90, 0.62),
        (0.91, 0.40),
        (0.86, 0.20),
    ]
    .iter()
    .map(|&(x, y)| at(x, y))
    .collect();
    if alpha > 0.0 {
        let (l, r) = strip.split_at(strip.len() / 2 + 1);
        painter.add(egui::Shape::line(
            l.to_vec(),
            Stroke::new(10.0 * s, left_col.gamma_multiply(alpha)),
        ));
        let mut r = r.to_vec();
        r.insert(0, l[l.len() - 1]);
        painter.add(egui::Shape::line(
            r,
            Stroke::new(10.0 * s, right_col.gamma_multiply(alpha)),
        ));
    }

    // Body: rounded top bridge and two grips.
    let body = Color32::from_rgb(0x2a, 0x2d, 0x38);
    painter.rect_filled(
        Rect::from_min_max(at(0.11, 0.10), at(0.89, 0.66)),
        70.0 * s,
        body,
    );
    painter.circle_filled(at(0.19, 0.66), 70.0 * s, body);
    painter.circle_filled(at(0.81, 0.66), 70.0 * s, body);
    painter.rect_filled(
        Rect::from_min_max(at(0.36, 0.70), at(0.64, 0.84)),
        10.0 * s,
        Color32::from_rgb(0x1f, 0x21, 0x29),
    );

    // Four white profile LEDs between the D-pad and the right stick.
    for i in 0..4 {
        let c = at(0.50, 0.46 + i as f32 * 0.045);
        let lit = marks.profile == Some(i);
        painter.rect_filled(
            Rect::from_center_size(c, Vec2::new(16.0, 5.0) * s),
            2.0,
            if lit {
                Color32::WHITE
            } else {
                Color32::from_gray(80)
            },
        );
    }
    painter.line_segment(
        [at(0.30, 0.885), at(0.70, 0.885)],
        Stroke::new(1.0, Color32::from_gray(60)),
    );
    painter.text(
        at(0.5, 0.885),
        Align2::CENTER_CENTER,
        " back ",
        FontId::proportional(10.0 * s.max(0.8)),
        TEXT_DIM,
    );

    let mut clicked = None;
    let pointer = resp.interact_pointer_pos();
    let hover = resp.hover_pos();
    for spot in SPOTS {
        if marks.lights_only && spot.kind != Kind::Home {
            continue;
        }
        let c = at(spot.at.0, spot.at.1);
        let (size, round) = match spot.kind {
            Kind::Face(_) => (Vec2::splat(34.0), 17.0),
            Kind::Small => (Vec2::new(46.0, 22.0), 11.0),
            Kind::Bumper => (Vec2::new(78.0, 24.0), 10.0),
            Kind::Stick => (Vec2::splat(70.0), 35.0),
            Kind::Dpad => (Vec2::splat(28.0), 6.0),
            Kind::Back => (Vec2::new(70.0, 26.0), 10.0),
            Kind::Func => (Vec2::new(38.0, 24.0), 6.0),
            Kind::Home => (Vec2::splat(34.0), 8.0),
        };
        let rect = Rect::from_center_size(c, size * s);
        let bit = 1u32 << spot.slot;
        let pressed = live.pressed & bit != 0;
        let is_sel = selected == Some(spot.slot);
        let hovered = hover.is_some_and(|p| rect.contains(p));
        if resp.clicked() && pointer.is_some_and(|p| rect.contains(p)) {
            clicked = Some(spot.slot);
        }
        let fill = match spot.kind {
            Kind::Face(col) if pressed => col,
            Kind::Face(_) => BG,
            _ if pressed => ACCENT,
            Kind::Home => marks.logo.unwrap_or(marks.light),
            Kind::Stick => Color32::from_rgb(0x1b, 0x1d, 0x24),
            Kind::Func => Color32::from_rgb(0x30, 0x33, 0x40),
            _ => CARD,
        };
        painter.rect_filled(rect, round * s, fill);
        let stroke = if is_sel {
            Stroke::new(2.5, Color32::WHITE)
        } else if hovered {
            Stroke::new(1.5, ACCENT)
        } else if let Kind::Face(col) = spot.kind {
            Stroke::new(1.5, col)
        } else if spot.slot == 30 {
            Stroke::new(1.5, FACE_B)
        } else {
            Stroke::new(1.0, Color32::from_gray(70))
        };
        painter.rect_stroke(rect, round * s, stroke, StrokeKind::Outside);

        // Live stick position inside the stick cap.
        if spot.kind == Kind::Stick {
            let (x, y) = if spot.slot == 13 {
                live.left
            } else {
                live.right
            };
            let dot = c + Vec2::new(x, -y) * (size.x * s * 0.35);
            painter.circle_filled(dot, 7.0 * s, if pressed { Color32::WHITE } else { ACCENT });
        }
        let text_col = match spot.kind {
            Kind::Face(col) if !pressed => col,
            _ => Color32::WHITE,
        };
        let label_pos = if spot.kind == Kind::Stick {
            c + Vec2::new(0.0, size.y * s * 0.62)
        } else {
            c
        };
        let font = if spot.kind == Kind::Func { 10.5 } else { 13.0 };
        painter.text(
            label_pos,
            Align2::CENTER_CENTER,
            spot.label,
            FontId::proportional(font * s.max(0.8)),
            text_col,
        );

        // Badges: turbo (amber) and remapped (green).
        let corner = rect.right_top();
        if marks.turbo & bit != 0 {
            painter.circle_filled(corner, 8.0, WARN);
            painter.text(
                corner,
                Align2::CENTER_CENTER,
                "T",
                FontId::proportional(10.0),
                Color32::BLACK,
            );
        }
        if marks.remapped & bit != 0 {
            let p = corner + Vec2::new(-16.0, 0.0);
            painter.circle_filled(p, 8.0, OK);
            painter.text(
                p,
                Align2::CENTER_CENTER,
                "↻",
                FontId::proportional(10.0),
                Color32::BLACK,
            );
        }
    }
    clicked
}

/// Stick position viewer: dead zone and outer limit rings with the live dot.
pub fn stick_view(ui: &mut egui::Ui, pos: (f32, f32), deadzone: u8, outer: u8, connected: bool) {
    let d = ui.available_width().min(150.0);
    let (resp, painter) = ui.allocate_painter(Vec2::splat(d), Sense::hover());
    let r = resp.rect;
    let c = r.center();
    let rad = d * 0.46;
    painter.circle_filled(c, rad, BG);
    painter.circle_stroke(
        c,
        rad * f32::from(outer) / 100.0,
        Stroke::new(1.0, Color32::from_gray(90)),
    );
    painter.circle_filled(
        c,
        rad * f32::from(deadzone) / 100.0,
        Color32::from_rgba_unmultiplied(0xff, 0x5c, 0x6c, 40),
    );
    painter.line_segment(
        [c - Vec2::new(rad, 0.0), c + Vec2::new(rad, 0.0)],
        Stroke::new(1.0, Color32::from_gray(45)),
    );
    painter.line_segment(
        [c - Vec2::new(0.0, rad), c + Vec2::new(0.0, rad)],
        Stroke::new(1.0, Color32::from_gray(45)),
    );
    if connected {
        let p = c + Vec2::new(pos.0, -pos.1) * rad;
        painter.line_segment([c, p], Stroke::new(1.5, ACCENT));
        painter.circle_filled(p, 7.0, Color32::WHITE);
        let mag = (pos.0.hypot(pos.1) * 100.0).min(100.0);
        painter.text(
            r.left_bottom() + Vec2::new(4.0, -4.0),
            Align2::LEFT_BOTTOM,
            format!("{mag:.0}%"),
            FontId::monospace(12.0),
            TEXT_DIM,
        );
    }
}
