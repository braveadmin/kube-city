//! Draws the world as an isometric city.
//!
//! The city is painted on a pixel canvas, then packed into the terminal two
//! pixels per cell with the upper half block `▀`. Those pixels are roughly
//! square, so tiles get the classic 2:1 isometric diamond shape.

mod canvas;

use ratatui::{
    Frame,
    buffer::Buffer,
    layout::Rect,
    style::{Color, Style},
};

pub use canvas::{Canvas, Rgb};

use crate::{
    layout::{self, BLOCK_SIDE, BLOCK_STRIDE, Layout},
    util::stable_hash,
    world::{PodInfo, PodState, World},
};

const TILE_W: i32 = 8;
const TILE_H: i32 = 4;
/// A building's footprint, smaller than its tile so lots read as separate.
const FOOT_W: i32 = 6;
const FOOT_H: i32 = 3;
/// Shifts the view down so the centre block, not the sky above it, is centred.
const CENTRE_Y: i32 = 8;
const LABEL_MAX: usize = 18;

const SKY: Rgb = Rgb(14, 17, 30);
const GRASS: Rgb = Rgb(40, 62, 40);
const ROAD: Rgb = Rgb(58, 58, 64);
const SCAFFOLD: Rgb = Rgb(205, 145, 45);
const FIRE: Rgb = Rgb(235, 75, 40);
const LIT_WINDOW: Rgb = Rgb(250, 215, 110);
const ASH: Rgb = Rgb(90, 90, 95);
const DISTRICT_GROUNDS: [Rgb; 7] = [
    Rgb(84, 110, 70),
    Rgb(120, 110, 85),
    Rgb(80, 95, 115),
    Rgb(105, 85, 105),
    Rgb(70, 105, 100),
    Rgb(115, 95, 70),
    Rgb(95, 105, 60),
];
const FACADES: [Rgb; 7] = [
    Rgb(170, 170, 180),
    Rgb(165, 95, 75),
    Rgb(90, 130, 170),
    Rgb(190, 170, 130),
    Rgb(80, 150, 140),
    Rgb(150, 120, 170),
    Rgb(200, 200, 205),
];

/// Where the view looks, as a pixel offset from the city centre.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct Camera {
    pub x: i32,
    pub y: i32,
}

impl Camera {
    pub fn pan(&mut self, dx: i32, dy: i32) {
        self.x += dx;
        self.y += dy;
    }
}

/// A namespace name to write next to its district, in canvas pixels.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Label {
    pub x: i32,
    pub y: i32,
    pub text: String,
}

/// Draws the city over the whole frame, with a status bar on the last row.
pub fn draw(frame: &mut Frame, world: &World, camera: Camera, source: &str) {
    let area = frame.area();
    if area.width == 0 || area.height < 2 {
        return;
    }
    let city = Rect { height: area.height - 1, ..area };
    let mut canvas = Canvas::new(i32::from(city.width), i32::from(city.height) * 2);
    let labels = paint(world, camera, &mut canvas);
    let buf = frame.buffer_mut();
    canvas.blit(buf, city);
    for label in &labels {
        put_label(buf, city, label);
    }
    let bar = Rect { y: area.bottom() - 1, height: 1, ..area };
    draw_status(buf, bar, world, source);
}

/// Paints ground, roads and buildings; returns the district labels to overlay.
pub fn paint(world: &World, camera: Camera, canvas: &mut Canvas) -> Vec<Label> {
    canvas.fill(SKY);
    let ox = canvas.width() / 2 + camera.x;
    let oy = canvas.height() / 2 + camera.y - CENTRE_Y;
    let layout = world.layout();

    // An island just big enough for the occupied blocks, ringed by road.
    let ring = layout.ring();
    let (lo, hi) = (-ring * BLOCK_STRIDE - 1, ring * BLOCK_STRIDE + BLOCK_SIDE);
    for ty in lo..=hi {
        for tx in lo..=hi {
            let (sx, sy) = project(tx, ty);
            fill_diamond(canvas, ox + sx, oy + sy, TILE_W, TILE_H, ground(layout, tx, ty));
        }
    }

    // Back to front, so nearer buildings cover the ones behind them.
    let mut buildings: Vec<_> = world.buildings().map(|b| (Layout::tile(b.lot), &b.pod)).collect();
    buildings.sort_by_key(|&((x, y), _)| (x + y, x));
    for ((tx, ty), pod) in buildings {
        let (sx, sy) = project(tx, ty);
        paint_building(canvas, ox + sx, oy + sy, &Look::of(pod));
    }

    layout
        .districts()
        .map(|(block, namespace)| {
            // Under the block's front corner, on the road.
            let (bx, by) = layout::spiral(block);
            let front = (bx * BLOCK_STRIDE + BLOCK_SIDE - 1, by * BLOCK_STRIDE + BLOCK_SIDE - 1);
            let (sx, sy) = project(front.0, front.1);
            Label { x: ox + sx, y: oy + sy + TILE_H + 1, text: namespace.to_owned() }
        })
        .collect()
}

