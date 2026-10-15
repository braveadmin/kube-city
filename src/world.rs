//! The city's model: which pods exist and where their buildings stand.
//!
//! Sources (the demo simulation, later the cluster watchers) only ever send
//! [`WorldEvent`]s; the world turns them into buildings on stable lots.

use std::collections::HashMap;

use crate::layout::{Layout, Lot};

/// What a pod is doing, reduced to what the city can show.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PodState {
    Pending,
    Running,
    CrashLoop,
    Terminating,
    Succeeded,
    Failed,
}

/// What kind of workload owns a pod; it decides the type of building.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WorkloadKind {
    Deployment,
    StatefulSet,
    DaemonSet,
    Job,
    /// A pod nothing owns.
    Standalone,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PodInfo {
    pub uid: String,
    pub namespace: String,
    /// The owning workload (e.g. a Deployment); its pods share one look.
    pub workload: String,
    pub kind: WorkloadKind,
    pub state: PodState,
}

/// A change reported by a source.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WorldEvent {
    PodUpserted(PodInfo),
    PodDeleted { uid: String },
}

#[derive(Debug)]
pub struct Building {
    pub pod: PodInfo,
    pub lot: Lot,
}

#[derive(Debug, Default)]
pub struct World {
    buildings: HashMap<String, Building>,
    layout: Layout,
}

impl World {
    pub fn apply(&mut self, event: WorldEvent) {
        match event {
            WorldEvent::PodUpserted(pod) => match self.buildings.get_mut(&pod.uid) {
                Some(building) => building.pod = pod,
                None => {
                    let lot = self.layout.place(&pod.namespace);
                    self.buildings.insert(pod.uid.clone(), Building { pod, lot });
                }
            },
            WorldEvent::PodDeleted { uid } => {
                if let Some(building) = self.buildings.remove(&uid) {
                    self.layout.release(building.lot);
                }
            }
        }
    }

    pub fn buildings(&self) -> impl Iterator<Item = &Building> {
        self.buildings.values()
    }

    pub fn layout(&self) -> &Layout {
        &self.layout
    }

    pub fn pod_count(&self) -> usize {
        self.buildings.len()
    }

    pub fn namespace_count(&self) -> usize {
        self.layout.districts().count()
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    pub(crate) fn upsert(uid: &str, namespace: &str, state: PodState) -> WorldEvent {
        WorldEvent::PodUpserted(PodInfo {
            uid: uid.to_owned(),
            namespace: namespace.to_owned(),
            workload: "web".to_owned(),
            kind: WorkloadKind::Deployment,
            state,
        })
    }

    pub(crate) fn delete(uid: &str) -> WorldEvent {
        WorldEvent::PodDeleted { uid: uid.to_owned() }
    }

    fn lot_of(world: &World, uid: &str) -> Lot {
        world.buildings.get(uid).expect("building exists").lot
    }

    #[test]
    fn a_new_pod_becomes_a_building() {
        let mut world = World::default();
        world.apply(upsert("a", "shop", PodState::Pending));
        assert_eq!(world.pod_count(), 1);
        assert_eq!(world.namespace_count(), 1);
    }

    #[test]
    fn a_state_change_keeps_the_building_in_place() {
        let mut world = World::default();
        world.apply(upsert("a", "shop", PodState::Pending));
        world.apply(upsert("b", "shop", PodState::Pending));
        let before = lot_of(&world, "b");
        world.apply(upsert("b", "shop", PodState::Running));
        assert_eq!(lot_of(&world, "b"), before);
        assert_eq!(world.buildings["b"].pod.state, PodState::Running);
    }

    #[test]
    fn a_deleted_pod_disappears_and_frees_its_lot() {
        let mut world = World::default();
        world.apply(upsert("a", "shop", PodState::Running));
        let lot = lot_of(&world, "a");
        world.apply(delete("a"));
        assert_eq!(world.pod_count(), 0);
        assert_eq!(world.namespace_count(), 0);
        world.apply(upsert("b", "shop", PodState::Running));
        assert_eq!(lot_of(&world, "b"), lot);
    }

    #[test]
    fn deleting_an_unknown_pod_is_ignored() {
        let mut world = World::default();
        world.apply(upsert("a", "shop", PodState::Running));
        world.apply(delete("nope"));
        assert_eq!(world.pod_count(), 1);
    }
}
