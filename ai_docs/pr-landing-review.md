# Brian's PR stack: review and landing

Snapshot: `2026-09-07_#225(f47b766cbf)`, with the reviewed corrections described
below applied on top. Tracking: minibeads-37 and minibeads-3.

## Completed landing

Verified at `2026-09-07_#227(19fc71cfc1)`:

- Correction commit `25b963ff0c` passed all seven jobs in GitHub Actions run
  [34158336503](https://github.com/rrnewton/minibeads/actions/runs/34158336503).
- A normal merge of #24 produced `19fc71cfc1`. GitHub reports **all ten PRs
  #15–#24 merged**, and every reviewed head is an ancestor of `origin/main`.
- The merged code, tests, dependencies, Makefile, and workflows match the tested
  tip. No force push, squash, or experimental overhaul code was used.
- All ten branches in the table have remote annotated archives named
  `codex/<suffix>.v1`; each dereferenced tag was checked against its PR head.
  Brian's fork branches were not deleted.
- Remaining work in minibeads-37 is the **human review of the sync-overhaul
  plan**, not further PR landing. The original dirty checkout/prototype is
  preserved separately; do not mistake it for this validated landing worktree.

## Post-merge test-harness follow-up

At `2026-09-07_#228(3f467d063a)`, the documentation-only handoff CI
([34158790240](https://github.com/rrnewton/minibeads/actions/runs/34158790240))
exposed an intermittent macOS `random_minibeads` failure: a worker could not
spawn `mb` after several successful actions. The original corrected PR and its
merge both passed all seven jobs. Parallel tests were independently rebuilding
the shared binaries while other tests executed them.

The follow-up caches one initialization result with `OnceLock`, builds both tools
in a single locked **debug** Cargo invocation, and obtains executable paths from
Cargo artifacts instead of assuming `target/release`. Five deterministic tests
cover concurrent initialization, cached failures, one shared debug build, custom
target paths/platform suffixes, and missing artifacts. They run in routine
`make validate` and the existing cross-platform CI suite.

Integrated validation passes **208 routine tests** (the earlier 203 plus these
five), fmt, and strict all-target/all-feature Clippy. The native harness passes
10 tests on stable and Rust 1.87 (five overlap the routine initialization tests).
The reviewer also validated a custom target directory containing spaces and
six seeded parallel runs totaling 602 iterations. Upstream comparisons were
excluded from these native checks. This fixes test execution, not sync semantics;
the overhaul remains on human-review hold. Final platform confirmation is recorded
by the follow-up commit's GitHub Actions checks.

## Scope and order

Brian (`unormal`) submitted ten cumulative PRs against `rrnewton/minibeads:main`.
Each original head is an ancestor of the next, in this order:

| PR | Branch suffix (`codex/`) | Reviewed change |
| --- | --- | --- |
| #15 | review-windows-baseline | Windows stack and validation baseline |
| #16 | review-safe-identifiers | Portable identifiers and path containment |
| #17 | review-id-migrations | Transactional ID and layout migrations |
| #18 | review-readiness | Prerequisite status handling |
| #19 | review-markdown-preservation | Lossy-write rejection and CRLF/BOM input |
| #20 | review-jsonl-sync | JSONL locking, conflicts, and output paths |
| #21 | review-github-sync | Checked snapshots and concurrent state updates |
| #22 | review-cli-safeguards | Read-only mode and command-history safeguards |
| #23 | review-code-references | Native, cycle-safe code-reference replacement |
| #24 | review-dependencies | Dependency advisory fixes and Rust 1.87 minimum |

Independent agent reviews covered #16/#19, #17/#18, #20/#21, #22/#23, and #24;
the parent reviewed #15 and integrated the corrections in a detached worktree.
No experimental ancestor archive, label filtering, or prose merger is included.
The owner's GitHub-sync overhaul remains gated on one human plan-review round.
Its proposal is available in the owner's working checkout at
`ai_docs/github-sync-plan.md`, tracked in minibeads-37.

## Corrections required by review

- **#17:** ID/layout migration refreshed Markdown timestamps and could cause an
  older issue to overwrite a newer independent JSONL edit. Preserve source mtime
  and logical update time across migration. Stage the desired timestamp on the
  temporary file before publishing it; retain #20's transaction timestamp API.
  Regressions cover flat/sharded layouts and all five ID migration paths.
  Rollback tests inject failure at each of three file operations and check both
  original contents and timestamps.
- **#21:** returning an unnormalized in-memory issue after writing Markdown
  made a later snapshot check reject the program's own trailing-newline/header
  normalization. Return the persisted snapshot under the lock and normalize
  imported issues before recording ancestry. The existing pull-only regression
  fails on the original #21 and passes on #20; two added tests also failed before
  these corrections and now pass.
- **#23:** ordinary regex word boundaries did not match `git grep -w` semantics
  for punctuation-ended IDs. Use half-word boundaries and test both leading and
  trailing punctuation. Additional coverage checks cycles across files, mixed
  line endings, executable permissions, and #22's escaped multiline history.

These are narrow fixes to Brian's existing features, not the unapproved overhaul.
Path containment does not claim immunity to concurrent hostile path replacement;
transaction tests do not establish power-loss durability on every filesystem;
Markdown CRLF/BOM support is not a byte-preserving formatting guarantee.

## Validation before submission

- Debug `make -o purge validate`: **203 passed** (66 library, 101 binary,
  7 shell fixtures, 1 standalone migration, 28 regressions); fmt and Clippy pass.
- Rust **1.87.0**: the same **203 tests pass**, using debug artifacts.
- Strict `cargo clippy --locked --all-targets --all-features -- -D warnings`
  and `git diff --check` pass.
- `cargo audit --deny warnings`: no vulnerabilities or warnings in 142 locked
  dependencies; 1,242 advisories at database commit
  `8a1eb4f933fb5821add5b4e98601ebd90b8b3538` (2026-09-07).
- `purge` is deliberately excluded locally because it traverses/removes private
  database artifacts. All actual validation gates run. Randomized stress tests
  are not included because their harness hardcodes release builds.

## Landing protocol

Earlier stack heads fail the security audit until #24. Original #24 also inherits
the #21 test failure. Therefore, validate and merge the corrected **full stack
atomically through #24**, preserving all ten original commits in their existing
order instead of publishing intermediate red states on main.

Fast-forward the existing fork tip with review corrections; do not force-push or
squash the cumulative stack. Require green final-head CI, then use a normal merge
commit. Verify remote ancestry and every PR's merged state independently. Archive
the completed branch heads with available `<branch>.vN` tags, without deleting
Brian's fork branches. Final CI/merge/tag results belong in minibeads-37.