/// The top vertex of tile (tx, ty), in pixels relative to the city origin.
fn project(tx: i32, ty: i32) -> (i32, i32) {
    ((tx - ty) * TILE_W / 2, (tx + ty) * TILE_H / 2)
}

/// Rows of a `w`×`h` diamond whose top vertex is at (`cx`, `top`), as
/// `(y, x_start, x_end_exclusive)`. Pixel centres are sampled, so diamonds of
/// neighbouring tiles fit together without gaps or overlap.
fn diamond_rows(cx: i32, top: i32, w: i32, h: i32) -> impl Iterator<Item = (i32, i32, i32)> {
    (0..h).map(move |r| {
        let distance = (2 * r + 1 - h).abs();
        let half = (w * (h - distance) + h) / (2 * h);
        (top + r, cx - half, cx + half)
    })
}

fn fill_diamond(canvas: &mut Canvas, cx: i32, top: i32, w: i32, h: i32, colour: Rgb) {
    for (y, x0, x1) in diamond_rows(cx, top, w, h) {
        for x in x0..x1 {
            canvas.set(x, y, colour);
        }
    }
}

fn ground(layout: &Layout, tx: i32, ty: i32) -> Rgb {
    if layout::is_road(tx, ty) {
        return ROAD;
    }
    let base = match layout.block_at(tx, ty) {
        Some(block) => DISTRICT_GROUNDS[pick(stable_hash(&block.namespace), DISTRICT_GROUNDS.len())],
        None => GRASS,
    };
    if (tx + ty).rem_euclid(2) == 0 { base } else { base.shade(0.94) }
}

fn pick(hash: u64, len: usize) -> usize {
    (hash % len as u64) as usize
}

enum Windows {
    None,
    Lit,
    Dark,
}

/// How a pod's building looks: workload decides shape and colour, state
/// decides everything else.
struct Look {
    height: i32,
    facade: Rgb,
    roof: Rgb,
    windows: Windows,
}

impl Look {
    fn of(pod: &PodInfo) -> Self {
        let hash = stable_hash(&format!("{}/{}", pod.namespace, pod.workload));
        let facade = FACADES[pick(hash, FACADES.len())];
        let height = 4 + ((hash >> 16) % 9) as i32;
        match pod.state {
            PodState::Running => Self { height, facade, roof: facade.shade(1.15), windows: Windows::Lit },
            PodState::Pending => Self { height: 2, facade: SCAFFOLD, roof: SCAFFOLD.shade(1.15), windows: Windows::None },
            PodState::CrashLoop => Self { height, facade: facade.shade(0.7), roof: FIRE, windows: Windows::Dark },
            PodState::Terminating => {
                let faded = facade.mix(ASH, 0.6);
                Self { height, facade: faded, roof: faded.shade(1.1), windows: Windows::Dark }
            }
            PodState::Succeeded => {
                Self { height: 2, facade: Rgb(120, 120, 125), roof: Rgb(140, 140, 145), windows: Windows::None }
            }
            PodState::Failed => Self { height: 3, facade: Rgb(110, 60, 60), roof: Rgb(130, 70, 70), windows: Windows::None },
        }
    }
}

/// A box standing on the tile whose top vertex is (`cx`, `tile_top`): two
/// side faces of pixel columns, then the roof diamond on top of them.
fn paint_building(canvas: &mut Canvas, cx: i32, tile_top: i32, look: &Look) {
    let foot_top = tile_top + (TILE_H - FOOT_H + 1) / 2;
    let footprint: Vec<_> = diamond_rows(cx, foot_top, FOOT_W, FOOT_H).collect();
    for x in cx - FOOT_W / 2..cx + FOOT_W / 2 {
        let Some(bottom) = footprint.iter().filter(|(_, x0, x1)| (*x0..*x1).contains(&x)).map(|(y, ..)| *y).max()
        else {
            continue;
        };
        let left = x < cx;
        let face = look.facade.shade(if left { 0.62 } else { 0.85 });
        // One column of windows in the middle of each face.
        let window_column = x - cx == if left { -2 } else { 1 };
        for level in 0..look.height {
            let window = window_column && level % 2 == 1 && level < look.height - 1;
            let colour = match (window, &look.windows) {
                (true, Windows::Lit) => LIT_WINDOW,
                (true, Windows::Dark) => face.shade(0.45),
                _ => face,
            };
            canvas.set(x, bottom - level, colour);
        }
    }
    fill_diamond(canvas, cx, foot_top - look.height, FOOT_W, FOOT_H, look.roof);
}

fn label_style() -> Style {
    Style::new().fg(Color::Rgb(235, 235, 240)).bg(Color::Rgb(20, 22, 32))
}

fn status_style() -> Style {
    Style::new().fg(Color::Rgb(200, 200, 210)).bg(Color::Rgb(30, 32, 45))
}

