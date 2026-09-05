use anyhow::{Context, Result};
use std::collections::{BTreeMap, HashMap};
use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

/// Result of searching for issue ID references in code
#[derive(Debug)]
pub struct CodeReferences {
    repo_root: PathBuf,
    /// Map from file path to list of (line_number, line_content) tuples
    pub matches: HashMap<String, Vec<(usize, String)>>,
    /// Total number of matches found
    pub total_matches: usize,
}

impl CodeReferences {
    fn new(repo_root: PathBuf) -> Self {
        Self {
            repo_root,
            matches: HashMap::new(),
            total_matches: 0,
        }
    }
}

/// Check if we're running in an interactive TTY
pub fn is_interactive_tty() -> bool {
    use std::io::IsTerminal;
    io::stdin().is_terminal() && io::stdout().is_terminal()
}

/// Search for references to an issue ID in code using git grep
///
/// Excludes minibeads metadata directories and uses word boundaries to match issue IDs.
/// Returns a mapping from file paths to matching lines with their line numbers.
pub fn find_code_references(issue_id: &str) -> Result<CodeReferences> {
    find_code_references_in(&std::env::current_dir()?, issue_id)
}

fn find_code_references_in(directory: &Path, issue_id: &str) -> Result<CodeReferences> {
    crate::paths::IssueId::parse(issue_id)?;
    let root = Command::new("git")
        .arg("-C")
        .arg(directory)
        .args(["rev-parse", "--show-toplevel"])
        .output()
        .context("Failed to locate Git working tree")?;
    anyhow::ensure!(root.status.success(), "Not inside a Git working tree");
    let root = String::from_utf8(root.stdout).context("Non-UTF-8 repository path")?;
    let root = root.strip_suffix('\n').unwrap_or(&root);
    let repo_root = PathBuf::from(root.strip_suffix('\r').unwrap_or(root));
    let output = Command::new("git")
        .arg("-C")
        .arg(&repo_root)
        .args([
            "grep",
            "-n",
            "-z",
            "-I",
            "--no-color",
            "--no-heading",
            "--full-name",
            "-F",
            "-w",
            "-e",
            issue_id,
            "--",
            ".",
            ":(top,exclude,glob)**/.minibeads/**",
            ":(top,exclude,glob)**/.beads/**",
        ])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .context("Failed to execute git grep")?;
    let mut references = CodeReferences::new(repo_root);
    if output.status.code() == Some(1) {
        return Ok(references);
    }
    anyhow::ensure!(
        output.status.success(),
        "git grep failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    // With -n -z Git emits filename NUL line-number NUL content LF.
    // Splitting whole lines first would corrupt filenames containing newlines.
    let mut remaining = output.stdout.as_slice();
    while !remaining.is_empty() {
        let filename_end = remaining
            .iter()
            .position(|b| *b == 0)
            .context("Missing filename delimiter in git grep output")?;
        let filename = std::str::from_utf8(&remaining[..filename_end])
            .context("Non-UTF-8 filename in git grep output")?;
        remaining = &remaining[filename_end + 1..];
        let number_end = remaining
            .iter()
            .position(|b| *b == 0)
            .context("Missing line-number delimiter in git grep output")?;
        let line_number = std::str::from_utf8(&remaining[..number_end])?.parse::<usize>()?;
        remaining = &remaining[number_end + 1..];
        let content_end = remaining
            .iter()
            .position(|b| *b == b'\n')
            .unwrap_or(remaining.len());
        let content = String::from_utf8_lossy(&remaining[..content_end]);
        references
            .matches
            .entry(filename.to_owned())
            .or_default()
            .push((line_number, content.trim_end_matches('\r').to_owned()));
        references.total_matches += 1;
        remaining = if content_end < remaining.len() {
            &remaining[content_end + 1..]
        } else {
            &[]
        };
    }
    Ok(references)
}

/// Ask user for confirmation
/// Ask user for confirmation to patch code references
///
/// Returns true if user confirms, false otherwise.
pub fn confirm_code_patch(old_id: &str, new_id: &str, references: &CodeReferences) -> Result<bool> {
    println!(
        "\nFound {} reference(s) to {} in code:",
        references.total_matches, old_id
    );
    println!();

    // Show all matches organized by file
    let mut files: Vec<&String> = references.matches.keys().collect();
    files.sort();

    for file in files {
        let matches = &references.matches[file];
        println!("  {}:", file);
        for (line_num, content) in matches {
            println!("    {}: {}", line_num, content.trim());
        }
        println!();
    }

    println!(
        "Do you want to replace all occurrences of {} with {} in these files? [Y/n]",
        old_id, new_id
    );
    print!("> ");
    io::stdout().flush()?;

    let mut input = String::new();
    io::stdin().read_line(&mut input)?;
    let input = input.trim().to_lowercase();

    // Accept empty input or 'y' as yes
    Ok(input.is_empty() || input == "y" || input == "yes")
}

/// Patch tracked text files with literal, word-boundary-aware ID replacements.
/// Preserve original line endings and permissions without depending on sed.
pub fn patch_code_files(old_id: &str, new_id: &str, references: &CodeReferences) -> Result<usize> {
    let mapping = HashMap::from([(old_id.to_owned(), new_id.to_owned())]);
    patch_code_mappings(&mapping, references)
}

fn patch_code_mappings(
    mapping: &HashMap<String, String>,
    references: &CodeReferences,
) -> Result<usize> {
    if mapping.is_empty() {
        return Ok(0);
    }
    for (old_id, new_id) in mapping {
        crate::paths::IssueId::parse(old_id)?;
        crate::paths::IssueId::parse(new_id)?;
    }
    let mut patterns: Vec<_> = mapping.keys().map(|id| regex::escape(id)).collect();
    patterns.sort_by_key(|pattern| std::cmp::Reverse(pattern.len()));
    let pattern = regex::Regex::new(&format!(r"\b(?:{})\b", patterns.join("|")))?;
    let mut prepared = BTreeMap::new();
    for file in references.matches.keys() {
        let path = references.repo_root.join(file);
        crate::paths::ensure_contained(&references.repo_root, &path)?;
        let original = fs::read_to_string(&path)
            .with_context(|| format!("Failed to read code file: {}", path.display()))?;
        // A single pass makes swaps/cycles safe and treats '$' in IDs literally.
        let replacement = pattern.replace_all(&original, |captures: &regex::Captures<'_>| {
            mapping
                .get(&captures[0])
                .expect("Regex contains only mapping keys")
                .as_str()
        });
        if replacement != original {
            let replacement = replacement.into_owned();
            prepared.insert(path, (original, replacement));
        }
    }
    for (path, (original, replacement)) in &prepared {
        anyhow::ensure!(
            fs::read(path)? == original.as_bytes(),
            "Code file changed while preparing patches: {}",
            path.display()
        );
        crate::transaction::atomic_write(path, replacement.as_bytes())?;
    }
    Ok(prepared.len())
}

