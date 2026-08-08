//! A flowing plasma field rendered in Unicode braille characters.
//!
//! The field is a classic demoscene-style plasma: several sine waves of
//! different frequency and direction combined over time, mapped to a colour
//! gradient. Each terminal cell is a 2x4 block of sub-pixels, and the
//! intensity at a sub-pixel decides how many of the eight braille dots get
//! lit, so the strip reads as a smooth, living gradient rather than a
//! scatter of dots.
//!
//! While the mouse cursor is over the strip, a ripple emanates from the
//! cursor position and a soft hot spot follows it, so the field reacts
//! directly to where the user is pointing.

use ratatui::{
    layout::Rect,
    style::{Color, Style},
    text::{Line, Span},
};

/// The order in which the eight braille dots inside a cell are filled as the
/// intensity rises. This walks the cell top-to-bottom, left-to-right so the
/// gradient reads as a smooth fill instead of a random pattern.
const FILL_ORDER: [(usize, usize); 8] = [
    (0, 0),
    (1, 0),
    (0, 1),
    (1, 1),
    (0, 2),
    (1, 2),
    (0, 3),
    (1, 3),
];

pub struct PlasmaField {
    /// Animation time, in arbitrary units. The wave phases advance with this.
    tick: f32,
    /// Last known mouse position in global terminal coordinates. The renderer
    /// decides whether it is inside the strip and converts it to local
    /// sub-pixel coordinates.
    mouse: Option<(u16, u16)>,
}

impl Default for PlasmaField {
    fn default() -> Self {
        Self::new()
    }
}

impl PlasmaField {
    pub fn new() -> Self {
        Self {
            tick: 0.0,
            mouse: None,
        }
    }

    pub fn set_mouse(&mut self, x: u16, y: u16) {
        self.mouse = Some((x, y));
    }

    pub fn step(&mut self) {
        // Advance the wave phases. Tuned so the field flows at a calm pace
        // (~one full cycle every few seconds) at the 25 fps frame rate.
        self.tick += 0.06;
    }

    /// Render the field into braille-character lines for the given terminal
    /// area. Returns one `Line` per row of the area.
    pub fn render(&self, area: Rect) -> Vec<Line<'static>> {
        let width = area.width as usize;
        let height = area.height as usize;
        if width == 0 || height == 0 {
            return Vec::new();
        }
        let sub_width = width * 2;
        let sub_height = height * 4;

        // Map the global mouse position into the local sub-pixel coordinate
        // system if it is currently over the strip. Outside the strip the
        // ripple and hot spot simply disappear.
        let local_mouse = self.mouse.and_then(|(mx, my)| {
            if mx >= area.x
                && mx < area.x + area.width
                && my >= area.y
                && my < area.y + area.height
            {
                let lx = f32::from(mx - area.x) * 2.0;
                let ly = f32::from(my - area.y) * 4.0;
                Some((lx, ly))
            } else {
                None
            }
        });

        // Per-sub-pixel intensity in the range 0..=8. 0 means the dot is off,
        // 8 means the cell is fully lit.
        let mut grid = vec![0u8; sub_width * sub_height];
        let t = self.tick;
        for sy in 0..sub_height {
            for sx in 0..sub_width {
                let value = plasma_value(sx as f32, sy as f32, t, local_mouse);
                let intensity = ((value + 1.0) * 4.0).clamp(0.0, 8.0) as u8;
                grid[sy * sub_width + sx] = intensity;
            }
        }

        // Pack intensities into braille characters, choosing the colour by
        // the highest intensity reached in the cell. Each cell lights the
        // first `max_intensity` dots in FILL_ORDER so the cell reads as a
        // smooth fill rather than a random pattern.
        let mut lines = Vec::with_capacity(height);
        for row in 0..height {
            let mut spans = Vec::with_capacity(width);
            for col in 0..width {
                let mut max_intensity: u8 = 0;
                for sub_y in 0..4 {
                    for sub_x in 0..2 {
                        let idx = (row * 4 + sub_y) * sub_width + col * 2 + sub_x;
                        max_intensity = max_intensity.max(grid[idx]);
                    }
                }
                let mut bits: u8 = 0;
                for (index, &(sub_x, sub_y)) in FILL_ORDER.iter().enumerate() {
                    if (index as u8) < max_intensity {
                        bits |= braille_bit(sub_x, sub_y);
                    }
                }
                let glyph = char::from_u32(0x2800 + bits as u32).unwrap_or(' ');
                if bits == 0 {
                    spans.push(Span::raw(" "));
                } else {
                    let color = plasma_color(max_intensity);
                    spans.push(Span::styled(glyph.to_string(), Style::default().fg(color)));
                }
            }
            lines.push(Line::from(spans));
        }
        lines
    }
}

fn braille_bit(sub_x: usize, sub_y: usize) -> u8 {
    match (sub_x, sub_y) {
        (0, 0) => 0x01,
        (0, 1) => 0x02,
        (0, 2) => 0x04,
        (0, 3) => 0x40,
        (1, 0) => 0x08,
        (1, 1) => 0x10,
        (1, 2) => 0x20,
        (1, 3) => 0x80,
        _ => 0,
    }
}

/// Compute the raw plasma value at a sub-pixel. Four sine waves of different
/// frequency and direction are summed, then optionally distorted by the
/// mouse position to add a ripple and a hot spot.
fn plasma_value(x: f32, y: f32, t: f32, mouse: Option<(f32, f32)>) -> f32 {
    let a = (x * 0.10 + t).sin();
    let b = (y * 0.30 + t * 0.6).sin();
    let c = ((x + y) * 0.05 + t * 0.4).sin();
    let d = ((x * x + y * y).sqrt() * 0.08 - t * 0.5).sin();
    let mut v = a + b + c + d;

    if let Some((mx, my)) = mouse {
        let dx = x - mx;
        let dy = y - my;
        let dist = (dx * dx + dy * dy).sqrt();
        // A radial ripple that travels outward and decays with distance.
        let ripple = (dist * 0.25 - t * 4.0).sin() * (-dist * 0.04).exp();
        // A localised hot spot that follows the cursor.
        let hotspot = (-dist * 0.10).exp() * 1.6;
        v += ripple * 1.5 + hotspot;
    }

    v / 4.0
}

fn plasma_color(intensity: u8) -> Color {
    // Walk from a dim blue through cyan up to a near-white peak so the
    // bright areas of the wave read as energetic plasma.
    match intensity {
        0 | 1 => Color::Rgb(40, 90, 150),
        2 => Color::Rgb(50, 140, 200),
        3 => Color::Rgb(70, 180, 220),
        4 => Color::Rgb(90, 210, 230),
        5 => Color::Cyan,
        6 => Color::Rgb(150, 230, 240),
        7 => Color::Rgb(200, 245, 250),
        _ => Color::Rgb(235, 250, 255),
    }
}
