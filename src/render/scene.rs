//! Isometric drawing: the projection and the solids buildings are made of.
//!
//! World coordinates are `x` and `y` in tiles and `z` in pixels at detail
//! zoom. Overview zoom halves everything, so a theme describes a building once
//! and it works at both sizes.

use super::canvas::{Canvas, Rgb};

/// A tile at detail zoom: 16 terminal columns by 4 rows.
const TILE_W: f32 = 16.0;
const TILE_H: f32 = 8.0;

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub enum Zoom {
    #[default]
    Detail,
    Overview,
    /// Buildings become small tiles coloured by state: a health map.
    Map,
}

impl Zoom {
    /// Screen pixels per world pixel.
    pub fn scale(self) -> f32 {
        match self {
            Zoom::Detail => 1.0,
            Zoom::Overview => 0.5,
            Zoom::Map => 0.25,
        }
    }

    pub fn next(self) -> Self {
        match self {
            Zoom::Detail => Zoom::Overview,
            Zoom::Overview => Zoom::Map,
            Zoom::Map => Zoom::Detail,
        }
    }
}

/// A rectangle on the ground, in tiles.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Footprint {
    pub x0: f32,
    pub y0: f32,
    pub x1: f32,
    pub y1: f32,
}

impl Footprint {
    pub fn new(x0: f32, y0: f32, x1: f32, y1: f32) -> Self {
        Self { x0, y0, x1, y1 }
    }

    /// Tile (tx, ty), shrunk by `inset` tiles on every side.
    pub fn lot(tx: i32, ty: i32, inset: f32) -> Self {
        Self::new(tx as f32, ty as f32, tx as f32 + 1.0, ty as f32 + 1.0).inset(inset)
    }

    pub fn inset(self, by: f32) -> Self {
        Self::new(self.x0 + by, self.y0 + by, self.x1 - by, self.y1 - by)
    }
}

/// One pixel of a wall, handed to face patterns: column from the left edge
/// and height from the bottom, both in screen pixels.
#[derive(Debug, Clone, Copy)]
pub struct FacePixel {
    pub col: i32,
    pub cols: i32,
    pub z: i32,
    pub height: i32,
}

pub struct Scene<'a> {
    canvas: &'a mut Canvas,
    origin: (f32, f32),
    scale: f32,
    frame: u64,
    /// While set, everything drawn is blended towards this colour.
    mute: Option<(Rgb, f32)>,
}

fn ink(mute: Option<(Rgb, f32)>, colour: Rgb) -> Rgb {
    mute.map_or(colour, |(towards, amount)| colour.mix(towards, amount))
}

impl<'a> Scene<'a> {
    /// `origin` is where the top corner of tile (0, 0) lands on the canvas;
    /// `frame` counts animation steps.
    pub fn new(canvas: &'a mut Canvas, zoom: Zoom, origin: (i32, i32), frame: u64) -> Self {
        Self { canvas, origin: (origin.0 as f32, origin.1 as f32), scale: zoom.scale(), frame, mute: None }
    }

    /// Fades what is drawn next towards `towards` by `amount` (0.0 to 1.0).
    pub fn set_mute(&mut self, mute: Option<(Rgb, f32)>) {
        self.mute = mute;
    }

    pub fn frame(&self) -> u64 {
        self.frame
    }

    pub fn scale(&self) -> f32 {
        self.scale
    }

    /// Small details (window grids, rooftop clutter) are only worth drawing here.
    pub fn detailed(&self) -> bool {
        self.scale >= 1.0
    }

    pub fn project(&self, x: f32, y: f32, z: f32) -> (f32, f32) {
        (
            self.origin.0 + (x - y) * TILE_W / 2.0 * self.scale,
            self.origin.1 + ((x + y) * TILE_H / 2.0 - z) * self.scale,
        )
    }

    /// Whether something on tile (tx, ty) rising `up` world pixels can reach the canvas.
    pub fn visible(&self, tx: i32, ty: i32, up: f32) -> bool {
        let (sx, sy) = self.project(tx as f32 + 0.5, ty as f32 + 0.5, 0.0);
        let (w, h) = (TILE_W * self.scale, TILE_H * self.scale);
        sx + w >= 0.0
            && sx - w <= self.canvas.width() as f32
            && sy + h >= 0.0
            && sy - h - up * self.scale <= self.canvas.height() as f32
    }

    fn z_px(&self, z: f32) -> i32 {
        (z * self.scale).round() as i32
    }

    /// One screen pixel, for particles and other effects.
    pub fn dot(&mut self, at: (f32, f32), colour: Rgb) {
        self.canvas.set(at.0.floor() as i32, at.1.floor() as i32, ink(self.mute, colour));
    }

