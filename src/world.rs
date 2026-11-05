//! What the sources report about pods: the events every source (cluster,
//! demo, recording) sends and the browser's page turns into the city.

use serde::{Deserialize, Serialize};

use crate::roles::Role;

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
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
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
    /// What the workload does, from its images (#48); worked out by the
    /// source, before anonymising. `None` only in recordings made before
    /// kube-city sent it, until `recording::load` fills it in.
    #[serde(default)]
    pub role: Option<Role>,
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
    /// CPU the pod requests, in millicores, counted as the scheduler does
    /// (#49); `None` when it requests none or the source does not know.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cpu: Option<u64>,
    /// Memory the pod requests, in bytes; `None` as for `cpu`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub memory: Option<u64>,
}

/// A change reported by a source.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum WorldEvent {
    PodUpserted(PodInfo),
    PodDeleted {
        uid: String,
    },
    /// How a workload is reached from outside the cluster (#50); sent when
    /// that changes, with no `ways` once it is not reached any more. A
    /// source sends a workload's first one before its pods where it can, so
    /// the page knows its shops when it lays out the street.
    Exposed(Exposure),
}

/// A workload that is reached from outside: a "shop" in the city (#50).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Exposure {
    pub namespace: String,
    pub workload: String,
    pub kind: WorkloadKind,
    /// Sorted, so the same exposure always reads the same.
    pub ways: Vec<Way>,
}

/// One way in: an Ingress or a Gateway route to one of the workload's
/// Services, or the Service itself as a load balancer or on node ports.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct Way {
    pub via: Via,
    /// The Service the way leads to.
    pub service: String,
    /// Host names (Ingress rules, route hostnames); sorted.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub hosts: Vec<String>,
    /// Paths (Ingress rules, HTTP route matches); sorted.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub paths: Vec<String>,
    /// Ports: the Service's port for an Ingress or route (a number or a
    /// port's name), the load balancer's ports, or the node ports; sorted.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub ports: Vec<String>,
    /// A load balancer's address (IP or host name), when it has one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub address: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum Via {
    Ingress,
    HttpRoute,
    GrpcRoute,
    LoadBalancer,
    NodePort,
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
            role: Some(Role::Other),
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
        assert_eq!(info.role, None);
    }

    #[test]
    fn requests_round_trip_and_stay_out_when_unknown() {
        let details = PodDetails { cpu: Some(1500), memory: Some(2 << 30), ..PodDetails::default() };
        let text = serde_json::to_string(&details).unwrap();
        assert_eq!(serde_json::from_str::<PodDetails>(&text).unwrap(), details);
        let none = serde_json::to_string(&PodDetails::default()).unwrap();
        assert!(!none.contains("cpu") && !none.contains("memory"), "{none}");
    }
}
