

The upstream beads tool can be found in ./beads and built/run with `go build -o bd ./cmd/bd` and the `./beads/bd` binary it produces.  Run `bd quickstart` to learn about it, and use the CLI to explore it's flags.

This project that you're planning then executing is to create MINIMAL Rust language implementation of the core of `bd`.

* We want to support every `bd subcommand` and flag that the beads MCP server is currently using (beads/integration/beads-mcp), because we will run Claude on containers that have minibeads as their `bd` command.
* We DO NOT want to have any SQL Lite database. Our implementation will be much simpler.
* ./beads contains a version of the upstream project with a recently added MARKDOWN BACKEND

In this session you have the beads MCP loaded. You can try operations on it and see what `bd ...` commands it attempts to invoke. Don't let it install any other version of beads than the Rust one we will develop here.  Let me know if it is trynig to.
## Markdown backend differences
We will not have multiple representations of the data (issues.jsonl + a DB), instead the markdown representation is the ONLY one. The core .beads directory structure is:

```
.beads/
├── config.yaml          # Required: contains issue-prefix
├── issues/              # Required: markdown storage (source of truth)
│   └── <prefix>-N.md   # Individual issue files
├── .gitignore           # Auto-generated
├── command_history.log  # Optional: command logging (can be disabled with --mb-no-cmd-logging)
├── issues.jsonl         # Optional: created by `bd export --mb-output-default`
└── minibeads.lock       # Temporary: coarse-grained lock file (PID-based)
```

**Note**: The markdown files in `.beads/issues/` are the single source of truth. The `issues.jsonl` file is NOT part of the core storage - it's only created when explicitly exporting with `bd export`.

We will use BEADS_DIR if it is available to find `.beads`, but we will attempt to respect the upstream beads `BEADS_DB` as well. If it points to `/dir/.beads/foo.db` we will just cut off the last part of the path and use that to find .beads.

Our config.yaml will currently ONLY have a field for `issue-prefix`. Beads operations validate the .yaml and require that that key is present. If it's not, they add it to the file without disturbing anything else. The main effect of `bd init` will be to create the .beads/config.yaml that establishes that we have a database.

Actually, the `issue-prefix` is redundant because the markdown issues themselves are the source of truth (SOT). If we find config.yaml but it's missing issue-prefix, then we just check the state of the file system. If all `.beads/issues/<prefix>-<nuum>.md` files share a common prefix, then that's obviously the prefix and we can write it to the config.yaml to make it official, but we should print a WARNING that we are doing so.
### Markdown representation with yaml frontmatter
Follow the same serialization scheme as the markdown backend in the Go ./beads implementation. But for now don't implement comments or events. We will come back to them later.  The ONLY files in our datastore will be `.beads/issues/prefix-XYZ.md`.

Upgrades compared to the Go/markdown version:
- On `bd create` proactively populate an empty `# Description` section in the .md file. Don't do this for the other sections.
- Add extra validation/sanitizing. Our serialization scheme relies on us owning the top-level section headers like "Description" and "Notes. if someone attempts to set one of the string fields in an issue `bd update --description`, but provides markdown that has TOP LEVEL SECTIONS, then increase the section level of everything in their string, moving H1 `# ` to H2 `## ` and so on.  Error if you hit max section level for markdown.
- Likewise if reading an issue from serialization, issue a warning if it has top level section headers that are NOT the built in sections we support.
### Coarse-grained locking
The upstream markdown store has a complicated lock-ordering scheme to lock individual .md issues. Let's do something much simpler. Whenever we operate on our markdown "database" touch a file `.beads/minibeads.lock` and set its contents to our PID. We do exponential backoff with sleeping up to five seconds total while waiting for the coarse-grained lock. In our initial design readers will take the lock as well.

**Implementation verified**: Lock file is `minibeads.lock`, uses PID-based locking with exponential backoff (10ms initial, up to 5000ms total) as specified. See `src/lock.rs`.

---

## Validation Status

**Checked up-to-date as of 2025-10-31_#69(3314053)**

All architectural claims verified against implementation:
- ✅ Directory structure matches specification (.beads/config.yaml, .beads/issues/)
- ✅ Markdown-only storage confirmed (issues.jsonl is export-only, not source of truth)
- ✅ Lock file naming and implementation verified (minibeads.lock with PID)
- ✅ Config.yaml contains only issue-prefix field as specified
- ✅ Description section auto-creation confirmed on `bd create`
- ✅ YAML frontmatter serialization implemented as described

