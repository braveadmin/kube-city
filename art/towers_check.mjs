#!/usr/bin/env node
// Checks the towers (#49) on random clusters: heights on their fixed scale,
// and where the heaviest stand. Prints what failed and a summary; exits 1 on
// a failure.
//
//   node art/towers_check.mjs [clusters] [seed]
//
// Heights: storeys from weight only (so other workloads never change them),
// growing with weight, LOW_STOREYS up to LIGHT, at most MAX_STOREYS;
// terminating and finished pods do not count; memory counts a core per 4 GiB.
// Lots: in every neighbourhood of a random town each lot stays in the block
// zonedLots gave it (#48's role zoning), no two workloads share a lot, and
// in each block a heavier workload never stands on a ring further out than a
// lighter one.

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
const L = new Function(`${hashFn}${page.slice(from, to)}\nreturn { planTown, lotsFor, zonedLots, frontOf, heaviestInside };`)();
const heights = page.slice(page.indexOf("const GIB_PER_CORE"), page.indexOf("function weightOfBuilding("));
const H = new Function(`const [PENDING, RUNNING, CRASH, TERMINATING, SUCCEEDED, FAILED] = [0, 1, 2, 3, 4, 5];
${heights}\nreturn { weightOf, storeysOf, LIGHT, LOW_STOREYS, MAX_STOREYS, GIB_PER_CORE };`)();

const failures = [];
const fail = text => { if (failures.length < 20) console.log(`FAIL ${text}`); failures.push(text); };

// Heights.
const { weightOf, storeysOf, LIGHT, LOW_STOREYS, MAX_STOREYS, GIB_PER_CORE } = H;
const scale = [[0, 2], [LIGHT, 2], [0.2, 4], [1, 9], [3.2, 12], [100, 12]];
for (const [w, want] of scale) if (storeysOf(w) !== want) fail(`storeysOf(${w}) = ${storeysOf(w)}, not ${want}`);
for (let w = 0, last = 0; w < 20; w += 0.01) {
  const s = storeysOf(w);
  if (s < last || s < LOW_STOREYS || s > MAX_STOREYS) fail(`storeysOf(${w}) = ${s} after ${last}`);
  last = s;
}
if (weightOf([[1, 500, 0], [0, 500, 0], [3, 4000, 0], [4, 4000, 0], [5, 4000, 0]]) !== 1) fail("terminating or finished pods counted");
if (weightOf([[1, 100, 8 * GIB_PER_CORE / 4]]) !== 2) fail("8 GiB is not 2 cores' worth");
// One building's pods never see the others', so adding workloads cannot move
// its height; what can is its own pods, checked here: more replicas, taller.
const one = [1, 250, 2 ** 29];
if (!(storeysOf(weightOf([one, one, one])) > storeysOf(weightOf([one])))) fail("replicas do not make a building taller");

// Lots.
let districts = 0, blocks = 0, moved = 0;
for (let c = 0; c < COUNT; c++) {
  const n = 1 + pick(12);
  const needs = Array.from({ length: n }, (_, k) => {
    const wl = Array.from({ length: 1 + pick(rand() < 0.3 ? 120 : 20) }, (_, w) => `n${k}w${w}`);
    return { name: `ns-${k}`, n: L.lotsFor(wl.length, rand() < 0.5 ? 0.3 : 0), zone: ["centre", "industry", "suburb"][pick(3)], wl };
  });
  const city = L.planTown(needs, 1 + pick(2), null);
  needs.forEach((need, idx) => {
    const d = city.districts[idx];
    const cells = L.zonedLots(d.blocks, L.frontOf(d.blocks, [city.W / 2, city.H / 2]));
    const lots = new Map(need.wl.map((w, k) => [w, k]));
    const weights = new Map(need.wl.map(w => [w, rand() < 0.2 ? 0 : Math.round(rand() * 40) / 10]));
    const seats = L.heaviestInside(d.blocks, cells, lots, weights);
    districts++; blocks += d.blocks.length;
    const where = `cluster ${c} ${need.name}`;
    if (new Set(seats).size !== seats.length || seats.some(s => s < 0 || s >= need.wl.length)) return fail(`${where}: seats are not the lots 0..${need.wl.length - 1}`);
    const blockOf = ([i, j]) => d.blocks.findIndex(b => i > b.x && j > b.y && i < b.x + b.w - 1 && j < b.y + b.h - 1);
    const ringOf = ([i, j], b) => Math.min(i - b.x - 1, b.x + b.w - 2 - i, j - b.y - 1, b.y + b.h - 2 - j);
    const placed = need.wl.map((w, lot) => ({ w: weights.get(w), cell: cells[seats[lot]], was: cells[lot] }));
    if (placed.some(p => blockOf(p.cell) < 0)) fail(`${where}: a lot outside every block`);
    for (const p of placed) {
      if (blockOf(p.cell) !== blockOf(p.was)) fail(`${where}: a lot left its block`);
      if (p.cell !== p.was) moved++;
    }
    for (const a of placed) for (const z of placed) {
      const bk = blockOf(a.cell);
      if (bk < 0 || bk !== blockOf(z.cell) || !(a.w > z.w)) continue;
      if (ringOf(a.cell, d.blocks[bk]) < ringOf(z.cell, d.blocks[bk])) { fail(`${where}: a heavier workload stands further out`); break; }
    }
  });
}

console.log(`${COUNT} clusters, ${districts} neighbourhoods, ${blocks} blocks, ${moved} workloads moved inwards or out by weight`);
console.log(failures.length ? `${failures.length} failures` : "all checks pass");
process.exit(failures.length ? 1 : 0);
