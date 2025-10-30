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
            .filter(|i| i.status != Status::Closed && !i.get_blocking_dependencies().is_empty())
            .count();

        // Calculate ready issues
        let ready = issues
            .iter()
            .filter(|i| i.status == Status::Open && i.get_blocking_dependencies().is_empty())
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

            let blocking_deps = issue.get_blocking_dependencies();
            if !blocking_deps.is_empty() {
                let blocked_by: Vec<String> = blocking_deps.iter().map(|s| (*s).clone()).collect();
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
    ) -> Result<Vec<Issue>> {
        let issues = self.list_issues(Some(Status::Open), priority, None, assignee, None)?;

        let ready: Vec<Issue> = issues
            .into_iter()
            .filter(|i| i.get_blocking_dependencies().is_empty())
            .take(limit)
            .collect();

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
