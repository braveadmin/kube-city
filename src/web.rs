//! `--web`: the city in a browser.
//!
//! Serves the page, the Kenney sprites and the fonts from the binary on 127.0.0.1 and
//! streams world events to every open page over a WebSocket, as JSON arrays
//! of the same events a recording holds, plus `{"Status": text}`,
//! `{"Contexts": {"current", "all"}}` and `{"Reset": null}`. A page that
//! connects late first gets Reset, the contexts, the status and every
//! current pod; one that falls too far behind gets that again.
//!
//! When the city shows a cluster, a page can ask for another kubeconfig
//! context with `{"Context": name}`: every page then resets and the city
//! fills up from that context.
//!
//! With `--anonymise` the pages only ever see pseudonyms, context names
//! included; a page asks for a context by its pseudonym.

use std::{
    collections::HashMap,
    net::Ipv4Addr,
    process::{Command, Stdio},
    sync::Arc,
};

use anyhow::{Context, Result};
use axum::{
    Router,
    extract::{
        Path, State,
        ws::{Message, WebSocket, WebSocketUpgrade},
    },
    http::{HeaderMap, StatusCode, header},
    response::{Html, IntoResponse, Response},
    routing::get,
};
use serde_json::{Value, json};
use tokio::{
    sync::{
        Mutex, broadcast,
        mpsc::{self, UnboundedReceiver, UnboundedSender},
        watch,
    },
    task::JoinHandle,
};

use crate::{
    anonymise, cluster,
    world::{PodInfo, WorldEvent},
};

const TEMPLATE: &str = include_str!("../art/city.template.html");
include!(concat!(env!("OUT_DIR"), "/sprites.rs"));
/// PixiJS (MIT), served from the binary so the city needs no CDN.
const PIXI: &str = include_str!("../assets/pixi/pixi.min.js");
/// Where the template loads PixiJS from when it is published as a page.
const PIXI_CDN: &str = "https://cdnjs.cloudflare.com/ajax/libs/pixi.js/7.4.2/pixi.min.js";
include!(concat!(env!("OUT_DIR"), "/fonts.rs"));
/// The `@font-face` rules for `FONTS`, put into the page in place of `GOOGLE_FONTS`.
const FONTS_CSS: &str = include_str!("../assets/fonts/fonts.css");
/// How the template loads its fonts when it is published as a page.
const GOOGLE_FONTS: &str = r#"<link rel="preconnect" href="https://fonts.googleapis.com">
<link rel="preconnect" href="https://fonts.gstatic.com" crossorigin>
<link rel="stylesheet" href="https://fonts.googleapis.com/css2?family=Barlow+Condensed:wght@500;600;700&family=Barlow:wght@400;500;600&family=IBM+Plex+Mono:wght@400;500&display=swap">"#;

/// Batches a page may fall behind before it is sent the whole city again.
const BACKLOG: usize = 1024;
/// Events sent to the pages in one message at most.
const BATCH: usize = 2000;
const STATUS_ERROR_MAX: usize = 120;

/// The clusters a page may switch between, when the city shows a cluster.
pub struct Clusters {
    /// Every context in the kubeconfig.
    pub contexts: Vec<String>,
    /// The context shown at start.
    pub current: String,
    /// The `--namespace` filter, kept for every context.
    pub namespaces: Vec<String>,
}

/// What the pump gets, in order. Every source of events has a generation;
/// switching contexts starts a new one and drops whatever the old one still
/// sends.
enum Input {
    Event(u64, WorldEvent),
    Status(u64, String),
    Switch(u64, String),
}

/// The city as the pages see it.
#[derive(Default)]
struct City {
    pods: HashMap<String, PodInfo>,
    context: Option<String>,
}

#[derive(Clone)]
struct Shared {
    city: Arc<Mutex<City>>,
    batches: broadcast::Sender<Arc<str>>,
    status: watch::Receiver<String>,
    page: Arc<str>,
    port: u16,
    contexts: Arc<Vec<String>>,
    switcher: Option<Arc<Switcher>>,
    anonymiser: Option<anonymise::Shared>,
}

