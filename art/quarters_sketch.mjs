#!/usr/bin/env node
// Sketch for #47: the city as quarters of related namespaces, each namespace
// a neighbourhood laid out on its own by the page's planCity(), neighbourhoods
// 2 tiles apart and quarters 4 tiles apart. Prints a map and the land per
// building against today's layout. Sizes only, no names.
//
//   node art/quarters_sketch.mjs recordings/big-cluster.jsonl [gap] [--live] [--zone-loners] [--html]
//
// --zone-loners puts the namespaces that are on their own into one quarter
// per zone (2 tiles apart) instead of a quarter each. --html also writes an
// isometric render to art/out/quarters.html (or quarters-zone-loners.html).
//
// Map: a lot (l centre, i industry, u suburb), p courtyard, g park, r road,
// s sidewalk, : the gap between neighbourhoods, # the belt between quarters,
// . grass. Digits in the margin number the quarters.

import { readFileSync } from "node:fs";

const args = process.argv.slice(2), live = args.includes("--live");
const html = args.includes("--html"), zoneLoners = args.includes("--zone-loners");
const [file, gapArg] = args.filter(a => !a.startsWith("--"));
if (!file) { console.error("usage: node art/quarters_sketch.mjs RECORDING.jsonl [gap] [--live]"); process.exit(2); }
const gap = Number(gapArg || 1);
const NEIGHBOURS = 2, BELT = 4;   // decided in #47 (2A)

const RENDER = readFileSync(new URL("quarters_render.html", import.meta.url), "utf8");
const page = readFileSync(new URL("city.template.html", import.meta.url), "utf8");
const MARK = "// ---------------------------------------------------------------- ";
const from = page.indexOf(`${MARK}layout\n`), to = page.indexOf(MARK, from + MARK.length);
const hashFn = page.slice(page.indexOf("function hash("), page.indexOf("function rng("));
const { planCity, roadTiles, lotsFor, zoneOf } = new Function(`${hashFn}${page.slice(from, to)}\nreturn { planCity, roadTiles, lotsFor, zoneOf };`)();

const workloads = new Map(), daemons = new Set();
for (const line of readFileSync(file, "utf8").split("\n")) {
  if (!line.trim()) continue;
  const p = JSON.parse(line).event.PodUpserted;
  if (!p) continue;
  if (!workloads.has(p.namespace)) workloads.set(p.namespace, new Set());
  workloads.get(p.namespace).add(`${p.kind}/${p.workload}`);
  if (p.kind === "DaemonSet") daemons.add(p.namespace);
}
const names = [...workloads.keys()];
const sets = names.map(n => workloads.get(n));
const sizes = sets.map(s => s.size);
const zones = names.map(n => zoneOf(n, daemons.has(n)));
const ZONE_ORDER = { industry: 0, centre: 1, suburb: 2 };

// Quarters (1B): two namespaces are related when they share at least half of
// each one's workloads; quarters are the groups that relation links.
const top = names.map((_, k) => k), find = k => (top[k] === k ? k : (top[k] = find(top[k])));
for (let a = 0; a < names.length; a++) for (let b = a + 1; b < names.length; b++) {
  let shared = 0;
  for (const w of sets[a]) if (sets[b].has(w)) shared++;
  if (2 * shared >= sets[a].size && 2 * shared >= sets[b].size) top[find(a)] = find(b);
}
const byRoot = new Map();
names.forEach((_, k) => { const r = find(k); if (!byRoot.has(r)) byRoot.set(r, []); byRoot.get(r).push(k); });
const all = [...byRoot.values()], quarters = zoneLoners ? all.filter(q => q.length > 1) : all;
if (zoneLoners) for (const z of ["industry", "centre", "suburb"]) {
  const loners = all.filter(q => q.length === 1 && zones[q[0]] === z).flat();
  if (loners.length) quarters.push(loners);
}

// A neighbourhood: one namespace laid out as a town of its own.
function neighbourhood(k) {
  const city = planCity([{ name: `ns-${k}`, n: lotsFor(sizes[k], live ? 0.3 : 0), zone: zones[k] }], gap);
  const at = new Map();
  for (const [t, main] of roadTiles({ ...city, gap })) at.set(t, main ? "R" : "r");
  const d = city.districts[0];
  for (const b of d.blocks) for (let j = b.y; j < b.y + b.h; j++) for (let i = b.x; i < b.x + b.w; i++) at.set(`${i}/${j}`, "s");
  d.cells.forEach(([i, j], lot) => at.set(`${i}/${j}`, lot < sizes[k] ? { centre: "l", industry: "i", suburb: "u" }[zones[k]] : "p"));
  for (const p of city.parks) for (let j = p.y; j < p.y + p.h; j++) for (let i = p.x; i < p.x + p.w; i++) at.set(`${i}/${j}`, "g");
  // Trim to the tiles used (roads may run on into the margin).
  const ks = [...at.keys()].map(t => t.split("/").map(Number)).filter(([i, j]) => i >= 0 && j >= 0 && i < city.W && j < city.H);
  return { w: city.W, h: city.H, at, weight: city.W * city.H, tiles: ks.length };
}

