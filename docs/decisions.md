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
- `master`: everything below, merged 2026-10-16 (fast-forward from
  `proto/web`, which contains `eval/rendering`). Both branches were left in
  place.
- `recordings/big-cluster.jsonl`: a 65 s recording of a big real
  cluster (890 pods, 27 namespaces). Git-ignored: it holds real namespace
  and workload names. Anonymise before sharing anything made from it.
- Evaluation page (private): https://claude.ai/artifact/5LBNjXKHiRMQLLoPJGhDCT
- Style samples page (private): https://claude.ai/artifact/HQhiDTRhhnCZ4VitLRu1fb
- Browser prototype (private): https://claude.ai/artifact/KmCkfNCaFEUc2yTUPvAbot

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
| Renderer | Browser (PixiJS/WebGL) is the main view; half blocks stay as terminal fallback | The owner liked the browser prototype ("I love the feeling it has now") |
| Look | Kenney sprites (CC0) as the default theme; Metropolis stays as a second theme | The owner, 2026-10-16: "I love kenney's assets!" |

## Open problems (owner's feedback, 2026-10-16)

1. **The owner does not like the half-block style**, even after the visual
   improvements.
2. **The furthest zoom does not show a big cluster properly.** The map zoom
   only shrinks buildings; 780 pods still fragment into tiny tiles.

## Browser prototype (built 2026-10-16; the owner chose it, with the Kenney look)

A self-contained page made from the big-cluster recording, to judge the
direction before building a `--web` server.

- Build: `python3 art/web_prototype.py recordings/big-cluster.jsonl art/out`
  writes `art/out/city.html` (git-ignored). The script replaces every
  namespace, workload and pod id with a consistent pseudonym and refuses to
  write the page if a real name would leak into it.
- Source: `art/city.template.html` (PixiJS 7.4.2 from cdnjs, WebGL).
- Art: the Metropolis look drawn as vector shapes (one shared
  `GraphicsGeometry` per look), so buildings stay sharp at every zoom.
- **One building per workload**, not per pod as in the terminal. Towers grow
  floors with replicas; a DaemonSet shows up to four houses, one per node.
  622 buildings instead of 780.
- Lots are planned from the whole recording: a namespace's workloads are
  sorted by kind and name, so the same component sits in the same spot in
  every branch namespace (landmarks by position as well as by look).
- Semantic zoom with three levels:
  - cluster: each namespace one district, tinted by health, a road-sign
    label with pod and problem counts, a state bar, pulsing markers on
    crashing workloads; click a district to fly to it
  - district: buildings, fire, smoke, cranes, cars
  - street: workload names with one dot per pod
- Interaction: wheel or pinch zoom to the cursor, drag to pan, hover
  tooltips, click to select (inspector with pods and "same component in N
  other namespaces"), search with rings on every match, focus mode, a
  "needs attention" list that flies to each problem, replay of the
  recording, toggles for finished jobs and simulated restarts and crashes.
- Deep links: `#district`, `#street` or `#<namespace>`.
- **Two looks** behind a switch in the header (Kenney by default, remembered
  per browser; `#metropolis.<deep link>` or `#kenney.<deep link>` picks one):
  - Metropolis: the vector art above.
  - Kenney: sprites from Kenney's *Isometric Buildings* and *Isometric City*
    packs (CC0). Buildings are stacked from a ground floor, upper floors
    (35 px each in sprite pixels) and a roof: Deployments are office blocks
    with a storey per replica, StatefulSets brick halls with barrel roofs,
    DaemonSets houses, Jobs works with a code-drawn chimney. Fire, smoke and
    the crane stay code-drawn on top. The ground is Kenney tiles: asphalt,
    pavement, planters, lamps and trees. Sprites go into one atlas at load
    time (with mipmaps); ground floors lose their earth sides there so the
    lot tile underneath shows them.
  - Each look is an object with `body(look)` and `ground()` in the page; the
    rest of the page does not know which look is active.
  - Known weak spot of the Kenney look: StatefulSet halls and DaemonSet
    houses are both brick with pitched or barrel roofs, so they are easy to
    confuse from far away; the packs have no tanks.
- The sprites the page uses are in `assets/kenney/` (CC0, with Kenney's
  licence files); `assets/kenney/README.md` says where the full packs are.
- `apply(event)` takes events in the recording's compact form, so a future
  `kube-city --web` can stream the same events over a WebSocket.
- Checked with headless Windows Chrome (SwiftShader WebGL): all three levels,
  search, selection and fly-to render without console errors. Not checked
  on a real phone.

## `kube-city --web` (built 2026-10-16)

- `src/web.rs` (axum 0.8): serves the page (`art/city.template.html`, built
  in with `include_str!`) and the Kenney sprites (embedded by `build.rs`
  from `assets/kenney`) on 127.0.0.1:7700, and streams events over `/ws`.
- Messages are JSON arrays of the recording's events
  (`{"PodUpserted": pod}`, `{"PodDeleted": {"uid"}}`) plus
  `{"Status": text}` and `{"Reset": null}`. A new or lagging page gets
  Reset, the status and every current pod; the server keeps the current pods
  for that. Events are batched (up to 2000 per message).
- The page plans lots incrementally: a workload keeps its first lot; a batch
  of new workloads is sorted by kind and name first (landmarks); live
  districts keep 30 % spare lots plus one, and the city re-packs only when a
  district overflows or a namespace appears. Districts are shelf-packed
  tallest first (the platform/branch split left large empty areas on small
  clusters).
- Live mode hides replay and simulated changes; the header shows the source
  status ("my-cluster · live"); the page reconnects with backoff.
- Opening the browser: `wslview` or `explorer.exe` under WSL, `open` on
  macOS, `xdg-open` elsewhere; `--no-open` to skip.
- Checked: Windows Chrome reaches the WSL server through localhost
  forwarding; live on `my-cluster` (158 pods) about 30 MB and 1 % of a
  core for the server; replay and demo work; a WebSocket probe saw the
  snapshot then 29 batches in 3 s of demo.
- Real names are shown: the server only listens on 127.0.0.1. An
  `--anonymise` flag (reusing `art/web_prototype.py`'s pseudonyms) is a
  possible addition for sharing screenshots.

## Next steps

1. Polish the Kenney look: tell StatefulSets and DaemonSets apart, Kenney
   road tiles with crossings, more variety.
2. Richer pod details in the inspector (restarts, age, node, image).
3. More objects: Services, Ingresses, Nodes.
4. Config file, remote repo, krew packaging.

Original plan for the browser prototype, kept for reference:

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
- Contexts that use a device-code kubelogin hang in non-interactive shells.
  Workaround: a kubeconfig copy converted with
  `kubelogin convert-kubeconfig -l azurecli` plus `az login` (tokens last
  12 hours).
- The owner's cluster names and which of them work: `docs/local-notes.md`
  (git-ignored; it also maps the generic names used here to the real ones).
