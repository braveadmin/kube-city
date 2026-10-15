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

    #[test]
    fn shade_and_mix_stay_in_range() {
        assert_eq!(Rgb(200, 100, 0).shade(2.0), Rgb(255, 200, 0));
        assert_eq!(Rgb(0, 0, 0).mix(Rgb(200, 100, 50), 0.5), Rgb(100, 50, 25));
    }
}
