//! Experimental: the city as real pixels, sent with a terminal graphics
//! protocol (Sixel, Kitty or iTerm2) instead of half blocks.
//!
//! The view keeps the same framing as the half-block renderer; every
//! half-block pixel becomes about half a character cell of real pixels, so
//! the same art gets roughly ten times the detail in each direction.

use anyhow::{Context, Result};
use image::{DynamicImage, RgbImage};
use ratatui::{
    Frame,
    layout::{Rect, Size},
};
use ratatui_image::{Image, Resize, picker::Picker};

use super::{Canvas, View, draw_status, paint_magnified, theme::Theme};
use crate::world::World;

pub struct PixelOutput {
    picker: Picker,
}

impl PixelOutput {
    /// Asks the terminal which graphics protocol it speaks and how big its
    /// character cells are. Call it after the terminal is set up and before
    /// reading key events.
    pub fn detect() -> Result<Self> {
        let picker = Picker::from_query_stdio().context("the terminal did not say which graphics it supports")?;
        Ok(Self { picker })
    }

    pub fn draw(&self, frame: &mut Frame, world: &World, view: View, theme: &dyn Theme, anim: u64, source: &str) {
        let area = frame.area();
        if area.width == 0 || area.height < 2 {
            return;
        }
        let city = Rect { height: area.height - 1, ..area };
        let Ok(image) = render(world, view, theme, anim, city, &self.picker) else {
            return;
        };
        frame.render_widget(Image::new(&image), city);
        let bar = Rect { y: area.bottom() - 1, height: 1, ..area };
        let protocol = format!("{:?}", self.picker.protocol_type()).to_lowercase();
        draw_status(frame.buffer_mut(), bar, world, theme.ui(), view, &format!("{source} · {protocol}"));
    }
}

/// Paints the city at the terminal's real pixel size for `area` and encodes
/// it for the picker's protocol.
fn render(
    world: &World,
    view: View,
    theme: &dyn Theme,
    anim: u64,
    area: Rect,
    picker: &Picker,
) -> Result<ratatui_image::protocol::Protocol> {
    let cell = picker.font_size();
    let (width, height) = (u32::from(area.width) * u32::from(cell.width), u32::from(area.height) * u32::from(cell.height));
    let mut canvas = Canvas::new(width as i32, height as i32);
    // A half-block pixel is half a cell tall.
    paint_magnified(world, view, theme, anim, &mut canvas, f32::from(cell.height) / 2.0);
    let image = RgbImage::from_raw(width, height, canvas.rgb_bytes()).context("canvas and image sizes differ")?;
    picker
        .new_protocol(DynamicImage::ImageRgb8(image), Size::new(area.width, area.height), Resize::Fit(None))
        .context("encoding the frame failed")
}

#[cfg(test)]
mod tests {
    use std::time::Instant;

    use ratatui_image::{FontSize, picker::ProtocolType, protocol::Protocol};

    use super::*;
    use crate::render::{Zoom, theme::Metropolis};

    /// Measures what one full-screen Sixel frame costs: a 200×55 terminal
    /// with 10×20 pixel cells, the size of Windows Terminal's default font.
    ///
    /// `KUBE_CITY_REPLAY=rec.jsonl cargo test --release --features sixel sixel_frame_cost -- --ignored --nocapture`
    #[test]
    #[ignore = "benchmark: needs KUBE_CITY_REPLAY"]
    fn sixel_frame_cost() {
        let replay = std::env::var("KUBE_CITY_REPLAY").expect("KUBE_CITY_REPLAY names a recording");
        let mut world = World::default();
        for (_, event) in crate::recording::load(std::path::Path::new(&replay)).expect("recording loads") {
            world.apply(event);
        }
        // No terminal to ask here, so the cell size is given.
        #[allow(deprecated)]
        let mut picker = Picker::from_fontsize(FontSize { width: 10, height: 20 });
        picker.set_protocol_type(ProtocolType::Sixel);
        let area = Rect::new(0, 0, 200, 54);
        for zoom in [Zoom::Detail, Zoom::Overview] {
            let view = View { zoom, ..View::default() };
            let mut timings = Vec::new();
            let mut bytes = 0;
            for anim in 0..5 {
                let started = Instant::now();
                let protocol = render(&world, view, &Metropolis, anim, area, &picker).expect("frame renders");
                timings.push(started.elapsed().as_millis());
                if let Protocol::Sixel(sixel) = protocol {
                    bytes = sixel.data.len();
                }
            }
            let started = Instant::now();
            let mut canvas = Canvas::new(2000, 1080);
            paint_magnified(&world, view, &Metropolis, 0, &mut canvas, 10.0);
            let paint = started.elapsed().as_millis();
            let image = DynamicImage::ImageRgb8(RgbImage::from_raw(2000, 1080, canvas.rgb_bytes()).expect("sizes match"));
            let started = Instant::now();
            picker.new_protocol(image, Size::new(200, 54), Resize::Fit(None)).expect("encodes");
            let encode = started.elapsed().as_millis();
            eprintln!("{zoom:?}: {timings:?} ms per frame (paint {paint} ms, encode {encode} ms), {} KB per frame", bytes / 1024);
            // The middle ground: pixel art drawn at 3× and enlarged without smoothing.
            let started = Instant::now();
            let mut small = Canvas::new(600, 324);
            paint_magnified(&world, view, &Metropolis, 0, &mut small, 3.0);
            let paint_x3 = started.elapsed().as_millis();
            let image = RgbImage::from_raw(600, 324, small.rgb_bytes()).expect("sizes match");
            let enlarged = image::imageops::resize(&image, 2000, 1080, image::imageops::FilterType::Nearest);
            let started = Instant::now();
            let protocol = picker
                .new_protocol(DynamicImage::ImageRgb8(enlarged), Size::new(200, 54), Resize::Fit(None))
                .expect("encodes");
            let encode_x3 = started.elapsed().as_millis();
            let bytes_x3 = if let Protocol::Sixel(sixel) = protocol { sixel.data.len() } else { 0 };
            eprintln!("{zoom:?} at 3x: paint {paint_x3} ms, encode {encode_x3} ms, {} KB per frame", bytes_x3 / 1024);
        }
    }
}
