# Roadmap

Where kube-city is going, roughly in order. Ideas, votes and questions are
welcome in [Discussions → Ideas](https://github.com/braveadmin/kube-city/discussions/categories/ideas).

## Done

- **Two views:** the city in the browser (`kube-city --web`, WebGL) and in the
  terminal, live and read-only, from a cluster, a recording or a demo.
- **Two themes:** Kenney (sprites from Kenney's CC0 packs) and Metropolis
  (drawn in code), picked from the **Theme** dropdown.
- **Readable big clusters:** cluster, district and street zoom levels; search;
  focus on problems; landmarks that look the same in every namespace.
- **Context dropdown:** switch to any context of your kubeconfig without
  restarting.
- **Collapsible details panel**, with the pods of the selected workload.
- **Releases:** binaries for Linux, macOS and Windows with shell and
  PowerShell installers (v0.1.0).
- **Package managers:** Homebrew (`brew install braveadmin/tap/kube-city`)
  and crates.io (`cargo install kube-city`) (v0.1.1).

## Next

- **Demo GIF** in the README: the city growing, burning and rebuilding.
- **Kenney polish:** StatefulSets and DaemonSets easier to tell apart, road
  tiles with crossings, more building variety.
- **Richer pod details:** restarts, age, node, image and the reason a pod is
  waiting (for example ImagePullBackOff).
- **`--anonymise`:** pseudonyms for namespaces and workloads, for sharing
  screenshots and demos of real clusters.

## Later

- **All contexts at the same time:** every cluster of your kubeconfig in one
  view, side by side or as groups of districts.
- **More Kubernetes objects:** Nodes as land, Services as roads or bridges,
  Ingresses as city gates, volumes as depots.
- **Smaller improvements:** fonts built into the binary (fully offline
  `--web`), smaller release binaries, faster sprite loading.
- **krew:** `kubectl krew install` from the central plugin index. Until then,
  a symlink gives you `kubectl city` (see the README).
