//! Bidirectional sync between markdown and JSONL formats
//!
//! This module implements bidirectional synchronization between:
//! - Markdown files (.minibeads/issues/*.md) - human-friendly, git-mergeable
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
//!    - no_change: Skip (canonical content matches)
//!    - conflict: Skip with warning (same timestamp, different content)
//! 4. Apply changes bidirectionally
//! 5. Preserve timestamps when writing (set file mtime)

use crate::lock::Lock;
use crate::storage::{issue_file_paths_in, Storage};
use crate::transaction::{atomic_write, FileTransaction};
use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use crate::format::markdown_to_issue;
use crate::paths::{ensure_contained, IssueId};
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
    /// No changes needed (canonical content matches)
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
    #[serde(default)]
    pub dry_run: bool,
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

    for (issue_id, path) in issue_file_paths_in(&issues_dir)? {
        IssueId::parse(&issue_id)?;
        ensure_contained(beads_dir, &path)?;
        // Get filesystem mtime
        let metadata = fs::metadata(&path)
            .with_context(|| format!("Failed to get metadata for {}", path.display()))?;
        let mtime = metadata
            .modified()
            .with_context(|| format!("Failed to get mtime for {}", path.display()))?;

        // Read and parse markdown file
        let content = fs::read_to_string(&path)
            .with_context(|| format!("Failed to read {}", path.display()))?;
        let issue = markdown_to_issue(&issue_id, &content)
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

        IssueId::parse(&issue.id)
            .with_context(|| format!("Invalid ID on JSONL line {}", line_num + 1))?;
        anyhow::ensure!(
            !result.contains_key(&issue.id),
            "Duplicate JSONL issue ID on line {}: {}",
            line_num + 1,
            issue.id
        );
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
    #[allow(dead_code)]
    pub fn analyze(
        &self,
        markdown_issues: HashMap<String, MarkdownIssue>,
        jsonl_issues: HashMap<String, JsonlIssue>,
    ) -> Result<SyncPlan> {
        self.analyze_ref(&markdown_issues, &jsonl_issues)
    }

    pub fn analyze_ref(
        &self,
        markdown_issues: &HashMap<String, MarkdownIssue>,
        jsonl_issues: &HashMap<String, JsonlIssue>,
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
                    if same_content(&md_issue.issue, &json_issue.issue)? {
                        plan.no_change.push(id.clone());
                        continue;
                    }
                    // Timestamps only choose a winner when the content differs.
                    match self.compare_timestamps(md_issue.mtime, json_issue.updated_at) {
                        std::cmp::Ordering::Greater => {
                            plan.markdown_newer.push(id.clone());
                        }
                        std::cmp::Ordering::Less => {
                            plan.jsonl_newer.push(id.clone());
                        }
                        std::cmp::Ordering::Equal => {
                            plan.conflicts.push(id.clone());
                        }
                    }
                }
                (None, None) => unreachable!("ID came from one of the maps"),
            }
        }

        Ok(plan)
    }

    /// Apply to the default JSONL store. Kept for library callers.
    #[allow(dead_code)]
    pub fn apply(
        &self,
        plan: &SyncPlan,
        markdown_issues: &HashMap<String, MarkdownIssue>,
        jsonl_issues: &HashMap<String, JsonlIssue>,
        beads_dir: &Path,
        dry_run: bool,
    ) -> Result<SyncReport> {
        self.apply_to_path(
            plan,
            markdown_issues,
            jsonl_issues,
            beads_dir,
            &beads_dir.join("issues.jsonl"),
            dry_run,
        )
    }

    /// Validate the analyzed snapshot under the database lock, then apply it.
    /// An edit made between analysis and lock acquisition requires a fresh plan.
    pub fn apply_to_path(
        &self,
        plan: &SyncPlan,
        markdown_issues: &HashMap<String, MarkdownIssue>,
        jsonl_issues: &HashMap<String, JsonlIssue>,
        beads_dir: &Path,
        jsonl_path: &Path,
        dry_run: bool,
    ) -> Result<SyncReport> {
        let _lock = Lock::acquire(beads_dir)?;
        let fresh_markdown = load_markdown_issues(beads_dir)?;
        let fresh_jsonl = load_jsonl_issues(jsonl_path)?;
        anyhow::ensure!(
            same_markdown_snapshot(markdown_issues, &fresh_markdown)?
                && same_jsonl_snapshot(jsonl_issues, &fresh_jsonl)?,
            "Sync inputs changed after analysis; rerun sync to avoid overwriting a concurrent edit"
        );

        let storage = Storage::open(beads_dir.to_path_buf())?;
        let mut transaction = FileTransaction::new(beads_dir);
        let mut planned_ids = std::collections::HashSet::new();
        let report = SyncReport {
            dry_run,
            created_in_jsonl: plan.markdown_only.len(),
            created_in_markdown: plan.jsonl_only.len(),
            updated_jsonl: plan.markdown_newer.len(),
            updated_markdown: plan.jsonl_newer.len(),
            skipped_conflicts: plan.conflicts.len(),
            errors: plan
                .conflicts
                .iter()
                .map(|id| format!("Conflict skipped: {id}"))
                .collect(),
        };
        let mut output: std::collections::BTreeMap<&str, &Issue> = jsonl_issues
            .iter()
            .map(|(id, record)| (id.as_str(), &record.issue))
            .collect();

        // Prepare and validate every write before touching either store.
        for (ids, creating) in [(&plan.jsonl_only, true), (&plan.jsonl_newer, false)] {
            for id in ids {
                anyhow::ensure!(planned_ids.insert(id), "Duplicate sync action for {id}");
                let record = jsonl_issues
                    .get(id)
                    .context("Sync plan references a missing JSONL record")?;
                anyhow::ensure!(
                    &record.issue.id == id,
                    "JSONL record ID does not match its key"
                );
                IssueId::parse(id)?;
                anyhow::ensure!(
                    markdown_issues.contains_key(id) != creating,
                    "Stale Markdown action for {id}"
                );
                let path = storage.existing_or_configured_issue_path(id)?;
                transaction.write(
                    path.clone(),
                    crate::format::issue_to_markdown(&record.issue)?.into_bytes(),
                );
                transaction.set_mtime(path, record.updated_at.into());
            }
        }
        for (ids, creating) in [(&plan.markdown_only, true), (&plan.markdown_newer, false)] {
            for id in ids {
                anyhow::ensure!(planned_ids.insert(id), "Duplicate sync action for {id}");
                let record = markdown_issues
                    .get(id)
                    .context("Sync plan references a missing Markdown record")?;
                anyhow::ensure!(
                    &record.issue.id == id,
                    "Markdown record ID does not match its key"
                );
                IssueId::parse(id)?;
                anyhow::ensure!(
                    jsonl_issues.contains_key(id) != creating,
                    "Stale JSONL action for {id}"
                );
                output.insert(id, &record.issue);
            }
        }
        let jsonl_content = if report.created_in_jsonl + report.updated_jsonl > 0 {
            let mut content = Vec::new();
            for issue in output.values() {
                serde_json::to_writer(&mut content, issue)?;
                content.push(b'\n');
            }
            Some(content)
        } else {
            None
        };

        if dry_run {
            return Ok(report);
        }
        if let Some(content) = jsonl_content {
            if jsonl_path.starts_with(beads_dir) {
                ensure_contained(beads_dir, jsonl_path)?;
                transaction.write(jsonl_path.to_path_buf(), content);
            } else {
                // A user-selected external JSONL path is replaced atomically
                // before committing Markdown. It is never put in a database
                // recovery journal, which must only restore database files.
                atomic_write(jsonl_path, &content)?;
            }
        }
        transaction.commit()?;
        // Conflicts are included in the report and cause the CLI to return a
        // nonzero status, even when other non-conflicting records were synced.
        Ok(report)
    }
}

