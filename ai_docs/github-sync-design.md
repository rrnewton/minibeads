# GitHub synchronization: common ancestors and English prose

Status: owner-approved design, implemented for review on 2026-09-15.
Tracking: `minibeads-37`. Implementation base:
`2026-09-15_#229(6e30bdc7a8)`.

This document records the design space, the chosen model, and its limits. The
approved acceptance contract is in [github-sync-plan.md](github-sync-plan.md).
Brian's PRs #15–#24 landed before this work; the implementation preserves their
checked local writes, migration transactions, comment ancestry, and concurrent
state merging.

## Existing protocols are different

Before this change, GitHub synchronization stored last-seen local and remote
hashes in `github-sync-state.json`. Those hashes could identify one-sided versus
two-sided change only while the prior hashes represented agreement. They did not
retain the old title/body/state, so they could not perform a true three-way merge.
Missing state and inherited divergence also had code paths that selected local
content, creating overwrite risk.

`mb sync`, which reconciles Markdown and JSONL, is a separate interoperability
protocol. It still uses issue timestamps to choose a newer copy and reports
equal-time content conflicts. Its locking and migration timestamp safeguards
remain unchanged. The GitHub ancestor design does not silently alter that model.

## Chosen synchronization model

For each linked GitHub issue, retain the last contents known to be common:

- canonical GitHub issue identity;
- owning local issue ID;
- title;
- description/body;
- GitHub's two-state open/closed projection.

On the next synchronization, compare both replicas to that common ancestor:

| Local versus ancestor | Remote versus ancestor | Result |
| --- | --- | --- |
| unchanged | unchanged | no field write |
| changed | unchanged | push local result |
| unchanged | changed | pull remote result |
| changed | changed, disjoint | merge and update both replicas |
| changed | changed, incompatible | conflict; update neither replica nor comments |

If local and remote already agree, their common value can establish or advance
the checkpoint. If no content ancestor exists and the replicas differ, normal
sync conflicts. A legacy equal hash can prove a one-sided transition, but a
legacy divergent hash pair cannot manufacture missing content. The only explicit
winner operation is `--pull-only --force`, which chooses GitHub.

Local workflow statuses project to GitHub as open unless they are closed. When a
merged GitHub state is open, an existing local `in_progress`, `blocked`, or other
nonclosed status remains intact. GitHub does not gain control of local labels,
priority, assignee, claims, dependencies, design notes, or acceptance criteria.

## Per-issue archive

Records live below the resolved database root, whether `.minibeads`, legacy
`.beads`, or an explicitly selected path:

```text
sync_ancestors/
└── github/
    └── sha256-<canonical-GitHub-URL>.json
```

The content-derived filename avoids unsafe URL path components. The JSON record
contains `schema_version: 1`, the canonical URL, validated local ID, and typed
common fields. Reads reject malformed JSON, future/unsupported schemas,
noncanonical URLs, URL/key mismatches, and paths escaping the database. Invalid
history is never interpreted as an empty trustworthy ancestor.

Each write uses the existing database lock and recoverable `FileTransaction`.
Compare-and-store re-reads the expected record while locked, preserving a newer
checkpoint written by another sync. Equal concurrent results are idempotent.
ID rename, prefix migration, hash/numeric migration, and numeric repacking stage
the ancestor's `local_id` update in the same transaction as issue/comment/JSONL
ownership changes.

One file per remote issue means a one-issue sync does not rewrite unrelated
history. `github-sync-state.json` remains for comment pairings and legacy hashes;
it is not the common-content archive.

## Selection semantics

`mb github sync` accepts zero or more explicit local IDs and repeatable `--label`.
Every supplied local label is required. Explicit IDs, labels, and `--since`
intersect. A selection matching zero issues performs zero GitHub calls and zero
archive writes; it never falls back to “all.”

`--since` is deliberately a local timestamp prefilter. It makes large local
incremental passes cheaper but cannot discover an issue changed only on GitHub.
The CLI help and README say to omit it when remote-only changes matter.

## English prose merge design space

### Alternatives considered

1. **Whole-line diff3.** Mature and predictable for source code, but Markdown
   paragraphs are commonly one long line. Independent word edits then overlap,
   while two additions at the same paragraph boundary often become a conflict.
2. **Paragraph alignment with token refinement.** Match stable Markdown blocks,
   merge disjoint block edits, union independent additions, and refine only
   overlapping plain prose to words/whitespace. This is the chosen model.
3. **Full Markdown AST merge.** Better structural knowledge, but source-preserving
   round trips are difficult and parser normalization can itself rewrite prose.
   It remains a possible later layer for specific constructs.
4. **CRDT or operational transformation.** Excellent when every operation has a
   shared identity and causal history. GitHub and local Markdown expose snapshots,
   not a common operation log, so retrofitting one does not solve bootstrap.
5. **LLM semantic rewriting.** It can produce fluent text but is nondeterministic,
   hard to replay, and may silently change meaning. It is unsuitable as an
   automatic synchronization primitive. An explicitly reviewed conflict helper
   could be separate in the future.

### Recommended algorithm

The implemented merger has four typed outcomes: merged text, competing edit,
unsupported structured overlap, or bounded-work exhaustion. Failure retains the
ancestor/local/remote inputs in the merge result and does not publish a winner.

1. Use equality fast paths: equal replicas win; if one side equals the ancestor,
   choose the other without allocation.
2. Segment Markdown into paragraph pieces while keeping fenced blocks together.
3. Align each side to the ancestor with bounded longest-common-subsequence work.
   Repeated content that admits multiple meaningful alignments fails closed.
