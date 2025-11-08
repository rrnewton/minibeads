use crate::format::{issue_to_markdown, markdown_to_issue};
use crate::hash;
use crate::lock::Lock;
use crate::types::{BlockedIssue, DependencyType, Issue, IssueType, Stats, Status};
use anyhow::{Context, Result};
use regex::Regex;
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

pub struct Storage {
    beads_dir: PathBuf,
    issues_dir: PathBuf,
}

/// Replace issue ID references in text fields using word boundaries
///
/// This function replaces all occurrences of issue IDs in text, but only when they appear
/// as standalone tokens (delimited by non-alphanumeric characters or word boundaries).
/// This prevents matching IDs that are embedded in longer strings.
///
/// Uses a HashMap for O(1) lookup of replacement mappings.
fn replace_issue_ids_in_text(text: &str, id_mapping: &HashMap<String, String>) -> String {
    if text.is_empty() || id_mapping.is_empty() {
        return text.to_string();
    }

    // Build a regex pattern that matches any issue ID with word boundaries
    // Pattern: \b(prefix1-suffix1|prefix2-suffix2|...)\b
    let mut patterns: Vec<String> = id_mapping.keys().map(|id| regex::escape(id)).collect();

    if patterns.is_empty() {
        return text.to_string();
    }

    // Sort by length (longest first) to avoid partial matches
    patterns.sort_by_key(|b| std::cmp::Reverse(b.len()));

    let pattern = format!(r"\b({})\b", patterns.join("|"));

    // Compile regex (note: in production code, this could be cached)
    let re = match Regex::new(&pattern) {
        Ok(r) => r,
        Err(_) => return text.to_string(), // Fallback: return original text
    };

    // Replace all matches using the mapping
    re.replace_all(text, |caps: &regex::Captures| {
        let matched_id = &caps[1];
        id_mapping
            .get(matched_id)
            .cloned()
            .unwrap_or_else(|| matched_id.to_string())
    })
    .to_string()
}

/// Apply ID replacements to all text fields of an issue
fn replace_ids_in_issue_text(issue: &mut Issue, id_mapping: &HashMap<String, String>) {
    issue.title = replace_issue_ids_in_text(&issue.title, id_mapping);
    issue.description = replace_issue_ids_in_text(&issue.description, id_mapping);
    issue.design = replace_issue_ids_in_text(&issue.design, id_mapping);
    issue.acceptance_criteria = replace_issue_ids_in_text(&issue.acceptance_criteria, id_mapping);
    issue.notes = replace_issue_ids_in_text(&issue.notes, id_mapping);
}

impl Storage {
    /// Get the beads directory path
    pub fn get_beads_dir(&self) -> PathBuf {
        self.beads_dir.clone()
    }
}

impl Storage {
    /// Open storage at the given .beads directory
    pub fn open(beads_dir: PathBuf) -> Result<Self> {
        let issues_dir = beads_dir.join("issues");

        // Create directories if they don't exist
        fs::create_dir_all(&issues_dir).context("Failed to create issues directory")?;

        // Validate and ensure config.yaml exists
        let config_path = beads_dir.join("config.yaml");
        if !config_path.exists() {
            // Try to infer prefix and create config
            let prefix = infer_prefix(&beads_dir).unwrap_or_else(|| "bd".to_string());
            let mut config = HashMap::new();
            config.insert("issue-prefix".to_string(), prefix);
            let config_yaml = serde_yaml::to_string(&config)?;
            fs::write(&config_path, config_yaml).context("Failed to create config.yaml")?;
        } else {
            // Validate that config has issue-prefix
            let content = fs::read_to_string(&config_path).context("Failed to read config.yaml")?;
            let config: HashMap<String, String> =
                serde_yaml::from_str(&content).context("Failed to parse config.yaml")?;
            if !config.contains_key("issue-prefix") {
                anyhow::bail!("config.yaml is missing required 'issue-prefix' field");
            }
        }

        // Ensure config-minibeads.yaml exists with defaults (don't clobber if exists)
        let minibeads_config_path = beads_dir.join("config-minibeads.yaml");
        if !minibeads_config_path.exists() {
            create_minibeads_config(&beads_dir, false)?; // Default to false for existing repos
        }

        // Ensure .gitignore exists and has required entries
        ensure_gitignore(&beads_dir)?;

        Ok(Self {
            beads_dir,
            issues_dir,
        })
    }

    /// Initialize a new beads database
    pub fn init(beads_dir: PathBuf, prefix: Option<String>, mb_hash_ids: bool) -> Result<Self> {
        // Create .beads directory
        fs::create_dir_all(&beads_dir).context("Failed to create .beads directory")?;

        // Create issues directory
        let issues_dir = beads_dir.join("issues");
        fs::create_dir_all(&issues_dir).context("Failed to create issues directory")?;

        // Determine prefix
        let prefix = prefix
            .or_else(|| infer_prefix(&beads_dir))
            .unwrap_or_else(|| "bd".to_string());

        // Create config.yaml with only upstream-compatible options
        let config_path = beads_dir.join("config.yaml");
        let mut config = HashMap::new();
        config.insert("issue-prefix".to_string(), prefix);
        let config_yaml = serde_yaml::to_string(&config)?;
        fs::write(&config_path, config_yaml).context("Failed to write config.yaml")?;

        // Create config-minibeads.yaml with minibeads-specific options
        create_minibeads_config(&beads_dir, mb_hash_ids)?;

        // Ensure .gitignore exists and has required entries
        ensure_gitignore(&beads_dir)?;

        Ok(Self {
            beads_dir,
            issues_dir,
        })
    }

    /// Get the issue prefix from config
    pub fn get_prefix(&self) -> Result<String> {
        let config_path = self.beads_dir.join("config.yaml");

        if !config_path.exists() {
            // Try to infer from existing issues
            return self.infer_prefix_from_issues();
        }

        let content = fs::read_to_string(&config_path).context("Failed to read config.yaml")?;
        let config: HashMap<String, String> =
            serde_yaml::from_str(&content).context("Failed to parse config.yaml")?;

        config
            .get("issue-prefix")
            .cloned()
            .ok_or_else(|| anyhow::anyhow!("issue-prefix not found in config.yaml"))
    }

    /// Check if hash-based IDs are enabled in config-minibeads.yaml
    fn use_hash_ids(&self) -> Result<bool> {
        let config_path = self.beads_dir.join("config-minibeads.yaml");

        if !config_path.exists() {
            return Ok(false); // Default to false if no config
        }

        let content =
            fs::read_to_string(&config_path).context("Failed to read config-minibeads.yaml")?;
        let config: HashMap<String, String> =
            serde_yaml::from_str(&content).context("Failed to parse config-minibeads.yaml")?;

        // Parse mb-hash-ids field (default to false if not present)
        match config.get("mb-hash-ids") {
            Some(value) => Ok(value == "true"),
            None => Ok(false),
        }
    }

    /// Infer prefix from existing issues in the filesystem
    fn infer_prefix_from_issues(&self) -> Result<String> {
        let entries = fs::read_dir(&self.issues_dir).context("Failed to read issues directory")?;

        let mut prefixes = HashMap::new();
        for entry in entries {
            let entry = entry?;
            let name = entry.file_name();
            let name_str = name.to_string_lossy();

            if let Some(issue_id) = name_str.strip_suffix(".md") {
                if let Some(pos) = issue_id.rfind('-') {
                    let prefix = &issue_id[..pos];
                    *prefixes.entry(prefix.to_string()).or_insert(0) += 1;
                }
            }
        }

        // Return most common prefix, or "bd" if none found
        prefixes
            .into_iter()
            .max_by_key(|(_, count)| *count)
            .map(|(prefix, _)| prefix)
            .ok_or_else(|| anyhow::anyhow!("No issues found to infer prefix"))
    }

    /// Get the next issue number
    fn get_next_number(&self, prefix: &str) -> Result<u32> {
        let entries = fs::read_dir(&self.issues_dir).context("Failed to read issues directory")?;

        let mut max_num = 0;
        for entry in entries {
            let entry = entry?;
            let name = entry.file_name();
            let name_str = name.to_string_lossy();

            if let Some(issue_id) = name_str.strip_suffix(".md") {
                if let Some(pos) = issue_id.rfind('-') {
                    let issue_prefix = &issue_id[..pos];
                    let num_str = &issue_id[pos + 1..];
                    if issue_prefix == prefix {
                        if let Ok(num) = num_str.parse::<u32>() {
                            max_num = max_num.max(num);
                        }
                    }
                }
            }
        }

        Ok(max_num + 1)
    }

