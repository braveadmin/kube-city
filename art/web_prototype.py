#!/usr/bin/env python3
"""Builds the browser prototype page from a kube-city recording.

Names in a recording are real, so every namespace, workload and pod id is
replaced by a pseudonym before it goes into the page. The same real name
always gets the same pseudonym, so a component deployed in many namespaces
still looks the same everywhere. The page is written to `art/out/city.html`
and refuses to be written if a real name would leak into it.

    python3 art/web_prototype.py recordings/big-cluster.jsonl art/out [KENNEY_DIR]

The page embeds the Kenney sprites it uses (CC0, kenney.nl) from
`assets/kenney`, or from KENNEY_DIR when given (a folder holding the packs
unpacked as `buildings/` and `city/`).
"""

import base64
import hashlib
import json
import os
import re
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
KENNEY_DIR = os.path.join(HERE, "..", "assets", "kenney")
SALT = "kube-city-web"

KINDS = ["Deployment", "StatefulSet", "DaemonSet", "Job", "Standalone"]
STATES = ["Pending", "Running", "CrashLoop", "Terminating", "Succeeded", "Failed"]

WORDS = """
alder amber aspen basalt birch bramble brook cedar cinder clover cobalt coral
cypress dawn delta dune egret elm ember fern finch flint fjord garnet glacier
granite harbor hazel heron hollow indigo iris jade juniper kelp kestrel lark
larch linden lumen maple marble meadow mesa moss nimbus north nova oak ochre
onyx orchid osprey pebble pine plover quartz quill raven reef ridge river rowan
sable saffron sage slate sorrel spruce summit swift tansy teal thistle tide
topaz tundra umber vale violet willow wren yarrow zephyr zinc
""".split()

SUFFIXES = {
    "Deployment": ["api", "service", "worker", "gateway", "ui", "controller", "router", "scheduler"],
    "StatefulSet": ["db", "store", "queue", "cache", "broker", "index", "ledger", "vault"],
    "DaemonSet": ["agent", "exporter", "node-proxy", "collector"],
    "Job": ["migrate", "seed", "backup", "report", "sync", "cleanup"],
    "Standalone": ["debug", "toolbox", "probe"],
}

# Namespaces every cluster has, and well-known infrastructure as a namespace's
# name or the start of it ("cert-manager-webhooks"): public product names, kept as
# `kube-city --anonymise` keeps them, so the page still puts them in the
# industrial quarter. The same list is in src/anonymise.rs and
# city.template.html.
KEPT_NAMESPACES = ["default", "kube-system", "kube-public", "kube-node-lease"]
INFRA = "argo argocd calico cert-manager cilium crossplane external-dns external-secrets flux forgejo gatekeeper gitea gitlab grafana ingress istio jaeger keda kyverno linkerd logging loki longhorn metallb minio monitoring observability openbao prometheus rook tekton tigera traefik vault velero".split()
INFRA_SUFFIXES = ["-system", "-operator", "-operators"]


def digest(text):
    return int(hashlib.sha256(f"{SALT}:{text}".encode()).hexdigest(), 16)


class Pseudonyms:
    def __init__(self):
        self.taken = set()
        self.names = {}

    def pick(self, real, candidates, ordered=False):
        if real in self.names:
            return self.names[real]
        h = 0 if ordered else digest(real)
        for step in range(len(candidates) * 4):
            name = candidates[(h + step) % len(candidates)]
            if step >= len(candidates):
                name = f"{name}-{step // len(candidates) + 1}"
            if name not in self.taken:
                self.taken.add(name)
                self.names[real] = name
                return name
        raise RuntimeError(f"ran out of pseudonyms for {real!r}")


def namespace_name(real, names):
    for prefix in ("feature", "bugfix"):
        if real.startswith(prefix + "-"):
            h = digest(real)
            words = [f"{prefix}-{w}-{100 + (h // 7 + i * 37) % 900}" for i, w in enumerate(WORDS)]
            return names.pick(real, words[h % len(words):] + words[:h % len(words)])
    if real in KEPT_NAMESPACES or real in INFRA:
        return names.pick(real, [real], ordered=True)
    for prefix in INFRA + ["kube"]:
        if real.startswith(prefix + "-"):
            return names.pick(real, [f"{prefix}-{w}" for w in WORDS])
    for suffix in INFRA_SUFFIXES:
        if real.endswith(suffix) and len(real) > len(suffix):
            return names.pick(real, [f"{w}{suffix}" for w in WORDS])
    return names.pick(real, [f"platform-{w}" for w in WORDS])