4. Apply disjoint edits from both sides in ancestor order.
5. For standalone additions at the same boundary, preserve each side's internal
   group order and order independent groups by content bytes, making the result
   deterministic and symmetric under swapping local/remote roles.
6. Coalesce identical insertion groups. If one side already contains the other
   group's exact prefix or suffix after a partial synchronization, keep the
   superset so retries do not duplicate paragraphs.
7. For overlapping plain prose, repeat alignment at word, whitespace, and
   punctuation boundaries. Thus different word edits in one paragraph can merge.
8. Report competing replacement, delete-versus-edit, ambiguous repeated
   alignment, and ambiguous insertion identity instead of guessing.
9. Treat overlapping fenced code, tables, lists, headings, links, inline code,
   HTML, and other structured Markdown conservatively. Structured content in an
   unrelated paragraph does not block a disjoint prose merge.

Input bytes, pieces per input, alignment cells, and compared bytes have separate
limits. Exceeding one reports which budget was exhausted; it is not mislabeled as
a semantic conflict. Fast-path equality is allowed even for inputs above the
expensive alignment limits.

### Ordering and repetition

“Keep both” does not by itself define order. Wall-clock time is unavailable and
local-first ordering changes if the replica roles are swapped. Stable byte order
for independent groups gives every replica the same result without coordination.
Within each group, author order is preserved.

Identical whole groups represent the same observed insertion and are coalesced.
Repeated paragraphs inside one newly inserted group, or partial shared groups
whose identity/order cannot be proven, remain conflicts. This preserves
intentional repetition instead of silently deduplicating it.

## Concurrency and recovery

Network requests are not made while holding the local database lock. Mutating
operations first acquire a project-local, per-GitHub-issue lease; different
issues remain independent, while two processes cannot publish the same comment
or race stale checkpoints for one issue. A sync:

1. acquires the issue lease, then reads the ancestor and comment/hash state
   together under the database lock;
2. fetches GitHub and computes a field plan;
3. refreshes GitHub immediately before each field mutation;
4. conditionally writes local fields only if the full persisted snapshot still
   matches, including edits made without changing timestamps;
5. refreshes both replicas after writing and performs bounded reconciliation
   retries when either side changed during the write;
6. reconciles comments only after field success, without network waits under the
   database lock;
7. rechecks the local issue under lock and stages the ancestor plus comment/hash
   state in one recoverable multi-file transaction.

Field conflicts happen before marker or comment mutation. Dry runs calculate the
same plan but write no issue, comment, marker, state, or ancestor. A successful
issue is checkpointed before processing the next issue, so a later fetch failure
does not erase earlier progress. If a remote write succeeds but local/checkpoint
work fails, the old ancestor remains; a replay merges from that old common state
and exact-superset handling avoids duplicate additions.

The GitHub CLI/API path used here has no atomic compare-and-swap for issue fields.
A remote edit can land between the final refresh and the write. Post-write
verification normally reconciles that edit through a bounded retry and advances
history only after convergence; an irreconcilable or continuously changing pair
keeps the old durable ancestor. The implementation cannot undo an overwrite
already accepted by GitHub, so it narrows rather than claims to eliminate that
remote API race.

## Comment protocol

Comments retain the existing ID-pair ancestry in `github-sync-state.json`.
Comment bodies do not use the prose merger. Pull-only never writes GitHub comments.
A remote counterpart that disappears can be an ID churn rather than a deletion,
so deleting the local paired comment requires `--force`. Any field conflict skips
all comment/marker changes for that issue.

## Verification evidence

At the implementation worktree on 2026-09-15:

- 44 focused prose tests cover additions, ordering, disjoint edits, token edits,
  Unicode/line endings, Markdown structures, ambiguity, replay, and all limits;
- 11 ancestor tests cover per-issue isolation, compare-and-store races, malformed
  and future records, URL/key validation, lease serialization, staged rollback,
  symlink containment, and recovery;
- mocked GitHub acceptance tests cover scoped selection, no-selection behavior,
  three-way merges/conflicts, missing/legacy history, dry-run, pull-only, richer
  statuses, local and remote write races, concurrent comment export, per-issue
  progress, relink ownership, and no-op stability;
- existing checked-write tests include a local edit with unchanged logical and
  filesystem timestamps;
- all five ID migration modes preserve ancestor ownership;
- debug `make -o purge validate` passes 296 tests, formatting, and Clippy;
- strict all-target/all-feature Clippy and whitespace checks pass.

Final CI and any landing evidence must be appended after review; no live stress
writes are required without a disposable repository explicitly authorized for it.

## References

- Unison manual: <https://github.com/bcpierce00/unison/tree/master/documentation>
- GNU diff3: <https://www.gnu.org/software/diffutils/manual/html_node/diff3-Merging.html>
- CommonMark 0.31.2: <https://spec.commonmark.org/0.31.2/>

The Unison lesson used here is architectural: retain last common state separately
from replicas, distinguish update detection from content resolution, and surface
conflicts. Unison is not claimed to understand English prose or automatically
union paragraphs; minibeads' deterministic prose policy is its own layer. The
current manual's “Reconciliation” section describes archives as recording each
path's state when last synchronized, while “Merging Files” separately explains
that a three-way merge needs the last synchronized **contents**, retained with
`backupcurrent`. That distinction directly motivates storing actual common issue
fields here instead of treating hashes or timestamps as merge ancestors.
