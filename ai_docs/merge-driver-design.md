# Git merge driver: three-way merge of issue and comment files

Status: implemented in 0.29.0 (`src/issue_merge.rs`, `src/diff3.rs`,
`src/merge_driver.rs`). It complements the GitHub common-ancestor sync in
`github-sync-design.md`: GitHub sync merges against a per-clone checkpoint,
while the git driver merges against the merge base that git computes.

## Contract

Git runs `mb merge-driver run %O %A %B --marker-size %L --path %P` for paths
that `.gitattributes` marks `merge=mb`. `%O`, `%A` and `%B` are temporary files
holding the base, ours and theirs. `%P` is the repository path, which git
shell-quotes itself. The driver writes the result over `%A` and exits 0 when the
merge is clean or 1 when it wrote conflict hunks. If any input is not UTF-8,
`%A` becomes one hunk holding the three versions byte for byte (reason
`NotUtf8`), so ours is never left in place looking merged. Every conflict and notice is
also printed to stderr as one line: `mb merge-driver: CONFLICT <path>:
<subject>: <reason>` or `mb merge-driver: NOTICE <path>: ...`.

`mb merge-driver install` sets `merge.mb.name` and `merge.mb.driver` (with
`--local` by default, or `--global`) and appends any missing routing lines to
`<toplevel>/.gitattributes`:

```
**/.minibeads/issues/**/*.md merge=mb
**/.minibeads/comments/*.json merge=mb
**/.beads/issues/**/*.md merge=mb
**/.beads/comments/*.json merge=mb
```

The leading `**/` also routes databases in subdirectories. The path decides the
merge kind. `<db>/issues/**/<id>.md` covers both the flat
and the sharded layouts. `<db>/comments/<id>.json` is a comment file. Any other
path is merged as plain lines.

## Issue files

Every input is parsed and then re-rendered. If the re-rendered text is not
byte-identical to the input (unknown YAML keys, hand formatting), the merge
never rewrites that file. If one side is unchanged it is taken byte for byte;
otherwise the file falls back to a line diff3 and a `TextualFallback` notice is
printed.

| Field | Rule |
|---|---|
| title, status, priority, issue_type, assignee, external_ref, claimed_at, claimed_until | Scalar: take the side that changed. Two different changes are a `BothChanged` conflict. |
| labels | Set: survivors keep base order and additions are sorted. Never conflicts. |
| depends_on | Merged per target ID like a scalar. A missing entry means "absent", so a change on one side against a removal on the other conflicts. With no base, additions merge and only different types for one target conflict (`NoCommonAncestor`). |
| created_at | Scalar, otherwise the minimum. |
| updated_at | Scalar, otherwise the maximum. |
| closed_at | Derived from the merged status, never a conflict of its own: set only when the status is closed (the scalar if possible, else the later close); cleared otherwise. When status conflicts, each side's `closed_at` travels inside the status hunk. |
| Description, Design, Acceptance Criteria, Notes | `prose_merge::merge_prose` against the base. With no base, the sides must be equal or it is a `NoCommonAncestor` conflict. |

When there is no merge base (both branches created the same ID), the file merges
only if both sides are identical. Otherwise the whole file becomes a single hunk.

### Precise hunks

The structured merge decides which fields conflict, and rendering puts the
markers exactly on them. It builds three views of the merged issue (base, ours
and theirs). The views agree on every merged field and differ only in the
conflicting ones, each holding its own side's value (a conflicting status
carries that side's `closed_at`). It then aligns the views' frontmatter, each
conflicting section and the comment JSON with `diff3::mark_view_differences`.
Unlike ordinary diff3, it turns every chunk that is not identical in all three
views into a hunk, even one changed on one side only, because a view
difference is by construction a conflict. Taking side S of every hunk
therefore reproduces view S exactly. A single conflicting title produces a
hunk of exactly three `title:` lines. Should the views of a conflicting field
ever render identically, the whole frontmatter or section is still forced into
one hunk so the conflict cannot pass as clean.

