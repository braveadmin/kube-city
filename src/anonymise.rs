//! `--anonymise`: pseudonyms for every name, so a city can be shared.
//!
//! Namespaces, workloads, pod names and uids, nodes, images and contexts get
//! made-up names before they leave the server, and the status line loses
//! error text (which can hold host names). The same real name keeps its
//! pseudonym for the whole run; a workload keeps it in every namespace, so
//! landmarks still work. Pseudonyms are new every run unless a salt is
//! given: the code is public, so stable ones could be matched by guessing.

use std::{
    collections::{HashMap, HashSet},
    hash::{BuildHasher, RandomState},
    sync::{Arc, Mutex},
};

use sha2::{Digest, Sha256};
use tokio::sync::{
    mpsc::{self, UnboundedReceiver},
    watch,
};

use crate::world::{PodInfo, WorkloadKind, WorldEvent};

const WORDS: &[&str] = &[
    "alder", "amber", "aspen", "basalt", "birch", "bramble", "brook", "cedar", "cinder", "clover", "cobalt", "coral",
    "cypress", "dawn", "delta", "dune", "egret", "elm", "ember", "fern", "finch", "flint", "fjord", "garnet",
    "glacier", "granite", "harbor", "hazel", "heron", "hollow", "indigo", "iris", "jade", "juniper", "kelp", "kestrel",
    "lark", "larch", "linden", "lumen", "maple", "marble", "meadow", "mesa", "moss", "nimbus", "north", "nova", "oak",
    "ochre", "onyx", "orchid", "osprey", "pebble", "pine", "plover", "quartz", "quill", "raven", "reef", "ridge",
    "river", "rowan", "sable", "saffron", "sage", "slate", "sorrel", "spruce", "summit", "swift", "tansy", "teal",
    "thistle", "tide", "topaz", "tundra", "umber", "vale", "violet", "willow", "wren", "yarrow", "zephyr", "zinc",
];

/// Namespaces every cluster has; their names give nothing away.
const KEPT_NAMESPACES: &[&str] = &["default", "kube-system", "kube-public", "kube-node-lease"];

/// Branch namespaces keep their prefix, so they still read as branches.
const BRANCH_PREFIXES: &[&str] = &["feature-", "bugfix-"];

/// Well-known infrastructure, as a namespace's name or the start of it
/// ("cert-manager-webhooks"). These are public product names, so they are kept, and
/// the page still puts them in the industrial quarter. The same list is in
/// `art/city.template.html` and `art/web_prototype.py`.
const INFRA_NAMES: &str = "argo argocd calico cert-manager cilium crossplane external-dns external-secrets flux forgejo gatekeeper gitea gitlab grafana ingress istio jaeger keda kyverno linkerd logging loki longhorn metallb minio monitoring observability openbao prometheus rook tekton tigera traefik vault velero";

/// Other infrastructure namespaces keep the part that marks them as such.
const INFRA_SUFFIXES: &[&str] = &["-system", "-operator", "-operators"];

/// What an infrastructure namespace keeps of its name: the part before the
/// pseudonym and the part after it. `None` for any other namespace.
fn infrastructure(real: &str) -> Option<(&str, &str)> {
    let prefix = INFRA_NAMES
        .split(' ')
        .chain(["kube"])
        .find(|name| real.strip_prefix(name).is_some_and(|rest| rest.starts_with('-')));
    if let Some(name) = prefix {
        return Some((&real[..=name.len()], ""));
    }
    INFRA_SUFFIXES
        .iter()
        .find(|suffix| real.len() > suffix.len() && real.ends_with(*suffix))
        .map(|suffix| ("", *suffix))
}

/// What the status line may say after the context; anything else is error
/// text and is dropped.
const PLAIN_STATUSES: &[&str] = &["live", "connecting"];

/// One anonymiser for the whole run, shared by every source of events.
pub type Shared = Arc<Mutex<Anonymiser>>;

pub struct Anonymiser {
    salt: String,
    namespaces: Names,
    workloads: Names,
    nodes: Names,
    images: Names,
    contexts: Names,
}

impl Anonymiser {
    /// Stable pseudonyms for the same salt; `None` picks a random one.
    pub fn new(salt: Option<String>) -> Self {
        let salt = salt.unwrap_or_else(|| format!("{:016x}", RandomState::new().hash_one("kube-city")));
        Self {
            salt,
            namespaces: Names::default(),
            workloads: Names::default(),
            nodes: Names::default(),
            images: Names::default(),
            contexts: Names::default(),
        }
    }

