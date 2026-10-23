# Roadmap

Where kube-city is going, roughly in order. Every item is a GitHub issue;
the next release is the [0.3.0 milestone](https://github.com/braveadmin/kube-city/milestone/1).
Ideas, votes and questions are welcome in
[Discussions → Ideas](https://github.com/braveadmin/kube-city/discussions/categories/ideas).

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

## Next (0.3.0)

- [`--anonymise`](https://github.com/braveadmin/kube-city/issues/13): pseudonyms for sharing screenshots and demos of real clusters.
- [StatefulSets and DaemonSets easier to tell apart](https://github.com/braveadmin/kube-city/issues/14) in the Kenney look.
- [Kenney road tiles](https://github.com/braveadmin/kube-city/issues/15) with lane markings and crossings.
- [More building variety](https://github.com/braveadmin/kube-city/issues/16) in the Kenney look.
- [Demo GIF](https://github.com/braveadmin/kube-city/issues/17) at the top of the README.

## Later

- [All contexts at the same time](https://github.com/braveadmin/kube-city/issues/18).
- [More Kubernetes objects](https://github.com/braveadmin/kube-city/issues/19): Nodes, Services, Ingresses, volumes.
- [Remove the deprecated terminal view](https://github.com/braveadmin/kube-city/issues/20), or keep it behind a build feature, depending on feedback.
- [Fonts built into the binary](https://github.com/braveadmin/kube-city/issues/21) (fully offline page).
- [Smaller release binaries and faster sprite loading](https://github.com/braveadmin/kube-city/issues/22).
- [krew](https://github.com/braveadmin/kube-city/issues/23): `kubectl krew install`; until then a symlink gives you `kubectl city` (see the README).
