//! Watches pods in a real cluster and turns them into world events.
//!
//! Access is read-only: the city only ever lists and watches pods.

use std::{collections::HashSet, time::Duration};

use anyhow::{Context, Result, bail};
use futures::{StreamExt, stream};
use k8s_openapi::api::core::v1::{Pod, PodStatus};
use kube::{
    Api, Client, Config,
    api::ListParams,
    config::{KubeConfigOptions, Kubeconfig},
    runtime::{WatchStreamExt, watcher},
};
use tokio::sync::{mpsc::UnboundedSender, watch};

use crate::world::{PodDetails, PodInfo, PodState, WorkloadKind, WorldEvent};

/// Long enough to finish an interactive (device code) login.
const CONNECT_TIMEOUT: Duration = Duration::from_secs(180);
const STATUS_ERROR_MAX: usize = 80;
/// Container waiting reasons that mean the pod will not start without help.
const BROKEN_REASONS: [&str; 7] = [
    "CrashLoopBackOff",
    "ImagePullBackOff",
    "ErrImagePull",
    "InvalidImageName",
    "CreateContainerConfigError",
    "CreateContainerError",
    "RunContainerError",
];

/// Every context in the kubeconfig, sorted.
pub fn contexts() -> Result<Vec<String>> {
    let kubeconfig = Kubeconfig::read().context("cannot read the kubeconfig")?;
    let mut names: Vec<String> = kubeconfig.contexts.into_iter().map(|named| named.name).collect();
    names.sort();
    names.dedup();
    Ok(names)
}

/// Picks the context from `KUBECONFIG` (all files, merged like kubectl does)
/// and checks the pods can be listed, before the terminal switches to the city.
/// Exec login plugins run here, so their prompts reach the user.
pub async fn connect(context: Option<&str>, namespaces: &[String]) -> Result<(Client, String)> {
    let kubeconfig = Kubeconfig::read().context("cannot read the kubeconfig")?;
    let context = match context {
        Some(name) => name.to_owned(),
        None => kubeconfig.current_context.clone().context("no current context; pass --context")?,
    };
    if !kubeconfig.contexts.iter().any(|named| named.name == context) {
        let known: Vec<_> = kubeconfig.contexts.iter().map(|named| named.name.as_str()).collect();
        bail!("context {context:?} not found; known contexts: {}", known.join(", "));
    }
    let options = KubeConfigOptions { context: Some(context.clone()), ..KubeConfigOptions::default() };
    let config = Config::from_custom_kubeconfig(kubeconfig, &options).await?;
    let client = Client::try_from(config)?;

    eprintln!("kube-city: connecting to {context}…");
    let check = async {
        for api in pod_apis(&client, namespaces) {
            api.list(&ListParams::default().limit(1)).await?;
        }
        Ok::<_, kube::Error>(())
    };
    tokio::time::timeout(CONNECT_TIMEOUT, check)
        .await
        .with_context(|| format!("no answer from {context} within {}s", CONNECT_TIMEOUT.as_secs()))?
        .with_context(|| {
            format!("cannot list pods in {context}; check your access, or limit the city with --namespace")
        })?;
    Ok((client, context))
}

/// Streams pod changes into `events` until the receiver goes away, keeping
/// `status` up to date for the status bar. Dropped connections are retried
/// with backoff.
pub async fn watch(
    client: Client,
    context: String,
    namespaces: Vec<String>,
    events: UnboundedSender<WorldEvent>,
    status: watch::Sender<String>,
) {
    let apis = pod_apis(&client, &namespaces);
    let mut trackers = vec![Tracker::default(); apis.len()];
    let mut live = vec![false; apis.len()];
    let streams = apis.into_iter().enumerate().map(|(i, api)| {
        watcher(api, watcher::Config::default().any_semantic()).default_backoff().map(move |event| (i, event)).boxed()
    });
    let mut merged = stream::select_all(streams);
    while let Some((i, result)) = merged.next().await {
        match result {
            Ok(event) => {
                if matches!(event, watcher::Event::InitDone) {
                    live[i] = true;
                    if live.iter().all(|&ok| ok) {
                        status.send_replace(format!("{context} · live"));
                    }
                }
                for change in trackers[i].handle(event) {
                    if events.send(change).is_err() {
                        return;
                    }
                }
            }
            Err(error) => {
                live[i] = false;
                let message: String = error.to_string().chars().take(STATUS_ERROR_MAX).collect();
                status.send_replace(format!("{context} · reconnecting: {message}"));
            }
        }
    }
}