    pub fn screen_line(&mut self, from: (f32, f32), to: (f32, f32), colour: Rgb) {
        self.canvas.line(from, to, ink(self.mute, colour));
    }

    pub fn line(&mut self, from: (f32, f32, f32), to: (f32, f32, f32), colour: Rgb) {
        let (a, b) = (self.project(from.0, from.1, from.2), self.project(to.0, to.1, to.2));
        self.canvas.line(a, b, ink(self.mute, colour));
    }

    pub fn ellipse(&mut self, centre: (f32, f32), rx: f32, ry: f32, colour: impl Fn(f32) -> Rgb) {
        let mute = self.mute;
        self.canvas.ellipse(centre.0, centre.1, rx, ry, |t| ink(mute, colour(t)));
    }

    pub fn polygon(&mut self, points: &[(f32, f32, f32)], colour: Rgb) {
        let projected: Vec<_> = points.iter().map(|&(x, y, z)| self.project(x, y, z)).collect();
        self.canvas.poly(&projected, ink(self.mute, colour));
    }

    /// A horizontal rectangle at height `z`: ground, roofs, slabs.
    pub fn flat(&mut self, f: Footprint, z: f32, colour: Rgb) {
        self.polygon(&[(f.x0, f.y0, z), (f.x1, f.y0, z), (f.x1, f.y1, z), (f.x0, f.y1, z)], colour);
    }

    /// The outline of a horizontal rectangle, e.g. a roof parapet.
    pub fn rim(&mut self, f: Footprint, z: f32, colour: Rgb) {
        let corners = [(f.x0, f.y0), (f.x1, f.y0), (f.x1, f.y1), (f.x0, f.y1)];
        for (i, &(x, y)) in corners.iter().enumerate() {
            let (nx, ny) = corners[(i + 1) % corners.len()];
            self.line((x, y, z), (nx, ny, z), colour);
        }
    }

    /// The wall on plane `y` between `x0` and `x1`: the one on the left of a
    /// building, facing the viewer.
    pub fn face_left(&mut self, x0: f32, x1: f32, y: f32, z0: f32, z1: f32, paint: impl Fn(FacePixel) -> Rgb) {
        let (ax, ay) = self.project(x0, y, 0.0);
        let cols = ((x1 - x0) * TILE_W / 2.0 * self.scale).round() as i32;
        self.face(ax, ay, cols, 0.5, z0, z1, paint);
    }

    /// The wall on plane `x` between `y1` (front) and `y0` (back): the one on
    /// the right of a building.
    pub fn face_right(&mut self, x: f32, y0: f32, y1: f32, z0: f32, z1: f32, paint: impl Fn(FacePixel) -> Rgb) {
        let (ax, ay) = self.project(x, y1, 0.0);
        let cols = ((y1 - y0) * TILE_W / 2.0 * self.scale).round() as i32;
        self.face(ax, ay, cols, -0.5, z0, z1, paint);
    }

    /// Paints a wall column by column, so patterns follow the 2:1 slope exactly.
    #[allow(clippy::too_many_arguments)]
    fn face(&mut self, ax: f32, ay: f32, cols: i32, slope: f32, z0: f32, z1: f32, paint: impl Fn(FacePixel) -> Rgb) {
        let (bottom, top) = (self.z_px(z0), self.z_px(z1));
        for col in 0..cols {
            let base = ay + (col as f32 + 0.5) * slope;
            let x = (ax + col as f32).floor() as i32;
            for z in bottom..top {
                let colour = paint(FacePixel { col, cols, z: z - bottom, height: top - bottom });
                self.canvas.set(x, (base - z as f32).floor() as i32, ink(self.mute, colour));
            }
        }
    }

    /// A box: two visible walls and a flat top.
    pub fn cuboid(
        &mut self,
        f: Footprint,
        z0: f32,
        z1: f32,
        top: Rgb,
        left: impl Fn(FacePixel) -> Rgb,
        right: impl Fn(FacePixel) -> Rgb,
    ) {
        self.face_left(f.x0, f.x1, f.y1, z0, z1, left);
        self.face_right(f.x1, f.y0, f.y1, z0, z1, right);
        self.flat(f, z1, top);
    }

