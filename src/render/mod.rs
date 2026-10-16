//! Draws the world as an isometric city.
//!
//! The city is painted on a pixel canvas, then packed into the terminal two
//! pixels per cell with the upper half block `▀`. Those pixels are roughly
//! square, so tiles get the classic 2:1 isometric diamond shape. What stands
//! where comes from the world's layout; how it looks comes from the theme.

mod canvas;
#[cfg(feature = "sixel")]
pub mod pixels;
mod scene;
pub mod theme;

use ratatui::{Frame, buffer::Buffer, layout::Rect, style::Style};

pub use canvas::Canvas;
pub use scene::Zoom;

use self::{
    scene::{Footprint, Scene},
    theme::{BuildingLook, Ground, RoadDirection, Theme, UiColours},
};
use crate::{
    layout::{self, BLOCK_SIDE, BLOCK_STRIDE, Layout},
    util::{mix, stable_hash},
    world::{PodInfo, PodState, World},
};

const LABEL_MAX: usize = 18;
/// How far focus mode fades healthy buildings into the background.
const FOCUS_FADE: f32 = 0.7;

/// What the screen shows: a pixel offset from the city centre, a zoom, and
/// whether healthy buildings fade so problems stand out.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct View {
    pub x: i32,
    pub y: i32,
    pub zoom: Zoom,
    pub focus: bool,
}

impl View {
    pub fn pan(&mut self, dx: i32, dy: i32) {
        self.x += dx;
        self.y += dy;
    }

    /// Moves to the next zoom level, keeping the same spot in the middle.
    pub fn next_zoom(&mut self) {
        let next = self.zoom.next();
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
    paint_magnified(world, view, theme, anim, canvas, 1.0)
}

/// [`paint`] at `density` canvas pixels per half-block pixel, for outputs with
/// real pixels; the view's offsets stay in half-block pixels.
pub fn paint_magnified(
    world: &World,
    view: View,
    theme: &dyn Theme,
    anim: u64,
    canvas: &mut Canvas,
    density: f32,
) -> Vec<Label> {
    canvas.fill(theme.sky());
    let k = view.zoom.scale() * density;
    // The centre of the first block sits a little below the middle of the screen.
    let origin = (
        canvas.width() / 2 + (view.x as f32 * density) as i32,
        canvas.height() / 2 + (view.y as f32 * density) as i32 - (8.0 * k) as i32,
    );
    let mut scene = Scene::new(canvas, view.zoom, origin, anim).magnified(density);
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
    let fade = Some((theme.sky(), FOCUS_FADE));
    for ((tx, ty), pod) in things {
        if !scene.visible(tx, ty, reach) {
            continue;
        }
        let calm = pod.is_none_or(|pod| pod.state == PodState::Running);
        scene.set_mute(if view.focus && calm { fade } else { None });
        match (pod, view.zoom) {
            (Some(pod), Zoom::Map) => {
                scene.flat(Footprint::lot(tx, ty, 0.1), 0.0, theme.map_colour(&look_of(pod)));
            }
            (Some(pod), _) => theme.building(&mut scene, tx, ty, &look_of(pod)),
            (None, Zoom::Map) => {}
            (None, _) => theme.empty_lot(&mut scene, tx, ty, mix(&[tx as u64, ty as u64])),
        }
    }
    scene.set_mute(None);

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
        // By name only, so a component looks the same in every namespace.
        style: stable_hash(&pod.workload),
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
    let mut left = format!(
        " kube-city · {source} · {} pods · {} namespaces",
        world.pod_count(),
        world.namespace_count()
    );
    if world.hidden_count() > 0 {
        left.push_str(&format!(" · {} finished hidden", world.hidden_count()));
    }
    let zoom = match view.zoom.next() {
        Zoom::Detail => "z detail",
        Zoom::Overview => "z overview",
        Zoom::Map => "z map",
    };
    let focus = if view.focus { "f all" } else { "f focus" };
    let right = format!("←↑↓→ pan · {zoom} · {focus} · c centre · q quit ");
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

    fn views() -> [View; 4] {
        [
            View::default(),
            View { zoom: Zoom::Overview, ..View::default() },
            View { zoom: Zoom::Map, ..View::default() },
            View { focus: true, ..View::default() },
        ]
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
            for zoom in [Zoom::Detail, Zoom::Overview, Zoom::Map] {
                for (x, y) in [(0, 0), (-5000, 5000), (37, -11)] {
                    let mut canvas = Canvas::new(w, h);
                    paint(&world, View { x, y, zoom, focus: x == 0 }, &Metropolis, 7, &mut canvas);
                }
            }
        }
    }