/// Serves the city until Ctrl+C, opening it in a browser when `open` is set.
pub async fn serve(
    events: UnboundedReceiver<WorldEvent>,
    status: watch::Receiver<String>,
    port: u16,
    open: bool,
    clusters: Option<Clusters>,
    anonymiser: Option<anonymise::Shared>,
) -> Result<()> {
    let listener = tokio::net::TcpListener::bind((Ipv4Addr::LOCALHOST, port))
        .await
        .with_context(|| format!("cannot listen on 127.0.0.1:{port}; is another kube-city running? Try --port"))?;
    let port = listener.local_addr()?.port();

    let (input, inputs) = mpsc::unbounded_channel();
    let first = tokio::spawn(forward(0, events, status, input.clone()));
    let (status_tx, status) = watch::channel(String::new());
    let city = City { pods: HashMap::new(), context: clusters.as_ref().map(|c| c.current.clone()) };
    let (contexts, switcher) = match clusters {
        Some(clusters) => {
            let switcher = Switcher {
                input,
                namespaces: clusters.namespaces,
                contexts: clusters.contexts.clone(),
                anonymiser: anonymiser.clone(),
                running: Mutex::new((0, first)),
            };
            (clusters.contexts, Some(Arc::new(switcher)))
        }
        None => (Vec::new(), None),
    };
    let shared = Shared {
        city: Arc::new(Mutex::new(city)),
        batches: broadcast::channel(BACKLOG).0,
        status,
        page: page().into(),
        port,
        contexts: Arc::new(contexts),
        switcher,
        anonymiser,
    };
    tokio::spawn(pump(inputs, status_tx, shared.clone()));

    let app = Router::new()
        .route("/", get(index))
        .route("/ws", get(socket))
        .route("/sprites/{file}", get(sprite))
        .route("/pixi.min.js", get(pixi))
        .route("/fonts/{file}", get(font))
        .with_state(shared);
    let url = format!("http://127.0.0.1:{port}/");
    eprintln!("kube-city: the city is at {url} (Ctrl+C to stop)");
    if open {
        open_browser(&url);
    }
    tokio::select! {
        served = axum::serve(listener, app) => served?,
        _ = tokio::signal::ctrl_c() => {}
    }
    Ok(())
}

/// The page with an empty live city and the sprites and fonts served next to it.
fn page() -> String {
    let data = json!({ "ns": [], "wl": [], "events": [], "live": true, "version": env!("CARGO_PKG_VERSION") });
    let sprites: serde_json::Map<String, Value> =
        SPRITES.iter().map(|(name, _)| (name.to_string(), Value::from(format!("sprites/{name}.png")))).collect();
    let body = TEMPLATE
        .replacen("/*CITY_DATA*/null", &data.to_string(), 1)
        .replacen("/*KENNEY_SPRITES*/null", &Value::Object(sprites).to_string(), 1)
        .replacen(PIXI_CDN, "pixi.min.js", 1)
        .replacen(GOOGLE_FONTS, &format!("<style>\n{FONTS_CSS}</style>"), 1);
    format!(
        "<!doctype html>\n<html lang=\"en\">\n<meta charset=\"utf-8\">\n\
         <meta name=\"viewport\" content=\"width=device-width, initial-scale=1, viewport-fit=cover\">\n{body}\n</html>\n"
    )
}

/// Passes one source's events and status on to the pump, tagged with its
/// generation, until either side goes away.
async fn forward(
    generation: u64,
    mut events: UnboundedReceiver<WorldEvent>,
    mut status: watch::Receiver<String>,
    input: UnboundedSender<Input>,
) {
    let first = status.borrow_and_update().clone();
    if input.send(Input::Status(generation, first)).is_err() {
        return;
    }
    let mut status_open = true;
    loop {
        tokio::select! {
            event = events.recv() => match event {
                Some(event) => {
                    if input.send(Input::Event(generation, event)).is_err() {
                        return;
                    }
                }
                None => return,
            },
            changed = status.changed(), if status_open => {
                if changed.is_err() {
                    // A replay or the demo never changes its status.
                    status_open = false;
                    continue;
                }
                let text = status.borrow_and_update().clone();
                if input.send(Input::Status(generation, text)).is_err() {
                    return;
                }
            }
        }
    }
}

