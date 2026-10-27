//! A simulated cluster, so kube-city runs without a real one.
//!
//! Made-up namespaces and pods get created, start, crash, recover, finish and
//! get deleted. Everything is driven by a seeded RNG, so a seed replays the
//! same city.

use std::time::Duration;

use tokio::sync::mpsc::UnboundedSender;

use crate::{
    util::Rng,
    world::{PodDetails, PodInfo, PodState, WorkloadKind, WorldEvent},
};

type Workload = (&'static str, WorkloadKind);

const DEP: WorkloadKind = WorkloadKind::Deployment;
const STS: WorkloadKind = WorkloadKind::StatefulSet;
const DS: WorkloadKind = WorkloadKind::DaemonSet;

/// Namespaces the simulation can open, with their long-running workloads.
const CATALOGUE: &[(&str, &[Workload])] = &[
    ("orders", &[("kafka", STS), ("schema-registry", DEP), ("orders-api", DEP), ("orders-sink", DEP)]),
    ("keycloak", &[("keycloak", DEP), ("postgres", STS)]),
    ("monitoring", &[("prometheus", STS), ("grafana", DEP), ("alertmanager", STS), ("node-exporter", DS)]),
    ("catalog", &[("catalog-api", DEP), ("catalog-worker", DEP), ("search-indexer", DEP)]),
    ("ingress", &[("ingress-nginx", DEP)]),
    ("payments", &[("payments-gateway", DEP), ("fraud-check", DEP), ("receipts", DEP)]),
    ("shipping", &[("carrier-connector", DEP), ("carrier-simulator", DEP)]),
    (
        "feature-preview",
        &[("console", DEP), ("middleware", DEP), ("core", DEP), ("debug-shell", WorkloadKind::Standalone)],
    ),
];
const JOB: Workload = ("report-job", WorkloadKind::Job);
/// Scale-ups stop here; restarts and new namespaces may briefly exceed it.
const MAX_PODS: usize = 120;
const INITIAL_NAMESPACES: usize = 4;

#[derive(Debug)]
struct SimPod {
    info: PodInfo,
    age: u32,
}

impl SimPod {
    fn is_job(&self) -> bool {
        self.info.kind == WorkloadKind::Job
    }
}

enum Transition {
    Stay,
    To(PodState),
    Remove,
}

#[derive(Debug)]
pub struct DemoCluster {
    rng: Rng,
    pods: Vec<SimPod>,
    /// Indexes into `CATALOGUE` of the namespaces that exist right now.
    open: Vec<usize>,
    next_uid: u64,
    started: bool,
}

impl DemoCluster {
    pub fn new(seed: u64) -> Self {
        Self { rng: Rng::new(seed), pods: Vec::new(), open: Vec::new(), next_uid: 0, started: false }
    }

    /// Advances the simulation by one step and returns what changed.
    pub fn step(&mut self) -> Vec<WorldEvent> {
        let mut events = Vec::new();
        if !self.started {
            self.started = true;
            for _ in 0..INITIAL_NAMESPACES {
                self.open_namespace(PodState::Running, &mut events);
            }
            return events;
        }
        self.advance_lifecycles(&mut events);
        match self.rng.below(100) {
            0..35 => self.scale_up(&mut events),
            35..60 => self.scale_down(&mut events),
            60..68 => self.crash(&mut events),
            68..76 => self.rolling_restart(&mut events),
            76..81 => self.open_namespace(PodState::Pending, &mut events),
            81..84 => self.close_namespace(&mut events),
            84..89 => self.run_job(&mut events),
            _ => {}
        }
        events
    }

    fn advance_lifecycles(&mut self, events: &mut Vec<WorldEvent>) {
        let mut i = 0;
        while i < self.pods.len() {
            let roll = self.rng.below(100);
            let pod = &mut self.pods[i];
            pod.age += 1;
            let transition = match pod.info.state {
                PodState::Pending if pod.age >= 3 => {
                    Transition::To(if roll < 85 { PodState::Running } else { PodState::CrashLoop })
                }
                PodState::Running if pod.is_job() && pod.age >= 5 => {
                    Transition::To(if roll < 80 { PodState::Succeeded } else { PodState::Failed })
                }
                PodState::CrashLoop if pod.age >= 10 => {
                    Transition::To(if roll < 50 { PodState::Running } else { PodState::CrashLoop })
                }
                PodState::Terminating if pod.age >= 2 => Transition::Remove,
                PodState::Succeeded if pod.age >= 6 => Transition::Remove,
                PodState::Failed if pod.age >= 8 => Transition::Remove,
                _ => Transition::Stay,
            };
            match transition {
                Transition::Stay => {}
                Transition::To(state) => {
                    pod.age = 0;
                    // A pod that stays in its crash loop has restarted again.
                    if pod.info.state != state || state == PodState::CrashLoop {
                        move_to(&mut pod.info, state);
                        events.push(WorldEvent::PodUpserted(pod.info.clone()));
                    }
                }
                Transition::Remove => {
                    let gone = self.pods.swap_remove(i);
                    events.push(WorldEvent::PodDeleted { uid: gone.info.uid });
                    continue;
                }
            }
            i += 1;
        }
    }

    fn scale_up(&mut self, events: &mut Vec<WorldEvent>) {
        if self.pods.len() >= MAX_PODS {
            return;
        }
        let Some((namespace, workloads)) = self.random_open_namespace() else {
            return self.open_namespace(PodState::Pending, events);
        };
        let workload = *self.rng.pick(workloads);
        self.spawn(namespace, workload, PodState::Pending, events);
    }

    fn scale_down(&mut self, events: &mut Vec<WorldEvent>) {
        if let Some(i) = self.random_pod(|pod| pod.info.state == PodState::Running && !pod.is_job()) {
            self.set_state(i, PodState::Terminating, events);
        }
    }

    fn crash(&mut self, events: &mut Vec<WorldEvent>) {
        if let Some(i) = self.random_pod(|pod| pod.info.state == PodState::Running && !pod.is_job()) {
            self.set_state(i, PodState::CrashLoop, events);
        }
    }

    fn rolling_restart(&mut self, events: &mut Vec<WorldEvent>) {
        let Some((namespace, workloads)) = self.random_open_namespace() else {
            return;
        };
        let workload = *self.rng.pick(workloads);
        let old: Vec<usize> = (0..self.pods.len())
            .filter(|&i| {
                let info = &self.pods[i].info;
                info.namespace == namespace && info.workload == workload.0 && info.state != PodState::Terminating
            })
            .collect();
        for _ in 0..old.len().max(1) {
            self.spawn(namespace, workload, PodState::Pending, events);
        }
        for i in old {
            self.set_state(i, PodState::Terminating, events);
        }
    }

    fn open_namespace(&mut self, state: PodState, events: &mut Vec<WorldEvent>) {
        let closed: Vec<usize> = (0..CATALOGUE.len()).filter(|i| !self.open.contains(i)).collect();
        if closed.is_empty() {
            return;
        }
        let entry = *self.rng.pick(&closed);
        self.open.push(entry);
        let (namespace, workloads) = CATALOGUE[entry];
        for &workload in workloads {
            for _ in 0..=self.rng.below(3) {
                self.spawn(namespace, workload, state, events);
            }
        }
    }

    fn close_namespace(&mut self, events: &mut Vec<WorldEvent>) {
        if self.open.len() <= 2 {
            return;
        }
        let position = self.rng.below(self.open.len() as u64) as usize;
        let (namespace, _) = CATALOGUE[self.open.swap_remove(position)];
        for i in 0..self.pods.len() {
            let info = &self.pods[i].info;
            if info.namespace == namespace && info.state != PodState::Terminating {
                self.set_state(i, PodState::Terminating, events);
            }
        }
    }

    fn run_job(&mut self, events: &mut Vec<WorldEvent>) {
        if let Some((namespace, _)) = self.random_open_namespace() {
            self.spawn(namespace, JOB, PodState::Pending, events);
        }
    }

    fn random_open_namespace(&mut self) -> Option<(&'static str, &'static [Workload])> {
        if self.open.is_empty() {
            return None;
        }
        Some(CATALOGUE[*self.rng.pick(&self.open)])
    }

    fn random_pod(&mut self, wanted: impl Fn(&SimPod) -> bool) -> Option<usize> {
        let candidates: Vec<usize> = (0..self.pods.len()).filter(|&i| wanted(&self.pods[i])).collect();
        (!candidates.is_empty()).then(|| *self.rng.pick(&candidates))
    }

    fn spawn(&mut self, namespace: &str, (workload, kind): Workload, state: PodState, events: &mut Vec<WorldEvent>) {
        self.next_uid += 1;
        let n = self.next_uid;
        let mut info = PodInfo {
            uid: format!("demo-{n}"),
            namespace: namespace.to_owned(),
            workload: workload.to_owned(),
            kind,
            state,
            // Derived from the counter, not the RNG, so seeds replay the same city.
            details: PodDetails {
                name: format!("{workload}-{}", suffix(n)),
                node: format!("node-{}", 1 + n % 4),
                images: vec![format!("{workload}:1.{}", n % 7)],
                containers: 1,
                ..PodDetails::default()
            },
        };
        move_to(&mut info, state);
        events.push(WorldEvent::PodUpserted(info.clone()));
        self.pods.push(SimPod { info, age: 0 });
    }

    fn set_state(&mut self, i: usize, state: PodState, events: &mut Vec<WorldEvent>) {
        let pod = &mut self.pods[i];
        move_to(&mut pod.info, state);
        pod.age = 0;
        events.push(WorldEvent::PodUpserted(pod.info.clone()));
    }
}

/// Changes a simulated pod's state and keeps its details in line: crashes
/// count as restarts, and the reason says what a real cluster would.
fn move_to(info: &mut PodInfo, state: PodState) {
    let from = info.state;
    let details = &mut info.details;
    if state == PodState::CrashLoop && from != PodState::Pending {
        details.restarts += 1;
    }
    details.ready = u32::from(state == PodState::Running);
    let reason = match state {
        PodState::Pending => Some("ContainerCreating"),
        PodState::CrashLoop if from == PodState::Pending => Some("ImagePullBackOff"),
        PodState::CrashLoop => Some("CrashLoopBackOff"),
        PodState::Failed => Some("Error"),
        PodState::Running if details.restarts > 0 => Some("OOMKilled"),
        _ => None,
    };
    details.reason = reason.map(str::to_owned);
    info.state = state;
}

/// Five letters that look like a pod name's random suffix.
fn suffix(n: u64) -> String {
    let mut x = n.wrapping_mul(0x9E37_79B9_7F4A_7C15) >> 20;
    (0..5)
        .map(|_| {
            let letter = b"bcdfghjklmnpqrstvwxz2456789"[(x % 27) as usize];
            x /= 27;
            letter as char
        })
        .collect()
}

/// Feeds the simulation into `events` at `steps_per_second` until the
/// receiver goes away.
pub async fn run(seed: u64, steps_per_second: f64, events: UnboundedSender<WorldEvent>) {
    let mut cluster = DemoCluster::new(seed);
    let mut ticker = tokio::time::interval(Duration::from_secs_f64(1.0 / steps_per_second));
    loop {
        ticker.tick().await;
        for event in cluster.step() {
            if events.send(event).is_err() {
                return;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use super::*;

    fn run_steps(seed: u64, steps: usize) -> Vec<WorldEvent> {
        let mut cluster = DemoCluster::new(seed);
        (0..steps).flat_map(|_| cluster.step()).collect()
    }

    #[test]
    fn the_same_seed_replays_the_same_city() {
        assert_eq!(run_steps(7, 300), run_steps(7, 300));
        assert_ne!(run_steps(7, 300), run_steps(8, 300));
    }

    #[test]
    fn it_starts_with_a_running_city() {
        let first = DemoCluster::new(1).step();
        assert!(!first.is_empty());
        assert!(first.iter().all(|e| matches!(e, WorldEvent::PodUpserted(p) if p.state == PodState::Running)));
    }

    #[test]
    fn simulated_pods_have_details_and_crashes_count_as_restarts() {
        let mut demo = DemoCluster::new(7);
        let mut upserts = Vec::new();
        for _ in 0..400 {
            upserts.extend(demo.step().into_iter().filter_map(|e| match e {
                WorldEvent::PodUpserted(info) => Some(info),
                WorldEvent::PodDeleted { .. } => None,
            }));
        }
        assert!(upserts.iter().all(|p| p.details.name.starts_with(&p.workload) && !p.details.node.is_empty()));
        assert!(upserts.iter().all(|p| p.details.images.len() == 1 && p.details.containers == 1));
        let crashing: Vec<_> = upserts.iter().filter(|p| p.state == PodState::CrashLoop).collect();
        assert!(!crashing.is_empty() && crashing.iter().all(|p| p.details.reason.is_some()));
        assert!(upserts.iter().any(|p| p.details.restarts > 0));
        assert!(upserts.iter().filter(|p| p.state == PodState::Running).all(|p| p.details.ready == 1));
    }

    #[test]
    fn events_stay_consistent_and_bounded_over_a_long_run() {
        let mut cluster = DemoCluster::new(1234);
        let mut alive = HashSet::new();
        let mut seen_states = HashSet::new();
        for _ in 0..3000 {
            for event in cluster.step() {
                match &event {
                    WorldEvent::PodUpserted(pod) => {
                        alive.insert(pod.uid.clone());
                        seen_states.insert(format!("{:?}", pod.state));
                    }
                    WorldEvent::PodDeleted { uid } => {
                        assert!(alive.remove(uid), "deleted a pod that was never created: {uid}");
                    }
                }
            }
            assert_eq!(alive.len(), cluster.pods.len());
            assert!(cluster.pods.len() < MAX_PODS * 2);
        }
        assert_eq!(seen_states.len(), 6, "every state shows up: {seen_states:?}");
    }
}
