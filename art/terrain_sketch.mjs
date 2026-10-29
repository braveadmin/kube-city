#!/usr/bin/env node
// Lays out a recorded cluster with the page's own layout code, grows the
// terrain round it with the page's own terrain code and prints the checks
// from issue #39, a map of levels and `art/out/terrain.html`: the town and its
// terrain, in which the seed can be changed. Only the sizes and zones of
// namespaces are used, so neither output holds names.
//
//   node art/terrain_sketch.mjs recordings/big-cluster.jsonl [seed] [gap]
//
// gap is the road width: 1 for the Kenney look (the default), 2 for Metropolis.
//
// Map: # the town (its river too), 0–9 and a–c the ground level (the town
// stands on 1).

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
// The ground's mesh builder, without the mesh (that needs PIXI).
const pen = page.slice(page.indexOf("class GroundPen {"), page.indexOf("\n}\n", page.indexOf("class GroundPen {")) + 2);
const { planCity, lotsFor, zoneOf, planTerrain, FRONT_LOW, FLAT, MARGIN, TREE, eachGroundTile, groundFaces, hiddenBy, iso, poly, GroundPen } =
  new Function(`${code}\n${pen}\nreturn { planCity, lotsFor, zoneOf, planTerrain, FRONT_LOW, FLAT, MARGIN, TREE, eachGroundTile, groundFaces, hiddenBy, iso, poly, GroundPen };`)();

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
const same = t.level.every((v, k) => v === again.level[k]) && t.height.every((v, k) => v === again.height[k]);
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

// Drawing (#40): the town's ground is drawn over the terrain and buildings
// and trees stand in the city layer, which is only right where no higher
// ground in front reaches over them. Every town tile, every block tile with
// a 200 px building on it, and every tile of the grass round the town where a
// tree may stand. A tree the ground hides is drawn into the ground instead,
// which is only right away from the town (2 tiles or more).
let townHidden = 0, buildingsHidden = 0, treesHidden = 0, treesNearTown = 0;
const blockTiles = new Set();
for (const b of plan.districts.flatMap(d => d.blocks)) for (let j = b.y; j < b.y + b.h; j++) for (let i = b.x; i < b.x + b.w; i++) blockTiles.add(`${i}/${j}`);
for (let k = 0; k < W * H; k++) {
  const i = k % W + t.x0, j = ((k / W) | 0) + t.y0;
  if (isTown(k)) {
    if (hiddenBy(t, i, j)) townHidden++;
    if (blockTiles.has(`${i}/${j}`) && hiddenBy(t, i, j, 200, 52)) buildingsHidden++;
  } else if (i >= -MARGIN && j >= -MARGIN && i < plan.W + MARGIN && j < plan.H + MARGIN && hiddenBy(t, i, j, TREE[1], TREE[0])) {
    treesHidden++;
    if (toTown[k] < 2) treesNearTown++;
  }
}

// Building the ground's mesh as the Metropolis look does (faces, a top per
// tile), timed; the Kenney look adds a quad per tile and its water.
const white = { baseTexture: { width: 16, height: 16 }, frame: { x: 0, y: 0, width: 16, height: 16 } };
const groundTimes = [];
let built;
for (let n = 0; n < 6; n++) {
  const t0 = performance.now();
  built = new GroundPen(white);
  eachGroundTile(t, (i, j, z, left, right) => {
    groundFaces(built, i, j, z, left, right, [0x6e9a58, 0x5b8248], [0x8a6a4a, 0x6b523a]);
    poly(built, 0x8cba6c, [iso(i, j), iso(i + 1, j), iso(i + 1, j + 1), iso(i, j + 1)].map(([x, y]) => [x, y - z]));
  });
  groundTimes.push(performance.now() - t0);
}

for (let j = 0; j < H; j++) {
  let row = "";
  for (let i = 0; i < W; i++) { const k = j * W + i; row += isTown(k) ? "#" : level[k].toString(13); }
  console.log(row);
}
console.log();
console.log(`seed ${JSON.stringify(seed)}: town ${city.W} × ${city.H}, map ${W} × ${H} (${W * H} tiles)`);
console.log(`terrain in ${times[0].toFixed(1)} ms the first time, then ${Math.min(...times.slice(1)).toFixed(1)}–${Math.max(...times.slice(1)).toFixed(1)} ms`);
console.log(`levels: ${hist.map((n, l) => `${l.toString(13)} ${(100 * n / (W * H)).toFixed(0)}%`).join(", ")}; highest ${hist.findLastIndex(n => n > 0)}`);
console.log(`mountains (level 8 and up): ${high} tiles, ${high ? (100 * highBack / high).toFixed(0) : "–"} % in the back half of the map; highest level within ${FRONT_LOW} tiles in front of the town: ${front}`);
console.log(`town and ${FLAT} tiles round it flat: ${flatOk ? "yes" : "NO"}; peaks taper: ${slopeOk ? "yes" : "NO"}`);
console.log(`same terrain twice: ${same ? "yes" : "NO"}; another seed, other terrain: ${differs ? "yes" : "NO"}`);
console.log(`hidden by higher ground in front: town tiles ${townHidden}, buildings ${buildingsHidden}; tree spots round the town ${treesHidden} (drawn into the ground), ${treesNearTown} of them next to it`);
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
const TOWN = "#b9b5ae", WATER = "#4f9fd8", T = 16;
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
    const j = s - i, k = j * W + i, z = level[k] * lift;
    const colour = inRiver(plan.river, i + t.x0, j + t.y0) ? WATER : toTown[k] === 0 ? TOWN : COLOURS[level[k]];
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
