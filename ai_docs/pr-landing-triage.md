# Offline PR landing triage: Brian / unormal

> **Historical offline notes, superseded for landing.** After the owner restarted
> with internet enabled, `with-proxy git` and `gh` succeeded. Live PRs #15–#24
> are Brian's cumulative stack; the unrelated cached branches below are not its
> landing plan. Independent reviews found and corrected migration timestamp
> precedence, persisted GitHub snapshot normalization, and punctuation-ID code
> replacement. The corrected stack passes 203 debug tests on stable and Rust
> 1.87, strict Clippy, and audit. Because #24 fixes earlier audit failures, the
> stack landed atomically through #24 (`19fc71cfc1`) with original commit order
> preserved after all seven final-head CI jobs passed. All ten PRs report MERGED
> and all ten branch archive tags are verified. Current review evidence is in the clean
> landing worktree's `ai_docs/pr-landing-review.md` and minibeads-37. Historical
> network-denial instructions below do not apply to the enabled session.

Snapshot: **2026-09-07_#215(9437f50c1a)**, from `date +%F`, `git rev-list --count HEAD`, and `git rev-parse --short=10 HEAD`. Cached `main`, `origin/main`, and `v0.28.0` point to this commit. Findings and source line numbers refer to these cached objects, not subsequent working-copy changes.

## Evidence boundary and attribution

- Inspected cached branch ancestry, authors/committers/messages, patches, source, and test configuration. The repository is not shallow; cached ref namespaces are heads/remotes/tags, with no PR refs. Cache freshness is unknown. No fetch, pull, GitHub/API request, alternate route, merge, commit, or source edit was performed.
- **No cached author or commit-message match identifies Brian/unormal.** Raw authors are Ryan Newton (two email addresses) and Ryan Newton + Claude. Branch names and authorship do not establish PR ownership. The branches below are review candidates, not confirmed Brian PRs.
- **Live PR numbers, authors, heads/bases, open/closed/merged status, reviews, and CI results are unverified.** No PR numbers are inferred. Cached ancestry proves integration into cached main only, not live PR closure.
- Cached commit `2f99c17` explicitly describes re-implementing two old PRs: correcting the installed binary name and accepting upstream dependency records. Both fixes are in main (`Makefile:61`, `src/types.rs:689`, `src/types.rs:699`); the latter has upstream-input/native-output regression tests. The message does not attribute those PRs to Brian/unormal. Do not revive their old implementations without an owner-provided mapping and a demonstrated remaining gap.
- Applied the supplied AGENTS instructions and read `PROJECT_VISION.md`; no additional applicable on-disk AGENTS or `OPTIMIZATION.md` was found. `bd quickstart` was unavailable (`bd` not installed). Private database contents were neither read nor modified. Baseline validation and sync implementation belong to the parent; no tests were run for this documentation-only triage.

## Cached branch inventory

Counts are `git rev-list --left-right --count origin/main...<ref>`: main-only / branch-only. All refs below have the `origin/` prefix.

| Ref | Tip | Counts | Cached disposition |
| --- | --- | --- | --- |
| `develop` | `c49e75e` | 99 / 0 | Integrated ancestor; no remaining commits. |
| `integration` | `725dfc0` | 11 / 0 | Integrated, including sharded storage (`36e6b0d`); do not re-land. |
| `feature/pull-only-local-edit-guard` | `975372c` | 10 / 0 | Integrated local-edit/comment-deletion protection, `--since`, `--token`, storage-prefix changes. |
| `codex/atomic-minibeads-lock` | `93b2e48` | 6 / 0 | Integrated atomic initial lock creation. |
| `codex/mirrored-description-guard` | `358ce48` | 2 / 0 | Integrated three-commit guard sequence: `30929f7`, `ef53bee`, `358ce48`. |
| `fix/migrate-description-truncation` | `de9358a` | 1 / 0 | Integrated parser/migration protection, released in `9437f50`. |
| `codex/refresh-sync-stamps` | `51de629` | 2 / 1 | **Only outstanding cached patch. Hold pending contract/test reconciliation.** |

For the six integrated tips, `git merge-base --is-ancestor <ref> origin/main` succeeds. For refresh, it does not, and `git cherry -v origin/main origin/codex/refresh-sync-stamps` reports `+ 51de629...`: neither ancestor-integrated nor patch-equivalent to main.

## Dependencies, overlaps, and landing order

Verified ancestry: `develop` → `integration` → pull-only guard → atomic lock → mirrored-description guard. Both refresh and the parser fix descend from mirrored guard `358ce48`; refresh does **not** contain the later parser fix/release.