fn pod_apis(client: &Client, namespaces: &[String]) -> Vec<Api<Pod>> {
    if namespaces.is_empty() {
        vec![Api::all(client.clone())]
    } else {
        namespaces.iter().map(|namespace| Api::namespaced(client.clone(), namespace)).collect()
    }
}

/// Remembers which pods one watch stream has reported, so that pods deleted
/// while the stream was reconnecting are removed when it lists them again.
#[derive(Debug, Default, Clone)]
struct Tracker {
    known: HashSet<String>,
    relisted: Option<HashSet<String>>,
}

impl Tracker {
    fn handle(&mut self, event: watcher::Event<Pod>) -> Vec<WorldEvent> {
        match event {
            watcher::Event::Init => {
                self.relisted = Some(HashSet::new());
                Vec::new()
            }
            watcher::Event::InitApply(pod) | watcher::Event::Apply(pod) => {
                let Some(info) = pod_info(&pod) else {
                    return Vec::new();
                };
                if let Some(relisted) = &mut self.relisted {
                    relisted.insert(info.uid.clone());
                }
                self.known.insert(info.uid.clone());
                vec![WorldEvent::PodUpserted(info)]
            }
            watcher::Event::Delete(pod) => match pod.metadata.uid {
                Some(uid) if self.known.remove(&uid) => vec![WorldEvent::PodDeleted { uid }],
                _ => Vec::new(),
            },
            watcher::Event::InitDone => {
                let relisted = self.relisted.take().unwrap_or_default();
                let gone: Vec<String> = self.known.difference(&relisted).cloned().collect();
                self.known = relisted;
                gone.into_iter().map(|uid| WorldEvent::PodDeleted { uid }).collect()
            }
        }
    }
}

fn pod_info(pod: &Pod) -> Option<PodInfo> {
    let meta = &pod.metadata;
    let (workload, kind) = workload_of(pod);
    Some(PodInfo {
        uid: meta.uid.clone()?,
        namespace: meta.namespace.clone()?,
        workload,
        kind,
        state: state_of(pod),
        details: details_of(pod),
    })
}

fn details_of(pod: &Pod) -> PodDetails {
    let spec = pod.spec.as_ref();
    let status = pod.status.as_ref();
    let statuses = status.and_then(|status| status.container_statuses.as_deref()).unwrap_or_default();
    PodDetails {
        name: pod.metadata.name.clone().unwrap_or_default(),
        node: spec.and_then(|spec| spec.node_name.clone()).unwrap_or_default(),
        images: spec.map(|spec| spec.containers.iter().filter_map(|c| c.image.clone()).collect()).unwrap_or_default(),
        restarts: statuses.iter().map(|c| u32::try_from(c.restart_count).unwrap_or(0)).sum(),
        ready: statuses.iter().filter(|c| c.ready).count() as u32,
        containers: spec.map_or(statuses.len(), |spec| spec.containers.len()) as u32,
        // Through serde, so this does not depend on k8s-openapi's time type.
        created: pod
            .metadata
            .creation_timestamp
            .as_ref()
            .and_then(|time| serde_json::to_value(time).ok()?.as_str().map(str::to_owned)),
        reason: reason_of(status),
    }
}

