#!/usr/bin/env node
// Checks the power stations (#96) with the page's own code: a station stands
// while its node exists or a pod runs on it, has one to three towers by CPU,
// goes dark without steam when NotReady, smokes under pressure and shows a
// barrier when cordoned; a building is in a blackout when all its pods that
// run (or are meant to) are on dark nodes, partly when only some are, and a
// node going dark or coming back marks those buildings for redrawing.
// Exits 1 on a failure.
//
//   node art/nodes_check.mjs

import { readFileSync } from "node:fs";

const page = readFileSync(new URL("city.template.html", import.meta.url), "utf8");
const from = page.indexOf("const exposures = new Map();");
const to = page.indexOf("// ---------------------------------------------------------------- scene");
const L = new Function(`
const [PENDING, RUNNING, CRASH, TERMINATING, SUCCEEDED, FAILED] = [0, 1, 2, 3, 4, 5];
const KINDS = ["Deployment", "StatefulSet", "DaemonSet", "Job", "Standalone", "Node"], OTHER = 1, POWER = "power grid";
const DATA = { ns: [], wl: [] }, world = { requests: false, pods: new Map(), buildings: new Map() };
const live = { ns: new Map(), wl: new Map() }, dirty = new Set(), registered = [];
const storeysOf = () => 2, weightOfBuilding = () => 0, registerBatch = events => registered.push(...events);
function intern(map, list, key, name) {
  let i = map.get(key);
  if (i === undefined) { i = list.length; list.push(name); map.set(key, i); }
  return i;
}
class Building {
  constructor(ns, wl, kind) { this.ns = ns; this.wl = wl; this.kind = kind; this.name = DATA.wl[wl]; this.key = ns + "/" + wl; this.pods = new Map(); this.variant = { h: 7, floors: 3 }; }
}
${page.slice(from, to)}
return { lookOf, stationOf, setNode, nodeGone, setPodNode, registerStations, nodes, world, dirty, registered, DATA, Building,
  S: { PENDING, RUNNING, CRASH, TERMINATING, SUCCEEDED, FAILED } };`)();
const { RUNNING, PENDING, SUCCEEDED, TERMINATING } = L.S;

let failures = 0;
const check = (what, got, want) => {
  if (JSON.stringify(got) !== JSON.stringify(want)) { failures++; console.log(`FAIL ${what}: ${JSON.stringify(got)}, not ${JSON.stringify(want)}`); }
};
const node = (name, more = {}) => ({ name, ready: true, ...more });
const station = name => L.nodes.get(name) && L.nodes.get(name).b;
const stationText = name => {
  const b = station(name), look = b && L.lookOf(b);
  if (!look) return "gone";
  return [`${look.p.towers} towers`, look.dark ? "dark" : "", look.steam ? (look.smoke ? "smoke" : "steam") : "", look.p.cordoned ? "barrier" : ""].filter(Boolean).join(" ");
};
// A workload with pods: [id, state, node].
const pods = new Map();
function workload(name, list) {
  const b = new L.Building(0, L.DATA.wl.push(name) - 1, "Deployment");
  for (const [id, state, on] of list) {
    b.pods.set(id, state);
    L.world.pods.set(id, { id, b, state, cpu: 0, memory: 0 });
    L.setPodNode(id, on);
  }
  pods.set(name, b);
  return b;
}
const dark = b => L.lookOf(b).dark;

L.registerStations(["b-node", "a-node"].sort());
check("new stations get lots in name order", L.registered.map(e => L.DATA.wl[e[3]]), ["a-node", "b-node"]);

// Known from its pods only (no access to Nodes, an old recording).
const web = workload("web", [[1, RUNNING, "n1"], [2, RUNNING, "n2"], [3, SUCCEEDED, "n3"]]);
check("a station from its pods", stationText("n1"), "1 towers steam");
check("no blackout without Node details", dark(web), 0);

L.setNode(node("n1", { cpu: 16000 }));
L.setNode(node("n2", { cpu: 64000 }));
L.setNode(node("n3", { cpu: 4000 }));
check("towers by CPU", [stationText("n1"), stationText("n2"), stationText("n3")], ["2 towers steam", "3 towers steam", "1 towers steam"]);

L.dirty.clear();
L.setNode(node("n1", { ready: false, cpu: 16000 }));
check("a node going dark marks its pods' buildings", [...L.dirty].map(b => b.name).sort(), ["n1", "web"]);
check("a dark station", stationText("n1"), "2 towers dark");
check("some pods on a dark node: a partial blackout", dark(web), 1);
L.setNode(node("n2", { ready: false, cpu: 64000 }));
check("every running pod on dark nodes: a blackout", dark(web), 2);
L.setNode(node("n3", { ready: false, cpu: 4000 }));
check("finished pods do not count", dark(web), 2);

const db = workload("db", [[4, TERMINATING, "n1"], [5, PENDING, "n4"]]);
check("a pending pod on a ready node keeps the lights on", dark(db), 0);

L.dirty.clear();
L.setNode(node("n1", { cpu: 16000 }));
check("coming back marks them again", [...L.dirty].map(b => b.name).sort(), ["db", "n1", "web"]);
check("back to partial", dark(web), 1);

L.setNode(node("n4", { pressure: ["MemoryPressure"], cordoned: true }));
check("pressure and cordoned", stationText("n4"), "1 towers smoke barrier");
L.setNode(node("n4", { ready: false, pressure: ["MemoryPressure"] }));
check("a dark node does not smoke", stationText("n4"), "1 towers dark");

L.nodeGone("n4");
check("a deleted node with a pod still on it stays", stationText("n4"), "1 towers steam");
L.setPodNode(5, null);
check("and goes with its last pod", stationText("n4"), "gone");
L.setNode(node("n5"));
check("an empty node has a station", stationText("n5"), "1 towers steam");
L.nodeGone("n5");
check("until it is deleted", stationText("n5"), "gone");

console.log(failures ? `${failures} failures` : "all checks pass");
process.exit(failures ? 1 : 0);
