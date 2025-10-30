mod format;
mod lock;
mod storage;
mod types;

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use std::collections::HashMap;
use std::env;
use std::path::{Path, PathBuf};
use storage::Storage;
use types::{DependencyType, IssueType, Status};

#[derive(Parser)]
#[command(name = "bd", about = "Minibeads - A minimal issue tracker", version)]
struct Cli {
    /// Path to database (supports BEADS_DB env var)
    #[arg(long, global = true)]
    db: Option<PathBuf>,

    /// Actor name for audit trail
    #[arg(long, global = true)]
    actor: Option<String>,

    /// Output JSON format
    #[arg(long, global = true)]
    json: bool,

    /// Validation mode for parsing issues
    #[arg(long, global = true, default_value = "error", value_name = "MODE")]
    validation: ValidationMode,

    /// Disable command logging to .beads/command_history.log
    #[arg(long, global = true)]
    no_cmd_logging: bool,

    /// Disable auto-flush (ignored for compatibility)
    #[arg(long, global = true)]
    no_auto_flush: bool,

    /// Disable auto-import (ignored for compatibility)
    #[arg(long, global = true)]
    no_auto_import: bool,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ValidationMode {
    Silent,
    Warn,
    Error,
}

impl std::str::FromStr for ValidationMode {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "silent" => Ok(ValidationMode::Silent),
            "warn" => Ok(ValidationMode::Warn),
            "error" => Ok(ValidationMode::Error),
            _ => Err(anyhow::anyhow!(
                "Invalid validation mode: '{}'. Valid values are: silent, warn, error",
                s
            )),
        }
    }
}

#[derive(Subcommand)]
enum Commands {
    /// Initialize beads in current directory
    Init {
        /// Issue prefix (e.g., 'myproject' for myproject-1, myproject-2)
        #[arg(long)]
        prefix: Option<String>,
    },

    /// Create a new issue
    Create {
        /// Issue title
        title: String,

        /// Priority (0-4, 0=highest)
        #[arg(short, long, default_value = "2")]
        priority: i32,

        /// Issue type
        #[arg(short = 't', long, default_value = "task")]
        issue_type: IssueType,

        /// Description
        #[arg(short, long, default_value = "")]
        description: String,

        /// Design notes
        #[arg(long)]
        design: Option<String>,

        /// Acceptance criteria
        #[arg(long)]
        acceptance: Option<String>,

        /// Assignee
        #[arg(long)]
        assignee: Option<String>,

        /// Labels (can be specified multiple times)
        #[arg(short, long)]
        label: Vec<String>,

        /// External reference
        #[arg(long)]
        external_ref: Option<String>,

        /// Explicit issue ID
        #[arg(long)]
        id: Option<String>,

        /// Dependencies (comma-separated, e.g., "bd-1,bd-2")
        #[arg(long)]
        deps: Option<String>,
    },

    /// List issues
    List {
        /// Filter by status
        #[arg(long)]
        status: Option<Status>,

        /// Filter by priority
        #[arg(long)]
        priority: Option<i32>,

        /// Filter by type
        #[arg(long)]
        r#type: Option<IssueType>,

        /// Filter by assignee
        #[arg(long)]
        assignee: Option<String>,

        /// Maximum number of issues to return
        #[arg(long, default_value = "50")]
        limit: usize,
    },

    /// Show issue details
    Show {
        /// Issue ID
        issue_id: String,
    },

    /// Update an issue
    Update {
        /// Issue ID
        issue_id: String,

        /// New status
        #[arg(long)]
        status: Option<Status>,

        /// New priority
        #[arg(long)]
        priority: Option<i32>,

        /// New assignee
        #[arg(long)]
        assignee: Option<String>,

        /// New title
        #[arg(long)]
        title: Option<String>,

        /// New description
        #[arg(long)]
        description: Option<String>,

        /// New design notes
        #[arg(long)]
        design: Option<String>,

        /// New acceptance criteria
        #[arg(long)]
        acceptance: Option<String>,

        /// Additional notes
        #[arg(long)]
        notes: Option<String>,

        /// New external reference
        #[arg(long)]
        external_ref: Option<String>,
    },

    /// Close an issue
    Close {
        /// Issue ID
        issue_id: String,

        /// Reason for closing
        #[arg(long, default_value = "Completed")]
        reason: String,
    },

    /// Reopen closed issues
    Reopen {
        /// Issue IDs to reopen
        issue_ids: Vec<String>,

        /// Reason for reopening
        #[arg(long)]
        reason: Option<String>,
    },

    /// Manage dependencies
    Dep {
        #[command(subcommand)]
        command: DepCommands,
    },

    /// Get statistics
    Stats,

