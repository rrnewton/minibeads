//! Bidirectional sync between markdown and JSONL formats
//!
//! This module implements bidirectional synchronization between:
//! - Markdown files (.beads/issues/*.md) - human-friendly, git-mergeable
//! - JSONL file (issues.jsonl) - machine-friendly, upstream bd compatible
//!
//! ## Architecture
//!
//! Either format can be modified independently, and `bd sync` merges changes
//! bidirectionally using timestamps:
//! - **Markdown**: Uses filesystem mtime (last modified time)
//! - **JSONL**: Uses updated_at field from JSON
//!
//! ## Algorithm
//!
//! 1. Parse all markdown issues with their filesystem mtimes
//! 2. Parse all JSONL issues with their updated_at timestamps
//! 3. Compare timestamps to classify each issue:
//!    - markdown_only: Create in JSONL
//!    - jsonl_only: Create in markdown
//!    - markdown_newer: Update JSONL from markdown
//!    - jsonl_newer: Update markdown from JSONL
//!    - no_change: Skip (timestamps match)
//!    - conflict: Skip with warning (same timestamp, different content)
//! 4. Apply changes bidirectionally
//! 5. Preserve timestamps when writing (set file mtime)

use anyhow::{anyhow, Context, Result};
use chrono::{DateTime, Utc};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use crate::format::markdown_to_issue;
use crate::types::Issue;

/// Timestamped issue from markdown (with filesystem mtime)
#[derive(Debug, Clone)]
pub struct MarkdownIssue {
    pub issue: Issue,
    pub mtime: SystemTime,
    #[allow(dead_code)]
    pub path: PathBuf,
}

/// Timestamped issue from JSONL (with updated_at field)
#[derive(Debug, Clone)]
pub struct JsonlIssue {
    pub issue: Issue,
    pub updated_at: DateTime<Utc>,
}

/// Issue classification for sync
#[allow(dead_code)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum IssueAction {
    /// Issue exists only in markdown - create in JSONL
    MarkdownOnly,
    /// Issue exists only in JSONL - create in markdown
    JsonlOnly,
    /// Markdown is newer - update JSONL from markdown
    MarkdownNewer,
    /// JSONL is newer - update markdown from JSONL
    JsonlNewer,
    /// No changes needed (timestamps match)
    NoChange,
    /// Conflict detected (same timestamp, different content)
    Conflict,
}

/// Sync plan categorizing all issues
#[derive(Debug, Default)]
pub struct SyncPlan {
    pub markdown_only: Vec<String>,
    pub jsonl_only: Vec<String>,
    pub markdown_newer: Vec<String>,
    pub jsonl_newer: Vec<String>,
    pub no_change: Vec<String>,
    pub conflicts: Vec<String>,
}

impl SyncPlan {
    pub fn is_empty(&self) -> bool {
        self.markdown_only.is_empty()
            && self.jsonl_only.is_empty()
            && self.markdown_newer.is_empty()
            && self.jsonl_newer.is_empty()
            && self.conflicts.is_empty()
    }

    #[allow(dead_code)]
    pub fn total_changes(&self) -> usize {
        self.markdown_only.len()
            + self.jsonl_only.len()
            + self.markdown_newer.len()
            + self.jsonl_newer.len()
    }
}

/// Sync execution report
#[derive(Debug, Default, serde::Serialize, serde::Deserialize)]
pub struct SyncReport {
    pub created_in_jsonl: usize,
    pub created_in_markdown: usize,
    pub updated_jsonl: usize,
    pub updated_markdown: usize,
    pub skipped_conflicts: usize,
    pub errors: Vec<String>,
}

impl SyncReport {
    pub fn total_changes(&self) -> usize {
        self.created_in_jsonl
            + self.created_in_markdown
            + self.updated_jsonl
            + self.updated_markdown
    }
}