The labels are fixed (`<<<<<<< ours`, `||||||| base`, `=======`, `>>>>>>>
theirs`). The marker length is `%L`, grown past the longest marker-like line in
any input (a setext `=======` underline, a quoted conflict), so a resolver can
tell markers from content by length.

`markdown_to_issue` refuses a file that has a complete hunk outside a code
fence, so an unresolved merge is reported, never parsed as prose. Section text
that merely quotes a conflict must still be storable (`mb create`, GitHub
import), so outside fenced code `issue_to_markdown` escapes every line that
would open a hunk: after any run of backslashes, seven or more `<` then a space
or the end of the line. It gains one leading backslash, and reading removes
one, which is exactly reversible and renders the same in Markdown. Inside a
fence a backslash would display, so fenced lines are written as they are; the
writer, the reader and the hunk check follow fences by one rule
(`FenceTracker`), and the hunk check skips fenced lines. A whole-database read such as
`mb list` still fails on a genuinely conflicted file, naming the file, line and
escape. Skipping that file instead would silently drop the issue from
whole-database exports such as the JSONL sync. A file written by 0.28 with an
unescaped quoted conflict needs the same one-character fix.

## Comment files

A comment file is a JSON array keyed by stable comment IDs. Local comments use
`<issue>-c<hash>`; GitHub imports use `gh-<remote id>`.

The file is merged as a three-way set of IDs, consistent with what git does
when one side leaves the file untouched (git then takes the other side at the
tree level without running the driver):

- IDs added on either side are all kept, exactly once, sorted by
  `(created_at, id)`.
- An ID deleted on one side and unchanged on the other is deleted.
- An ID deleted on one side and edited on the other is kept with the edit and
  a `KeptEditedCommentDeletedOnOneSide` notice, so no edit is silently lost.
- An ID deleted on both sides disappears.
- If both sides edited the same ID:
  - Two snapshots of the same GitHub comment (same `source_id`): the later
    `updated_at` wins.
  - A local comment whose identity fields agree: its body goes through the prose
    merger.
  - Anything else conflicts. The JSON views are aligned so that the hunk covers
    only that comment's differing lines, usually the `"body"` line. All views
    share one order (by earliest `(created_at, id)` of each comment), so a
    resolution that mixes sides hunk by hunk never loses or repeats an ID.
- Unparseable, duplicate-ID or non-canonical input falls back to a line merge
  with a notice.

## Determinism fix

`Frontmatter.depends_on` was a `HashMap`, so an issue with two or more
dependencies was written in random order and every rewrite produced spurious
diffs and conflicts. It is now a `BTreeMap`, and the JSONL dependency export is
sorted too.

## Prose merger fixes found by the properties