    /// Generate a hash-based ID with adaptive length and collision handling
    fn generate_hash_id(&self, prefix: &str, title: &str, description: &str) -> Result<String> {
        use chrono::Utc;

        let timestamp = Utc::now();

        // Count existing issues to determine adaptive length
        let entries = fs::read_dir(&self.issues_dir).context("Failed to read issues directory")?;
        let issue_count = entries.count();

        // Use hash::generate_hash_id_with_collision_check with filesystem checker
        hash::generate_hash_id_with_collision_check(
            prefix,
            title,
            description,
            timestamp,
            issue_count,
            |candidate| self.issues_dir.join(format!("{}.md", candidate)).exists(),
        )
    }

    /// Create a new issue
    #[allow(clippy::too_many_arguments)]
    pub fn create_issue(
        &self,
        title: String,
        description: String,
        design: Option<String>,
        acceptance: Option<String>,
        priority: i32,
        issue_type: IssueType,
        assignee: Option<String>,
        labels: Vec<String>,
        external_ref: Option<String>,
        id: Option<String>,
        deps: Vec<(String, DependencyType)>,
    ) -> Result<Issue> {
        let _lock = Lock::acquire(&self.beads_dir)?;

        // Generate ID if not provided
        let issue_id = if let Some(id) = id {
            id
        } else {
            let prefix = self.get_prefix()?;
            let use_hash_ids = self.use_hash_ids()?;

            if use_hash_ids {
                // Use hash-based ID generation
                self.generate_hash_id(&prefix, &title, &description)?
            } else {
                // Use sequential numbering
                let num = self.get_next_number(&prefix)?;
                format!("{}-{}", prefix, num)
            }
        };

        // Create issue
        let mut issue = Issue::new(issue_id.clone(), title, priority, issue_type);
        issue.description = if description.is_empty() {
            String::new()
        } else {
            description
        };
        issue.design = design.unwrap_or_default();
        issue.acceptance_criteria = acceptance.unwrap_or_default();
        issue.assignee = assignee.unwrap_or_default();
        issue.labels = labels;
        issue.external_ref = external_ref;

        // Add dependencies (with validation)
        for (dep_id, dep_type) in deps {
            // Validate dependency target exists (warn if not)
            self.validate_dependency_exists(&dep_id);
            issue.depends_on.insert(dep_id, dep_type);
        }

        // Write to file
        let issue_path = self.issues_dir.join(format!("{}.md", issue_id));
        let markdown = issue_to_markdown(&issue)?;
        fs::write(&issue_path, markdown).context("Failed to write issue file")?;

        Ok(issue)
    }

    /// Get an issue by ID
    pub fn get_issue(&self, id: &str) -> Result<Option<Issue>> {
        let _lock = Lock::acquire(&self.beads_dir)?;

        let issue_path = self.issues_dir.join(format!("{}.md", id));
        if !issue_path.exists() {
            return Ok(None);
        }

        let content = fs::read_to_string(&issue_path).context("Failed to read issue file")?;
        let mut issue = markdown_to_issue(id, &content)?;

        // Populate dependents by scanning all issues
        let all_issues = self.list_all_issues_no_dependents()?;
        Self::populate_dependents_for_one(&all_issues, &mut issue);

        Ok(Some(issue))
    }

    /// Helper to load all issues without computing dependents (to avoid recursion)
    fn list_all_issues_no_dependents(&self) -> Result<Vec<Issue>> {
        let entries = fs::read_dir(&self.issues_dir).context("Failed to read issues directory")?;

        let mut issues = Vec::new();
        for entry in entries {
            let entry = entry?;
            let name = entry.file_name();
            let name_str = name.to_string_lossy();

            if !name_str.ends_with(".md") {
                continue;
            }

            let issue_id = &name_str[..name_str.len() - 3];
            let content = fs::read_to_string(entry.path())?;
            let issue = markdown_to_issue(issue_id, &content)?;
            issues.push(issue);
        }

        Ok(issues)
    }

    /// Populate dependents for a single issue given all issues
    fn populate_dependents_for_one(all_issues: &[Issue], target_issue: &mut Issue) {
        use crate::types::Dependency;

        let mut dependents = Vec::new();
        for issue in all_issues {
            if issue.depends_on.contains_key(&target_issue.id) {
                if let Some(dep_type) = issue.depends_on.get(&target_issue.id) {
                    dependents.push(Dependency {
                        id: issue.id.clone(),
                        dep_type: dep_type.to_string(),
                    });
                }
            }
        }

        target_issue.dependents = dependents;
    }

    /// Update an issue
    pub fn update_issue(&self, id: &str, updates: HashMap<String, String>) -> Result<Issue> {
        let _lock = Lock::acquire(&self.beads_dir)?;

        let issue_path = self.issues_dir.join(format!("{}.md", id));
        if !issue_path.exists() {
            anyhow::bail!("Issue not found: {}", id);
        }

        let content = fs::read_to_string(&issue_path).context("Failed to read issue file")?;
        let mut issue = markdown_to_issue(id, &content)?;

        // Apply updates
        for (key, value) in updates {
            match key.as_str() {
                "title" => issue.title = value,
                "description" => issue.description = value,
                "design" => issue.design = value,
                "notes" => issue.notes = value,
                "acceptance_criteria" => issue.acceptance_criteria = value,
                "status" => issue.status = value.parse()?,
                "priority" => issue.priority = value.parse()?,
                "issue_type" => issue.issue_type = value.parse()?,
                "assignee" => issue.assignee = value,
                "external_ref" => {
                    issue.external_ref = if value.is_empty() { None } else { Some(value) }
                }
                _ => {}
            }
        }

        issue.updated_at = chrono::Utc::now();

        // Write back
        let markdown = issue_to_markdown(&issue)?;
        fs::write(&issue_path, markdown).context("Failed to write issue file")?;

        Ok(issue)
    }

    /// Close an issue
    pub fn close_issue(&self, id: &str, _reason: &str) -> Result<Issue> {
        let _lock = Lock::acquire(&self.beads_dir)?;

        let issue_path = self.issues_dir.join(format!("{}.md", id));
        if !issue_path.exists() {
            anyhow::bail!("Issue not found: {}", id);
        }

        let content = fs::read_to_string(&issue_path).context("Failed to read issue file")?;
        let mut issue = markdown_to_issue(id, &content)?;

        issue.status = Status::Closed;
        issue.closed_at = Some(chrono::Utc::now());
        issue.updated_at = chrono::Utc::now();

        let markdown = issue_to_markdown(&issue)?;
        fs::write(&issue_path, markdown).context("Failed to write issue file")?;

        Ok(issue)
    }

    /// Reopen an issue
    pub fn reopen_issue(&self, id: &str) -> Result<Issue> {
        let _lock = Lock::acquire(&self.beads_dir)?;

        let issue_path = self.issues_dir.join(format!("{}.md", id));
        if !issue_path.exists() {
            anyhow::bail!("Issue not found: {}", id);
        }

        let content = fs::read_to_string(&issue_path).context("Failed to read issue file")?;
        let mut issue = markdown_to_issue(id, &content)?;

        issue.status = Status::Open;
        issue.closed_at = None;
        issue.updated_at = chrono::Utc::now();

        let markdown = issue_to_markdown(&issue)?;
        fs::write(&issue_path, markdown).context("Failed to write issue file")?;

        Ok(issue)
    }

