#!/usr/bin/env node
// Lays out a recorded cluster with the page's own layout code and prints the
// city as a map of tiles, with a few numbers to compare layouts by. Only the
// sizes of namespaces are used, so the output holds no names.
//
//   node art/layout_sketch.mjs recordings/big-cluster.jsonl [gap] [--live]
//
// gap is the road width: 1 for the Kenney look (the default), 2 for Metropolis.
// --live keeps spare lots for new workloads, as a live city does.
//
// Map: R main road, r road, + crossroads, s sidewalk, l lot, p courtyard (a
// lot left over), g pocket park.

import { readFileSync } from "node:fs";

const args = process.argv.slice(2), live = args.includes("--live");
const [file, gapArg] = args.filter(a => a !== "--live");
if (!file) { console.error("usage: node art/layout_sketch.mjs RECORDING.jsonl [gap] [--live]"); process.exit(2); }
const gap = Number(gapArg || 1);

// The page's layout section, run as it is.
const page = readFileSync(new URL("city.template.html", import.meta.url), "utf8");
const MARK = "// ---------------------------------------------------------------- ";
const from = page.indexOf(`${MARK}layout\n`), to = page.indexOf(MARK, from + MARK.length);
const { planCity, roadTiles, lotsFor } = new Function(`${page.slice(from, to)}\nreturn { planCity, roadTiles, lotsFor };`)();

// Workloads per namespace, as the page counts them: one building per workload.
const workloads = new Map();
for (const line of readFileSync(file, "utf8").split("\n")) {
  if (!line.trim()) continue;
  const p = JSON.parse(line).event.PodUpserted;
  if (!p) continue;
  if (!workloads.has(p.namespace)) workloads.set(p.namespace, new Set());
  workloads.get(p.namespace).add(`${p.kind}/${p.workload}`);
}
const sizes = [...workloads.values()].map(s => s.size);
const needs = sizes.map((n, k) => ({ name: `ns-${String(k).padStart(3, "0")}`, n: lotsFor(n, live ? 0.3 : 0) }));

const t0 = performance.now();
const city = planCity(needs, gap);
const ms = performance.now() - t0;
const plan = { ...city, gap };

// Tiles, ring road included.
const road = roadTiles(plan);
const at = new Map();
for (const [k, main] of road) at.set(k, main ? "R" : "r");
city.districts.forEach((d, k) => {
  for (const b of d.blocks) for (let j = b.y; j < b.y + b.h; j++) for (let i = b.x; i < b.x + b.w; i++) at.set(`${i}/${j}`, "s");
  d.cells.forEach(([i, j], lot) => at.set(`${i}/${j}`, lot < sizes[k] ? "l" : "p"));
});
for (const p of city.parks) for (let j = p.y; j < p.y + p.h; j++) for (let i = p.x; i < p.x + p.w; i++) at.set(`${i}/${j}`, "g");

// Junctions by how many road tiles a road tile joins; the longest straight
// run of road inside the ring.
const isRoad = (i, j) => road.has(`${i}/${j}`);
const count = { t: 0, cross: 0, dead: 0 };
for (const k of road.keys()) {
  const [i, j] = k.split("/").map(Number);
  const n = [isRoad(i, j - 1), isRoad(i + 1, j), isRoad(i, j + 1), isRoad(i - 1, j)].filter(Boolean).length;
  if (gap === 1) { if (n === 3) count.t++; if (n === 4) count.cross++; if (n === 1) count.dead++; }
}
const inner = city.roads.slice(4);
const crossroads = new Set([...road.keys()].filter(k => {
  const [i, j] = k.split("/").map(Number);
  return gap === 1 && isRoad(i, j - 1) && isRoad(i + 1, j) && isRoad(i, j + 1) && isRoad(i - 1, j);
}));

const lo = -gap, hiI = city.W + gap, hiJ = city.H + gap;
for (let j = lo; j < hiJ; j++) {
  let row = "";
  for (let i = lo; i < hiI; i++) row += crossroads.has(`${i}/${j}`) ? "+" : at.get(`${i}/${j}`) || ".";
  console.log(row);
}


const longest = Math.max(0, ...inner.map(r => Math.abs(r.i1 - r.i0) + Math.abs(r.j1 - r.j0) - gap));
const buildings = sizes.reduce((s, n) => s + n, 0);
const area = (city.W + 2 * gap) * (city.H + 2 * gap);
console.log();
console.log(`${needs.length} districts, ${buildings} buildings; city ${city.W} × ${city.H} inside the ring road; planned in ${ms.toFixed(1)} ms`);
console.log(`tiles per building (ring road included): ${(area / buildings).toFixed(2)}`);
const blocks = city.districts.reduce((s, d) => s + d.blocks.length, 0), lots = city.districts.reduce((s, d) => s + d.cells.length, 0);
console.log(`roads: ${inner.length} cuts (${inner.filter(r => r.main).length} main), longest ${longest} tiles; pocket parks: ${city.parks.length}`);
console.log(`blocks: ${blocks} (${city.districts.filter(d => d.blocks.length > 1).length} quarters); lots: ${lots}, ${(100 * buildings / lots).toFixed(0)} % built on`);
if (gap === 1) console.log(`junctions: ${count.t} T, ${count.cross} crossroads, ${count.dead} dead ends`);