    /// Get blocked issues
    Blocked,

    /// Find ready work (issues with no blockers)
    Ready {
        /// Filter by assignee
        #[arg(long)]
        assignee: Option<String>,

        /// Filter by priority
        #[arg(long)]
        priority: Option<i32>,

        /// Maximum number of issues to return
        #[arg(long, default_value = "10")]
        limit: usize,
    },

    /// Show quickstart guide
    Quickstart,

    /// Show version information
    Version,
}

#[derive(Subcommand)]
enum DepCommands {
    /// Add a dependency
    Add {
        /// Issue that has the dependency
        issue_id: String,

        /// Issue that issue_id depends on
        depends_on_id: String,

        /// Dependency type
        #[arg(long, default_value = "blocks")]
        r#type: DependencyType,
    },
}

fn main() {
    if let Err(e) = run() {
        eprintln!("Error: {:#}", e);
        std::process::exit(1);
    }
}

fn run() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Commands::Init { prefix } => {
            let beads_dir = PathBuf::from(".beads");
            let storage = Storage::init(beads_dir, prefix)?;

            // Log command after successful init
            if !cli.no_cmd_logging {
                let _ = log_command(&storage.get_beads_dir(), &env::args().collect::<Vec<_>>());
            }

            if !cli.json {
                println!(
                    "Initialized beads database with prefix: {}",
                    storage.get_prefix()?
                );
            }
            Ok(())
        }

        Commands::Create {
            title,
            priority,
            issue_type,
            description,
            design,
            acceptance,
            assignee,
            label,
            external_ref,
            id,
            deps,
        } => {
            let storage = get_storage(&cli.db)?;

            // Log command after storage is validated
            if !cli.no_cmd_logging {
                let _ = log_command(&storage.get_beads_dir(), &env::args().collect::<Vec<_>>());
            }

            // Parse dependencies
            let parsed_deps = if let Some(deps_str) = deps {
                deps_str
                    .split(',')
                    .map(|s| (s.trim().to_string(), DependencyType::Blocks))
                    .collect()
            } else {
                Vec::new()
            };

            let issue = storage.create_issue(
                title,
                description,
                design,
                acceptance,
                priority,
                issue_type,
                assignee,
                label,
                external_ref,
                id,
                parsed_deps,
            )?;

            if cli.json {
                println!("{}", serde_json::to_string_pretty(&issue)?);
            } else {
                println!("Created issue: {}", issue.id);
            }
            Ok(())
        }

        Commands::List {
            status,
            priority,
            r#type,
            assignee,
            limit,
        } => {
            let storage = get_storage(&cli.db)?;

            // Log command after storage is validated
            if !cli.no_cmd_logging {
                let _ = log_command(&storage.get_beads_dir(), &env::args().collect::<Vec<_>>());
            }

            let issues =
                storage.list_issues(status, priority, r#type, assignee.as_deref(), Some(limit))?;

            if cli.json {
                println!("{}", serde_json::to_string_pretty(&issues)?);
            } else {
                for issue in issues {
                    println!(
                        "{}: {} [{}] (priority: {})",
                        issue.id, issue.title, issue.status, issue.priority
                    );
                }
            }
            Ok(())
        }

        Commands::Show { issue_id } => {
            let storage = get_storage(&cli.db)?;

            // Log command after storage is validated
            if !cli.no_cmd_logging {
                let _ = log_command(&storage.get_beads_dir(), &env::args().collect::<Vec<_>>());
            }

            let issue = storage
                .get_issue(&issue_id)?
                .ok_or_else(|| anyhow::anyhow!("Issue not found: {}", issue_id))?;

            if cli.json {
                println!("{}", serde_json::to_string_pretty(&[issue])?);
            } else {
                println!("ID: {}", issue.id);
                println!("Title: {}", issue.title);
                println!("Status: {}", issue.status);
                println!("Priority: {}", issue.priority);
                println!("Type: {}", issue.issue_type);
                if !issue.assignee.is_empty() {
                    println!("Assignee: {}", issue.assignee);
                }
                if !issue.description.is_empty() {
                    println!("\nDescription:\n{}", issue.description);
                }
                if !issue.depends_on.is_empty() {
                    println!("\nDependencies:");
                    for (dep_id, dep_type) in &issue.depends_on {
                        println!("  {} ({})", dep_id, dep_type);
                    }
                }
            }
            Ok(())
        }

        Commands::Update {
            issue_id,
            status,
            priority,
            assignee,
            title,
            description,
            design,
            acceptance,
            notes,
            external_ref,
        } => {
            let storage = get_storage(&cli.db)?;

            // Log command after storage is validated
            if !cli.no_cmd_logging {
                let _ = log_command(&storage.get_beads_dir(), &env::args().collect::<Vec<_>>());
            }

            let mut updates = HashMap::new();
            if let Some(s) = status {
                updates.insert("status".to_string(), s.to_string());
            }
            if let Some(p) = priority {
                updates.insert("priority".to_string(), p.to_string());
            }
            if let Some(a) = assignee {
                updates.insert("assignee".to_string(), a);
            }
            if let Some(t) = title {
                updates.insert("title".to_string(), t);
            }
            if let Some(d) = description {
                updates.insert("description".to_string(), d);
            }
            if let Some(d) = design {
                updates.insert("design".to_string(), d);
            }
            if let Some(a) = acceptance {
                updates.insert("acceptance_criteria".to_string(), a);
            }
            if let Some(n) = notes {
                updates.insert("notes".to_string(), n);
            }
            if let Some(e) = external_ref {
                updates.insert("external_ref".to_string(), e);
            }

            let issue = storage.update_issue(&issue_id, updates)?;

            if cli.json {
                println!("{}", serde_json::to_string_pretty(&[issue])?);
            } else {
                println!("Updated issue: {}", issue_id);
            }
            Ok(())
        }

        Commands::Close { issue_id, reason } => {
            let storage = get_storage(&cli.db)?;

            // Log command after storage is validated
            if !cli.no_cmd_logging {
                let _ = log_command(&storage.get_beads_dir(), &env::args().collect::<Vec<_>>());
            }

            let issue = storage.close_issue(&issue_id, &reason)?;

            if cli.json {
                println!("{}", serde_json::to_string_pretty(&[issue])?);
            } else {
                println!("Closed issue: {}", issue_id);
            }
            Ok(())
        }

        Commands::Reopen {
            issue_ids,
            reason: _,
        } => {
            let storage = get_storage(&cli.db)?;

            // Log command after storage is validated
            if !cli.no_cmd_logging {
                let _ = log_command(&storage.get_beads_dir(), &env::args().collect::<Vec<_>>());
            }

            let mut reopened = Vec::new();

            for issue_id in issue_ids {
                let issue = storage.reopen_issue(&issue_id)?;
                reopened.push(issue);
            }

            if cli.json {
                println!("{}", serde_json::to_string_pretty(&reopened)?);
            } else {
                for issue in reopened {
                    println!("Reopened issue: {}", issue.id);
                }
            }
            Ok(())
        }

        Commands::Dep {
            command:
                DepCommands::Add {
                    issue_id,
                    depends_on_id,
                    r#type,
                },
        } => {
            let storage = get_storage(&cli.db)?;

            // Log command after storage is validated
            if !cli.no_cmd_logging {
                let _ = log_command(&storage.get_beads_dir(), &env::args().collect::<Vec<_>>());
            }

            storage.add_dependency(&issue_id, &depends_on_id, r#type)?;

            if !cli.json {
                println!(
                    "Added dependency: {} depends on {} ({})",
                    issue_id, depends_on_id, r#type
                );
            }
            Ok(())
        }

        Commands::Stats => {
            let storage = get_storage(&cli.db)?;

            // Log command after storage is validated
            if !cli.no_cmd_logging {
                let _ = log_command(&storage.get_beads_dir(), &env::args().collect::<Vec<_>>());
            }

            let stats = storage.get_stats()?;

            if cli.json {
                println!("{}", serde_json::to_string_pretty(&stats)?);
            } else {
                println!("Total issues: {}", stats.total_issues);
                println!("Open: {}", stats.open_issues);
                println!("In Progress: {}", stats.in_progress_issues);
                println!("Blocked: {}", stats.blocked_issues);
                println!("Closed: {}", stats.closed_issues);
                println!("Ready: {}", stats.ready_issues);
                println!(
                    "Average lead time: {:.1} hours",
                    stats.average_lead_time_hours
                );
            }
            Ok(())
        }

        Commands::Blocked => {
            let storage = get_storage(&cli.db)?;

            // Log command after storage is validated
            if !cli.no_cmd_logging {
                let _ = log_command(&storage.get_beads_dir(), &env::args().collect::<Vec<_>>());
            }

            let blocked = storage.get_blocked()?;

            if cli.json {
                println!("{}", serde_json::to_string_pretty(&blocked)?);
            } else {
                for item in blocked {
                    println!(
                        "{}: {} - blocked by: {}",
                        item.issue.id,
                        item.issue.title,
                        item.blocked_by.join(", ")
                    );
                }
            }
            Ok(())
        }

        Commands::Ready {
            assignee,
            priority,
            limit,
        } => {
            let storage = get_storage(&cli.db)?;

            // Log command after storage is validated
            if !cli.no_cmd_logging {
                let _ = log_command(&storage.get_beads_dir(), &env::args().collect::<Vec<_>>());
            }

            let ready = storage.get_ready(assignee.as_deref(), priority, limit)?;

            if cli.json {
                println!("{}", serde_json::to_string_pretty(&ready)?);
            } else {
                for issue in ready {
                    println!(
                        "{}: {} [priority: {}]",
                        issue.id, issue.title, issue.priority
                    );
                }
            }
            Ok(())
        }

        Commands::Quickstart => {
            print_quickstart();
            Ok(())
        }

        Commands::Version => {
            println!("bd version 0.9.0");
            Ok(())
        }
    }
}