    /// Rename an issue ID
    ///
    /// This operation:
    /// - Renames the markdown file
    /// - Updates the ID in the issue frontmatter
    /// - Updates all references in other issues' dependencies
    /// - Updates all text mentions of the old ID in all issues (title, description, design, notes, acceptance_criteria)
    /// - Is atomic (all updates succeed or none)
    pub fn rename_issue(&self, old_id: &str, new_id: &str, dry_run: bool) -> Result<Vec<String>> {
        let _lock = Lock::acquire(&self.beads_dir)?;

        let old_path = self.issues_dir.join(format!("{}.md", old_id));
        let new_path = self.issues_dir.join(format!("{}.md", new_id));

        // Validate old issue exists
        if !old_path.exists() {
            anyhow::bail!("Issue not found: {}", old_id);
        }

        // Validate new ID doesn't already exist
        if new_path.exists() {
            anyhow::bail!("Target issue ID already exists: {}", new_id);
        }

        // Track all changes for dry-run mode
        let mut changes = Vec::new();
        changes.push(format!("Rename file: {}.md -> {}.md", old_id, new_id));

        // Load the issue to rename
        let content = fs::read_to_string(&old_path).context("Failed to read issue file")?;
        let mut issue = markdown_to_issue(old_id, &content)?;

        // Update the issue's ID
        issue.id = new_id.to_string();
        issue.updated_at = chrono::Utc::now();
        changes.push(format!(
            "Update ID in frontmatter: {} -> {}",
            old_id, new_id
        ));

        // Build ID mapping for text replacement
        let mut id_mapping = HashMap::new();
        id_mapping.insert(old_id.to_string(), new_id.to_string());

        // Apply text replacements to the renamed issue itself
        replace_ids_in_issue_text(&mut issue, &id_mapping);

        // Find all issues that reference the old ID (either in dependencies or text)
        let all_issues = self.list_all_issues_no_dependents()?;
        let mut issues_to_update = Vec::new();

        for other_issue in all_issues {
            if other_issue.id == old_id {
                continue; // Skip the renamed issue itself
            }

            let mut other_issue = other_issue;
            let mut has_changes = false;

            // Check if this issue has explicit dependency on the renamed issue
            if other_issue.depends_on.contains_key(old_id) {
                changes.push(format!(
                    "Update dependency in {}: {} -> {}",
                    other_issue.id, old_id, new_id
                ));
                has_changes = true;
            }

            // Apply text replacements to all text fields
            let old_title = other_issue.title.clone();
            let old_description = other_issue.description.clone();
            let old_design = other_issue.design.clone();
            let old_notes = other_issue.notes.clone();
            let old_acceptance = other_issue.acceptance_criteria.clone();

            replace_ids_in_issue_text(&mut other_issue, &id_mapping);

            // Check if any text fields changed
            if other_issue.title != old_title
                || other_issue.description != old_description
                || other_issue.design != old_design
                || other_issue.notes != old_notes
                || other_issue.acceptance_criteria != old_acceptance
            {
                changes.push(format!(
                    "Update text references in {}: {} -> {}",
                    other_issue.id, old_id, new_id
                ));
                has_changes = true;
            }

            if has_changes {
                issues_to_update.push(other_issue);
            }
        }

        // If dry-run, return changes without applying
        if dry_run {
            return Ok(changes);
        }

        // Apply changes atomically
        // First, write all updated issues
        for mut other_issue in issues_to_update {
            // Update the explicit dependency reference
            if let Some(dep_type) = other_issue.depends_on.remove(old_id) {
                other_issue.depends_on.insert(new_id.to_string(), dep_type);
            }
            other_issue.updated_at = chrono::Utc::now();

            // Write the updated issue
            let other_path = self.issues_dir.join(format!("{}.md", other_issue.id));
            let markdown = issue_to_markdown(&other_issue)?;
            fs::write(&other_path, markdown)
                .context(format!("Failed to update issue: {}", other_issue.id))?;
        }

        // Write the renamed issue with new ID
        let markdown = issue_to_markdown(&issue)?;
        fs::write(&new_path, markdown).context("Failed to write renamed issue")?;

        // Remove the old file
        fs::remove_file(&old_path).context("Failed to remove old issue file")?;

        Ok(changes)
    }

    /// Repair broken references by scanning all issues and fixing stale references
    ///
    /// This scans all issues and removes references to nonexistent issues
    pub fn repair_references(&self, dry_run: bool) -> Result<Vec<String>> {
        let _lock = Lock::acquire(&self.beads_dir)?;

        let mut changes = Vec::new();
        let all_issues = self.list_all_issues_no_dependents()?;

        // Build a set of all valid issue IDs
        let valid_ids: std::collections::HashSet<String> =
            all_issues.iter().map(|i| i.id.clone()).collect();

        // Find issues with broken references
        for issue in all_issues {
            let mut broken_refs = Vec::new();

            for dep_id in issue.depends_on.keys() {
                if !valid_ids.contains(dep_id) {
                    broken_refs.push(dep_id.clone());
                }
            }

            if !broken_refs.is_empty() {
                for broken_ref in &broken_refs {
                    changes.push(format!(
                        "Remove broken reference in {}: {} (does not exist)",
                        issue.id, broken_ref
                    ));
                }

                // If not dry-run, apply the fix
                if !dry_run {
                    let mut updated_issue = issue.clone();
                    for broken_ref in &broken_refs {
                        updated_issue.depends_on.remove(broken_ref);
                    }
                    updated_issue.updated_at = chrono::Utc::now();

                    let issue_path = self.issues_dir.join(format!("{}.md", updated_issue.id));
                    let markdown = issue_to_markdown(&updated_issue)?;
                    fs::write(&issue_path, markdown)
                        .context(format!("Failed to update issue: {}", updated_issue.id))?;
                }
            }
        }

        if changes.is_empty() {
            changes.push("No broken references found".to_string());
        }

        Ok(changes)
    }

    /// Validate that a dependency target exists (warns if not)
    fn validate_dependency_exists(&self, dep_id: &str) -> bool {
        let dep_path = self.issues_dir.join(format!("{}.md", dep_id));
        let exists = dep_path.exists();

        if !exists {
            eprintln!("Warning: Dependency target does not exist: {}", dep_id);
            eprintln!("  This issue will be blocked until {} is created.", dep_id);
        }

        exists
    }

    /// Add a dependency between issues
    pub fn add_dependency(
        &self,
        from_id: &str,
        to_id: &str,
        dep_type: DependencyType,
    ) -> Result<()> {
        let _lock = Lock::acquire(&self.beads_dir)?;

        let issue_path = self.issues_dir.join(format!("{}.md", from_id));
        if !issue_path.exists() {
            anyhow::bail!("Issue not found: {}", from_id);
        }

        // Validate dependency target exists (warn if not)
        self.validate_dependency_exists(to_id);

        let content = fs::read_to_string(&issue_path).context("Failed to read issue file")?;
        let mut issue = markdown_to_issue(from_id, &content)?;

        // Add dependency
        issue.depends_on.insert(to_id.to_string(), dep_type);
        issue.updated_at = chrono::Utc::now();

        let markdown = issue_to_markdown(&issue)?;
        fs::write(&issue_path, markdown).context("Failed to write issue file")?;

        Ok(())
    }

    pub fn remove_dependency(&self, from_id: &str, to_id: &str) -> Result<()> {
        let _lock = Lock::acquire(&self.beads_dir)?;

        let issue_path = self.issues_dir.join(format!("{}.md", from_id));
        if !issue_path.exists() {
            anyhow::bail!("Issue not found: {}", from_id);
        }

        let content = fs::read_to_string(&issue_path).context("Failed to read issue file")?;
        let mut issue = markdown_to_issue(from_id, &content)?;

        // Remove dependency
        if issue.depends_on.remove(to_id).is_none() {
            anyhow::bail!("Dependency not found: {} -> {}", from_id, to_id);
        }
        issue.updated_at = chrono::Utc::now();

        let markdown = issue_to_markdown(&issue)?;
        fs::write(&issue_path, markdown).context("Failed to write issue file")?;

        Ok(())
    }

    /// Get dependency tree starting from a given issue
    pub fn get_dependency_tree(
        &self,
        issue_id: &str,
        max_depth: usize,
        show_all_paths: bool,
    ) -> Result<crate::types::TreeNode> {
        use std::collections::HashSet;

        // Load all issues to build the tree
        let issues = self.list_issues(None, None, None, None, None)?;
        let issues_map: HashMap<String, Issue> = issues
            .into_iter()
            .map(|issue| (issue.id.clone(), issue))
            .collect();

        // Find the root issue
        let root_issue = issues_map
            .get(issue_id)
            .ok_or_else(|| anyhow::anyhow!("Issue not found: {}", issue_id))?;

        // Track visited nodes for cycle detection (if not showing all paths)
        let mut visited = HashSet::new();

        // Build tree recursively
        build_tree_node(
            root_issue,
            &issues_map,
            &mut visited,
            0,
            max_depth,
            show_all_paths,
            None,
        )
    }

