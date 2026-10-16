//! The city's model: which pods exist and where their buildings stand.
//!
//! Sources (the demo simulation, later the cluster watchers) only ever send
//! [`WorldEvent`]s; the world turns them into buildings on stable lots.

use std::collections::{HashMap, HashSet};

use serde::{Deserialize, Serialize};

use crate::layout::{Layout, Lot};

/// What a pod is doing, reduced to what the city can show.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PodState {
    Pending,
    Running,
    CrashLoop,
    Terminating,
    Succeeded,
    Failed,
}

/// What kind of workload owns a pod; it decides the type of building.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum WorkloadKind {
    Deployment,
    StatefulSet,
    DaemonSet,
    Job,
    /// A pod nothing owns.
    Standalone,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PodInfo {
    pub uid: String,
    pub namespace: String,
    /// The owning workload (e.g. a Deployment); its pods share one look.
    pub workload: String,
    pub kind: WorkloadKind,
    pub state: PodState,
}

/// A change reported by a source.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum WorldEvent {
    PodUpserted(PodInfo),
    PodDeleted { uid: String },
}

/// Choices about what the city shows; all off keeps one building per pod.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Rules {
    /// Pods of finished jobs (Succeeded) leave the city; they are only counted.
    pub hide_completed: bool,
    /// All pods of a DaemonSet share one building instead of one per node.
    pub group_daemonsets: bool,
}

/// One building: usually one pod, or several when pods are grouped.
#[derive(Debug)]
pub struct Building {
    /// The pod the building stands for, with the worst state of its members.
    pub pod: PodInfo,
    pub lot: Lot,
    members: HashMap<String, PodState>,
}

#[derive(Debug, Default)]
pub struct World {
    rules: Rules,
    /// Keyed by pod uid, or by group for grouped pods.
    buildings: HashMap<String, Building>,
    /// Which building each pod belongs to.
    pod_buildings: HashMap<String, String>,
    hidden: HashSet<String>,
    layout: Layout,
}

impl World {
    pub fn new(rules: Rules) -> Self {
        Self { rules, ..Self::default() }
    }

    pub fn apply(&mut self, event: WorldEvent) {
        match event {
            WorldEvent::PodUpserted(pod) if self.rules.hide_completed && pod.state == PodState::Succeeded => {
                self.remove(&pod.uid);
                self.hidden.insert(pod.uid);
            }
            WorldEvent::PodUpserted(pod) => {
                self.hidden.remove(&pod.uid);
                let key = self.building_key(&pod);
                self.pod_buildings.insert(pod.uid.clone(), key.clone());
                let layout = &mut self.layout;
                let building = self.buildings.entry(key).or_insert_with(|| Building {
                    lot: layout.place(&pod.namespace),
                    pod: pod.clone(),
                    members: HashMap::new(),
                });
                building.members.insert(pod.uid.clone(), pod.state);
                building.pod = PodInfo { state: worst(building.members.values()), ..pod };
            }
            WorldEvent::PodDeleted { uid } => {
                self.hidden.remove(&uid);
                self.remove(&uid);
            }
        }
    }

    fn building_key(&self, pod: &PodInfo) -> String {
        if self.rules.group_daemonsets && pod.kind == WorkloadKind::DaemonSet {
            format!("daemonset/{}/{}", pod.namespace, pod.workload)
        } else {
            pod.uid.clone()
        }
    }

    /// Takes a pod out of its building; the building goes once it is empty.
    fn remove(&mut self, uid: &str) {
        let Some(key) = self.pod_buildings.remove(uid) else {
            return;
        };
        let Some(building) = self.buildings.get_mut(&key) else {
            return;
        };
        building.members.remove(uid);
        if building.members.is_empty() {
            let lot = building.lot;
            self.buildings.remove(&key);
            self.layout.release(lot);
        } else {
            building.pod.state = worst(building.members.values());
        }
    }

    pub fn buildings(&self) -> impl Iterator<Item = &Building> {
        self.buildings.values()
    }

    pub fn layout(&self) -> &Layout {
        &self.layout
    }

    /// Pods in the city, hidden ones not included.
    pub fn pod_count(&self) -> usize {
        self.pod_buildings.len()
    }

    /// Pods of finished jobs left out by [`Rules::hide_completed`].
    pub fn hidden_count(&self) -> usize {
        self.hidden.len()
    }

    pub fn namespace_count(&self) -> usize {
        self.layout.districts().count()
    }
}

/// The state that needs the most attention among a building's pods.
fn worst<'a>(states: impl Iterator<Item = &'a PodState>) -> PodState {
    let urgency = |state: &PodState| match state {
        PodState::CrashLoop => 5,
        PodState::Failed => 4,
        PodState::Pending => 3,
        PodState::Terminating => 2,
        PodState::Running => 1,
        PodState::Succeeded => 0,
    };
    states.max_by_key(|state| urgency(state)).copied().unwrap_or(PodState::Running)
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

    fn pod(uid: &str, kind: WorkloadKind, workload: &str, state: PodState) -> WorldEvent {
        WorldEvent::PodUpserted(PodInfo {
            uid: uid.to_owned(),
            namespace: "ops".to_owned(),
            workload: workload.to_owned(),
            kind,
            state,
        })
    }

    #[test]
    fn grouped_daemonset_pods_share_one_building_showing_the_worst_state() {
        let mut world = World::new(Rules { group_daemonsets: true, ..Rules::default() });
        world.apply(pod("n1", WorkloadKind::DaemonSet, "agent", PodState::Running));
        world.apply(pod("n2", WorkloadKind::DaemonSet, "agent", PodState::CrashLoop));
        world.apply(pod("n3", WorkloadKind::DaemonSet, "other", PodState::Running));
        assert_eq!((world.pod_count(), world.buildings().count()), (3, 2));
        let agent = world.buildings().find(|b| b.pod.workload == "agent").expect("agent building");
        assert_eq!((agent.pod.state, agent.members.len()), (PodState::CrashLoop, 2));

        world.apply(delete("n2"));
        let agent = world.buildings().find(|b| b.pod.workload == "agent").expect("agent building");
        assert_eq!((agent.pod.state, agent.members.len()), (PodState::Running, 1));
        world.apply(delete("n1"));
        assert_eq!(world.buildings().count(), 1, "the empty group's building goes");
    }

    #[test]
    fn hidden_completed_jobs_leave_the_city_but_are_counted() {
        let mut world = World::new(Rules { hide_completed: true, ..Rules::default() });
        world.apply(pod("j", WorkloadKind::Job, "report", PodState::Running));
        assert_eq!(world.buildings().count(), 1);
        world.apply(pod("j", WorkloadKind::Job, "report", PodState::Succeeded));
        assert_eq!((world.buildings().count(), world.pod_count(), world.hidden_count()), (0, 0, 1));
        world.apply(delete("j"));
        assert_eq!(world.hidden_count(), 0);
    }

    #[test]
    fn without_rules_every_pod_is_its_own_building() {
        let mut world = World::default();
        world.apply(pod("n1", WorkloadKind::DaemonSet, "agent", PodState::Running));
        world.apply(pod("n2", WorkloadKind::DaemonSet, "agent", PodState::Running));
        world.apply(pod("j", WorkloadKind::Job, "report", PodState::Succeeded));
        assert_eq!((world.buildings().count(), world.hidden_count()), (3, 0));
    }

    #[test]
    fn deleting_an_unknown_pod_is_ignored() {
        let mut world = World::default();
        world.apply(upsert("a", "shop", PodState::Running));
        world.apply(delete("nope"));
        assert_eq!(world.pod_count(), 1);
    }
}