fn get_storage(db_arg: &Option<PathBuf>) -> Result<Storage> {
    let beads_dir = if let Some(db) = db_arg {
        // If --db points to a .db file, use its parent directory
        if db.extension().is_some_and(|e| e == "db") {
            db.parent()
                .ok_or_else(|| anyhow::anyhow!("Invalid database path"))?
                .to_path_buf()
        } else {
            db.clone()
        }
    } else if let Ok(beads_db) = env::var("BEADS_DB") {
        let db_path = PathBuf::from(beads_db);
        if db_path.extension().is_some_and(|e| e == "db") {
            db_path
                .parent()
                .ok_or_else(|| anyhow::anyhow!("Invalid BEADS_DB path"))?
                .to_path_buf()
        } else {
            db_path
        }
    } else if let Ok(beads_dir) = env::var("BEADS_DIR") {
        PathBuf::from(beads_dir)
    } else {
        // Search for .beads directory
        find_beads_dir()?
    };

    Storage::open(beads_dir).context("Failed to open storage")
}

fn find_beads_dir() -> Result<PathBuf> {
    let mut current = env::current_dir()?;

    loop {
        let beads_dir = current.join(".beads");
        if beads_dir.exists() && beads_dir.is_dir() {
            return Ok(beads_dir);
        }

        if !current.pop() {
            anyhow::bail!("No .beads directory found. Run 'bd init' to initialize a new database.");
        }
    }
}