    /// Detect dependency cycles in the issue graph
    pub fn detect_dependency_cycles(&self) -> Result<Vec<Vec<String>>> {
        use std::collections::{HashMap, HashSet};

        // Load all issues
        let issues = self.list_issues(None, None, None, None, None)?;
        let issues_map: HashMap<String, Issue> = issues
            .into_iter()
            .map(|issue| (issue.id.clone(), issue))
            .collect();

        let mut cycles = Vec::new();
        let mut visited = HashSet::new();
        let mut rec_stack = HashSet::new();
        let mut path = Vec::new();

        // Try to find cycles starting from each unvisited node
        for issue_id in issues_map.keys() {
            if !visited.contains(issue_id) {
                find_cycles_dfs(
                    issue_id,
                    &issues_map,
                    &mut visited,
                    &mut rec_stack,
                    &mut path,
                    &mut cycles,
                );
            }
        }

        Ok(cycles)
    }
}

/// Recursively build a tree node
fn build_tree_node(
    issue: &Issue,
    issues_map: &HashMap<String, Issue>,
    visited: &mut std::collections::HashSet<String>,
    current_depth: usize,
    max_depth: usize,
    show_all_paths: bool,
    dep_type: Option<String>,
) -> Result<crate::types::TreeNode> {
    let mut node = crate::types::TreeNode {
        id: issue.id.clone(),
        title: issue.title.clone(),
        status: issue.status,
        priority: issue.priority,
        dep_type,
        children: Vec::new(),
        is_cycle: false,
        depth_exceeded: false,
    };

    // Check for cycle
    if !show_all_paths && visited.contains(&issue.id) {
        node.is_cycle = true;
        return Ok(node);
    }

    // Check depth limit
    if current_depth >= max_depth {
        node.depth_exceeded = true;
        return Ok(node);
    }

    // Mark as visited (if not showing all paths)
    if !show_all_paths {
        visited.insert(issue.id.clone());
    }

    // Add children (dependencies)
    for (dep_id, dep_type_val) in &issue.depends_on {
        if let Some(dep_issue) = issues_map.get(dep_id) {
            let child = build_tree_node(
                dep_issue,
                issues_map,
                visited,
                current_depth + 1,
                max_depth,
                show_all_paths,
                Some(dep_type_val.to_string()),
            )?;
            node.children.push(child);
        }
    }

    // Unmark as visited (for backtracking if not showing all paths)
    if !show_all_paths {
        visited.remove(&issue.id);
    }

    Ok(node)
}

/// DFS helper function to find cycles in the dependency graph
fn find_cycles_dfs(
    current_id: &str,
    issues_map: &HashMap<String, Issue>,
    visited: &mut std::collections::HashSet<String>,
    rec_stack: &mut std::collections::HashSet<String>,
    path: &mut Vec<String>,
    cycles: &mut Vec<Vec<String>>,
) {
    // Mark current node as visited and add to recursion stack
    visited.insert(current_id.to_string());
    rec_stack.insert(current_id.to_string());
    path.push(current_id.to_string());

    // Get the current issue
    if let Some(issue) = issues_map.get(current_id) {
        // Check all dependencies
        for dep_id in issue.depends_on.keys() {
            if !visited.contains(dep_id) {
                // Recursively visit unvisited dependencies
                find_cycles_dfs(dep_id, issues_map, visited, rec_stack, path, cycles);
            } else if rec_stack.contains(dep_id) {
                // Found a cycle! Extract the cycle from the path
                if let Some(cycle_start_idx) = path.iter().position(|id| id == dep_id) {
                    let cycle: Vec<String> = path[cycle_start_idx..].to_vec();
                    // Only add if this exact cycle hasn't been found before
                    if !cycles.iter().any(|c| cycles_equal(c, &cycle)) {
                        cycles.push(cycle);
                    }
                }
            }
        }
    }

    // Remove from recursion stack and path
    rec_stack.remove(current_id);
    path.pop();
}

/// Check if two cycles are equal (considering rotation)
fn cycles_equal(cycle1: &[String], cycle2: &[String]) -> bool {
    if cycle1.len() != cycle2.len() {
        return false;
    }

    // Check all rotations
    for i in 0..cycle1.len() {
        let mut match_found = true;
        for j in 0..cycle1.len() {
            if cycle1[(i + j) % cycle1.len()] != cycle2[j] {
                match_found = false;
                break;
            }
        }
        if match_found {
            return true;
        }
    }
    false
}

impl Storage {
    /// Populate dependents field for a vector of issues
    fn populate_dependents(issues: &mut [Issue]) {
        use crate::types::Dependency;
        use std::collections::HashMap;

        // Build a reverse dependency map: issue_id -> [(dependent_id, dep_type), ...]
        let mut reverse_deps: HashMap<String, Vec<Dependency>> = HashMap::new();

        for issue in issues.iter() {
            for (dep_id, dep_type) in &issue.depends_on {
                reverse_deps
                    .entry(dep_id.clone())
                    .or_default()
                    .push(Dependency {
                        id: issue.id.clone(),
                        dep_type: dep_type.to_string(),
                    });
            }
        }

        // Populate dependents for each issue (zero-copy: take ownership from HashMap)
        for issue in issues.iter_mut() {
            issue.dependents = reverse_deps.remove(&issue.id).unwrap_or_default();
        }
    }

    /// List all issues
    pub fn list_issues(
        &self,
        status: Option<Status>,
        priority: Option<Vec<i32>>,
        issue_type: Option<IssueType>,
        assignee: Option<&str>,
        limit: Option<usize>,
    ) -> Result<Vec<Issue>> {
        let _lock = Lock::acquire(&self.beads_dir)?;

        let entries = fs::read_dir(&self.issues_dir).context("Failed to read issues directory")?;

        let mut issues = Vec::new();
        for entry in entries {
            let entry = entry?;
            let name = entry.file_name();
            let name_str = name.to_string_lossy();

            if !name_str.ends_with(".md") {
                continue;
            }

            let issue_id = &name_str[..name_str.len() - 3];
            let content = fs::read_to_string(entry.path())?;
            let issue = markdown_to_issue(issue_id, &content)?;

            // Apply filters
            if let Some(s) = status {
                if issue.status != s {
                    continue;
                }
            }
            if let Some(ref priorities) = priority {
                if !priorities.contains(&issue.priority) {
                    continue;
                }
            }
            if let Some(t) = issue_type {
                if issue.issue_type != t {
                    continue;
                }
            }
            if let Some(a) = assignee {
                if issue.assignee != a {
                    continue;
                }
            }

            issues.push(issue);
        }

        // Sort by creation date (oldest first)
        issues.sort_by_key(|i| i.created_at);

        // Apply limit
        if let Some(limit) = limit {
            issues.truncate(limit);
        }

        // Populate dependents
        Self::populate_dependents(&mut issues);

        Ok(issues)
    }

    /// Get statistics
    pub fn get_stats(&self) -> Result<Stats> {
        let issues = self.list_issues(None, None, None, None, None)?;

        let total = issues.len();
        let open = issues.iter().filter(|i| i.status == Status::Open).count();
        let in_progress = issues
            .iter()
            .filter(|i| i.status == Status::InProgress)
            .count();
        let closed = issues.iter().filter(|i| i.status == Status::Closed).count();

        // Calculate blocked issues (those with blocking dependencies)
        let blocked = issues
            .iter()
            .filter(|i| i.status != Status::Closed && i.has_blocking_dependencies())
            .count();

        // Calculate ready issues
        let ready = issues
            .iter()
            .filter(|i| i.status == Status::Open && !i.has_blocking_dependencies())
            .count();

        // Calculate average lead time for closed issues
        let mut lead_times = Vec::new();
        for issue in &issues {
            if issue.status == Status::Closed {
                if let Some(closed_at) = issue.closed_at {
                    let duration = closed_at.signed_duration_since(issue.created_at);
                    lead_times.push(duration.num_hours() as f64);
                }
            }
        }

        let avg_lead_time_hours = if lead_times.is_empty() {
            0.0
        } else {
            lead_times.iter().sum::<f64>() / lead_times.len() as f64
        };

        Ok(Stats {
            total_issues: total,
            open_issues: open,
            in_progress_issues: in_progress,
            blocked_issues: blocked,
            closed_issues: closed,
            ready_issues: ready,
            average_lead_time_hours: avg_lead_time_hours,
        })
    }

