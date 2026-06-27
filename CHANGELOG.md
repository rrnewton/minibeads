# Changelog

All notable changes to minibeads are recorded here. minibeads is a markdown-based
drop-in replacement for the [beads](https://github.com/steveyegge/beads) (`bd`)
issue tracker; the binary is named `mb`.

## [Unreleased]

### Added

- Added `mb github import`, which imports matching unlinked GitHub Issues as new
  linked minibeads issues while leaving already linked issues to normal sync.

### Fixed

- `make validate` now builds the debug CLI before running routine tests and
  excludes long randomized stress tests, which are available via
  `make stress-test`.

## [0.21.2] - 2026-06-27

### Added

- `mb show` now uses TTY-aware color and external markdown highlighting via
  `batcat`/`bat` when available, while keeping piped output plain.
- `mb github stress-test --adversarial` now mutates a batch of temporary
  GitHub-linked issues before syncing, including deliberate both-side conflicts.

### Fixed

- `mb show` now displays notes and comments alongside description/design/
  acceptance content in one markdown-formatted issue view.
- GitHub sync now removes any previously imported `MB_DO_NOT_SYNC` marker
  comments from local minibeads comments and keeps them out of sync state.
- GitHub sync now leaves conflicted field ancestry untouched instead of
  recording divergent local/remote hashes as if they were synced.

## [0.21.1]

### Fixed

- `mb version` now reports the crate version instead of a stale hard-coded
  value.

## [0.21.0]

### Added

- Linked GitHub issues now receive a GitHub-side marker comment containing
  `MB_DO_NOT_SYNC`, pointing back to the synced local minibeads issue. The marker
  is excluded from comment sync.
- Added `mb github stress-test -R owner/repo` for seeded randomized
  real-GitHub sync testing against a disposable repository, with `--steps` and
  `--seed` controls for aggressive reproducible runs. Use `--verbose` to print
  each live mutation and verification step.
- `mb github sync --verbose` now prints each underlying `gh` CLI call and its
  elapsed time to stderr.

### Fixed

- GitHub sync now routes issue operations through an async per-run `GithubStore`
  cache, avoiding redundant `gh issue view` calls while keeping sync execution
  sequential.
- GitHub sync now repairs inherited divergent sync-state entries by pushing the
  local issue fields to GitHub to establish a single common base. This fixes
  cases where a closed local issue was linked to an open GitHub issue but future
  syncs did nothing because both divergent hashes had been recorded as synced.

## [0.20.1]

### Fixed

- `mb github link` and `mb github publish` now use the same default per-issue
  output as `mb github sync`, instead of printing only the summary line.

## [0.20.0]

### Added

- **GitHub Issues sync via `gh`** (`mb github link`, `mb github publish`,
  `mb github sync`). A subset of minibeads issues can now be linked to GitHub
  Issues by storing the GitHub issue URL in `external_ref`; unlinked issues are
  ignored by GitHub sync.
- **Bidirectional field sync** for linked GitHub issues. Sync covers title,
  description/body, and open/closed state, with `.beads/github-sync-state.json`
  tracking the last synced local and remote hashes so local-only changes,
  GitHub-only changes, and both-sides conflicts can be distinguished.
- **GitHub comment sync** for linked issues. Local `mb comments add` comments
  are exported to GitHub, and GitHub issue comments are imported back into
  minibeads comments.
- **File-backed comments** (`mb comments add`, `mb comments list`) stored under
  `.beads/comments/`. Comments are timestamped discussion entries, separate from
  the issue's mutable `notes` field.
- **Linkage visibility** with `mb github list`, `mb list --github`, and
  `External ref:` in `mb show`.
- **More useful GitHub sync output**. `mb github sync` now prints one line per
  linked issue by default, supports `--quiet` for the previous one-line summary,
  and supports `--verbose` for per-issue details.

### Fixed

- GitHub comment parsing now tolerates `gh issue view --json comments` output
  where comments include `createdAt` but omit `updatedAt`.
- GitHub comment sync avoids duplicate comment export/import on retry after a
  partial sync failure.

### Verified

- Real manual QA was run against `rrnewton/minibeads` GitHub issue #9:
  link, local-to-GitHub title/body/comment sync, GitHub-to-local title/body/
  comment sync, `mb github list`, `mb show` external refs, and close sync.
  The QA record is stored as closed minibeads issue `minibeads-31`.

## [0.19.0]

### Added

- **Targeted search/replace edits** (`mb update --search TEXT --replace TEXT`).
  A safer alternative to overwriting a whole field with `--description`: find an
  exact substring and swap it for the replacement, mirroring the "aider"
  search/replace pattern that LLM agents handle far more reliably than wholesale
  rewrites or line-numbered diffs. This discourages agents from hand-editing the
  `.beads/*.md` files directly (which bypasses locking and corrupts state).

  ```
  mb update myapp-1 --search "old sentence" --replace "new sentence"
  mb update myapp-1 --field design --search "foo" --replace "bar"
  mb update myapp-1 --search "TODO" --replace "DONE" --replace-all
  ```

  By default the search text must match **exactly once**; a missing match, or an
  ambiguous one (multiple matches without `--replace-all`), is an error and the
  issue file is left untouched. `--field` selects which text field is edited
  (`title`, `description`, `design`, `notes`, `acceptance`; default
  `description`). `--search` is mutually exclusive with the wholesale field
  setters and with `--claim`. `mb quickstart` now steers agents toward this
  method. (minibeads-specific — upstream `bd` has no equivalent, so this is an
  additive extension.)

## [0.18.0]

### Added

- **Issue claiming for cross-machine coordination** (`mb claim`, `mb update --claim`).
  An agent working through the backlog can claim an issue before starting work:

  ```
  mb claim myapp-1            # claim for yourself (default window: 48h)
  mb claim myapp-1 --for 4h   # custom window (e.g. 4h, 2d, 90m)
  mb claim myapp-1 --team backend   # identity becomes 'host/team'
  mb claim myapp-1 --release  # return the issue to the backlog
  mb update myapp-1 --claim   # equivalent long form
  ```

  A claim records `assignee` (defaults to the machine hostname, optionally
  `host/team`), flips `status` to `in_progress`, and stamps `claimed_at` /
  `claimed_until` into the issue's markdown frontmatter.

  Claiming is a compare-and-swap: it fails if another worker holds an *active*
  claim. The lock is enforced across machines by committing and pushing the
  change — a losing `git push` is rejected, so the loser pulls, sees the issue is
  taken, and moves on.

  Unlike upstream `bd`'s claim (which has no expiry), a claim past its
  `claimed_until` is **stale** and may be reclaimed by anyone — so a crashed or
  abandoned agent never pins an issue forever. The default lifetime is 48 hours.
  `--release` clears the claim and reopens the issue (`--force` releases a claim
  held by another worker). `mb show` displays the claim window and whether it is
  active or stale.

  Compatible with upstream on the shared fields (`assignee` + `status`); the
  minibeads-only `claimed_at`/`claimed_until` fields are additive and are ignored
  by tools that don't understand them.

## [0.17.0]

### Added

- `mb list` clusters numeric issue IDs first (in numeric order, so the most
  recent appear last), then hash-based IDs.

### Fixed

- `make validate` / `Storage::open` now tolerate an upstream-style `config.yaml`
  where `issue-prefix` is commented out, inferring the prefix from issue
  filenames.

### Changed

- crates.io publish metadata and a packaging `exclude` list were added.