/// Log command to command_history.log
fn log_command(beads_dir: &Path, args: &[String]) -> Result<()> {
    use std::fs::OpenOptions;
    use std::io::Write;

    let log_path = beads_dir.join("command_history.log");
    let timestamp = chrono::Utc::now().to_rfc3339();

    // Skip the first argument (binary path) and only log the CLI options
    let command_line = if args.len() > 1 {
        args[1..].join(" ")
    } else {
        String::new()
    };

    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(&log_path)
        .context("Failed to open command history log")?;

    writeln!(file, "{} {}", timestamp, command_line)
        .context("Failed to write to command history log")?;

    Ok(())
}

fn print_quickstart() {
    println!(
        r#"bd - Dependency-Aware Issue Tracker

Issues chained together like beads.

GETTING STARTED
  bd init   Initialize bd in your project
            Creates .beads/ directory with project-specific database
            Auto-detects prefix from directory name (e.g., myapp-1, myapp-2)

  bd init --prefix api   Initialize with custom prefix
            Issues will be named: api-1, api-2, ...

CREATING ISSUES
  bd create "Fix login bug"
  bd create "Add auth" -p 0 -t feature
  bd create "Write tests" -d "Unit tests for auth" --assignee alice

VIEWING ISSUES
  bd list       List all issues
  bd list --status open  List by status
  bd list --priority 0  List by priority (0-4, 0=highest)
  bd show bd-1       Show issue details

MANAGING DEPENDENCIES
  bd dep add bd-1 bd-2     Add dependency (bd-2 blocks bd-1)

DEPENDENCY TYPES
  blocks  Task B must complete before task A
  related  Soft connection, doesn't block progress
  parent-child  Epic/subtask hierarchical relationship
  discovered-from  Auto-created when AI discovers related work

READY WORK
  bd ready       Show issues ready to work on
            Ready = status is 'open' AND no blocking dependencies
            Perfect for agents to claim next work!

UPDATING ISSUES
  bd update bd-1 --status in_progress
  bd update bd-1 --priority 0
  bd update bd-1 --assignee bob

CLOSING ISSUES
  bd close bd-1
  bd close bd-1 --reason "Fixed in PR #42"

DATABASE LOCATION
  bd automatically discovers your database:
    1. --db /path/to/.beads flag
    2. $BEADS_DB environment variable
    3. $BEADS_DIR environment variable
    4. .beads/ in current directory or ancestors

Ready to start!
Run bd create "My first issue" to create your first issue.
"#
    );
}
