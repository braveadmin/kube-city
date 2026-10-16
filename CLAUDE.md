# kube-city

A terminal companion that draws a Kubernetes cluster as a living isometric
city (Rust: ratatui, kube-rs, tokio). **Read `docs/decisions.md` first**: it
holds the decisions so far, the open problems and the recommended next step.

## Commands

```bash
cargo run --release -- --demo                                  # simulated cluster
cargo run --release -- --replay recordings/big-cluster.jsonl  # recorded real cluster (git-ignored)
cargo run --release -- --context my-cluster                 # live, read-only
cargo test
cargo clippy --all-targets   # keep it warning-free
```

Browser prototype from a recording (anonymised, writes `art/out/city.html`):

```bash
python3 art/web_prototype.py recordings/big-cluster.jsonl art/out
```

Kenney sprites (CC0) live in `assets/kenney/`; see its README to add more.

Snapshot images of a recording, for design work (writes PPM files):

```bash
KUBE_CITY_REPLAY=recordings/big-cluster.jsonl KUBE_CITY_OUT=/tmp/snap cargo test render_snapshots -- --ignored
```

## Layout

- `src/world.rs`: city model (pods, buildings, grouping rules) fed by `WorldEvent`s
- `src/layout.rs`: stable lots; blocks handed out along a spiral
- `src/cluster.rs`, `src/demo.rs`, `src/recording.rs`: event sources
- `src/render/`: `scene.rs` (isometric primitives), `theme/` (all art; Metropolis)
- `assets/kenney/`: the Kenney sprites the browser view uses (CC0)
- `art/`: style samples script, evaluation page sources, browser prototype
  (`web_prototype.py` + `city.template.html`)

## Conventions

- Cluster access is read-only (list/watch); never write to clusters or kubeconfig files.
- Recordings hold real namespace and workload names: keep them out of git and
  anonymise anything shared from them.
- Conventional Commits (`feat:`, `fix:`, `docs:` …), no Jira keys here.
- New looks go through the `Theme` trait, not into the renderer.
