# kube-city

A living isometric city of your Kubernetes cluster. Namespaces are
districts, workloads are buildings, and pods coming and going make the city
grow, burn and rebuild, live.

![kube-city in the browser: districts of office blocks, houses and brick halls, one of them on fire](https://raw.githubusercontent.com/braveadmin/kube-city/master/docs/screenshot.png)

- **In your browser.** `kube-city` opens the city in your browser (WebGL,
  mouse-wheel zoom, search, details on click). The old terminal view is still
  there with `--tui`, but deprecated.
- **Read-only.** It only lists and watches pods, the way `kubectl get pods -w`
  does, and never changes anything in the cluster.
- **Big clusters stay readable.** Zoom out to one district per namespace,
  coloured by health; zoom in for buildings, then names and individual pods.
- **Problems stand out.** Crashing workloads burn, pending ones are
  construction sites, failed ones are burnt-out shells; `f` fades everything
  that is healthy.

## Install

Linux and macOS:

```bash
curl --proto '=https' --tlsv1.2 -LsSf https://github.com/braveadmin/kube-city/releases/latest/download/kube-city-installer.sh | sh
```

Windows (PowerShell):

```powershell
powershell -ExecutionPolicy Bypass -c "irm https://github.com/braveadmin/kube-city/releases/latest/download/kube-city-installer.ps1 | iex"
```

Homebrew (macOS and Linux):

```bash
brew install braveadmin/tap/kube-city
```

With Rust: `cargo install kube-city`.

As a kubectl plugin (`kubectl city`), link it under a name kubectl
looks for, in a directory on your `PATH`:

```bash
ln -s "$(command -v kube-city)" ~/.local/bin/kubectl-city
```

Or download an archive for your platform from the
[releases page](https://github.com/braveadmin/kube-city/releases).

The binaries are not signed. On macOS, if Gatekeeper refuses to open a
downloaded binary, run `xattr -d com.apple.quarantine kube-city` once
(Homebrew installs do not need this).

## Use

```bash
kube-city                               # current kubectl context, in the browser
kube-city --context my-cluster          # another context
kube-city -n shop -n payments           # only these namespaces
kube-city --demo                        # a simulated cluster, no cluster needed
kube-city --tui                         # the terminal view (deprecated)
```

kube-city serves the city on `http://127.0.0.1:7700/` (`--port` to change
it) and opens it in your browser; under WSL that is the Windows browser.
`--no-open` only prints the address. The page stays live while it is open; a
page opened later, or one that lost the connection, gets the whole city
again. (`--web` still works; it is the default now.)

On a remote machine, forward the port and open the city locally:
`ssh -L 7700:127.0.0.1:7700 server`, then run `kube-city --no-open` on the
server and open `http://127.0.0.1:7700/` on your computer.

In the browser: scroll or pinch to zoom through three levels (cluster,
district, street), drag to pan, hover for details, click a building for its
pods and the same component in other namespaces. Keys: arrows pan, `+` `-`
zoom, `c` fits the city, `/` searches, `f` fades healthy buildings, `i`
folds the details panel away, `b` hides the name banners (handy in crowded
districts), `Esc` clears. The header shows which version is running. The **Theme** dropdown switches
between Kenney (sprites) and Metropolis (drawn in code). When the city shows
a cluster, the **Context** dropdown switches to any other context in your
kubeconfig without restarting (not while `--record` is saving).

`--record city.jsonl` saves everything that happens and `--replay
city.jsonl` plays it back later (`--replay-speed 10` for ten times faster):
handy for showing a cluster without access to it. Recordings contain your
namespace and workload names.

`--anonymise` replaces every name (namespaces, workloads, pods, nodes,
images, contexts) with a made-up one before it leaves kube-city: in the
page, in the status line, in the **Context** dropdown and in `--record`
files. Use it for screenshots, talks and recordings you want to share; it
also anonymises an old recording played with `--replay`. The made-up names
are new every run; `--anonymise-salt SECRET` keeps them the same between
runs (keep the salt secret, or names could be matched by guessing).

### What it needs

- A kubeconfig: `KUBECONFIG` (all listed files, merged like kubectl does) or
  `~/.kube/config`. Exec plugins such as `kubelogin` or `aws eks get-token`
  work; kube-city connects before it starts, so login prompts show up as
  plain text.
- Permission to `list` and `watch` pods, in all namespaces or in the ones
  you pass with `-n`.
- A browser with WebGL. The page is served only on 127.0.0.1;
  it loads its fonts from Google Fonts and falls back to system fonts
  without internet. Everything else (PixiJS, sprites) is in the binary.

## The browser city

| Kubernetes        | Kenney look                                  |
|-------------------|----------------------------------------------|
| Namespace         | A block with its name on a road sign         |
| Deployment        | Office block, a storey per replica           |
| StatefulSet       | Brick hall with a tank on the roof           |
| DaemonSet         | House with a pitched roof                    |
| Job               | Works with a chimney, smoking while it runs  |
| Pod without owner | Kiosk                                        |
| Pending           | Dug foundation with a crane                  |
| Crashing          | On fire (CrashLoopBackOff, ImagePullBackOff and similar) |
| Failed            | Burnt-out shell                              |
| Terminating       | Fading                                       |

One building stands for one workload; zoom to street level for one dot per
pod. Click a building to see its pods: name, readiness, restarts, age, node
and, in red, why a pod is not simply running (ImagePullBackOff, OOMKilled,
Evicted, …), plus the workload's images. A workload looks the same, and stands in the same spot, in every
namespace that has it, so environments that deploy the same stack are easy
to compare.

## The terminal city (deprecated)

The terminal view is frozen: it gets bug fixes but no new features, and it
will be removed in a later release, depending on feedback (see
[ROADMAP.md](ROADMAP.md)). Tell us in
[Discussions](https://github.com/braveadmin/kube-city/discussions) if you
rely on it.

Drawn in the Metropolis theme: daylight pixel art in the spirit of SimCity
2000. Each image pixel is half a terminal cell (`▀` with true colour).

Run `kube-city --tui`; it needs a true-colour terminal. Keys:
arrows / `hjkl` pan, `z` cycles detail, overview and map, `f` fades healthy
buildings, `c` re-centres, `q` / `Esc` / `Ctrl+C` quit. If the watch drops,
the status bar says so and it reconnects.

| Kubernetes          | City                                                       |
|---------------------|------------------------------------------------------------|
| Namespace           | District: one or more blocks of 4×4 lots, with its own ground colour |
| Deployment pod      | Office tower; workload sets colour, height and setbacks    |
| StatefulSet pod     | Storage tank, twin tanks or silo, coloured per workload    |
| DaemonSet           | One house for all its pods, showing the worst state        |
| Job pod             | Factory with a smoking chimney                             |
| Pod without owner   | Kiosk                                                      |
| Free lot            | Lawn or trees                                              |

| Pod state           | Building                                                   |
|---------------------|------------------------------------------------------------|
| Pending             | Construction site with a working crane                     |
| Running             | Lit windows that switch on and off over time               |
| CrashLoopBackOff    | Fire and smoke on the roof, glowing windows                |
| Terminating         | Faded, partly demolished, with dust                        |
| Succeeded / Failed  | Counted in the status bar / scorched pavilion with a red cross |

CrashLoopBackOff covers every container stuck waiting for help:
ImagePullBackOff, ErrImagePull, CreateContainerConfigError and similar.

A workload looks the same in every namespace, so a component deployed in many
namespaces is recognisable everywhere. `--tui --all-pods` shows every pod as its own
building, finished jobs (pavilions with a green tick) included.

The map zoom draws each building as a small tile coloured by state, so a big
cluster fits on screen and problems show as red spots.

Every pod keeps its lot for its whole life. Blocks are handed out along a
spiral around the centre, so the city grows outwards; a block that empties
goes back to the city.

## Terminal themes

`src/render/theme/` holds the terminal view's looks (the browser's are in
`art/city.template.html`). The renderer decides what stands where
and in which order things are drawn; a `Theme` paints ground, buildings and
free lots with the primitives in `src/render/scene.rs` (boxes, walls with
window patterns, pitched roofs, round solids), and works at both zooms.
Metropolis is the only theme so far. `art/samples.py` renders the three
candidate styles (Metropolis, Neon night, Toy town) as reference images:
`python3 art/samples.py art/out`.

## Questions, ideas and bugs

- Questions and ideas: [Discussions](https://github.com/braveadmin/kube-city/discussions).
- Bugs: [issues](https://github.com/braveadmin/kube-city/issues), with your
  kube-city version (`kube-city --version`), operating system and browser.
- Security problems: report them privately, see [SECURITY.md](SECURITY.md).
- What is planned: [ROADMAP.md](ROADMAP.md).

## Development

```bash
cargo run --release -- --demo   # browser view of the simulated cluster
cargo test
cargo clippy --all-targets
```

The browser page is `art/city.template.html`; `src/web.rs` serves it.
`docs/decisions.md` records how the project got here and why.

## License

kube-city is licensed under either of [Apache License 2.0](LICENSE-APACHE) or
[MIT license](LICENSE-MIT), at your option.

It includes:

- Sprites from Kenney's *Isometric Buildings* and *Isometric City* packs
  (CC0, [kenney.nl](https://kenney.nl)), in `assets/kenney/`.
- [PixiJS](https://pixijs.com) 7.4.2 (MIT, see `assets/pixi/LICENSE`).

The page uses the Barlow, Barlow Condensed and IBM Plex Mono fonts from
Google Fonts (SIL Open Font License).