/// Load all markdown issues with their filesystem mtimes
pub fn load_markdown_issues(beads_dir: &Path) -> Result<HashMap<String, MarkdownIssue>> {
    let issues_dir = beads_dir.join("issues");

    if !issues_dir.exists() {
        return Ok(HashMap::new());
    }

    let mut result = HashMap::new();

    for entry in fs::read_dir(&issues_dir).context("Failed to read issues directory")? {
        let entry = entry?;
        let path = entry.path();

        if path.extension().and_then(|e| e.to_str()) != Some("md") {
            continue;
        }

        // Get filesystem mtime
        let metadata = fs::metadata(&path)
            .with_context(|| format!("Failed to get metadata for {}", path.display()))?;
        let mtime = metadata
            .modified()
            .with_context(|| format!("Failed to get mtime for {}", path.display()))?;

        // Extract issue ID from filename (remove .md extension)
        let issue_id = path
            .file_stem()
            .and_then(|s| s.to_str())
            .ok_or_else(|| anyhow!("Invalid filename: {}", path.display()))?;

        // Read and parse markdown file
        let content = fs::read_to_string(&path)
            .with_context(|| format!("Failed to read {}", path.display()))?;
        let issue = markdown_to_issue(issue_id, &content)
            .with_context(|| format!("Failed to parse {}", path.display()))?;

        result.insert(issue.id.clone(), MarkdownIssue { issue, mtime, path });
    }

    Ok(result)
}

/// Load all JSONL issues with their updated_at timestamps
pub fn load_jsonl_issues(jsonl_path: &Path) -> Result<HashMap<String, JsonlIssue>> {
    if !jsonl_path.exists() {
        return Ok(HashMap::new());
    }

    let content = fs::read_to_string(jsonl_path)
        .with_context(|| format!("Failed to read {}", jsonl_path.display()))?;

    let mut result = HashMap::new();

    for (line_num, line) in content.lines().enumerate() {
        if line.trim().is_empty() {
            continue;
        }

        let issue: Issue = serde_json::from_str(line).with_context(|| {
            format!(
                "Failed to parse line {} in {}",
                line_num + 1,
                jsonl_path.display()
            )
        })?;

        result.insert(
            issue.id.clone(),
            JsonlIssue {
                updated_at: issue.updated_at,
                issue,
            },
        );
    }

    Ok(result)
}

/// Main sync engine
pub struct SyncEngine {
    /// Tolerance for timestamp comparison (in milliseconds)
    /// Allows small differences due to filesystem precision
    tolerance_ms: u64,
}

impl SyncEngine {
    /// Create a new sync engine with default tolerance (1 second)
    pub fn new() -> Self {
        Self { tolerance_ms: 1000 }
    }

    /// Create a sync engine with custom tolerance
    #[allow(dead_code)]
    pub fn with_tolerance_ms(tolerance_ms: u64) -> Self {
        Self { tolerance_ms }
    }

    /// Compare two timestamps and determine which is newer
    ///
    /// Returns:
    /// - Ordering::Less if markdown is older
    /// - Ordering::Greater if markdown is newer
    /// - Ordering::Equal if within tolerance
    fn compare_timestamps(
        &self,
        mtime: SystemTime,
        jsonl_time: DateTime<Utc>,
    ) -> std::cmp::Ordering {
        // Convert JSONL DateTime to SystemTime
        let jsonl_systime: SystemTime = jsonl_time.into();

        // Compare with tolerance
        let diff = match mtime.duration_since(jsonl_systime) {
            Ok(d) => d.as_millis() as i64,
            Err(e) => -(e.duration().as_millis() as i64),
        };

        let tolerance = self.tolerance_ms as i64;

        if diff > tolerance {
            std::cmp::Ordering::Greater // markdown newer
        } else if diff < -tolerance {
            std::cmp::Ordering::Less // jsonl newer
        } else {
            std::cmp::Ordering::Equal // within tolerance
        }
    }

