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
merge is clean or 1 when it wrote conflict hunks. Every conflict and notice is
also printed to stderr as one line: `mb merge-driver: CONFLICT <path>:
<subject>: <reason>` or `mb merge-driver: NOTICE <path>: ...`.

`mb merge-driver install` sets `merge.mb.name` and `merge.mb.driver` (with
`--local` by default, or `--global`) and appends any missing routing lines to
`<toplevel>/.gitattributes`:

```
.minibeads/issues/**/*.md merge=mb
.minibeads/comments/*.json merge=mb
.beads/issues/**/*.md merge=mb
.beads/comments/*.json merge=mb
```

The path decides the merge kind. `<db>/issues/**/<id>.md` covers both the flat
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
| depends_on | Merged per target ID like a scalar. A missing entry means "absent", so a change on one side against a removal on the other conflicts. |
| created_at | Scalar, otherwise the minimum. |
| updated_at | Scalar, otherwise the maximum. |
| closed_at | Derived, never a conflict: the scalar if possible, else the later of two values, else whichever value exists while the merged (or conflicted) status is closed. |
| Description, Design, Acceptance Criteria, Notes | `prose_merge::merge_prose` against the base. With no base, the sides must be equal or it is a `NoCommonAncestor` conflict. |

When there is no merge base (both branches created the same ID), the file merges
only if both sides are identical. Otherwise the whole file becomes a single hunk.

### Precise hunks

The structured merge decides which fields conflict, and rendering puts the
markers exactly on them. It builds three views of the merged issue (base, ours
and theirs). The views agree on every merged field and differ only in the
conflicting ones, each holding its own side's value. It then runs a line diff3
over the views' frontmatter and over each conflicting section, so the hunks
cover only the lines that actually differ. A single conflicting title therefore
produces a hunk of exactly three `title:` lines. There are two safety nets. If
the line diff3 merges cleanly something the field merge rejected, such as a
dependency modified on one side and deleted on the other where the lines happen
to line up, the whole frontmatter or section is forced into one hunk. And since
the views agree everywhere else, taking any one side of every hunk yields a
file that parses.

The labels are fixed (`<<<<<<< ours`, `||||||| base`, `=======`, `>>>>>>>
theirs`) and the marker length follows `%L`. `markdown_to_issue` refuses a file
that has a complete hunk outside a code fence, and `issue_to_markdown` refuses to
write one. An unresolved merge is therefore reported, never parsed as prose.

## Comment files

A comment file is a JSON array keyed by stable comment IDs. Local comments use
`<issue>-c<hash>`; GitHub imports use `gh-<remote id>`.

- IDs added on either side are all kept, exactly once, sorted by
  `(created_at, id)`.
- **Append-only:** an ID deleted on one side and present on the other is kept,
  with a `KeptCommentDeletedOnOneSide` notice. Only an ID deleted on both sides
  disappears. For this reason comment files have no "one side unchanged" byte
  shortcut: a one-sided deletion against an untouched file must not win either.
- If both sides edited the same ID:
  - Two snapshots of the same GitHub comment (same `source_id`): the later
    `updated_at` wins.
  - A local comment whose identity fields agree: its body goes through the prose
    merger.
  - Anything else conflicts. The JSON views are diffed so that the hunk covers
    only that comment's differing lines, usually the `"body"` line.
- Unparseable, duplicate-ID or non-canonical input falls back to a line merge
  with a notice.

## Determinism fix

`Frontmatter.depends_on` was a `HashMap`, so an issue with two or more
dependencies was written in random order and every rewrite produced spurious
diffs and conflicts. It is now a `BTreeMap`, and the JSONL dependency export is
sorted too.

## Prose merger fix found by the properties

The re-merge idempotence property found a silent duplication in
`prose_merge.rs` (PR #26). When an overlap spanning several paragraphs was
refined to word level, the token alignment could anchor on words that an
inserted paragraph repeats, and the insertion was emitted twice. Word-level
refinement now happens only when the overlap is a single paragraph on all three
sides; otherwise it is a `CompetingEdit` conflict, which is fail-closed. The
regression test is
`prose_merge::tests::overlap_spanning_paragraphs_never_duplicates_an_insertion`.

## Tests and evidence (2026-10-03_#231(7f1cb2c35d) plus uncommitted work)

- Explicit cases in `src/issue_merge_tests.rs`:
  - a single-line title hunk, a hunk confined to one paragraph, and clean
    disjoint paragraph edits;
  - the label set and dependency change/remove conflicts;
  - close-time rules;
  - add/add with no base;
  - non-canonical fallback and the byte shortcut;
  - comments: union, one-sided delete kept, deleted on both sides, newer import
    wins, prose-merged comment body, and a JSON hunk on the body line only.
- Seeded property tests, 2500 generated cases each, over random overlapping
  edits to every field:
  - commutativity (same cleanliness, same conflict fields, same text, hunks
    mirrored);
  - identity when one side is unchanged (without the byte shortcut);
  - re-merge idempotence (field-exact, with a bounded prose-overlap exemption
    explained in the test);
  - disjoint edits are always clean and contain both edits;
  - machine-resolvable hunks (taking either side gives that side's value for
    every conflicting field and identical values for every merged field);
  - comments: no ID lost, duplicated, invented or resurrected, time order kept,
    commutativity and idempotence.
- `tests/merge_driver.sh` runs real `git merge`s through `mb merge-driver
  install`:
  - a clean merge of disjoint edits, labels and comments from both branches;
  - a one-sided comment deletion that is kept and reported;
  - competing titles that produce exactly one three-line hunk, after which `mb`
    refuses the unresolved file and a mechanical "take theirs" resolution parses;
  - the `--stdout` preview.
- Mutation check: each of the following deliberate breakages was applied alone
  and made the suite fail (number of failing tests in parentheses):
  - dropping one-side-deleted comments (2) or theirs-only comments (4);
  - restoring the comment byte shortcut (2);
  - having ours win the labels (2);
  - taking ours' `updated_at` (4);
  - letting ours silently win scalar conflicts (13);
  - removing the paragraph-span guard (2);
  - having theirs win line-level diff3 conflicts (6).

## Limitations and follow-ups

- `.beads/github-sync-state.json` is tracked by git but has no merge driver, so
  concurrent GitHub syncs on two branches conflict textually.
- If one branch exports a local comment to GitHub while another branch imports
  it as `gh-<id>`, the merge holds the same text under two IDs. The comment
  ancestry in `github-sync-state.json` is what pairs them, and the merge driver
  cannot see it.
- `mb sync` (Markdown and JSONL) still chooses winners by timestamp. It is a
  separate protocol.
- Comment deletions do not propagate through merges, by design. A deletion that
  should stick has to be repeated after the merge.
