# kube-city: decisions and state

What was decided while building kube-city, and why. Read this before
changing direction; update it when a decision changes.

Last updated: 2026-10-16.

## The idea

A terminal companion that shows a Kubernetes cluster as a living isometric
city. Kubernetes objects are buildings; they appear, change and disappear as
the cluster changes. Nothing similar existed (closest: KubeInvaders,
KubeDoom, KubeCraftAdmin, kube-universe; none is an ambient city).

## Where things are

- Repo: `~/repos/.offtopic/kube-city`. Local only, no remote yet.
- `master` (`67852cc`): Metropolis theme, live cluster watching, demo mode.
- `eval/rendering` (`03e4961`, not merged): recording/replay, the readable
  city for big clusters (below), evaluation page source. **Waiting for the
  owner's OK to merge into `master`.**
- `recordings/big-cluster.jsonl`: a 65 s recording of a big real
  cluster (890 pods, 27 namespaces). Git-ignored: it holds real namespace
  and workload names. Anonymise before sharing anything made from it.
- Evaluation page (private): https://claude.ai/artifact/5LBNjXKHiRMQLLoPJGhDCT
- Style samples page (private): https://claude.ai/artifact/HQhiDTRhhnCZ4VitLRu1fb

## Decisions

| Topic | Decision | Why |
|---|---|---|
| Language | Rust: ratatui, kube-rs, tokio, clap | Smooth cell-diffing renderer, low idle CPU, single binary |
| Local test cluster | None (no kind) | kind clashes with the owner's one-file-per-context kubeconfig; `--demo` and `--replay` replace it |
| Cluster access | Read-only list/watch through the merged `KUBECONFIG`, `--context`, `-n` | Never write to clusters or kubeconfig files |
| Art style | Metropolis (daylight pixel art, SimCity 2000-like) | Chosen from three samples (Metropolis, Neon night, Toy town) |
| Themes | Everything visual behind the `Theme` trait (`src/render/theme/`) | The owner wants selectable themes later; Neon and Toy town samples are in `art/samples.py` |
| Buildings | Deployment: tower; StatefulSet: tank, twin tanks or silo; DaemonSet: house; Job: factory; no owner: kiosk | Kind of workload is recognisable at a glance |
| States | Pending: construction site with crane; CrashLoop (incl. ImagePullBackOff and similar): fire; Terminating: faded and crumbling; Failed: scorched pavilion | Problems must stand out |
| Landmarks | A workload's look depends on its name only, not its namespace | The same component deployed in many namespaces looks the same everywhere |
| Noise | Default: one building per DaemonSet, finished jobs only counted; `--all-pods` turns this off | 153 DaemonSet pods were 9 DaemonSets; 111 finished jobs were clutter |
| Zoom | `z` cycles detail, overview, map (state-coloured tiles); `f` fades healthy buildings | Overview and focus for finding problems |
| Recording | `--record FILE` / `--replay FILE` (JSON lines of world events) | Reproducible tests on real clusters without access |
| Sixel | Ruled out and removed | Sixel images do not get from WSL to Windows Terminal 1.24: even a test bar printed from the shell does not appear (ConPTY) |
| Pre-rendered sprites | Not in the terminal | Only pay off with more pixels than half blocks give |

## Open problems (owner's feedback, 2026-10-16)

1. **The owner does not like the half-block style**, even after the visual
   improvements.
2. **The furthest zoom does not show a big cluster properly.** The map zoom
   only shrinks buildings; 780 pods still fragment into tiny tiles.

## Recommended next step: browser prototype

Not started; the owner asked about cost first.

- `kube-city --web` would serve a local page (open in the Windows browser)
  fed by the existing event stream as JSON over a WebSocket. The Rust side
  stays: watchers, recording/replay, layout, rules.
- Rendering with PixiJS (WebGL): real isometric sprites (own art or CC0
  packs such as Kenney's), smooth mouse-wheel zoom, hover for pod details,
  click to select, focus mode.
- **Semantic zoom**, needed whatever the renderer:
  - cluster: each namespace one labelled district, sized by pod count,
    coloured by health
  - district: a namespace's workloads
  - street: individual pods
- First step: a self-contained prototype page from the anonymised big-cluster
  recording, to judge the direction before building the `--web` server.
- Half blocks stay as a quick terminal view and an SSH fallback.

Alternatives considered: a native window (Bevy or macroquad; more work for
text and panels), another terminal such as WezTerm (unverified under WSL;
Sixel would still be about 6 fps at 1 MB per frame), text-mode art (`▓▒░`,
box lines; a different style, not sharper).

## Measurements (890-pod recording, release build)

| What | Result |
|---|---|
| Half blocks, new defaults, 200×55 terminal, 20 fps | about 9% of one CPU core |
| Live on `my-cluster` (158 pods) | about 5% of one core, 30 MB memory |
| Sixel frame drawn at 3× and enlarged | draw 35–43 ms, encode 117–141 ms, 1.1–1.4 MB per frame |
| Sixel frame at full resolution, code-drawn art | draw 0.4–1.4 s, encode 0.1–0.26 s |

## Environment notes

- Windows Terminal 1.24 with WSL; true colour works, Sixel does not.
- `KUBECONFIG` lists `~/.kube/config` plus one file per context in
  `~/.kube/contexts`; kube-rs merges them correctly.
- Working read-only right now: `my-cluster`, `dev-cluster`, `test-cluster`.
- Azure contexts (`cluster-a`,
  `cluster-b`) use a device-code kubelogin that hangs in
  non-interactive shells. Workaround: a kubeconfig copy converted with
  `kubelogin convert-kubeconfig -l azurecli` plus `az login` (tokens last
  12 hours).
- `old-cluster` no longer resolves (stale context).