    #[test]
    fn zoom_cycles_through_three_levels_keeping_the_spot() {
        let mut view = View { x: 40, y: -24, ..View::default() };
        view.next_zoom();
        assert_eq!((view.x, view.y, view.zoom), (20, -12, Zoom::Overview));
        view.next_zoom();
        assert_eq!((view.x, view.y, view.zoom), (10, -6, Zoom::Map));
        view.next_zoom();
        assert_eq!(view, View { x: 40, y: -24, ..View::default() });
    }

    #[test]
    fn focus_fades_healthy_buildings_but_not_broken_ones() {
        let mut world = World::default();
        world.apply(upsert("a", "shop", PodState::Running));
        let focused = View { focus: true, ..View::default() };
        assert_ne!(painted(&world, focused), painted(&world, View::default()));
        world.apply(upsert("a", "shop", PodState::CrashLoop));
        let empty_city = World::default();
        // Only the lot's trees fade; the burning building itself is untouched.
        assert_ne!(painted(&world, focused), painted(&empty_city, focused));
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
        assert!(row(39).contains("z overview · f focus"), "status: {}", row(39));
        assert!((0..39).any(|y| row(y).contains("shop")), "district label is drawn");
    }

    /// A design tool: renders a recording as PPM images, one per view, at the
    /// pixel size of a 200×55 terminal (times `KUBE_CITY_DENSITY`, if set).
    ///
    /// `KUBE_CITY_REPLAY=rec.jsonl KUBE_CITY_OUT=dir cargo test render_snapshots -- --ignored`
    #[test]
    #[ignore = "design tool: needs KUBE_CITY_REPLAY and KUBE_CITY_OUT"]
    fn render_snapshots() {
        let replay = std::env::var("KUBE_CITY_REPLAY").expect("KUBE_CITY_REPLAY names a recording");
        let out = std::path::PathBuf::from(std::env::var("KUBE_CITY_OUT").expect("KUBE_CITY_OUT names a folder"));
        let views = [
            ("detail", View::default()),
            ("overview", View { zoom: Zoom::Overview, ..View::default() }),
            ("map", View { zoom: Zoom::Map, ..View::default() }),
            ("focus", View { zoom: Zoom::Overview, focus: true, ..View::default() }),
        ];
        let rules = crate::world::Rules {
            hide_completed: std::env::var_os("KUBE_CITY_PROTOTYPE").is_some(),
            group_daemonsets: std::env::var_os("KUBE_CITY_PROTOTYPE").is_some(),
        };
        let mut world = World::new(rules);
        for (_, event) in crate::recording::load(std::path::Path::new(&replay)).expect("recording loads") {
            world.apply(event);
        }
        // KUBE_CITY_DENSITY=10 renders at the real pixel size of a terminal with 10×20 cells.
        let density: i32 = std::env::var("KUBE_CITY_DENSITY").map_or(1, |d| d.parse().expect("a whole number"));
        for (name, view) in views {
            let mut canvas = Canvas::new(200 * density, 110 * density);
            paint_magnified(&world, view, &Metropolis, 12, &mut canvas, density as f32);
            std::fs::write(out.join(format!("{name}.ppm")), canvas.to_ppm()).expect("write snapshot");
        }
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