    /// Get blocked issues
    pub fn get_blocked(&self) -> Result<Vec<BlockedIssue>> {
        let issues = self.list_issues(None, None, None, None, None)?;

        let mut blocked = Vec::new();
        for issue in issues {
            if issue.status == Status::Closed {
                continue;
            }

            // Zero-copy: collect blocking dependencies directly without intermediate Vec
            let blocked_by: Vec<String> = issue.get_blocking_dependencies().cloned().collect();

            if !blocked_by.is_empty() {
                let blocked_by_count = blocked_by.len();
                blocked.push(BlockedIssue {
                    issue,
                    blocked_by,
                    blocked_by_count,
                });
            }
        }

        Ok(blocked)
    }

    /// Get ready work
    pub fn get_ready(
        &self,
        assignee: Option<&str>,
        priority: Option<i32>,
        limit: usize,
        sort_policy: &str,
    ) -> Result<Vec<Issue>> {
        // Convert single priority to vector for list_issues
        let priority_list = priority.map(|p| vec![p]);
        let issues = self.list_issues(Some(Status::Open), priority_list, None, assignee, None)?;

        let mut ready: Vec<Issue> = issues
            .into_iter()
            .filter(|i| !i.has_blocking_dependencies())
            .collect();

        // Apply sorting based on policy
        match sort_policy {
            "priority" => {
                // Sort by priority (0 is highest priority, so ascending order)
                ready.sort_by_key(|i| i.priority);
            }
            "oldest" => {
                // Sort by creation date (oldest first)
                ready.sort_by_key(|i| i.created_at);
            }
            "hybrid" => {
                // Hybrid: Sort by priority first, then by creation date (oldest first) for same priority
                ready.sort_by(|a, b| {
                    a.priority
                        .cmp(&b.priority)
                        .then_with(|| a.created_at.cmp(&b.created_at))
                });
            }
            _ => {
                // Default to hybrid if invalid (shouldn't happen due to CLI validation)
                ready.sort_by(|a, b| {
                    a.priority
                        .cmp(&b.priority)
                        .then_with(|| a.created_at.cmp(&b.created_at))
                });
            }
        }

        // Apply limit after sorting
        ready.truncate(limit);

        Ok(ready)
    }

    /// Export issues to JSONL format
    pub fn export_to_jsonl(
        &self,
        output_path: &Path,
        status: Option<Status>,
        priority: Option<i32>,
        issue_type: Option<IssueType>,
        assignee: Option<&str>,
    ) -> Result<usize> {
        use std::io::Write;

        // Convert single priority to vector for list_issues
        let priority_list = priority.map(|p| vec![p]);

        // Get issues with filters (list_issues acquires its own lock)
        let issues = self.list_issues(status, priority_list, issue_type, assignee, None)?;

        // Open output file
        let mut file = fs::File::create(output_path)
            .with_context(|| format!("Failed to create output file: {}", output_path.display()))?;

        // Write each issue as a JSON line
        for issue in &issues {
            let json =
                serde_json::to_string(&issue).context("Failed to serialize issue to JSON")?;
            writeln!(file, "{}", json).context("Failed to write to output file")?;
        }

        Ok(issues.len())
    }

    /// Import issues from JSONL format
    ///
    /// Returns: (imported_count, skipped_count, errors)
    #[allow(dead_code)] // Used by sync command (not yet implemented)
    pub fn import_from_jsonl(
        &self,
        input_path: &Path,
        overwrite: bool,
    ) -> Result<(usize, usize, Vec<String>)> {
        use std::io::{BufRead, BufReader};

        let _lock = Lock::acquire(&self.beads_dir)?;

        // Open input file
        let file = fs::File::open(input_path)
            .with_context(|| format!("Failed to open input file: {}", input_path.display()))?;
        let reader = BufReader::new(file);

        let mut imported = 0;
        let mut skipped = 0;
        let mut errors = Vec::new();

        // Read and parse each line
        for (line_num, line_result) in reader.lines().enumerate() {
            let line = match line_result {
                Ok(l) => l,
                Err(e) => {
                    errors.push(format!("Line {}: Failed to read line: {}", line_num + 1, e));
                    continue;
                }
            };

            // Skip empty lines
            if line.trim().is_empty() {
                continue;
            }

            // Parse JSON
            let issue: Issue = match serde_json::from_str(&line) {
                Ok(i) => i,
                Err(e) => {
                    errors.push(format!(
                        "Line {}: Failed to parse JSON: {}",
                        line_num + 1,
                        e
                    ));
                    continue;
                }
            };

            // Check if markdown file already exists
            let issue_path = self.issues_dir.join(format!("{}.md", issue.id));
            if issue_path.exists() && !overwrite {
                skipped += 1;
                continue;
            }

            // Convert to markdown and write
            match issue_to_markdown(&issue) {
                Ok(markdown) => {
                    if let Err(e) = fs::write(&issue_path, &markdown) {
                        errors.push(format!(
                            "Issue {}: Failed to write markdown file: {}",
                            issue.id, e
                        ));
                        continue;
                    }

                    // Set file mtime to match issue's updated_at timestamp (preserve timestamp)
                    if let Err(e) = set_file_mtime_from_issue(&issue_path, &issue) {
                        // Non-fatal: log warning but don't fail the import
                        eprintln!("Warning: Failed to set mtime for {}: {}", issue.id, e);
                    }

                    imported += 1;
                }
                Err(e) => {
                    errors.push(format!(
                        "Issue {}: Failed to convert to markdown: {}",
                        issue.id, e
                    ));
                }
            }
        }

        Ok((imported, skipped, errors))
    }

