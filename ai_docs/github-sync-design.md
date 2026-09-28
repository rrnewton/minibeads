# GitHub synchronization: ancestors and English prose

> **Session update:** Internet access now works after the owner's restart.
> Network-blocked statements below describe the earlier offline investigation.
> Brian's live PR stack #15–#24 and its review fixes are now merged via
> `19fc71cfc1`, with green CI. The experimental prototype remains frozen and excluded.

> **Human-review hold:** Start with [the short proposed plan](github-sync-plan.md).
> The local prototype described here was produced before the review-first
> clarification and is frozen and unapproved. No further overhaul implementation
> or landing is authorized until that plan-review round is complete. The eventual
> implementation must integrate Brian's migration and checked-sync fixes first.

## Status, scope, and provenance

**Source snapshot: `2026-09-07_#215(9437f50c1a)`.** Date, git depth, and commit
identify the starting checkout inspected for this analysis, not a completed
integration. The local ancestry/prose implementation and its offline tests are
integrated in the uncommitted working tree; GitHub access and landing remain blocked.

There are three different kinds of claims in this document:

1. **AS-IS** describes the inspected starting revision of
   [src/github.rs](../src/github.rs), [src/main.rs](../src/main.rs), and the
   [README's GitHub section](../README.md#github-issues-sync).
2. **Local implementation handoff** records the parent's implementation report
   and selected working-tree observations, not validated completion or a promise
   about a released binary. Offline integration results are recorded below.
3. **Recommendation** specifies desired behavior, safeguards, and acceptance
   tests, including follow-up work beyond the first integration.

Only this document and `ai_docs/README.md` are deliverables of this research.
No source, private database, or tracking issue is edited by this work; it makes
no commit. No live GitHub API behavior, remote CI, or landing was verified.
External references below are **unverified literature pointers**, not quotations
from successfully retrieved sources. The implementation analysis is grounded in
local source, rather than those references.

### Local implementation handoff: offline tests validated

At the same source-snapshot timestamp, the parent reports the following changes
implemented locally. The starting-revision table below intentionally describes
the older behavior; it is not a description of this modified working tree.

- Store versioned full GitHub sync state atomically at project-local
  `.minibeads/sync_ancestors/github.json`; read the old
  `.minibeads/github-sync-state.json` only as a legacy fallback.
- Add an optional per-issue ancestor containing canonical synchronized fields.
  Schema version 1 validates stored ancestor hashes and rejects unsupported
  versions. Corrupt active archives do not trigger the legacy fallback.
- With no usable ancestor, divergent sides conflict conservatively. Neither
  ordinary sync nor unforced pull-only chooses a winner automatically; explicit
  `--pull-only --force` can choose GitHub. A legacy record proving a common hash
  permits a provable one-sided update, as described below.
- Remove inherited-divergence repair that automatically overwrites GitHub with
  local fields.
- Merge title as a scalar; merge body with deterministic paragraph preservation
  and disjoint token edits. Exact supported cases must be checked against the
  implementation and tests; its conservative limits are described below.
- Project status to GitHub's open/closed domain while preserving a richer local
  nonclosed status whenever the resulting GitHub state remains open.
- Add repeated `--label` filters matching **all local labels**, intersected with
  explicit issue IDs and the existing local `--since` filter.
- Publish through a named temporary file, flush and file `fsync`, atomic
  persistence, and Unix directory `fsync`. Storage-generated ignore rules and
  root ignore rules cover the namespace. Save the full archive map after each
  successful issue, with no archive write for an empty/all-conflict selection;
  preserve no-op sync timestamps.
- Field conflicts skip **all** issue/comment/marker changes for that issue.
  Refresh remote fields before writes and verify convergence with a post-write
  refresh before checkpointing. These checks do not eliminate the no-CAS race.
- Keep the existing conflict report and CLI exit behavior; this integration does
  not introduce a new nonzero-on-conflict exit contract.

The three-way scope is **title/body/open-closed only**. Labels, assignee, claims,
and mutable comment-body edits are not added to that field merge. Comments retain
their separate reconciliation protocol.

Canonical title/body values retain the existing
`trim_end_matches(['\r', '\n'])` equivalence because the issue-storage roundtrip
drops final line terminators. State is the GitHub open/closed projection. Thus
the ancestor is **not a byte-exact archive of Markdown issue files** and does
not preserve terminal-newline distinctions. The prose engine separately
preserves the input bytes it receives, after sync-layer canonicalization.

The archive is one project file, not one file per issue. A per-issue checkpoint
serializes the full map; sharding is a possible future performance optimization,
not current behavior. Do not infer pending-operation journals, retained backup
generations, cross-process state locking, or stronger race guarantees from the
atomic-file implementation.

### Delivered prose module: offline integration validated

The delivered [prose module](../src/prose_merge.rs) implements borrowed
equality/unchanged-side paths, paragraph alignment, and refinement into
whitespace-separated word spans. Punctuation remains attached to each word;
whitespace is represented losslessly. This is not a full punctuation tokenizer
or a Markdown parser.

Distinct standalone same-gap paragraph additions use **local-first order**,
followed by remote paragraphs not already present at that gap. Comparisons ignore
framing CR/LF separators, preventing a partial-write retry from duplicating a
paragraph merely because it now has a following paragraph. Repeated additions
and shared paragraphs that require reordering fail closed rather than silently
discarding repetitions or violating remote paragraph order. This
is deterministic for fixed local/remote roles, but **not role-symmetric** and
not the canonical-order follow-up recommended below. Deduplication is by exact
paragraph chunks in the competing additions, not by stable operation IDs;
repeated-intent and partial-failure cases still need explicit tests.

Plain-prose checks gate overlapping refinement and distinct same-gap union
across the body: markup/code-like punctuation, tabs, and indentation can disable
them. Disjoint paragraph edits are still a separate supported path. This does
not establish Markdown structural correctness. Conflicts distinguish overlaps,
delete/edit, ambiguous alignment/content, and work limits inside the module;
the integration currently reports a conflicting description field rather than
promising all internal reasons in the CLI.

Nontrivial merges are bounded by 1 MiB per input, 4,096 pieces per segmentation,
one million total LCS cells, and 16 MiB of charged comparisons. Borrowed fast
paths are exempt from those limits. The module adds no dependencies. The prose
implementation has **24 module tests passing**, including the parent's regression
for a reproduced partial-write paragraph-duplication bug. These contracts are
not published performance guarantees.

## AS-IS at the starting revision

The root README correctly identifies synchronized fields as title,
description/body, open/closed state, and comments. Labels, priority, assignee,
dependencies, and other minibeads metadata are not mirrored issue fields.
Linked issues have a GitHub URL in `external_ref`; unlinked issues are skipped.

| Area | Inspected behavior and source anchors |
| --- | --- |
| Change detection | `sync_linked_with_store`, `hash_local_issue`, and `hash_remote_issue` compare stored hashes of the title/body/status tuple, not modification timestamps. `synced_at` is metadata; `--since` is candidate selection only. |
| Stored history | `GithubSyncState` maps remote URLs to `GithubIssueState`: local ID, separate local/remote hashes, sync time, comment ID sets, and comment pairings. It contains no ancestor title or body text. Hashes detect changes but cannot reconstruct old prose. |
| Equality | `hash_fields` SHA-256 hashes NUL-separated field values after trimming terminal CR/LF characters from each field. Local status is hashed as its own spelling; remote status is normalized to open/closed. This is not yet the coordinated canonical status projection. |
| Ordinary conflict | Both sides changing to different tuples produces a conflict and leaves issue fields unchanged, even when they edited different fields. There is no paragraph, line, token, or Markdown merge. |
| Bootstrap | With no state, ordinary sync pushes local fields. Pull-only copies GitHub fields even without a prior record. These are the destructive bootstrap defaults the coordinated direction replaces. |
| Divergent saved state | If old local/remote hashes differ and neither side subsequently changed, ordinary sync pushes local fields to establish a common base. `link_issue_async` can save different hashes without reconciling fields, so an observed pair is not necessarily a common ancestor. |
| Pull-only | With an old state and changed local fields, a differing GitHub tuple is refused unless forced. No-state protection is missing at this revision. `--force` is not an ordinary-sync prose merge policy. |
| Scope | Explicit IDs intersect with `--since`, which checks local `updated_at >= cutoff`. Remote-only changes to an excluded issue are not discovered. Sync has no local-label filter yet; import's remote-label filter is a separate operation. |
| Comments | ID ancestry supports additions and deletion propagation. Marker comments containing `MB_DO_NOT_SYNC` are excluded. Conservative local-deletion safeguards and force handling exist separately from issue-field reconciliation. This is not a generic three-way comment-body merge. |
| Conflicts and side effects | Field conflicts do not prevent all comment work. Marker handling and comment reconciliation can still occur. `field_conflict` prevents advancing that issue's ancestry, including when a comment-deletion safeguard reports a conflict. |
| Persistence | `load_state` reads `github-sync-state.json`, defaulting only if absent. `save_state` uses direct `std::fs::write`, not atomic replacement. The sync loop saves the state map at the end; an earlier `?` error can leave completed writes without a saved checkpoint. |
| Remote observations | `GithubIssueHandle` caches reads and updates cached fields after successful CLI writes. `snapshot_for_state` refreshes for dirty comments, but can return cached issue fields after a field-only write. This is not authoritative post-write verification. |
| Dry-run | The sync routine skips mutations, marker writes, and state saving, but reads remote issues and previews work. The outer GitHub CLI dispatch still logs commands unless `--mb-no-cmd-logging` is used; “no writes anywhere” is not established. |

Ordinary push uses `gh issue edit` for title/body and a separate close/reopen
operation for status. Those commands, local issue updates, comment writes, and
the final state-file write are not one transaction. A hash common base therefore
does not imply transactional safety.

## Recommendation: a deterministic field merge contract

### Common ancestor versus last observation

Let `ancestor` be the last **agreed synchronized projection**, `local` the current
local projection, and `remote` the fetched GitHub projection. Keep last-seen
observations for diagnostics, but do not promote a divergent observation pair to
an agreed ancestor. Equal hashes alone do not supply missing text for diff3.

Use explicit types such as `CanonicalIssueFields`, `GithubState`,
`RemoteIssueIdentity`, `LocalIssueId`, `AncestorVersion`, and `MergeOutcome`.
`MergeOutcome` should distinguish clean/unchanged results, conflicts, missing
history, and operational failures rather than overloading strings or booleans.

Define and version canonicalization independently of serialization. The local
integration intentionally retains terminal-CR/LF trimming for title/body and
projects status to open/closed; it does not normalize internal CRLF or all
whitespace. Preserve the remaining Unicode, separators, and source text. Exact
pre-canonicalization content would need separate recovery retention; the agreed
ancestor does not supply it. Never treat paragraph reflow, Unicode normalization,
or collapsed whitespace as automatically meaning-preserving. A legacy hash must
be interpreted with its original status/equality policy, not assumed compatible
with every newly normalized digest.

For each scalar field, evaluate these rules in order:

| Relationship | Result |
| --- | --- |
| `local == remote` | Their common value; neither side loses an independent value. |
| `local == ancestor` | Use remote: only remote changed. |
| `remote == ancestor` | Use local: only local changed. |
| Otherwise | Both changed differently: a field conflict, or the body-specific merge below. |

Apply these decisions **per field**, not to the entire issue hash. A local title
edit and a remote description edit should combine without choosing an issue-wide
winner. Keep title scalar; automatically concatenating competing titles is not
a useful resolution.

Treat `GithubState` as an explicit open/closed enum. Every local nonclosed status
projects to open. When the merged projection is open, preserve an already
nonclosed local status, such as `in_progress`; when reopening a locally closed
issue, use the normal local open status. A merged closed result closes locally.
Do not churn a local workflow status simply because GitHub cannot represent it.

Plan all fields before applying any. The local integration now uses
issue-field all-or-conflict: if body conflicts, do not quietly apply a title or
status subset, and skip that issue's comment/marker changes as well. This is a
deliberate change from the starting revision. Field-granular commits would
require correspondingly granular ancestors; they are not implied by storing one
optional canonical-fields snapshot.

### Pull-only is not ordinary bidirectional merging

Pull-only never writes GitHub fields, comments, or marker comments. It may accept
remote changes when local synchronized fields are unchanged, or record an
already equal pair. If preserving local edits would require a merged value not
present on GitHub, leave those edits intact and report a conflict rather than
pretending the pull-only run established a common ancestor. The explicit
`--pull-only --force` choice may discard local synchronized values, with a clear
preview and preferably retained pre-write content. Do not silently reinterpret
`--force` alone as “prefer local” or “merge everything.”

## English prose: what can and cannot be merged

The objective is to preserve independently authored edits, not to generate
better English. A deterministic clean merge is a structural statement, **not a
proof that the resulting requirements are logically consistent**.

For example, starting with “Allow retries for failed requests,” one writer can
change “Allow” to “Never allow,” and another can change “failed” to “all.” These
edits may occupy disjoint token ranges, yet the combined policy may not be what
either author intended. Flag ambiguity rather than claiming semantic resolution.

### Alternatives and recommended progression

| Technique | Strength | Failure mode / recommended role |
| --- | --- | --- |
| Whole-field three-way selection | Simple, deterministic, protects conflicting scalar edits | Cannot combine concurrent body edits. Keep as the base contract and conservative fallback. |
| Line diff3 | Mature model: compare both edits against the same original line sequence | Long prose lines turn distant word edits into one conflict; wrapping creates noisy edits. Useful inside well-bounded plain-text regions, not sufficient alone. |
| Paragraph diff3 | Often separates independent additions and edits into useful units | Blank-line splitting alone misidentifies fenced code, lists, and quotes; repeated paragraphs and moves give ambiguous alignment. Prefer uniquely anchored prose blocks. |
| Token diff3 | Combines disjoint changes within one long paragraph | Word-only tokenization loses punctuation/spacing; repeated tokens and adjacent insertions make alignment ambiguous. Use lossless token spans and bounded refinement. |
| Markdown-aware blocks | Can keep code fences, list structure, and link definitions intact | Parsing/printing can reformat unrelated text; headings are not durable IDs. Use source spans, not an AST pretty-printer. |
| Unconditional union | Retains both input fragments | Can resurrect deleted content, duplicate requirements, or turn a correction into two contradictory statements. Restrict to well-defined independent insertions. |
| CRDT / OT | Resolves operation-level concurrency when participants share the protocol | GitHub issue edits provide snapshots, not a shared operation history. Not a drop-in solution for this sync boundary. |

Recommended progression for the body module:

1. Run equality and one-sided-change fast paths first, returning existing content
   without normalization or unnecessary allocation.
2. Identify uniquely anchored paragraphs or supported Markdown blocks in the
   ancestor. Preserve unchanged slices exactly; derive deterministic edit scripts
   for ancestor-to-local and ancestor-to-remote.
3. Combine nonoverlapping block replacements. Handle insertions with the explicit
   ordering rule below, not by appending whichever version was visited last.
4. Where both sides touch one supported prose block, refine using tokens that
   partition the original text completely: words, punctuation, and whitespace.
   Store UTF-8-safe source ranges. Apply disjoint edits only when alignment and
   boundaries are unambiguous.
5. Return a structured conflict for unresolved overlap, ambiguous matching,
   unsupported structure, or deterministic work-limit exhaustion. Preserve all
   three inputs and a reason; never truncate or fall back to a winner silently.

The observed paragraph-preservation/word-edit implementation covers part of this
approach, with explicit ambiguity/work-limit failures. It does not establish
Markdown parsing, punctuation-level refinement, or every law below. Those are
implementation review items and follow-up proposals.

### Insertion union, order, and repeatability

Concurrent insertions into **different ancestor gaps** retain ancestor order.
For example, a new paragraph after the introduction and another before the
conclusion both survive without reordering existing paragraphs.

For the **same gap**, distinguish complete independent paragraph additions from
competing inline text or replacements. The role-independent ordering and stricter
insertion-block deduplication below are **follow-up recommendations**, not the
observed local-first/chunk-deduplicating implementation:

- Identical insertion blocks at the same uniquely identified gap can coalesce
  once. Do not deduplicate equal paragraphs elsewhere in the document.
- Distinct, self-contained plain-prose paragraph insertion blocks may preserve
  both under an explicit union policy. Treat each side's ordered insertion block
  as a unit so its internal narrative order survives.
- Choose a stable, role-independent tie-breaker, for example lexicographic order
  of the exact UTF-8 insertion-block bytes. Preserve original separators and
  supply only a specified structural separator when required. Record the policy
  version; local-first, fetch order, wall-clock time, and hash-map iteration are
  not canonical ordering rules.
- Ordering is not intent. “First do X” and “then do Y,” list items, duplicate
  heading sections, or mutually exclusive requirements may require a conflict
  or an explicitly reviewed union. A basic prose algorithm cannot reliably
  recognize all such semantic relationships.
- Same-gap inline insertions, partial duplicate insertion blocks, and different
  rewrites of the same ancestor paragraph conflict by default. Keeping two
  alternative rewrites is useful in a conflict view, not as a silent replacement
  of one paragraph with two asserted truths.

Engine-input byte preservation and repeatability need separate tests; sync-layer
terminal-newline trimming is a separate, intentional equivalence. Require
`merge(ancestor, local, local) == local` and unchanged-side laws; require
role-symmetric results if adopting the proposed canonical ordering. The current
local-first policy deliberately does not satisfy that stronger law. After both sides accept
`merged` and the ancestor advances to it, the next sync must be a no-op, including
stable ancestry bytes and no duplicate comments.

Do **not** assume that same-gap sorting proves idempotence under stale history.
After a partial write, comparing `merged` against an original input with the old
ancestor may look like another insertion. Content-only snapshots cannot always
distinguish an intentional repeated paragraph from a replay. Recognize an exact
pending output during recovery if it was persisted; otherwise conservatively
conflict on ambiguous overlap. Do not use global paragraph-set union or fuzzy
deduplication to hide this problem.

### Delete/edit and ambiguous alignment

- Delete versus unchanged accepts the deletion; both deleting accepts it once.
- Delete versus an edit to the deleted range conflicts. Do not resurrect the
  original automatically, or discard the surviving writer's change.
- Insertion inside a concurrently deleted range conflicts. Boundary insertions
  require an explicitly tested anchor-ownership rule; adjacency alone is not
  proof of independence.
- Conflicting replacements of the same tokens conflict. Equal replacements of
  the same ancestor range may coalesce.
- Repeated headings, repeated identical paragraphs, moves, and reordering can
  produce multiple plausible edit scripts. A deterministic diff tie-breaker
  guarantees reproducibility, not that it chose the intended occurrence. Reject
  uncertain alignments rather than pretending to recover author intent.
- Whole-issue disappearance, unlinking, and an inaccessible remote are distinct
  from deleting body text. Do not infer an issue deletion from a failed fetch or
  absence in a filtered query.

### Markdown is structured source

Preserve heading hierarchy, fenced and indented code, nested lists, blockquotes,
reference definitions, HTML blocks, inline code, links, and whitespace-sensitive
line breaks. GitHub's tables and task lists extend CommonMark; a CommonMark-only
parser does not establish coverage of those structures.

Do not split on every blank line: a blank line inside a code fence is not a prose
paragraph boundary. Do not reflow Markdown, renumber lists, normalize URLs,
reformat code, or reserialize an entire AST as a merge side effect. In a first
implementation without a lossless Markdown parser, make complex/unsupported
blocks opaque for concurrent edits. Whole-block one-sided changes remain safe
under the field contract; overlapping changes can conflict.

If a parser is introduced later, use it to locate original byte spans and stable
structural boundaries, then splice source slices. Cross-block constraints still
matter: disjoint edits to a link definition and its uses can leave an invalid or
misleading reference. Parsing success is not semantic agreement.

### No silent LLM rewrites

Automatic synchronization must not call an LLM to summarize, reconcile intent,
fix grammar, or produce replacement prose. Clean output consists of selected
input slices plus narrowly specified structural separators. Report the
deterministic decisions when requested. A separately requested assistant or
external interactive merge may suggest a resolution for human review, but its
output must not become synchronized truth without explicit acceptance and
retaining the original conflict inputs.

## Unison: the relevant model and its limits

**This comparison is an offline conceptual account; exact manual/version and
preference behavior remain unverified in this research.**

Unison is a replica synchronizer, not a native English-prose merge engine. Its
archives record information about replica state observed at the last
synchronization so that update detection can distinguish unchanged, one-sided,
and conflicting changes. That archive information should not be conflated with
a repository containing the complete historical text of every file.

Unison can optionally invoke a configured **external content merge program**
for appropriate conflicting files. A three-way content merger needs the two
current contents and the old common content. Retained backups/current versions
(often discussed under `backup`, `backupcurrent`, and `merge` preferences) can
provide content needed by that workflow. Change-detection archives and retained
content backups have different roles. Availability and retention must be
configured and checked; metadata archives alone do not magically provide prose
ancestors. Exact placeholders, invocation rules, and failure behavior must be
verified against the chosen Unison release before prescribing a configuration.

The lesson for minibeads is architectural:

- Maintain private synchronization knowledge separately from normal issue
  fields; remember agreement, not just which side has the latest timestamp.
- Retain actual ancestor text if the merge policy requires old text. A digest is
  a useful equality check but not a replacement for content.
- Detect and surface conflicts rather than guaranteeing automatic resolution.
- If an external content merge is ever supported, retain ancestor/local/remote
  inputs, run it only by explicit policy, inspect its exit status and output,
  require review for unresolved/interactive results, and revalidate observations
  before publishing. Missing input, failure, or rejected output leaves the
  ancestor unchanged.

Do not claim that Unison normally unions conflicting paragraphs, understands
English intent, or supplies a transaction across a local issue file and GitHub.
Adopt its separation of update detection and optional content resolution, not an
imagined native semantic merger.

### Why not start with CRDTs or operational transformation?

A sequence CRDT uses stable operation/element identities and causal information
to converge under its protocol. OT transforms concurrent operations against
one another under a defined history/order model. Both can be effective when
editors participate in the protocol. Neither automatically prevents two authors
from writing contradictory English.

The `gh` issue interface exposes current title/body/state snapshots; GitHub's web
editor does not participate in minibeads' operation log. Inferring operations
from snapshots still needs alignment and an ancestor, and loses the author-time
identity needed to disambiguate repeats. A CRDT used only inside minibeads would
leave this boundary unresolved. Defer CRDT/OT integration unless all relevant
editing paths can preserve shared identities and causality. Ancestor-based
snapshot reconciliation is the smaller, auditable fit here.

## Project-local persistence and recovery

### Implemented local storage layout

`.minibeads/sync_ancestors/github.json` is the implemented **versioned full-state
file**, not a separate prose-only cache. Here `.minibeads` means the resolved
project database root; honor the existing storage selection instead of placing
state in a user's global home directory or blindly using the current directory.
Legacy database-root selection also needs an integration test.

Preserve comment IDs/pairings, legacy hashes needed for migration, local/remote
identity, and the optional canonical-field ancestor in the complete state.
Keep issue markdown as the normal issue source of truth: ancestor refreshes must
not churn it or its timestamps when no issue content changed.

The parent implements namespace ignore rules through Storage and the root
`.gitignore`; verify both new and existing databases in tests. Do not hide the
entire tracked issue database. No ignore-file change is made by this
documentation task. Runtime snapshots and any recovery content may contain
private issue text and need the same access controls as the database.

### Recommended schema and migration invariants

- Give the file an explicit format version and define the canonical-field/hash
  policy version. Unknown future versions or corrupt files must fail closed,
  not become an empty successful state.
- Bind each entry to a validated remote identity (host/repository/issue, or a
  stronger stable provider ID if available) and the local issue ID. The path is
  provider-specific, but `github.json` may contain multiple repositories. A
  URL-map entry must not be reused blindly after relinking or local ID reuse.
- Use existing canonical remote URLs deliberately. If aliases/renames are
  supported, verify identity before migration; never derive filesystem paths
  from arbitrary issue titles or URLs.
- `ancestor: None` means insufficient common content, not an empty issue body.
  Store a snapshot only after actual agreement under the documented projection.
- Read `github-sync-state.json` only when the new file is absent. Do not fall
  back on parse errors, unsupported versions, or permission failures. Once the
  new file exists, do not combine it with a potentially stale legacy file.
- Carry forward legacy comment ancestry without pretending its hashes are full
  text. Migrate all retained entries, not only the selected issue subset. The
  initial direction need not delete or keep writing the legacy file; one active
  writer format avoids split history.
- Keep deterministic serialization and stable timestamps on no-ops. Save the
  entire retained map, not a replacement containing only this run's candidates.

### Bootstrap without an ancestor

| Available evidence | Recommended normal behavior |
| --- | --- |
| Current local and remote canonical fields already agree | Record that agreed content without a destructive write. |
| No record, or legacy observations were divergent, and current fields differ | Conflict. Do not guess from timestamps, “local ownership,” or an empty body. |
| Legacy hashes prove an old common tuple; one current side still matches that hash | Permit the provable one-sided update using the legacy comparison rules; after confirmed agreement, store full canonical content. |
| Legacy common hashes exist, but both sides changed to different tuples | Conflict: hashes cannot reconstruct the old title/body needed for field or prose diff3. |
| Missing/corrupt/unsupported active state | Absence uses conservative bootstrap; corruption or unsupported versions require explicit recovery, not silent fallback. |

For divergent no-history input, pull-only also conflicts unless explicitly
forced. `--pull-only --force` selects GitHub, previews what local content would
be discarded, and seeds an ancestor only after the local write succeeds. For a
legacy common tuple with a local-only edit, unforced pull-only still must not
erase the local change: direction constraints take precedence over normal
one-sided push eligibility.

Hash migration must account for the old richer-status hashing and terminal
newline equivalence. Do not infer proof of an unchanged side across different
canonicalization policies. Equal current canonical projections are independently
sufficient to seed a new ancestor, even when legacy hashes differ.

Audit all entry points: linking two existing issues does not prove equality;
importing into a newly created local issue or publishing a newly created remote
can establish agreement only for fields actually copied and confirmed. No path
may reintroduce local-wins bootstrap by saving different observations as if they
were agreed content.

### Atomic file publication is necessary, not a transaction

The parent reports complete-state serialization into a named temporary file in
the same directory, flush and file sync, atomic persistence to `github.json`, and
Unix directory sync. Validate platform guarantees and failure handling. Atomic
rename alone promises neither durable storage after power loss nor a remote
transaction. An error must leave the previous valid file available whenever
publication has not occurred.

Serialize concurrent sync writers or use a generation-checked read/modify/write
protocol under an appropriate local lock. Atomic replacement by two writers
without coordination can still drop one writer's full-state update. Existing
per-operation storage locks should not be assumed to protect an entire network
sync. Reuse safe-Rust storage primitives without introducing nested-lock
deadlocks; compare local revisions again if locks are released during network IO.
Local locks do not coordinate independent machines or manual remote edits.

### Recommended execution and partial-failure policy

1. Select candidates, load validated state, and read the needed local/remote
   snapshots. Build a pure plan before side effects.
2. For a clean plan, revalidate relevant observations and preserve enough inputs
   to recover before the first irreversible write. Do not overwrite conflict
   inputs merely to reduce the number of conflicts reported.
3. Apply only planned operations; title/body, status, comments, local files, and
   state publication have distinct failure points.
4. Obtain confirmed post-operation projections. Promote a field ancestor only
   when both sides agree on that projection; do not use an optimistically edited
   cache as the sole confirmation.
5. Checkpoint each successful issue into the full retained state under the
   publication lock, so a later issue's failure does not discard earlier progress.
   Report successes, conflicts, and failures separately. A state-save failure is
   an operational failure even if content writes succeeded.

The parent now implements per-successful-issue checkpoints and pre-/post-write
remote-field refreshes. Durable pending-input retention and transaction journals
remain **recommendations**; do not infer them from checkpointing or verification.

| Failure window | Required safety / recommended recovery |
| --- | --- |
| Before any write | Keep the previous ancestor and report the failed operation. |
| Title/body accepted, status or comments fail | Do not announce complete convergence. Keep the old field ancestor unless the whole field projection was independently confirmed; preserve evidence of completed operations. |
| Remote write succeeds, local write fails | Keep the old committed ancestor and fetched/pre-write inputs. On retry, fetch again; do not repeat a blind local-wins overwrite. |
| Both content writes succeed, state save fails | Report content success plus checkpoint failure. A later run can seed agreement from equal current projections, but must still reconcile comments and other unfinished operations. |
| Comment addition succeeds but response/checkpoint is lost | Verify remote identity/content before retry. Existing body-based suppression is not a general exactly-once guarantee, especially for deliberately identical comments. |
| One issue succeeds and another fails | Preserve a valid checkpoint for the successful issue; retain unselected/failed/conflicting entries. |
| Process dies with a temporary state file | Trust only the last published valid file. Do not adopt a temporary file solely because it is newer. |
| Active file is corrupt or missing after prior synchronization | Use a validated retained generation through explicit recovery, or conservative bootstrap if truly absent; never infer a safe winner from timestamps. |

A useful follow-up is a bounded **pending-operation record** with operation ID,
input fingerprints/content, intended result, and known completed steps, persisted
before side effects. It could live in the versioned state or an explicitly
versioned recovery journal. It is separate from the committed ancestor. A
retained last-good state generation protects metadata; pre-write input retention
protects user edits. Neither is currently promised by the atomic-file direction.
Without durable pending evidence, ambiguous retries must remain conflicts.

Keep retention bounded and explicit. Garbage collection must not remove content
referenced by unresolved conflicts or pending operations. Do not blindly replay a
pending write: compare fresh observations with its exact input/output first.
If the same projected output is already present, finish the checkpoint; if
anything unexpected changed, recompute or stop for review.

### Remote race limitation

The inspected `gh issue edit` flow does not enforce a compare-and-swap condition
on the fetched body. Another GitHub writer can change an issue between read and
write, and a successful write can replace content that minibeads never observed.
Pre-write refetch narrows this window; post-write refetch can detect some
divergence, but neither proves that no unseen edit was overwritten. Remote
timestamps are not a lock. Do not claim race-free or universally lossless sync.

Investigate an actual server-enforced conditional update only against verified
API documentation and tests; do not assume that an ETag or `If-Match` works for
GitHub issue updates. If such an API is unavailable, document best-effort
behavior, stop on detected races, preserve observed inputs, and recommend a
single synchronization writer or explicit review during active concurrent edits.

## Filtered scope and dry-run

The coordinated sync candidate set is:

`linked local issues ∩ requested IDs ∩ all requested local labels ∩ local since cutoff`

An omitted filter imposes no restriction. Repeated `--label` means AND, not OR,
and selects on **local** labels; it neither imports remote labels nor writes
labels to GitHub. `--since` remains based on local `updated_at`, not a remote
watermark or merge winner rule. Periodically run a sufficiently broad sync to
discover remote-only changes. The repository override configures GitHub access;
do not silently advertise it as another local candidate filter.

Unselected issues must produce no remote mutations, marker writes, comment
deletions, or ancestor refreshes. Preserve their state entries during full-file
saves and legacy migration. Absence from the candidate set is not deletion;
recovery/garbage collection must not prune it. Validate duplicate links and
relinking independently of filters so scope cannot cause identity aliasing.

Recommended dry-run computes the same field/prose plan, conflicts, filter scope,
and direction restrictions as an actual run against the same observations. It
must not publish/migrate ancestry, create `sync_ancestors`, write issue/comment
content, add markers, update backups, or mutate GitHub. Include proposed
bootstrap/migration and conflict reasons in output rather than performing them.
An actual run must refetch/revalidate; dry-run output is not a reservation.

For a strict byte-unchanged local preview, audit CLI command logging and storage
initialization/lock behavior as well as the sync function. At the inspected
revision, `--mb-no-cmd-logging` avoids command-history appends, but no broader
strict dry-run guarantee was tested. Distinguish transient read locks from
persistent data changes in the documented contract and in tests.

## Validation and parent handoff

### Evidence at this snapshot

**Working-tree validation at `2026-09-07_#215(9437f50c1a)`: 208 tests pass**:
64 library tests, 137 binary tests, and 7 shell e2e tests. This includes 24 prose
tests, 27 injected-GitHub sync tests, and a CLI selection-parsing regression.
The parent reproduced and fixed duplicate paragraph insertion during replay
after a remote write but before local checkpointing; both prose and sync-layer
regressions cover this case. No tests use live GitHub.

**Local validation is now green.** The first runs exposed baseline formatting
differences in `src/format.rs` and `tests/migration_description_truncation.rs`,
plus strict-Clippy findings for eight positional comment-reconciliation arguments
and a needless `Vec` in an ordering test. A follow-up applies only the required
formatting, uses an array in that test, and groups the existing comment flags in
`CommentDeletionOptions` without changing deletion behavior.
`make -o purge validate` now passes all build, test, formatting, and Clippy gates.
`cargo clippy --all-targets --all-features -- -D warnings` also passes without
lint allowances. The standalone description-migration integration test passes
separately, for **209 passing tests observed: 208 routine + 1 standalone**.
Diff whitespace checks pass.
The baseline test count was 156; the implementation adds 52 passing tests.

The parent also flags cached candidate `51de629` as conflicting with a test.
Do not assume that candidate is safe to apply or land; validate the actual final
working tree. This research neither applies it nor verifies its remote status.

The final rerun uses `make -o purge validate` to skip the purge script's traversal
of private legacy database directories; the build, tests, and format gate are
unchanged. All iterative builds use debug mode. No commits, pushes, merges, or
live CI checks succeeded or are claimed. The offline tests are part of the
existing binary suite run by both the validation target and GitHub CI.

### Acceptance matrix for implementation owners

| Area | Cases to make permanent in unit / mocked integration tests |
| --- | --- |
| Field selection | Unchanged inputs; identical concurrent edits; one-sided changes; local title plus remote body; conflicting titles; body conflict does not partly commit fields. |
| Status | Every richer nonclosed local state versus GitHub open; preserve richer state during body-only pulls; close/reopen transitions; old hash policy migration. |
| Bootstrap | Missing record with equal and unequal fields; missing optional ancestor; divergent legacy hashes; common legacy hash with either one-sided change; both changed without text; corrupt/future version refuses fallback. |
| Direction | Divergent no-history normal and unforced pull-only conflict; explicit forced pull selects GitHub; local-only edit is not destroyed by unforced pull-only; no pull-only remote marker/comment writes. |
| Prose | Different-paragraph edits; long single-line paragraphs with disjoint token edits; identical and distinct same-gap insertions; preserve insertion-block order; repeated paragraphs; empty text; no final newline; CRLF; Unicode and punctuation/spacing preservation. |
| Conflicts | Delete/edit, delete/insert, overlapping replacements, ambiguous repeated-token anchors, moves/reordering, unsupported Markdown blocks, semantic contradiction examples that must not be advertised as semantically resolved. |
| Markdown | Blank lines inside fences, nested lists, code spans, reference definitions, tables/task lists, hard breaks; unchanged bytes survive; no unrelated reformatting. |
| Algebra and replay | Equality and unchanged-side laws; explicit local-first same-gap results; role symmetry only for any future canonical-order policy; deterministic serialization; second sync makes no writes; retries after one side already accepted a union do not duplicate paragraphs. |
| Scope | Repeated labels require all labels; intersect labels with IDs/since; local versus remote labels; remote-only changes outside cutoff; unselected state retained; no-op empty selection. |
| Persistence | Legacy fallback only on absence; full-map migration retains comments/unselected entries; temp-write/rename failures; concurrent local state writers; stable no-op timestamps; ignored namespace in isolated test repositories. |
| Fault injection | Failure after each remote/local/state operation, late batch failure, remote mutation before/after write, stale cache verification, lost comment response, restart with a pending/temporary record. |
| Dry-run | Same planned conflicts/merges as non-dry-run on fixed inputs; zero mutating mocked `gh` calls; no data/state/migration/backup writes; logging and lock behavior explicitly accounted for. |

Use synthetic issues and mocked `gh` responses for repeatable tests; no live
repository is needed to test merge laws and failure windows. Bound worst-case
diff work with deterministic size/work limits and return a conflict rather than
unbounded allocation. Borrow source slices and iterator views where possible;
allocate the final merged body when necessary. No `unsafe` is justified by this
design. Any new manual regression must become a permanent test exercised by
both the normal validation target and CI, per project conventions.

Before marking integration complete, coordinate with the parent to verify:

1. The actual schema/version, atomic replacement/durability guarantees, ignore
   setup, legacy fallback, and all link/import/publish ancestry-seeding paths.
2. Whether the prose implementation merges replacements as well as insertions,
   its same-gap ordering, stale-baseline replay behavior, token boundaries,
   Markdown limitations, and conflict representation.
3. The exact pull-only/force semantics, normalized-status behavior, local-label
   filtering, and preservation of excluded state and comment ancestry.
4. Which recovery checkpoints, remote revalidation, and strict dry-run safeguards
   are implemented versus deferred. Do not describe planned journals/backups as
   existing functionality.
5. Fresh implementation test results and user-facing README/CLI help accuracy.
   Update the transient timestamp when those claims are rechecked. No local
   result by itself establishes GitHub landing or CI status.

## Sources and literature pointers

### Verified locally

- [GitHub sync implementation](../src/github.rs): `GithubIssueState`,
  `sync_linked_with_store`, `update_state_entry`, `load_state`, `save_state`,
  `hash_fields`, and `GithubIssueHandle` at the source snapshot above.
- [CLI implementation](../src/main.rs): `GithubCommands::Sync`, GitHub command
  dispatch, `get_storage`, and `log_command` at that snapshot.
- [Root README](../README.md#github-issues-sync): user-visible sync fields,
  comment/marker handling, and hash-history terminology.
- [Prose merge module](../src/prose_merge.rs): local working-tree observations
  during the implementation handoff, not part of the starting commit or a
  claim that GitHub landing or release validation has completed.
- [Validation target](../Makefile) and
  [purge script](../purge-bd-upstream.sh): debug build/test workflow and the
  private-database side effects of full validation.

### Unverified external literature pointers

The following titles/links identify material to verify later, not sources
whose text was successfully checked in this task. No quotations or claims
about exact current API/configuration behavior depend on them.

- **Unison File Synchronizer User Manual**, especially archives/update
  detection, backups/current-version retention, and external merging:
  <https://www.cis.upenn.edu/~bcpierce/unison/download/releases/stable/unison-manual.html>.
  The `stable` target is mutable; verify a concrete release before citing
  preference semantics or example commands.
- **GNU Diffutils manual, “Merging From a Common Ancestor” / diff3 merging**:
  <https://www.gnu.org/software/diffutils/manual/html_node/diff3-Merging.html>.
  Relevant to ancestor-relative edits and overlapping-change conflicts, not
  semantic English correctness.
- **CommonMark specification 0.31.2**:
  <https://spec.commonmark.org/0.31.2/>. Relevant to block/inline boundaries and
  whitespace-sensitive syntax; GitHub extensions require separate coverage.
- Sanjeev Khanna, Keshav Kunal, and Benjamin C. Pierce,
  **“A Formal Investigation of Diff3”** (2007): an offline bibliographic pointer
  for examining merge properties and limitations; publication details unverified.
- Marc Shapiro, Nuno Preguiça, Carlos Baquero, and Marek Zawirski,
  **“Conflict-Free Replicated Data Types”** (2011): an offline pointer for the
  protocol/causality model, not evidence that a GitHub snapshot is a CRDT.
- C. A. Ellis and S. J. Gibbs, **“Concurrency Control in Groupware Systems”**
  (1989): an offline pointer for operational transformation; details unverified.

**Bottom line:** ship deterministic ancestor-based field reconciliation with
conservative missing-history behavior first. Merge only well-defined prose
edits, preserve unresolved inputs, and explain the remote race limitation.
Treat stronger Markdown merging, external tools, and durable recovery journals
as separately validated work rather than hidden promises of “automatic merge.”