/// The most telling reason a pod is not simply running: what a container is
/// waiting for (reasons that keep it from starting first), why a container
/// failed, why a restarted container last stopped (e.g. OOMKilled), or why
/// the pod itself stopped (e.g. Evicted).
fn reason_of(status: Option<&PodStatus>) -> Option<String> {
    let status = status?;
    let containers = || status.init_container_statuses.iter().chain(&status.container_statuses).flatten();
    let waiting = || containers().filter_map(|c| c.state.as_ref()?.waiting.as_ref()?.reason.as_deref());
    let failed = containers()
        .filter_map(|c| c.state.as_ref()?.terminated.as_ref()?.reason.as_deref())
        .find(|reason| *reason != "Completed");
    let last_stop = containers()
        .filter(|c| c.restart_count > 0)
        .find_map(|c| c.last_state.as_ref()?.terminated.as_ref()?.reason.as_deref());
    waiting()
        .find(|reason| BROKEN_REASONS.contains(reason))
        .or_else(|| waiting().next())
        .or(failed)
        .or(last_stop)
        .or(status.reason.as_deref())
        .map(str::to_owned)
}

fn state_of(pod: &Pod) -> PodState {
    if pod.metadata.deletion_timestamp.is_some() {
        return PodState::Terminating;
    }
    let status = pod.status.as_ref();
    match status.and_then(|status| status.phase.as_deref()) {
        Some("Succeeded") => PodState::Succeeded,
        Some("Failed") => PodState::Failed,
        _ if is_broken(status) => PodState::CrashLoop,
        Some("Running") => PodState::Running,
        // Pending, Unknown, or no status reported yet.
        _ => PodState::Pending,
    }
}

fn is_broken(status: Option<&PodStatus>) -> bool {
    status
        .into_iter()
        .flat_map(|status| status.init_container_statuses.iter().chain(&status.container_statuses).flatten())
        .filter_map(|container| container.state.as_ref()?.waiting.as_ref()?.reason.as_deref())
        .any(|reason| BROKEN_REASONS.contains(&reason))
}

/// The workload that owns the pod, so its pods share a look across restarts
/// and rollouts: the Deployment for ReplicaSet pods, the CronJob for its Jobs'
/// pods, the direct controller otherwise, and the pod itself if nothing owns it.
/// Controllers kube-city doesn't know (operators' custom resources) count as
/// Deployments.
fn workload_of(pod: &Pod) -> (String, WorkloadKind) {
    let meta = &pod.metadata;
    let Some(owner) = meta.owner_references.iter().flatten().find(|owner| owner.controller == Some(true)) else {
        return (meta.name.clone().unwrap_or_default(), WorkloadKind::Standalone);
    };
    let kind = match owner.kind.as_str() {
        "StatefulSet" => WorkloadKind::StatefulSet,
        "DaemonSet" => WorkloadKind::DaemonSet,
        "Job" => WorkloadKind::Job,
        _ => WorkloadKind::Deployment,
    };
    let name = owner.name.as_str();
    let base = match owner.kind.as_str() {
        "ReplicaSet" => meta
            .labels
            .as_ref()
            .and_then(|labels| labels.get("pod-template-hash"))
            .and_then(|hash| name.strip_suffix(hash.as_str())?.strip_suffix('-')),
        // CronJob jobs end in the scheduled time in minutes: 8+ digits.
        "Job" => name
            .rsplit_once('-')
            .filter(|(_, stamp)| stamp.len() >= 8 && stamp.bytes().all(|b| b.is_ascii_digit()))
            .map(|(cron_job, _)| cron_job),
        _ => None,
    };
    (base.unwrap_or(name).to_owned(), kind)
}

#[cfg(test)]
mod tests {
    use serde_json::{Value, json};

    use super::*;

    fn pod(manifest: Value) -> Pod {
        serde_json::from_value(manifest).expect("valid pod")
    }

    fn running(uid: &str) -> Pod {
        pod(json!({
            "metadata": { "uid": uid, "name": uid, "namespace": "shop" },
            "status": { "phase": "Running" }
        }))
    }

    fn waiting(phase: &str, reason: &str, init: bool) -> Pod {
        let statuses = json!([{
            "name": "app", "image": "app", "imageID": "", "ready": false, "restartCount": 3,
            "state": { "waiting": { "reason": reason } }
        }]);
        let key = if init { "initContainerStatuses" } else { "containerStatuses" };
        pod(json!({
            "metadata": { "uid": "u", "name": "p", "namespace": "shop" },
            "status": { "phase": phase, key: statuses }
        }))
    }

