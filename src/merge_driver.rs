//! `mb merge-driver`: a git merge driver for minibeads issue and comment files.
//!
//! Git invokes `mb merge-driver run %O %A %B --marker-size %L --path %P` for
//! paths marked `merge=mb` in `.gitattributes`. The merged result replaces the
//! `%A` file; the exit status is 0 for a clean merge and 1 when conflict hunks
//! were written, exactly as git expects from a merge driver.

use crate::diff3::MarkerSize;
use crate::issue_merge::{merge_comment_files, merge_issue_files, FileMerge, MergeInputs};
use anyhow::{Context, Result};
use std::fs;
use std::path::{Component, Path, PathBuf};
use std::process::Command;

/// Name of the driver in `merge.<name>.driver` and `merge=<name>`.
pub(crate) const DRIVER_NAME: &str = "mb";

/// Storage directories whose issue and comment files the driver handles.
const DATABASE_DIRS: [&str; 2] = [".minibeads", ".beads"];

/// The kind of file being merged, determined from its repository path.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum MergeFileKind {
    /// `<db>/issues/**/<id>.md` (flat or sharded layout).
    Issue,
    /// `<db>/comments/<id>.json`.
    Comments,
    /// Anything else: merged as plain lines.
    Text,
}

/// Classify a repository-relative path such as `.beads/issues/mb-1.md`.
pub(crate) fn classify_path(path: &Path) -> MergeFileKind {
    let components: Vec<&str> = path
        .components()
        .filter_map(|component| match component {
            Component::Normal(name) => name.to_str(),
            _ => None,
        })
        .collect();
    let Some(db_index) = components
        .iter()
        .rposition(|name| DATABASE_DIRS.contains(name))
    else {
        return MergeFileKind::Text;
    };
    let rest = &components[db_index + 1..];
    let extension = path.extension().and_then(|extension| extension.to_str());
    match (rest, extension) {
        ([dir, .., _file], Some("md")) if *dir == "issues" => MergeFileKind::Issue,
        (["comments", _file], Some("json")) => MergeFileKind::Comments,
        _ => MergeFileKind::Text,
    }
}

/// Merge one file's three versions according to its kind.
pub(crate) fn merge_file(
    kind: MergeFileKind,
    path: &Path,
    inputs: MergeInputs<'_>,
    size: MarkerSize,
) -> FileMerge {
    match kind {
        MergeFileKind::Issue => {
            let issue_id = path
                .file_stem()
                .and_then(|stem| stem.to_str())
                .unwrap_or("issue");
            merge_issue_files(issue_id, inputs, size)
        }
        MergeFileKind::Comments => merge_comment_files(inputs, size),
        MergeFileKind::Text => {
            let merged = crate::diff3::merge_lines(
                crate::diff3::ThreeWay {
                    base: inputs.base,
                    ours: inputs.ours,
                    theirs: inputs.theirs,
                },
                size,
            );
            FileMerge {
                text: merged.text,
                conflicts: (0..merged.hunks.get())
                    .map(|_| crate::issue_merge::MergeConflict {
                        subject: crate::issue_merge::ConflictSubject::Lines,
                        reason: crate::issue_merge::ConflictReason::BothChanged,
                    })
                    .collect(),
                notices: Vec::new(),
            }
        }
    }
}

/// Whether the merge left conflicts for the user to resolve.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum DriverOutcome {
    Clean,
    Conflicted,
}

fn read_input(path: &Path) -> Result<String> {
    let bytes = fs::read(path).with_context(|| format!("Failed to read {}", path.display()))?;
    String::from_utf8(bytes).with_context(|| format!("{} is not UTF-8", path.display()))
}

/// Paths and options of one driver invocation.
pub(crate) struct DriverRun<'a> {
    pub(crate) base: &'a Path,
    pub(crate) ours: &'a Path,
    pub(crate) theirs: &'a Path,
    /// Repository path of the file (git's `%P`), used to pick the merge kind.
    pub(crate) path: Option<&'a Path>,
    pub(crate) size: MarkerSize,
    /// Print the result instead of overwriting `ours`.
    pub(crate) stdout: bool,
}

/// Run the driver: merge, write the result, and report every conflict and
/// notice on stderr as one `mb merge-driver: ...` line each.
pub(crate) fn run_driver(run: &DriverRun<'_>) -> Result<DriverOutcome> {
    let base = read_input(run.base)?;
    let ours = read_input(run.ours)?;
    let theirs = read_input(run.theirs)?;
    let path = run.path.unwrap_or(run.ours);
    let kind = classify_path(path);
    let merged = merge_file(
        kind,
        path,
        MergeInputs {
            base: &base,
            ours: &ours,
            theirs: &theirs,
        },
        run.size,
    );

    if run.stdout {
        print!("{}", merged.text);
    } else {
        crate::transaction::atomic_write(run.ours, merged.text.as_bytes())
            .with_context(|| format!("Failed to write {}", run.ours.display()))?;
    }
    let shown = path.display();
    for notice in &merged.notices {
        eprintln!("mb merge-driver: NOTICE {shown}: {notice}");
    }
    for conflict in &merged.conflicts {
        eprintln!(
            "mb merge-driver: CONFLICT {shown}: {}: {}",
            conflict.subject, conflict.reason
        );
    }
    Ok(if merged.is_clean() {
        DriverOutcome::Clean
    } else {
        DriverOutcome::Conflicted
    })
}

