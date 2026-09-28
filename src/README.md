# Rust implementation

- `main.rs`: CLI parsing, database discovery, locking, and command dispatch.
- `storage.rs`, `format.rs`, `types.rs`: issue persistence, Markdown serialization,
  and domain types.
- `github.rs`: authenticated `gh` transport, issue/comment reconciliation, scoped
  selection, and versioned local GitHub synchronization ancestors.
- `prose_merge.rs`: deterministic, conservative three-way text reconciliation;
  no network access or filesystem side effects.
- `sync.rs`: the separate Markdown/JSONL timestamp-based synchronization engine.
- `lock.rs`, `hash.rs`, `code_patch.rs`: locking, identifiers, and reference edits.

Use debug `cargo build` for development. `make validate` runs library and binary
unit tests (including GitHub mock and prose-merge tests), the shell e2e harness,
format checks, and Clippy. Live GitHub stress testing is separate and requires
an explicitly disposable remote repository.

Design rationale and limitations are recorded in
[`ai_docs/github-sync-design.md`](../ai_docs/github-sync-design.md).
