//! Random beads action generator for property-based testing
//!
//! This module provides a reusable library for generating and executing
//! random sequences of beads commands against minibeads or upstream bd.
//!
//! ## Key Assumption: Sequential Issue Numbering
//!
//! **IMPORTANT**: This generator assumes that beads implementations always number
//! new issues sequentially as `N+1` where `N` is the maximum previous issue ID.
//!
//! This assumption allows us to:
//! - Generate valid action sequences without executing commands
//! - Predict which issue IDs will exist at any point in the sequence
//! - Generate contextually valid actions (e.g., only update issues that exist)
//! - Verify correctness by checking that created issues match predictions
//!
//! ### Verification Strategy
//!
//! When executing action sequences, we VERIFY this assumption by:
//! 1. Each `Create` action includes an `expected_id` field
//! 2. After executing `bd create`, we parse the output to extract the actual issue ID
//! 3. We assert that `actual_id == expected_id`
//! 4. If the assertion fails, the test fails with a clear error message
//!
//! This verification ensures that:
//! - Our model of issue state matches reality
//! - The beads implementation follows sequential numbering
//! - All subsequent actions operate on the correct issues
//!
//! ### Example
//!
//! ```text
//! Action sequence:
//!   1. Init { prefix: "test" }
//!   2. Create { expected_id: "test-1", ...}  →  Verify actual ID is "test-1"
//!   3. Create { expected_id: "test-2", ... } →  Verify actual ID is "test-2"
//!   4. Update { issue_id: "test-1", ... }     →  Valid because test-1 exists
//!   5. Close { issue_id: "test-2", ... }      →  Valid because test-2 exists
//! ```

use anyhow::{Context, Result};
use rand::rngs::StdRng;
use rand::{Rng, SeedableRng};
use std::collections::HashSet;
use std::process::Command;

/// Represents a beads command/action
#[derive(Debug, Clone, PartialEq)]
pub enum BeadsAction {
    /// Initialize beads in directory
    Init { prefix: Option<String> },

    /// Create a new issue with expected ID for verification
    Create {
        expected_id: String,
        title: String,
        priority: i32,
        issue_type: IssueType,
        description: Option<String>,
    },

    /// List issues with optional filters
    List {
        status: Option<Status>,
        priority: Option<i32>,
    },

    /// Show a specific issue
    Show { issue_id: String },

    /// Update an issue
    Update {
        issue_id: String,
        status: Option<Status>,
        priority: Option<i32>,
    },

    /// Close an issue
    Close { issue_id: String, reason: String },

    /// Reopen an issue
    Reopen { issue_id: String },

    /// Add a dependency
    AddDependency {
        issue_id: String,
        depends_on: String,
        dep_type: DependencyType,
    },

    /// Export to JSONL
    Export { output: String },
}