    /// Analyze markdown and JSONL issues to create a sync plan
    pub fn analyze(
        &self,
        markdown_issues: HashMap<String, MarkdownIssue>,
        jsonl_issues: HashMap<String, JsonlIssue>,
    ) -> Result<SyncPlan> {
        let mut plan = SyncPlan::default();

        // Get all unique issue IDs
        let all_ids: std::collections::HashSet<String> = markdown_issues
            .keys()
            .chain(jsonl_issues.keys())
            .cloned()
            .collect();

        for id in all_ids {
            let md = markdown_issues.get(&id);
            let json = jsonl_issues.get(&id);

            match (md, json) {
                (Some(_), None) => {
                    plan.markdown_only.push(id.clone());
                }
                (None, Some(_)) => {
                    plan.jsonl_only.push(id.clone());
                }
                (Some(md_issue), Some(json_issue)) => {
                    // Compare timestamps
                    match self.compare_timestamps(md_issue.mtime, json_issue.updated_at) {
                        std::cmp::Ordering::Greater => {
                            plan.markdown_newer.push(id.clone());
                        }
                        std::cmp::Ordering::Less => {
                            plan.jsonl_newer.push(id.clone());
                        }
                        std::cmp::Ordering::Equal => {
                            // Timestamps match - assume no changes for now
                            // TODO(minibeads-19): Implement content-based conflict detection
                            plan.no_change.push(id.clone());
                        }
                    }
                }
                (None, None) => unreachable!("ID came from one of the maps"),
            }
        }

        Ok(plan)
    }

    /// Apply a sync plan (create/update files bidirectionally)
    pub fn apply(
        &self,
        plan: &SyncPlan,
        markdown_issues: &HashMap<String, MarkdownIssue>,
        jsonl_issues: &HashMap<String, JsonlIssue>,
        beads_dir: &Path,
        dry_run: bool,
    ) -> Result<SyncReport> {
        let mut report = SyncReport::default();
        let issues_dir = beads_dir.join("issues");
        let jsonl_path = beads_dir.join("issues.jsonl");

        // Ensure issues directory exists
        if !dry_run && !issues_dir.exists() {
            fs::create_dir_all(&issues_dir).context("Failed to create issues directory")?;
        }

        // 1. Create markdown files from JSONL-only issues
        for id in &plan.jsonl_only {
            if let Some(json_issue) = jsonl_issues.get(id) {
                if dry_run {
                    println!("[DRY RUN] Would create markdown: {}.md", id);
                } else {
                    match self.write_markdown_issue(
                        &issues_dir,
                        &json_issue.issue,
                        json_issue.updated_at,
                    ) {
                        Ok(_) => report.created_in_markdown += 1,
                        Err(e) => report
                            .errors
                            .push(format!("Failed to create {}.md: {}", id, e)),
                    }
                }
            }
        }

        // 2. Update markdown files from JSONL when JSONL is newer
        for id in &plan.jsonl_newer {
            if let Some(json_issue) = jsonl_issues.get(id) {
                if dry_run {
                    println!(
                        "[DRY RUN] Would update markdown: {}.md (JSONL is newer)",
                        id
                    );
                } else {
                    match self.write_markdown_issue(
                        &issues_dir,
                        &json_issue.issue,
                        json_issue.updated_at,
                    ) {
                        Ok(_) => report.updated_markdown += 1,
                        Err(e) => report
                            .errors
                            .push(format!("Failed to update {}.md: {}", id, e)),
                    }
                }
            }
        }

        // 3. Create JSONL entries from markdown-only issues
        for id in &plan.markdown_only {
            if let Some(md_issue) = markdown_issues.get(id) {
                if dry_run {
                    println!("[DRY RUN] Would create JSONL entry: {}", id);
                } else {
                    match self.append_jsonl_issue(&jsonl_path, &md_issue.issue) {
                        Ok(_) => report.created_in_jsonl += 1,
                        Err(e) => report
                            .errors
                            .push(format!("Failed to create JSONL entry {}: {}", id, e)),
                    }
                }
            }
        }

        // 4. Update JSONL entries when markdown is newer
        for id in &plan.markdown_newer {
            if let Some(md_issue) = markdown_issues.get(id) {
                if dry_run {
                    println!(
                        "[DRY RUN] Would update JSONL entry: {} (markdown is newer)",
                        id
                    );
                } else {
                    match self.update_jsonl_issue(&jsonl_path, &md_issue.issue) {
                        Ok(_) => report.updated_jsonl += 1,
                        Err(e) => report
                            .errors
                            .push(format!("Failed to update JSONL entry {}: {}", id, e)),
                    }
                }
            }
        }

        // 5. Report conflicts (skip them)
        for id in &plan.conflicts {
            report.skipped_conflicts += 1;
            if dry_run {
                println!("[DRY RUN] Would skip conflict: {}", id);
            } else {
                report.errors.push(format!("Conflict skipped: {}", id));
            }
        }

        Ok(report)
    }

