//! Which workloads are reached from outside the cluster (#50): Services,
//! Ingresses and Gateway routes matched to the pods' labels. The server keeps
//! the labels and selectors; the page only gets each workload's ways in.

use std::collections::{BTreeMap, BTreeSet, HashMap};

use k8s_openapi::api::{core::v1::Service, networking::v1::Ingress};
use kube::{ResourceExt, core::DynamicObject, runtime::watcher};
use serde_json::Value;

use crate::world::{Exposure, Via, Way, WorkloadKind, WorldEvent};

type Labels = BTreeMap<String, String>;
/// A workload: its namespace, kind and name.
type WorkloadKey = (String, WorkloadKind, String);

/// What exposure needs of a pod.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PodLabels {
    pub namespace: String,
    pub labels: Labels,
    pub workload: String,
    pub kind: WorkloadKind,
}

/// A Service, as far as exposure goes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServiceFacts {
    namespace: String,
    name: String,
    selector: Labels,
    /// LoadBalancer or NodePort; `None` for a Service only reached inside.
    outside: Option<Via>,
    /// The load balancer's ports, or the node ports.
    ports: Vec<String>,
    address: Option<String>,
}

/// Where an Ingress rule or a route's rule sends traffic.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Backend {
    via: Via,
    namespace: String,
    service: String,
    hosts: Vec<String>,
    paths: Vec<String>,
    port: Option<String>,
}

/// The objects of one watch stream, by namespace and name. A relist (after
/// a dropped connection) replaces them all once it is complete, so objects
/// deleted meanwhile go.
#[derive(Debug)]
pub struct Store<T> {
    items: HashMap<String, T>,
    relist: Option<HashMap<String, T>>,
    /// The first list is complete.
    pub ready: bool,
}

impl<T> Default for Store<T> {
    fn default() -> Self {
        Self { items: HashMap::new(), relist: None, ready: false }
    }
}

impl<T> Store<T> {
    /// Applies one watch event; true when the objects changed.
    pub fn handle<K: kube::Resource>(&mut self, event: watcher::Event<K>, read: impl Fn(&K) -> Option<T>) -> bool {
        let key = |object: &K| format!("{}/{}", object.meta().namespace.as_deref().unwrap_or(""), object.name_any());
        match event {
            watcher::Event::Init => {
                self.relist = Some(HashMap::new());
                false
            }
            watcher::Event::InitApply(object) => {
                if let (Some(relist), Some(item)) = (&mut self.relist, read(&object)) {
                    relist.insert(key(&object), item);
                }
                false
            }
            watcher::Event::InitDone => {
                self.items = self.relist.take().unwrap_or_default();
                self.ready = true;
                true
            }
            watcher::Event::Apply(object) => {
                match read(&object) {
                    Some(item) => self.items.insert(key(&object), item),
                    None => self.items.remove(&key(&object)),
                };
                true
            }
            watcher::Event::Delete(object) => self.items.remove(&key(&object)).is_some(),
        }
    }

    fn values(&self) -> impl Iterator<Item = &T> {
        self.items.values()
    }
}

/// Everything exposure is worked out from, and what was last sent.
#[derive(Debug, Default)]
pub struct Exposures {
    pods: HashMap<String, PodLabels>,
    pub services: HashMap<usize, Store<ServiceFacts>>,
    /// Ingresses' and routes' backends, one store per watch stream.
    pub backends: HashMap<usize, Store<Vec<Backend>>>,
    sent: HashMap<WorkloadKey, Vec<Way>>,
}

impl Exposures {
    /// A pod was listed or changed; true when that can change exposure.
    pub fn pod_seen(&mut self, uid: &str, pod: PodLabels) -> bool {
        if self.pods.get(uid) == Some(&pod) {
            return false;
        }
        self.pods.insert(uid.to_owned(), pod);
        true
    }

    pub fn pod_gone(&mut self, uid: &str) -> bool {
        self.pods.remove(uid).is_some()
    }

