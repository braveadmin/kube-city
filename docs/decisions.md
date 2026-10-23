# kube-city: decisions and state

What was decided while building kube-city, and why. Read this before
changing direction; update it when a decision changes.

Last updated: 2026-10-16.

## The idea

Shows a Kubernetes cluster as a living isometric city, in the browser (the
terminal view it started as is deprecated, see below). Kubernetes objects are buildings; they appear, change and disappear as
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
    with a storey per replica, StatefulSets brick halls with a code-drawn
    tank, twin tanks or water tower on the roof, DaemonSets houses, Jobs
    works with a code-drawn chimney. Fire, smoke and
    the crane stay code-drawn on top. The ground is Kenney tiles: asphalt,
    pavement, planters, lamps and trees. Sprites go into one atlas at load
    time (with mipmaps); ground floors lose their earth sides there so the
    lot tile underneath shows them.
  - Each look is an object with `body(look)` and `ground()` in the page; the
    rest of the page does not know which look is active.
  - StatefulSet halls and DaemonSet houses are both brick, so they were easy
    to confuse from far away and the packs have no tanks. The owner chose
    (2026-10-23, #14) code-drawn tanks on StatefulSet roofs, keeping the
    "storage" idea from Metropolis; pale tanks read at district zoom.
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

## Distribution (2026-10-22)

| Topic | Decision | Why |
|---|---|---|
| Hosting | Public GitHub repo `braveadmin/kube-city` | The owner wants people to use it without compiling |
| Identity | Commits by `braveadmin` with an empty email; history rewritten once to remove the work email, cluster names and employer-specific demo names | Keep the owner's employer and private details out of a public repo |
| License | MIT OR Apache-2.0 | The usual Rust dual license |
| Binaries | cargo-dist on version tags: Linux x86_64 (static musl) and arm64, macOS arm64 and x86_64, Windows x86_64, plus shell and PowerShell installers; pull requests build the same without publishing | One command to install; musl runs on any distro |
| Signing | None; README explains the macOS quarantine command | Apple signing needs a paid account |
| PixiJS | Built into the binary (`assets/pixi`), served at `/pixi.min.js` | `--web` must work without a CDN; fonts still come from Google Fonts and fall back to system fonts |
| Line endings | `.gitattributes`: LF everywhere, PNG binary | Windows checkouts embedded CRLF copies of the page |
| Package managers (0.1.1) | Homebrew tap `braveadmin/homebrew-tap` (formula pushed by the release workflow), crates.io `kube-city`; krew planned | One-command installs for macOS, Linux and Rust users; krew reaches kubectl users |
| Archives | `.tar.gz` on Linux and macOS (was `.tar.xz`) | krew accepts only `.tar.gz` and `.zip` |
| Dependabot vs dist | `allow-dirty = ["ci"]` | dist rejected Dependabot's action bumps in the generated release workflow |

## Page controls (2026-10-22)

| Topic | Decision | Why |
|---|---|---|
| Theme | A "Theme" dropdown (native `<select>`) instead of the Kenney/Metropolis buttons | Room for more themes; keyboard and screen readers work |
| Details panel | Collapses into its header on every screen size (`i`), remembered per browser; a fitted view refits to the space it leaves | More room for the city |
| Context | A "Context" dropdown when `--web` shows a cluster (not with `--record`, `--replay` or `--demo`); the server switches its watcher | Look at another cluster without restarting |
| Switching | One ordered input stream into the pump, every source tagged with a generation; a switch clears the city, sends Reset + Contexts to every page, and drops late events of the old context; connect and watch run in one task that a newer switch aborts | No mix of two clusters, even while the old watcher winds down |
| Socket origin | `/ws` only accepts the city's own page: Host must be 127.0.0.1/localhost/[::1] on the server's port, and Origin (if sent) the same | Any website can open a WebSocket to localhost; switching made that worse than reading |

## Terminal view frozen (2026-10-23)

Owner's decision: **freeze now, remove later.** The browser is the default
(`kube-city` = the old `--web`, which still works); the terminal view moved
behind `--tui`, prints a deprecation line, gets bug fixes only, and removal
is decided around 0.3 after feedback (or it moves behind an optional cargo
feature).

Why: the terminal view was ~2,240 of 3,851 lines in `src/` (58 %: `app.rs`,
`layout.rs`, `render/**` with a second Metropolis theme, most of
`world.rs`), 38 of 56 tests, and `ratatui` with ~35 extra crates on Linux.
The web path only uses the shared event types. Every feature (pod details,
`--anonymise`, contexts) would otherwise be designed twice, and the terminal
already lacked the inspector, contexts, Kenney and search. Costs accepted:
over SSH the browser needs `ssh -L 7700:127.0.0.1:7700`; no fallback without
WebGL; plain `kube-city` now opens a browser.

Flags: `--fps` and `--all-pods` only work with `--tui` (checked in `main.rs`,
because clap's `requires` counts `--tui`'s default as given); `--port`,
`--no-open` and `--web` conflict with `--tui`.

## Roadmap

The public roadmap is [`ROADMAP.md`](../ROADMAP.md); keep it up to date when
a step starts or ends. Notes that do not belong there:

- Homebrew and crates.io shipped with 0.1.1. The release needs the
  `HOMEBREW_TAP_TOKEN` secret (set); without it a release is not published.
- krew is last on purpose (owner, 2026-10-22): it is optional, and the
  central index needs the CNCF CLA (commits with a GitHub noreply email, not
  the empty one), a review, and a name: `kube-` is not allowed, `city` may
  be called too generic, `pod-city` is the safe choice. A self-hosted index
  (`braveadmin/krew-index`) needs none of that. Archives are already
  `.tar.gz`, which krew requires.
- All contexts at the same time needs a watcher per context and
  context-aware layout and labels.

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
