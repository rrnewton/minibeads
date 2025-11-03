use anyhow::{Context, Result};
use std::collections::HashMap;
use std::io::{self, Write};
use std::process::{Command, Stdio};

/// Result of searching for issue ID references in code
#[derive(Debug)]
pub struct CodeReferences {
    /// Map from file path to list of (line_number, line_content) tuples
    pub matches: HashMap<String, Vec<(usize, String)>>,
    /// Total number of matches found
    pub total_matches: usize,
}

impl CodeReferences {
    fn new() -> Self {
        Self {
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
/// Excludes the .beads directory and uses word boundaries to match issue IDs.
/// Returns a mapping from file paths to matching lines with their line numbers.
pub fn find_code_references(issue_id: &str) -> Result<CodeReferences> {
    // Build the grep pattern with word boundaries
    // Use `\<` and `\>` for word boundaries in git grep (compatible with both GNU and BSD grep)
    let pattern = format!(r"\<{}\>", regex::escape(issue_id));

    // Run git grep to find matches
    // -n: show line numbers
    // -I: ignore binary files
    // --no-color: don't colorize output
    // -- ':(exclude).beads': exclude .beads directory
    let output = Command::new("git")
        .args([
            "grep",
            "-n",
            "-I",
            "--no-color",
            &pattern,
            "--",
            ":(exclude).beads/*",
        ])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .context("Failed to execute git grep")?;

    let mut references = CodeReferences::new();

    if !output.status.success() {
        // Exit status 1 means no matches found, which is ok
        if output.status.code() == Some(1) {
            return Ok(references);
        }
        // Other non-zero exit codes are errors
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("git grep failed: {}", stderr);
    }

    // Parse the output
    // Format: filename:line_number:line_content
    let stdout = String::from_utf8_lossy(&output.stdout);
    for line in stdout.lines() {
        if let Some((file_and_line, content)) = line.split_once(':') {
            if let Some((file, line_num_str)) = file_and_line.rsplit_once(':') {
                if let Ok(line_num) = line_num_str.parse::<usize>() {
                    references
                        .matches
                        .entry(file.to_string())
                        .or_default()
                        .push((line_num, content.to_string()));
                    references.total_matches += 1;
                }
            }
        }
    }

    Ok(references)
}

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

/// Patch code files to replace old issue ID with new issue ID
///
/// Uses sed to perform word-boundary-aware replacement in each file.
/// This modifies files in place.
pub fn patch_code_files(old_id: &str, new_id: &str, references: &CodeReferences) -> Result<usize> {
    let mut files_patched = 0;

    // Build sed pattern with word boundaries
    // Use `\<` and `\>` for word boundaries (compatible with both GNU and BSD sed)
    let sed_pattern = format!(r"s/\<{}\>/{}/g", regex::escape(old_id), new_id);

    for file in references.matches.keys() {
        // Run sed in-place replacement
        let status = Command::new("sed")
            .args(["-i", &sed_pattern, file])
            .status()
            .context(format!("Failed to patch file: {}", file))?;

        if !status.success() {
            anyhow::bail!("sed failed to patch file: {}", file);
        }

        files_patched += 1;
    }

    Ok(files_patched)
}

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

/// Patch code for multiple ID mappings (used by mb-migrate)
///
/// For each old_id -> new_id mapping, searches for references and patches them.
/// Only asks for confirmation if there are actually matches found.
///
/// Returns the total number of files patched.
pub fn patch_code_for_migration(id_mapping: &HashMap<String, String>) -> Result<usize> {
    // Check if we're in an interactive TTY
    if !is_interactive_tty() {
        eprintln!("Warning: --mb-patch-code requires an interactive TTY. Skipping code patching.");
        return Ok(0);
    }

    let mut total_files_patched = 0;

    for (old_id, new_id) in id_mapping {
        // Search for references
        let references = find_code_references(old_id)?;

        // Skip if no references found
        if references.total_matches == 0 {
            continue;
        }

        // Ask for confirmation
        if !confirm_code_patch(old_id, new_id, &references)? {
            println!("Skipping {} -> {}", old_id, new_id);
            continue;
        }

        // Patch files
        let files_patched = patch_code_files(old_id, new_id, &references)?;
        total_files_patched += files_patched;

        println!(
            "Patched {} file(s) for {} -> {}",
            files_patched, old_id, new_id
        );
    }

    if total_files_patched > 0 {
        println!(
            "\nTotal: patched {} file(s) in working copy.",
            total_files_patched
        );
    }

    Ok(total_files_patched)
}
