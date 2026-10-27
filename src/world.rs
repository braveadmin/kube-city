//! What the sources report about pods: the events every source (cluster,
//! demo, recording) sends and the browser's page turns into the city.

use serde::{Deserialize, Serialize};

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
    /// What the browser's inspector shows; empty in recordings made before
    /// kube-city collected it.
    #[serde(default)]
    pub details: PodDetails,
}

/// A pod's particulars, beyond what decides its building.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct PodDetails {
    /// The pod's own name.
    pub name: String,
    /// The node it runs on; empty until it is scheduled.
    pub node: String,
    /// Container images, in the order of the pod spec.
    pub images: Vec<String>,
    /// Restarts of all its containers together.
    pub restarts: u32,
    /// Containers that are ready, out of `containers`.
    pub ready: u32,
    pub containers: u32,
    /// When the pod was created (RFC 3339), for its age as kubectl shows it.
    pub created: Option<String>,
    /// Why it is not simply running, e.g. ImagePullBackOff or OOMKilled.
    pub reason: Option<String>,
}

/// A change reported by a source.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum WorldEvent {
    PodUpserted(PodInfo),
    PodDeleted { uid: String },
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
            details: PodDetails::default(),
        })
    }

    pub(crate) fn delete(uid: &str) -> WorldEvent {
        WorldEvent::PodDeleted { uid: uid.to_owned() }
    }

    #[test]
    fn events_recorded_before_pod_details_still_load() {
        let old =
            r#"{"PodUpserted":{"uid":"u","namespace":"shop","workload":"web","kind":"Deployment","state":"Running"}}"#;
        let event: WorldEvent = serde_json::from_str(old).expect("old recordings stay readable");
        let WorldEvent::PodUpserted(info) = event else { panic!("an upsert") };
        assert_eq!(info.details, PodDetails::default());
        assert_eq!(info.state, PodState::Running);
    }
}
