# GitHub sync overhaul — plan for human review

**Status: awaiting your review. Do not implement or land this overhaul yet.**

Snapshot: `2026-09-07_#215(9437f50c1a)`. Tracking: **minibeads-37**.
This is the short proposal; [the design-space analysis](github-sync-design.md)
contains the longer prose-merging and Unison discussion.

## What the existing model actually does

Verified against Brian's reviewed stack plus its corrections
(`2026-09-07_#226(25b963ff0c)`):

- **GitHub sync is not timestamp-only:** it stores separate last-seen local and
  remote content hashes, detects which side changed, and pushes/pulls whole
  synchronized fields. It has no retained common text for a real three-way
  content merge. Concurrent changes generally conflict, even when disjoint.
- Missing-state initialization and inherited-divergence repair can still push
  local fields over different remote content. Those are concrete overwrite
  risks to remove in the approved overhaul, not semantics changed by the narrow
  PR-review corrections.
- Explicit single/multiple local issue IDs already work. Label selection does
  not yet exist. `--since` checks only local timestamps.
- **Markdown/JSONL `mb sync` is a separate protocol:** it still uses timestamp
  precedence and reports equal-time content conflicts. Brian's fixes add locking
  and prevent migration from falsely making older Markdown look newer; they do
  not turn this protocol into a three-way merger.

## Scope and sequencing

1. **Done:** review and land Brian's existing stacked fixes in order:
   [15](https://github.com/rrnewton/minibeads/pull/15) through
   [24](https://github.com/rrnewton/minibeads/pull/24). All ten are merged via
   `19fc71cfc1`, atomically preserving original commit order rather than landing
   audit-failing intermediate heads. The corrected stack passed all seven CI
   jobs and 203 debug tests on both stable and Rust 1.87.
   A subsequent stress-harness initialization fix brings routine validation to
   208 tests; final main `6e30bdc7a8` passes all seven jobs in
   [CI run 34159742387](https://github.com/rrnewton/minibeads/actions/runs/34159742387).
2. Complete **one human plan-review round on this document**.
3. Incorporate the review, then implement the approved scope against the landed
   code—not against the old prototype's base.

An uncommitted prototype was produced before the review-first clarification.
It is **frozen, unapproved, and not part of Brian's landing stack**. Its tests
are experimental evidence, not approval to ship its design. In particular,
Brian's migration transactions (#17) and checked GitHub writes (#21) must be
preserved; they were absent from that prototype's starting revision.

## Proposed user-visible behavior

- Keep `mb github sync ISSUE_ID...` for one or several linked local issues.
- Add repeatable `--label`: require **all supplied local labels**, intersected
  with explicit IDs. Empty selection must mean no work, never "sync all".
- Retain `--since` only as an explicitly local-change filter; warn in help that
  it cannot discover remote-only changes.
- Reconcile **title, description, and GitHub open/closed state** against a
  recorded common content ancestor. Never choose a winner by modification time.
- Keep richer local statuses when GitHub stays open. Local labels, priority,
  assignee, claims, and dependencies remain local; comment reconciliation stays
  a separate protocol rather than silently gaining new merge semantics.
- If history is missing, matching copies establish a baseline; divergent copies
  require explicit resolution. No bootstrap or "repair" path may overwrite one
  side merely because the other side was visited first.
- Pull-only stays remote-read-only and refuses to discard local edits unless
  explicitly forced. A normal bidirectional force flag must not hide conflicts.

## Prose-merging recommendation

Use deterministic, ancestor-relative edits, not an LLM rewrite and not blind
concatenation of entire descriptions.

1. Match unchanged Markdown blocks while retaining their original text.
2. Merge disjoint paragraph edits; keep independent paragraph additions from
   both replicas, including additions at the same paragraph boundary.
3. Preserve each replica's insertion-group order. Use a stable content-derived
   ordering for genuinely independent groups; conflicting ordering constraints
   must be surfaced rather than silently rearranging text.
4. Refine overlapping **plain-prose** changes to words/whitespace so changing
   different words on a long line can merge. `foo → bar` versus `foo → baz`,
   delete-versus-edit, and ambiguous repeated-text alignment remain conflicts.
5. Treat code fences, tables, and other structured blocks conservatively. Their
   presence elsewhere must not prohibit merging unrelated prose paragraphs.
6. Bound memory/work. Report a work-limit or unsupported-structure result
   distinctly from an actual competing edit; preserve all inputs for resolution.

The prototype's local-first paragraph union is **not the proposed final ordering
contract**. Replays after partial success, identical insertions, intentional
repetitions, and paragraph moves need explicit tests before choosing an algorithm.

## Ancestors, concurrency, and recovery

Use **project-local, git-ignored `.minibeads/sync_ancestors/`**, honoring the chosen
database root (including legacy `.beads`), rather than a global home-directory
store. Follow Unison's last-common-state model; its archive alone is not a
native semantic prose merger.

Recommended layout: versioned per-issue records keyed by a validated canonical
remote identity, containing common synced fields and the metadata needed to
interpret that checkpoint. This avoids rewriting every issue's history for a
one-issue sync. Decide the exact schema only after reviewing migration and
comment-state compatibility against Brian's landed implementation.

- Reuse #17's local transaction/recovery machinery and #21's snapshot checks,
  locking, and merge-only-changed-state behavior. Do not replace them with a
  last-writer-wins whole-state save.
- Recheck local contents under the lock at the actual local write; timestamps
  alone cannot detect edits. Keep network waits outside long-held local locks.
- Revalidate remote fields before writes and verify convergence before advancing
  the common ancestor. Persist updates atomically, with recoverable evidence for
  partial operations; a failed later issue must not erase earlier progress.
- Carry ancestry correctly through ID/prefix/layout migrations and relinking.
- Read legacy hash-only state conservatively; reject corrupt/future schemas
  rather than interpreting them as an empty database.
- GitHub's current `gh` issue-writing interface has no atomic compare-and-swap
  contract here. Narrow that race, retain evidence, and document the remaining
  limit; do not promise lossless concurrent writes that the remote API cannot
  guarantee.

## Acceptance gates after approval

- Single-ID, multiple-ID, label intersection, empty-scope, and remote-only-change
  tests; unrelated issue/history records remain untouched.
- Three-way field/prose tests covering independent additions, same-word
  conflicts, Markdown blocks, Unicode/line endings, repeated text, and ordering.
- Concurrency and failure-injection tests: local edits without timestamp changes,
  concurrent scoped syncs, migrations, failed writes/checkpoints, stale-history
  retries without duplicate paragraphs, and remote changes during a sync.
- Preview makes no issue/comment/ancestor writes; conflicts preserve inputs.
- New tests run in debug `make validate` and CI; review the resulting patch before
  landing. No live GitHub stress writes without a disposable authorized repo.

**Please review:** the local-label AND semantics, conservative missing-history
behavior, prose ordering/structure policy, per-issue ancestor layout, and the
explicit remote-race limitation. Approval here gates implementation, not just
the final merge.
