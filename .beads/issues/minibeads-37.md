---
title: Land sharded issue storage layout (a7a670c) onto main + build targeted GitHub sync
status: in_progress
priority: 0
issue_type: task
labels:
- human
created_at: 2026-07-30T16:11:54.945437012+00:00
updated_at: 2026-09-07T20:37:27.446208350+00:00
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

2026-09-07_#215(9437f50c1a): Investigating GitHub synchronization and collaborator PR landing. Working tree started clean. GitHub fetch/PR listing/CI access is currently unavailable, so live PR inventory and landing remain unverified and blocked; cached refs are being triaged separately. Baseline debug validation: 64 library + 85 binary + 7 shell e2e tests pass; existing formatting differences prevent make validate from completing. GitHub sync uses field hashes, not timestamp winner selection, but lacks content ancestors and unsafe bootstrap/divergence-repair paths can overwrite fields. In progress: ancestor-backed three-way merges, local-label-scoped sync, deterministic prose merging and design document. No duplicate issues created.

2026-09-07_#215(9437f50c1a), uncommitted working-tree implementation: Added project-local, git-ignored sync_ancestors/github.json with schema validation, canonical title/body/open-closed ancestor contents, atomic durable replacement, legacy hash-state migration, and per-successful-issue checkpoints. Missing-history divergence no longer silently pushes or pulls; explicit --pull-only --force chooses GitHub. Added local-label AND filtering intersecting existing IDs/--since, deterministic bounded prose three-way merging, rich local status preservation, pre-write remote refresh and post-write convergence verification. Field conflicts skip all issue/comment writes and preserve ancestry. Reproduced and fixed duplicate paragraph replay after a remote write before local checkpointing; repeated/ordering-ambiguous additions fail closed. Final debug validation: 64 library + 137 binary + 7 shell e2e = 208 tests pass (52 new). Full validation remains blocked by baseline formatting in src/format.rs and tests/migration_description_truncation.rs; strict Clippy also flags pre-existing too_many_arguments in comment reconciliation and useless_vec in a storage test. With only those two lint categories allowed, all-target/all-feature Clippy passes. Final validation skipped the private-database purge prerequisite. Design/recommendations/limitations: ai_docs/github-sync-design.md. Cached PR triage: ai_docs/pr-landing-triage.md. Six cached branches are already integrated; only timestamp candidate 51de629 is outstanding and conflicts with a no-op test. None of the cached evidence identifies Brian/unormal PR ownership; no PR IDs guessed, no commits/merges/pushes made. NEXT: restore GitHub access, fetch current Brian PR inventory, establish overlap/dependency order against this draft, review baseline validation failures and actual CI, then land and archive only validated completed feature branches. Residual limitations: GitHub gh writes have no atomic CAS, Markdown/code-like overlaps remain conservative, mutable comment bodies and local Markdown/JSONL sync do not use this prose merger.

2026-09-07_#215(9437f50c1a), continuation validation update superseding the earlier gate failures: Local validation is now green. Applied the two baseline formatting corrections, changed the storage ordering test Vec to an array, and grouped comment-deletion flags into CommentDeletionOptions without behavior changes. make -o purge validate passes debug build, 64 library + 137 binary + 7 shell tests (208 total), cargo fmt, and Clippy. Strict cargo clippy --all-targets --all-features -- -D warnings passes with no allowances. The standalone migration regression also passes (209 tests observed including that separate target). Docs updated. Cached refs are unchanged; GitHub proxy denial has not been bypassed or retried, and live Brian/unormal PR attribution/order/CI/landing remain blocked. No commits, branches, merges, or pushes created.

2026-09-07_#215(9437f50c1a), HUMAN PLAN-REVIEW HOLD: Owner requires one plan-review round BEFORE further GitHub sync overhaul implementation. Review-ready short proposal: ai_docs/github-sync-plan.md; longer analysis: ai_docs/github-sync-design.md. Previously produced uncommitted prototype is frozen, unapproved, and excluded from PR landing work. Any approved implementation must build on Brian PR17 migration transactions and PR21 checked/local-write concurrency safeguards, not replace them. Proposed choices requiring review include local-label AND selection, fail-closed missing history, prose insertion-group ordering and Markdown awareness, per-issue ancestor layout, and remote no-CAS limitations. GitHub access now works after the owner restarted with internet enabled. Brian/unormal has ten cumulative PRs: https://github.com/rrnewton/minibeads/pull/15 through https://github.com/rrnewton/minibeads/pull/24, explicitly ordered 15->16->17->18->19->20->21->22->23->24. Review agents use clean detached worktrees; main prototype remains untouched. Fork CI runs were awaiting approval and are being approved/reviewed. Global proxy skill installed separately at ~/.codex/skills/github-with-proxy/SKILL.md with account-level discovery link; it distinguishes proxy setup from session internet permissions.

2026-09-07_#215(9437f50c1a): Live-session update: corrected PR24 tip 25b963ff0c (review depth 226) is pushed and final-head CI run 34158336503 is running. Independent reviews and all 203 debug tests (stable and Rust 1.87), strict Clippy, and audit pass. Landing is atomic through PR24 to avoid earlier audit-red states; original commit order is preserved. The review plan ai_docs/github-sync-plan.md now explicitly distinguishes GitHub hash-based change detection/no retained common text from separate timestamp-based Markdown/JSONL sync. Prototype remains frozen; no further overhaul implementation until the human plan-review round.

2026-09-07_#215(9437f50c1a): LANDING VERIFIED remotely at 2026-09-07_#227(19fc71cfc1). All Brian/unormal PRs #15–#24 are MERGED; every reviewed head is an ancestor of origin/main. Correction 25b963ff0c passed all seven CI jobs in run 34158336503 and 203 debug tests on stable and Rust 1.87. All ten codex/review-* branches archived as verified remote .v1 tags; fork branches retained. Clean landing workspace is /tmp/minibeads-landing-01a07d50; this original main working checkout remains on its old base with the frozen, unapproved prototype and is intentionally not pulled over. Global github-with-proxy skill installed at ~/.codex/skills/github-with-proxy/SKILL.md with ~/.agents/skills discovery link. BLOCKED ONLY ON HUMAN PLAN REVIEW: review ai_docs/github-sync-plan.md; no more overhaul implementation until that round is complete.

2026-09-07_#215(9437f50c1a): FINAL REMOTE HANDOFF verified at main 2026-09-07_#229(6e30bdc7a8). All ten Brian PRs #15–#24 remain merged; no open unormal PRs remain. Final main CI34159742387 passed all seven jobs, including macOS/Windows/Linux, after the separate shared-binary stress-harness fix. Routine validation is now 208 tests; native harness passes on stable and Rust 1.87. Tracking/review evidence committed in main. Global github-with-proxy skill validated in both account skill-discovery roots. Ready for your human review of ai_docs/github-sync-plan.md; overhaul stays frozen. Original working checkout and its unapproved prototype are preserved on the old base; isolated landed checkout /tmp/minibeads-landing-01a07d50 is at current main.
