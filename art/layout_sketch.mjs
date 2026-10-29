#!/usr/bin/env node
// Lays out a recorded cluster with the page's own layout code and prints the
// city as a map of tiles, with a few numbers to compare layouts by. Only the
// sizes and zones of namespaces are used, so the output holds no names.
//
//   node art/layout_sketch.mjs recordings/big-cluster.jsonl [gap] [--live]
//
// gap is the road width: 1 for the Kenney look (the default), 2 for Metropolis.
// --live keeps spare lots for new workloads, as a live city does.
//
// Map: R main road, r road, + crossroads, ~ river, = bridge, s sidewalk, a lot in the town
// centre (l), the industrial quarter (i) or a suburb (u), p courtyard (a lot
// left over), g pocket park, . grass (the margin and the strips the edge gave
// back).

import { readFileSync } from "node:fs";

const args = process.argv.slice(2), live = args.includes("--live");
const [file, gapArg] = args.filter(a => a !== "--live");
if (!file) { console.error("usage: node art/layout_sketch.mjs RECORDING.jsonl [gap] [--live]"); process.exit(2); }
const gap = Number(gapArg || 1);

// The page's layout section, run as it is.
const page = readFileSync(new URL("city.template.html", import.meta.url), "utf8");
const MARK = "// ---------------------------------------------------------------- ";
const from = page.indexOf(`${MARK}layout\n`), to = page.indexOf(MARK, from + MARK.length);
const hashFn = page.slice(page.indexOf("function hash("), page.indexOf("function rng("));
const { planCity, roadTiles, inRiver, lotsFor, zoneOf, MARGIN } = new Function(`${hashFn}${page.slice(from, to)}\nreturn { planCity, roadTiles, inRiver, lotsFor, zoneOf, MARGIN };`)();

// Workloads per namespace, as the page counts them: one building per workload.
const workloads = new Map(), daemons = new Set();
for (const line of readFileSync(file, "utf8").split("\n")) {
  if (!line.trim()) continue;
  const p = JSON.parse(line).event.PodUpserted;
  if (!p) continue;
  if (!workloads.has(p.namespace)) workloads.set(p.namespace, new Set());
  workloads.get(p.namespace).add(`${p.kind}/${p.workload}`);
  if (p.kind === "DaemonSet") daemons.add(p.namespace);
}
const sizes = [...workloads.values()].map(s => s.size);
const zones = [...workloads.keys()].map(name => zoneOf(name, daemons.has(name)));
const needs = sizes.map((n, k) => ({ name: `ns-${String(k).padStart(3, "0")}`, n: lotsFor(n, live ? 0.3 : 0), zone: zones[k] }));

const t0 = performance.now();
const city = planCity(needs, gap);
const ms = performance.now() - t0;
const plan = { ...city, gap };

// Tiles.
const road = roadTiles(plan);
const at = new Map();
for (const [k, main] of road) at.set(k, main ? "R" : "r");
for (let j = -MARGIN; j < city.H + MARGIN; j++) for (let i = -MARGIN; i < city.W + MARGIN; i++)
  if (inRiver(city.river, i, j)) at.set(`${i}/${j}`, road.has(`${i}/${j}`) ? "=" : "~");
city.districts.forEach((d, k) => {
  for (const b of d.blocks) for (let j = b.y; j < b.y + b.h; j++) for (let i = b.x; i < b.x + b.w; i++) at.set(`${i}/${j}`, "s");
  d.cells.forEach(([i, j], lot) => at.set(`${i}/${j}`, lot < sizes[k] ? { centre: "l", industry: "i", suburb: "u" }[zones[k]] : "p"));
});
for (const p of city.parks) for (let j = p.y; j < p.y + p.h; j++) for (let i = p.x; i < p.x + p.w; i++) at.set(`${i}/${j}`, "g");

// Junctions by how many road tiles a road tile joins; the longest straight
// run of road.
const isRoad = (i, j) => road.has(`${i}/${j}`);
const count = { t: 0, cross: 0, dead: 0 };
for (const k of road.keys()) {
  const [i, j] = k.split("/").map(Number);
  const n = [isRoad(i, j - 1), isRoad(i + 1, j), isRoad(i, j + 1), isRoad(i - 1, j)].filter(Boolean).length;
  if (gap === 1) { if (n === 3) count.t++; if (n === 4) count.cross++; if (n === 1) count.dead++; }
}
const inner = city.roads;
const crossroads = new Set([...road.keys()].filter(k => {
  const [i, j] = k.split("/").map(Number);
  return gap === 1 && isRoad(i, j - 1) && isRoad(i + 1, j) && isRoad(i, j + 1) && isRoad(i - 1, j);
}));

const lo = -MARGIN, hiI = city.W + MARGIN, hiJ = city.H + MARGIN;
for (let j = lo; j < hiJ; j++) {
  let row = "";
  for (let i = lo; i < hiI; i++) row += crossroads.has(`${i}/${j}`) ? "+" : at.get(`${i}/${j}`) || ".";
  console.log(row);
}

// The road network in pieces: a road joins another where one of its ends
// lies on the other (as the cars see it).
const on = (p, r) => p[0] >= r.i0 && p[0] <= r.i1 && p[1] >= r.j0 && p[1] <= r.j1;
const piece = inner.map((r, k) => k);
const top = k => (piece[k] === k ? k : (piece[k] = top(piece[k])));
inner.forEach((r, a) => inner.forEach((s, b) => {
  if (a !== b && (on([r.i0, r.j0], s) || on([r.i1, r.j1], s))) piece[top(a)] = top(b);
}));
const pieces = new Set(inner.map((r, k) => top(k))).size;

const longest = Math.max(0, ...inner.map(r => Math.abs(r.i1 - r.i0) + Math.abs(r.j1 - r.j0) - gap));
const buildings = sizes.reduce((s, n) => s + n, 0);
// The town's land: every tile inside the town that is not grass.
let town = 0;
for (let j = 0; j < city.H; j++) for (let i = 0; i < city.W; i++) if (at.has(`${i}/${j}`)) town++;
console.log();
console.log(`${needs.length} districts, ${buildings} buildings; town ${city.W} × ${city.H}; planned in ${ms.toFixed(1)} ms`);
const zoneCount = z => `${zones.filter(x => x === z).length} (${sizes.filter((n, k) => zones[k] === z).reduce((s, n) => s + n, 0)} buildings)`;
console.log(`zones: centre ${zoneCount("centre")}, industry ${zoneCount("industry")}, suburbs ${zoneCount("suburb")}`);
console.log(`tiles per building: town ${(town / buildings).toFixed(2)}, its rectangle ${(city.W * city.H / buildings).toFixed(2)}`);
const blocks = city.districts.reduce((s, d) => s + d.blocks.length, 0), lots = city.districts.reduce((s, d) => s + d.cells.length, 0);
const bridges = inner.filter(r => r.bridge).length;
console.log(`roads: ${inner.length - bridges} cuts (${inner.filter(r => r.main && !r.bridge).length} main), longest ${longest} tiles, ${pieces} piece${pieces === 1 ? "" : "s"}; pocket parks: ${city.parks.length}`);
console.log(`river: ${city.river ? `${Math.max(city.river.w, city.river.h)} tiles long, ${bridges} bridge${bridges === 1 ? "" : "s"}` : "none"}`);
console.log(`blocks: ${blocks} (${city.districts.filter(d => d.blocks.length > 1).length} quarters); lots: ${lots}, ${(100 * buildings / lots).toFixed(0)} % built on`);
if (gap === 1) console.log(`junctions: ${count.t} T, ${count.cross} crossroads, ${count.dead} dead ends`);
