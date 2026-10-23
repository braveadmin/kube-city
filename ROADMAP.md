# Roadmap

Where kube-city is going, roughly in order. Ideas, votes and questions are
welcome in [Discussions → Ideas](https://github.com/braveadmin/kube-city/discussions/categories/ideas).

## Done

- **The city in your browser** (WebGL), live and read-only, from a cluster,
  a recording or a demo. The terminal view (`--tui`) is deprecated.
- **Two themes:** Kenney (sprites from Kenney's CC0 packs) and Metropolis
  (drawn in code), picked from the **Theme** dropdown.
- **Readable big clusters:** cluster, district and street zoom levels; search;
  focus on problems; landmarks that look the same in every namespace.
- **Context dropdown:** switch to any context of your kubeconfig without
  restarting.
- **Collapsible details panel**, with the pods of the selected workload:
  name, readiness, restarts, age, node, images and why a pod is stuck.
- **Releases:** binaries for Linux, macOS and Windows with shell and
  PowerShell installers (v0.1.0).
- **Package managers:** Homebrew (`brew install braveadmin/tap/kube-city`)
  and crates.io (`cargo install kube-city`) (v0.1.1).
- **v0.2.0:** browser by default (terminal view deprecated), pod details,
  context and theme dropdowns, banners toggle, version in the page. See
  [CHANGELOG.md](CHANGELOG.md).

## Next

- **Demo GIF** in the README: the city growing, burning and rebuilding.
- **Kenney polish:** StatefulSets and DaemonSets easier to tell apart, road
  tiles with crossings, more building variety.
- **`--anonymise`:** pseudonyms for namespaces and workloads, for sharing
  screenshots and demos of real clusters.

## Later

- **All contexts at the same time:** every cluster of your kubeconfig in one
  view, side by side or as groups of districts.
- **More Kubernetes objects:** Nodes as land, Services as roads or bridges,
  Ingresses as city gates, volumes as depots.
- **Smaller improvements:** fonts built into the binary (fully offline
  `--web`), smaller release binaries, faster sprite loading.
- **Remove the deprecated terminal view** (around 0.3), or keep it behind an
  optional build feature, depending on feedback.
- **krew:** `kubectl krew install` from the central plugin index. Until then,
  a symlink gives you `kubectl city` (see the README).