def workload_name(real, kind, names):
    suffixes = SUFFIXES[kind]
    h = digest(real)
    candidates = [f"{WORDS[(h + i) % len(WORDS)]}-{suffixes[(h // 3 + i) % len(suffixes)]}"
                  for i in range(len(WORDS) * len(suffixes))]
    return names.pick((kind, real), candidates)


def anonymise(lines):
    ns_names, wl_names = Pseudonyms(), Pseudonyms()
    ns_index, wl_index, pod_index = {}, {}, {}
    namespaces, workloads, events = [], [], []
    real = set()

    def index(table, key, out, make):
        if key not in table:
            table[key] = len(out)
            out.append(make())
        return table[key]

    for line in lines:
        if not line.strip():
            continue
        entry = json.loads(line)
        at, event = entry["at_ms"], entry["event"]
        if "PodUpserted" in event:
            pod = event["PodUpserted"]
            real.update([pod["namespace"], pod["workload"]])
            ns = index(ns_index, pod["namespace"], namespaces,
                       lambda: namespace_name(pod["namespace"], ns_names))
            wl = index(wl_index, (pod["kind"], pod["workload"]), workloads,
                       lambda: workload_name(pod["workload"], pod["kind"], wl_names))
            uid = pod_index.setdefault(pod["uid"], len(pod_index))
            events.append([at, uid, ns, wl, KINDS.index(pod["kind"]), STATES.index(pod["state"])])
        elif "PodDeleted" in event:
            uid = pod_index.get(event["PodDeleted"]["uid"])
            if uid is not None:
                events.append([at, uid])
    return {"ns": namespaces, "wl": workloads, "events": events}, real


def leaks(page, real, data):
    """Real names that would end up in the page, ignoring generic ones."""
    allowed = set(data["ns"]) | set(data["wl"]) | set(WORDS) | set(INFRA)
    found = []
    for name in sorted(real):
        if len(name) < 5 or name in allowed:
            continue
        if re.search(r"(?<![\w-])" + re.escape(name) + r"(?![\w-])", page):
            found.append(name)
    return found


SPRITE_DIRS = {"b": ("buildings/PNG", "buildingTiles"), "c": ("city/PNG", "cityTiles"), "d": ("city/Details", "cityDetails")}


def kenney_sprites(template, pack_dir):
    """Data URIs of every Kenney sprite the template names, like "b001"."""
    sprites = {}
    for name in sorted(set(re.findall(r'"([bcd]\d{3})"', template))):
        folder, prefix = SPRITE_DIRS[name[0]]
        path = os.path.join(pack_dir, folder, f"{prefix}_{name[1:]}.png")
        with open(path, "rb") as f:
            sprites[name] = "data:image/png;base64," + base64.b64encode(f.read()).decode()
    return sprites


def main():
    if len(sys.argv) not in (3, 4):
        sys.exit(__doc__)
    source, out_dir = sys.argv[1:3]
    pack_dir = sys.argv[3] if len(sys.argv) == 4 else KENNEY_DIR
    with open(source) as f:
        data, real = anonymise(f)
    with open(os.path.join(HERE, "city.template.html")) as f:
        template = f.read()
    with open(os.path.join(HERE, "..", "Cargo.toml")) as f:
        data["version"] = re.search(r'^version = "([^"]+)"', f.read(), re.M).group(1)
    blob = json.dumps(data, separators=(",", ":"))
    page = template.replace("/*CITY_DATA*/null", blob)
    found = leaks(page, real, data)
    if found:
        sys.exit(f"refusing to write the page: {len(found)} real names would leak, e.g. {found[:3]}")
    sprites = kenney_sprites(template, pack_dir)
    page = page.replace("/*KENNEY_SPRITES*/null", json.dumps(sprites, separators=(",", ":")))
    os.makedirs(out_dir, exist_ok=True)
    out = os.path.join(out_dir, "city.html")
    with open(out, "w") as f:
        f.write(page)
    print(f"{out}: {len(data['events'])} events, {len(data['ns'])} namespaces, "
          f"{len(data['wl'])} workloads, {len(sprites or {})} sprites, {len(page) // 1024} KiB")


if __name__ == "__main__":
    main()
