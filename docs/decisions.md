# kube-city: decisions and state

What was decided while building kube-city, and why. Read this before
changing direction; update it when a decision changes.

Last updated: 2026-10-27.

## The idea

Shows a Kubernetes cluster as a living isometric city, in the browser (the
terminal view it started as was removed in 0.4.0, see below). Kubernetes objects are buildings; they appear, change and disappear as
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
- Layout sketches (private): https://claude.ai/artifact/FvpTBm1czGseeufRkjbZx4
- Terrain seeds sketch (private): https://claude.ai/artifact/Xf8STkK2mYYScQoCPf2bv5

## Decisions

| Topic | Decision | Why |
|---|---|---|
| Language | Rust: kube-rs, tokio, axum, clap (ratatui until 0.4.0) | Low idle CPU, single binary |
| Local test cluster | None (no kind) | kind clashes with the owner's one-file-per-context kubeconfig; `--demo` and `--replay` replace it |
| Cluster access | Read-only list/watch through the merged `KUBECONFIG`, `--context`, `-n` | Never write to clusters or kubeconfig files |
| Art style | Metropolis (daylight pixel art, SimCity 2000-like) | Chosen from three samples (Metropolis, Neon night, Toy town) |
| Themes | Terminal view: everything visual behind the `Theme` trait (`src/render/theme/`, removed in 0.4.0); browser: themes in `art/city.template.html` | The owner wants selectable themes later; Neon and Toy town samples are in `art/samples.py` |
| Buildings | Deployment: tower; StatefulSet: tank, twin tanks or silo; DaemonSet: house; Job: factory; no owner: kiosk | Kind of workload is recognisable at a glance |
| States | Pending: construction site with crane; CrashLoop (incl. ImagePullBackOff and similar): fire; Terminating: faded and crumbling; Failed: scorched pavilion | Problems must stand out |
| Landmarks | A workload's look depends on its name only, not its namespace | The same component deployed in many namespaces looks the same everywhere |
| Noise | Terminal view: one building per DaemonSet, finished jobs only counted; `--all-pods` turned this off (removed in 0.4.0) | 153 DaemonSet pods were 9 DaemonSets; 111 finished jobs were clutter |
| Zoom | Terminal view: `z` cycled detail, overview, map (state-coloured tiles); `f` fades healthy buildings | Overview and focus for finding problems |
| Recording | `--record FILE` / `--replay FILE` (JSON lines of world events) | Reproducible tests on real clusters without access |
| Sixel | Ruled out and removed | Sixel images do not get from WSL to Windows Terminal 1.24: even a test bar printed from the shell does not appear (ConPTY) |
| Pre-rendered sprites | Not in the terminal | Only pay off with more pixels than half blocks give |
| Renderer | Browser (PixiJS/WebGL) is the main view; half blocks were the terminal fallback until 0.4.0 | The owner liked the browser prototype ("I love the feeling it has now") |
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
- Deep links: `#district`, `#street` or `#<namespace>`; `#tour` plays a
  15 s camera loop (whole city, district, a fire, a building site) for the
  README GIF (`art/make_gif.sh`).
