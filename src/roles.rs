//! What a workload does, from the images it runs (#48): the page zones a
//! neighbourhood's blocks by it. Worked out before `--anonymise` replaces
//! image names, so only the role word leaves the source.

use serde::{Deserialize, Serialize};

use crate::world::WorkloadKind;

/// A workload's part in its neighbourhood.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    /// Web servers and proxies: the street at the front.
    Web,
    /// Everything else: the offices in the middle.
    Other,
    /// Databases, caches and search engines: the utility block.
    Data,
    /// Brokers and queues: the utility block too.
    Messaging,
    /// Jobs and CronJobs: the working edge.
    Worker,
}

/// Well-known images per role, as the start of a word in the image's path
/// (`postgres` matches `bitnami/postgresql`, `nginx` matches
/// `ingress-nginx/controller`). Public
/// product names; the same lists are in `art/web_prototype.py`.
const DATA: &str = "cassandra clickhouse cockroach couchdb dragonfly elasticsearch etcd influxdb keydb mariadb memcached minio mongo mysql neo4j opensearch postgis postgres redis scylla timescale valkey";
const MESSAGING: &str = "activemq artemis emqx kafka mosquitto nats pulsar rabbitmq redpanda zookeeper";
const WEB: &str = "caddy envoy haproxy httpd kong nginx traefik";

/// Containers that ride along with the one that does the work, by the last
/// part of their path; skipped when a pod has another container.
const SIDECARS: &str = "cloud-sql-proxy consul-dataplane fluent-bit istio-proxy kube-rbac-proxy linkerd-proxy linkerd2-proxy oauth2-proxy proxyv2 vault";

/// The role of a workload of `kind` whose pod runs `images` (in the order of
/// the pod spec). Recordings made before kube-city kept images have none;
/// then the workload's name is looked up the same way.
pub fn role_of(kind: WorkloadKind, images: &[String], workload: &str) -> Role {
    if kind == WorkloadKind::Job {
        return Role::Worker;
    }
    let paths: Vec<String> = images.iter().map(|image| path(image)).collect();
    let main = paths.iter().find(|path| !is_sidecar(path)).or(paths.first());
    let name = main.map_or(workload, String::as_str);
    [(DATA, Role::Data), (MESSAGING, Role::Messaging), (WEB, Role::Web)]
        .into_iter()
        .find(|(names, _)| has_word(names, name))
        .map_or(Role::Other, |(_, role)| role)
}

/// An image's path, lower case, without its registry, tag or digest
/// (`registry.example:5000/team/web:1.4@sha256:…` → `team/web`).
fn path(image: &str) -> String {
    let image = image.split('@').next().unwrap_or(image);
    let mut parts: Vec<&str> = image.split('/').collect();
    if parts.len() > 1 && (parts[0].contains(['.', ':']) || parts[0] == "localhost") {
        parts.remove(0);
    }
    if let Some(last) = parts.last_mut() {
        *last = last.split(':').next().unwrap_or(last);
    }
    parts.join("/").to_ascii_lowercase()
}

fn is_sidecar(path: &str) -> bool {
    let last = path.rsplit('/').next().unwrap_or(path);
    SIDECARS.split(' ').any(|known| last == known)
}

/// Whether a word of `name` starts with one of `names`.
fn has_word(names: &str, name: &str) -> bool {
    let name = name.to_ascii_lowercase();
    name.split(['/', '-', '_', '.']).any(|word| names.split(' ').any(|known| word.starts_with(known)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use WorkloadKind::*;

    fn of(kind: WorkloadKind, images: &[&str]) -> Role {
        role_of(kind, &images.iter().map(|image| (*image).to_owned()).collect::<Vec<_>>(), "app")
    }

    #[test]
    fn well_known_images_get_their_role() {
        assert_eq!(of(StatefulSet, &["docker.io/bitnami/postgresql:16.2.0"]), Role::Data);
        assert_eq!(of(StatefulSet, &["redis:7-alpine"]), Role::Data);
        assert_eq!(of(Deployment, &["quay.io/strimzi/kafka:0.40.0-kafka-3.7.0"]), Role::Messaging);
        assert_eq!(of(Deployment, &["registry.k8s.io/ingress-nginx/controller:v1.10.0"]), Role::Web);
        assert_eq!(of(Deployment, &["nginx:1.27@sha256:0123abcd"]), Role::Web);
        assert_eq!(of(Deployment, &["registry.example:5000/team/orders-api:1.4"]), Role::Other);
        assert_eq!(of(DaemonSet, &["envoyproxy/envoy:v1.30"]), Role::Web);
    }

    #[test]
    fn jobs_are_workers_whatever_they_run() {
        assert_eq!(of(Job, &["postgres:16"]), Role::Worker);
        assert_eq!(of(Job, &[]), Role::Worker);
    }

    #[test]
    fn sidecars_do_not_decide() {
        assert_eq!(of(Deployment, &["docker.io/istio/proxyv2:1.22", "mysql:8"]), Role::Data);
        assert_eq!(of(Deployment, &["quay.io/brancz/kube-rbac-proxy:v0.18", "orders:1"]), Role::Other);
        // Unless there is nothing else.
        assert_eq!(of(Deployment, &["quay.io/oauth2-proxy/oauth2-proxy:v7"]), Role::Other);
        // The first container decides when none is a sidecar.
        assert_eq!(of(Deployment, &["orders:1", "envoyproxy/envoy:v1.30"]), Role::Other);
    }

    #[test]
    fn without_images_the_workload_name_decides() {
        assert_eq!(role_of(Deployment, &[], "argocd-redis"), Role::Data);
        assert_eq!(role_of(StatefulSet, &[], "orders-kafka"), Role::Messaging);
        assert_eq!(role_of(Deployment, &[], "argocd-server"), Role::Other);
    }

    #[test]
    fn whole_words_only_at_their_start() {
        // "nats" is a word start, not a substring anywhere.
        assert_eq!(of(Deployment, &["donations:1"]), Role::Other);
        assert_eq!(of(Deployment, &["nats-streaming:0.25"]), Role::Messaging);
    }

    #[test]
    fn the_role_is_sent_as_a_word() {
        assert_eq!(serde_json::to_string(&Role::Messaging).unwrap(), "\"messaging\"");
    }

    #[test]
    fn the_prototype_knows_the_same_images() {
        let prototype = include_str!("../art/web_prototype.py");
        for list in [DATA, MESSAGING, WEB, SIDECARS] {
            assert!(prototype.contains(&format!("\"{list}\"")), "art/web_prototype.py has another list: {list}");
        }
    }
}