/// Starts and stops the connection to whichever context the pages ask for.
struct Switcher {
    input: UnboundedSender<Input>,
    namespaces: Vec<String>,
    contexts: Vec<String>,
    anonymiser: Option<anonymise::Shared>,
    /// The current generation and the task feeding it.
    running: Mutex<(u64, JoinHandle<()>)>,
}

impl Switcher {
    async fn switch(&self, context: &str) {
        if !self.contexts.iter().any(|known| known == context) {
            return;
        }
        let mut running = self.running.lock().await;
        running.1.abort();
        running.0 += 1;
        let generation = running.0;
        let _ = self.input.send(Input::Switch(generation, context.to_owned()));
        let show = show_context(
            generation,
            context.to_owned(),
            self.namespaces.clone(),
            self.input.clone(),
            self.anonymiser.clone(),
        );
        running.1 = tokio::spawn(show);
    }
}

/// Connects to `context` and feeds its pods to the pump; a failure ends up
/// in the status line.
async fn show_context(
    generation: u64,
    context: String,
    namespaces: Vec<String>,
    input: UnboundedSender<Input>,
    anonymiser: Option<anonymise::Shared>,
) {
    match cluster::connect(Some(&context), &namespaces).await {
        Err(error) => {
            let message: String = format!("{error:#}").chars().take(STATUS_ERROR_MAX).collect();
            let text = format!("{context} · {message}");
            let text = match &anonymiser {
                Some(anonymiser) => anonymiser.lock().expect("anonymiser lock").status(&text),
                None => text,
            };
            let _ = input.send(Input::Status(generation, text));
        }
        Ok((client, context)) => {
            let (events_tx, events) = mpsc::unbounded_channel();
            let (status_tx, status) = watch::channel(format!("{context} · connecting"));
            let (events, status) = match anonymiser {
                Some(anonymiser) => anonymise::stage(anonymiser, events, status),
                None => (events, status),
            };
            let watching = cluster::watch(client, context, namespaces, events_tx, status_tx);
            tokio::join!(watching, forward(generation, events, status, input));
        }
    }
}

/// Keeps the city the pages see and passes every change on to them.
async fn pump(mut inputs: UnboundedReceiver<Input>, status: watch::Sender<String>, shared: Shared) {
    let mut current = 0;
    let mut next = None;
    loop {
        let input = match next.take() {
            Some(input) => input,
            None => match inputs.recv().await {
                Some(input) => input,
                None => return,
            },
        };
        match input {
            Input::Event(generation, first) => {
                let mut batch = Vec::new();
                if generation == current {
                    batch.push(first);
                }
                // Gather what is already waiting, up to the next non-event.
                while batch.len() < BATCH {
                    match inputs.try_recv() {
                        Ok(Input::Event(generation, event)) => {
                            if generation == current {
                                batch.push(event);
                            }
                        }
                        Ok(other) => {
                            next = Some(other);
                            break;
                        }
                        Err(_) => break,
                    }
                }
                if !batch.is_empty() {
                    apply(&shared, &batch).await;
                }
            }
            Input::Status(generation, text) => {
                if generation == current {
                    status.send_replace(text);
                }
            }
            Input::Switch(generation, context) => {
                current = generation;
                let mut city = shared.city.lock().await;
                city.pods.clear();
                city.context = Some(context.clone());
                let text = shared.anonymised_status(&format!("{context} · connecting"));
                status.send_replace(text.clone());
                let reset = json!([{ "Reset": null }, contexts(&city, &shared), { "Status": text }]).to_string();
                let _ = shared.batches.send(reset.into());
            }
        }
    }
}

async fn apply(shared: &Shared, batch: &[WorldEvent]) {
    let mut city = shared.city.lock().await;
    for event in batch {
        match event {
            WorldEvent::PodUpserted(pod) => {
                city.pods.insert(pod.uid.clone(), pod.clone());
            }
            WorldEvent::PodDeleted { uid } => {
                city.pods.remove(uid);
            }
        }
    }
    // Sent while holding the lock: a page subscribing under the same lock
    // sees each change either in its snapshot or as a batch.
    let text = serde_json::to_string(batch).expect("world events always serialize");
    let _ = shared.batches.send(text.into());
}

