//! Metropolis: daylight pixel art in the spirit of SimCity 2000.
//!
//! Deployments are office towers, StatefulSets storage tanks, DaemonSets
//! houses, Jobs factories and pods without an owner kiosks. Pending pods are
//! construction sites, crash loops burn, terminating buildings fade and
//! crumble. Animation frames are 100 ms apart.

use super::{BuildingLook, Ground, RoadDirection, Theme, UiColours};
use crate::{
    render::{
        canvas::Rgb,
        scene::{FacePixel, Footprint, Scene},
    },
    util::{mix, stable_hash, unit},
    world::{PodState, WorkloadKind},
};

const SKY: Rgb = Rgb(30, 36, 51);
const LAND: Rgb = Rgb(70, 104, 58);
const ROAD: Rgb = Rgb(74, 77, 85);
const ROAD_LINE: Rgb = Rgb(230, 200, 74);
const SIDEWALK: Rgb = Rgb(185, 180, 168);
/// Healthy buildings on the health map: present but quiet.
const MAP_CALM: Rgb = Rgb(120, 146, 136);
/// Lot ground per namespace, so neighbouring districts read apart.
const LOT_GROUNDS: [Rgb; 5] =
    [Rgb(111, 154, 79), Rgb(150, 144, 128), Rgb(176, 160, 124), Rgb(124, 138, 146), Rgb(92, 128, 86)];

const FACADES: [Rgb; 5] =
    [Rgb(216, 199, 163), Rgb(127, 163, 200), Rgb(178, 96, 74), Rgb(228, 226, 220), Rgb(111, 176, 173)];
/// Tiers of a tower step back from the lot edge by these insets.
const TIER_INSETS: [f32; 3] = [0.18, 0.28, 0.38];
const GLASS: Rgb = Rgb(61, 88, 120);
const LIT: Rgb = Rgb(246, 227, 154);
const SHOPFRONT: Rgb = Rgb(44, 52, 64);
const STEEL: Rgb = Rgb(154, 163, 173);
const ANTENNA: Rgb = Rgb(192, 198, 204);
const BEACON: Rgb = Rgb(255, 64, 64);
/// About a minute: how long an office window stays lit or dark.
const WINDOW_FRAMES: u64 = 600;

const EMBER: Rgb = Rgb(255, 138, 58);
const FLAMES: [Rgb; 3] = [Rgb(255, 224, 102), Rgb(255, 154, 46), Rgb(232, 69, 44)];
const SMOKE: (Rgb, Rgb) = (Rgb(74, 74, 80), Rgb(138, 138, 144));
const STEAM: (Rgb, Rgb) = (Rgb(232, 232, 236), Rgb(176, 176, 186));
const ASH: Rgb = Rgb(138, 138, 138);
const DUST: Rgb = Rgb(200, 184, 154);

const TANKS: [Rgb; 6] = [
    Rgb(184, 190, 198),
    Rgb(226, 224, 214),
    Rgb(150, 180, 140),
    Rgb(140, 170, 204),
    Rgb(212, 190, 150),
    Rgb(192, 124, 98),
];
const LADDER: Rgb = Rgb(92, 99, 108);

const HOUSE_WALLS: [Rgb; 4] = [Rgb(232, 220, 192), Rgb(214, 226, 236), Rgb(236, 214, 200), Rgb(220, 232, 206)];
const HOUSE_ROOFS: [Rgb; 3] = [Rgb(181, 81, 62), Rgb(96, 112, 140), Rgb(140, 96, 70)];
const DOOR: Rgb = Rgb(107, 74, 50);
const HOUSE_WINDOW: Rgb = Rgb(93, 132, 168);
const CHIMNEY: Rgb = Rgb(138, 74, 58);

const FACTORY: Rgb = Rgb(168, 154, 134);
const FACTORY_WINDOW: Rgb = Rgb(242, 196, 106);
const SAWTOOTH_GLASS: Rgb = Rgb(111, 122, 134);
const SAWTOOTH_ROOF: Rgb = Rgb(140, 151, 163);
const STACK: Rgb = Rgb(156, 90, 72);
const STACK_STRIPE: Rgb = Rgb(232, 232, 232);

