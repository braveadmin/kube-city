#!/usr/bin/env node
// Checks planTown on random clusters: each grown one namespace at a time and
// then one namespace after another growing by half, in both road widths,
// with and without spare lots. Prints what failed and a summary; exits 1 on
// a failure.
//
//   node art/town_check.mjs [clusters] [seed]
//
// Checked on every plan: the road network is one piece; neighbourhoods keep
// their gaps (NEIGHBOURS in a quarter, BELT between quarters); no road on a
// block and every lot inside its blocks; nothing of the town on the river's
// water but bridges, which cross it straight on a run; the water is one
// piece; the river's runs are long enough and its bends wide enough; no
// neighbourhood moved for another one (a plan that kept the town; one that
// left its quarter may move, as it needs a belt round it now); a small
// town keeps to one bank on a fresh pack.

import { readFileSync } from "node:fs";

const [countArg, seedArg] = process.argv.slice(2);
const COUNT = Number(countArg || 250);
let seed = Number(seedArg || 1);
const rand = () => ((seed = (seed * 1103515245 + 12345) % 2147483648) / 2147483648);
const pick = n => Math.floor(rand() * n);

const page = readFileSync(new URL("city.template.html", import.meta.url), "utf8");
const MARK = "// ---------------------------------------------------------------- ";
const from = page.indexOf(`${MARK}layout\n`), to = page.indexOf(MARK, from + MARK.length);
const hashFn = page.slice(page.indexOf("function hash("), page.indexOf("function rng("));
const L = new Function(`${hashFn}${page.slice(from, to)}\nreturn { planTown, roadTiles, inRiver, lotsFor, MARGIN, NEIGHBOURS, BELT, RIVER, SMALL_TOWN };`)();
const { planTown, roadTiles, inRiver, lotsFor, NEIGHBOURS, BELT, RIVER, SMALL_TOWN } = L;

// A random cluster: up to three stacks shared by branch namespaces, the rest
// on their own.
function cluster() {
  const n = 1 + pick(40), stacks = Array.from({ length: pick(4) }, (_, s) => Array.from({ length: 5 + pick(80) }, (_, k) => `s${s}w${k}`));
  const zones = ["centre", "industry", "suburb"];
  return Array.from({ length: n }, (_, k) => {
    let wl;
    if (stacks.length && rand() < 0.6) {
      const st = stacks[pick(stacks.length)];
      wl = st.filter(() => rand() < 0.85);
      if (!wl.length) wl = [st[0]];
    } else wl = Array.from({ length: 1 + pick(rand() < 0.2 ? 60 : 15) }, (_, w) => `n${k}w${w}`);
    return { name: `ns-${k}`, wl, zone: zones[pick(3)] };
  });
}

const fails = new Map(), counts = { plans: 0, fresh: 0, keptRiver: 0, freshWithWas: 0, keptDown: 0, leftQuarter: 0, bends: 0, bridges: 0, small: 0 };
let where = null;
const fail = (what, info) => { if (!fails.has(what)) fails.set(what, { n: 0, info: { ...info, ...where } }); fails.get(what).n++; };

