//! Themes decide how the city looks: colours, ground and building art.
//!
//! The renderer only decides what stands where and in which order things are
//! drawn; a theme paints each tile and building with the [`Scene`] primitives.
//! A new look is a new [`Theme`] implementation.

mod metropolis;

pub use metropolis::Metropolis;

use super::{canvas::Rgb, scene::Scene};
use crate::world::{PodState, WorkloadKind};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RoadDirection {
    AlongX,
    AlongY,
    Crossing,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Ground<'a> {
    /// The island around the districts.
    Land,
    Road(RoadDirection),
    /// A lot in a namespace's block, built on or not.
    Lot {
        namespace: &'a str,
    },
}

/// What a theme needs to know to draw one pod's building.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BuildingLook {
    pub kind: WorkloadKind,
    pub state: PodState,
    /// The same for every pod of a workload: picks colours and proportions.
    pub style: u64,
    /// Different for every pod: varies small details like lit windows.
    pub seed: u64,
}

/// Colours for the text kube-city writes over the city.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct UiColours {
    pub label_fg: Rgb,
    pub label_bg: Rgb,
    pub status_fg: Rgb,
    pub status_bg: Rgb,
}

pub trait Theme {
    /// The background around the island.
    fn sky(&self) -> Rgb;

    fn ui(&self) -> UiColours;

    /// The tallest the theme draws, smoke included, in world pixels; things
    /// further than this off screen are skipped.
    fn max_height(&self) -> f32;

    fn ground(&self, scene: &mut Scene, tx: i32, ty: i32, ground: Ground);

    /// A pod's building on tile (tx, ty).
    fn building(&self, scene: &mut Scene, tx: i32, ty: i32, look: &BuildingLook);

    /// A building's colour on the health map: calm when fine, loud when not.
    fn map_colour(&self, look: &BuildingLook) -> Rgb;

    /// Decoration for a free lot in a namespace's block; `seed` is stable per lot.
    fn empty_lot(&self, scene: &mut Scene, tx: i32, ty: i32, seed: u64);
}