fn put_label(buf: &mut Buffer, area: Rect, label: &Label) {
    if label.y < 0 || label.y / 2 >= i32::from(area.height) {
        return;
    }
    let row = area.y + (label.y / 2) as u16;
    let text: Vec<char> = label.text.chars().take(LABEL_MAX).collect();
    let start = label.x - text.len() as i32 / 2;
    for (i, ch) in text.into_iter().enumerate() {
        let col = start + i as i32;
        if (0..i32::from(area.width)).contains(&col)
            && let Some(cell) = buf.cell_mut((area.x + col as u16, row))
        {
            cell.set_char(ch).set_style(label_style());
        }
    }
}

fn draw_status(buf: &mut Buffer, bar: Rect, world: &World, source: &str) {
    let left = format!(
        " kube-city · {source} · {} pods · {} namespaces",
        world.pod_count(),
        world.namespace_count()
    );
    let right = "←↑↓→ pan · c centre · q quit ";
    buf.set_style(bar, status_style());
    buf.set_stringn(bar.x, bar.y, &left, usize::from(bar.width), status_style());
    let (left_len, right_len) = (left.chars().count(), right.chars().count());
    if left_len + 2 + right_len <= usize::from(bar.width) {
        buf.set_string(bar.right() - right_len as u16, bar.y, right, status_style());
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use ratatui::{Terminal, backend::TestBackend};

    use super::*;
    use crate::{
        demo::DemoCluster,
        world::tests::{delete, upsert},
    };

    fn painted(world: &World, camera: Camera) -> Canvas {
        let mut canvas = Canvas::new(160, 100);
        paint(world, camera, &mut canvas);
        canvas
    }

    fn demo_world(steps: usize) -> World {
        let mut cluster = DemoCluster::new(99);
        let mut world = World::default();
        for event in (0..steps).flat_map(|_| cluster.step()) {
            world.apply(event);
        }
        world
    }

    #[test]
    fn tiles_cover_the_ground_exactly_once() {
        let mut coverage: HashMap<(i32, i32), u32> = HashMap::new();
        for ty in -5..=5 {
            for tx in -5..=5 {
                let (sx, sy) = project(tx, ty);
                for (y, x0, x1) in diamond_rows(sx, sy, TILE_W, TILE_H) {
                    for x in x0..x1 {
                        *coverage.entry((x, y)).or_default() += 1;
                    }
                }
            }
        }
        for y in -8..8 {
            for x in -8..8 {
                assert_eq!(coverage.get(&(x, y)), Some(&1), "pixel ({x}, {y})");
            }
        }
    }

    #[test]
    fn a_pod_appears_and_disappears_from_the_city() {
        let mut world = World::default();
        let empty = painted(&world, Camera::default());
        world.apply(upsert("a", "shop", PodState::Running));
        assert_ne!(painted(&world, Camera::default()), empty);
        world.apply(delete("a"));
        assert_eq!(painted(&world, Camera::default()), empty);
    }

    #[test]
    fn a_state_change_repaints_the_building() {
        let mut world = World::default();
        world.apply(upsert("a", "shop", PodState::Running));
        let running = painted(&world, Camera::default());
        world.apply(upsert("a", "shop", PodState::CrashLoop));
        assert_ne!(painted(&world, Camera::default()), running);
    }

    #[test]
    fn painting_never_panics_at_odd_sizes_or_far_cameras() {
        let world = demo_world(400);
        for (w, h) in [(0, 0), (1, 1), (3, 2), (80, 48), (300, 160)] {
            for camera in [Camera::default(), Camera { x: -5000, y: 5000 }, Camera { x: 37, y: -11 }] {
                let mut canvas = Canvas::new(w, h);
                paint(&world, camera, &mut canvas);
            }
        }
    }

    #[test]
    fn the_status_bar_counts_pods_and_namespaces() {
        let mut world = World::default();
        world.apply(upsert("a", "shop", PodState::Running));
        world.apply(upsert("b", "bank", PodState::Pending));
        let mut terminal = Terminal::new(TestBackend::new(100, 30)).expect("test terminal");
        terminal.draw(|frame| draw(frame, &world, Camera::default(), "test")).expect("draw");
        let buf = terminal.backend().buffer();
        let row = |y: u16| (0..100).map(|x| buf[(x, y)].symbol()).collect::<String>();
        assert!(row(29).contains("test · 2 pods · 2 namespaces"), "status: {}", row(29));
        assert!((0..29).any(|y| row(y).contains("shop")), "district label is drawn");
    }

    #[test]
    fn tiny_terminals_draw_nothing_instead_of_panicking() {
        let world = demo_world(50);
        for (w, h) in [(1, 1), (10, 1), (1, 5)] {
            let mut terminal = Terminal::new(TestBackend::new(w, h)).expect("test terminal");
            terminal.draw(|frame| draw(frame, &world, Camera::default(), "demo")).expect("draw");
        }
    }
}