    /// A pitched roof on top of `f` at height `z`, with its ridge along x.
    pub fn gable(&mut self, f: Footprint, z: f32, rise: f32, roof: Rgb, back: Rgb, end: Rgb) {
        let (middle, ridge) = ((f.y0 + f.y1) / 2.0, z + rise);
        self.polygon(&[(f.x0, f.y0, z), (f.x1, f.y0, z), (f.x1, middle, ridge), (f.x0, middle, ridge)], back);
        self.polygon(&[(f.x1, f.y0, z), (f.x1, f.y1, z), (f.x1, middle, ridge)], end);
        self.polygon(&[(f.x0, f.y1, z), (f.x1, f.y1, z), (f.x1, middle, ridge), (f.x0, middle, ridge)], roof);
    }

    /// A round solid around (cx, cy): stacked ellipses whose radius (in
    /// tiles) and colour vary with height. `side` gets the horizontal position
    /// (-1.0 to 1.0) and the world height.
    pub fn revolve(
        &mut self,
        centre: (f32, f32),
        z0: f32,
        z1: f32,
        radius: impl Fn(f32) -> f32,
        side: impl Fn(f32, f32) -> Rgb,
        top: Option<Rgb>,
    ) {
        let (sx, sy) = self.project(centre.0, centre.1, 0.0);
        let stretch = std::f32::consts::SQRT_2 * self.scale;
        let mute = self.mute;
        let (bottom, top_px) = (self.z_px(z0), self.z_px(z1));
        for z in bottom..top_px {
            let world_z = z as f32 / self.scale;
            let r = radius(world_z) * stretch;
            self.canvas.ellipse(sx, sy - z as f32, r * TILE_W / 2.0, r * TILE_H / 2.0, |t| ink(mute, side(t, world_z)));
        }
        if let Some(colour) = top {
            let r = radius(z1) * stretch;
            self.canvas.ellipse(sx, sy - top_px as f32, r * TILE_W / 2.0, r * TILE_H / 2.0, |_| ink(mute, colour));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ground_tiles_cover_the_plane_without_gaps_at_every_zoom() {
        let (hole, a, b) = (Rgb(0, 0, 0), Rgb(1, 1, 1), Rgb(2, 2, 2));
        for zoom in [Zoom::Detail, Zoom::Overview, Zoom::Map] {
            let mut canvas = Canvas::new(64, 48);
            canvas.fill(hole);
            let mut scene = Scene::new(&mut canvas, zoom, (32, 4), 0);
            for ty in -40..40 {
                for tx in -40..40 {
                    let colour = if (tx + ty) % 2 == 0 { a } else { b };
                    scene.flat(Footprint::lot(tx, ty, 0.0), 0.0, colour);
                }
            }
            for y in 8..40 {
                for x in 8..56 {
                    assert_ne!(canvas.get(x, y), Some(hole), "gap at ({x}, {y}) in {zoom:?}");
                }
            }
        }
    }

    #[test]
    fn overview_draws_everything_at_half_size() {
        let mut canvas = Canvas::new(1, 1);
        let detail = Scene::new(&mut canvas, Zoom::Detail, (0, 0), 0).project(2.0, 1.0, 10.0);
        let overview = Scene::new(&mut canvas, Zoom::Overview, (0, 0), 0).project(2.0, 1.0, 10.0);
        assert_eq!((detail.0 / 2.0, detail.1 / 2.0), overview);
    }

    #[test]
    fn muting_fades_everything_drawn_until_it_is_lifted() {
        let mut canvas = Canvas::new(4, 1);
        let mut scene = Scene::new(&mut canvas, Zoom::Detail, (0, 0), 0);
        scene.set_mute(Some((Rgb(0, 0, 0), 0.5)));
        scene.dot((0.0, 0.0), Rgb(200, 100, 50));
        scene.set_mute(None);
        scene.dot((1.0, 0.0), Rgb(200, 100, 50));
        assert_eq!((canvas.get(0, 0), canvas.get(1, 0)), (Some(Rgb(100, 50, 25)), Some(Rgb(200, 100, 50))));
    }

    #[test]
    fn walls_follow_the_two_to_one_slope() {
        let ink = Rgb(9, 9, 9);
        let mut canvas = Canvas::new(40, 40);
        Scene::new(&mut canvas, Zoom::Detail, (20, 10), 0).face_left(0.0, 1.0, 1.0, 0.0, 1.0, |_| ink);
        let rows: Vec<i32> = (0..40).filter_map(|x| (0..40).find(|&y| canvas.get(x, y) == Some(ink))).collect();
        assert_eq!(rows.len(), 8, "a one-tile wall is 8 columns wide");
        assert!(rows.windows(2).all(|pair| (0..=1).contains(&(pair[1] - pair[0]))), "{rows:?}");
        assert_eq!(rows[7] - rows[0], 3, "it drops half a pixel per column");
    }
}
