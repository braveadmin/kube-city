# kube-city

A living isometric city of your Kubernetes cluster, drawn in the terminal.

```bash
cargo run --release -- --demo            # simulated cluster
cargo run --release -- --demo --seed 7   # replay the same city
```

Keys: arrows / `hjkl` pan, `c` re-centre, `q` / `Esc` / `Ctrl+C` quit.

## The city

| Kubernetes             | City                                          |
|------------------------|-----------------------------------------------|
| Namespace              | District (one or more 4×4 blocks)             |
| Pod                    | Building, on a lot it keeps for its lifetime  |
| Workload (Deployment…) | Building colour and height                    |
| Pending                | Low orange scaffolding                        |
| Running                | Lit windows                                   |
| CrashLoopBackOff       | Red roof, dark windows                        |
| Terminating            | Faded, dark windows                           |
| Succeeded / Failed     | Low grey / dark red stub                      |

Blocks are handed out along a spiral around the centre, so the city grows
outwards; a block that empties goes back to the city.

## Status

Demo mode only. Watching a real cluster (`--context`, `--namespace`) comes next.
