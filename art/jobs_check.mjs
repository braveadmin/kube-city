#!/usr/bin/env node
// Checks how a Job's works looks (#65), with the page's own lookOf(): it
// stands while any of its pods exists, finished ones too; it works while a
// run is pending or running; it idles when every run has ended; its first
// run is a construction site, later runs start in the works; a failed last
// run is a burnt-out shell, a crashing one burns; it goes with its last pod.
// Other workloads' finished pods still do not count. Exits 1 on a failure.
//
//   node art/jobs_check.mjs

import { readFileSync } from "node:fs";

const page = readFileSync(new URL("city.template.html", import.meta.url), "utf8");
const from = page.indexOf("const exposures = new Map();");
const to = page.indexOf("// ---------------------------------------------------------------- scene");
const L = new Function(`
const [PENDING, RUNNING, CRASH, TERMINATING, SUCCEEDED, FAILED] = [0, 1, 2, 3, 4, 5];
const DATA = { ns: ["ns"] }, world = { requests: false };
const storeysOf = () => 2, weightOfBuilding = () => 0;
${page.slice(from, to)}
return { lookOf, S: { PENDING, RUNNING, CRASH, TERMINATING, SUCCEEDED, FAILED } };`)();
const { PENDING, RUNNING, CRASH, TERMINATING, SUCCEEDED, FAILED } = L.S;

const building = (kind, states) => ({
  ns: 0, kind, name: "nightly", variant: { h: 7, floors: 3 },
  pods: new Map(states.map((s, k) => [k, s])),
});
const look = (kind, ...states) => L.lookOf(building(kind, states));

let failures = 0;
const check = (what, got, want) => {
  const text = got === null ? "gone" : `${got.cat}${got.working ? " working" : ""}${got.fire ? " on fire" : ""}`;
  if (text !== want) { failures++; console.log(`FAIL ${what}: ${text}, not ${want}`); }
};

check("first run pending", look("Job", PENDING), "build");
check("first run running", look("Job", RUNNING), "factory working");
check("runs ended", look("Job", SUCCEEDED, SUCCEEDED, SUCCEEDED), "factory");
check("a new run pending after earlier ones", look("Job", SUCCEEDED, SUCCEEDED, PENDING), "factory working");
check("a new run running", look("Job", SUCCEEDED, RUNNING), "factory working");
check("last run failed", look("Job", SUCCEEDED, FAILED), "scorched");
check("a new run after a failed one", look("Job", FAILED, PENDING), "factory working");
check("a run crashing", look("Job", SUCCEEDED, CRASH), "factory on fire");
check("old runs being deleted", look("Job", SUCCEEDED, TERMINATING), "factory");
check("every pod deleted", look("Job"), "gone");
check("a finished bare pod", look("Standalone", SUCCEEDED), "gone");
check("a Deployment", look("Deployment", RUNNING, SUCCEEDED), "tower");

console.log(failures ? `${failures} failures` : "all checks pass");
process.exit(failures ? 1 : 0);