function check(city, needs, gap, was) {
  counts.plans++;
  const plan = { ...city, gap };
  const road = roadTiles(plan);
  const half = gap / 2, d = RIVER / 2 + half, h = (2 + 2 * gap + RIVER) / 2;
  // Roads in one piece.
  const on = (p, r) => p[0] >= r.i0 && p[0] <= r.i1 && p[1] >= r.j0 && p[1] <= r.j1;
  const piece = city.roads.map((r, k) => k);
  const top = k => (piece[k] === k ? k : (piece[k] = top(piece[k])));
  city.roads.forEach((r, a) => city.roads.forEach((s, b) => { if (a !== b && (on([r.i0, r.j0], s) || on([r.i1, r.j1], s))) piece[top(a)] = top(b); }));
  if (city.roads.length && new Set(city.roads.map((r, k) => top(k))).size > 1) fail("roads in more than one piece");
  if (needs.length < 2) return;
  // Gaps between neighbourhoods.
  const qOf = new Map(city.quarters.flatMap(q => q.hoods.map(k => [k, q.q])));
  city.districts.forEach((a, x) => city.districts.forEach((b, y) => {
    if (x >= y) return;
    const s = qOf.get(x) === qOf.get(y) ? NEIGHBOURS : BELT;
    for (const p of a.blocks) for (const r of b.blocks) if (p.x < r.x + r.w + s && r.x < p.x + p.w + s && p.y < r.y + r.h + s && r.y < p.y + p.h + s) fail("neighbourhoods closer than their gap");
  }));
  // Blocks: no road, no water; lots inside.
  const block = new Set();
  for (const dd of city.districts) for (const b of dd.blocks) for (let j = b.y; j < b.y + b.h; j++) for (let i = b.x; i < b.x + b.w; i++) {
    block.add(`${i}/${j}`);
    if (road.has(`${i}/${j}`)) fail("road on a block");
    if (inRiver(city.river, i, j)) fail("block on the river");
  }
  for (const dd of city.districts) for (const [i, j] of dd.cells) if (!block.has(`${i}/${j}`)) fail("lot outside its blocks");
  for (const p of city.parks) for (let j = p.y; j < p.y + p.h; j++) for (let i = p.x; i < p.x + p.w; i++) if (inRiver(city.river, i, j)) fail("park on the river");
  // The river.
  const r = city.river;
  if (!r) { fail("no river"); return; }
  const water = new Set();
  for (const p of r.parts) for (let j = p.y; j < p.y + p.h; j++) for (let i = p.x; i < p.x + p.w; i++) water.add(`${i}/${j}`);
  const seen = new Set([water.values().next().value]), queue = [...seen];
  while (queue.length) {
    const [i, j] = queue.pop().split("/").map(Number);
    for (const n of [`${i + 1}/${j}`, `${i - 1}/${j}`, `${i}/${j + 1}`, `${i}/${j - 1}`]) if (water.has(n) && !seen.has(n)) { seen.add(n); queue.push(n); }
  }
  if (seen.size !== water.size) fail("river's water in pieces");
  const runs = r.runs, Lr = r.down ? city.H : city.W;
  counts.bends += runs.length - 1;
  runs.forEach((q, k) => {
    if (k && k < runs.length - 1 && q.a1 - q.a0 < 2 * h + 2) fail("a run of the river too short", { len: q.a1 - q.a0 });
    if (k && Math.abs(q.c - runs[k - 1].c) < RIVER + 1) fail("a bend too narrow");
    if (k < runs.length - 1 && (q.a1 <= 0 || q.a1 >= Lr)) fail("a bend outside the town");
  });
  // Road tiles on water: only bridges, across a run.
  const bridges = city.roads.filter(x => x.bridge);
  counts.bridges += bridges.length;
  if (!bridges.length) fail("no bridge");
  const bridgeTiles = roadTiles({ roads: bridges, gap });
  for (const t of road.keys()) if (water.has(t) && !bridgeTiles.has(t)) fail("road on the water but not a bridge");
  for (const b of bridges) {
    const u = r.down ? b.j0 : b.i0, [v0, v1] = r.down ? [b.i0, b.i1] : [b.j0, b.j1];
    const run = runs.find(q => u >= q.a0 + d && u <= q.a1 - d);
    if (!run || v0 !== run.c - d || v1 !== run.c + d || (r.down ? b.j0 !== b.j1 : b.i0 !== b.i1)) fail("bridge not straight across a run");
  }
  // A fresh pack of a small town keeps to one bank.
  if (!city.shift) {
    counts.fresh++;
    const land = city.placed && [...city.placed.pos.values()].reduce((s, p) => s + p.w * p.h, 0);
    if (land < 2 * SMALL_TOWN) {
      counts.small++;
      const side = p => ((r.down ? p.x : p.y) < (runs.find(q => (r.down ? p.y : p.x) < q.a1) || runs[runs.length - 1]).c ? 0 : 1);
      if (new Set([...city.placed.pos.values()].map(side)).size > 1) fail("a small town on both banks");
    }
    if (was) {
      counts.freshWithWas++;
      if (was.down === r.down) counts.keptDown++;
      if (was.down === r.down && was.runs[0].c === runs[0].c && was.runs[was.runs.length - 1].c === runs[runs.length - 1].c) counts.keptRiver++;
    }
  } else if (was) {
    // Nobody moved: every neighbourhood of the same size where it was.
    for (const [name, p] of city.placed.pos) {
      const o = was.pos.get(name);
      if (o && o.w === p.w && o.h === p.h && (o.x + city.shift[0] !== p.x || o.y + city.shift[1] !== p.y)) {
        // One that left its quarter needs a belt round it now, not a gap.
        if (o.q !== p.q) counts.leftQuarter++; else fail("a neighbourhood moved", { name });
      }
    }
    if (was.runs.length !== runs.length || was.runs.some((q, k) => q.c + (r.down ? city.shift[0] : city.shift[1]) !== runs[k].c)) fail("the river moved in a kept town");
  }
}

const t0 = performance.now();
for (let c = 0; c < COUNT; c++) {
  const base = cluster(), gap = 1 + (c % 2), spare = c % 4 >= 2 ? 0.3 : 0;
  const needsOf = list => list.map(x => ({ name: x.name, n: lotsFor(x.wl.length, spare), zone: x.zone, wl: x.wl }));
  let was = null;
  const grown = [];
  for (const x of base) {
    grown.push(x);
    where = { cluster: c, plan: grown.length };
    const needs = needsOf(grown);
    const city = planTown(needs, gap, was);
    check(city, needs, gap, was);
    was = city.placed;
  }
  for (let k = 0; k < grown.length; k++) {
    where = { cluster: c, grows: k };
    grown[k] = { ...grown[k], wl: [...grown[k].wl, ...grown[k].wl.slice(0, Math.ceil(grown[k].wl.length / 2)).map(w => `${w}+`)] };
    const needs = needsOf(grown);
    const city = planTown(needs, gap, was);
    check(city, needs, gap, was);
    was = city.placed;
  }
}
const ms = performance.now() - t0;
console.log(`${COUNT} clusters, ${counts.plans} plans (${(ms / counts.plans).toFixed(0)} ms each); fresh packs ${counts.fresh} (${counts.small} of small towns), ${counts.bends} bends, ${counts.bridges} bridges`);
console.log(`neighbourhoods that left their quarter and moved: ${counts.leftQuarter}`);
console.log(`a fresh pack after another plan kept the river's direction ${counts.keptDown} times of ${counts.freshWithWas}, its way in and out too ${counts.keptRiver} times`);
for (const [what, { n, info }] of fails) console.log(`FAIL ${what}: ${n} (e.g. ${JSON.stringify(info)})`);
if (!fails.size) console.log("all checks pass");
process.exit(fails.size ? 1 : 0);