    /// Every exposed workload's ways in, worked out afresh.
    fn compute(&self) -> HashMap<WorkloadKey, Vec<Way>> {
        let mut pods_in: HashMap<&str, Vec<&PodLabels>> = HashMap::new();
        for pod in self.pods.values() {
            pods_in.entry(&pod.namespace).or_default().push(pod);
        }
        let services: HashMap<(&str, &str), &ServiceFacts> = self
            .services
            .values()
            .flat_map(Store::values)
            .map(|service| ((service.namespace.as_str(), service.name.as_str()), service))
            .collect();
        let selected = |service: &ServiceFacts| -> BTreeSet<WorkloadKey> {
            if service.selector.is_empty() {
                return BTreeSet::new();
            }
            pods_in
                .get(service.namespace.as_str())
                .into_iter()
                .flatten()
                .filter(|pod| service.selector.iter().all(|(key, value)| pod.labels.get(key) == Some(value)))
                .map(|pod| (pod.namespace.clone(), pod.kind, pod.workload.clone()))
                .collect()
        };

        #[derive(Default)]
        struct Gathered {
            hosts: BTreeSet<String>,
            paths: BTreeSet<String>,
            ports: BTreeSet<String>,
            address: Option<String>,
        }
        let mut found: HashMap<WorkloadKey, BTreeMap<(Via, String), Gathered>> = HashMap::new();
        for service in services.values() {
            let Some(via) = service.outside else { continue };
            for workload in selected(service) {
                let way = found.entry(workload).or_default().entry((via, service.name.clone())).or_default();
                way.ports.extend(service.ports.iter().cloned());
                way.address = way.address.take().or_else(|| service.address.clone());
            }
        }
        for backend in self.backends.values().flat_map(Store::values).flatten() {
            let Some(service) = services.get(&(backend.namespace.as_str(), backend.service.as_str())) else {
                continue;
            };
            for workload in selected(service) {
                let way = found.entry(workload).or_default().entry((backend.via, service.name.clone())).or_default();
                way.hosts.extend(backend.hosts.iter().cloned());
                way.paths.extend(backend.paths.iter().cloned());
                way.ports.extend(backend.port.iter().cloned());
            }
        }
        found
            .into_iter()
            .map(|(workload, ways)| {
                let ways = ways
                    .into_iter()
                    .map(|((via, service), way)| Way {
                        via,
                        service,
                        hosts: way.hosts.into_iter().collect(),
                        paths: way.paths.into_iter().collect(),
                        ports: sorted_ports(way.ports),
                        address: way.address,
                    })
                    .collect();
                (workload, ways)
            })
            .collect()
    }

    /// What changed since the last call, as events: a workload's new ways,
    /// or none once it is not exposed any more. Sorted, so the same changes
    /// always come in the same order.
    pub fn changes(&mut self) -> Vec<WorldEvent> {
        let now = self.compute();
        let mut changed: Vec<(WorkloadKey, Vec<Way>)> = now
            .iter()
            .filter(|(workload, ways)| self.sent.get(*workload) != Some(*ways))
            .map(|(workload, ways)| (workload.clone(), ways.clone()))
            .collect();
        changed
            .extend(self.sent.keys().filter(|workload| !now.contains_key(*workload)).map(|w| (w.clone(), Vec::new())));
        changed.sort();
        self.sent = now;
        changed
            .into_iter()
            .map(|((namespace, kind, workload), ways)| {
                WorldEvent::Exposed(Exposure { namespace, workload, kind, ways })
            })
            .collect()
    }
}

/// Numbers in numeric order, then names.
fn sorted_ports(ports: BTreeSet<String>) -> Vec<String> {
    let mut ports: Vec<String> = ports.into_iter().collect();
    ports.sort_by_key(|port| (port.parse::<u64>().map_or((1, 0), |n| (0, n)), port.clone()));
    ports
}

pub fn service_facts(service: &Service) -> Option<ServiceFacts> {
    let spec = service.spec.as_ref()?;
    let ports = spec.ports.iter().flatten();
    let (outside, ports): (Option<Via>, Vec<String>) = match spec.type_.as_deref() {
        Some("LoadBalancer") => (Some(Via::LoadBalancer), ports.map(|port| port.port.to_string()).collect()),
        Some("NodePort") => {
            (Some(Via::NodePort), ports.filter_map(|port| port.node_port).map(|n| n.to_string()).collect())
        }
        _ => (None, Vec::new()),
    };
    let address = service
        .status
        .as_ref()
        .and_then(|status| status.load_balancer.as_ref()?.ingress.as_ref()?.first().cloned())
        .and_then(|ingress| ingress.ip.or(ingress.hostname))
        .filter(|_| outside == Some(Via::LoadBalancer));
    Some(ServiceFacts {
        namespace: service.namespace()?,
        name: service.name_any(),
        selector: spec.selector.clone().unwrap_or_default(),
        outside,
        ports,
        address,
    })
}

