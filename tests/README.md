# Tests

Use debug builds for local development and validation.

- `make validate` runs the routine Rust tests, seven shell-test fixtures,
  migration/regression suites, deterministic harness-initialization checks,
  formatting, and Clippy. Its `purge` prerequisite removes upstream artifacts;
  use `make -o purge validate` when the private-database boundary prohibits that
  cleanup. This still runs all validation gates.
- `cargo test --locked --test regressions` exercises CLI/storage integration
  regressions in isolated temporary databases.
- `cargo test --locked --bin mb github::sync_tests` exercises mocked GitHub
  three-way merges, scoped selection, dry-run, concurrency, and recovery without
  writing to a live repository. Prose and ancestor unit suites run in the same
  binary test target.
- `cargo test --locked --test random_minibeads harness_initialization` checks
  concurrent test-binary initialization without running the stress workload.
- `cargo test --locked --test random_minibeads minibeads` runs the native
  numeric/hash random and parallel stress cases. The wrapper builds both debug
  tools together once before any worker executes them, so another test cannot
  replace a binary while it is running.
- `cargo test --locked --test random_minibeads test_migration_stress` runs the
  randomized migration case. Upstream comparisons require the optional `beads`
  checkout and `make upstream`; absence of that checkout is not native coverage.

The wrapper respects Cargo's target-directory configuration. GitHub's
cross-platform `cargo test` runs the full random-wrapper suite, while routine
validation includes only its deterministic initialization regressions. Add
regressions to both routine validation and CI rather than relying on temporary
manual test commands.
