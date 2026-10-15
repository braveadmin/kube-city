# kube-city

A living isometric city of your Kubernetes cluster, drawn in the terminal.

```bash
cargo run --release                                  # current kubectl context
cargo run --release -- --context my-cluster       # another context
cargo run --release -- -n kube-system -n monitoring  # only these namespaces
cargo run --release -- --demo                        # simulated cluster
cargo run --release -- --demo --seed 7               # replay the same city
```

Contexts come from `KUBECONFIG` (all listed files, merged like kubectl does).
kube-city only lists and watches pods. It connects before taking over the
terminal, so login prompts (e.g. kubelogin device code) and access errors show
up as plain text. If the watch drops, the status bar says so and it reconnects;
pods deleted in the meantime disappear once it has relisted.

Keys: arrows / `hjkl` pan, `c` re-centre, `q` / `Esc` / `Ctrl+C` quit.

## The city

| Kubernetes             | City                                          |
|------------------------|-----------------------------------------------|
| Namespace              | District (one or more 4×4 blocks)             |
| Pod                    | Building, on a lot it keeps for its lifetime  |
| Workload               | Building colour and height (pods of one Deployment, StatefulSet, CronJob… look alike) |
| Pending                | Low orange scaffolding                        |
| Running                | Lit windows                                   |
| CrashLoopBackOff       | Red roof, dark windows                        |
| Terminating            | Faded, dark windows                           |
| Succeeded / Failed     | Low grey / dark red stub                      |

Blocks are handed out along a spiral around the centre, so the city grows
outwards; a block that empties goes back to the city.

CrashLoopBackOff covers every container stuck waiting for help:
ImagePullBackOff, ErrImagePull, CreateContainerConfigError and similar.