/// The `.gitattributes` lines routing minibeads files to the driver.
pub(crate) fn gitattributes_lines() -> Vec<String> {
    DATABASE_DIRS
        .iter()
        .flat_map(|dir| {
            [
                format!("{dir}/issues/**/*.md merge={DRIVER_NAME}"),
                format!("{dir}/comments/*.json merge={DRIVER_NAME}"),
            ]
        })
        .collect()
}

/// The `merge.mb.driver` command line for a given `mb` executable.
///
/// Git substitutes `%P` already shell-quoted, so it must not be quoted again.
pub(crate) fn driver_command(executable: &str) -> String {
    let plain = !executable.is_empty()
        && executable
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || "/._-+:@".contains(character));
    let executable = if plain {
        std::borrow::Cow::Borrowed(executable)
    } else {
        // Git runs the driver through `sh -c`.
        std::borrow::Cow::Owned(format!("'{}'", executable.replace('\'', r"'\''")))
    };
    format!("{executable} merge-driver run %O %A %B --marker-size %L --path %P")
}

fn git(args: &[&str]) -> Result<String> {
    let output = Command::new("git")
        .args(args)
        .output()
        .with_context(|| format!("Failed to run git {}", args.join(" ")))?;
    anyhow::ensure!(
        output.status.success(),
        "git {} failed: {}",
        args.join(" "),
        String::from_utf8_lossy(&output.stderr).trim()
    );
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_owned())
}

/// Options of `mb merge-driver install`.
pub(crate) struct InstallOptions<'a> {
    /// Command git should run; `mb` resolves through `PATH` at merge time.
    pub(crate) executable: &'a str,
    /// Write `git config` into the user's global config instead of the repo's.
    pub(crate) global: bool,
    /// Leave `.gitattributes` untouched.
    pub(crate) skip_gitattributes: bool,
}

/// Register the driver with git and route minibeads files to it.
pub(crate) fn install(options: &InstallOptions<'_>) -> Result<Vec<String>> {
    let scope = if options.global {
        "--global"
    } else {
        "--local"
    };
    let mut report = Vec::new();
    let name_key = format!("merge.{DRIVER_NAME}.name");
    let driver_key = format!("merge.{DRIVER_NAME}.driver");
    let command = driver_command(options.executable);
    git(&[
        "config",
        scope,
        &name_key,
        "minibeads three-way issue and comment merge",
    ])?;
    git(&["config", scope, &driver_key, &command])?;
    report.push(format!("git config {scope} {driver_key} '{command}'"));

    if !options.skip_gitattributes {
        let root = PathBuf::from(git(&["rev-parse", "--show-toplevel"])?);
        let attributes_path = root.join(".gitattributes");
        let existing = match fs::read_to_string(&attributes_path) {
            Ok(text) => text,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => String::new(),
            Err(error) => {
                return Err(error)
                    .with_context(|| format!("Failed to read {}", attributes_path.display()))
            }
        };
        let mut updated = existing.clone();
        for line in gitattributes_lines() {
            if !existing
                .lines()
                .any(|existing_line| existing_line.trim() == line)
            {
                if !updated.is_empty() && !updated.ends_with('\n') {
                    updated.push('\n');
                }
                updated.push_str(&line);
                updated.push('\n');
                report.push(format!("{}: {line}", attributes_path.display()));
            }
        }
        if updated != existing {
            fs::write(&attributes_path, updated)
                .with_context(|| format!("Failed to write {}", attributes_path.display()))?;
        }
    }
    Ok(report)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classifies_flat_and_sharded_issue_paths() {
        for path in [
            ".beads/issues/minibeads-3.md",
            ".minibeads/issues/n/0/3/proj-3.md",
            "sub/project/.minibeads/issues/h/ab/proj-ab12.md",
        ] {
            assert_eq!(
                classify_path(Path::new(path)),
                MergeFileKind::Issue,
                "{path}"
            );
        }
    }

    #[test]
    fn classifies_comment_paths_and_everything_else() {
        assert_eq!(
            classify_path(Path::new(".beads/comments/minibeads-30.json")),
            MergeFileKind::Comments
        );
        for path in [
            ".beads/github-sync-state.json",
            ".beads/config.yaml",
            "README.md",
            "docs/issues/a.md",
            ".beads/comments/nested/a.json",
        ] {
            assert_eq!(
                classify_path(Path::new(path)),
                MergeFileKind::Text,
                "{path}"
            );
        }
    }

    #[test]
    fn driver_command_passes_every_git_placeholder() {
        let command = driver_command("mb");
        for placeholder in ["%O", "%A", "%B", "%L", "%P"] {
            assert!(command.contains(placeholder), "{placeholder}");
        }
        assert!(command.ends_with("--path %P"), "git quotes %P itself");
        assert!(driver_command("/opt/my tools/mb").starts_with("'/opt/my tools/mb' merge-driver"));
        assert!(driver_command("/x/it's/mb").starts_with(r"'/x/it'\''s/mb' "));
    }

    #[test]
    fn gitattributes_cover_both_database_directories() {
        let lines = gitattributes_lines();
        assert!(lines.contains(&".beads/issues/**/*.md merge=mb".to_owned()));
        assert!(lines.contains(&".minibeads/comments/*.json merge=mb".to_owned()));
    }
}
