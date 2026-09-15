---
title: Land sharded issue storage layout (a7a670c) onto main + build targeted GitHub sync
status: open
priority: 0
issue_type: task
labels:
- human
created_at: 2026-07-30T16:11:54.945437012+00:00
updated_at: 2026-09-15T20:22:17.469008597+00:00
---

# Description

Owner request (2026-07-30). Two parts:

PART 1 (DONE): rebase/land the 'Add sharded issue storage layout' commit
(a7a670c) which existed ONLY on the local `integration` branch (never
pushed, not on main/origin) -- it subdivides .minibeads/issues/ into
shards (issues/n/<tens>/<ones>/ for numeric IDs, issues/h/<hex><hex>/ for
hash IDs) to avoid many-thousands-of-files-in-one-flat-directory.

  - Verified content-level (not ancestry): a7a670c was NOT on main or
    origin/main; it existed only as the tip of the local `integration`
    branch, one commit ahead of an old point on main (0.22.0).
  - Design is opt-in and reversible: new repos default to the flat layout
    unless `--mb-issue-layout sharded` is passed to init; existing repos
    (e.g. deepscry's live, thousands-of-issue store) are UNAFFECTED until
    someone explicitly runs `mb mb-migrate --to=sharded` (or `--to=flat`
    to revert). Every issue lookup checks the configured layout first,
    then falls back to checking both flat and sharded paths, so a store
    migrated mid-transition still resolves every issue.
  - Rebased onto main (which had advanced through releases 0.23.0/
    0.24.0/0.25.0 since this branch forked at 0.22.0); resolved a trivial
    Cargo.toml/Cargo.lock version conflict, then found and fixed two
    real problems the textual rebase merged silently instead of
    conflicting on: (1) a test call site (ready_tests, added on main
    after this branch diverged) using the old 3-arg Storage::init
    signature -- broke the test build; (2) a clippy::collapsible_if the
    rebase introduced -- broke `make`-style clippy -D warnings.
  - Verified with the FULL suite before releasing: 65 unit tests, 7 e2e
    shell tests (incl. ready_filters.sh, which had been failing against a
    stale target/debug/mb binary during my first pass -- rebuilding debug
    fixed it, it was not a real regression), and the fuzz/stress binary
    (7 passed incl. test_migration_stress, 1 ignored) run against both
    minibeads' own storage and upstream `bd` compatibility. clippy -D
    warnings clean.
  - Released as 0.26.0 (semver minor bump, new feature): CHANGELOG entry
    added, Cargo.toml/Cargo.lock bumped, commit 'Release 0.26.0: sharded
    issue storage layout'. Pushed directly to main + integration on
    origin (both fast-forwarded from b491f61 -> facc798), per owner's
    explicit permission for Part 1 (unlike Part 2, this needed no PR).
  - Did NOT touch the globally-installed `mb` (~/.cargo/bin/mb, still
    0.25.0 built 2026-07-20) -- every other agent on this box depends on
    it continuously; upgrading it is deferred to Part 2 below, when a
    cargo install is actually needed to test the new CLI surface.
  - A stale/orphaned `git rebase` (of `integration` onto a174894) and an
    orphaned `git stash` (WIP on integration: a7a670c, containing a tiny
    unrelated .beads/issues/minibeads-30.md 'TMP_NOTES' placeholder edit
    + a sync-state timestamp bump) were both present when this task
    started. Both were inspected before touching anything. The rebase
    was stale/abandoned (git rebase --abort cleanly resolved it, a
    no-op since integration already equaled its orig-head). The stash
    was LEFT UNTOUCHED (not popped, not dropped) since its content looks
    like unrelated scratch, not something blocking this work -- owner
    should look at stash@{0} directly if they want that content back.

PART 2 (NOT STARTED): build a lightweight targeted single/few-issue
GitHub sync (vs. the heavyweight full poll), on a feature branch + PR
only -- owner reviews before merge/release, no direct release to
crates.io, semver bump if features added. Coordinate scope with the
`ui-issue-workflow` agent (deepscry side, via the team lead) rather than
duplicating its gap investigation into what's missing GitHub-sync-wise.
Known/suspected gaps to check: does an update overwrite the GitHub
description on every sync pass (prior history of that being
destructive); can a bead carry a screenshot reference; is the
bead<->issue link reliably bidirectional; does a subset/single-issue
filter exist at all today.


PART 2 PROGRESS (2026-07-30): team-lead relayed ui-issue-workflow's live-verified findings. Targeted sync already existed (mb github sync <ids>) -- did not rebuild it. Implemented on branch feature/pull-only-local-edit-guard, not yet merged:
1. --pull-only silent-overwrite fix (Gap 1): refuses to discard a locally-changed title/description/status, prints what would be discarded, records a conflict, requires --force to override. Regression test added and confirmed to fail pre-fix.
2. mb github sync --since <cutoff> (Gap 2): RFC3339 or relative duration (24h/2d/90m), composable with explicit IDs.
3. mb github --token <TOKEN> <subcommand> (Gap 3, the token/bot-attribution flag): scopes GH_TOKEN to this invocation only, no gh auth switch, applies to all github subcommands via one change point.
4. Separately flagged safety bug (mb create defaulting to prefix "tmp" when config.yaml is missing but real issues already exist): fixed by preferring inference from existing issue files over the directory-name guess in both Storage::open and Storage::init. Regression test added and confirmed to fail pre-fix.
Screenshot/attachment field (lowest priority) not started.
Bumping to 0.27.0 (CHANGELOG updated). Full test suite + clippy in progress before opening the PR (feature branch + PR only, no merge, no crates.io -- per standing constraint).

2026-09-07_#225(f47b766cbf): Reviewed Brian/unormal cumulative PRs #15 through #24 with independent agents. Corrected migration timestamp precedence (#17), persisted-snapshot normalization (#21), and punctuation code-reference boundaries (#23); see ai_docs/pr-landing-review.md. Full corrected stack passes 203 debug tests on stable and Rust 1.87, fmt, strict all-target/all-feature Clippy, and security audit (zero warnings/vulnerabilities). Next: fast-forward review fixes to existing PR24, require final-head CI, merge stack atomically preserving original commit order, verify all PR states and archive branch tags. Earlier heads fail audit until #24, so do not land red intermediate states. No new issues created. The separate sync-overhaul prototype remains frozen and excluded; human plan review is required before further implementation. Review proposal: ai_docs/github-sync-plan.md in the owner working checkout.

2026-09-07_#226(25b963ff0c): Final corrected PR24 CI run 34158336503 passed all seven jobs: tests, coverage, fmt/Clippy, security audit, Linux, macOS, Windows. Merge preparation complete. Next: guarded normal merge of exact reviewed head 25b963ff0c; verify PR15–24 ancestry and archive branch tags. Overhaul implementation remains held for owner review; no new issues created.

2026-09-07_#227(19fc71cfc1): LANDING COMPLETE. Brian/unormal PRs #15–#24 are all MERGED on GitHub through normal merge 19fc71cfc1. Final corrected head 25b963ff0c passed all seven CI jobs (run 34158336503). Verified every original head is an ancestor of origin/main and merged source matches tested tip. All ten completed codex/review-* branches archived as remote annotated .v1 tags and verified against their PR heads; fork branches retained. ai_docs/pr-landing-review.md records results. No new issues created. BLOCKED ONLY ON HUMAN PLAN REVIEW: ai_docs/github-sync-plan.md in owner working checkout covers issue/label scope, real common ancestors, deterministic prose merging, Unison model, concurrency and recovery. Existing experimental prototype is frozen/unapproved and excluded from the landing. Do not resume overhaul implementation until the requested review round is complete.

2026-09-07_#228(3f467d063a): A post-merge macOS stress-harness flake was fixed separately under minibeads-3: parallel tests now initialize debug binaries exactly once instead of rebuilding executables while siblings use them. This adds five routine regressions (208 routine tests now) without changing synchronization behavior or the approved PR ancestry. The only remaining feature-work prerequisite is the requested human review of ai_docs/github-sync-plan.md; the prototype stays frozen. No new issues created.

2026-09-15: Owner approved the reviewed GitHub sync plan with "Get to work". Implementation restarted from landed main 6e30bdc7a8 in isolated worktree /tmp/minibeads-sync-overhaul.NLLkth; the old prototype remains untouched. Baseline make -o purge validate passes. Parallel work is scoped to new prose_merge and github_ancestor modules while the parent integrates label/ID selection, full three-way field reconciliation, checked local/remote writes, legacy-state handling, and acceptance tests. No new issue created.

2026-09-15_#229(6e30bdc7a8): APPROVED SYNC OVERHAUL IMPLEMENTED AND READY FOR OWNER PATCH REVIEW. Added repeatable local-label AND filtering intersected with IDs/--since; versioned per-issue common-content ancestors; full title/body/open-state three-way reconciliation; deterministic bounded prose merge; per-issue sync leases; checked local writes; bounded local/remote race retries; atomic ancestor + comment-state transactions; conservative legacy/future-schema handling; ownership-safe relinking; and ID-migration ancestry updates. Two independent adversarial reviews initially found fourteen safety gaps; all were fixed with regressions, including repeated-paragraph data loss, structured Markdown overlap, replay stability, comparison/work determinism, stale concurrent checkpoints, duplicate comment export, lock-held network I/O, dry-run recovery writes, split checkpoints, and relink ownership. Final local evidence: make -o purge validate passes 296 debug tests (66 lib, 189 bin, 7 shell, 1 migration, 28 regressions, 5 harness), fmt, and Clippy; strict all-target/all-feature Clippy passes; Rust 1.87 passes 303 executed tests with 1 ignored stress test. Design/recommendation: ai_docs/github-sync-design.md. Next and only gate: owner reviews the resulting patch/PR before merge; no crates.io release and no live GitHub stress writes were performed.