const KIOSK: Rgb = Rgb(222, 214, 200);
const AWNING: [Rgb; 2] = [Rgb(214, 72, 60), Rgb(245, 240, 230)];

const CONCRETE: Rgb = Rgb(168, 168, 160);
const SCAFFOLD: Rgb = Rgb(240, 160, 48);
const CRANE: [Rgb; 2] = [Rgb(242, 210, 58), Rgb(184, 154, 32)];
const CABLE: Rgb = Rgb(106, 106, 106);

const PAVILION: Rgb = Rgb(122, 166, 200);
const SCORCHED: Rgb = Rgb(92, 76, 70);
const TICK: Rgb = Rgb(111, 224, 138);
const CROSS: Rgb = Rgb(232, 80, 64);

const TRUNK: Rgb = Rgb(107, 74, 46);
const LEAVES: [Rgb; 3] = [Rgb(63, 122, 53), Rgb(88, 160, 70), Rgb(124, 194, 90)];
/// Tree spots on a lot, back to front.
const GROVES: [[(f32, f32); 3]; 3] = [
    [(0.3, 0.35), (0.7, 0.4), (0.45, 0.72)],
    [(0.35, 0.3), (0.25, 0.7), (0.65, 0.65)],
    [(0.5, 0.3), (0.3, 0.6), (0.72, 0.62)],
];

#[derive(Debug, Default, Clone, Copy)]
pub struct Metropolis;

impl Theme for Metropolis {
    fn sky(&self) -> Rgb {
        SKY
    }

    fn ui(&self) -> UiColours {
        UiColours {
            label_fg: Rgb(240, 238, 230),
            label_bg: Rgb(40, 46, 62),
            status_fg: Rgb(210, 214, 224),
            status_bg: Rgb(22, 26, 38),
        }
    }

    fn max_height(&self) -> f32 {
        // The tallest tower, its antenna and the smoke above a fire.
        90.0
    }

    fn ground(&self, s: &mut Scene, tx: i32, ty: i32, ground: Ground) {
        let tile = Footprint::lot(tx, ty, 0.0);
        let (x, y) = (tx as f32, ty as f32);
        match ground {
            Ground::Land => s.flat(tile, 0.0, if (tx + ty) % 2 == 0 { LAND } else { LAND.shade(0.95) }),
            Ground::Road(direction) => {
                s.flat(tile, 0.0, ROAD);
                match direction {
                    RoadDirection::AlongX => s.line((x + 0.15, y + 0.5, 0.0), (x + 0.55, y + 0.5, 0.0), ROAD_LINE),
                    RoadDirection::AlongY => s.line((x + 0.5, y + 0.15, 0.0), (x + 0.5, y + 0.55, 0.0), ROAD_LINE),
                    RoadDirection::Crossing => {}
                }
            }
            Ground::Lot { namespace } => {
                s.flat(tile, 0.0, SIDEWALK);
                s.flat(tile.inset(0.125), 0.0, pick(&LOT_GROUNDS, stable_hash(namespace)));
            }
        }
    }

    fn building(&self, s: &mut Scene, tx: i32, ty: i32, look: &BuildingLook) {
        match look.state {
            PodState::Pending => construction_site(s, tx, ty, look),
            PodState::Succeeded => pavilion(s, tx, ty, true),
            PodState::Failed => pavilion(s, tx, ty, false),
            PodState::Running | PodState::CrashLoop | PodState::Terminating => match look.kind {
                WorkloadKind::Deployment => tower(s, tx, ty, look),
                WorkloadKind::StatefulSet => tank(s, tx, ty, look),
                WorkloadKind::DaemonSet => house(s, tx, ty, look),
                WorkloadKind::Job => factory(s, tx, ty, look),
                WorkloadKind::Standalone => kiosk(s, tx, ty, look),
            },
        }
    }