    /// Rename the issue prefix for all issues
    ///
    /// This operation:
    /// - Renames all issue files from old-prefix-N to new-prefix-N
    /// - Updates the ID in all issue frontmatter
    /// - Updates all dependency references
    /// - Updates all text mentions of old IDs in all issues (title, description, design, notes, acceptance_criteria)
    /// - Updates the prefix in config.yaml
    /// - Is atomic (all updates succeed or none)
    pub fn rename_prefix(
        &self,
        new_prefix: &str,
        dry_run: bool,
        force: bool,
    ) -> Result<Vec<String>> {
        let _lock = Lock::acquire(&self.beads_dir)?;

        // Get current prefix
        let old_prefix = self.get_prefix()?;

        // Validate new prefix is different
        if old_prefix == new_prefix {
            anyhow::bail!("New prefix '{}' is the same as current prefix", new_prefix);
        }

        // Validate new prefix format (alphanumeric and hyphens only)
        if !new_prefix.chars().all(|c| c.is_alphanumeric() || c == '-') {
            anyhow::bail!(
                "Invalid prefix format: '{}'. Use only alphanumeric characters and hyphens.",
                new_prefix
            );
        }

        // Load all issues
        let all_issues = self.list_all_issues_no_dependents()?;

        // Build mapping from old ID to new ID
        let mut id_mapping = HashMap::new();
        for issue in &all_issues {
            // Check if this issue uses the current prefix
            if let Some(pos) = issue.id.rfind('-') {
                let issue_prefix = &issue.id[..pos];
                let issue_number = &issue.id[pos + 1..];

                if issue_prefix == old_prefix {
                    let new_id = format!("{}-{}", new_prefix, issue_number);

                    // Check if new ID would conflict with existing issue
                    if !force {
                        let new_path = self.issues_dir.join(format!("{}.md", new_id));
                        if new_path.exists() {
                            anyhow::bail!(
                                "Cannot rename: new ID '{}' already exists. Use --force to override.",
                                new_id
                            );
                        }
                    }

                    id_mapping.insert(issue.id.clone(), new_id);
                }
            }
        }

        if id_mapping.is_empty() {
            anyhow::bail!("No issues found with prefix '{}'", old_prefix);
        }

        // Track all changes for dry-run mode
        let mut changes = Vec::new();
        changes.push(format!(
            "Update config.yaml: issue-prefix: {} -> {}",
            old_prefix, new_prefix
        ));

        // Plan all file renames and content updates
        for issue in &all_issues {
            if let Some(new_id) = id_mapping.get(&issue.id) {
                changes.push(format!("Rename file: {}.md -> {}.md", issue.id, new_id));
                changes.push(format!(
                    "Update ID in frontmatter: {} -> {}",
                    issue.id, new_id
                ));

                // Check if this issue has dependencies that will be renamed
                for dep_id in issue.depends_on.keys() {
                    if id_mapping.contains_key(dep_id) {
                        changes.push(format!(
                            "Update dependency in {}: {} -> {}",
                            new_id,
                            dep_id,
                            id_mapping.get(dep_id).unwrap()
                        ));
                    }
                }
            }
        }

        // If dry-run, return changes without applying
        if dry_run {
            return Ok(changes);
        }

        // Apply changes atomically
        // Step 1: Update all issue files (content + dependencies + text replacements)
        for issue in all_issues {
            let mut updated_issue = issue.clone();
            let mut issue_modified = false;

            // Check if this issue's ID needs to be renamed
            if let Some(new_id) = id_mapping.get(&issue.id) {
                updated_issue.id = new_id.clone();
                issue_modified = true;
            }

            // Update dependency references
            let mut new_depends_on = HashMap::new();
            for (dep_id, dep_type) in &updated_issue.depends_on {
                let mapped_dep_id = id_mapping.get(dep_id).unwrap_or(dep_id);
                if mapped_dep_id != dep_id {
                    issue_modified = true;
                }
                new_depends_on.insert(mapped_dep_id.clone(), *dep_type);
            }
            updated_issue.depends_on = new_depends_on;

            // Apply text replacements to all text fields
            let old_title = updated_issue.title.clone();
            let old_description = updated_issue.description.clone();
            let old_design = updated_issue.design.clone();
            let old_notes = updated_issue.notes.clone();
            let old_acceptance = updated_issue.acceptance_criteria.clone();

            replace_ids_in_issue_text(&mut updated_issue, &id_mapping);

            // Check if any text fields changed
            if updated_issue.title != old_title
                || updated_issue.description != old_description
                || updated_issue.design != old_design
                || updated_issue.notes != old_notes
                || updated_issue.acceptance_criteria != old_acceptance
            {
                issue_modified = true;
            }

            // Only write if the issue was modified
            if issue_modified {
                updated_issue.updated_at = chrono::Utc::now();

                // Write to new file (or overwrite if ID didn't change)
                let new_path = self.issues_dir.join(format!("{}.md", updated_issue.id));
                let markdown = issue_to_markdown(&updated_issue)?;
                fs::write(&new_path, markdown).context(format!(
                    "Failed to write renamed issue: {}",
                    updated_issue.id
                ))?;

                // Remove old file if ID changed
                if updated_issue.id != issue.id {
                    let old_path = self.issues_dir.join(format!("{}.md", issue.id));
                    fs::remove_file(&old_path)
                        .context(format!("Failed to remove old issue file: {}", issue.id))?;
                }
            }
        }

        // Step 2: Update config.yaml with new prefix
        let config_path = self.beads_dir.join("config.yaml");
        let mut config = HashMap::new();
        config.insert("issue-prefix".to_string(), new_prefix.to_string());
        let config_yaml = serde_yaml::to_string(&config)?;
        fs::write(&config_path, config_yaml).context("Failed to update config.yaml")?;

        Ok(changes)
    }

    /// Migrate from numeric to hash-based IDs
    ///
    /// This operation:
    /// - Generates hash-based IDs for all issues with numeric IDs
    /// - Renames all issue files from prefix-N to prefix-hash
    /// - Updates all dependency references
    /// - Updates all text mentions of old IDs in all issues (title, description, design, notes, acceptance_criteria)
    /// - Updates config-minibeads.yaml to set mb-hash-ids: true (unless update_config is false)
    /// - Is atomic (all updates succeed or none)
    pub fn migrate_to_hash_ids(
        &self,
        dry_run: bool,
        update_config: bool,
    ) -> Result<(Vec<String>, HashMap<String, String>)> {
        let _lock = Lock::acquire(&self.beads_dir)?;

        // Check if already using hash IDs
        if self.use_hash_ids()? {
            anyhow::bail!("Database is already using hash-based IDs (mb-hash-ids: true in config-minibeads.yaml)");
        }

        // Get current prefix
        let prefix = self.get_prefix()?;

        // Load all issues
        let all_issues = self.list_all_issues_no_dependents()?;

        // Build mapping from old numeric ID to new hash ID
        let mut id_mapping = HashMap::new();
        for issue in &all_issues {
            // Check if this issue has a numeric ID (prefix-N pattern)
            if let Some(pos) = issue.id.rfind('-') {
                let issue_prefix = &issue.id[..pos];
                let issue_suffix = &issue.id[pos + 1..];

                // Only migrate if it's a numeric ID
                if issue_prefix == prefix && issue_suffix.parse::<u32>().is_ok() {
                    // Generate hash-based ID
                    let hash_id =
                        self.generate_hash_id(&prefix, &issue.title, &issue.description)?;

                    // Check if new ID would conflict with existing issue
                    let new_path = self.issues_dir.join(format!("{}.md", hash_id));
                    if new_path.exists() {
                        anyhow::bail!(
                            "Cannot migrate: generated hash ID '{}' already exists. This is a collision - please report this bug.",
                            hash_id
                        );
                    }

                    id_mapping.insert(issue.id.clone(), hash_id);
                }
            }
        }

        if id_mapping.is_empty() {
            anyhow::bail!(
                "No numeric IDs found to migrate. All issues already use hash-based or custom IDs."
            );
        }

        // Track all changes for dry-run mode
        let mut changes = Vec::new();
        if update_config {
            changes.push("Update config-minibeads.yaml: mb-hash-ids: false -> true".to_string());
        }

        // Plan all file renames and content updates
        for issue in &all_issues {
            if let Some(new_id) = id_mapping.get(&issue.id) {
                changes.push(format!("Rename file: {}.md -> {}.md", issue.id, new_id));
                changes.push(format!(
                    "Update ID in frontmatter: {} -> {}",
                    issue.id, new_id
                ));

                // Check if this issue has dependencies that will be renamed
                for dep_id in issue.depends_on.keys() {
                    if id_mapping.contains_key(dep_id) {
                        changes.push(format!(
                            "Update dependency in {}: {} -> {}",
                            new_id,
                            dep_id,
                            id_mapping.get(dep_id).unwrap()
                        ));
                    }
                }
            }
        }

        // If dry-run, return changes without applying (return empty mapping for dry-run)
        if dry_run {
            return Ok((changes, HashMap::new()));
        }

        // Apply changes atomically
        // Step 1: Update all issue files (content + dependencies + text replacements)
        for issue in all_issues {
            let mut updated_issue = issue.clone();
            let mut issue_modified = false;

            // Check if this issue's ID needs to be migrated
            if let Some(new_id) = id_mapping.get(&issue.id) {
                updated_issue.id = new_id.clone();
                issue_modified = true;
            }

            // Update dependency references
            let mut new_depends_on = HashMap::new();
            for (dep_id, dep_type) in &updated_issue.depends_on {
                let mapped_dep_id = id_mapping.get(dep_id).unwrap_or(dep_id);
                if mapped_dep_id != dep_id {
                    issue_modified = true;
                }
                new_depends_on.insert(mapped_dep_id.clone(), *dep_type);
            }
            updated_issue.depends_on = new_depends_on;

            // Apply text replacements to all text fields
            let old_title = updated_issue.title.clone();
            let old_description = updated_issue.description.clone();
            let old_design = updated_issue.design.clone();
            let old_notes = updated_issue.notes.clone();
            let old_acceptance = updated_issue.acceptance_criteria.clone();

            replace_ids_in_issue_text(&mut updated_issue, &id_mapping);

            // Check if any text fields changed
            if updated_issue.title != old_title
                || updated_issue.description != old_description
                || updated_issue.design != old_design
                || updated_issue.notes != old_notes
                || updated_issue.acceptance_criteria != old_acceptance
            {
                issue_modified = true;
            }

            // Only write if the issue was modified
            if issue_modified {
                updated_issue.updated_at = chrono::Utc::now();

                // Write to new file (or overwrite if ID didn't change)
                let new_path = self.issues_dir.join(format!("{}.md", updated_issue.id));
                let markdown = issue_to_markdown(&updated_issue)?;
                fs::write(&new_path, markdown).context(format!(
                    "Failed to write renamed issue: {}",
                    updated_issue.id
                ))?;

                // Remove old file if ID changed
                if updated_issue.id != issue.id {
                    let old_path = self.issues_dir.join(format!("{}.md", issue.id));
                    fs::remove_file(&old_path)
                        .context(format!("Failed to remove old issue file: {}", issue.id))?;
                }
            }
        }

        // Step 2: Update config-minibeads.yaml to set mb-hash-ids: true (if requested)
        if update_config {
            let minibeads_config_path = self.beads_dir.join("config-minibeads.yaml");
            update_yaml_key_value(&minibeads_config_path, "mb-hash-ids", "true")?;
        }

        Ok((changes, id_mapping))
    }