/// Main entry point for code patching
/// Main entry point for code patching functionality
///
/// This orchestrates the entire code patching workflow:
/// 1. Check for interactive TTY
/// 2. Search for references
/// 3. Ask for confirmation
/// 4. Patch files
///
/// Returns the number of files patched, or None if no patching was performed.
pub fn patch_code_for_rename(old_id: &str, new_id: &str) -> Result<Option<usize>> {
    // Check if we're in an interactive TTY
    if !is_interactive_tty() {
        eprintln!("Warning: --mb-patch-code requires an interactive TTY. Skipping code patching.");
        return Ok(None);
    }

    // Search for references
    let references = find_code_references(old_id)?;

    // If no references found, skip
    if references.total_matches == 0 {
        // Don't print anything if no matches - keeps output clean
        return Ok(Some(0));
    }

    // Ask for confirmation
    if !confirm_code_patch(old_id, new_id, &references)? {
        println!("Skipping code patching.");
        return Ok(Some(0));
    }

    // Patch files
    let files_patched = patch_code_files(old_id, new_id, &references)?;

    println!("Patched {} file(s) in working copy.", files_patched);

    Ok(Some(files_patched))
}

/// Collect all accepted mappings before editing, then replace them in one pass.
pub fn patch_code_for_migration(id_mapping: &HashMap<String, String>) -> Result<usize> {
    if !is_interactive_tty() {
        eprintln!("Warning: --mb-patch-code requires an interactive TTY. Skipping code patching.");
        return Ok(0);
    }

    let mut accepted = HashMap::new();
    let mut combined: Option<CodeReferences> = None;
    let mut mappings: Vec<_> = id_mapping.iter().collect();
    mappings.sort_by_key(|(old_id, _)| *old_id);
    for (old_id, new_id) in mappings {
        if old_id == new_id {
            continue;
        }
        let references = find_code_references(old_id)?;
        if references.total_matches == 0 {
            continue;
        }
        if !confirm_code_patch(old_id, new_id, &references)? {
            println!("Skipping {} -> {}", old_id, new_id);
            continue;
        }
        accepted.insert(old_id.clone(), new_id.clone());
        if let Some(combined) = &mut combined {
            anyhow::ensure!(
                combined.repo_root == references.repo_root,
                "Git working tree changed"
            );
            combined.total_matches += references.total_matches;
            for (file, matches) in references.matches {
                combined.matches.entry(file).or_default().extend(matches);
            }
        } else {
            combined = Some(references);
        }
    }

    let total = match combined {
        Some(references) => patch_code_mappings(&accepted, &references)?,
        None => 0,
    };
    if total > 0 {
        println!("\nTotal: patched {} file(s) in working copy.", total);
    }
    Ok(total)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn repository(files: &[(&str, &str)]) -> tempfile::TempDir {
        let directory = tempfile::tempdir().unwrap();
        let output = Command::new("git")
            .arg("-C")
            .arg(directory.path())
            .args(["init", "--quiet"])
            .output()
            .unwrap();
        assert!(output.status.success());
        for (name, content) in files {
            let path = directory.path().join(name);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(path, content).unwrap();
        }
        let output = Command::new("git")
            .arg("-C")
            .arg(directory.path())
            .args(["-c", "core.safecrlf=false", "add", "--", "."])
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        directory
    }

    #[test]
    fn finds_and_patches_from_subdirectories_without_sed() {
        let directory = repository(&[
            (
                "src/code with spaces.rs",
                "first\r\n// r[1]-1: track this\r\nr[1]-10 untouched\r\n",
            ),
            (".minibeads/issues/r[1]-1.md", "r[1]-1"),
            (".beads/issues/r[1]-1.md", "r[1]-1"),
            ("nested/.minibeads/notes", "r[1]-1"),
        ]);
        let references = find_code_references_in(&directory.path().join("src"), "r[1]-1").unwrap();
        assert_eq!(references.total_matches, 1);
        assert_eq!(
            references.matches["src/code with spaces.rs"][0],
            (2, "// r[1]-1: track this".into())
        );
        assert_eq!(patch_code_files("r[1]-1", "r-$1", &references).unwrap(), 1);
        assert_eq!(
            fs::read(directory.path().join("src/code with spaces.rs")).unwrap(),
            b"first\r\n// r-$1: track this\r\nr[1]-10 untouched\r\n"
        );
        assert_eq!(
            fs::read_to_string(directory.path().join(".minibeads/issues/r[1]-1.md")).unwrap(),
            "r[1]-1"
        );
        assert_eq!(
            find_code_references_in(directory.path(), "missing-42")
                .unwrap()
                .total_matches,
            0
        );
    }

    #[test]
    fn migration_swaps_ids_once_and_preserves_word_boundaries() {
        let directory = repository(&[("code.txt", "r-1 r-2 r-10 pre_r-1 r-1tail\n")]);
        let references = find_code_references_in(directory.path(), "r-1").unwrap();
        let mapping = HashMap::from([("r-1".into(), "r-2".into()), ("r-2".into(), "r-1".into())]);
        assert_eq!(patch_code_mappings(&mapping, &references).unwrap(), 1);
        assert_eq!(
            fs::read_to_string(directory.path().join("code.txt")).unwrap(),
            "r-2 r-1 r-10 pre_r-1 r-1tail\n"
        );
    }

    #[cfg(unix)]
    #[test]
    fn nul_output_preserves_colons_and_newlines_in_filenames() {
        let name = "src/colon:and\nnewline.txt";
        let directory = repository(&[(name, "r-1\n")]);
        let references = find_code_references_in(directory.path(), "r-1").unwrap();
        assert!(references.matches.contains_key(name));
        patch_code_files("r-1", "r-2", &references).unwrap();
        assert_eq!(
            fs::read_to_string(directory.path().join(name)).unwrap(),
            "r-2\n"
        );
    }
}