    fn map_colour(&self, look: &BuildingLook) -> Rgb {
        match look.state {
            PodState::Running => MAP_CALM,
            PodState::Pending => SCAFFOLD,
            PodState::CrashLoop => FLAMES[2],
            PodState::Failed => CROSS.shade(0.7),
            PodState::Terminating => ASH,
            PodState::Succeeded => CONCRETE,
        }
    }

    fn empty_lot(&self, s: &mut Scene, tx: i32, ty: i32, seed: u64) {
        // Some free lots are plain lawn, the rest get a few trees.
        if seed % 10 >= 6 {
            return;
        }
        let grove = GROVES[(seed >> 8) as usize % GROVES.len()];
        let count = 2 + (seed >> 16) as usize % 2;
        for (i, (dx, dy)) in grove.into_iter().take(count).enumerate() {
            tree(s, tx as f32 + dx, ty as f32 + dy, 1.0 - 0.12 * i as f32);
        }
    }
}

fn pick<T: Copy>(items: &[T], hash: u64) -> T {
    items[(hash % items.len() as u64) as usize]
}

/// Light comes from the right, so left walls are darker.
fn walls(colour: Rgb) -> (Rgb, Rgb) {
    (colour.shade(0.72), colour.shade(0.9))
}

fn solid(colour: Rgb) -> impl Fn(FacePixel) -> Rgb {
    move |_| colour
}

fn centre_of(f: Footprint) -> (f32, f32) {
    ((f.x0 + f.x1) / 2.0, (f.y0 + f.y1) / 2.0)
}

/// What the pod's state does to its building.
#[derive(Debug, Clone, Copy)]
struct Condition {
    burning: bool,
    leaving: bool,
}

impl Condition {
    fn of(state: PodState) -> Self {
        Self { burning: state == PodState::CrashLoop, leaving: state == PodState::Terminating }
    }

    /// A terminating building fades towards grey.
    fn tint(self, colour: Rgb) -> Rgb {
        if self.leaving { colour.mix(ASH, 0.6) } else { colour }
    }

    /// Fire on a burning building, dust over a crumbling one.
    fn aftermath(self, s: &mut Scene, at: (f32, f32, f32), seed: u64) {
        if self.burning {
            fire(s, at, seed);
        } else if self.leaving {
            dust(s, at, seed);
        }
    }
}

/// Where window panes go on a wall, in screen pixels.
#[derive(Debug, Clone, Copy)]
struct WindowGrid {
    floor: i32,
    pane: i32,
    period: i32,
    width: i32,
    bottom: i32,
    top: i32,
}

impl WindowGrid {
    fn office(s: &Scene) -> Self {
        if s.detailed() {
            Self { floor: 4, pane: 2, period: 3, width: 2, bottom: 3, top: 3 }
        } else {
            Self { floor: 2, pane: 1, period: 2, width: 1, bottom: 2, top: 1 }
        }
    }

    fn factory(s: &Scene) -> Self {
        if s.detailed() {
            Self { floor: 5, pane: 2, period: 4, width: 2, bottom: 3, top: 2 }
        } else {
            Self { floor: 3, pane: 1, period: 2, width: 1, bottom: 2, top: 1 }
        }
    }

    /// The pane's column and row when this pixel is glass.
    fn pane(self, p: FacePixel) -> Option<(i32, i32)> {
        let (z, i) = (p.z - self.bottom, p.col - 1);
        let glass = z >= 0
            && p.z < p.height - self.top
            && z % self.floor < self.pane
            && i >= 0
            && p.col < p.cols - 1
            && i % self.period < self.width;
        glass.then_some((i / self.period, z / self.floor))
    }
}

/// Office windows switch on and off now and then, each on its own clock;
/// in a crash loop some glow with the fire inside.
fn office_window(seed: u64, wall: u64, (col, row): (i32, i32), state: PodState, frame: u64) -> Rgb {
    let pane = mix(&[seed, wall, col as u64, row as u64]);
    match state {
        PodState::Running => {
            let shift = (frame + pane % WINDOW_FRAMES) / WINDOW_FRAMES;
            if mix(&[pane, shift]) % 100 < 25 { LIT } else { GLASS }
        }
        PodState::CrashLoop if pane % 100 < 30 => {
            if (frame + pane) % 4 < 2 {
                EMBER
            } else {
                FLAMES[1]
            }
        }
        _ => GLASS.shade(0.6),
    }
}