impl Default for SyncEngine {
    fn default() -> Self {
        Self::new()
    }
}

pub(crate) fn canonical_issue(issue: &Issue) -> Result<serde_json::Value> {
    let mut value = serde_json::to_value(issue)?;
    let object = value
        .as_object_mut()
        .context("Issue must serialize to an object")?;
    object.remove("dependents"); // Derived from the rest of the database.
    if let Some(dependencies) = object
        .get_mut("dependencies")
        .and_then(serde_json::Value::as_array_mut)
    {
        dependencies.sort_by(|a, b| a["id"].as_str().cmp(&b["id"].as_str()));
    }
    if let Some(labels) = object
        .get_mut("labels")
        .and_then(serde_json::Value::as_array_mut)
    {
        labels.sort_by(|a, b| a.as_str().cmp(&b.as_str()));
    }
    Ok(value)
}

fn same_content(first: &Issue, second: &Issue) -> Result<bool> {
    let mut first = canonical_issue(first)?;
    let mut second = canonical_issue(second)?;
    first.as_object_mut().unwrap().remove("updated_at");
    second.as_object_mut().unwrap().remove("updated_at");
    Ok(first == second)
}

fn same_markdown_snapshot(
    before: &HashMap<String, MarkdownIssue>,
    after: &HashMap<String, MarkdownIssue>,
) -> Result<bool> {
    if before.len() != after.len() {
        return Ok(false);
    }
    for (id, previous) in before {
        let Some(current) = after.get(id) else {
            return Ok(false);
        };
        if previous.mtime != current.mtime
            || previous.path != current.path
            || canonical_issue(&previous.issue)? != canonical_issue(&current.issue)?
        {
            return Ok(false);
        }
    }
    Ok(true)
}

fn same_jsonl_snapshot(
    before: &HashMap<String, JsonlIssue>,
    after: &HashMap<String, JsonlIssue>,
) -> Result<bool> {
    if before.len() != after.len() {
        return Ok(false);
    }
    for (id, previous) in before {
        let Some(current) = after.get(id) else {
            return Ok(false);
        };
        if previous.updated_at != current.updated_at
            || canonical_issue(&previous.issue)? != canonical_issue(&current.issue)?
        {
            return Ok(false);
        }
    }
    Ok(true)
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
