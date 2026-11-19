//! Watches pods in a real cluster and turns them into world events, Nodes
//! for the power stations (#96), and Services, Ingresses and Gateway routes
//! for which workloads are reached from outside (#50).
//!
//! Access is read-only: the city only ever lists and watches.

use std::{
    collections::{HashMap, HashSet},
    time::Duration,
};

use anyhow::{Context, Result, bail};
use futures::{StreamExt, stream, stream::BoxStream};
use k8s_openapi::api::{
    core::v1::{Container, Node, Pod, PodSpec, PodStatus, Service},
    networking::v1::Ingress,
};
use kube::{
    Api, Client, Config, Resource,
    api::ListParams,
    config::{KubeConfigOptions, Kubeconfig},
    core::{ApiResource, DynamicObject, GroupVersionKind},
    runtime::{WatchStreamExt, watcher},
};
use serde::de::DeserializeOwned;
use tokio::sync::{mpsc::UnboundedSender, watch};

use crate::{
    exposure::{self, Exposures, PodLabels},
    roles::role_of,
    world::{NodeInfo, PodDetails, PodInfo, PodState, Via, WorkloadKind, WorldEvent},
};

/// Long enough to finish an interactive (device code) login.
const CONNECT_TIMEOUT: Duration = Duration::from_secs(180);
const STATUS_ERROR_MAX: usize = 80;
/// How long the first pods wait for the Services, Ingresses and routes, so a
/// workload's exposure reaches the page before its pods do (#50).
const EXPOSURE_WAIT: Duration = Duration::from_secs(15);
/// How often exposure is worked out again while something changes.
const EXPOSURE_EVERY: Duration = Duration::from_secs(1);
/// The Gateway API's stable routes: plural, kind, and how the city names them.
const ROUTES: [(&str, &str, Via); 2] =
    [("httproutes", "HTTPRoute", Via::HttpRoute), ("grpcroutes", "GRPCRoute", Via::GrpcRoute)];
const GATEWAY_API: (&str, &str) = ("gateway.networking.k8s.io", "v1");
/// Node conditions that warn of trouble when True (#96).
const PRESSURE: [&str; 4] = ["MemoryPressure", "DiskPressure", "PIDPressure", "NetworkUnavailable"];
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