fn tower(s: &mut Scene, tx: i32, ty: i32, look: &BuildingLook) {
    let condition = Condition::of(look.state);
    let facade = condition.tint(pick(&FACADES, look.style));
    let full_height = 20.0 + ((look.style >> 8) % 25) as f32;
    let tiers = if full_height >= 28.0 { 1 + ((look.style >> 16) % 3) as usize } else { 1 };
    // A terminating tower is already partly demolished.
    let height = if condition.leaving { (full_height * 0.7).round() } else { full_height };
    let grid = WindowGrid::office(s);
    let (frame, detailed) = (s.frame(), s.detailed());
    let (left_wall, right_wall) = walls(facade);

    let mut z0 = 0.0;
    let mut roof = Footprint::lot(tx, ty, TIER_INSETS[0]);
    for (tier, &inset) in TIER_INSETS.iter().enumerate().take(tiers) {
        let z1 = if tier + 1 == tiers { height } else { (height * (0.55 + 0.2 * tier as f32)).round() };
        roof = Footprint::lot(tx, ty, inset);
        let shopfront = tier == 0 && detailed;
        let wall = |base: Rgb, side: u64| {
            move |p: FacePixel| {
                if shopfront && p.z < 3 && p.col % 3 != 0 {
                    return SHOPFRONT;
                }
                grid.pane(p).map_or(base, |pane| office_window(look.seed, side, pane, look.state, frame))
            }
        };
        let tier_id = 2 * tier as u64;
        s.cuboid(roof, z0, z1, facade.shade(1.08), wall(left_wall, tier_id), wall(right_wall, tier_id + 1));
        if detailed {
            // Parapet and roof gravel.
            s.rim(roof, z1, facade.shade(1.25));
            s.flat(roof.inset(0.06), z1, facade.shade(0.95));
        }
        s.line((roof.x1, roof.y1, z0), (roof.x1, roof.y1, z1), facade.shade(1.15));
        z0 = z1;
    }

    let centre = centre_of(roof);
    if !condition.leaving {
        if tiers >= 2 && detailed {
            let unit = Footprint::new(centre.0 + 0.02, centre.1 - 0.1, centre.0 + 0.14, centre.1 + 0.02);
            s.cuboid(unit, height, height + 4.0, STEEL.shade(1.05), solid(STEEL.shade(0.72)), solid(STEEL.shade(0.88)));
        }
        if tiers == 3 {
            let base = s.project(centre.0, centre.1, height);
            let tip = s.project(centre.0, centre.1, height + 10.0);
            s.screen_line(base, tip, ANTENNA);
            if frame / 5 % 2 == 0 {
                s.dot((tip.0, tip.1 - 1.0), BEACON);
            }
        }
    }
    condition.aftermath(s, (centre.0, centre.1, height), look.seed);
}

/// StatefulSets: a storage tank, twin tanks or a tall silo, coloured per workload.
fn tank(s: &mut Scene, tx: i32, ty: i32, look: &BuildingLook) {
    let condition = Condition::of(look.state);
    let colour = condition.tint(pick(&TANKS, look.style >> 4));
    let height = 16.0 + ((look.style >> 8) % 8) as f32;
    let (x, y) = (tx as f32, ty as f32);
    let top = match (look.style >> 20) % 3 {
        0 => {
            cylinder(s, (x + 0.5, y + 0.5), 0.36, height, colour, Roof::Dome);
            if s.detailed() {
                s.line((x + 0.62, y + 0.86, 1.0), (x + 0.62, y + 0.86, height - 2.0), LADDER);
            }
            (x + 0.5, y + 0.5, height + 4.0)
        }
        1 => {
            cylinder(s, (x + 0.32, y + 0.32), 0.2, height, colour, Roof::Dome);
            cylinder(s, (x + 0.68, y + 0.68), 0.2, height - 5.0, colour, Roof::Dome);
            (x + 0.32, y + 0.32, height + 4.0)
        }
        _ => {
            cylinder(s, (x + 0.5, y + 0.5), 0.26, height + 10.0, colour, Roof::Cone);
            (x + 0.5, y + 0.5, height + 16.0)
        }
    };
    condition.aftermath(s, top, look.seed);
}