    pub fn shared(salt: Option<String>) -> Shared {
        Arc::new(Mutex::new(Self::new(salt)))
    }

    fn hash(&self, what: &str, text: &str) -> u64 {
        let digest = Sha256::new().chain_update(&self.salt).chain_update([0]).chain_update(what).chain_update([0]);
        let digest = digest.chain_update(text).finalize();
        u64::from_be_bytes(digest[..8].try_into().expect("a SHA-256 digest has 32 bytes"))
    }

    pub fn event(&mut self, event: WorldEvent) -> WorldEvent {
        match event {
            WorldEvent::PodUpserted(pod) => WorldEvent::PodUpserted(self.pod(pod)),
            WorldEvent::PodDeleted { uid } => WorldEvent::PodDeleted { uid: self.uid(&uid) },
        }
    }

    fn pod(&mut self, pod: PodInfo) -> PodInfo {
        let workload = self.workload(&pod.workload, pod.kind);
        let mut details = pod.details;
        details.name = self.pod_name(&details.name, &pod.workload, &workload);
        if !details.node.is_empty() {
            details.node = self.node(&details.node);
        }
        details.images = details.images.iter().map(|image| self.image(image)).collect();
        // Reasons are Kubernetes' own words (OOMKilled, Evicted); anything
        // else could be a message.
        if let Some(reason) = &details.reason
            && !(reason.len() <= 40 && reason.bytes().all(|b| b.is_ascii_alphanumeric()))
        {
            details.reason = Some("Hidden".into());
        }
        PodInfo { uid: self.uid(&pod.uid), namespace: self.namespace(&pod.namespace), workload, details, ..pod }
    }

    fn uid(&self, uid: &str) -> String {
        format!("{:016x}", self.hash("uid", uid))
    }

    fn namespace(&mut self, real: &str) -> String {
        if KEPT_NAMESPACES.contains(&real) || INFRA_NAMES.split(' ').any(|name| name == real) {
            return real.to_owned();
        }
        let hash = self.hash("namespace", real);
        let prefix = BRANCH_PREFIXES.iter().find(|prefix| real.starts_with(*prefix)).copied();
        let infra = infrastructure(real);
        self.namespaces.pick(real, |i| match (prefix, infra) {
            (Some(prefix), _) => format!("{prefix}{}-{}", word(hash, i), 100 + (hash / 7 + i as u64 * 37) % 900),
            (None, Some((before, after))) => format!("{before}{}{after}", word(hash, i)),
            (None, None) => word(hash, i).to_owned(),
        })
    }

    /// The name only, not the namespace: the same component looks the same
    /// in every namespace.
    fn workload(&mut self, real: &str, kind: WorkloadKind) -> String {
        let suffixes: &[&str] = match kind {
            WorkloadKind::Deployment => &["api", "service", "worker", "gateway", "ui", "controller", "router"],
            WorkloadKind::StatefulSet => &["db", "store", "queue", "cache", "broker", "index", "ledger", "vault"],
            WorkloadKind::DaemonSet => &["agent", "exporter", "node-proxy", "collector"],
            WorkloadKind::Job => &["migrate", "seed", "backup", "report", "sync", "cleanup"],
            WorkloadKind::Standalone => &["debug", "toolbox", "probe"],
        };
        let key = format!("{kind:?}/{real}");
        let hash = self.hash("workload", &key);
        self.workloads
            .pick(&key, |i| format!("{}-{}", word(hash, i), suffixes[(hash / 3 + i as u64) as usize % suffixes.len()]))
    }

    /// The workload's pseudonym and a made-up suffix; a StatefulSet's ordinal
    /// (`db-0`) is kept.
    fn pod_name(&self, real: &str, real_workload: &str, workload: &str) -> String {
        if real.is_empty() {
            return String::new();
        }
        if let Some(ordinal) = real.strip_prefix(real_workload).and_then(|rest| rest.strip_prefix('-'))
            && !ordinal.is_empty()
            && ordinal.bytes().all(|b| b.is_ascii_digit())
        {
            return format!("{workload}-{ordinal}");
        }
        format!("{workload}-{}", base36(self.hash("pod", real), 5))
    }

    fn node(&mut self, real: &str) -> String {
        let hash = self.hash("node", real);
        self.nodes.pick(real, |i| format!("node-{}", word(hash, i)))
    }

