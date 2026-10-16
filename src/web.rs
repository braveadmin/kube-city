//! `--web`: the city in a browser.
//!
//! Serves the page and the Kenney sprites from the binary on 127.0.0.1 and
//! streams world events to every open page over a WebSocket, as JSON arrays
//! of the same events a recording holds. A page that connects late first
//! gets `{"Reset": null}`, the status and every current pod; one that falls
//! too far behind gets that again.

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
    http::{StatusCode, header},
    response::{Html, IntoResponse, Response},
    routing::get,
};
use serde_json::{Value, json};
use tokio::sync::{Mutex, broadcast, mpsc::UnboundedReceiver, watch};

use crate::world::{PodInfo, WorldEvent};

const TEMPLATE: &str = include_str!("../art/city.template.html");
include!(concat!(env!("OUT_DIR"), "/sprites.rs"));

/// Batches a page may fall behind before it is sent the whole city again.
const BACKLOG: usize = 1024;
/// Events sent to the pages in one message at most.
const BATCH: usize = 2000;

#[derive(Clone)]
struct Shared {
    pods: Arc<Mutex<HashMap<String, PodInfo>>>,
    batches: broadcast::Sender<Arc<str>>,
    status: watch::Receiver<String>,
    page: Arc<str>,
}

/// Serves the city until Ctrl+C, opening it in a browser when `open` is set.
pub async fn serve(events: UnboundedReceiver<WorldEvent>, status: watch::Receiver<String>, port: u16, open: bool) -> Result<()> {
    let shared = Shared { pods: Arc::default(), batches: broadcast::channel(BACKLOG).0, status, page: page().into() };
    tokio::spawn(pump(events, shared.clone()));
    let app = Router::new()
        .route("/", get(index))
        .route("/ws", get(socket))
        .route("/sprites/{file}", get(sprite))
        .with_state(shared);
    let listener = tokio::net::TcpListener::bind((Ipv4Addr::LOCALHOST, port))
        .await
        .with_context(|| format!("cannot listen on 127.0.0.1:{port}; is another kube-city running? Try --port"))?;
    let url = format!("http://127.0.0.1:{}/", listener.local_addr()?.port());
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

/// The page with an empty live city and the sprites served next to it.
fn page() -> String {
    let data = json!({ "ns": [], "wl": [], "events": [], "live": true });
    let sprites: serde_json::Map<String, Value> =
        SPRITES.iter().map(|(name, _)| (name.to_string(), Value::from(format!("sprites/{name}.png")))).collect();
    let body = TEMPLATE
        .replacen("/*CITY_DATA*/null", &data.to_string(), 1)
        .replacen("/*KENNEY_SPRITES*/null", &Value::Object(sprites).to_string(), 1);
    format!(
        "<!doctype html>\n<html lang=\"en\">\n<meta charset=\"utf-8\">\n\
         <meta name=\"viewport\" content=\"width=device-width, initial-scale=1, viewport-fit=cover\">\n{body}\n</html>\n"
    )
}

/// Keeps the current pods and passes every event on to the open pages.
async fn pump(mut events: UnboundedReceiver<WorldEvent>, shared: Shared) {
    while let Some(first) = events.recv().await {
        let mut batch = vec![first];
        while batch.len() < BATCH {
            let Ok(event) = events.try_recv() else { break };
            batch.push(event);
        }
        let mut pods = shared.pods.lock().await;
        for event in &batch {
            match event {
                WorldEvent::PodUpserted(pod) => {
                    pods.insert(pod.uid.clone(), pod.clone());
                }
                WorldEvent::PodDeleted { uid } => {
                    pods.remove(uid);
                }
            }
        }
        // Sent while holding the lock: a page subscribing under the same
        // lock sees each change either in its snapshot or as a batch.
        let text = serde_json::to_string(&batch).expect("world events always serialize");
        let _ = shared.batches.send(text.into());
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

async fn socket(upgrade: WebSocketUpgrade, State(shared): State<Shared>) -> Response {
    upgrade.on_upgrade(|socket| feed(socket, shared))
}

/// Sends one page the whole city, then every change, until it goes away.
async fn feed(mut socket: WebSocket, shared: Shared) {
    let mut status = shared.status.clone();
    let mut status_open = true;
    loop {
        let (snapshot, mut batches) = {
            let pods = shared.pods.lock().await;
            (snapshot(&pods, &status.borrow_and_update()), shared.batches.subscribe())
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
                changed = status.changed(), if status_open => {
                    if changed.is_err() {
                        // A replay or the demo never changes its status.
                        status_open = false;
                        continue;
                    }
                    let text = json!([{ "Status": *status.borrow_and_update() }]).to_string();
                    if socket.send(Message::Text(text.into())).await.is_err() {
                        return;
                    }
                }
                incoming = socket.recv() => match incoming {
                    Some(Ok(Message::Close(_)) | Err(_)) | None => return,
                    Some(Ok(_)) => {}
                },
            }
        }
    }
}

fn snapshot(pods: &HashMap<String, PodInfo>, status: &str) -> String {
    let mut items = vec![json!({ "Reset": null }), json!({ "Status": status })];
    items.extend(pods.values().map(|pod| json!({ "PodUpserted": pod })));
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
        }
    }

    #[test]
    fn page_has_live_data_and_every_sprite_it_names() {
        let page = page();
        assert!(!page.contains("/*CITY_DATA*/") && !page.contains("/*KENNEY_SPRITES*/"));
        assert!(page.contains(r#""live":true"#));
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
        let (status_tx, status) = watch::channel(String::from("test"));
        let shared = Shared { pods: Arc::default(), batches: broadcast::channel(BACKLOG).0, status, page: "".into() };
        let mut pages = shared.batches.subscribe();
        let (events, receiver) = tokio::sync::mpsc::unbounded_channel();
        tokio::spawn(pump(receiver, shared.clone()));

        events.send(WorldEvent::PodUpserted(pod("a", PodState::Pending))).unwrap();
        events.send(WorldEvent::PodUpserted(pod("b", PodState::Running))).unwrap();
        events.send(WorldEvent::PodDeleted { uid: "a".into() }).unwrap();
        let mut seen = Vec::new();
        while seen.len() < 3 {
            let batch: Vec<WorldEvent> = serde_json::from_str(&pages.recv().await.unwrap()).unwrap();
            seen.extend(batch);
        }
        assert_eq!(seen[2], WorldEvent::PodDeleted { uid: "a".into() });

        let pods = shared.pods.lock().await;
        assert_eq!(pods.keys().collect::<Vec<_>>(), ["b"]);
        let snapshot: Value = serde_json::from_str(&snapshot(&pods, "ctx · live")).unwrap();
        assert_eq!(snapshot[0], json!({ "Reset": null }));
        assert_eq!(snapshot[1], json!({ "Status": "ctx · live" }));
        assert_eq!(snapshot[2]["PodUpserted"]["uid"], "b");
        drop(status_tx);
    }
}
