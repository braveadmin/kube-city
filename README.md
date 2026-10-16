# kube-city

A living isometric city of your Kubernetes cluster, drawn in the terminal.

```bash
cargo run --release                                  # current kubectl context
cargo run --release -- --context my-cluster       # another context
cargo run --release -- -n kube-system -n monitoring  # only these namespaces
cargo run --release -- --demo                        # simulated cluster
cargo run --release -- --demo --seed 7               # replay the same city
cargo run --release -- --record city.jsonl           # also save what happens
cargo run --release -- --replay city.jsonl           # play a saved city back
```

Contexts come from `KUBECONFIG` (all listed files, merged like kubectl does).
kube-city only lists and watches pods. It connects before taking over the
terminal, so login prompts (e.g. kubelogin device code) and access errors show
up as plain text. If the watch drops, the status bar says so and it reconnects;
pods deleted in the meantime disappear once it has relisted.

Keys: arrows / `hjkl` pan, `z` cycle detail, overview and map, `f` fade
healthy buildings so problems stand out, `c` re-centre, `q` / `Esc` /
`Ctrl+C` quit.

`--record FILE` writes every change as JSON lines while the city runs, and
`--replay FILE` plays it back (`--replay-speed 10` for ten times faster): a
real cluster's city without access to the cluster.

## The city

Drawn in the Metropolis theme: daylight pixel art in the spirit of SimCity
2000. Each image pixel is half a terminal cell (`▀` with true colour).

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
namespaces is recognisable everywhere. `--all-pods` shows every pod as its own
building, finished jobs (pavilions with a green tick) included.

The map zoom draws each building as a small tile coloured by state, so a big
cluster fits on screen and problems show as red spots.

Every pod keeps its lot for its whole life. Blocks are handed out along a
spiral around the centre, so the city grows outwards; a block that empties
goes back to the city.

## Themes

`src/render/theme/` holds the looks. The renderer decides what stands where
and in which order things are drawn; a `Theme` paints ground, buildings and
free lots with the primitives in `src/render/scene.rs` (boxes, walls with
window patterns, pitched roofs, round solids), and works at both zooms.
Metropolis is the only theme so far. `art/samples.py` renders the three
candidate styles (Metropolis, Neon night, Toy town) as reference images:
`python3 art/samples.py art/out`.