    /// The repository becomes a word; a tag or digest becomes a short hash,
    /// so two versions of one image still look related.
    fn image(&mut self, real: &str) -> String {
        let name_start = real.rfind('/').map_or(0, |slash| slash + 1);
        let version = real.find('@').or_else(|| real[name_start..].find(':').map(|colon| name_start + colon));
        let repository = &real[..version.unwrap_or(real.len())];
        let hash = self.hash("image", repository);
        let name = self.images.pick(repository, |i| word(hash, i).to_owned());
        match version {
            Some(_) => format!("{name}:{}", base36(self.hash("version", real), 7)),
            None => name,
        }
    }

    pub fn context(&mut self, real: &str) -> String {
        let hash = self.hash("context", real);
        self.contexts.pick(real, |i| format!("cluster-{}", word(hash, i)))
    }

    /// The real context a page asked for by its pseudonym.
    pub fn real_context(&self, pseudonym: &str) -> Option<String> {
        self.contexts.real.iter().find(|(_, name)| *name == pseudonym).map(|(real, _)| real.clone())
    }

    /// `context · live` keeps its shape; error text is dropped. Statuses
    /// without a context (demo, replay) are left as they are.
    pub fn status(&mut self, text: &str) -> String {
        let Some((context, rest)) = text.split_once(" · ") else { return text.to_owned() };
        let rest = if PLAIN_STATUSES.contains(&rest) {
            rest
        } else if rest.starts_with("reconnecting") {
            "reconnecting"
        } else {
            "cannot connect (details hidden by --anonymise)"
        };
        format!("{} · {rest}", self.context(context))
    }
}

/// Pseudonyms of one kind: each real name gets its own, for the whole run.
#[derive(Default)]
struct Names {
    real: HashMap<String, String>,
    taken: HashSet<String>,
}

impl Names {
    /// The first free name from `candidate(0)`, `candidate(1)` …; once all
    /// words are taken, they come round again with a number.
    fn pick(&mut self, real: &str, candidate: impl Fn(usize) -> String) -> String {
        if let Some(name) = self.real.get(real) {
            return name.clone();
        }
        let name = (0..)
            .map(|step| match step / WORDS.len() {
                0 => candidate(step),
                round => format!("{}-{}", candidate(step), round + 1),
            })
            .find(|name| !self.taken.contains(name))
            .expect("there is always a free name");
        self.taken.insert(name.clone());
        self.real.insert(real.to_owned(), name.clone());
        name
    }
}

/// The `i`-th word after the one `hash` picks.
fn word(hash: u64, i: usize) -> &'static str {
    WORDS[(hash as usize % WORDS.len() + i) % WORDS.len()]
}

fn base36(mut value: u64, length: usize) -> String {
    (0..length)
        .map(|_| {
            let digit = (value % 36) as u32;
            value /= 36;
            char::from_digit(digit, 36).expect("below 36")
        })
        .collect()
}

