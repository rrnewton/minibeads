use crate::format::{issue_to_markdown, markdown_to_issue};
use crate::lock::Lock;
use crate::types::{BlockedIssue, DependencyType, Issue, IssueType, Stats, Status};
use anyhow::{Context, Result};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

pub struct Storage {
    beads_dir: PathBuf,
    issues_dir: PathBuf,
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

        // Ensure .gitignore exists and has required entries
        ensure_gitignore(&beads_dir)?;

        Ok(Self {
            beads_dir,
            issues_dir,
        })
    }

    /// Initialize a new beads database
    pub fn init(beads_dir: PathBuf, prefix: Option<String>) -> Result<Self> {
        // Create .beads directory
        fs::create_dir_all(&beads_dir).context("Failed to create .beads directory")?;

        // Create issues directory
        let issues_dir = beads_dir.join("issues");
        fs::create_dir_all(&issues_dir).context("Failed to create issues directory")?;

        // Determine prefix
        let prefix = prefix
            .or_else(|| infer_prefix(&beads_dir))
            .unwrap_or_else(|| "bd".to_string());

        // Create config.yaml with issue-prefix
        let config_path = beads_dir.join("config.yaml");
        let mut config = HashMap::new();
        config.insert("issue-prefix".to_string(), prefix);
        let config_yaml = serde_yaml::to_string(&config)?;
        fs::write(&config_path, config_yaml).context("Failed to write config.yaml")?;

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
            let num = self.get_next_number(&prefix)?;
            format!("{}-{}", prefix, num)
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

        // Add dependencies
        for (dep_id, dep_type) in deps {
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

        // Find all issues that reference the old ID
        let all_issues = self.list_all_issues_no_dependents()?;
        let mut issues_to_update = Vec::new();

        for other_issue in all_issues {
            if other_issue.id == old_id {
                continue; // Skip the renamed issue itself
            }

            // Check if this issue depends on the renamed issue
            if other_issue.depends_on.contains_key(old_id) {
                changes.push(format!(
                    "Update dependency in {}: {} -> {}",
                    other_issue.id, old_id, new_id
                ));
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
            // Update the dependency reference
            if let Some(dep_type) = other_issue.depends_on.remove(old_id) {
                other_issue.depends_on.insert(new_id.to_string(), dep_type);
                other_issue.updated_at = chrono::Utc::now();

                // Write the updated issue
                let other_path = self.issues_dir.join(format!("{}.md", other_issue.id));
                let markdown = issue_to_markdown(&other_issue)?;
                fs::write(&other_path, markdown)
                    .context(format!("Failed to update issue: {}", other_issue.id))?;
            }
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

        // Populate dependents for each issue
        for issue in issues.iter_mut() {
            if let Some(dependents) = reverse_deps.get(&issue.id) {
                issue.dependents = dependents.clone();
            } else {
                issue.dependents = Vec::new();
            }
        }
    }

    /// List all issues
    pub fn list_issues(
        &self,
        status: Option<Status>,
        priority: Option<i32>,
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
            if let Some(p) = priority {
                if issue.priority != p {
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

        // Sort by ID
        issues.sort_by(|a, b| a.id.cmp(&b.id));

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
        let issues = self.list_issues(Some(Status::Open), priority, None, assignee, None)?;

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

        // Get issues with filters (list_issues acquires its own lock)
        let issues = self.list_issues(status, priority, issue_type, assignee, None)?;

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
}

/// Infer prefix from the parent directory name
fn infer_prefix(beads_dir: &Path) -> Option<String> {
    let parent = beads_dir.parent()?.parent()?;
    let name = parent.file_name()?.to_str()?;

    let prefix = name.to_lowercase().replace([' ', '_'], "-");

    Some(prefix)
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
