//! Draws the world as an isometric city.
//!
//! The city is painted on a pixel canvas, then packed into the terminal two
//! pixels per cell with the upper half block `▀`. Those pixels are roughly
//! square, so tiles get the classic 2:1 isometric diamond shape. What stands
//! where comes from the world's layout; how it looks comes from the theme.

mod canvas;
mod scene;
pub mod theme;

use ratatui::{Frame, buffer::Buffer, layout::Rect, style::Style};

pub use canvas::Canvas;
pub use scene::Zoom;

use self::{
    scene::Scene,
    theme::{BuildingLook, Ground, RoadDirection, Theme, UiColours},
};
use crate::{
    layout::{self, BLOCK_SIDE, BLOCK_STRIDE, Layout},
    util::{mix, stable_hash},
    world::{PodInfo, World},
};

const LABEL_MAX: usize = 18;

/// What the screen shows: a pixel offset from the city centre and a zoom.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct View {
    pub x: i32,
    pub y: i32,
    pub zoom: Zoom,
}

impl View {
    pub fn pan(&mut self, dx: i32, dy: i32) {
        self.x += dx;
        self.y += dy;
    }

    /// Switches zoom, keeping the same spot in the middle of the screen.
    pub fn toggle_zoom(&mut self) {
        let next = self.zoom.toggled();
        let ratio = next.scale() / self.zoom.scale();
        self.x = (self.x as f32 * ratio).round() as i32;
        self.y = (self.y as f32 * ratio).round() as i32;
        self.zoom = next;
    }

    pub fn recentre(&mut self) {
        self.x = 0;
        self.y = 0;
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
/// `anim` counts 100 ms animation steps.
pub fn draw(frame: &mut Frame, world: &World, view: View, theme: &dyn Theme, anim: u64, source: &str) {
    let area = frame.area();
    if area.width == 0 || area.height < 2 {
        return;
    }
    let city = Rect { height: area.height - 1, ..area };
    let mut canvas = Canvas::new(i32::from(city.width), i32::from(city.height) * 2);
    let labels = paint(world, view, theme, anim, &mut canvas);
    let buf = frame.buffer_mut();
    canvas.blit(buf, city);
    let ui = theme.ui();
    for label in &labels {
        put_label(buf, city, label, ui);
    }
    let bar = Rect { y: area.bottom() - 1, height: 1, ..area };
    draw_status(buf, bar, world, ui, view, source);
}

/// Paints ground and buildings; returns the district labels to overlay.
pub fn paint(world: &World, view: View, theme: &dyn Theme, anim: u64, canvas: &mut Canvas) -> Vec<Label> {
    canvas.fill(theme.sky());
    let k = view.zoom.scale();
    // The centre of the first block sits a little below the middle of the screen.
    let origin = (canvas.width() / 2 + view.x, canvas.height() / 2 + view.y - (8.0 * k) as i32);
    let mut scene = Scene::new(canvas, view.zoom, origin, anim);
    let layout = world.layout();

    // An island just big enough for the occupied blocks, ringed by road.
    let ring = layout.ring();
    let (lo, hi) = (-ring * BLOCK_STRIDE - 1, ring * BLOCK_STRIDE + BLOCK_SIDE);
    for ty in lo..=hi {
        for tx in lo..=hi {
            if scene.visible(tx, ty, 0.0) {
                theme.ground(&mut scene, tx, ty, ground_at(layout, tx, ty));
            }
        }
    }

    // Back to front, so nearer things cover the ones behind them.
    let mut things: Vec<((i32, i32), Option<&PodInfo>)> = world
        .buildings()
        .map(|b| (Layout::tile(b.lot), Some(&b.pod)))
        .chain(layout.empty_lots().map(|lot| (Layout::tile(lot), None)))
        .collect();
    things.sort_by_key(|&((x, y), _)| (x + y, x));
    let reach = theme.max_height();
    for ((tx, ty), pod) in things {
        if !scene.visible(tx, ty, reach) {
            continue;
        }
        match pod {
            Some(pod) => theme.building(&mut scene, tx, ty, &look_of(pod)),
            None => theme.empty_lot(&mut scene, tx, ty, mix(&[tx as u64, ty as u64])),
        }
    }

    layout
        .districts()
        .map(|(block, namespace)| {
            // Under the block's front corner, on the road.
            let (bx, by) = layout::spiral(block);
            let corner = ((bx * BLOCK_STRIDE + BLOCK_SIDE) as f32, (by * BLOCK_STRIDE + BLOCK_SIDE) as f32);
            let (x, y) = scene.project(corner.0, corner.1, 0.0);
            Label { x: x as i32, y: (y + 2.0 * k) as i32, text: namespace.to_owned() }
        })
        .collect()
}

fn ground_at(layout: &Layout, tx: i32, ty: i32) -> Ground<'_> {
    if !layout::is_road(tx, ty) {
        return layout.block_at(tx, ty).map_or(Ground::Land, |block| Ground::Lot { namespace: &block.namespace });
    }
    let crosses_x = tx.rem_euclid(BLOCK_STRIDE) == BLOCK_SIDE;
    let crosses_y = ty.rem_euclid(BLOCK_STRIDE) == BLOCK_SIDE;
    Ground::Road(match (crosses_x, crosses_y) {
        (true, true) => RoadDirection::Crossing,
        (true, false) => RoadDirection::AlongY,
        _ => RoadDirection::AlongX,
    })
}

fn look_of(pod: &PodInfo) -> BuildingLook {
    BuildingLook {
        kind: pod.kind,
        state: pod.state,
        style: stable_hash(&format!("{}/{}", pod.namespace, pod.workload)),
        seed: stable_hash(&pod.uid),
    }
}

fn put_label(buf: &mut Buffer, area: Rect, label: &Label, ui: UiColours) {
    if label.y < 0 || label.y / 2 >= i32::from(area.height) {
        return;
    }
    let style = Style::new().fg(ui.label_fg.into()).bg(ui.label_bg.into());
    let row = area.y + (label.y / 2) as u16;
    let text: Vec<char> = label.text.chars().take(LABEL_MAX).collect();
    let start = label.x - text.len() as i32 / 2;
    for (i, ch) in text.into_iter().enumerate() {
        let col = start + i as i32;
        if (0..i32::from(area.width)).contains(&col)
            && let Some(cell) = buf.cell_mut((area.x + col as u16, row))
        {
            cell.set_char(ch).set_style(style);
        }
    }
}

fn draw_status(buf: &mut Buffer, bar: Rect, world: &World, ui: UiColours, view: View, source: &str) {
    let style = Style::new().fg(ui.status_fg.into()).bg(ui.status_bg.into());
    let left = format!(
        " kube-city · {source} · {} pods · {} namespaces",
        world.pod_count(),
        world.namespace_count()
    );
    let zoom = match view.zoom {
        Zoom::Detail => "z overview",
        Zoom::Overview => "z detail",
    };
    let right = format!("←↑↓→ pan · {zoom} · c centre · q quit ");
    buf.set_style(bar, style);
    buf.set_stringn(bar.x, bar.y, &left, usize::from(bar.width), style);
    let (left_len, right_len) = (left.chars().count(), right.chars().count());
    if left_len + 2 + right_len <= usize::from(bar.width) {
        buf.set_string(bar.right() - right_len as u16, bar.y, &right, style);
    }
}

#[cfg(test)]
mod tests {
    use ratatui::{Terminal, backend::TestBackend};