    fn owned_by(kind: &str, owner: &str, labels: Value) -> Pod {
        pod(json!({
            "metadata": {
                "uid": "u", "name": "the-pod", "namespace": "shop", "labels": labels,
                "ownerReferences": [{ "apiVersion": "v1", "kind": kind, "name": owner, "uid": "o", "controller": true }]
            }
        }))
    }

    fn uids(events: &[WorldEvent]) -> Vec<(&str, &str)> {
        let mut out: Vec<_> = events
            .iter()
            .map(|event| match event {
                WorldEvent::PodUpserted(info) => ("upsert", info.uid.as_str()),
                WorldEvent::PodDeleted { uid } => ("delete", uid.as_str()),
            })
            .collect();
        out.sort_unstable();
        out
    }

    #[test]
    fn phases_map_to_city_states() {
        let phase = |p: &str| pod(json!({ "metadata": {}, "status": { "phase": p } }));
        assert_eq!(state_of(&phase("Running")), PodState::Running);
        assert_eq!(state_of(&phase("Pending")), PodState::Pending);
        assert_eq!(state_of(&phase("Succeeded")), PodState::Succeeded);
        assert_eq!(state_of(&phase("Failed")), PodState::Failed);
        assert_eq!(state_of(&phase("Unknown")), PodState::Pending);
        assert_eq!(state_of(&pod(json!({ "metadata": {} }))), PodState::Pending);
    }

    #[test]
    fn stuck_containers_count_as_crash_loop() {
        assert_eq!(state_of(&waiting("Running", "CrashLoopBackOff", false)), PodState::CrashLoop);
        assert_eq!(state_of(&waiting("Pending", "ImagePullBackOff", false)), PodState::CrashLoop);
        assert_eq!(state_of(&waiting("Pending", "CrashLoopBackOff", true)), PodState::CrashLoop);
        assert_eq!(state_of(&waiting("Pending", "ContainerCreating", false)), PodState::Pending);
    }

    #[test]
    fn a_deletion_timestamp_means_terminating() {
        let pod = pod(json!({
            "metadata": { "deletionTimestamp": "2026-10-15T09:00:00Z" },
            "status": { "phase": "Running" }
        }));
        assert_eq!(state_of(&pod), PodState::Terminating);
    }

    #[test]
    fn workloads_are_named_after_their_owners() {
        let name = |pod: &Pod| workload_of(pod).0;
        let deployment = owned_by("ReplicaSet", "web-7d9f8b6c5", json!({ "pod-template-hash": "7d9f8b6c5" }));
        assert_eq!(name(&deployment), "web");
        assert_eq!(name(&owned_by("ReplicaSet", "bare-rs", json!({}))), "bare-rs");
        assert_eq!(name(&owned_by("StatefulSet", "kafka", json!({}))), "kafka");
        assert_eq!(name(&owned_by("Job", "nightly-report-29345760", json!({}))), "nightly-report");
        assert_eq!(name(&owned_by("Job", "migrate-1", json!({}))), "migrate-1");
        assert_eq!(name(&running("lonely")), "lonely");
    }

    #[test]
    fn the_owner_decides_the_kind_of_building() {
        let kind = |owner: &str| workload_of(&owned_by(owner, "x", json!({}))).1;
        assert_eq!(kind("ReplicaSet"), WorkloadKind::Deployment);
        assert_eq!(kind("StatefulSet"), WorkloadKind::StatefulSet);
        assert_eq!(kind("DaemonSet"), WorkloadKind::DaemonSet);
        assert_eq!(kind("Job"), WorkloadKind::Job);
        assert_eq!(kind("StrimziPodSet"), WorkloadKind::Deployment);
        assert_eq!(workload_of(&running("lonely")).1, WorkloadKind::Standalone);
    }