pub fn ingress_backends(ingress: &Ingress) -> Option<Vec<Backend>> {
    let namespace = ingress.namespace()?;
    let spec = ingress.spec.as_ref()?;
    let backend = |service: &k8s_openapi::api::networking::v1::IngressServiceBackend, host: Option<&String>, path| {
        let port = service.port.as_ref().and_then(|port| port.number.map(|n| n.to_string()).or(port.name.clone()));
        Backend {
            via: Via::Ingress,
            namespace: namespace.clone(),
            service: service.name.clone(),
            hosts: host.into_iter().cloned().collect(),
            paths: path,
            port,
        }
    };
    let mut backends: Vec<Backend> = spec
        .default_backend
        .as_ref()
        .and_then(|default| default.service.as_ref())
        .map(|service| backend(service, None, Vec::new()))
        .into_iter()
        .collect();
    for rule in spec.rules.iter().flatten() {
        for path in rule.http.iter().flat_map(|http| &http.paths) {
            if let Some(service) = &path.backend.service {
                backends.push(backend(service, rule.host.as_ref(), path.path.iter().cloned().collect()));
            }
        }
    }
    Some(backends)
}

/// An HTTPRoute's or GRPCRoute's backends: Services only, in the route's
/// namespace unless a backend names another.
pub fn route_backends(route: &DynamicObject, via: Via) -> Option<Vec<Backend>> {
    let namespace = route.namespace()?;
    let spec = route.data.get("spec")?;
    let strings = |value: Option<&Value>| -> Vec<String> {
        value.and_then(Value::as_array).into_iter().flatten().filter_map(Value::as_str).map(str::to_owned).collect()
    };
    let hosts = strings(spec.get("hostnames"));
    let mut backends = Vec::new();
    for rule in spec.get("rules").and_then(Value::as_array).into_iter().flatten() {
        let paths: Vec<String> = rule
            .get("matches")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(|m| m.get("path")?.get("value")?.as_str().map(str::to_owned))
            .collect();
        for reference in rule.get("backendRefs").and_then(Value::as_array).into_iter().flatten() {
            let text = |key: &str| reference.get(key).and_then(Value::as_str);
            let core_service = text("kind").unwrap_or("Service") == "Service" && text("group").unwrap_or("").is_empty();
            let Some(service) = text("name").filter(|_| core_service) else { continue };
            backends.push(Backend {
                via,
                namespace: text("namespace").map_or_else(|| namespace.clone(), str::to_owned),
                service: service.to_owned(),
                hosts: hosts.clone(),
                paths: paths.clone(),
                port: reference.get("port").and_then(Value::as_u64).map(|n| n.to_string()),
            });
        }
    }
    Some(backends)
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    fn labels(pairs: &[(&str, &str)]) -> Labels {
        pairs.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect()
    }

    fn pod(namespace: &str, workload: &str, pairs: &[(&str, &str)]) -> PodLabels {
        PodLabels {
            namespace: namespace.into(),
            labels: labels(pairs),
            workload: workload.into(),
            kind: WorkloadKind::Deployment,
        }
    }

    fn service(manifest: Value) -> Service {
        serde_json::from_value(manifest).expect("a Service")
    }

    fn add_service(exposures: &mut Exposures, manifest: Value) {
        exposures.services.entry(0).or_default().handle(watcher::Event::Apply(service(manifest)), service_facts);
    }

    fn add_backends(exposures: &mut Exposures, stream: usize, backends: Vec<Backend>) {
        let store = exposures.backends.entry(stream).or_default();
        let key = format!("{stream}/{}", store.items.len());
        store.items.insert(key, backends);
    }

    fn exposed(events: &[WorldEvent]) -> Vec<(String, Vec<Way>)> {
        events
            .iter()
            .map(|event| match event {
                WorldEvent::Exposed(e) => (format!("{}/{}", e.namespace, e.workload), e.ways.clone()),
                other => panic!("not an exposure: {other:?}"),
            })
            .collect()
    }

    fn cluster_ip(namespace: &str, name: &str, app: &str) -> Value {
        json!({
            "metadata": { "name": name, "namespace": namespace },
            "spec": { "selector": { "app": app }, "ports": [{ "port": 80 }] }
        })
    }

    #[test]
    fn load_balancers_and_node_ports_expose_the_pods_they_select() {
        let mut exposures = Exposures::default();
        exposures.pod_seen("a", pod("shop", "web", &[("app", "web"), ("tier", "front")]));
        exposures.pod_seen("b", pod("shop", "db", &[("app", "db")]));
        exposures.pod_seen("c", pod("other", "web", &[("app", "web")]));
        add_service(
            &mut exposures,
            json!({
                "metadata": { "name": "web-lb", "namespace": "shop" },
                "spec": { "type": "LoadBalancer", "selector": { "app": "web" },
                          "ports": [{ "port": 443, "nodePort": 31443 }, { "port": 80, "nodePort": 30080 }] },
                "status": { "loadBalancer": { "ingress": [{ "ip": "203.0.113.7" }] } }
            }),
        );
        add_service(
            &mut exposures,
            json!({
                "metadata": { "name": "db-np", "namespace": "shop" },
                "spec": { "type": "NodePort", "selector": { "app": "db" }, "ports": [{ "port": 5432, "nodePort": 30432 }] }
            }),
        );
        add_service(&mut exposures, cluster_ip("shop", "web", "web"));
        let shown = exposed(&exposures.changes());
        let web = Way {
            via: Via::LoadBalancer,
            service: "web-lb".into(),
            hosts: vec![],
            paths: vec![],
            ports: vec!["80".into(), "443".into()],
            address: Some("203.0.113.7".into()),
        };
        let db = Way {
            via: Via::NodePort,
            service: "db-np".into(),
            ports: vec!["30432".into()],
            address: None,
            ..web.clone()
        };
        // Only in the Service's namespace; a ClusterIP Service alone exposes nothing.
        assert_eq!(shown, [("shop/db".to_owned(), vec![db]), ("shop/web".to_owned(), vec![web])]);
        // Nothing changed: nothing to send.
        assert!(exposures.changes().is_empty());
    }

    #[test]
    fn ingresses_reach_workloads_through_their_services() {
        let ingress: Ingress = serde_json::from_value(json!({
            "metadata": { "name": "front", "namespace": "shop" },
            "spec": {
                "defaultBackend": { "service": { "name": "web", "port": { "name": "http" } } },
                "rules": [
                    { "host": "shop.example.com", "http": { "paths": [
                        { "path": "/api", "pathType": "Prefix", "backend": { "service": { "name": "api", "port": { "number": 8080 } } } },
                        { "path": "/", "pathType": "Prefix", "backend": { "service": { "name": "web", "port": { "number": 80 } } } }
                    ] } },
                    { "host": "www.example.com", "http": { "paths": [
                        { "path": "/", "pathType": "Prefix", "backend": { "service": { "name": "web", "port": { "number": 80 } } } }
                    ] } }
                ]
            }
        }))
        .unwrap();
        let mut exposures = Exposures::default();
        exposures.pod_seen("a", pod("shop", "web", &[("app", "web")]));
        exposures.pod_seen("b", pod("shop", "api", &[("app", "api")]));
        add_service(&mut exposures, cluster_ip("shop", "web", "web"));
        add_service(&mut exposures, cluster_ip("shop", "api", "api"));
        add_backends(&mut exposures, 1, ingress_backends(&ingress).unwrap());
        let shown = exposed(&exposures.changes());
        assert_eq!(shown.len(), 2);
        let (api, web) = (&shown[0].1[0], &shown[1].1[0]);
        assert_eq!((api.via, api.service.as_str()), (Via::Ingress, "api"));
        assert_eq!(
            (api.hosts.clone(), api.paths.clone(), api.ports.clone()),
            (vec!["shop.example.com".into()], vec!["/api".into()], vec!["8080".into()])
        );
        // One way per Service, with every host, path and port that leads there.
        assert_eq!(web.hosts, ["shop.example.com", "www.example.com"]);
        assert_eq!(web.paths, ["/"]);
        assert_eq!(web.ports, ["80", "http"]);
    }

    #[test]
    fn routes_reach_services_in_their_own_or_another_namespace() {
        let route: DynamicObject = serde_json::from_value(json!({
            "apiVersion": "gateway.networking.k8s.io/v1", "kind": "HTTPRoute",
            "metadata": { "name": "r", "namespace": "edge" },
            "spec": {
                "hostnames": ["pay.example.com"],
                "rules": [{
                    "matches": [{ "path": { "type": "PathPrefix", "value": "/pay" } }],
                    "backendRefs": [
                        { "name": "payments", "namespace": "shop", "port": 8080 },
                        { "name": "local", "port": 80 },
                        { "name": "bucket", "kind": "Backend", "group": "example.com" }
                    ]
                }]
            }
        }))
        .unwrap();
        let backends = route_backends(&route, Via::HttpRoute).unwrap();
        assert_eq!(backends.len(), 2, "only Services count");
        assert_eq!((backends[0].namespace.as_str(), backends[1].namespace.as_str()), ("shop", "edge"));

        let mut exposures = Exposures::default();
        exposures.pod_seen("a", pod("shop", "payments", &[("app", "payments")]));
        add_service(&mut exposures, cluster_ip("shop", "payments", "payments"));
        add_backends(&mut exposures, 2, backends);
        let shown = exposed(&exposures.changes());
        let way = &shown[0].1[0];
        assert_eq!((shown[0].0.as_str(), way.via), ("shop/payments", Via::HttpRoute));
        assert_eq!(
            (way.hosts.clone(), way.paths.clone(), way.ports.clone()),
            (vec!["pay.example.com".into()], vec!["/pay".into()], vec!["8080".into()])
        );
    }

    #[test]
    fn exposure_follows_labels_and_ends_when_the_pods_go() {
        let mut exposures = Exposures::default();
        add_service(
            &mut exposures,
            json!({
                "metadata": { "name": "lb", "namespace": "shop" },
                "spec": { "type": "LoadBalancer", "selector": { "app": "web" }, "ports": [{ "port": 80 }] }
            }),
        );
        assert!(exposures.pod_seen("a", pod("shop", "web", &[("app", "web")])));
        assert!(!exposures.pod_seen("a", pod("shop", "web", &[("app", "web")])), "the same labels change nothing");
        assert_eq!(exposed(&exposures.changes())[0].0, "shop/web");
        // Relabelled: no longer behind the Service.
        exposures.pod_seen("a", pod("shop", "web", &[("app", "old")]));
        assert_eq!(exposed(&exposures.changes()), [("shop/web".to_owned(), vec![])]);
        exposures.pod_seen("a", pod("shop", "web", &[("app", "web")]));
        assert_eq!(exposed(&exposures.changes()).len(), 1);
        assert!(exposures.pod_gone("a"));
        assert_eq!(exposed(&exposures.changes()), [("shop/web".to_owned(), vec![])]);
        // A Service without a selector (its endpoints set by hand) selects nothing.
        exposures.pod_seen("b", pod("shop", "web", &[]));
        add_service(
            &mut exposures,
            json!({ "metadata": { "name": "manual", "namespace": "shop" }, "spec": { "type": "NodePort", "ports": [{ "port": 80, "nodePort": 30001 }] } }),
        );
        assert!(exposures.changes().is_empty());
    }

    #[test]
    fn a_relist_drops_services_deleted_meanwhile() {
        let mut store: Store<ServiceFacts> = Store::default();
        let lb = |name: &str| {
            service(json!({
                "metadata": { "name": name, "namespace": "shop" },
                "spec": { "type": "LoadBalancer", "selector": { "app": name } }
            }))
        };
        for event in [watcher::Event::Init, watcher::Event::InitApply(lb("a")), watcher::Event::InitApply(lb("b"))] {
            assert!(!store.handle(event, service_facts));
        }
        assert!(!store.ready);
        assert!(store.handle(watcher::Event::InitDone, service_facts));
        assert!(store.ready && store.items.len() == 2);
        store.handle(watcher::Event::Init, service_facts);
        store.handle(watcher::Event::InitApply(lb("b")), service_facts);
        store.handle(watcher::Event::InitDone, service_facts);
        assert_eq!(store.items.keys().collect::<Vec<_>>(), ["shop/b"]);
        assert!(store.handle(watcher::Event::Delete(lb("b")), service_facts));
        assert!(store.items.is_empty());
    }
}
