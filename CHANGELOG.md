# Changelog

## Unreleased

### Added

- The fonts (Barlow, Barlow Condensed, IBM Plex Mono) are built into the
  binary, so the page makes no request to Google Fonts and looks the same
  without internet (#21).
- A world per cluster: the land round the town grows from a seed the page
  picks the first time it shows a context and keeps in the browser for it
  (one for the demo and one for replays); the **World** section of the
  details panel shows it, takes a typed one and rolls a new one with
  **New world**. For now the seed places the trees on the grass; the
  terrain will grow from it (#38).

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