- **Two looks** behind a switch in the header (Kenney by default, remembered
  per browser; `#metropolis.<deep link>` or `#kenney.<deep link>` picks one):
  - Metropolis: the vector art above.
  - Kenney: sprites from Kenney's *Isometric Buildings* and *Isometric City*
    packs (CC0). Buildings are stacked from a ground floor, upper floors
    (35 px each in sprite pixels) and a roof: Deployments are office blocks
    with a storey per replica, StatefulSets brick halls with a code-drawn
    tank, twin tanks or water tower on the roof, DaemonSets houses, Jobs
    works with a code-drawn chimney. Fire, smoke and
    the crane stay code-drawn on top. The ground is Kenney tiles: roads,
    pavement, planters, lamps and trees. Sprites go into one atlas at load
    time (with mipmaps); ground floors lose their earth sides there so the
    lot tile underneath shows them.
  - Each look is an object with `body(look)` and `ground()` in the page; the
    rest of the page does not know which look is active.
  - StatefulSet halls and DaemonSet houses are both brick, so they were easy
    to confuse from far away and the packs have no tanks. The owner chose
    (2026-10-23, #14) code-drawn tanks on StatefulSet roofs, keeping the
    "storage" idea from Metropolis; pale tanks read at district zoom.
  - Roads (#15, 2026-10-23): the Kenney look uses the pack's one-tile roads
    with their own kerbs instead of two tiles of asphalt with drawn dashes.
    Each road tile is picked from its neighbours (straights, T-junctions,
    crossroads, rounded corners), with a zebra crossing on the straight next
    to every junction. Roads run around the city, between the shelves and
    along both sides of every block. The pack has no lane dashes, so there
    are none. Each look sets its road width (`gap`: Metropolis 2, Kenney 1),
    so switching looks re-plans the districts; a camera the viewer moved
    stays over the same district.
  - Variety (#16, 2026-10-27): every workload name picks, from its own
    seed, a ground floor, upper floor, facade pattern (all alike, banded,
    other top or other bottom storey), roof, one of four subtle tints
    (original, warm, cool, weathered), whether it is mirrored, and on
    offices a roof detail (antenna, air conditioners, skylights, small
    tank); works also get a chimney height and colour. Three unused shop
    fronts became office ground floors, the pale roof pieces office and
    house roofs. On the big recording every workload got its own look and no
    facade is shared by more than 3 % of a kind (was about 20 %). Mirroring
    swaps Kenney's lit and shaded walls; the owner chose it anyway and it is
    one switch (`MIRROR`) in the page. Only pack 1 is used (owner's choice).
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
- Real names are shown: the server only listens on 127.0.0.1; see
  `--anonymise` below for sharing.

## `--anonymise` (2026-10-23, issue #13)

| Topic | Decision | Why |
|---|---|---|
| Where | `src/anonymise.rs`, a stage on the event channel before `--record` and the page; switched contexts get the same stage | Page, recording and status line all see pseudonyms; one anonymiser per run keeps names consistent |
| Stability | New pseudonyms every run (random salt); `--anonymise-salt` makes them stable | The owner's choice: the code is public, so stable unsalted names could be matched by guessing |
| What | Namespaces (`feature-`/`bugfix-` prefix kept; `default` and `kube-*` kept), workloads (per kind and name, same in every namespace), pod names (StatefulSet ordinals kept), uids, nodes, images (repository a word, tag a short hash), contexts | Every name the page or a recording holds; landmarks still work |
| Status line | Context replaced, error text dropped (`reconnecting`, `cannot connect (details hidden by --anonymise)`); a replay shows `replay recording` | Errors can hold host names; recording file names can say whose cluster it is |
| Hash | Salted SHA-256 (`sha2`, already a dependency of kube) | Stable salted pseudonyms must not be reversible from the public code |
| Terminal view | `--anonymise` conflicted with `--tui` | The terminal view was frozen (removed in 0.4.0) |

## Distribution (2026-10-22)

| Topic | Decision | Why |
|---|---|---|
| Hosting | Public GitHub repo `braveadmin/kube-city` | The owner wants people to use it without compiling |
| Identity | Commits by `braveadmin` with an empty email; history rewritten once to remove the work email, cluster names and employer-specific demo names | Keep the owner's employer and private details out of a public repo |
| License | MIT OR Apache-2.0 | The usual Rust dual license |
| Binaries | cargo-dist on version tags: Linux x86_64 (static musl) and arm64, macOS arm64 and x86_64, Windows x86_64, plus shell and PowerShell installers; pull requests build the same without publishing | One command to install; musl runs on any distro |
| Signing | None; README explains the macOS quarantine command | Apple signing needs a paid account |
| PixiJS | Built into the binary (`assets/pixi`), served at `/pixi.min.js` | `--web` must work without a CDN |
| Fonts (#21) | Latin subsets of Barlow, Barlow Condensed and IBM Plex Mono built into the binary (`assets/fonts`, about 180 KB); the page gets `@font-face` rules for `/fonts/…` in place of the Google Fonts link, which the template keeps for the published prototype | `--web` fully offline, no request to Google; Latin-only keeps the binary small (#22), other characters fall back to system fonts |
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

## Terminal view removed (2026-10-27, #20)

Owner's decision: **remove it** in 0.4.0, not keep it behind a cargo
feature. Nobody asked for it in Discussions or issues, and a feature would
have kept the code, a second CI build and ratatui updates for a view nobody
uses.

Gone: `app.rs`, `layout.rs`, `render/` (the terminal's Metropolis theme and
the `render_snapshots` design tool), `World`/`Rules` in `world.rs` (which
now only holds the events sources send), the unused hash helpers in
`util.rs`, `--fps`, `--all-pods` and the `ratatui` dependency: about 2,300
lines and 34 tests. `--tui` stays as a hidden flag that exits with a
message pointing to 0.3.0 (`cargo install kube-city --version 0.3.0`). The
browser's own layout and both looks live in `art/city.template.html`.

## A more natural city (2026-10-27, #33–#50)

The owner's feedback: the city looks great but feels squared and
predictable. Planned in three parts, each judged from sketches made from
the big recording's sizes (no names in them; links above).

**Layout** (#33–#37). Districts stay rectangles, so stable lots, landmarks
and the cluster overlay keep working.

| Topic | Decision | Why |
|---|---|---|
| Layout | Cut the city in two again and again, by namespace size; every cut is a road (#33) | Removes the long straight roads that cross the whole city; blocks get their own proportions; no more land than today (sketch: 3.2 tiles per building, today 3.4) |
| Blocks | Buildings along the streets, courtyards inside; namespaces over ~40 workloads become several blocks with lanes (#34) | Real blocks instead of lot grids; about 1.2× today's land |
| Edge | No ring road; edge blocks give back the rows they do not need (#35) | A town should thin out, not end in a rectangle |
| River | A river cut through town with bridges (#37) | Owner's pick among the ingredients |
| Zones | Centre, industry, suburbs by namespace (#36); with #47 this decides where quarters sit | Open: by name, by content, or both |

Found while sketching: today's layout already has mostly T-junctions (54
against 3 crossroads), so the straight lines, not crossroads, are what
reads as a grid. Water tiles in the Kenney City pack are not checked yet
(#37 decides: download the full pack, or draw water in code).

**Terrain** (#38–#46): a world generated once from a seed: hills,
mountains, sea, lakes, rivers, forests, fields and roads out of town.

| Topic | Decision | Why |
|---|---|---|
| Seed | Random the first time a context is opened, kept in the browser per context; "New world" rolls another (#38) | Owner's choice. Costs: another browser or cleared site data gives a new world, and with `--anonymise` without `--anonymise-salt` the world changes every run |
| Town site | Flat; terrain blends in a few tiles out; mountains mostly behind the town | In the isometric view nothing may stand in front of a district |
| Styles | Valley, Coast, Island, Lake country, Highlands; the seed picks, a dropdown overrides (#45) | Owner's choice: variety without a setting to learn |
| Height in the Kenney look | Pack tiles as tops, sides and cliffs drawn in code (#40) | Owner's choice; no new download |
| Ground drawing | Drawn once into textures, not a sprite per tile (#40) | About four times today's tiles (~14,000 on the big recording) |
| #35 and #37 | Kept, narrowed: #35 the edge (grass until #43), #37 the river in town (#42 continues it) | Owner's choice: they can ship before the terrain |
| Readability | Calmer terrain at cluster zoom; "fit" frames the town (#46) | Terrain must not hide problems |

Generator, as sketched: layered and ridged noise from seeded tables;
height in 13 levels with tapering peaks; a priority flood turns each hollow
into a whole lake or fills it; the town river runs from its cut into the
hills and down to the sea along the cheapest path; side rivers from water
collecting downhill; ground cover from height, slope and moisture. About
60–130 ms for the big recording in Node.

**Neighbourhoods and zones** (#47–#50): the owner asked for a city spread
out like SimCity or Cities: Skylines (houses, shops, towers).

| Topic | Decision | Why |
|---|---|---|
| Unit | Related namespaces (sharing at least half their workloads) form a quarter; each namespace is a neighbourhood (#47) | Owner's choice; branches of one stack belong together |
| Spacing | One city, neighbourhoods a few tiles apart, split by parks, rivers and main roads (#47) | Owner's choice; keeps the overview near today's size (target ≤1.5× today's land) |
| Stability | A new namespace becomes a neighbourhood on free land without moving the others (#47) | Today a new namespace re-packs the whole city |
| Zones | From what workloads run: image roles (#48), towers from CPU and memory requests (#49), shops from Ingress or LoadBalancer exposure (#50, after #19) | Owner's choice; a building keeps its kind's shape, zones only decide where it stands |
| Roles and `--anonymise` | The server works out the role from images before anonymising and sends only the role word (#48) | Pseudonymised images say nothing |

Why zones need new signals (big recording, 731 workloads): 57 % are
StatefulSets, 26 % Deployments, 16 % Jobs and 1 % DaemonSets (9 houses);
180 of 190 Deployments run one replica, so height by replicas gives no
towers; 70–75 % of workload names contain no role word. Grouping by shared
workloads gives one quarter of 13 branch namespaces with 674 workloads and
14 small namespaces on their own, so the branch quarter needs #48's inner
structure to stay readable.

Order: #33, #34, #35, #37 first; the terrain (#38–#46) and neighbourhoods
(#47–#50) are planned again once those are in and the owner has seen them.

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
- Half blocks stay as a quick terminal view and an SSH fallback (until 0.4.0).

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
