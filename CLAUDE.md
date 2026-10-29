# kube-city

Draws a Kubernetes cluster as a living isometric city in the browser (Rust:
kube-rs, tokio, axum; the page is PixiJS). The old terminal view was
removed in 0.4.0. **Read `docs/decisions.md` first** (and `ROADMAP.md` for what is planned): it
holds the decisions so far, the open problems and the recommended next step.

## Commands

```bash
cargo run --release -- --demo                                  # simulated cluster, in the browser
cargo run --release -- --context my-cluster                    # live, read-only
cargo run --release -- --replay recordings/big-cluster.jsonl  # recorded real cluster (git-ignored)
cargo test
cargo clippy --all-targets   # keep it warning-free
```

Browser prototype from a recording (anonymised, writes `art/out/city.html`):

```bash
python3 art/web_prototype.py recordings/big-cluster.jsonl art/out
```

The city's layout as a map of tiles, with land per building and junction
counts (sizes only, no names; `2` for Metropolis's road width):

```bash
node art/layout_sketch.mjs recordings/big-cluster.jsonl [2]
```

Kenney sprites (CC0) live in `assets/kenney/`; see its README to add more.

## Layout

- `src/world.rs`: the `WorldEvent`s every source sends (pods and their details)
- `src/cluster.rs`, `src/demo.rs`, `src/recording.rs`: event sources
- `src/web.rs`: the browser view's server (page, sprites from `build.rs`, WebSocket feed);
  the page itself is `art/city.template.html`
- `assets/kenney/`: the Kenney sprites the browser view uses (CC0)
- `art/`: style samples script, evaluation page sources, browser prototype
  (`web_prototype.py` + `city.template.html`)

## Conventions

- Cluster access is read-only (list/watch); never write to clusters or kubeconfig files.
- Recordings hold real namespace and workload names: keep them out of git and
  anonymise anything shared from them.
- Conventional Commits (`feat:`, `fix:`, `docs:` …), no Jira keys here.
- No `Co-Authored-By: Claude` trailer in commits and no "Generated with Claude
  Code" footer in PR descriptions (owner's choice, 2026-10-23).

## Releasing

Public repo: https://github.com/braveadmin/kube-city (MIT OR Apache-2.0).
Commits carry only the name `braveadmin` with an empty email (set in this
repo's git config); keep it that way and keep employer or cluster names out.

1. Bump `version` in `Cargo.toml`, commit.
2. `git tag vX.Y.Z && git push origin vX.Y.Z`: cargo-dist (`dist-workspace.toml`,
   `.github/workflows/release.yml`) builds every platform and publishes the
   GitHub release with installers. `dist plan` previews it locally.
3. `cargo publish` (crates.io) after the GitHub release is out.
4. The Homebrew formula goes to `braveadmin/homebrew-tap` from the release
   workflow; it needs the `HOMEBREW_TAP_TOKEN` secret. Without it the
   release is not published at all.

After changing dist settings: `allow-dirty = ["ci"]` (so Dependabot can bump
actions) makes `dist generate` skip the workflow, so remove that line, run
`dist generate`, then put the line back, and restore the hand-made
`paths-ignore` on `pull_request` in `release.yml` (docs-only PRs skip the
release builds; `plan` is not a required check for that reason).

## Working agreement

The owner is product owner: decides what and why, accepts features, decides
releases. Claude implements, decides how, and advises as an expert.

Flow, one feature per GitHub issue (and per session: "work on issue #N"):

1. **Request** (owner): an issue with problem, for whom, "done when".
2. **Plan** (Claude): reply in the issue with plan, effort, decisions needed
   and what the owner must do; plan mode for anything big.
3. **Decide** (owner): answers in one go; `needs-decision` label until then.
4. **Build** (Claude): branch in its own worktree (below), PR with
   "Closes #N" and tests; no screenshots (owner's choice, 2026-10-29): for
   visible changes, give the owner a command to run it instead.
5. **Accept** (owner): tests and merges, or comments.
6. **Release** (Claude, on the owner's go): milestone → CHANGELOG.md → tag →
   GitHub, Homebrew, crates.io.
7. **Look back** after each milestone: three lines on what to keep or change.

Refer to work by issue or PR number, never by "step 4".

**One worktree per feature.** Several sessions may run at once, and they
share the main checkout; switching branches there moves every other
session's files and commits too (it put #13's commit on #14's branch).
So every branch, docs and fixes included, gets its own worktree, and the
main checkout stays on `master` and is only used for reading and `git
fetch`:

```bash
git fetch origin
git worktree add ../kube-city-wt/<branch> -b <branch> origin/master
# work, build, test and commit only inside ../kube-city-wt/<branch>
git worktree remove ../kube-city-wt/<branch>   # once the PR is merged
```

Each worktree has its own `target/`, so the first build there is a full one.

Claude may, without asking: create branches; force-push its own PR
branches; rebase PRs; merge `docs:`, `chore:`, `ci:` and Dependabot PRs once
checks are green; delete merged branches; open and label issues.

Claude asks first before: merging features; changing branch protection,
tags or releases; publishing outside GitHub (crates.io, krew,
announcements); heavy local work (Docker images, long-running browsers, big
downloads).

The owner avoids GitHub's "Update branch" button (it adds merge commits);
ask Claude to rebase instead.

Messages: first line says what is needed from the owner (or "nothing"),
then the result, then details. Choices as multiple-choice questions. Claude
says so plainly when work is derailing: scope creep, a session growing too
long, effort out of proportion, or a decision that conflicts with an
earlier one.
