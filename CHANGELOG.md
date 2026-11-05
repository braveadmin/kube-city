# Changelog

## 0.6.0

### Changed

- Jobs are always on the map: a Job's works stands while any of its pods
  exists, finished ones too, so a CronJob stays in its place between runs
  and goes only when its Jobs are deleted. The **Finished jobs** button is
  gone, and the stats line, the district bar and the street-zoom dots
  always count finished pods (#65).
- A Job that runs is busy: thicker smoke, a wheel turning on its wall, lit
  windows that flicker, and a truck driving to the nearest road and back.
  Between runs it stands idle in its own colours; only its first run is a
  construction site. A crashing or failed run looks as before (#65).
- In the demo, each namespace's `report-job` keeps its last three runs,
  like a CronJob's history (#65).

### Fixed

- A Deployment or StatefulSet with a completed pod could show as a
  construction site: finished pods were counted out twice (#65).

## 0.5.0

### Added

- Roads out of town: every road that reaches the edge of town runs on to
  the edge of the world along the easiest way, round steep slopes and
  lakes, up ramps and over rivers on bridges; cars drive them and hide
  behind hills (#44).
- World styles: the seed picks a valley, a coast, an island with a harbour,
  lake country or highlands, and a **World style** dropdown can force one;
  the legend names the style and the seed (#45).
- Quarter labels at cluster zoom: each quarter of related namespaces gets
  one label with its shared name ("feature-* · 6") and the pods, problems
  and pending pods of all of them (#47).
- Each workload's role (web, office, data, messaging or worker), worked
  out from its images, in the hover tip, the inspector and the legend
  (#48).
- Towers from CPU and memory requests: offices and brick halls get two
  storeys for every doubling of what their pods request (4 GiB counts as a
  core), from 2 storeys at 0.1 core up to 12, the heaviest in the middle
  of their block; the inspector and hover tip show the requests (#49).
- Shops: workloads reached from outside the cluster (behind an Ingress, a
  Gateway HTTPRoute or GRPCRoute, a LoadBalancer or a NodePort Service)
  get a shop front and stand on their neighbourhood's street, facing the
  middle of town; the inspector shows how each is reached. kube-city now
  also lists and watches Services, Ingresses and Gateway routes, still
  read-only; without access to them the city runs without shops and the
  status line says so (#50).

### Changed

- The city is a town of neighbourhoods: each namespace is laid out on its
  own, namespaces that share at least half their workloads form a quarter,
  with parks between neighbourhoods and wider belts between quarters. A new
  namespace finds free land without moving the others, and the river bends
  between the two halves of town (#47).
- Inside a neighbourhood, workloads stand by role: web servers at the
  front, then offices, data and messaging, Jobs at the back (#48).
- The terrain stays readable at every zoom: its colours and details calm
  down as the district colours come in, and the farthest zoom frames the
  town with a ring of country instead of the whole world (#46).
- `--anonymise` also gives Services and host names made-up names and drops
  load balancers' addresses (#50).
- In the Kenney look, ground floors with awnings are kept for shops; works
  and brick halls have plain ones (#50).
- No macOS binaries in this release (#80); on macOS, `cargo install
  kube-city` works.

## 0.4.0

### Added

- The fonts (Barlow, Barlow Condensed, IBM Plex Mono) are built into the
  binary, so the page makes no request to Google Fonts and looks the same
  without internet (#21).
- A world per cluster: the land round the town grows from a seed the page
  picks the first time it shows a context and keeps in the browser for it
  (one for the demo and one for replays); the **World** section of the
  details panel shows it, takes a typed one and rolls a new one with
  **New world**. The trees, terrain, water and land cover all grow from the
  seed (#38).
- Terrain round the town from the world seed: rolling hills, mountain
  ranges and a flat town site, drawn at their height in both looks (#39,
  #40).
- Sea, coast and lakes: on about half the worlds the land sinks into the
  sea on one side, with beaches, bays and headlands; hollows become lakes;
  the water glints (#41).
- Forests, woods, fields with farms, rock and snow cover the land outside
  the town (#43).
- The town's river runs on outside it, down to the sea and up into the
  hills, cutting small valleys; side rivers join it, with waterfalls where
  they drop (#42).
- Zones: branch namespaces (`feature-`, `bugfix-`) are suburbs,
  infrastructure is an industrial quarter and the rest is the town centre,
  each with its own ground and a **Zones** row in the legend (#36).
- `#tour` after the page's address plays a 15-second camera loop round the
  city until you pan, zoom or press a key; the README opens with a GIF of
  it.

### Changed

- Release binaries are about half the size (17 MB to 8 MB on Linux, about
  4 MB to download): symbols stripped, built with full LTO and optimised
  for size; `cargo install` builds are stripped too (#22).
- The city appears sooner: the sprites come inside the page instead of in
  144 requests, and a live city connects while the sprites load (#22).
- The city is laid out by cutting it in two again and again, each cut a
  road, instead of in rows: no more long straight roads across the whole
  city, blocks of their own proportions, main roads lined with trees,
  pocket parks, and cars that turn at junctions (#33).
- Blocks are built along their streets first, with a green courtyard in the
  middle; a namespace of more than about 50 workloads is a quarter of
  several blocks with lanes between them, still one district (#34).
- No more ring road: the town thins out into grass with a few trees, edge
  blocks give back the rows they do not need, main roads run on out of town
  and lanes may end at its edge (#35).
- A river runs through town, between the suburbs and the middle of town,
  with a road on each bank and one to three bridges that the cars cross;
  both looks draw it (#37).

### Removed

- The terminal view (`--tui`), deprecated since 0.2.0, with `--fps` and
  `--all-pods`. `--tui` now only says so; `cargo install kube-city
  --version 0.3.0` still has it. About half of the code and the `ratatui`
  dependency go with it (#20).

## 0.3.0

### Added

- `--anonymise`: made-up names for namespaces, workloads, pods, nodes,
  images and contexts in the page, the status line and `--record` files, so
  a real cluster can be shared; `--anonymise-salt` keeps the names stable
  between runs (#13).
- StatefulSets carry a tank, twin tanks or a water tower on their roof in
  the Kenney look, so they no longer look like DaemonSet houses (#14).
- Kenney roads use the pack's own road tiles: junctions, rounded corners
  and a zebra crossing next to every junction (#15).
- More building variety in the Kenney look: tints, mirrored buildings,
  mixed facades, roof details and more shop fronts and roofs, all chosen by
  the workload's name (#16).

## 0.2.0

### Breaking

- **The browser is the default.** Plain `kube-city` opens the city in your
  browser (what `--web` did; `--web` still works). The terminal view moved
  behind `--tui`, is deprecated and gets bug fixes only; `--fps` and
  `--all-pods` now need `--tui`. Over SSH, forward the port:
  `ssh -L 7700:127.0.0.1:7700 server`, then `kube-city --no-open`.

### New

- **Pod details** in the details panel: pod name, readiness, restarts, age,
  node, images, and why a pod is not simply running (ImagePullBackOff,
  OOMKilled, Evicted, …).
- **Context dropdown:** switch between the contexts of your kubeconfig
  without restarting.
- **Theme dropdown** (Kenney or Metropolis) and a **collapsible details
  panel** (`i`).
- **Banners toggle** (`b`) to hide all name signs in crowded districts.
- The page shows which **version** is running.

### Security

- The live connection only accepts kube-city's own page, so other websites
  cannot read the city or switch its context.

## 0.1.1

- Homebrew (`brew install braveadmin/tap/kube-city`) and crates.io
  (`cargo install kube-city`); Linux and macOS archives are `.tar.gz`.

## 0.1.0

- First release: the city in the browser (`--web`) and in the terminal,
  live and read-only, with binaries and installers for Linux, macOS and
  Windows.