    /// Migrate from hash-based or mixed IDs to pure numeric IDs
    ///
    /// This operation:
    /// - Identifies hash-based IDs (non-numeric chars OR length >= 4)
    /// - Sorts them by created timestamp
    /// - Assigns sequential numbers starting from MAX_ID+1
    /// - Updates all dependency references
    /// - Updates all text mentions of old IDs in all issues (title, description, design, notes, acceptance_criteria)
    /// - Updates config-minibeads.yaml to set mb-hash-ids: false (unless update_config is false)
    /// - Is atomic (all updates succeed or none)
    pub fn migrate_to_numeric_ids(
        &self,
        dry_run: bool,
        update_config: bool,
    ) -> Result<(Vec<String>, HashMap<String, String>)> {
        let _lock = Lock::acquire(&self.beads_dir)?;

        // Get current prefix
        let prefix = self.get_prefix()?;

        // Load all issues
        let all_issues = self.list_all_issues_no_dependents()?;

        // Identify hash-based IDs and collect for migration using gap-based heuristic
        const MAX_GAP: u32 = 100; // Gap size to distinguish numeric vs hash IDs

        let mut hash_issues = Vec::new();
        let mut numeric_ids = Vec::new();
        let mut numeric_id_to_issue: HashMap<u32, Issue> = HashMap::new();

        // First pass: collect all numeric IDs with matching prefix
        for issue in &all_issues {
            if let Some(pos) = issue.id.rfind('-') {
                let issue_prefix = &issue.id[..pos];
                let issue_suffix = &issue.id[pos + 1..];

                if issue_prefix == prefix {
                    // Check if it's numeric
                    if let Ok(num) = issue_suffix.parse::<u32>() {
                        numeric_ids.push(num);
                        numeric_id_to_issue.insert(num, issue.clone());
                    } else {
                        // Non-numeric suffix = hash-based ID
                        hash_issues.push(issue.clone());
                    }
                } else {
                    // Different prefix, check suffix length >= 4
                    if issue_suffix.len() >= 4 {
                        hash_issues.push(issue.clone());
                    }
                }
            }
        }

        // Use gap-based heuristic to find true max numeric ID and identify hash IDs
        let (max_numeric_id, ids_above_gap) = find_max_numeric_id_before_gap(&numeric_ids, MAX_GAP);

        // Add numeric IDs above the gap to hash_issues (these are likely hash IDs with all-numeric hashes)
        if !ids_above_gap.is_empty() {
            eprintln!("Warning: Found {} numeric ID(s) above a gap of {} (likely hash IDs with all-numeric hashes)",
                      ids_above_gap.len(), MAX_GAP);
            eprintln!("         These will be treated as hash IDs and renumbered:");
            for id_num in &ids_above_gap {
                if let Some(issue) = numeric_id_to_issue.get(id_num) {
                    eprintln!("         - {}", issue.id);
                    hash_issues.push(issue.clone());
                }
            }
            eprintln!(
                "         True max numeric ID before gap: {}",
                max_numeric_id
            );
        }

        if hash_issues.is_empty() {
            anyhow::bail!(
                "No hash-based IDs found to migrate. All issues already use numeric IDs."
            );
        }

        // Sort hash issues by created timestamp
        hash_issues.sort_by_key(|issue| issue.created_at);

        // Build mapping from old hash ID to new numeric ID
        let mut id_mapping = HashMap::new();
        let mut next_id = max_numeric_id + 1;

        for issue in &hash_issues {
            let new_id = format!("{}-{}", prefix, next_id);

            // Check if new ID would conflict with existing issue
            let new_path = self.issues_dir.join(format!("{}.md", new_id));
            if new_path.exists() {
                anyhow::bail!(
                    "Cannot migrate: numeric ID '{}' already exists. This should not happen - please report this bug.",
                    new_id
                );
            }

            id_mapping.insert(issue.id.clone(), new_id);
            next_id += 1;
        }

        // Track all changes for dry-run mode
        let mut changes = Vec::new();
        if update_config {
            changes.push("Update config-minibeads.yaml: mb-hash-ids: true -> false".to_string());
        }

        // Plan all file renames and content updates
        for issue in &all_issues {
            if let Some(new_id) = id_mapping.get(&issue.id) {
                changes.push(format!("Rename file: {}.md -> {}.md", issue.id, new_id));
                changes.push(format!(
                    "Update ID in frontmatter: {} -> {}",
                    issue.id, new_id
                ));

                // Check if this issue has dependencies that will be renamed
                for dep_id in issue.depends_on.keys() {
                    if id_mapping.contains_key(dep_id) {
                        changes.push(format!(
                            "Update dependency in {}: {} -> {}",
                            new_id,
                            dep_id,
                            id_mapping.get(dep_id).unwrap()
                        ));
                    }
                }
            }
        }

        // If dry-run, return changes without applying (return empty mapping for dry-run)
        if dry_run {
            return Ok((changes, HashMap::new()));
        }

        // Apply changes atomically
        // Step 1: Update all issue files (content + dependencies + text replacements)
        for issue in all_issues {
            let mut updated_issue = issue.clone();
            let mut issue_modified = false;

            // Check if this issue's ID needs to be migrated
            if let Some(new_id) = id_mapping.get(&issue.id) {
                updated_issue.id = new_id.clone();
                issue_modified = true;
            }

            // Update dependency references
            let mut new_depends_on = HashMap::new();
            for (dep_id, dep_type) in &updated_issue.depends_on {
                let mapped_dep_id = id_mapping.get(dep_id).unwrap_or(dep_id);
                if mapped_dep_id != dep_id {
                    issue_modified = true;
                }
                new_depends_on.insert(mapped_dep_id.clone(), *dep_type);
            }
            updated_issue.depends_on = new_depends_on;

            // Apply text replacements to all text fields
            let old_title = updated_issue.title.clone();
            let old_description = updated_issue.description.clone();
            let old_design = updated_issue.design.clone();
            let old_notes = updated_issue.notes.clone();
            let old_acceptance = updated_issue.acceptance_criteria.clone();

            replace_ids_in_issue_text(&mut updated_issue, &id_mapping);

            // Check if any text fields changed
            if updated_issue.title != old_title
                || updated_issue.description != old_description
                || updated_issue.design != old_design
                || updated_issue.notes != old_notes
                || updated_issue.acceptance_criteria != old_acceptance
            {
                issue_modified = true;
            }

            // Only write if the issue was modified
            if issue_modified {
                updated_issue.updated_at = chrono::Utc::now();

                // Write to new file (or overwrite if ID didn't change)
                let new_path = self.issues_dir.join(format!("{}.md", updated_issue.id));
                let markdown = issue_to_markdown(&updated_issue)?;
                fs::write(&new_path, markdown).context(format!(
                    "Failed to write renamed issue: {}",
                    updated_issue.id
                ))?;

                // Remove old file if ID changed
                if updated_issue.id != issue.id {
                    let old_path = self.issues_dir.join(format!("{}.md", issue.id));
                    fs::remove_file(&old_path)
                        .context(format!("Failed to remove old issue file: {}", issue.id))?;
                }
            }
        }

        // Step 2: Update config-minibeads.yaml to set mb-hash-ids: false (if requested)
        if update_config {
            let minibeads_config_path = self.beads_dir.join("config-minibeads.yaml");
            update_yaml_key_value(&minibeads_config_path, "mb-hash-ids", "false")?;
        }

        Ok((changes, id_mapping))
    }
}