#[derive(Debug, Clone, Copy)]
enum Roof {
    Dome,
    Cone,
}

/// A banded cylinder lit from the right, with a dome or a cone on top.
fn cylinder(s: &mut Scene, centre: (f32, f32), radius: f32, height: f32, colour: Rgb, roof: Roof) {
    let (band, cap) = (colour.shade(0.78), colour.shade(1.15));
    let lighting = |t: f32| 0.7 + 0.35 * (t + 1.0) / 2.0;
    s.revolve(
        centre,
        0.0,
        height,
        |_| radius,
        |t, z| (if z as i32 % 6 >= 4 { band } else { colour }).shade(lighting(t)),
        None,
    );
    let (rise, profile): (f32, fn(f32) -> f32) = match roof {
        Roof::Dome => (4.0, |f| (1.0 - f * f).max(0.0).sqrt()),
        Roof::Cone => (6.0, |f| 1.0 - f),
    };
    s.revolve(
        centre,
        height,
        height + rise,
        |z| radius * profile((z - height) / rise),
        |t, _| cap.shade(0.85 + 0.25 * (t + 1.0) / 2.0),
        None,
    );
}

fn house(s: &mut Scene, tx: i32, ty: i32, look: &BuildingLook) {
    let condition = Condition::of(look.state);
    let wall = condition.tint(pick(&HOUSE_WALLS, look.style >> 4));
    let roof = condition.tint(pick(&HOUSE_ROOFS, look.style));
    let window = if condition.burning { EMBER } else { HOUSE_WINDOW };
    let (x, y) = (tx as f32, ty as f32);
    let f = Footprint::new(x + 0.2, y + 0.25, x + 0.8, y + 0.75);
    let (left_wall, right_wall) = walls(wall);
    let detailed = s.detailed();
    let front = move |p: FacePixel| {
        let door = detailed && (p.cols / 2 - 1..=p.cols / 2).contains(&p.col) && p.z < 4;
        let pane = detailed && (p.col == 1 || p.col == p.cols - 2) && (3..=4).contains(&p.z);
        if door {
            DOOR
        } else if pane {
            window
        } else {
            left_wall
        }
    };
    let side = move |p: FacePixel| {
        if detailed && (1..=2).contains(&p.col) && (3..=4).contains(&p.z) { window } else { right_wall }
    };
    s.cuboid(f, 0.0, 7.0, wall, front, side);
    s.gable(f.inset(-0.04), 7.0, 6.0, roof, roof.shade(0.75), wall.shade(0.95));
    let chimney = Footprint::new(f.x0 + 0.1, f.y0 + 0.06, f.x0 + 0.2, f.y0 + 0.16);
    s.cuboid(chimney, 9.0, 13.0, CHIMNEY.shade(1.1), solid(CHIMNEY.shade(0.8)), solid(CHIMNEY));
    let (cx, cy) = centre_of(f);
    condition.aftermath(s, (cx, cy, 13.0), look.seed);
}