    #[test]
    fn details_come_from_the_spec_and_the_container_statuses() {
        let pod = pod(json!({
            "metadata": {
                "uid": "u", "name": "web-7d9f8b6c5-x2kq9", "namespace": "shop",
                "creationTimestamp": "2026-10-20T08:00:00Z"
            },
            "spec": { "nodeName": "node-3", "containers": [
                { "name": "app", "image": "registry.example/web:1.4" },
                { "name": "proxy", "image": "envoy:1.30" }
            ] },
            "status": { "phase": "Running", "startTime": "2026-10-22T08:00:00Z", "containerStatuses": [
                { "name": "app", "image": "registry.example/web:1.4", "imageID": "", "ready": false, "restartCount": 4,
                  "state": { "running": {} }, "lastState": { "terminated": { "exitCode": 137, "reason": "OOMKilled" } } },
                { "name": "proxy", "image": "envoy:1.30", "imageID": "", "ready": true, "restartCount": 1,
                  "state": { "running": {} } }
            ] }
        }));
        let details = pod_info(&pod).expect("a pod").details;
        assert_eq!(details.name, "web-7d9f8b6c5-x2kq9");
        assert_eq!(details.node, "node-3");
        assert_eq!(details.images, ["registry.example/web:1.4", "envoy:1.30"]);
        assert_eq!((details.restarts, details.ready, details.containers), (5, 1, 2));
        // The age kubectl shows counts from creation, not from the last start.
        assert_eq!(details.created.as_deref(), Some("2026-10-20T08:00:00Z"));
        assert_eq!(details.reason.as_deref(), Some("OOMKilled"));
    }

    #[test]
    fn the_most_telling_reason_wins() {
        let reason = |pod: &Pod| reason_of(pod.status.as_ref());
        // Waiting to start beats the reason a restarted container stopped.
        let mut stuck = waiting("Running", "ImagePullBackOff", false);
        let status = stuck.status.as_mut().unwrap().container_statuses.as_mut().unwrap();
        status[0].last_state =
            serde_json::from_value(json!({ "terminated": { "exitCode": 137, "reason": "OOMKilled" } })).ok();
        assert_eq!(reason(&stuck).as_deref(), Some("ImagePullBackOff"));
        assert_eq!(reason(&waiting("Pending", "ContainerCreating", false)).as_deref(), Some("ContainerCreating"));
        let evicted = pod(json!({ "metadata": {}, "status": { "phase": "Failed", "reason": "Evicted" } }));
        assert_eq!(reason(&evicted).as_deref(), Some("Evicted"));
        assert_eq!(reason(&running("fine")), None);
    }

    #[test]
    fn pods_without_uid_or_namespace_are_skipped() {
        assert!(pod_info(&pod(json!({ "metadata": { "uid": "u" } }))).is_none());
        assert!(pod_info(&pod(json!({ "metadata": { "namespace": "n" } }))).is_none());
    }

    #[test]
    fn the_tracker_streams_applies_and_deletes() {
        let mut tracker = Tracker::default();
        assert!(tracker.handle(watcher::Event::Init).is_empty());
        assert_eq!(uids(&tracker.handle(watcher::Event::InitApply(running("a")))), [("upsert", "a")]);
        assert!(tracker.handle(watcher::Event::InitDone).is_empty());
        assert_eq!(uids(&tracker.handle(watcher::Event::Apply(running("b")))), [("upsert", "b")]);
        assert_eq!(uids(&tracker.handle(watcher::Event::Delete(running("a")))), [("delete", "a")]);
        assert!(tracker.handle(watcher::Event::Delete(running("never-seen"))).is_empty());
    }

    #[test]
    fn a_relist_removes_pods_that_vanished_while_disconnected() {
        let mut tracker = Tracker::default();
        for event in [
            watcher::Event::Init,
            watcher::Event::InitApply(running("a")),
            watcher::Event::InitApply(running("b")),
            watcher::Event::InitDone,
            watcher::Event::Apply(running("c")),
        ] {
            tracker.handle(event);
        }
        // The connection drops; meanwhile "a" and "c" are deleted, "d" is created.
        tracker.handle(watcher::Event::Init);
        tracker.handle(watcher::Event::InitApply(running("b")));
        tracker.handle(watcher::Event::InitApply(running("d")));
        assert_eq!(uids(&tracker.handle(watcher::Event::InitDone)), [("delete", "a"), ("delete", "c")]);
        assert_eq!(tracker.known, HashSet::from(["b".to_owned(), "d".to_owned()]));
    }
}