/// Infer prefix from the parent directory name
fn infer_prefix(beads_dir: &Path) -> Option<String> {
    let parent = beads_dir.parent()?.parent()?;
    let name = parent.file_name()?.to_str()?;

    let prefix = name.to_lowercase().replace([' ', '_'], "-");

    Some(prefix)
}

/// Create config-minibeads.yaml with minibeads-specific options
/// This file contains options that are NOT compatible with upstream bd
fn create_minibeads_config(beads_dir: &Path, mb_hash_ids: bool) -> Result<()> {
    use std::io::Write;

    let config_path = beads_dir.join("config-minibeads.yaml");

    // Don't clobber existing config
    if config_path.exists() {
        return Ok(());
    }

    let mut file =
        fs::File::create(&config_path).context("Failed to create config-minibeads.yaml")?;

    // Write header with commented explanation
    writeln!(file, "# Minibeads-specific configuration options")?;
    writeln!(
        file,
        "# This file contains options that are NOT compatible with upstream bd"
    )?;
    writeln!(file)?;

    writeln!(
        file,
        "# Use hash-based issue IDs instead of sequential numbers"
    )?;
    writeln!(
        file,
        "# When true, issues are named like: prefix-a1b2c3 (based on content hash)"
    )?;
    writeln!(
        file,
        "# When false, issues are named like: prefix-1, prefix-2, ... (sequential)"
    )?;
    writeln!(file, "# Default: false")?;
    writeln!(
        file,
        "mb-hash-ids: {}",
        if mb_hash_ids { "true" } else { "false" }
    )?;

    Ok(())
}

/// Ensure .gitignore exists and contains required entries
fn ensure_gitignore(beads_dir: &Path) -> Result<()> {
    use std::io::{BufRead, BufReader, Write};

    let gitignore_path = beads_dir.join(".gitignore");
    let required_entries = ["minibeads.lock", "command_history.log"];

    // Read existing content if file exists
    let mut existing_lines = Vec::new();
    if gitignore_path.exists() {
        let file = fs::File::open(&gitignore_path).context("Failed to read .gitignore")?;
        let reader = BufReader::new(file);
        for line in reader.lines() {
            existing_lines.push(line?);
        }
    }

    // Check which entries are missing
    let mut missing_entries = Vec::new();
    for entry in &required_entries {
        if !existing_lines.iter().any(|line| line.trim() == *entry) {
            missing_entries.push(*entry);
        }
    }

    // Append missing entries if any
    if !missing_entries.is_empty() {
        let mut file = fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&gitignore_path)
            .context("Failed to open .gitignore for writing")?;

        // Add a newline before our entries if file already had content
        if !existing_lines.is_empty() && !existing_lines.last().unwrap().is_empty() {
            writeln!(file)?;
        }

        // Write missing entries
        for entry in missing_entries {
            writeln!(file, "{}", entry)?;
        }
    }

    Ok(())
}

/// Set file mtime to match issue's updated_at timestamp
/// This preserves timestamps when importing from JSONL
#[allow(dead_code)] // Used by import and sync (not yet fully wired up)
fn set_file_mtime_from_issue(file_path: &Path, issue: &Issue) -> Result<()> {
    use filetime::{set_file_mtime, FileTime};
    use std::time::SystemTime;

    // Convert DateTime<Utc> to SystemTime
    let system_time: SystemTime = issue.updated_at.into();

    // Convert to FileTime
    let file_time = FileTime::from_system_time(system_time);

    // Set the file's modification time
    set_file_mtime(file_path, file_time)
        .with_context(|| format!("Failed to set mtime for {}", file_path.display()))?;

    Ok(())
}

/// Get file mtime as DateTime<Utc>
/// This is used for comparison during sync
#[allow(dead_code)] // Used by sync command (not yet implemented)
pub fn get_file_mtime(file_path: &Path) -> Result<chrono::DateTime<chrono::Utc>> {
    use chrono::{DateTime, Utc};
    use std::time::SystemTime;

    let metadata = fs::metadata(file_path)
        .with_context(|| format!("Failed to get metadata for {}", file_path.display()))?;

    let mtime: SystemTime = metadata
        .modified()
        .with_context(|| format!("Failed to get modified time for {}", file_path.display()))?;

    // Convert SystemTime to DateTime<Utc>
    let datetime: DateTime<Utc> = mtime.into();

    Ok(datetime)
}

/// Find the true maximum numeric ID before a large gap
///
/// This function implements a gap-based heuristic to distinguish legitimate numeric IDs
/// from hash IDs that happen to be all-numeric. It counts sequentially from 1 until it
/// finds MAX_GAP (default 100) consecutive missing IDs, then treats everything before
/// that gap as legitimate numeric IDs and everything after as hash IDs.
///
/// Returns: (max_numeric_id, ids_above_gap)
/// - max_numeric_id: The highest numeric ID before the gap (or 0 if none found)
/// - ids_above_gap: Numeric IDs that appear after the gap (likely hash IDs)
fn find_max_numeric_id_before_gap(numeric_ids: &[u32], max_gap: u32) -> (u32, Vec<u32>) {
    if numeric_ids.is_empty() {
        return (0, Vec::new());
    }

    // Sort the numeric IDs
    let mut sorted_ids = numeric_ids.to_vec();
    sorted_ids.sort_unstable();

    // Find the first gap of size >= MAX_GAP
    let mut max_before_gap = sorted_ids[0];
    let mut gap_start = 0;
    let mut found_gap = false;

    for i in 0..sorted_ids.len() - 1 {
        let current = sorted_ids[i];
        let next = sorted_ids[i + 1];
        let gap_size = next - current;

        if gap_size >= max_gap {
            max_before_gap = current;
            gap_start = i + 1;
            found_gap = true;
            break;
        }
    }

    // If no gap found, use the highest ID
    if !found_gap {
        max_before_gap = *sorted_ids.last().unwrap();
        return (max_before_gap, Vec::new());
    }

    // Collect IDs above the gap (these are likely hash IDs with numeric-only hashes)
    let ids_above_gap = sorted_ids[gap_start..].to_vec();

    (max_before_gap, ids_above_gap)
}

/// Update a single key-value pair in a YAML file while preserving comments and formatting
///
/// This function safely updates a YAML config file by:
/// - Reading the file line by line
/// - Finding the line with the specified key (format: "key: value")
/// - Replacing only that line with the new value
/// - Preserving all comments, blank lines, and other formatting
///
/// This approach is more reliable than parsing/serializing YAML for simple config files
/// because it doesn't require a comment-preserving YAML library.
fn update_yaml_key_value(file_path: &Path, key: &str, new_value: &str) -> Result<()> {
    use std::io::{BufRead, BufReader};

    // Read all lines
    let file = fs::File::open(file_path)
        .with_context(|| format!("Failed to open config file: {}", file_path.display()))?;
    let reader = BufReader::new(file);
    let mut lines: Vec<String> = reader
        .lines()
        .collect::<Result<_, _>>()
        .with_context(|| format!("Failed to read config file: {}", file_path.display()))?;

    // Find and update the line with the key
    let key_prefix = format!("{}:", key);
    let mut found = false;

    for line in &mut lines {
        let trimmed = line.trim();
        // Check if this line starts with "key:" (ignoring leading whitespace)
        if trimmed.starts_with(&key_prefix) {
            // Preserve indentation by finding where non-whitespace starts
            let indent = line.len() - line.trim_start().len();
            *line = format!("{}{}: {}", " ".repeat(indent), key, new_value);
            found = true;
            break;
        }
    }

    if !found {
        anyhow::bail!(
            "Key '{}' not found in config file: {}",
            key,
            file_path.display()
        );
    }

    // Write back all lines
    let content = lines.join("\n") + "\n"; // Ensure file ends with newline
    fs::write(file_path, content)
        .with_context(|| format!("Failed to write config file: {}", file_path.display()))?;

    Ok(())
}