fn factory(s: &mut Scene, tx: i32, ty: i32, look: &BuildingLook) {
    let condition = Condition::of(look.state);
    let wall = condition.tint(FACTORY);
    let (x, y) = (tx as f32, ty as f32);
    let f = Footprint::new(x + 0.12, y + 0.18, x + 0.88, y + 0.82);
    let grid = WindowGrid::factory(s);
    let running = look.state == PodState::Running;
    let pane = if condition.burning {
        EMBER
    } else if running {
        FACTORY_WINDOW
    } else {
        GLASS.shade(0.6)
    };
    let (left_wall, right_wall) = walls(wall);
    s.cuboid(f, 0.0, 10.0, wall, move |p| grid.pane(p).map_or(left_wall, |_| pane), solid(right_wall));

    // Sawtooth roof: each tooth rises along x and ends in a glass wall.
    let (roof, glass) = (condition.tint(SAWTOOTH_ROOF), condition.tint(SAWTOOTH_GLASS));
    let step = (f.x1 - f.x0) / 3.0;
    for tooth in 0..3 {
        let x0 = f.x0 + step * tooth as f32;
        let x1 = x0 + step;
        s.polygon(&[(x0, f.y0, 10.0), (x0, f.y1, 10.0), (x1, f.y1, 14.0), (x1, f.y0, 14.0)], roof);
        s.polygon(&[(x0, f.y1, 10.0), (x1, f.y1, 10.0), (x1, f.y1, 14.0)], left_wall);
        s.polygon(&[(x1, f.y0, 10.0), (x1, f.y1, 10.0), (x1, f.y1, 14.0), (x1, f.y0, 14.0)], glass);
    }

    let stack = (x + 0.78, y + 0.3);
    let (brick, stripe) = (condition.tint(STACK), condition.tint(STACK_STRIPE));
    s.revolve(
        stack,
        12.0,
        26.0,
        |_| 0.07,
        |t, z| (if z >= 22.0 { stripe } else { brick }).shade(0.75 + 0.3 * (t + 1.0) / 2.0),
        Some(SMOKE.0),
    );
    if running {
        let top = s.project(stack.0, stack.1, 26.0);
        smoke(s, (top.0, top.1 - 1.0), look.seed, STEAM, 22);
    }
    condition.aftermath(s, (x + 0.4, y + 0.5, 14.0), look.seed);
}

fn kiosk(s: &mut Scene, tx: i32, ty: i32, look: &BuildingLook) {
    let condition = Condition::of(look.state);
    let wall = condition.tint(KIOSK);
    let (x, y) = (tx as f32, ty as f32);
    let f = Footprint::new(x + 0.28, y + 0.3, x + 0.72, y + 0.7);
    let (left_wall, right_wall) = walls(wall);
    let detailed = s.detailed();
    let front = move |p: FacePixel| {
        if p.z >= p.height - 2 {
            AWNING[(p.col / 2 % 2) as usize]
        } else if detailed && p.z < 4 && p.col % 3 != 0 {
            SHOPFRONT
        } else {
            left_wall
        }
    };
    s.cuboid(f, 0.0, 7.0, wall.shade(1.05), front, solid(right_wall));
    let (cx, cy) = centre_of(f);
    condition.aftermath(s, (cx, cy, 7.0), look.seed);
}

/// Any pending pod: a slab, scaffolding and a tower crane at work.
fn construction_site(s: &mut Scene, tx: i32, ty: i32, look: &BuildingLook) {
    let height = 14.0 + ((look.style >> 8) % 6) as f32;
    let (x, y) = (tx as f32, ty as f32);
    s.cuboid(
        Footprint::lot(tx, ty, 0.15),
        0.0,
        4.0,
        CONCRETE,
        solid(CONCRETE.shade(0.76)),
        solid(CONCRETE.shade(0.88)),
    );
    let frame = Footprint::lot(tx, ty, 0.12);
    let mut level = 8.0;
    while level <= height {
        s.rim(frame, level, SCAFFOLD);
        level += 4.0;
    }
    for (px, py) in [(frame.x0, frame.y1), (frame.x1, frame.y1), (frame.x1, frame.y0)] {
        s.line((px, py, 0.0), (px, py, height), SCAFFOLD.shade(0.8));
    }

    let k = s.scale();
    let (mx, my) = s.project(x + 0.3, y + 0.3, 0.0);
    let top = height + 16.0;
    for z in 0..(top * k) as i32 {
        s.dot((mx, my - z as f32), CRANE[(z % 2) as usize]);
    }
    let jib = my - top * k;
    s.screen_line((mx - 6.0 * k, jib), (mx + 14.0 * k, jib), CRANE[0]);
    s.dot((mx - 6.0 * k, jib + 1.0), CABLE);
    // The trolley runs out along the jib and back.
    let phase = (s.frame() / 4 % 16) as f32;
    let out = if phase < 8.0 { phase } else { 16.0 - phase };
    let hook = mx + (4.0 + out) * k;
    s.screen_line((hook, jib), (hook, jib + 9.0 * k), CABLE);
    s.dot((hook, jib + 10.0 * k), CABLE.shade(0.6));
}

