#!/usr/bin/env node
// Lays out a recorded cluster with the page's own layout code, grows the
// terrain round it with the page's own terrain code and prints the checks
// from issues #39 to #43, a map of levels and `art/out/terrain.html`: the town
// and its terrain, in which the seed can be changed. Only the sizes and zones of
// namespaces are used, so neither output holds names.
//
//   node art/terrain_sketch.mjs recordings/big-cluster.jsonl [seed] [gap]
//
// gap is the road width: 1 for the Kenney look (the default), 2 for Metropolis.
//
// Map: # the town (its river too), 0–9 and a–c the ground level (the town
// stands on 1), ~ sea, = lake, . beach, F forest, f woods, : field, h farm,
// ^ rock, * snow, % river (the town's river and side rivers); meadow shows its level.

import { mkdirSync, readFileSync, writeFileSync } from "node:fs";

const [file, seed = "1", gapArg] = process.argv.slice(2);
if (!file) { console.error("usage: node art/terrain_sketch.mjs RECORDING.jsonl [seed] [gap]"); process.exit(2); }
const gap = Number(gapArg || 1);

// The page's helpers, layout and terrain sections, run as they are.
const page = readFileSync(new URL("city.template.html", import.meta.url), "utf8");
const MARK = "// ---------------------------------------------------------------- ";
const section = name => { const from = page.indexOf(`${MARK}${name}\n`); return page.slice(from, page.indexOf(MARK, from + MARK.length)); };
const helpers = page.slice(page.indexOf("const TW = "), page.indexOf("function shade("));
const polyCode = page.match(/^function poly\(.*$/m)[0];
const code = `${helpers}${polyCode}\n${section("layout")}${section("terrain")}`;
// The ground's mesh builder, without the mesh (that needs PIXI), and what
// draws the ground cover in the Metropolis look.
const block = start => page.slice(page.indexOf(start), page.indexOf("\n}", page.indexOf(start)) + 2);
const art = [page.slice(page.indexOf("function shade("), page.indexOf("const cssColor")), block("const ART = {"), page.match(/^const P = .*$/m)[0],
  page.match(/^function shadedBox\(.*$/m)[0],
  ...["class GroundPen {", "function box(", "function tree(", "function pine(", "function farm(", "function coverDetails("].map(block)].join("\n");
const { planCity, lotsFor, zoneOf, planTerrain, FRONT_LOW, FLAT, LEVELS, LAKE_MIN, HILLS, COVER, isWater, eachGroundTile, groundFaces, hiddenBy, iso, poly, GroundPen, coverTrees, coverDetails, tree, pine } =
  new Function(`${code}\n${art}\nreturn { planCity, lotsFor, zoneOf, planTerrain, FRONT_LOW, FLAT, LEVELS, LAKE_MIN, HILLS, COVER, isWater, eachGroundTile, groundFaces, hiddenBy, iso, poly, GroundPen, coverTrees, coverDetails, tree, pine };`)();

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
const needs = [...workloads].map(([name, set], k) => ({ name: `ns-${String(k).padStart(3, "0")}`, n: lotsFor(set.size, 0), zone: zoneOf(name, daemons.has(name)) }));
const city = planCity(needs, gap);
// What the terrain reads of a plan, and nothing more.
const plan = { districts: city.districts.map(d => ({ blocks: d.blocks })), parks: city.parks, roads: city.roads, river: city.river, W: city.W, H: city.H, gap };

// Timing (the first run warms up), and the same terrain twice.
const times = [];
let t;
for (let n = 0; n < 6; n++) { const t0 = performance.now(); t = planTerrain(plan, seed); times.push(performance.now() - t0); }
const again = planTerrain(plan, seed);
const same = t.level.every((v, k) => v === again.level[k]) && t.height.every((v, k) => v === again.height[k]) && t.cover.every((v, k) => v === again.cover[k]);
const other = planTerrain(plan, `${seed}x`);
const differs = t.level.some((v, k) => v !== other.level[k]);

// The map, and the checks.
const { W, H, level, toTown } = t;
const isTown = k => toTown[k] === 0;
let flatOk = true, front = 0, hist = new Array(13).fill(0), high = 0, highBack = 0;
const townRows = new Map();   // screen column -> the town's front row (largest i + j)
for (let k = 0; k < W * H; k++) if (isTown(k)) { const c = k % W - ((k / W) | 0); townRows.set(c, Math.max(townRows.get(c) ?? -Infinity, k % W + ((k / W) | 0))); }
for (let k = 0; k < W * H; k++) {
  const i = k % W, j = (k / W) | 0;
  hist[level[k]]++;
  if (toTown[k] <= FLAT && level[k] !== 1) flatOk = false;
  // In front: below a town tile on screen (the same column or the next), within FRONT_LOW tiles.
  const row = Math.max(townRows.get(i - j) ?? -Infinity, townRows.get(i - j - 1) ?? -Infinity, townRows.get(i - j + 1) ?? -Infinity);
  if (!isTown(k) && i + j > row && (i + j - row) / 2 <= FRONT_LOW) front = Math.max(front, level[k]);
  if (level[k] >= 8) { high++; if (i - W / 2 + j - H / 2 < 0) highBack++; }
}
let slopeOk = true;
for (let k = 0; k < W * H; k++) {
  const i = k % W, j = (k / W) | 0, low = Math.min(...[[1, 0], [-1, 0], [0, 1], [0, -1]].filter(([di, dj]) => i + di >= 0 && j + dj >= 0 && i + di < W && j + dj < H).map(([di, dj]) => level[k + dj * W + di]));
  if (level[k] > low + 2) slopeOk = false;
}

// Water (#41): no water or beach on the town or the flat ground round it;
// every sea and lake at least LAKE_MIN tiles, all at one level; and no
// hollow left: flooded again from the edges and the town, no land lies below
// the water's level round it and every lake stands at its lowest rim.
const { cover } = t, sides = [[1, 0], [-1, 0], [0, 1], [0, -1]];
let dryTown = true, smallest = Infinity, flatWater = true, lakes = 0, hollows = 0;
const part = new Int32Array(W * H).fill(-1);
for (let k0 = 0; k0 < W * H; k0++) {
  if (toTown[k0] <= FLAT && (isWater(cover[k0]) || cover[k0] === COVER.beach) && !t.main[k0]) dryTown = false;
  if (!isWater(cover[k0]) || cover[k0] === COVER.river || part[k0] >= 0) continue;
  const tiles = [k0];
  part[k0] = k0;
  for (let n = 0; n < tiles.length; n++) {
    const k = tiles[n], i = k % W, j = (k / W) | 0;
    for (const [di, dj] of sides) {
      const nk = (j + dj) * W + i + di;
      if (i + di >= 0 && j + dj >= 0 && i + di < W && j + dj < H && part[nk] < 0 && cover[nk] === cover[k0]) { part[nk] = k0; tiles.push(nk); }
    }
  }
  smallest = Math.min(smallest, tiles.length);
  if (tiles.some(k => level[k] !== level[k0])) flatWater = false;
  if (cover[k0] === COVER.lake) lakes++;
}
const fill = new Int8Array(W * H).fill(-1), buckets = Array.from({ length: LEVELS }, () => []);
for (let k = 0; k < W * H; k++) {
  const i = k % W, j = (k / W) | 0;
  if (i === 0 || j === 0 || i === W - 1 || j === H - 1 || isTown(k)) { fill[k] = level[k]; buckets[level[k]].push(k); }
}
for (let l = 0; l < LEVELS; l++) for (let n = 0; n < buckets[l].length; n++) {
  const k = buckets[l][n], i = k % W, j = (k / W) | 0;
  for (const [di, dj] of sides) {
    const nk = (j + dj) * W + i + di;
    if (i + di >= 0 && j + dj >= 0 && i + di < W && j + dj < H && fill[nk] < 0) { fill[nk] = Math.max(level[nk], l); buckets[fill[nk]].push(nk); }
  }
}
for (let k = 0; k < W * H; k++) if (fill[k] > level[k]) hollows++;
const share = c => (100 * cover.filter(v => v === c).length / (W * H)).toFixed(1);

// Rivers (#42). The town river: from where it meets the sea or leaves the
// map, every tile of it is reached going upstream without going down, so it
// never runs uphill; it reaches the hills. Side rivers: each tile drains to
// one no higher, and into a river, a lake, the sea or off the map. The town
// drains: level-1 ground (or lower) joins it to the sea or the map's edge.
const { main, down } = t, N = W * H, edgeTile = k => { const i = k % W, j = (k / W) | 0; return i === 0 || j === 0 || i === W - 1 || j === H - 1; };
const nearOf = k => sides.map(([di, dj]) => [k % W + di, ((k / W) | 0) + dj]).filter(([i, j]) => i >= 0 && j >= 0 && i < W && j < H).map(([i, j]) => j * W + i);
const mouths = [];
for (let k = 0; k < N; k++) if (main[k] && (edgeTile(k) || nearOf(k).some(n => cover[n] === COVER.sea))) mouths.push(k);
const upstream = new Uint8Array(N);
for (const k of mouths) upstream[k] = 1;
for (let n = 0; n < mouths.length; n++) for (const nk of nearOf(mouths[n])) if (main[nk] && !upstream[nk] && level[nk] >= level[mouths[n]]) { upstream[nk] = 1; mouths.push(nk); }
let mainTiles = 0, mainUphill = 0, mainTop = 0, sideTiles = 0, sideUphill = 0, sideLost = 0, sources = 0, forks = 0;
for (let k = 0; k < N; k++) {
  if (main[k]) { mainTiles++; if (!upstream[k]) mainUphill++; mainTop = Math.max(mainTop, level[k]); continue; }
  if (cover[k] !== COVER.river) continue;
  sideTiles++;
  const d = down[k];
  if (d >= 0 && level[d] > level[k]) sideUphill++;
  if (d >= 0 && !main[d] && !isWater(cover[d])) sideLost++;
  if (!nearOf(k).some(n => cover[n] === COVER.river && !main[n] && down[n] === k)) { sources++; if (nearOf(k).some(n => main[n])) forks++; }
}
const drains = [];
const reached = new Uint8Array(N);
for (let k = 0; k < N; k++) if (isTown(k)) { reached[k] = 1; drains.push(k); }
let townDrains = false;
for (let n = 0; n < drains.length && !townDrains; n++) for (const nk of nearOf(drains[n])) {
  if (reached[nk] || level[nk] > 1) continue;
  if (edgeTile(nk) || cover[nk] === COVER.sea) { townDrains = true; break; }
  reached[nk] = 1; drains.push(nk);
}

// Drawing (#40): the town's ground is drawn over the terrain and buildings
// stand in the city layer, which is only right where no higher ground in
// front reaches over them: every town tile, and every block tile with a 200 px
// building on it. Trees next to the town (under 2 tiles) stand in the city
// layer too, the rest are drawn into the ground (#43); none of the first may
// be hidden.
let townHidden = 0, buildingsHidden = 0, treesHidden = 0;
const blockTiles = new Set();
for (const b of plan.districts.flatMap(d => d.blocks)) for (let j = b.y; j < b.y + b.h; j++) for (let i = b.x; i < b.x + b.w; i++) blockTiles.add(`${i}/${j}`);
for (let k = 0; k < W * H; k++) {
  const i = k % W + t.x0, j = ((k / W) | 0) + t.y0;
  if (isTown(k)) {
    if (hiddenBy(t, i, j)) townHidden++;
    if (blockTiles.has(`${i}/${j}`) && hiddenBy(t, i, j, 200, 52)) buildingsHidden++;
  }
}
const trees = coverTrees(t, seed);
let pines = 0, treesNear = 0;
for (const [i, j, , , pine] of trees) {
  const a = Math.floor(i), b = Math.floor(j);
  if (pine) pines++;
  if (toTown[(b - t.y0) * W + a - t.x0] < 2) { treesNear++; if (hiddenBy(t, a, b, 28, 24)) treesHidden++; }
}

// Ground cover (#43): none on the town; fields only on low land (level 1–2);
// snow only above rock; farms 3 tiles or more from the town.
const { crop, farms } = t;
let coverOk = true, snowLow = Infinity, rockHigh = 0;
for (let k = 0; k < W * H; k++) {
  const c = cover[k];
  if (isTown(k) && c) coverOk = false;
  if (c === COVER.field && level[k] > 2) coverOk = false;
  if (c !== COVER.field && crop[k]) coverOk = false;
  if (c === COVER.snow) snowLow = Math.min(snowLow, level[k]);
  if (c === COVER.rock) rockHigh = Math.max(rockHigh, level[k]);
}
for (const k of farms.keys()) if (toTown[k] < 3 || cover[k] !== COVER.field) coverOk = false;
const land = cover.filter((c, k) => !isTown(k) && !isWater(c)).length;
const ofLand = c => (100 * cover.filter((v, k) => v === c && !isTown(k)).length / land).toFixed(0);

// Building the ground's mesh as the Metropolis look does (faces, a top per
// tile, the cover's details and trees), timed; the Kenney look adds a quad
// per tile and its water, and draws its round trees as one quad each.
const white = { baseTexture: { width: 16, height: 16 }, frame: { x: 0, y: 0, width: 16, height: 16 } };
const groundTimes = [];
let built;
for (let n = 0; n < 6; n++) {
  const t0 = performance.now();
  built = new GroundPen(white);
  const inGround = new Map();
  for (const tr of coverTrees(t, seed)) {
    const k = (Math.floor(tr[1]) - t.y0) * W + Math.floor(tr[0]) - t.x0;
    if (toTown[k] >= 2) inGround.set(k, [...(inGround.get(k) || []), tr]);
  }
  eachGroundTile(t, (i, j, z, left, right, k) => {
    groundFaces(built, i, j, z, left, right, [0x6e9a58, 0x5b8248], [0x8a6a4a, 0x6b523a]);
    poly(built, 0x8cba6c, [iso(i, j), iso(i + 1, j), iso(i + 1, j + 1), iso(i, j + 1)].map(([x, y]) => [x, y - z]));
    if (isWater(t.cover[k])) poly(built, 0xb3d6ec, [iso(i + 0.3, j + 0.4), iso(i + 0.7, j + 0.4), iso(i + 0.7, j + 0.45), iso(i + 0.3, j + 0.45)].map(([x, y]) => [x, y - z]));
    else coverDetails(built, t, k, i, j, z, inGround, ([x, y, size, s, conifer]) => (conifer ? pine : tree)(built, x, y, size, s, true));
  });
  groundTimes.push(performance.now() - t0);
}

for (let j = 0; j < H; j++) {
  let row = "";
  for (let i = 0; i < W; i++) { const k = j * W + i; row += isTown(k) ? "#" : farms.has(k) ? "h" : ["", "~", "=", ".", "F", "f", ":", "^", "*", "%"][cover[k]] || level[k].toString(13); }
  console.log(row);
}
console.log();
console.log(`seed ${JSON.stringify(seed)}: town ${city.W} × ${city.H}, map ${W} × ${H} (${W * H} tiles)`);
console.log(`terrain in ${times[0].toFixed(1)} ms the first time, then ${Math.min(...times.slice(1)).toFixed(1)}–${Math.max(...times.slice(1)).toFixed(1)} ms`);
console.log(`levels: ${hist.map((n, l) => `${l.toString(13)} ${(100 * n / (W * H)).toFixed(0)}%`).join(", ")}; highest ${hist.findLastIndex(n => n > 0)}`);
console.log(`mountains (level 8 and up): ${high} tiles, ${high ? (100 * highBack / high).toFixed(0) : "–"} % in the back half of the map; highest level within ${FRONT_LOW} tiles in front of the town: ${front}`);
console.log(`town and ${FLAT} tiles round it flat: ${flatOk ? "yes" : "NO"}; peaks taper: ${slopeOk ? "yes" : "NO"}`);
console.log(`same terrain twice: ${same ? "yes" : "NO"}; another seed, other terrain: ${differs ? "yes" : "NO"}`);
console.log(`sea ${share(COVER.sea)} %, ${lakes} lakes (${share(COVER.lake)} %), beach ${share(COVER.beach)} %; smallest sea or lake ${smallest === Infinity ? "–" : smallest} tiles (at least ${LAKE_MIN}: ${smallest >= LAKE_MIN ? "yes" : "NO"}); each one flat: ${flatWater ? "yes" : "NO"}; hollows left: ${hollows ? hollows + " tiles, NO" : "none"}; town and its flat ground dry: ${dryTown ? "yes" : "NO"}`);
console.log(`town river: ${!plan.river ? "none (no cut)" : `${mainTiles} tiles, ${mouths.length ? "reaches the sea or the map's edge" : "NO mouth"}, uphill tiles ${mainUphill}${mainUphill ? " NO" : ""}, rises at level ${mainTop} (hills ${HILLS}+: ${mainTop >= HILLS ? "yes" : "NO"})`}; side rivers: ${sideTiles} tiles from ${sources} sources (${forks} next to the town river), uphill ${sideUphill}${sideUphill ? " NO" : ""}, ending on dry land ${sideLost}${sideLost ? " NO" : ""}; town drains: ${townDrains ? "yes" : "NO"}`);
console.log(`hidden by higher ground in front: town tiles ${townHidden}, buildings ${buildingsHidden}, trees next to the town ${treesHidden} of ${treesNear}`);
console.log(`land outside town ${land} tiles: meadow ${ofLand(COVER.grass)} %, forest ${ofLand(COVER.forest)} %, woods ${ofLand(COVER.woods)} %, fields ${ofLand(COVER.field)} % (${farms.size} farms), rock ${ofLand(COVER.rock)} %, snow ${ofLand(COVER.snow)} % (from level ${snowLow === Infinity ? "–" : snowLow}, rock up to ${rockHigh}); ${trees.length} trees, ${pines} pines`);
console.log(`cover where it belongs: ${coverOk && (snowLow === Infinity || snowLow > rockHigh) ? "yes" : "NO"}`);
console.log(`ground mesh in ${groundTimes[0].toFixed(1)} ms the first time, then ${Math.min(...groundTimes.slice(1)).toFixed(1)}–${Math.max(...groundTimes.slice(1)).toFixed(1)} ms: ${built.xy.length / 2} vertices, ${built.index.length / 3} triangles`);

// The page: the same code and plan, drawn in the browser, any seed.
const html = `<!doctype html>
<meta charset="utf-8">
<title>kube-city terrain sketch</title>
<style>
  body { margin: 0; padding: 12px 16px; font: 14px system-ui, sans-serif; background: #c4dce2; color: #1e2a29; }
  form { display: flex; flex-wrap: wrap; gap: 8px 16px; align-items: center; margin-bottom: 8px; }
  input[type=text] { width: 12ch; font: inherit; }
  canvas { display: block; max-width: 100%; height: auto; }
  p { margin: 6px 0; font-variant-numeric: tabular-nums; }
</style>
<form onsubmit="return false">
  <label>Seed <input id="seed" type="text" maxlength="40" value=${JSON.stringify(JSON.stringify(seed))}></label>
  <button id="new" type="button">New world</button>
  <label>Relief <input id="relief" type="range" min="0" max="12" value="8"></label>
</form>
<p id="facts"></p>
<canvas id="map"></canvas>
<script>
${code}
const plan = ${JSON.stringify(plan)};
// Grass in the valleys, olive on the hills, rock higher up, snow on top.
const COLOURS = ["#7fae5a", "#9cc865", "#93bf5c", "#8ab456", "#a3ad5c", "#b3a462", "#a99266", "#9a8a72", "#8f877e", "#a19c95", "#c3c0bb", "#e2e1de", "#f6f7f8"];
// Ground cover (#43): forest, woods, fields, rock, snow.
const COVERS = { [COVER.forest]: "#5f9a4c", [COVER.woods]: "#7fb35e", [COVER.field]: "#d8c46a", [COVER.rock]: "#a39d93", [COVER.snow]: "#f2f4f6" };
const TOWN = "#b9b5ae", WATER = "#4f9fd8", SHALLOW = "#6fb4e0", DEEP = "#2f77b5", BEACH = "#e8d9a2", T = 16;
const shade = (hex, f) => "#" + [1, 3, 5].map(n => Math.round(parseInt(hex.slice(n, n + 2), 16) * f).toString(16).padStart(2, "0")).join("");
function draw() {
  const seed = document.getElementById("seed").value, lift = Number(document.getElementById("relief").value);
  const t0 = performance.now(), t = planTerrain(plan, seed), ms = performance.now() - t0;
  const { W, H, level, toTown } = t, canvas = document.getElementById("map"), top = 12 * lift + T;
  canvas.width = (W + H) * T; canvas.height = (W + H) * T / 2 + top;
  const g = canvas.getContext("2d");
  const at = (i, j, z) => [(i - j + H) * T, (i + j) * T / 2 + top - z];
  const poly = (fill, pts) => { g.fillStyle = fill; g.beginPath(); pts.forEach(([x, y], n) => (n ? g.lineTo(x, y) : g.moveTo(x, y))); g.fill(); };
  for (let s = 0; s < W + H - 1; s++) for (let i = Math.max(0, s - H + 1); i <= Math.min(W - 1, s); i++) {
    const j = s - i, k = j * W + i, wet = isWater(t.cover[k]), z = level[k] * lift - (wet ? 2 : 0);
    const sea = t.depth[k] < 2 ? SHALLOW : t.depth[k] < 6 ? WATER : DEEP;
    const colour = inRiver(plan.river, i + t.x0, j + t.y0) ? WATER : toTown[k] === 0 ? TOWN : wet ? sea : t.cover[k] === COVER.beach ? BEACH : COVERS[t.cover[k]] || COLOURS[level[k]];
    poly(shade(colour, 0.78), [at(i, j + 1, z), at(i + 1, j + 1, z), at(i + 1, j + 1, 0), at(i, j + 1, 0)]);
    poly(shade(colour, 0.62), [at(i + 1, j, z), at(i + 1, j + 1, z), at(i + 1, j + 1, 0), at(i + 1, j, 0)]);
    poly(colour, [at(i, j, z), at(i + 1, j, z), at(i + 1, j + 1, z), at(i, j + 1, z)]);
  }
  const hist = new Array(LEVELS).fill(0);
  for (const l of level) hist[l]++;
  document.getElementById("facts").textContent = \`map \${W} × \${H}, terrain in \${ms.toFixed(1)} ms; levels \${hist.map((n, l) => \`\${l}: \${(100 * n / level.length).toFixed(0)}%\`).join(", ")}\`;
}
document.getElementById("seed").addEventListener("change", draw);
document.getElementById("relief").addEventListener("input", draw);
document.getElementById("new").addEventListener("click", () => { document.getElementById("seed").value = String(1 + Math.floor(Math.random() * 999999)); draw(); });
draw();
</script>
`;
const out = new URL("out/", import.meta.url);
mkdirSync(out, { recursive: true });
writeFileSync(new URL("terrain.html", out), html);
console.log("written: art/out/terrain.html");
