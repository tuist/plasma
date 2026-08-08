//! Particle animation rendered in Unicode braille characters that sits between
//! the document and the prompt. The particles drift around a bounded field, and
//! when the user moves the mouse over the strip they get pulled towards the
//! cursor, giving the area above the text field a sense of life.

use ratatui::{
    style::{Color, Style},
    text::{Line, Span},
};

const FIELD_WIDTH: f32 = 100.0;
const FIELD_HEIGHT: f32 = 24.0; // 6 braille rows * 4 sub-rows

#[derive(Clone, Copy)]
struct Particle {
    x: f32,
    y: f32,
    vx: f32,
    vy: f32,
    hue: f32,
}

pub struct PlasmaField {
    particles: Vec<Particle>,
    tick: u64,
    mouse: Option<(f32, f32)>,
}

impl Default for PlasmaField {
    fn default() -> Self {
        Self::new()
    }
}

impl PlasmaField {
    pub fn new() -> Self {
        // Spread particles across the field with deterministic pseudo-random
        // starting positions so the animation looks similar on every launch
        // without needing an RNG dependency.
        let mut particles = Vec::with_capacity(28);
        for index in 0..28 {
            let seed = index as f32 * 1.7;
            let x = (seed * 13.0).rem_euclid(FIELD_WIDTH);
            let y = (seed * 7.0).rem_euclid(FIELD_HEIGHT);
            let vx = (seed * 0.91).sin() * 0.6;
            let vy = (seed * 1.13).cos() * 0.4;
            let hue = seed.rem_euclid(1.0);
            particles.push(Particle { x, y, vx, vy, hue });
        }
        Self {
            particles,
            tick: 0,
            mouse: None,
        }
    }

    pub fn set_mouse(&mut self, x: u16, y: u16) {
        self.mouse = Some((x as f32, y as f32));
    }

    pub fn clear_mouse(&mut self) {
        self.mouse = None;
    }

    pub fn step(&mut self) {
        self.tick = self.tick.wrapping_add(1);
        let phase = self.tick as f32 * 0.05;
        for particle in &mut self.particles {
            // Gentle ambient swirl so the field always has motion, even without
            // mouse input.
            particle.vx += (phase + particle.hue * std::f32::consts::TAU).sin() * 0.04;
            particle.vy += (phase + particle.hue * std::f32::consts::PI).cos() * 0.04;

            if let Some((mx, my)) = self.mouse {
                let dx = mx - particle.x;
                let dy = my - particle.y;
                let dist_sq = dx * dx + dy * dy + 8.0;
                let pull = 18.0 / dist_sq;
                particle.vx += dx * pull * 0.02;
                particle.vy += dy * pull * 0.02;
            }

            // Damping to keep velocities bounded.
            particle.vx *= 0.94;
            particle.vy *= 0.94;

            particle.x += particle.vx;
            particle.y += particle.vy;

            // Bounce off the field edges.
            if particle.x < 0.0 {
                particle.x = 0.0;
                particle.vx = -particle.vx;
            } else if particle.x > FIELD_WIDTH {
                particle.x = FIELD_WIDTH;
                particle.vx = -particle.vx;
            }
            if particle.y < 0.0 {
                particle.y = 0.0;
                particle.vy = -particle.vy;
            } else if particle.y > FIELD_HEIGHT {
                particle.y = FIELD_HEIGHT;
                particle.vy = -particle.vy;
            }
        }
    }

    /// Render the field as a stack of braille-character lines. `width` and
    /// `height` are in terminal cells, where each cell becomes a 2x4 block of
    /// sub-pixels.
    pub fn render(&self, width: u16, height: u16) -> Vec<Line<'static>> {
        let width = width as usize;
        let height = height as usize;
        let sub_width = width * 2;
        let sub_height = height * 4;
        let mut grid = vec![0u8; sub_width * sub_height];

        for particle in &self.particles {
            // Map the field coordinate (0..FIELD_WIDTH / FIELD_HEIGHT) into the
            // sub-pixel grid of the rendered area.
            let sx = ((particle.x / FIELD_WIDTH) * sub_width as f32) as i32;
            let sy = ((particle.y / FIELD_HEIGHT) * sub_height as f32) as i32;
            // Each particle lights its own dot plus a soft halo of neighbours
            // so it reads as a glowing point rather than a single sub-pixel.
            for dy in -1..=1 {
                for dx in -1..=1 {
                    let nx = sx + dx;
                    let ny = sy + dy;
                    if nx < 0 || ny < 0 || nx >= sub_width as i32 || ny >= sub_height as i32 {
                        continue;
                    }
                    let manhattan = dx.unsigned_abs() + dy.unsigned_abs();
                    let intensity = match manhattan {
                        0 => 3,
                        1 => 2,
                        _ => 1,
                    };
                    let idx = ny as usize * sub_width + nx as usize;
                    grid[idx] = grid[idx].saturating_add(intensity);
                }
            }
        }

        let mut lines = Vec::with_capacity(height);
        for row in 0..height {
            let mut spans = Vec::with_capacity(width);
            for col in 0..width {
                let mut bits: u8 = 0;
                for sub_y in 0..4 {
                    for sub_x in 0..2 {
                        if grid[row * 4 * sub_width + sub_y * sub_width + col * 2 + sub_x] > 0 {
                            bits |= braille_bit(sub_x, sub_y);
                        }
                    }
                }
                let glyph = char::from_u32(0x2800 + bits as u32).unwrap_or(' ');
                if bits == 0 {
                    spans.push(Span::raw(" "));
                } else {
                    let color = plasma_color(self.tick, col as u32, row as u32);
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

fn plasma_color(tick: u64, col: u32, row: u32) -> Color {
    // Cycle through cyan, blue and a soft magenta so the strip never sits on
    // a single flat colour.
    let phase = (tick as f32 * 0.1 + col as f32 * 0.3 + row as f32 * 0.5) % std::f32::consts::TAU;
    let band = (phase.sin() + 1.0) / 2.0;
    if band < 0.5 {
        Color::Rgb(40, 180 + ((1.0 - band * 2.0) * 60.0) as u8, 220)
    } else {
        Color::Rgb(80 + ((band - 0.5) * 2.0 * 80.0) as u8, 200, 220)
    }
}
