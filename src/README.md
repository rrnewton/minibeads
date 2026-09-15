# Source architecture

The binary composes safe-Rust modules around Markdown issue storage:

- `storage.rs`, `transaction.rs`, and `lock.rs` own checked local reads/writes,
  coarse locking, atomic replacement, and rollback.
- `sync.rs` reconciles Markdown and JSONL. It is a timestamp-based interoperability
  protocol and is intentionally separate from GitHub synchronization.
- `github.rs` orchestrates GitHub issue/comment synchronization through `gh`,
  keeping network waits outside local lock critical sections where possible.
- `github_ancestor.rs` stores versioned per-issue common field checkpoints under
  the resolved database root and publishes them with compare-and-store semantics.
- `github_merge.rs` performs typed three-way field reconciliation.
- `prose_merge.rs` implements bounded deterministic Markdown/prose merging. It
  does not use an LLM and never silently resolves typed conflicts.
- `format.rs` and `types.rs` define the Markdown representation and domain types.

GitHub field synchronization follows this sequence: select local issues, read the
remote snapshot, derive a three-way plan from the common checkpoint, revalidate
before writes, verify convergence, reconcile comments separately, then checkpoint
under the local database lock. Missing or invalid history is not treated as an
empty trusted ancestor. See `../ai_docs/github-sync-plan.md` for the approved
contract and acceptance gates.