    use super::*;
    use crate::{
        demo::DemoCluster,
        render::theme::Metropolis,
        world::{
            PodState,
            tests::{delete, upsert},
        },
    };

    fn painted(world: &World, view: View) -> Canvas {
        let mut canvas = Canvas::new(160, 100);
        paint(world, view, &Metropolis, 0, &mut canvas);
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

    fn views() -> [View; 2] {
        [View::default(), View { zoom: Zoom::Overview, ..View::default() }]
    }

    #[test]
    fn a_pod_appears_and_disappears_from_the_city() {
        for view in views() {
            let mut world = World::default();
            let empty = painted(&world, view);
            world.apply(upsert("a", "shop", PodState::Running));
            assert_ne!(painted(&world, view), empty);
            world.apply(delete("a"));
            assert_eq!(painted(&world, view), empty);
        }
    }

    #[test]
    fn a_state_change_repaints_the_building() {
        let mut world = World::default();
        world.apply(upsert("a", "shop", PodState::Running));
        let running = painted(&world, View::default());
        world.apply(upsert("a", "shop", PodState::Pending));
        assert_ne!(painted(&world, View::default()), running);
    }

    #[test]
    fn painting_never_panics_at_odd_sizes_or_far_views() {
        let world = demo_world(400);
        for (w, h) in [(0, 0), (1, 1), (3, 2), (80, 48), (300, 160)] {
            for zoom in [Zoom::Detail, Zoom::Overview] {
                for (x, y) in [(0, 0), (-5000, 5000), (37, -11)] {
                    let mut canvas = Canvas::new(w, h);
                    paint(&world, View { x, y, zoom }, &Metropolis, 7, &mut canvas);
                }
            }
        }
    }

    #[test]
    fn toggling_zoom_twice_returns_to_the_same_view() {
        let mut view = View { x: 40, y: -24, zoom: Zoom::Detail };
        view.toggle_zoom();
        assert_eq!((view.x, view.y, view.zoom), (20, -12, Zoom::Overview));
        view.toggle_zoom();
        assert_eq!(view, View { x: 40, y: -24, zoom: Zoom::Detail });
    }

    #[test]
    fn the_status_bar_counts_pods_and_namespaces() {
        let mut world = World::default();
        world.apply(upsert("a", "shop", PodState::Running));
        world.apply(upsert("b", "bank", PodState::Pending));
        let mut terminal = Terminal::new(TestBackend::new(120, 40)).expect("test terminal");
        terminal.draw(|frame| draw(frame, &world, View::default(), &Metropolis, 0, "test")).expect("draw");
        let buf = terminal.backend().buffer();
        let row = |y: u16| (0..120).map(|x| buf[(x, y)].symbol()).collect::<String>();
        assert!(row(39).contains("test · 2 pods · 2 namespaces"), "status: {}", row(39));
        assert!(row(39).contains("z overview"), "status: {}", row(39));
        assert!((0..39).any(|y| row(y).contains("shop")), "district label is drawn");
    }

    #[test]
    fn tiny_terminals_draw_nothing_instead_of_panicking() {
        let world = demo_world(50);
        for (w, h) in [(1, 1), (10, 1), (1, 5)] {
            let mut terminal = Terminal::new(TestBackend::new(w, h)).expect("test terminal");
            terminal.draw(|frame| draw(frame, &world, View::default(), &Metropolis, 0, "demo")).expect("draw");
        }
    }
}