fn contexts(city: &City, shared: &Shared) -> Value {
    match &shared.anonymiser {
        None => json!({ "Contexts": { "current": city.context, "all": *shared.contexts } }),
        Some(anonymiser) => {
            let mut anonymiser = anonymiser.lock().expect("anonymiser lock");
            let current = city.context.as_deref().map(|context| anonymiser.context(context));
            let all: Vec<String> = shared.contexts.iter().map(|context| anonymiser.context(context)).collect();
            json!({ "Contexts": { "current": current, "all": all } })
        }
    }
}

impl Shared {
    fn anonymised_status(&self, text: &str) -> String {
        match &self.anonymiser {
            Some(anonymiser) => anonymiser.lock().expect("anonymiser lock").status(text),
            None => text.to_owned(),
        }
    }

    /// The real name of a context a page asks for.
    fn requested_context(&self, asked: &str) -> Option<String> {
        match &self.anonymiser {
            Some(anonymiser) => anonymiser.lock().expect("anonymiser lock").real_context(asked),
            None => Some(asked.to_owned()),
        }
    }
}

async fn index(State(shared): State<Shared>) -> Html<String> {
    Html(shared.page.to_string())
}

async fn sprite(Path(file): Path<String>) -> Response {
    let name = file.strip_suffix(".png").unwrap_or(&file);
    match SPRITES.iter().find(|(sprite, _)| *sprite == name) {
        Some((_, bytes)) => {
            ([(header::CONTENT_TYPE, "image/png"), (header::CACHE_CONTROL, "max-age=86400")], *bytes).into_response()
        }
        None => StatusCode::NOT_FOUND.into_response(),
    }
}

async fn pixi() -> Response {
    ([(header::CONTENT_TYPE, "text/javascript"), (header::CACHE_CONTROL, "max-age=86400")], PIXI).into_response()
}

async fn font(Path(file): Path<String>) -> Response {
    match FONTS.iter().find(|(font, _)| *font == file) {
        Some((_, bytes)) => {
            ([(header::CONTENT_TYPE, "font/woff2"), (header::CACHE_CONTROL, "max-age=86400")], *bytes).into_response()
        }
        None => StatusCode::NOT_FOUND.into_response(),
    }
}

async fn socket(upgrade: WebSocketUpgrade, headers: HeaderMap, State(shared): State<Shared>) -> Response {
    if !from_own_page(&headers, shared.port) {
        return StatusCode::FORBIDDEN.into_response();
    }
    upgrade.on_upgrade(|socket| feed(socket, shared))
}

/// Browsers let any website open a WebSocket to 127.0.0.1, so only the
/// city's own page may connect: the host must be this server (which also
/// stops DNS rebinding) and the origin, when a browser sends one, the same.
fn from_own_page(headers: &HeaderMap, port: u16) -> bool {
    let value = |name| headers.get(name).and_then(|value: &header::HeaderValue| value.to_str().ok());
    let Some(host) = value(header::HOST) else { return false };
    let local = [format!("127.0.0.1:{port}"), format!("localhost:{port}"), format!("[::1]:{port}")];
    if !local.iter().any(|allowed| allowed == host) {
        return false;
    }
    value(header::ORIGIN).is_none_or(|origin| origin == format!("http://{host}"))
}

/// Sends one page the whole city, then every change, until it goes away.
async fn feed(mut socket: WebSocket, shared: Shared) {
    let mut status = shared.status.clone();
    loop {
        let (snapshot, mut batches) = {
            let city = shared.city.lock().await;
            (snapshot(&city, &shared, &status.borrow_and_update()), shared.batches.subscribe())
        };
        if socket.send(Message::Text(snapshot.into())).await.is_err() {
            return;
        }
        loop {
            tokio::select! {
                batch = batches.recv() => match batch {
                    Ok(text) => {
                        if socket.send(Message::Text(text.as_ref().into())).await.is_err() {
                            return;
                        }
                    }
                    Err(broadcast::error::RecvError::Lagged(_)) => break,
                    Err(broadcast::error::RecvError::Closed) => return,
                },
                changed = status.changed() => {
                    if changed.is_err() {
                        return;
                    }
                    let text = json!([{ "Status": *status.borrow_and_update() }]).to_string();
                    if socket.send(Message::Text(text.into())).await.is_err() {
                        return;
                    }
                }
                incoming = socket.recv() => match incoming {
                    Some(Ok(Message::Text(text))) => {
                        if let Some(switcher) = &shared.switcher
                            && let Ok(request) = serde_json::from_str::<Value>(&text)
                            && let Some(asked) = request.get("Context").and_then(Value::as_str)
                            && let Some(context) = shared.requested_context(asked)
                        {
                            switcher.switch(&context).await;
                        }
                    }
                    Some(Ok(Message::Close(_)) | Err(_)) | None => return,
                    Some(Ok(_)) => {}
                },
            }
        }
    }
}