    /// Write an issue to markdown file with specified timestamp
    fn write_markdown_issue(
        &self,
        issues_dir: &Path,
        issue: &Issue,
        timestamp: DateTime<Utc>,
    ) -> Result<()> {
        use crate::format::issue_to_markdown;

        let path = issues_dir.join(format!("{}.md", issue.id));
        let content = issue_to_markdown(issue)?;

        fs::write(&path, content).with_context(|| format!("Failed to write {}", path.display()))?;

        // Set the file's mtime to match the JSONL timestamp
        let systime: SystemTime = timestamp.into();
        filetime::set_file_mtime(&path, filetime::FileTime::from_system_time(systime))
            .with_context(|| format!("Failed to set mtime for {}", path.display()))?;

        Ok(())
    }

    /// Append a new issue to JSONL file
    fn append_jsonl_issue(&self, jsonl_path: &Path, issue: &Issue) -> Result<()> {
        let json = serde_json::to_string(issue).context("Failed to serialize issue to JSON")?;

        let mut file = fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(jsonl_path)
            .with_context(|| format!("Failed to open {}", jsonl_path.display()))?;

        use std::io::Write;
        writeln!(file, "{}", json).context("Failed to write to JSONL file")?;

        Ok(())
    }

    /// Update an existing issue in JSONL file
    fn update_jsonl_issue(&self, jsonl_path: &Path, issue: &Issue) -> Result<()> {
        // Read all issues
        let mut all_issues = if jsonl_path.exists() {
            load_jsonl_issues(jsonl_path)?
        } else {
            HashMap::new()
        };

        // Update or insert the issue
        all_issues.insert(
            issue.id.clone(),
            JsonlIssue {
                issue: issue.clone(),
                updated_at: issue.updated_at,
            },
        );

        // Write all issues back
        let mut lines: Vec<String> = all_issues
            .values()
            .map(|json_issue| serde_json::to_string(&json_issue.issue))
            .collect::<Result<Vec<_>, _>>()
            .context("Failed to serialize issues")?;

        lines.sort(); // Ensure deterministic order
        let content = lines.join("\n") + "\n";

        fs::write(jsonl_path, content)
            .with_context(|| format!("Failed to write {}", jsonl_path.display()))?;

        Ok(())
    }
}

impl Default for SyncEngine {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Duration;

    #[test]
    fn test_compare_timestamps_equal() {
        let engine = SyncEngine::new();
        let now = Utc::now();
        let systime: SystemTime = now.into();

        assert_eq!(
            engine.compare_timestamps(systime, now),
            std::cmp::Ordering::Equal
        );
    }

    #[test]
    fn test_compare_timestamps_markdown_newer() {
        let engine = SyncEngine::new();
        let now = Utc::now();
        let future = now + Duration::seconds(10);
        let systime: SystemTime = future.into();

        assert_eq!(
            engine.compare_timestamps(systime, now),
            std::cmp::Ordering::Greater
        );
    }

    #[test]
    fn test_compare_timestamps_jsonl_newer() {
        let engine = SyncEngine::new();
        let now = Utc::now();
        let past = now - Duration::seconds(10);
        let systime: SystemTime = past.into();

        assert_eq!(
            engine.compare_timestamps(systime, now),
            std::cmp::Ordering::Less
        );
    }

    #[test]
    fn test_compare_timestamps_within_tolerance() {
        let engine = SyncEngine::with_tolerance_ms(1000);
        let now = Utc::now();
        let slightly_future = now + Duration::milliseconds(500);
        let systime: SystemTime = slightly_future.into();

        assert_eq!(
            engine.compare_timestamps(systime, now),
            std::cmp::Ordering::Equal
        );
    }
}