/// Passes a source's events and status through `anonymiser`, until either
/// side goes away.
pub fn stage(
    anonymiser: Shared,
    mut events: UnboundedReceiver<WorldEvent>,
    mut status: watch::Receiver<String>,
) -> (UnboundedReceiver<WorldEvent>, watch::Receiver<String>) {
    let (events_tx, events_out) = mpsc::unbounded_channel();
    let first = anonymiser.lock().expect("anonymiser lock").status(&status.borrow_and_update());
    let (status_tx, status_out) = watch::channel(first);
    tokio::spawn(async move {
        let mut status_open = true;
        loop {
            tokio::select! {
                event = events.recv() => {
                    let Some(event) = event else { return };
                    let event = anonymiser.lock().expect("anonymiser lock").event(event);
                    if events_tx.send(event).is_err() {
                        return;
                    }
                }
                changed = status.changed(), if status_open => {
                    if changed.is_err() {
                        // A replay or the demo never changes its status.
                        status_open = false;
                        continue;
                    }
                    let text = anonymiser.lock().expect("anonymiser lock").status(&status.borrow_and_update());
                    status_tx.send_replace(text);
                }
            }
        }
    });
    (events_out, status_out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        recording,
        world::{PodDetails, PodState},
    };

    const REAL: &[&str] = &[
        "acme-payments",
        "feature-acme-4711",
        "ledger-writer",
        "ledger-writer-7d9f8c6b5-x2k4q",
        "0b6c1d2e-uid-of-a-real-pod",
        "ip-10-1-2-3.acme.internal",
        "registry.acme.example",
        "acme-prod-eu",
        "secret-host.acme.example",
    ];

    fn pod(namespace: &str, workload: &str, kind: WorkloadKind, name: &str) -> PodInfo {
        PodInfo {
            uid: "0b6c1d2e-uid-of-a-real-pod".into(),
            namespace: namespace.into(),
            workload: workload.into(),
            kind,
            state: PodState::CrashLoop,
            details: PodDetails {
                name: name.into(),
                node: "ip-10-1-2-3.acme.internal".into(),
                images: vec![
                    "registry.acme.example:5000/team/ledger-writer:1.2.3".into(),
                    "registry.acme.example:5000/team/ledger-writer@sha256:abc".into(),
                    "busybox".into(),
                ],
                restarts: 4,
                ready: 0,
                containers: 1,
                created: Some("2026-10-22T08:00:00Z".into()),
                reason: Some("CrashLoopBackOff".into()),
            },
        }
    }

    fn leaked(text: &str) -> Vec<&'static str> {
        REAL.iter().copied().filter(|real| text.contains(real) || text.contains("acme")).collect()
    }

    #[test]
    fn no_real_name_is_left_in_a_pod() {
        let mut anonymiser = Anonymiser::new(None);
        let real = pod("acme-payments", "ledger-writer", WorkloadKind::Deployment, "ledger-writer-7d9f8c6b5-x2k4q");
        let WorldEvent::PodUpserted(fake) = anonymiser.event(WorldEvent::PodUpserted(real.clone())) else {
            panic!("an upsert stays an upsert")
        };
        let text = serde_json::to_string(&fake).unwrap();
        assert_eq!(leaked(&text), [] as [&str; 0], "{text}");
        // What is not a name stays, so the city looks the same.
        assert_eq!((fake.kind, fake.state, fake.details.restarts), (real.kind, real.state, 4));
        assert_eq!(fake.details.reason.as_deref(), Some("CrashLoopBackOff"));
        assert!(fake.details.name.starts_with(&format!("{}-", fake.workload)), "{text}");
        // Two versions of one image keep one name.
        let [tagged, digest, plain] = &fake.details.images[..] else { panic!("three images") };
        assert_eq!(tagged.split(':').next(), digest.split(':').next());
        assert_ne!(tagged, digest);
        assert!(!plain.contains(':'));
        // A delete names the same pod as the upsert did.
        assert_eq!(
            anonymiser.event(WorldEvent::PodDeleted { uid: real.uid }),
            WorldEvent::PodDeleted { uid: fake.uid }
        );
    }

    #[test]
    fn names_hold_for_the_run_and_a_workload_keeps_its_name_everywhere() {
        let mut anonymiser = Anonymiser::new(None);
        let a = anonymiser.pod(pod("acme-payments", "ledger-writer", WorkloadKind::StatefulSet, "ledger-writer-0"));
        let b = anonymiser.pod(pod("feature-acme-4711", "ledger-writer", WorkloadKind::StatefulSet, "ledger-writer-1"));
        assert_eq!(a.workload, b.workload);
        assert_ne!(a.namespace, b.namespace);
        assert!(b.namespace.starts_with("feature-"), "{}", b.namespace);
        assert_eq!(b.details.name, format!("{}-1", b.workload));
        assert_eq!(anonymiser.namespace("acme-payments"), a.namespace);
        assert_eq!(anonymiser.namespace("kube-system"), "kube-system");
        assert_eq!(anonymiser.namespace("cert-manager"), "cert-manager");
        // Another kind of workload with the same name is another building.
        assert_ne!(anonymiser.workload("ledger-writer", WorkloadKind::Job), a.workload);
    }

    #[test]
    fn infrastructure_keeps_what_marks_it() {
        let mut anonymiser = Anonymiser::new(None);
        let gitlab = anonymiser.namespace("gitlab-acme-runners");
        assert!(gitlab.starts_with("gitlab-") && !gitlab.contains("acme"), "{gitlab}");
        let system = anonymiser.namespace("acme-system");
        assert!(system.ends_with("-system") && !system.contains("acme"), "{system}");
        let operator = anonymiser.namespace("acme-operator");
        assert!(operator.ends_with("-operator") && !operator.contains("acme"), "{operator}");
        let kube = anonymiser.namespace("kube-acme");
        assert!(kube.starts_with("kube-") && !kube.contains("acme"), "{kube}");
        // "argocd-…" is not "argo-…", and a plain name is a plain word.
        assert!(anonymiser.namespace("argocd-acme").starts_with("argocd-"));
        assert!(!anonymiser.namespace("acme-payments").contains('-'));
    }

    #[test]
    fn pseudonyms_never_read_as_infrastructure() {
        // A plain namespace's pseudonym is a word; none may be a kept name.
        for name in INFRA_NAMES.split(' ') {
            assert!(!WORDS.contains(&name), "{name}");
        }
    }

    #[test]
    fn the_page_and_the_prototype_know_the_same_infrastructure() {
        for (file, text) in [
            ("art/city.template.html", include_str!("../art/city.template.html")),
            ("art/web_prototype.py", include_str!("../art/web_prototype.py")),
        ] {
            assert!(text.contains(&format!("\"{INFRA_NAMES}\"")), "{file} has another list of infrastructure names");
        }
    }

    #[test]
    fn names_are_new_every_run_unless_salted() {
        let names = |salt: Option<&str>| {
            let mut anonymiser = Anonymiser::new(salt.map(str::to_owned));
            ["one", "two", "three", "four", "five"].map(|real| anonymiser.namespace(real))
        };
        assert_ne!(names(None), names(None));
        assert_eq!(names(Some("s3cret")), names(Some("s3cret")));
        assert_ne!(names(Some("s3cret")), names(Some("other")));
    }

    #[test]
    fn running_out_of_words_still_gives_unique_names() {
        let mut anonymiser = Anonymiser::new(None);
        let names: HashSet<String> = (0..WORDS.len() * 3).map(|i| anonymiser.node(&format!("node-{i}"))).collect();
        assert_eq!(names.len(), WORDS.len() * 3);
    }

    #[test]
    fn the_status_line_keeps_its_shape_but_not_error_text() {
        let mut anonymiser = Anonymiser::new(None);
        let context = anonymiser.context("acme-prod-eu");
        assert_eq!(anonymiser.status("acme-prod-eu · live"), format!("{context} · live"));
        assert_eq!(
            anonymiser.status("acme-prod-eu · reconnecting: dial secret-host.acme.example: refused"),
            format!("{context} · reconnecting")
        );
        let failed = anonymiser.status("acme-prod-eu · cannot list pods in acme-prod-eu: secret-host.acme.example");
        assert_eq!(leaked(&failed), [] as [&str; 0], "{failed}");
        assert_eq!(anonymiser.status("demo seed 7"), "demo seed 7");
        assert_eq!(anonymiser.real_context(&context).as_deref(), Some("acme-prod-eu"));
        assert_eq!(anonymiser.real_context("acme-prod-eu"), None);
    }

    #[test]
    fn odd_reasons_are_hidden() {
        let mut anonymiser = Anonymiser::new(None);
        let mut real = pod("acme-payments", "ledger-writer", WorkloadKind::Deployment, "x");
        real.details.reason = Some("failed to pull registry.acme.example/x".into());
        assert_eq!(anonymiser.pod(real).details.reason.as_deref(), Some("Hidden"));
    }

    #[tokio::test]
    async fn a_recording_made_with_anonymise_holds_no_real_name() {
        let path = std::env::temp_dir().join(format!("kube-city-{}-anonymised.jsonl", std::process::id()));
        let (source, events) = mpsc::unbounded_channel();
        let (status_tx, status) = watch::channel("acme-prod-eu · connecting".to_owned());
        let (events, mut status) = stage(Anonymiser::shared(None), events, status);
        let (to_city, mut city) = mpsc::unbounded_channel();
        let file = tokio::fs::File::create(&path).await.unwrap();
        let recorder = tokio::spawn(recording::tee(events, to_city, file));

        let real = pod("acme-payments", "ledger-writer", WorkloadKind::Deployment, "ledger-writer-7d9f8c6b5-x2k4q");
        source.send(WorldEvent::PodUpserted(real.clone())).unwrap();
        source.send(WorldEvent::PodDeleted { uid: real.uid }).unwrap();
        status_tx.send_replace("acme-prod-eu · live".into());
        status.changed().await.unwrap();
        drop(source);
        recorder.await.unwrap();

        let recorded = std::fs::read_to_string(&path).unwrap();
        std::fs::remove_file(&path).ok();
        assert_eq!(recorded.lines().count(), 2);
        assert_eq!(leaked(&recorded), [] as [&str; 0], "{recorded}");
        let mut shown = String::new();
        while let Ok(event) = city.try_recv() {
            shown += &serde_json::to_string(&event).unwrap();
        }
        shown += &status.borrow();
        assert_eq!(leaked(&shown), [] as [&str; 0], "{shown}");
        assert!(status.borrow().ends_with(" · live"));
    }
}
