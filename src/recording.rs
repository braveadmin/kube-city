//! Recording a feed of world events to a file, and replaying it later.
//!
//! A recording holds one JSON object per line: when the event happened, in
//! milliseconds since the recording started, and the event itself. Replays
//! make a real cluster's city reproducible without access to the cluster.

use std::{
    fs,
    path::Path,
    time::{Duration, Instant},
};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use tokio::{
    fs::File,
    io::{AsyncWriteExt, BufWriter},
    sync::mpsc::{UnboundedReceiver, UnboundedSender},
};

use crate::{roles::role_of, world::WorldEvent};

#[derive(Debug, PartialEq, Serialize, Deserialize)]
struct Entry {
    at_ms: u64,
    event: WorldEvent,
}

/// Forwards `events` to `out`, writing each one to `file` on the way. If
/// writing fails, recording stops but the city keeps getting its events.
pub async fn tee(mut events: UnboundedReceiver<WorldEvent>, out: UnboundedSender<WorldEvent>, file: File) {
    let started = Instant::now();
    let mut writer = Some(BufWriter::new(file));
    while let Some(event) = events.recv().await {
        if let Some(file) = &mut writer {
            let entry = Entry { at_ms: started.elapsed().as_millis() as u64, event: event.clone() };
            if write_entry(file, &entry).await.is_err() {
                writer = None;
            }
        }
        if out.send(event).is_err() {
            break;
        }
    }
    if let Some(mut file) = writer {
        // Nothing left to report a failure to at this point.
        let _ = file.flush().await;
    }
}

async fn write_entry(file: &mut BufWriter<File>, entry: &Entry) -> Result<()> {
    let mut line = serde_json::to_vec(entry)?;
    line.push(b'\n');
    file.write_all(&line).await?;
    file.flush().await?;
    Ok(())
}

/// Reads a whole recording, failing on the first line that isn't one.
pub fn load(path: &Path) -> Result<Vec<(Duration, WorldEvent)>> {
    let text = fs::read_to_string(path).with_context(|| format!("cannot read {}", path.display()))?;
    text.lines()
        .enumerate()
        .filter(|(_, line)| !line.trim().is_empty())
        .map(|(i, line)| {
            let mut entry: Entry = serde_json::from_str(line)
                .with_context(|| format!("{}, line {}: not a kube-city recording", path.display(), i + 1))?;
            // Recordings made before roles were sent (#48).
            if let WorldEvent::PodUpserted(pod) = &mut entry.event
                && pod.role.is_none()
            {
                pod.role = Some(role_of(pod.kind, &pod.details.images, &pod.workload));
            }
            Ok((Duration::from_millis(entry.at_ms), entry.event))
        })
        .collect()
}

/// Plays `recording` into `events` at `speed` times the recorded pace.
pub async fn replay(recording: Vec<(Duration, WorldEvent)>, speed: f64, events: UnboundedSender<WorldEvent>) {
    let started = tokio::time::Instant::now();
    for (at, event) in recording {
        tokio::time::sleep_until(started + at.div_f64(speed)).await;
        if events.send(event).is_err() {
            return;
        }
    }
}

#[cfg(test)]
mod tests {
    use tokio::sync::mpsc::unbounded_channel;

    use super::*;
    use crate::{
        roles::Role,
        world::{
            PodState,
            tests::{delete, upsert},
        },
    };

    fn scratch_file(name: &str) -> std::path::PathBuf {
        std::env::temp_dir().join(format!("kube-city-{}-{name}.jsonl", std::process::id()))
    }

    #[tokio::test]
    async fn a_recording_replays_the_same_events_in_order() {
        let path = scratch_file("roundtrip");
        let events = vec![upsert("a", "shop", PodState::Pending), upsert("a", "shop", PodState::Running), delete("a")];

        let (source, recorded) = unbounded_channel();
        let (forwarded, mut city) = unbounded_channel();
        let file = File::create(&path).await.expect("create recording");
        let recorder = tokio::spawn(tee(recorded, forwarded, file));
        for event in &events {
            source.send(event.clone()).expect("send");
        }
        drop(source);
        recorder.await.expect("recorder finishes");
        let mut passed_through = Vec::new();
        while let Ok(event) = city.try_recv() {
            passed_through.push(event);
        }
        assert_eq!(passed_through, events, "the city still gets every event");

        let recording = load(&path).expect("load recording");
        assert!(recording.windows(2).all(|pair| pair[0].0 <= pair[1].0), "times never go back");
        let (replayed, mut city) = unbounded_channel();
        replay(recording, 1000.0, replayed).await;
        let mut played = Vec::new();
        while let Ok(event) = city.try_recv() {
            played.push(event);
        }
        assert_eq!(played, events);
        fs::remove_file(path).ok();
    }

    #[test]
    fn recordings_without_roles_get_them_on_load() {
        let path = scratch_file("roles");
        let old = r#"{"at_ms":5,"event":{"PodUpserted":{"uid":"u","namespace":"ci","workload":"argocd-redis","kind":"Deployment","state":"Running"}}}
{"at_ms":6,"event":{"PodUpserted":{"uid":"v","namespace":"ci","workload":"db","kind":"StatefulSet","state":"Running","details":{"images":["postgres:16"]}}}}
{"at_ms":7,"event":{"PodUpserted":{"uid":"w","namespace":"ci","workload":"nginx","kind":"Deployment","state":"Running","role":"other"}}}"#;
        fs::write(&path, old).expect("write recording");
        let roles: Vec<_> = load(&path)
            .expect("load recording")
            .into_iter()
            .map(|(_, event)| match event {
                WorldEvent::PodUpserted(pod) => pod.role,
                WorldEvent::PodDeleted { .. } | WorldEvent::Exposed(_) => None,
            })
            .collect();
        // From the name, from the images, and as recorded.
        assert_eq!(roles, [Some(Role::Data), Some(Role::Data), Some(Role::Other)]);
        fs::remove_file(path).ok();
    }

    #[test]
    fn a_broken_line_names_the_file_and_line() {
        let path = scratch_file("broken");
        fs::write(&path, "{\"at_ms\":0,\"event\":{\"PodDeleted\":{\"uid\":\"a\"}}}\n\nnot json\n").expect("write");
        let error = load(&path).expect_err("line 3 is broken").to_string();
        assert!(error.contains("line 3"), "{error}");
        fs::remove_file(path).ok();
    }
}