/// What a finished job leaves: a pavilion with a tick, or a scorched one with a cross.
fn pavilion(s: &mut Scene, tx: i32, ty: i32, succeeded: bool) {
    let slab = Footprint::lot(tx, ty, 0.2);
    s.cuboid(slab, 0.0, 2.0, CONCRETE.shade(1.12), solid(CONCRETE.shade(0.85)), solid(CONCRETE));
    let (body, mark) = if succeeded { (PAVILION, TICK) } else { (SCORCHED, CROSS) };
    s.cuboid(Footprint::lot(tx, ty, 0.35), 2.0, 6.0, body.shade(1.1), solid(body.shade(0.65)), solid(body.shade(0.82)));
    if s.detailed() {
        let (cx, cy) = s.project(tx as f32 + 0.5, ty as f32 + 0.5, 6.0);
        let shape: &[(f32, f32)] = if succeeded {
            &[(-2.0, 0.0), (-1.0, 1.0), (0.0, 0.0), (1.0, -1.0), (2.0, -2.0)]
        } else {
            &[(-1.0, -1.0), (1.0, -1.0), (0.0, 0.0), (-1.0, 1.0), (1.0, 1.0)]
        };
        for (dx, dy) in shape {
            s.dot((cx + dx, cy - 4.0 + dy), mark);
        }
    }
}

fn tree(s: &mut Scene, x: f32, y: f32, size: f32) {
    let k = s.scale();
    let (bx, by) = s.project(x, y, 0.0);
    s.screen_line((bx, by), (bx, by - 3.0 * k), TRUNK);
    s.ellipse((bx, by - 5.5 * k), 4.2 * size * k, 3.4 * size * k, |t| {
        if t < -0.2 {
            LEAVES[0]
        } else if t < 0.5 {
            LEAVES[1]
        } else {
            LEAVES[2]
        }
    });
}

/// Flickering flames with smoke rising from them.
fn fire(s: &mut Scene, at: (f32, f32, f32), seed: u64) {
    let (cx, cy) = s.project(at.0, at.1, at.2);
    let (k, frame) = (s.scale(), s.frame());
    for i in 0..(40.0 * k) as u64 {
        let h = mix(&[seed, frame, i]);
        let dx = (unit(h) + unit(h.rotate_left(21)) - 1.0) * 4.0 * k;
        let rise = unit(h.rotate_left(42)).powi(2) * 7.0 * k;
        s.dot((cx + dx, cy - rise), FLAMES[((rise / (2.4 * k)) as usize).min(2)]);
    }
    smoke(s, (cx + k, cy - 6.0 * k), seed, SMOKE, 26);
}

/// A column of particles drifting up and to the right, each on its own phase.
fn smoke(s: &mut Scene, from: (f32, f32), seed: u64, (young, old): (Rgb, Rgb), count: u64) {
    const LIFE: u64 = 30;
    let (k, frame) = (s.scale(), s.frame());
    for i in 0..(count as f32 * k) as u64 {
        let h = mix(&[seed, i]);
        let age = (frame + h % LIFE) % LIFE;
        let t = age as f32 / LIFE as f32;
        let wobble = unit(mix(&[h, (frame + h % 7) / 4])) - 0.5;
        s.dot((from.0 + (t * 8.0 + wobble * 1.5) * k, from.1 - t * 18.0 * k), young.mix(old, t));
    }
}