impl std::fmt::Display for BeadsAction {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            BeadsAction::Init { prefix } => {
                if let Some(p) = prefix {
                    write!(f, "init --prefix {}", p)
                } else {
                    write!(f, "init")
                }
            }
            BeadsAction::Create {
                expected_id,
                priority,
                issue_type,
                ..
            } => write!(
                f,
                "create {} (p:{}, type:{})",
                expected_id,
                priority,
                issue_type.as_str()
            ),
            BeadsAction::List { status, priority } => {
                let mut parts = vec!["list".to_string()];
                if let Some(s) = status {
                    parts.push(format!("status:{}", s.as_str()));
                }
                if let Some(p) = priority {
                    parts.push(format!("priority:{}", p));
                }
                write!(f, "{}", parts.join(" "))
            }
            BeadsAction::Show { issue_id } => write!(f, "show {}", issue_id),
            BeadsAction::Update {
                issue_id,
                status,
                priority,
            } => {
                let mut parts = vec![format!("update {}", issue_id)];
                if let Some(s) = status {
                    parts.push(format!("status:{}", s.as_str()));
                }
                if let Some(p) = priority {
                    parts.push(format!("priority:{}", p));
                }
                write!(f, "{}", parts.join(" "))
            }
            BeadsAction::Close { issue_id, .. } => write!(f, "close {}", issue_id),
            BeadsAction::Reopen { issue_id } => write!(f, "reopen {}", issue_id),
            BeadsAction::AddDependency {
                issue_id,
                depends_on,
                dep_type,
            } => write!(
                f,
                "dep add {} → {} ({})",
                issue_id,
                depends_on,
                dep_type.as_str()
            ),
            BeadsAction::Export { output } => write!(f, "export to {}", output),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IssueType {
    Bug,
    Feature,
    Task,
    Epic,
    Chore,
}

impl IssueType {
    fn as_str(&self) -> &'static str {
        match self {
            IssueType::Bug => "bug",
            IssueType::Feature => "feature",
            IssueType::Task => "task",
            IssueType::Epic => "epic",
            IssueType::Chore => "chore",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Status {
    Open,
    InProgress,
    Blocked,
    Closed,
}

impl Status {
    fn as_str(&self) -> &'static str {
        match self {
            Status::Open => "open",
            Status::InProgress => "in_progress",
            Status::Blocked => "blocked",
            Status::Closed => "closed",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DependencyType {
    Blocks,
    Related,
    ParentChild,
}

impl DependencyType {
    fn as_str(&self) -> &'static str {
        match self {
            DependencyType::Blocks => "blocks",
            DependencyType::Related => "related",
            DependencyType::ParentChild => "parent-child",
        }
    }
}

/// Generates random beads action sequences
///
/// Maintains state to ensure generated actions are contextually valid:
/// - Tracks which issues exist (for Update, Show, Close, etc.)
/// - Tracks which issues are closed (for Reopen)
/// - Predicts issue IDs using sequential numbering assumption
pub struct ActionGenerator {
    rng: StdRng,
    prefix: String,
    next_issue_num: usize,
    existing_issues: Vec<String>, // Maintains creation order
    closed_issues: HashSet<String>,
}

impl ActionGenerator {
    /// Create a new generator with the given seed
    pub fn new(seed: u64) -> Self {
        Self {
            rng: StdRng::seed_from_u64(seed),
            prefix: "test".to_string(),
            next_issue_num: 1,
            existing_issues: Vec::new(),
            closed_issues: HashSet::new(),
        }
    }

    /// Generate a sequence of random actions
    ///
    /// Always starts with Init, then generates num_actions random valid actions
    pub fn generate_sequence(&mut self, num_actions: usize) -> Vec<BeadsAction> {
        let mut actions = Vec::new();

        // Always start with init
        actions.push(BeadsAction::Init {
            prefix: Some(self.prefix.clone()),
        });

        // Generate random actions
        for _ in 0..num_actions {
            actions.push(self.generate_action());
        }

        actions
    }

    /// Generate a single random action based on current state
    fn generate_action(&mut self) -> BeadsAction {
        // Weight actions based on what makes sense
        let action_type = if self.existing_issues.is_empty() {
            // If no issues exist, must create one
            0
        } else {
            // Otherwise, pick randomly
            // Bias towards creating issues (30% chance) to build up state
            let rand_val = self.rng.gen_range(0..100);
            if rand_val < 30 {
                0 // Create
            } else if rand_val < 45 {
                1 // List
            } else if rand_val < 55 {
                2 // Show
            } else if rand_val < 70 {
                3 // Update
            } else if rand_val < 80 {
                4 // Close
            } else if rand_val < 85 {
                5 // Reopen
            } else if rand_val < 95 {
                6 // AddDependency
            } else {
                7 // Export
            }
        };

        match action_type {
            0 => self.generate_create(),
            1 => self.generate_list(),
            2 => self.generate_show(),
            3 => self.generate_update(),
            4 => self.generate_close(),
            5 => self.generate_reopen(),
            6 => self.generate_add_dependency(),
            7 => self.generate_export(),
            _ => unreachable!(),
        }
    }

    fn generate_create(&mut self) -> BeadsAction {
        let expected_id = format!("{}-{}", self.prefix, self.next_issue_num);
        self.next_issue_num += 1;
        self.existing_issues.push(expected_id.clone());

        let title = format!("Issue {}", self.rng.gen_range(1000..9999));
        let priority = self.rng.gen_range(0..5);
        let issue_type = match self.rng.gen_range(0..5) {
            0 => IssueType::Bug,
            1 => IssueType::Feature,
            2 => IssueType::Task,
            3 => IssueType::Epic,
            _ => IssueType::Chore,
        };

        let description = if self.rng.gen_bool(0.5) {
            Some(format!("Description for {}", title))
        } else {
            None
        };

        BeadsAction::Create {
            expected_id,
            title,
            priority,
            issue_type,
            description,
        }
    }

    fn generate_list(&mut self) -> BeadsAction {
        let status = if self.rng.gen_bool(0.3) {
            Some(match self.rng.gen_range(0..4) {
                0 => Status::Open,
                1 => Status::InProgress,
                2 => Status::Blocked,
                _ => Status::Closed,
            })
        } else {
            None
        };

        let priority = if self.rng.gen_bool(0.3) {
            Some(self.rng.gen_range(0..5))
        } else {
            None
        };

        BeadsAction::List { status, priority }
    }

    fn generate_show(&mut self) -> BeadsAction {
        let issue_id = self.pick_random_issue();
        BeadsAction::Show { issue_id }
    }

    fn generate_update(&mut self) -> BeadsAction {
        let issue_id = self.pick_random_issue();

        let status = if self.rng.gen_bool(0.5) {
            Some(match self.rng.gen_range(0..3) {
                0 => Status::Open,
                1 => Status::InProgress,
                _ => Status::Blocked,
            })
        } else {
            None
        };

        let priority = if self.rng.gen_bool(0.5) {
            Some(self.rng.gen_range(0..5))
        } else {
            None
        };

        BeadsAction::Update {
            issue_id,
            status,
            priority,
        }
    }

    fn generate_close(&mut self) -> BeadsAction {
        let issue_id = self.pick_random_issue();
        self.closed_issues.insert(issue_id.clone());

        BeadsAction::Close {
            issue_id,
            reason: "Completed".to_string(),
        }
    }

    fn generate_reopen(&mut self) -> BeadsAction {
        // Try to reopen a closed issue, or pick any issue
        let issue_id = if !self.closed_issues.is_empty() && self.rng.gen_bool(0.7) {
            let idx = self.rng.gen_range(0..self.closed_issues.len());
            let issue = self.closed_issues.iter().nth(idx).unwrap().clone();
            self.closed_issues.remove(&issue);
            issue
        } else {
            self.pick_random_issue()
        };

        BeadsAction::Reopen { issue_id }
    }

    fn generate_add_dependency(&mut self) -> BeadsAction {
        // Need at least 2 issues for a dependency
        if self.existing_issues.len() < 2 {
            return self.generate_create();
        }

        let issue_id =
            self.existing_issues[self.rng.gen_range(0..self.existing_issues.len())].clone();

        // Pick a different issue to depend on
        let mut depends_on =
            self.existing_issues[self.rng.gen_range(0..self.existing_issues.len())].clone();
        while depends_on == issue_id && self.existing_issues.len() > 1 {
            depends_on =
                self.existing_issues[self.rng.gen_range(0..self.existing_issues.len())].clone();
        }

        let dep_type = match self.rng.gen_range(0..3) {
            0 => DependencyType::Blocks,
            1 => DependencyType::Related,
            _ => DependencyType::ParentChild,
        };

        BeadsAction::AddDependency {
            issue_id,
            depends_on,
            dep_type,
        }
    }

    fn generate_export(&mut self) -> BeadsAction {
        BeadsAction::Export {
            output: "issues.jsonl".to_string(),
        }
    }

    fn pick_random_issue(&mut self) -> String {
        if self.existing_issues.is_empty() {
            // Shouldn't happen, but handle it
            format!("{}-1", self.prefix)
        } else {
            let idx = self.rng.gen_range(0..self.existing_issues.len());
            self.existing_issues[idx].clone()
        }
    }
}

/// Executes beads actions against a specific implementation
pub struct ActionExecutor {
    binary_path: String,
    work_dir: String,
    use_no_db: bool,
}

impl ActionExecutor {
    /// Create a new executor for the given binary
    ///
    /// `use_no_db`: If true, prepends --no-db to all commands (for upstream bd)
    pub fn new(binary_path: &str, work_dir: &str, use_no_db: bool) -> Self {
        Self {
            binary_path: binary_path.to_string(),
            work_dir: work_dir.to_string(),
            use_no_db,
        }
    }

    /// Build a command with the binary path, working directory, and --no-db flag if needed
    fn build_command(&self) -> Command {
        let mut cmd = Command::new(&self.binary_path);
        cmd.current_dir(&self.work_dir);
        if self.use_no_db {
            cmd.arg("--no-db");
        }
        cmd
    }

    /// Execute a single action
    ///
    /// For Create actions, verifies that the created issue ID matches expected_id
    pub fn execute(&self, action: &BeadsAction) -> Result<ExecutionResult> {
        let output = match action {
            BeadsAction::Init { prefix } => {
                let mut cmd = self.build_command();
                cmd.arg("init");
                if let Some(p) = prefix {
                    cmd.arg("--prefix").arg(p);
                }
                cmd.output().context("Failed to execute init command")?
            }

            BeadsAction::Create {
                expected_id,
                title,
                priority,
                issue_type,
                description,
            } => {
                let mut cmd = self.build_command();
                cmd.arg("create")
                    .arg(title)
                    .arg("-p")
                    .arg(priority.to_string())
                    .arg("-t")
                    .arg(issue_type.as_str());

                if let Some(desc) = description {
                    cmd.arg("-d").arg(desc);
                }

                let output = cmd.output().context("Failed to execute create command")?;

                // Verify the created issue ID matches our expectation
                if output.status.success() {
                    let stdout = String::from_utf8_lossy(&output.stdout);
                    if let Some(actual_id) = extract_issue_id(&stdout) {
                        if actual_id != *expected_id {
                            anyhow::bail!(
                                "SEQUENTIAL NUMBERING ASSUMPTION VIOLATED!\n\
                                 Expected created issue ID: {}\n\
                                 Actual created issue ID: {}\n\
                                 This means the beads implementation does not follow sequential numbering.",
                                expected_id,
                                actual_id
                            );
                        }
                    }
                }

                output
            }

            BeadsAction::List { status, priority } => {
                let mut cmd = self.build_command();
                cmd.arg("list");

                if let Some(s) = status {
                    cmd.arg("--status").arg(s.as_str());
                }
                if let Some(p) = priority {
                    cmd.arg("--priority").arg(p.to_string());
                }

                cmd.output().context("Failed to execute list command")?
            }

            BeadsAction::Show { issue_id } => {
                let mut cmd = self.build_command();
                cmd.arg("show").arg(issue_id);
                cmd.output().context("Failed to execute show command")?
            }

            BeadsAction::Update {
                issue_id,
                status,
                priority,
            } => {
                let mut cmd = self.build_command();
                cmd.arg("update").arg(issue_id);

                if let Some(s) = status {
                    cmd.arg("--status").arg(s.as_str());
                }
                if let Some(p) = priority {
                    cmd.arg("--priority").arg(p.to_string());
                }

                cmd.output().context("Failed to execute update command")?
            }

            BeadsAction::Close { issue_id, reason } => {
                let mut cmd = self.build_command();
                cmd.arg("close").arg(issue_id).arg("--reason").arg(reason);
                cmd.output().context("Failed to execute close command")?
            }

            BeadsAction::Reopen { issue_id } => {
                let mut cmd = self.build_command();
                cmd.arg("reopen").arg(issue_id);
                cmd.output().context("Failed to execute reopen command")?
            }

            BeadsAction::AddDependency {
                issue_id,
                depends_on,
                dep_type,
            } => {
                let mut cmd = self.build_command();
                cmd.arg("dep")
                    .arg("add")
                    .arg(issue_id)
                    .arg(depends_on)
                    .arg("-t")
                    .arg(dep_type.as_str());
                cmd.output().context("Failed to execute dep add command")?
            }

            BeadsAction::Export { output } => {
                let mut cmd = self.build_command();
                cmd.arg("export").arg("--output").arg(output);
                cmd.output().context("Failed to execute export command")?
            }
        };

        Ok(ExecutionResult {
            success: output.status.success(),
            stdout: String::from_utf8_lossy(&output.stdout).to_string(),
            stderr: String::from_utf8_lossy(&output.stderr).to_string(),
            exit_code: output.status.code(),
        })
    }

    /// Execute a sequence of actions
    #[allow(dead_code)]
    pub fn execute_sequence(&self, actions: &[BeadsAction]) -> Result<Vec<ExecutionResult>> {
        let mut results = Vec::new();
        for action in actions {
            let result = self.execute(action)?;
            results.push(result);
        }
        Ok(results)
    }
}

/// Extract issue ID from create command output
///
/// Looks for patterns like:
/// - "Created issue: test-1"
/// - "Created: test-1"
/// - "test-1"
fn extract_issue_id(output: &str) -> Option<String> {
    // Try to find issue ID in various formats
    for line in output.lines() {
        // Look for "Created issue: <id>"
        if let Some(pos) = line.find("Created issue:") {
            let id = line[pos + 14..].trim();
            if !id.is_empty() {
                return Some(id.to_string());
            }
        }

        // Look for "Created: <id>"
        if let Some(pos) = line.find("Created:") {
            let id = line[pos + 8..].trim();
            if !id.is_empty() {
                return Some(id.to_string());
            }
        }

        // Look for issue ID pattern (prefix-number)
        let words: Vec<&str> = line.split_whitespace().collect();
        for word in words {
            if word.contains('-')
                && word
                    .split('-')
                    .next_back()
                    .map(|s| s.parse::<usize>().is_ok())
                    .unwrap_or(false)
            {
                return Some(word.to_string());
            }
        }
    }

    None
}

/// Result of executing a beads action
#[derive(Debug, Clone)]
pub struct ExecutionResult {
    pub success: bool,
    pub stdout: String,
    pub stderr: String,
    pub exit_code: Option<i32>,
}
