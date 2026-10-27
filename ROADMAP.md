# Roadmap

Where kube-city is going, roughly in order. Every item is a GitHub issue.
Ideas, votes and questions are welcome in
[Discussions → Ideas](https://github.com/braveadmin/kube-city/discussions/categories/ideas).

## Done

- **The city in your browser** (WebGL), live and read-only, from a cluster,
  a recording or a demo.
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
- **v0.3.0:** `--anonymise` for sharing real clusters; in the Kenney look,
  tanks on StatefulSet roofs, the pack's road tiles with junctions and
  crossings, and much more building variety.

## Next

- [Remove the deprecated terminal view](https://github.com/braveadmin/kube-city/issues/20) (v0.4.0).

## Planned: a more natural city

The city's layout, then a world around it, then neighbourhoods with zones.

- **Layout:** [cut the city by roads](https://github.com/braveadmin/kube-city/issues/33)
  instead of shelf packing, [blocks with buildings along the streets](https://github.com/braveadmin/kube-city/issues/34),
  a [ragged edge instead of a ring road](https://github.com/braveadmin/kube-city/issues/35),
  a [river through town](https://github.com/braveadmin/kube-city/issues/37) and
  [zones](https://github.com/braveadmin/kube-city/issues/36) (centre, industry, suburbs).
- **A world from a seed:** [a seed per context](https://github.com/braveadmin/kube-city/issues/38),
  [hills and mountains](https://github.com/braveadmin/kube-city/issues/39),
  [terrain with height](https://github.com/braveadmin/kube-city/issues/40),
  [sea, coast and lakes](https://github.com/braveadmin/kube-city/issues/41),
  [rivers](https://github.com/braveadmin/kube-city/issues/42),
  [forests, fields, rock and snow](https://github.com/braveadmin/kube-city/issues/43),
  [roads out of town](https://github.com/braveadmin/kube-city/issues/44),
  [world styles](https://github.com/braveadmin/kube-city/issues/45), and
  [terrain that stays readable](https://github.com/braveadmin/kube-city/issues/46) at every zoom.
- **Neighbourhoods:** [related namespaces as quarters](https://github.com/braveadmin/kube-city/issues/47),
  [zones from what workloads run](https://github.com/braveadmin/kube-city/issues/48),
  [towers from CPU and memory requests](https://github.com/braveadmin/kube-city/issues/49) and
  [a shopping street](https://github.com/braveadmin/kube-city/issues/50) of exposed workloads.

## Later

- [Demo GIF](https://github.com/braveadmin/kube-city/issues/17) at the top of the README.
- [All contexts at the same time](https://github.com/braveadmin/kube-city/issues/18).
- [More Kubernetes objects](https://github.com/braveadmin/kube-city/issues/19): Nodes, Services, Ingresses, volumes.
- [Fonts built into the binary](https://github.com/braveadmin/kube-city/issues/21) (fully offline page).
- [Smaller release binaries and faster sprite loading](https://github.com/braveadmin/kube-city/issues/22).
- [krew](https://github.com/braveadmin/kube-city/issues/23): `kubectl krew install`; until then a symlink gives you `kubectl city` (see the README).