/// Dust swirling over a building being torn down.
fn dust(s: &mut Scene, at: (f32, f32, f32), seed: u64) {
    let (cx, cy) = s.project(at.0, at.1, at.2);
    let k = s.scale();
    for i in 0..(16.0 * k) as u64 {
        let h = mix(&[seed, i, s.frame() / 3]);
        s.dot((cx + (unit(h) - 0.5) * 12.0 * k, cy - unit(h.rotate_left(30)) * 4.0 * k), DUST);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::render::{
        canvas::Canvas,
        scene::{Scene, Zoom},
    };

    const KINDS: [WorkloadKind; 5] = [
        WorkloadKind::Deployment,
        WorkloadKind::StatefulSet,
        WorkloadKind::DaemonSet,
        WorkloadKind::Job,
        WorkloadKind::Standalone,
    ];
    const STATES: [PodState; 6] = [
        PodState::Pending,
        PodState::Running,
        PodState::CrashLoop,
        PodState::Terminating,
        PodState::Succeeded,
        PodState::Failed,
    ];

    fn painted(zoom: Zoom, frame: u64, draw: impl FnOnce(&mut Scene)) -> Canvas {
        let mut canvas = Canvas::new(64, 96);
        canvas.fill(SKY);
        draw(&mut Scene::new(&mut canvas, zoom, (32, 60), frame));
        canvas
    }

    fn look(kind: WorkloadKind, state: PodState, style: u64) -> BuildingLook {
        BuildingLook { kind, state, style, seed: 99 }
    }

    #[test]
    fn every_kind_and_state_draws_something_at_both_zooms() {
        let empty = |zoom| painted(zoom, 0, |_| {});
        for zoom in [Zoom::Detail, Zoom::Overview] {
            for kind in KINDS {
                for state in STATES {
                    let canvas = painted(zoom, 0, |s| Metropolis.building(s, 0, 0, &look(kind, state, 7)));
                    assert_ne!(canvas, empty(zoom), "{kind:?} {state:?} at {zoom:?}");
                }
            }
        }
    }

    #[test]
    fn a_workloads_pods_look_alike_and_kinds_look_different() {
        let draw = |kind, style| {
            painted(Zoom::Detail, 0, |s| Metropolis.building(s, 0, 0, &look(kind, PodState::Pending, style)))
        };
        // Construction sites only depend on the workload, not on the pod.
        assert_eq!(draw(WorkloadKind::Deployment, 3), draw(WorkloadKind::Deployment, 3));
        let tower = painted(Zoom::Detail, 0, |s| {
            Metropolis.building(s, 0, 0, &look(WorkloadKind::Deployment, PodState::Running, 3))
        });
        let tank = painted(Zoom::Detail, 0, |s| {
            Metropolis.building(s, 0, 0, &look(WorkloadKind::StatefulSet, PodState::Running, 3))
        });
        assert_ne!(tower, tank);
    }

    #[test]
    fn fire_moves_between_frames() {
        let burning = |frame| {
            painted(Zoom::Detail, frame, |s| {
                Metropolis.building(s, 0, 0, &look(WorkloadKind::Deployment, PodState::CrashLoop, 3))
            })
        };
        assert_ne!(burning(0), burning(1));
    }

    #[test]
    fn office_windows_stay_put_between_nearby_frames() {
        let lit = |frame| office_window(1, 0, (2, 3), PodState::Running, frame);
        let changes = (0..WINDOW_FRAMES * 3).filter(|&f| lit(f) != lit(f + 1)).count();
        assert!(changes <= 3, "a window changes at most once a minute, changed {changes} times");
    }

    #[test]
    fn window_grid_leaves_wall_around_the_panes() {
        let grid = WindowGrid { floor: 4, pane: 2, period: 3, width: 2, bottom: 3, top: 3 };
        let at = |col, z| grid.pane(FacePixel { col, cols: 10, z, height: 20 });
        assert_eq!(at(0, 3), None, "edge column");
        assert_eq!(at(9, 3), None, "edge column");
        assert_eq!(at(1, 2), None, "below the first floor");
        assert_eq!(at(1, 3), Some((0, 0)));
        assert_eq!(at(3, 3), None, "gap between panes");
        assert_eq!(at(4, 7), Some((1, 1)));
        assert_eq!(at(1, 17), None, "below the roof line");
    }
}