1. **Keep cached main as the baseline.** All preceding safety/storage work is already present; no historical branch needs merging first.
2. **Resolve refresh semantics with the parent's sync implementation, then land one combined solution.** Refresh changes only `src/github.rs` (26 insertions, 16 deletions). Main's two post-fork commits touch only `src/format.rs`, its migration integration test, and Cargo version files, so there is no cached file overlap. However, refresh semantically overlaps existing no-op/pull-only guarantees and the parent's sync work; do not apply a second independent timestamp fix.
3. **Do not land refresh unchanged.** Preserve the parser release and conflict guards, add the missing behavioral coverage below, then validate before any owner-approved landing. Handle already-integrated coverage follow-ups separately rather than re-merging old branches.
4. For any additional Brian work, obtain an offline patch/ref bundle plus ownership/base/head information from the owner and repeat ancestry/patch-equivalence review. The current cache cannot establish another landing candidate or a live PR order.

## Concrete risks and missing coverage

### Outstanding refresh patch

- **Blocking contract conflict, statically identified:** `51de629` always replaces `synced_at` with `Utc::now()`, removing unchanged-state preservation in `update_state_entry`. Existing `github_sync_pull_only_imports_without_writing_to_github` asserts byte-identical sync-state JSON after a second no-op (`src/github.rs:3399`, `src/github.rs:3416`). That unchanged assertion is expected to fail when the timestamps differ; this was not executed here. Decide explicitly whether acknowledgment freshness supersedes byte-stable metadata. If yes, retain no-op assertions for issue/comment data, counters, remote writes, hashes, and comment mappings while allowing only the intended timestamp change. State-file contents will now churn on successful no-ops; `save_state` already writes the file on main (`src/github.rs:1260`, `src/github.rs:2374`).
- **Not a complete incremental-sync fix:** `--since` filters only local `updated_at` (`src/github.rs:884`); remote-only changes on older local records remain outside that window. Cached production code has no staleness reader of `synced_at`. Advancing it supplies acknowledgment metadata, not remote-change discovery or a demonstrated freshness UI. Specify the intended consumer and full-sync policy; test full versus incremental passes, the cutoff boundary, explicit-ID/filter interaction, and unchanged timestamps for skipped records.
- **Coverage is helper-only:** the new test backdates an entry and checks two hashes plus timestamp advancement, without persistence or comments. The commit records only that one filtered test passing, not suite validation. Add fake-GitHub end-to-end tests for normal and pull-only no-ops, populated comment mappings, dry-run, mixed success/conflict batches, comment-deletion conflicts, force resolution, and failed remote operations. Existing pull-only local-edit coverage checks unchanged state on one conflict; retain it and explicitly check acknowledgment behavior. `src/github.rs:1233` gates updates on non-dry-run/non-conflict; do not refresh skipped/conflicted entries globally.

### Already-integrated follow-ups, not additional landing dependencies

- **Lock lifecycle:** atomic creation does not establish owner-checked stale-lock cleanup (`src/lock.rs:65`); malformed/empty lock contents intentionally time out rather than self-repair. The added absent-lock, same-process threaded test (`src/lock.rs:172`) does not cover multiprocess contention, stale-lock reclamation, or owner death during initialization. Add lifecycle/recovery coverage before claiming comprehensive concurrency safety.
- **Mirrored-description guard:** current regressions exercise the predicate, not CLI wiring (`src/main.rs:3791`, `src/main.rs:3819`). Add CLI coverage for inline/file/stdin descriptions, search/replace, append, simultaneous linking, explicit override, non-description edits, and mixed linked/unlinked batches that must reject before any write. These paths overlap pull-only protection but address a distinct earlier write boundary.
- **Migration test scheduling:** `tests/migration_description_truncation.rs:17` covers flat-layout hash-to-numeric migration, but `make validate` selects only lib/bins/`e2e_tests` (`Makefile:16`), omitting this standalone Rust integration target. Cached cross-platform CI runs unrestricted `cargo test` (`.github/workflows/ci.yml:124`), so it is not wholly absent from CI. Include it in routine validation and add sharded-layout migration coverage; keep this distinction explicit when reporting test totals.

## Unblock / landing checklist

- [ ] Owner supplies offline Brian/unormal-to-branch/patch attribution and PR metadata if needed; record its provenance separately from cached evidence. Do not retry the denied GitHub endpoints or infer PR identifiers/status.
- [x] Parent resolves local formatting/lint gate failures without changing parser/storage/guard behavior. Working-tree validation at `2026-09-07_#215(9437f50c1a)` passes 208 routine tests, fmt, Clippy, and a separate migration test; strict all-target/all-feature Clippy also passes. The purge prerequisite is skipped to preserve the private-database boundary. No branch/ref movement is part of this triage.
- [ ] Parent chooses one timestamp contract/implementation, reconciles the existing byte-equality test, and adds persisted no-op, skipped/conflict/dry-run, comment-mapping, and incremental-window coverage using local fake GitHub only.
- [ ] Parent wires any new regression tests into both routine `make validate` and CI; address the omitted migration target explicitly. Run debug `cargo build`, targeted tests, then `make validate`, with no release-mode iteration or live GitHub stress tests. Offline dependency gaps are blockers, not reasons to fetch.
- [ ] Before landing, review the final diff against the preserved main baseline and record actual test totals. Recheck ancestry if the owner supplies newer cached refs; land only the still-missing implementation. Live CI approval remains unverified until separately supplied.