// Rectangles packed bottom-left on a skyline, biggest first (in the order
// given by key), for every width from the widest rectangle up; the squarest
// small result wins. space is the gap kept on the right and below each one.
function pack(items, space, key = () => 0) {
  const order = [...items].sort((a, b) => key(a) - key(b) || b.h * b.w - a.h * a.w);
  const minW = Math.max(...items.map(r => r.w)), total = items.reduce((s, r) => s + (r.w + space) * (r.h + space), 0);
  let best = null;
  for (let W = minW; W <= Math.max(minW, Math.ceil(2 * Math.sqrt(total))) + space; W++) {
    const sky = new Array(W + space).fill(0), place = [];
    for (const r of order) {
      let bx = 0, by = Infinity;
      for (let x = 0; x + r.w <= W; x++) {
        let y = 0; for (let i = x; i < x + r.w + space && i < sky.length; i++) y = Math.max(y, sky[i]);
        if (y < by) { by = y; bx = x; }
      }
      for (let i = bx; i < bx + r.w + space && i < sky.length; i++) sky[i] = by + r.h + space;
      place.push([r, bx, by]);
    }
    const w = Math.max(...place.map(([r, x]) => x + r.w)), h = Math.max(...place.map(([r, , y]) => y + r.h));
    const cost = w * h * Math.max(w / h, h / w) ** 0.5;
    if (!best || cost < best.cost) best = { w, h, place, cost };
  }
  return best;
}

const t0 = performance.now();
const qs = quarters.map((ks, q) => {
  const p = pack(ks.map(neighbourhood), NEIGHBOURS);
  return { w: p.w, h: p.h, weight: p.w * p.h, ks, q, place: p.place, zone: Math.min(...ks.map(k => ZONE_ORDER[zones[k]])) };
});
// Quarters of one zone next to each other: placed in zone order.
const packed = pack(qs, BELT, q => q.zone);
const city = { w: packed.w, h: packed.h, place: [[{ place: packed.place }, 0, 0]] };
const ms = performance.now() - t0;

// Draw.
const at = new Map(), qAt = new Map();
for (const [zp, zx, zy] of city.place) for (const [q, qx, qy] of zp.place) {
  for (let j = 0; j < q.h; j++) for (let i = 0; i < q.w; i++) qAt.set(`${zx + qx + i}/${zy + qy + j}`, q.q);
  for (const [n, nx, ny] of q.place) for (const [t, c] of n.at) {
    const [i, j] = t.split("/").map(Number);
    if (i >= 0 && j >= 0 && i < n.w && j < n.h) at.set(`${zx + qx + nx + i}/${zy + qy + ny + j}`, c);
  }
}
const rows = [];
for (let j = 0; j < city.h; j++) {
  let row = "";
  for (let i = 0; i < city.w; i++) { const t = `${i}/${j}`; row += at.get(t) || (qAt.has(t) ? ":" : "#"); }
  rows.push(row);
  if (!html) console.log(row);
}

const buildings = sizes.reduce((s, n) => s + n, 0);
const used = [...at.keys()].length;
const today = planCity(names.map((_, k) => ({ name: `ns-${k}`, n: lotsFor(sizes[k], live ? 0.3 : 0), zone: zones[k] })), gap);
let todayUsed = 0;
{ const road = roadTiles({ ...today, gap });
  const s = new Set(road.keys());
  for (const d of today.districts) for (const b of d.blocks) for (let j = b.y; j < b.y + b.h; j++) for (let i = b.x; i < b.x + b.w; i++) s.add(`${i}/${j}`);
  for (const p of today.parks) for (let j = p.y; j < p.y + p.h; j++) for (let i = p.x; i < p.x + p.w; i++) s.add(`${i}/${j}`);
  todayUsed = [...s].filter(t => { const [i, j] = t.split("/").map(Number); return i >= 0 && j >= 0 && i < today.W && j < today.H; }).length; }
console.log();
console.log(`${names.length} neighbourhoods in ${quarters.length} quarters, ${buildings} buildings; planned in ${ms.toFixed(1)} ms`);
console.log(`quarters: ${qs.filter(q => q.ks.length > 1).map(q => `${q.ks.length} ns (${q.ks.reduce((s, k) => s + sizes[k], 0)} buildings)`).join(", ")}; ${qs.filter(q => q.ks.length === 1).length} on their own`);
console.log(`city ${city.w} × ${city.h}, today ${today.W} × ${today.H}`);
console.log(`rectangle per building: ${(city.w * city.h / buildings).toFixed(2)} (today ${(today.W * today.H / buildings).toFixed(2)}, ${(city.w * city.h / (today.W * today.H)).toFixed(2)} ×)`);
console.log(`built land per building (gaps and belts not counted): ${(used / buildings).toFixed(2)} (today ${(todayUsed / buildings).toFixed(2)})`);

if (html) {
  const { mkdirSync, writeFileSync } = await import("node:fs");
  const out = new URL(`out/quarters${zoneLoners ? "-zone-loners" : ""}.html`, import.meta.url);
  mkdirSync(new URL("out/", import.meta.url), { recursive: true });
  const title = `#47 sketch: ${zoneLoners ? "loners of a zone as one quarter" : "every loner its own quarter"}, ${(city.w * city.h / (today.W * today.H)).toFixed(2)} × today's land`;
  writeFileSync(out, RENDER.replace("/*ROWS*/null", JSON.stringify(rows)).replace(/TITLE/g, title));
  console.log(`wrote ${out.pathname}`);
}