/// Streams pod changes and exposure (#50) into `events` until the receiver
/// goes away, keeping `status` up to date for the status bar. Dropped
/// connections are retried with backoff.
///
/// The first pods are held until the Services, Ingresses and routes are
/// listed too (at most `EXPOSURE_WAIT`), and follow the first exposure, so
/// the page knows its shops when it lays out the street.
pub async fn watch(
    client: Client,
    context: String,
    namespaces: Vec<String>,
    events: UnboundedSender<WorldEvent>,
    status: watch::Sender<String>,
) {
    let (aux, mut missing) = exposure_sources(&client, &namespaces).await;
    // Nodes are cluster-wide: users limited to their namespaces often may not
    // list them, and then the stations come from the pods' node names alone.
    let node_api = Api::<Node>::all(client.clone());
    let mut nodes = match can_list(std::slice::from_ref(&node_api)).await {
        true => watcher(node_api, watcher::Config::default().any_semantic()).default_backoff().boxed(),
        false => {
            missing.insert(0, "Nodes");
            stream::pending().boxed()
        }
    };
    let mut node_tracker = NodeTracker::default();
    let live_text = match missing.is_empty() {
        true => format!("{context} · live"),
        false => format!("{context} · {}", limited(&missing)),
    };
    let aux_total = aux.len();
    let mut aux = stream::select_all(aux);
    let apis = pod_apis(&client, &namespaces);
    let mut trackers = vec![Tracker::default(); apis.len()];
    let mut live = vec![false; apis.len()];
    let streams = apis.into_iter().enumerate().map(|(i, api)| {
        watcher(api, watcher::Config::default().any_semantic()).default_backoff().map(move |event| (i, event)).boxed()
    });
    let mut merged = stream::select_all(streams);
    let mut exposures = Exposures::default();
    let mut held: Option<Vec<WorldEvent>> = Some(Vec::new());
    let mut dirty = false;
    let wait = tokio::time::sleep(EXPOSURE_WAIT);
    tokio::pin!(wait);
    let mut every = tokio::time::interval(EXPOSURE_EVERY);
    every.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    loop {
        tokio::select! {
            next = merged.next() => {
                let Some((i, result)) = next else { return };
                match result {
                    Ok(event) => {
                        if matches!(event, watcher::Event::InitDone) {
                            live[i] = true;
                            if live.iter().all(|&ok| ok) {
                                status.send_replace(live_text.clone());
                            }
                        }
                        if let watcher::Event::InitApply(pod) | watcher::Event::Apply(pod) = &event
                            && let Some((uid, labels)) = pod_labels(pod)
                        {
                            dirty |= exposures.pod_seen(&uid, labels);
                        }
                        for change in trackers[i].handle(event) {
                            if let WorldEvent::PodDeleted { uid } = &change {
                                dirty |= exposures.pod_gone(uid);
                            }
                            match &mut held {
                                Some(held) => held.push(change),
                                None => {
                                    if events.send(change).is_err() {
                                        return;
                                    }
                                }
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
            Some(fact) = aux.next(), if aux_total > 0 => dirty |= fact.apply(&mut exposures),
            // A dropped node stream retries with backoff; the pods' stream
            // reports the trouble.
            Some(Ok(event)) = nodes.next() => {
                for change in node_tracker.handle(event) {
                    match &mut held {
                        Some(held) => held.push(change),
                        None => {
                            if events.send(change).is_err() {
                                return;
                            }
                        }
                    }
                }
            }
            () = &mut wait, if held.is_some() => {
                if !release(&mut held, &mut exposures, &events) {
                    return;
                }
                dirty = false;
            }
            _ = every.tick(), if held.is_none() && dirty => {
                dirty = false;
                for change in exposures.changes() {
                    if events.send(change).is_err() {
                        return;
                    }
                }
            }
        }
        let aux_ready = exposures.services.values().filter(|store| store.ready).count()
            + exposures.backends.values().filter(|store| store.ready).count();
        if held.is_some() && live.iter().all(|&ok| ok) && aux_ready >= aux_total {
            if !release(&mut held, &mut exposures, &events) {
                return;
            }
            dirty = false;
        }
    }
}

/// Sends the first exposure, then the pods held for it; false when the
/// receiver has gone.
fn release(
    held: &mut Option<Vec<WorldEvent>>,
    exposures: &mut Exposures,
    events: &UnboundedSender<WorldEvent>,
) -> bool {
    let pods = held.take().unwrap_or_default();
    exposures.changes().into_iter().chain(pods).all(|event| events.send(event).is_ok())
}

/// The status after the context when Nodes or some of what exposure needs
/// cannot be listed; `--anonymise` lets it through, it names no object.
pub fn limited(missing: &[&str]) -> String {
    format!("live · no access to {}; {} may be missing", missing.join(", "), what_is_missing(missing))
}

fn what_is_missing(missing: &[&str]) -> &'static str {
    match (missing.contains(&"Nodes"), missing.iter().any(|kind| *kind != "Nodes")) {
        (true, true) => "power stations and shops",
        (true, false) => "power stations",
        _ => "shops",
    }
}

/// One change from the watchers exposure is worked out from.
#[expect(clippy::large_enum_variant, reason = "facts are passed one at a time; boxing every object buys nothing")]
enum Fact {
    Services(usize, watcher::Event<Service>),
    Ingresses(usize, watcher::Event<Ingress>),
    Routes(usize, Via, watcher::Event<DynamicObject>),
    /// A stream lost its connection; it retries with backoff, and the pods'
    /// stream reports the trouble.
    Dropped,
}

impl Fact {
    /// True when exposure may have changed.
    fn apply(self, exposures: &mut Exposures) -> bool {
        match self {
            Fact::Services(i, event) => exposures.services.entry(i).or_default().handle(event, exposure::service_facts),
            Fact::Ingresses(i, event) => {
                exposures.backends.entry(i).or_default().handle(event, exposure::ingress_backends)
            }
            Fact::Routes(i, via, event) => {
                exposures.backends.entry(i).or_default().handle(event, |route| exposure::route_backends(route, via))
            }
            Fact::Dropped => false,
        }
    }
}

/// The watchers exposure needs, for what may be listed, and what may not.
/// A cluster without the Gateway API simply has no routes.
async fn exposure_sources(
    client: &Client,
    namespaces: &[String],
) -> (Vec<BoxStream<'static, Fact>>, Vec<&'static str>) {
    let mut streams = Vec::new();
    let mut missing = Vec::new();
    let services = scoped(namespaces, || Api::<Service>::all(client.clone()), |n| Api::namespaced(client.clone(), n));
    if can_list(&services).await {
        for api in services {
            let i = streams.len();
            streams.push(watch_stream(api, move |event| Fact::Services(i, event)));
        }
    } else {
        missing.push("Services");
    }
    let ingresses = scoped(namespaces, || Api::<Ingress>::all(client.clone()), |n| Api::namespaced(client.clone(), n));
    if can_list(&ingresses).await {
        for api in ingresses {
            let i = streams.len();
            streams.push(watch_stream(api, move |event| Fact::Ingresses(i, event)));
        }
    } else {
        missing.push("Ingresses");
    }
    let (group, version) = GATEWAY_API;
    let served = client.list_api_group_resources(&format!("{group}/{version}")).await.map(|list| list.resources);
    let mut routes_missing = false;
    for (plural, kind, via) in ROUTES {
        if !served.as_ref().is_ok_and(|resources| resources.iter().any(|r| r.name == plural)) {
            continue;
        }
        let resource = ApiResource::from_gvk_with_plural(&GroupVersionKind::gvk(group, version, kind), plural);
        let routes = scoped(
            namespaces,
            || Api::<DynamicObject>::all_with(client.clone(), &resource),
            |n| Api::namespaced_with(client.clone(), n, &resource),
        );
        if !can_list(&routes).await {
            routes_missing = true;
            continue;
        }
        for api in routes {
            let i = streams.len();
            streams.push(watch_stream(api, move |event| Fact::Routes(i, via, event)));
        }
    }
    if routes_missing {
        missing.push("Gateway routes");
    }
    (streams, missing)
}

/// One Api for the whole cluster, or one per `--namespace`.
fn scoped<K>(namespaces: &[String], all: impl Fn() -> Api<K>, one: impl Fn(&str) -> Api<K>) -> Vec<Api<K>> {
    if namespaces.is_empty() { vec![all()] } else { namespaces.iter().map(|namespace| one(namespace)).collect() }
}

async fn can_list<K>(apis: &[Api<K>]) -> bool
where
    K: Resource + Clone + DeserializeOwned + std::fmt::Debug,
{
    for api in apis {
        if api.list(&ListParams::default().limit(1)).await.is_err() {
            return false;
        }
    }
    true
}

fn watch_stream<K>(api: Api<K>, fact: impl Fn(watcher::Event<K>) -> Fact + Send + 'static) -> BoxStream<'static, Fact>
where
    K: Resource + Clone + DeserializeOwned + std::fmt::Debug + Send + 'static,
{
    watcher(api, watcher::Config::default().any_semantic())
        .default_backoff()
        .map(move |result| result.map_or(Fact::Dropped, &fact))
        .boxed()
}

/// What exposure needs of a pod: its labels and its workload.
fn pod_labels(pod: &Pod) -> Option<(String, PodLabels)> {
    let meta = &pod.metadata;
    let (workload, kind) = workload_of(pod);
    let labels = meta.labels.clone().unwrap_or_default();
    Some((meta.uid.clone()?, PodLabels { namespace: meta.namespace.clone()?, labels, workload, kind }))
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

/// The nodes one watch stream has reported, as last sent, so only what the
/// city shows of a node is sent again, and nodes deleted while the stream
/// was reconnecting go when it lists them again.
#[derive(Debug, Default)]
struct NodeTracker {
    known: HashMap<String, NodeInfo>,
    relisted: Option<HashSet<String>>,
}

impl NodeTracker {
    fn handle(&mut self, event: watcher::Event<Node>) -> Vec<WorldEvent> {
        match event {
            watcher::Event::Init => {
                self.relisted = Some(HashSet::new());
                Vec::new()
            }
            watcher::Event::InitApply(node) | watcher::Event::Apply(node) => {
                let Some(info) = node_info(&node) else {
                    return Vec::new();
                };
                if let Some(relisted) = &mut self.relisted {
                    relisted.insert(info.name.clone());
                }
                if self.known.get(&info.name) == Some(&info) {
                    return Vec::new();
                }
                self.known.insert(info.name.clone(), info.clone());
                vec![WorldEvent::Node(info)]
            }
            watcher::Event::Delete(node) => match node.metadata.name {
                Some(name) if self.known.remove(&name).is_some() => vec![WorldEvent::NodeGone { name }],
                _ => Vec::new(),
            },
            watcher::Event::InitDone => {
                let relisted = self.relisted.take().unwrap_or_default();
                let mut gone: Vec<String> =
                    self.known.keys().filter(|name| !relisted.contains(*name)).cloned().collect();
                gone.sort();
                for name in &gone {
                    self.known.remove(name);
                }
                gone.into_iter().map(|name| WorldEvent::NodeGone { name }).collect()
            }
        }
    }
}

fn node_info(node: &Node) -> Option<NodeInfo> {
    let meta = &node.metadata;
    let status = node.status.as_ref();
    let conditions = status.and_then(|status| status.conditions.as_deref()).unwrap_or_default();
    let is_true = |kind: &str| conditions.iter().any(|c| c.type_ == kind && c.status == "True");
    let labels = meta.labels.as_ref();
    let has_label = |key: &str| labels.is_some_and(|labels| labels.contains_key(key));
    let capacity = |resource: &str, unit: f64| {
        let quantity = status?.capacity.as_ref()?.get(resource)?;
        let amount = (quantity_of(&quantity.0)? * unit).round();
        (amount >= 1.0).then_some(amount as u64)
    };
    Some(NodeInfo {
        name: meta.name.clone()?,
        ready: is_true("Ready"),
        pressure: PRESSURE.iter().filter(|kind| is_true(kind)).map(|kind| kind.to_string()).collect(),
        cordoned: node.spec.as_ref().and_then(|spec| spec.unschedulable).unwrap_or(false),
        control_plane: has_label("node-role.kubernetes.io/control-plane")
            || has_label("node-role.kubernetes.io/master"),
        cpu: capacity("cpu", 1000.0),
        memory: capacity("memory", 1.0),
        kubelet: status.and_then(|status| Some(status.node_info.as_ref()?.kubelet_version.clone())),
        created: meta
            .creation_timestamp
            .as_ref()
            .and_then(|time| serde_json::to_value(time).ok()?.as_str().map(str::to_owned)),
    })
}

fn pod_info(pod: &Pod) -> Option<PodInfo> {
    let meta = &pod.metadata;
    let (workload, kind) = workload_of(pod);
    let details = details_of(pod);
    Some(PodInfo {
        uid: meta.uid.clone()?,
        namespace: meta.namespace.clone()?,
        role: Some(role_of(kind, &details.images, &workload)),
        workload,
        kind,
        state: state_of(pod),
        details,
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
        cpu: spec.and_then(|spec| requested(spec, "cpu", 1000.0)),
        memory: spec.and_then(|spec| requested(spec, "memory", 1.0)),
    }
}

/// What a pod requests of one resource, as the scheduler counts it: its
/// containers and sidecars (init containers that keep running) together, or
/// an init container plus the sidecars started before it, if that is more.
/// In `unit`s of the resource (1000 for millicores); `None` for nothing.
fn requested(spec: &PodSpec, resource: &str, unit: f64) -> Option<u64> {
    let request = |container: &Container| {
        let quantity = container.resources.as_ref()?.requests.as_ref()?.get(resource)?;
        quantity_of(&quantity.0)
    };
    let mut sidecars = 0.0;
    let mut init = 0.0_f64;
    for container in spec.init_containers.iter().flatten() {
        let amount = request(container).unwrap_or(0.0);
        if container.restart_policy.as_deref() == Some("Always") {
            sidecars += amount;
        } else {
            init = init.max(sidecars + amount);
        }
    }
    let running: f64 = spec.containers.iter().filter_map(request).sum::<f64>() + sidecars;
    let total = (running.max(init) * unit).round();
    (total >= 1.0).then_some(total as u64)
}

/// A Kubernetes quantity (`500m`, `1.5`, `2Gi`, `512M`, `1e3`) as a number.
fn quantity_of(text: &str) -> Option<f64> {
    const SUFFIXES: [(&str, f64); 15] = [
        ("Ki", 1024.0),
        ("Mi", 1_048_576.0),
        ("Gi", 1_073_741_824.0),
        ("Ti", 1_099_511_627_776.0),
        ("Pi", 1_125_899_906_842_624.0),
        ("Ei", 1_152_921_504_606_846_976.0),
        ("n", 1e-9),
        ("u", 1e-6),
        ("m", 1e-3),
        ("k", 1e3),
        ("M", 1e6),
        ("G", 1e9),
        ("T", 1e12),
        ("P", 1e15),
        ("E", 1e18),
    ];
    let text = text.trim();
    // A plain number, or one with an exponent ("1e3", "1E3").
    if let Ok(number) = text.parse::<f64>() {
        return number.is_finite().then_some(number);
    }
    SUFFIXES.iter().find_map(|(suffix, scale)| {
        let number: f64 = text.strip_suffix(suffix)?.parse().ok()?;
        number.is_finite().then_some(number * scale)
    })
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
                WorldEvent::Exposed(e) => ("exposed", e.workload.as_str()),
                WorldEvent::Node(node) => ("node", node.name.as_str()),
                WorldEvent::NodeGone { name } => ("node gone", name.as_str()),
            })
            .collect();
        out.sort_unstable();
        out
    }

    fn node(name: &str, ready: &str, extra: Value) -> Node {
        let mut manifest = json!({
            "metadata": { "name": name, "labels": { "node-role.kubernetes.io/control-plane": "" },
                          "creationTimestamp": "2026-10-01T08:00:00Z" },
            "spec": { "unschedulable": true },
            "status": {
                "capacity": { "cpu": "8", "memory": "32Gi", "pods": "110" },
                "nodeInfo": { "kubeletVersion": "v1.31.2", "machineID": "", "systemUUID": "", "bootID": "",
                              "kernelVersion": "", "osImage": "", "containerRuntimeVersion": "",
                              "kubeProxyVersion": "", "operatingSystem": "linux", "architecture": "amd64" },
                "conditions": [
                    { "type": "Ready", "status": ready },
                    { "type": "MemoryPressure", "status": "True" },
                    { "type": "DiskPressure", "status": "False" }
                ]
            }
        });
        if let (Some(target), Value::Object(extra)) = (manifest.as_object_mut(), extra) {
            target.extend(extra);
        }
        serde_json::from_value(manifest).expect("valid node")
    }

    #[test]
    fn a_node_reads_as_its_station() {
        let info = node_info(&node("cp-1", "True", json!({}))).unwrap();
        assert_eq!(
            info,
            NodeInfo {
                name: "cp-1".into(),
                ready: true,
                pressure: vec!["MemoryPressure".into()],
                cordoned: true,
                control_plane: true,
                cpu: Some(8000),
                memory: Some(32 << 30),
                kubelet: Some("v1.31.2".into()),
                created: Some("2026-10-01T08:00:00Z".into()),
            }
        );
        let bare = node_info(&node("w", "Unknown", json!({ "metadata": { "name": "w" }, "spec": {}, "status": {} })));
        let bare = bare.unwrap();
        assert!(!bare.ready && !bare.cordoned && !bare.control_plane && bare.pressure.is_empty() && bare.cpu.is_none());
    }

    #[test]
    fn nodes_are_sent_when_they_change_and_go_when_gone() {
        let mut tracker = NodeTracker::default();
        let names = |events: Vec<WorldEvent>| -> Vec<String> {
            events
                .into_iter()
                .map(|event| match event {
                    WorldEvent::Node(n) => format!("{} {}", n.name, n.ready),
                    WorldEvent::NodeGone { name } => format!("{name} gone"),
                    other => panic!("{other:?}"),
                })
                .collect()
        };
        tracker.handle(watcher::Event::Init);
        assert_eq!(names(tracker.handle(watcher::Event::InitApply(node("a", "True", json!({}))))), ["a true"]);
        assert_eq!(names(tracker.handle(watcher::Event::InitApply(node("b", "True", json!({}))))), ["b true"]);
        tracker.handle(watcher::Event::InitDone);
        // A heartbeat that changes nothing the city shows is not sent.
        assert!(tracker.handle(watcher::Event::Apply(node("a", "True", json!({})))).is_empty());
        assert_eq!(names(tracker.handle(watcher::Event::Apply(node("a", "False", json!({}))))), ["a false"]);
        // b is deleted while the stream reconnects.
        tracker.handle(watcher::Event::Init);
        assert!(tracker.handle(watcher::Event::InitApply(node("a", "False", json!({})))).is_empty());
        assert_eq!(names(tracker.handle(watcher::Event::InitDone)), ["b gone"]);
        assert_eq!(names(tracker.handle(watcher::Event::Delete(node("a", "False", json!({}))))), ["a gone"]);
    }

    #[test]
    fn the_status_says_what_may_be_missing() {
        assert!(limited(&["Nodes"]).ends_with("Nodes; power stations may be missing"));
        assert!(limited(&["Services"]).ends_with("Services; shops may be missing"));
        assert!(limited(&["Nodes", "Ingresses"]).ends_with("power stations and shops may be missing"));
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
    fn quantities_read_like_kubernetes_writes_them() {
        let close = |text: &str, want: f64| {
            let got = quantity_of(text).unwrap_or_else(|| panic!("{text} parses"));
            assert!((got - want).abs() <= want.abs() * 1e-9, "{text}: {got} != {want}");
        };
        close("500m", 0.5);
        close("1.5", 1.5);
        close("2", 2.0);
        close("2Gi", 2.0 * 1024.0 * 1024.0 * 1024.0);
        close("512Mi", 512.0 * 1024.0 * 1024.0);
        close("512M", 512e6);
        close("1e3", 1000.0);
        close("1E", 1e18);
        close("100k", 1e5);
        close("250000u", 0.25);
        assert_eq!(quantity_of("lots"), None);
        assert_eq!(quantity_of(""), None);
    }

    #[test]
    fn requests_count_as_the_scheduler_counts_them() {
        let resources = |cpu: &str, memory: &str| json!({ "requests": { "cpu": cpu, "memory": memory } });
        let pod = pod(json!({
            "metadata": { "uid": "u", "name": "p", "namespace": "shop" },
            "spec": {
                "initContainers": [
                    { "name": "migrate", "resources": resources("2", "1Gi") },
                    { "name": "mesh", "restartPolicy": "Always", "resources": resources("100m", "64Mi") },
                    { "name": "warm", "resources": resources("200m", "3Gi") }
                ],
                "containers": [
                    { "name": "app", "resources": resources("500m", "1Gi") },
                    { "name": "log", "resources": { "requests": { "cpu": "50m" } } },
                    { "name": "bare" }
                ]
            }
        }));
        let details = pod_info(&pod).expect("a pod").details;
        // CPU: the first init container (2) beats the running ones with the sidecar (0.65).
        assert_eq!(details.cpu, Some(2000));
        // Memory: the last init container with the sidecar started before it (3Gi + 64Mi)
        // beats the running ones (1Gi + 64Mi).
        assert_eq!(details.memory, Some((3 * 1024 + 64) * 1024 * 1024));
        let bare = pod_info(&running("lonely")).expect("a pod").details;
        assert_eq!((bare.cpu, bare.memory), (None, None));
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
