//! An RGB pixel canvas that packs into terminal cells two pixels at a time.

use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Color, Style},
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rgb(pub u8, pub u8, pub u8);

impl Rgb {
    /// Scales brightness; factors above 1.0 lighten.
    pub fn shade(self, factor: f32) -> Self {
        let scale = |c: u8| (f32::from(c) * factor).round().clamp(0.0, 255.0) as u8;
        Self(scale(self.0), scale(self.1), scale(self.2))
    }

    /// Blends towards `other`: 0.0 keeps `self`, 1.0 gives `other`.
    pub fn mix(self, other: Self, t: f32) -> Self {
        let blend = |a: u8, b: u8| (f32::from(a) + (f32::from(b) - f32::from(a)) * t).round() as u8;
        Self(blend(self.0, other.0), blend(self.1, other.1), blend(self.2, other.2))
    }
}

impl From<Rgb> for Color {
    fn from(c: Rgb) -> Self {
        Color::Rgb(c.0, c.1, c.2)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Canvas {
    width: i32,
    height: i32,
    pixels: Vec<Rgb>,
}

impl Canvas {
    pub fn new(width: i32, height: i32) -> Self {
        let (width, height) = (width.max(0), height.max(0));
        Self { width, height, pixels: vec![Rgb(0, 0, 0); (width * height) as usize] }
    }

    pub fn width(&self) -> i32 {
        self.width
    }

    pub fn height(&self) -> i32 {
        self.height
    }

    pub fn fill(&mut self, colour: Rgb) {
        self.pixels.fill(colour);
    }

    /// Sets a pixel; anything off the canvas is clipped.
    pub fn set(&mut self, x: i32, y: i32, colour: Rgb) {
        if (0..self.width).contains(&x) && (0..self.height).contains(&y) {
            self.pixels[(y * self.width + x) as usize] = colour;
        }
    }

    pub fn get(&self, x: i32, y: i32) -> Option<Rgb> {
        ((0..self.width).contains(&x) && (0..self.height).contains(&y))
            .then(|| self.pixels[(y * self.width + x) as usize])
    }

    /// Fills a polygon, sampling pixel centres, so polygons that share an
    /// edge neither overlap nor leave a gap.
    pub fn poly(&mut self, points: &[(f32, f32)], colour: Rgb) {
        let Some((low, high)) = points.iter().map(|p| p.1).fold(None, |range: Option<(f32, f32)>, y| {
            Some(range.map_or((y, y), |(low, high)| (low.min(y), high.max(y))))
        }) else {
            return;
        };
        let mut crossings = Vec::with_capacity(points.len());
        for y in (low.floor() as i32).max(0)..=(high.ceil() as i32).min(self.height - 1) {
            let centre = y as f32 + 0.5;
            crossings.clear();
            for (i, &(x1, y1)) in points.iter().enumerate() {
                let (x2, y2) = points[(i + 1) % points.len()];
                if (y1 <= centre && centre < y2) || (y2 <= centre && centre < y1) {
                    crossings.push(x1 + (centre - y1) * (x2 - x1) / (y2 - y1));
                }
            }
            crossings.sort_by(f32::total_cmp);
            for &[start, end] in crossings.as_chunks::<2>().0 {
                let from = ((start - 0.5).ceil() as i32).max(0);
                let to = ((end - 0.5).ceil() as i32).min(self.width);
                for x in from..to {
                    self.set(x, y, colour);
                }
            }
        }
    }

    /// A one-pixel line (Bresenham).
    pub fn line(&mut self, from: (f32, f32), to: (f32, f32), colour: Rgb) {
        let (mut x, mut y) = (from.0.round() as i32, from.1.round() as i32);
        let (x1, y1) = (to.0.round() as i32, to.1.round() as i32);
        let (dx, dy) = ((x1 - x).abs(), -(y1 - y).abs());
        let (step_x, step_y) = ((x1 - x).signum(), (y1 - y).signum());
        let mut error = dx + dy;
        loop {
            self.set(x, y, colour);
            if x == x1 && y == y1 {
                break;
            }
            let doubled = 2 * error;
            if doubled >= dy {
                error += dy;
                x += step_x;
            }
            if doubled <= dx {
                error += dx;
                y += step_y;
            }
        }
    }

    /// A filled ellipse; `colour` gets the horizontal position from -1.0 (left
    /// edge) to 1.0 (right edge), for shading round things.
    pub fn ellipse(&mut self, cx: f32, cy: f32, rx: f32, ry: f32, colour: impl Fn(f32) -> Rgb) {
        if rx <= 0.0 || ry <= 0.0 {
            return;
        }
        for y in ((cy - ry).floor() as i32).max(0)..=((cy + ry).ceil() as i32).min(self.height - 1) {
            let dy = (y as f32 + 0.5 - cy) / ry;
            if dy.abs() >= 1.0 {
                continue;
            }
            let half = rx * (1.0 - dy * dy).sqrt();
            let from = ((cx - half - 0.5).ceil() as i32).max(0);
            let to = ((cx + half - 0.5).ceil() as i32).min(self.width);
            for x in from..to {
                self.set(x, y, colour((x as f32 + 0.5 - cx) / rx));
            }
        }
    }

    /// The canvas as a plain-text PPM image, for snapshots.
    #[cfg(test)]
    pub fn to_ppm(&self) -> String {
        let mut out = format!("P3\n{} {}\n255\n", self.width, self.height);
        for Rgb(r, g, b) in &self.pixels {
            out.push_str(&format!("{r} {g} {b}\n"));
        }
        out
    }

    /// Writes pixel rows `2r` and `2r + 1` into cell row `r` as `▀`, with the
    /// upper pixel as foreground and the lower one as background.
    pub fn blit(&self, buf: &mut Buffer, area: Rect) {
        for row in 0..area.height {
            for col in 0..area.width {
                let (x, y) = (i32::from(col), i32::from(row) * 2);
                let (Some(upper), Some(lower)) = (self.get(x, y), self.get(x, y + 1)) else {
                    continue;
                };
                if let Some(cell) = buf.cell_mut((area.x + col, area.y + row)) {
                    cell.set_symbol("▀").set_style(Style::new().fg(upper.into()).bg(lower.into()));
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn off_canvas_pixels_are_clipped() {
        let mut canvas = Canvas::new(2, 2);
        canvas.set(-1, 0, Rgb(9, 9, 9));
        canvas.set(2, 1, Rgb(9, 9, 9));
        assert_eq!(canvas, Canvas::new(2, 2));
        assert_eq!(canvas.get(0, 2), None);
    }

    #[test]
    fn blit_packs_two_pixel_rows_per_cell() {
        let mut canvas = Canvas::new(1, 2);
        canvas.set(0, 0, Rgb(255, 0, 0));
        canvas.set(0, 1, Rgb(0, 0, 255));
        let mut buf = Buffer::empty(Rect::new(0, 0, 1, 1));
        canvas.blit(&mut buf, Rect::new(0, 0, 1, 1));
        let cell = &buf[(0, 0)];
        assert_eq!(cell.symbol(), "▀");
        assert_eq!(cell.fg, Color::Rgb(255, 0, 0));
        assert_eq!(cell.bg, Color::Rgb(0, 0, 255));
    }

    fn count(canvas: &Canvas, colour: Rgb) -> usize {
        canvas.pixels.iter().filter(|&&p| p == colour).count()
    }

    #[test]
    fn polygons_sharing_an_edge_neither_overlap_nor_leave_gaps() {
        let ink = Rgb(1, 1, 1);
        let left = [(3.0, 2.0), (11.0, 6.0), (7.5, 13.0), (1.0, 9.0)];
        let right = [(11.0, 6.0), (19.0, 3.0), (17.0, 12.0), (7.5, 13.0)];
        let alone = |shape: &[(f32, f32)]| {
            let mut canvas = Canvas::new(24, 16);
            canvas.poly(shape, ink);
            count(&canvas, ink)
        };
        let mut both = Canvas::new(24, 16);
        both.poly(&left, ink);
        both.poly(&right, ink);
        assert_eq!(count(&both, ink), alone(&left) + alone(&right));
    }

    #[test]
    fn shapes_far_off_the_canvas_are_clipped() {
        let mut canvas = Canvas::new(4, 4);
        canvas.poly(&[(-1e6, -1e6), (1e6, -1e6), (0.0, 1e6)], Rgb(1, 1, 1));
        canvas.line((-500.0, 2.0), (500.0, 2.0), Rgb(2, 2, 2));
        canvas.ellipse(2.0, 2.0, 1e5, 1e5, |_| Rgb(3, 3, 3));
        canvas.poly(&[], Rgb(4, 4, 4));
        assert_eq!(count(&canvas, Rgb(3, 3, 3)), 16);
    }

    #[test]
    fn shade_and_mix_stay_in_range() {
        assert_eq!(Rgb(200, 100, 0).shade(2.0), Rgb(255, 200, 0));
        assert_eq!(Rgb(0, 0, 0).mix(Rgb(200, 100, 50), 0.5), Rgb(100, 50, 25));
    }
}
