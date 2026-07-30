---
title: Land sharded issue storage layout (a7a670c) onto main + build targeted GitHub sync
status: open
priority: 0
issue_type: task
labels:
- human
created_at: 2026-07-30T16:11:54.945437012+00:00
updated_at: 2026-07-30T16:11:54.945437012+00:00
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
