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
import), so `issue_to_markdown` escapes every line that would open a hunk:
after any run of backslashes, seven or more `<` then a space or the end of the
line. It gains one leading backslash, and reading removes one, which is exactly
reversible and renders the same in Markdown. A whole-database read such as
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

The re-merge idempotence property and the review found silent corruption in
`prose_merge.rs` (PR #26). An overlapping edit is refined to word level by
aligning each side to the ancestor independently. When a word repeats, the two
sides can anchor it at different occurrences, and the merge emitted an
insertion twice or dropped both copies of a once-deleted word, within one
paragraph as well as across paragraphs. A word-level merge is now accepted only
if every content word occurs in the result between its counts on the two sides
(`require_word_counts_within_sides`); otherwise it is a `CompetingEdit`
conflict (`InconsistentWordCounts`), which is fail-closed. A sound merge never
violates this bound: each word count is either one side's or, when both
changed it, a count between theirs. Regression tests:
`word_merges_never_duplicate_a_shared_insertion`,
`word_merges_never_drop_both_copies_of_one_deleted_word`, and the seeded
`property_merging_a_subsumed_side_returns_the_superset`.

A paragraph piece carries the blank line that separates it from the next one,
except the last. A final paragraph that one side appends after therefore no
longer matched itself, and the edit widened into a spurious overlap. When no
input ends in a line break, all three now get a common separator that is
removed afterwards (`a_final_paragraph_that_stops_being_final_still_aligns`).

## Tests and evidence (2026-10-03_#232(5742073e8e) plus uncommitted review fixes)

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
- `prose_merge` adds a 5000-seed property: merging a side whose changes are a
  subset of the other's either returns the superset or refuses.
- `format` checks that quoted conflict markers round-trip through the escape;
  `merge_driver` checks non-UTF-8 byte hunks and marker growth.
- `tests/merge_driver.sh` runs real `git merge`s through `mb merge-driver
  install`:
  - routing, including a nested database;
  - a clean merge of disjoint edits, labels and comments from both branches;
  - comment deletions: against an added comment, against an untouched comment
    file, and against an edit (kept and reported);
  - competing titles that produce exactly one three-line hunk, after which `mb`
    refuses the unresolved file and a mechanical "take theirs" resolution parses;
  - the `--stdout` preview.
- Mutation check: each deliberate breakage below was applied alone and made
  the unit suite fail (number of failing tests in parentheses):
  - keeping a comment deleted on one side and unchanged on the other (3);
  - letting a deletion beat an edit (2);
  - dropping theirs-only comments (5);
  - reversing the comment view order (8);
  - having ours win the labels (2);
  - taking ours' `updated_at` (4);
  - letting ours silently win scalar conflicts (15);
  - leaving `closed_at` out of the status hunk (2);
  - keeping `closed_at` on a reopened issue (4);
  - merging the frontmatter views with ordinary diff3 (1);
  - removing the word-count check (4);
  - removing the final-separator padding (2);
  - having theirs win line-level diff3 conflicts (6);
  - never growing the markers (1);
  - not escaping quoted markers (1);
  - leaving ours in place for non-UTF-8 input (1).

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