fn snapshot(city: &City, shared: &Shared, status: &str) -> String {
    let mut items = vec![json!({ "Reset": null })];
    if city.context.is_some() {
        items.push(contexts(city, shared));
    }
    items.push(json!({ "Status": status }));
    items.extend(city.pods.values().map(|pod| json!({ "PodUpserted": pod })));
    Value::Array(items).to_string()
}

/// Best effort: under WSL the Windows browser, elsewhere the desktop's.
fn open_browser(url: &str) {
    let commands: &[&str] = if std::env::var_os("WSL_DISTRO_NAME").is_some() {
        &["wslview", "explorer.exe"]
    } else if cfg!(target_os = "macos") {
        &["open"]
    } else {
        &["xdg-open"]
    };
    let opened = commands.iter().any(|command| {
        Command::new(command).arg(url).stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null()).spawn().is_ok()
    });
    if !opened {
        eprintln!("kube-city: open {url} in a browser");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::world::{PodState, WorkloadKind};

    fn pod(uid: &str, state: PodState) -> PodInfo {
        PodInfo {
            uid: uid.into(),
            namespace: "shop".into(),
            workload: "cart".into(),
            kind: WorkloadKind::Deployment,
            state,
            details: Default::default(),
        }
    }

    fn shared(contexts: &[&str]) -> (Shared, watch::Sender<String>) {
        let (status_tx, status) = watch::channel(String::new());
        let shared = Shared {
            city: Arc::default(),
            batches: broadcast::channel(BACKLOG).0,
            status,
            page: "".into(),
            port: 7700,
            contexts: Arc::new(contexts.iter().map(|c| c.to_string()).collect()),
            switcher: None,
            anonymiser: None,
        };
        (shared, status_tx)
    }

    async fn next_items(pages: &mut broadcast::Receiver<Arc<str>>) -> Vec<Value> {
        serde_json::from_str(&pages.recv().await.unwrap()).unwrap()
    }

    #[test]
    fn page_has_live_data_and_every_sprite_and_font_it_names() {
        let page = page();
        assert!(!page.contains("/*CITY_DATA*/") && !page.contains("/*KENNEY_SPRITES*/"));
        assert!(page.contains(r#""live":true"#));
        assert!(page.contains(&format!(r#""version":"{}""#, env!("CARGO_PKG_VERSION"))));
        // The built-in PixiJS must be the version the template asks for.
        assert!(TEMPLATE.contains(PIXI_CDN) && PIXI.contains(" * pixi.js - v7.4.2"));
        assert!(page.contains(r#"<script src="pixi.min.js">"#) && !page.contains("cdnjs"));
        // Nothing comes from Google; every font the page names is built in, and every built-in font is used.
        assert!(TEMPLATE.contains(GOOGLE_FONTS) && !page.contains("googleapis") && !page.contains("gstatic"));
        let fonts: Vec<&str> = page.split("url(fonts/").skip(1).map(|s| &s[..s.find(')').unwrap()]).collect();
        assert_eq!(fonts.len(), FONTS.len());
        for name in fonts {
            assert!(FONTS.iter().any(|(font, _)| *font == name), "font {name} is not embedded");
        }
        let named: Vec<&str> = TEMPLATE
            .split('"')
            .filter(|s| s.len() == 4 && s[1..].bytes().all(|b| b.is_ascii_digit()) && "bcd".contains(&s[..1]))
            .collect();
        assert!(named.len() > 100);
        for name in named {
            assert!(SPRITES.iter().any(|(sprite, _)| *sprite == name), "sprite {name} is not embedded");
        }
    }

    #[tokio::test]
    async fn pump_keeps_current_pods_and_forwards_batches() {
        let (shared, status_tx) = shared(&[]);
        let mut pages = shared.batches.subscribe();
        let (input, inputs) = mpsc::unbounded_channel();
        tokio::spawn(pump(inputs, status_tx, shared.clone()));

        input.send(Input::Status(0, "ctx · live".into())).unwrap();
        input.send(Input::Event(0, WorldEvent::PodUpserted(pod("a", PodState::Pending)))).unwrap();
        input.send(Input::Event(0, WorldEvent::PodUpserted(pod("b", PodState::Running)))).unwrap();
        input.send(Input::Event(0, WorldEvent::PodDeleted { uid: "a".into() })).unwrap();
        let mut seen = Vec::new();
        while seen.len() < 3 {
            seen.extend(next_items(&mut pages).await);
        }
        assert_eq!(seen[2], json!({ "PodDeleted": { "uid": "a" } }));

        let city = shared.city.lock().await;
        assert_eq!(city.pods.keys().collect::<Vec<_>>(), ["b"]);
        let snapshot: Value = serde_json::from_str(&snapshot(&city, &shared, "ctx · live")).unwrap();
        assert_eq!(snapshot[0], json!({ "Reset": null }));
        assert_eq!(snapshot[1], json!({ "Status": "ctx · live" }));
        assert_eq!(snapshot[2]["PodUpserted"]["uid"], "b");
    }

    #[tokio::test]
    async fn switching_resets_the_city_and_drops_the_old_contexts_events() {
        let (shared, status_tx) = shared(&["one", "two"]);
        let mut status = shared.status.clone();
        let mut pages = shared.batches.subscribe();
        let (input, inputs) = mpsc::unbounded_channel();
        tokio::spawn(pump(inputs, status_tx, shared.clone()));

        input.send(Input::Event(0, WorldEvent::PodUpserted(pod("old", PodState::Running)))).unwrap();
        input.send(Input::Switch(1, "two".into())).unwrap();
        // Late events from the first context are ignored.
        input.send(Input::Event(0, WorldEvent::PodUpserted(pod("late", PodState::Running)))).unwrap();
        input.send(Input::Status(0, "one · live".into())).unwrap();
        input.send(Input::Event(1, WorldEvent::PodUpserted(pod("new", PodState::Running)))).unwrap();

        assert_eq!(next_items(&mut pages).await[0]["PodUpserted"]["uid"], "old");
        let reset = next_items(&mut pages).await;
        assert_eq!(reset[0], json!({ "Reset": null }));
        assert_eq!(reset[1], json!({ "Contexts": { "current": "two", "all": ["one", "two"] } }));
        assert_eq!(reset[2], json!({ "Status": "two · connecting" }));
        let after = next_items(&mut pages).await;
        assert_eq!(after, [json!({ "PodUpserted": pod("new", PodState::Running) })]);

        let city = shared.city.lock().await;
        assert_eq!(city.pods.keys().collect::<Vec<_>>(), ["new"]);
        assert_eq!(*status.borrow_and_update(), "two · connecting");
    }

    #[test]
    fn only_the_citys_own_page_may_open_the_socket() {
        let headers = |pairs: &[(header::HeaderName, &str)]| {
            let mut map = HeaderMap::new();
            for (name, value) in pairs {
                map.insert(name.clone(), value.parse().unwrap());
            }
            map
        };
        let own = headers(&[(header::HOST, "127.0.0.1:7700"), (header::ORIGIN, "http://127.0.0.1:7700")]);
        assert!(from_own_page(&own, 7700));
        assert!(from_own_page(&headers(&[(header::HOST, "localhost:7700")]), 7700));
        let other_site = headers(&[(header::HOST, "127.0.0.1:7700"), (header::ORIGIN, "https://evil.example")]);
        assert!(!from_own_page(&other_site, 7700));
        let rebound = headers(&[(header::HOST, "evil.example:7700"), (header::ORIGIN, "http://evil.example:7700")]);
        assert!(!from_own_page(&rebound, 7700));
        assert!(!from_own_page(&HeaderMap::new(), 7700));
    }
}