The re-merge idempotence property, generated properties and two reviews found
silent corruption in `prose_merge.rs` (PR #26). Each side is diffed against
the ancestor on its own, so when words or paragraphs repeat the two sides can
anchor shared text at different occurrences, and a merge emitted an insertion
twice or dropped text that neither side dropped. Five rules now make it sound
on everything the properties generate:

- **Containment.** If one side lies on a shortest token edit path from the
  ancestor to the other (`d(A,X) + d(X,Y) = d(A,Y)` in insert/delete distance
  over the merger's tokens), some minimal edit script of Y makes every edit X
  made, so the merge is Y as it stands (`superseding_side`). This needs no
  alignment, so it holds however text repeats, and it covers replayed and
  cherry-picked edits. A token script cannot tell deleting text from
  rewriting it, so when the piecewise merge reports delete-versus-edit, that
  conflict stands.
- **Alignment agreement.** Of all longest common subsequences, `diff` can
  report the earliest or the latest; every other lies between them. The
  piecewise merge runs with both sides aligned earliest and with both aligned
  latest, and is accepted only when the two agree (otherwise
  `AmbiguousRepeatedAlignment`). Mixing the alignments across sides refused
  more merges without catching anything more in the generated cases. Each
  alignment alone corrupts merges that every other rule accepts
  (`each_alignment_alone_corrupts_some_merge`, found by running the
  generated properties under one alignment).
- **Merge containment.** The piecewise result M is accepted only when each
  side lies on a shortest token edit path from the ancestor to M
  (`d(A,M) = d(A,L) + d(L,M) = d(A,R) + d(R,M)`, else `MissesASideEdit`):
  the containment test above, applied to the result. Some minimal script of
  M then makes every edit of each side, edits made alike on both sides count
  once, and an edit that alignment carried over to an identical copy
  elsewhere is caught when the move costs edits
  (`merges_that_carry_an_edit_to_repeated_text_elsewhere_are_refused`, from
  generated seed 111328, where both alignments agree on the wrong copy). The
  check fails closed when its distances exceed the work limits; distances use
  Myers' greedy algorithm, whose work grows with the distance rather than the
  text length, so long sections with small edits stay well inside them
  (`long_sections_with_distant_small_edits_merge`;
  `token_distance_matches_the_quadratic_table` checks it against the
  quadratic table).
- **Word-count bound.** A word-level merge is accepted only if every content
  word occurs in the result between its counts on the two sides
  (`InconsistentWordCounts`). The bound is necessary for a sound merge, not
  sufficient; it is a cheap extra check.
- **Separator edits.** A matched paragraph covers its content and only the
  blank lines around it that both pieces share, so the separator a final
  paragraph gains when one side appends after it is an edit of those bytes
  alone, not of the whole paragraph
  (`a_final_paragraph_that_stops_being_final_still_aligns`). This replaces an
  earlier padding scheme that review showed could corrupt text.

A heuristic that took a side whose paragraphs were the other's minus an edge
deletion (`replay_includes_edge_deletion`) was removed: the generated
properties showed it silently dropped a paragraph, and containment covers the
replays it was meant for.

Every rule refuses rather than guesses, so the merger is conservative. Over
300,000 generated seeds per shape in both roles (2026-10-03_#233(de008f76e3)
plus uncommitted review fixes), 163,364 of 165,204 subsumed merges (98.9%) and
42,298 of 164,296 disjoint ones (25.7%) came out clean, with no wrong merge;
the rest were refused, mostly because a six-word vocabulary makes nearly every
alignment ambiguous. Real prose repeats far less.

## Tests and evidence (2026-10-03_#233(de008f76e3) plus uncommitted review fixes)

- Explicit cases in `src/issue_merge_tests.rs`:
  - a single-line title hunk, a hunk confined to one paragraph, and clean
    disjoint paragraph edits;
  - the label set, dependency change/remove conflicts, and a no-base
    dependency conflict;
  - close-time rules;
  - add/add with no base;
  - non-canonical fallback and the byte shortcut;
  - comments: union, a one-sided delete (both orders, matching git's byte
    shortcut), delete against edit kept and reported, deleted on both sides,
    newer import wins, a prose-merged comment body, a JSON hunk on the body
    line only, and every hunk-by-hunk resolution of comments whose creation
    times differ.
- Seeded property tests, 2500 generated cases each, over random overlapping
  edits to every field (13 edit kinds):
  - commutativity (same cleanliness, conflict fields, text, mirrored hunks),
    with `closed_at` set exactly when the status is closed;
  - identity when one side is unchanged (without the byte shortcut);
  - re-merge idempotence: field-exact, except that a re-merge may conflict in a
    prose section only where classic paragraph-level diff3 conflicts too (an
    oracle, not a tolerance), and its resolution must reproduce the first merge;
  - disjoint edits are always clean and contain both edits;
  - machine-resolvable hunks: each of ours, theirs and base gives that side's
    value for every conflicting field, identical values for every merged field,
    the same `created_at`, and a status-consistent `closed_at`;
  - comments: exactly the expected three-way set of IDs (no loss, duplicate,
    invention or resurrection) after all-ours, all-theirs and random mixed
    resolutions, time order kept, commutativity and idempotence.
- `prose_merge` generates cases from a six-word vocabulary with repeated and
  near-copied paragraphs and possibly adjacent changes, kept only when each
  side's changes form a minimal edit script both in the generator's tokens and
  in the merger's (so no two changes cancel out). Two properties of 20,000
  seeds each, in both roles: a subsumed side merges to exactly the superset,
  and disjoint changes merge to both applied; anything else must be a
  refusal, and the clean rates are bounded below. "Both applied" is the
  generator's text, or the same words and paragraphs with insertions that
  both sides made at one point in the other order: where text repeats, an
  equally short script of a side puts its insertion in another gap, and the
  merger orders concurrent insertions canonically. Such a result must also be
  exactly one side's change from the other side and the sum of both changes
  from the ancestor, in the merger's own token distance. Explicit regression
  cases pin the generated and reviewer reproducers.
- `format` checks that quoted conflict markers round-trip through the escape,
  inside and outside fences; `merge_driver` checks non-UTF-8 byte hunks, their
  marker growth, one-sided non-UTF-8 changes, and marker growth.
- `tests/merge_driver.sh` runs real `git merge`s through `mb merge-driver
  install`:
  - routing, including a nested database;
  - a clean merge of disjoint edits, labels and comments from both branches;
  - comment deletions: against an added comment, against an untouched comment
    file, and against an edit (kept and reported);
  - competing titles that produce exactly one three-line hunk, after which `mb`
    refuses the unresolved file and a mechanical "take theirs" resolution parses;
  - the `--stdout` preview.
- Mutation check (2026-10-03_#233(de008f76e3) plus uncommitted review
  fixes): each deliberate breakage below was applied alone and made the unit
  suite fail (number of failing tests in parentheses):
  - keeping a comment deleted on one side and unchanged on the other (3);
  - letting a deletion beat an edit (2);
  - dropping theirs-only comments (5);
  - reversing the comment view order (8);
  - having ours win the labels (2);
  - taking ours' `updated_at` (4);
  - letting ours silently win scalar conflicts (15);
  - leaving `closed_at` out of the status hunk (2);
  - keeping `closed_at` on a reopened issue (4);
  - merging the frontmatter views with ordinary diff3 (2);
  - removing the word-count check (2);
  - removing the containment rule (5);
  - removing the delete-versus-edit guard (1);
  - removing the merge containment check (1);
  - aligning earliest only (1), or latest only (1);
  - matching whole paragraphs with their separators (4), or content
    only (6);
  - measuring token distance with the quadratic table instead of Myers (1:
    the long-section test exhausts the work limit);
  - having theirs win line-level diff3 conflicts (6);
  - never growing the markers (2), or not for non-UTF-8 hunks (1);
  - not escaping quoted markers (1), or escaping inside fences (2);
  - leaving ours in place for non-UTF-8 input (1), or conflicting on a
    one-sided non-UTF-8 change (1).

## Limitations and follow-ups

- `.beads/github-sync-state.json` is tracked by git but has no merge driver, so
  concurrent GitHub syncs on two branches conflict textually.
- If one branch exports a local comment to GitHub while another branch imports
  it as `gh-<id>`, the merge holds the same text under two IDs. The comment
  ancestry in `github-sync-state.json` is what pairs them, and the merge driver
  cannot see it.
- `mb sync` (Markdown and JSONL) still chooses winners by timestamp. It is a
  separate protocol.
- A resolution that mixes sides between a status hunk and other hunks is still
  consistent (`closed_at` travels with the status), but a resolver that edits
  a hunk by hand can of course break the invariant.
- A description that starts with blank lines is not written canonically, so
  its file always takes the textual fallback.
- The prose merger is conservative over repeated text, and a single
  paragraph of about 500 words or more exhausts the word-level alignment
  budget, as it did before this work (minibeads-41).
