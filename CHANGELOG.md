# Changelog

All notable changes to minibeads are recorded here. minibeads is a markdown-based
drop-in replacement for the [beads](https://github.com/steveyegge/beads) (`bd`)
issue tracker; the binary is named `mb`.

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
