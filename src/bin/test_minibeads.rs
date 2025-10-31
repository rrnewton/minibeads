//! Test toolkit for minibeads
//!
//! This binary provides various testing utilities for minibeads.
//!
//! Usage:
//!   test_minibeads random-actions [OPTIONS]
//!   test_minibeads --help

use anyhow::Result;
use clap::{Parser, Subcommand};
use minibeads::beads_generator::{ActionExecutor, ActionGenerator, ReferenceInterpreter};
use rand::RngCore;
use std::path::PathBuf;

#[derive(Parser, Debug)]
#[command(name = "test_minibeads")]
#[command(about = "Test toolkit for minibeads")]
#[command(version)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand, Debug)]
enum Commands {
    /// Random property-based testing for beads implementations
    #[command(name = "random-actions")]
    RandomActions {
        /// Seed for deterministic RNG
        #[arg(long)]
        seed: Option<u64>,

        /// Use entropy for random seed (non-deterministic)
        #[arg(long)]
        seed_from_entropy: bool,

        /// Number of iterations to run (each with a different seed)
        #[arg(long, default_value = "1")]
        iters: usize,

        /// Number of actions to generate per iteration
        #[arg(long, default_value = "20")]
        actions_per_iter: usize,

        /// Implementation to test: minibeads or upstream
        #[arg(long, default_value = "minibeads")]
        r#impl: Implementation,

        /// Path to binary (overrides --impl default)
        #[arg(long)]
        binary: Option<String>,

        /// Enable verbose output (print action sequence and detailed checks)
        #[arg(long, short)]
        verbose: bool,
    },
}

#[derive(Debug, Clone, clap::ValueEnum)]
enum Implementation {
    Minibeads,
    Upstream,
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Commands::RandomActions {
            seed,
            seed_from_entropy,
            iters,
            actions_per_iter,
            r#impl,
            binary,
            verbose,
        } => run_random_actions(
            seed,
            seed_from_entropy,
            iters,
            actions_per_iter,
            r#impl,
            binary,
            verbose,
        ),
    }
}

fn run_random_actions(
    seed: Option<u64>,
    seed_from_entropy: bool,
    iters: usize,
    actions_per_iter: usize,
    implementation: Implementation,
    binary: Option<String>,
    verbose: bool,
) -> Result<()> {
    // Determine the binary path
    let binary_path = if let Some(path) = binary {
        // Explicit path provided
        path
    } else {
        // Default based on implementation
        match implementation {
            Implementation::Minibeads => {
                // minibeads bd is in same directory as test_minibeads
                let exe_path =
                    std::env::current_exe().expect("Failed to get current executable path");
                let exe_dir = exe_path
                    .parent()
                    .expect("Failed to get executable directory");
                exe_dir.join("bd").to_str().unwrap().to_string()
            }
            Implementation::Upstream => {
                // upstream bd is in beads/bd-upstream relative to project root
                // Find project root by going up from current_exe until we find Cargo.toml
                let exe_path =
                    std::env::current_exe().expect("Failed to get current executable path");
                let mut current = exe_path
                    .parent()
                    .expect("Failed to get executable directory");

                // Go up directories to find project root (has Cargo.toml)
                while !current.join("Cargo.toml").exists() {
                    current = current
                        .parent()
                        .unwrap_or_else(|| panic!("Could not find project root (Cargo.toml)"));
                }

                current
                    .join("beads/bd-upstream")
                    .to_str()
                    .unwrap()
                    .to_string()
            }
        }
    };

    let impl_name = match implementation {
        Implementation::Minibeads => "minibeads",
        Implementation::Upstream => "upstream bd",
    };

    // Check if binary exists
    if !PathBuf::from(&binary_path).exists() {
        eprintln!("ERROR: Binary not found at: {}", binary_path);
        match implementation {
            Implementation::Minibeads => eprintln!("Build it first with: cargo build"),
            Implementation::Upstream => eprintln!("Build it first with: make upstream"),
        }
        std::process::exit(1);
    }

    println!("Testing implementation: {}", impl_name);
    println!("Binary: {}", binary_path);

    // Run iterations
    for iter in 0..iters {
        let iter_seed = if seed_from_entropy {
            // Generate random seed
            let mut rng = rand::thread_rng();
            rng.next_u64()
        } else if let Some(s) = seed {
            // Use provided seed (possibly offset for multiple iterations)
            if iters > 1 {
                s.wrapping_add(iter as u64)
            } else {
                s
            }
        } else {
            // Default seed
            42u64.wrapping_add(iter as u64)
        };

        println!("\n{}", "=".repeat(60));
        println!("Iteration {}/{}", iter + 1, iters);
        println!("SEED: {}", iter_seed);
        println!("{}\n", "=".repeat(60));

        // Determine if we should use --no-db flag (for upstream)
        let use_no_db = matches!(implementation, Implementation::Upstream);

        // Run the test with this seed
        if let Err(e) = run_test(
            iter_seed,
            actions_per_iter,
            &binary_path,
            verbose,
            use_no_db,
        ) {
            eprintln!("\n❌ TEST FAILED with SEED: {}", iter_seed);
            eprintln!("Error: {:?}", e);
            eprintln!("\nTo reproduce this failure, run:");
            let impl_flag = match implementation {
                Implementation::Minibeads => "--impl minibeads",
                Implementation::Upstream => "--impl upstream",
            };
            eprintln!(
                "  test_minibeads random-actions --seed {} {}",
                iter_seed, impl_flag
            );
            std::process::exit(1);
        }

        println!("✅ Iteration {} completed successfully\n", iter + 1);
    }

    println!("\n{}", "=".repeat(60));
    println!("✅ All {} iterations passed!", iters);
    println!("{}", "=".repeat(60));

    Ok(())
}

fn run_test(
    seed: u64,
    num_actions: usize,
    binary_path: &str,
    verbose: bool,
    use_no_db: bool,
) -> Result<()> {
    // Create a temporary directory for this test
    let temp_dir = tempfile::tempdir()?;
    let work_dir = temp_dir.path().to_str().unwrap();

    println!("Working directory: {}", work_dir);

    // Create action generator
    let mut generator = ActionGenerator::new(seed);

    // Generate action sequence
    let actions = generator.generate_sequence(num_actions);
    println!("Generated {} actions", actions.len());

    // Print action sequence if verbose
    if verbose {
        println!("\n📋 Generated Action Sequence:");
        for (i, action) in actions.iter().enumerate() {
            println!("  {}. {}", i + 1, action);
        }
        println!();
    }

    // Create executor
    let executor = ActionExecutor::new(binary_path, work_dir, use_no_db);

    // Create reference interpreter to maintain golden state
    let mut reference = ReferenceInterpreter::new("test".to_string());

    // Execute actions
    if verbose {
        println!("\nExecuting actions...");
    } else {
        println!("\nExecuting actions (use --verbose for details)...");
    }
    let mut success_count = 0;
    let mut failure_count = 0;

    for (i, action) in actions.iter().enumerate() {
        if verbose {
            println!("{:3}. {:?}", i + 1, action);
        }

        match executor.execute(action) {
            Ok(result) => {
                if result.success {
                    success_count += 1;
                    // Update reference interpreter with successful actions
                    if let Err(e) = reference.execute(action) {
                        eprintln!("     ❌ Reference interpreter failed: {:?}", e);
                        return Err(e);
                    }
                } else {
                    // Some failures are expected (e.g., dependency cycles, duplicate deps)
                    // Only fail the test for unexpected errors
                    if is_critical_failure(&result) {
                        eprintln!("     ❌ CRITICAL FAILURE");
                        eprintln!("     Exit code: {:?}", result.exit_code);
                        eprintln!("     Stderr: {}", result.stderr);
                        return Err(anyhow::anyhow!(
                            "Critical failure on action {}: {:?}",
                            i + 1,
                            action
                        ));
                    } else {
                        // Expected failure (e.g., validation error)
                        failure_count += 1;
                        if verbose {
                            println!(
                                "     ⚠️  Expected failure: {}",
                                result.stderr.lines().next().unwrap_or("")
                            );
                        }
                    }
                }
            }
            Err(e) => {
                eprintln!("     ❌ Failed to execute action: {:?}", e);
                return Err(e);
            }
        }
    }

    println!(
        "\nResults: {} successful, {} expected failures",
        success_count, failure_count
    );

    // Verify final state consistency
    verify_consistency(&executor, work_dir, verbose, use_no_db, &reference)?;

    Ok(())
}

/// Check if a failure is critical (unexpected) vs expected (validation error)
fn is_critical_failure(result: &minibeads::beads_generator::ExecutionResult) -> bool {
    // These are expected validation errors, not critical failures
    let expected_errors = [
        "already exists",
        "not found",
        "cycle detected",
        "Invalid status",
        "Invalid priority",
        "already has dependency",
        "cannot depend on itself",
    ];

    for expected in &expected_errors {
        if result.stderr.contains(expected) {
            return false;
        }
    }

    // If exit code is non-zero and it's not an expected error, it's critical
    result.exit_code.unwrap_or(1) != 0
}

/// Verify the final state is consistent with the reference interpreter
fn verify_consistency(
    _executor: &ActionExecutor,
    work_dir: &str,
    verbose: bool,
    use_no_db: bool,
    reference: &ReferenceInterpreter,
) -> Result<()> {
    if verbose {
        println!("\n🔍 Deep verification: comparing actual state with reference interpreter...");
    }

    // Check that .beads directory exists
    let beads_dir = PathBuf::from(work_dir).join(".beads");
    if !beads_dir.exists() {
        return Err(anyhow::anyhow!(".beads directory does not exist"));
    }

    // Step 1: Recursively walk .beads directory and report files
    let (files, total_size) = walk_beads_directory(&beads_dir, verbose)?;

    if verbose {
        println!(
            "   Found {} files, total size: {} bytes",
            files.len(),
            total_size
        );
    }

    // Step 2: For upstream with --no-db, verify that SQLite databases DO NOT exist
    if use_no_db {
        let db_files: Vec<_> = files
            .iter()
            .filter(|f| f.path.extension().map(|ext| ext == "db").unwrap_or(false))
            .collect();

        if !db_files.is_empty() {
            return Err(anyhow::anyhow!(
                "SQLite database files found in .beads/ directory when using --no-db: {:?}",
                db_files
                    .iter()
                    .map(|f| f.path.file_name())
                    .collect::<Vec<_>>()
            ));
        }

        if verbose {
            println!("   ✓ No SQLite database files (--no-db mode)");
        }
    }

    // Step 3: Verify config.yaml and compare prefix
    let config_path = beads_dir.join("config.yaml");
    if !config_path.exists() {
        return Err(anyhow::anyhow!("config.yaml does not exist"));
    }

    verify_config(&config_path, reference, verbose)?;

    // Step 4: Compare actual issues with reference state
    if use_no_db {
        // Upstream: verify issues.jsonl
        verify_upstream_state(&beads_dir, reference, verbose)?;
    } else {
        // Minibeads: verify markdown files in issues/ directory
        verify_minibeads_state(&beads_dir, reference, verbose)?;
    }

    if verbose {
        println!("✅ Deep verification passed: all states match reference interpreter");
    } else {
        println!("✅ Verification passed");
    }

    Ok(())
}

#[derive(Debug)]
#[allow(dead_code)]
struct FileInfo {
    path: PathBuf,
    size: u64,
}

/// Recursively walk .beads directory and collect file info
fn walk_beads_directory(
    beads_dir: &std::path::Path,
    verbose: bool,
) -> Result<(Vec<FileInfo>, u64)> {
    let mut files = Vec::new();
    let mut total_size = 0u64;

    fn walk_recursive(
        dir: &std::path::Path,
        files: &mut Vec<FileInfo>,
        total_size: &mut u64,
        verbose: bool,
    ) -> Result<()> {
        for entry in std::fs::read_dir(dir)? {
            let entry = entry?;
            let path = entry.path();
            let metadata = entry.metadata()?;

            if metadata.is_dir() {
                walk_recursive(&path, files, total_size, verbose)?;
            } else {
                let size = metadata.len();
                *total_size += size;
                if verbose {
                    println!("      {:6} bytes  {}", size, path.display());
                }
                files.push(FileInfo { path, size });
            }
        }
        Ok(())
    }

    walk_recursive(beads_dir, &mut files, &mut total_size, verbose)?;
    Ok((files, total_size))
}

/// Parse and verify config.yaml
fn verify_config(
    config_path: &std::path::Path,
    reference: &ReferenceInterpreter,
    verbose: bool,
) -> Result<()> {
    let config_str = std::fs::read_to_string(config_path)?;
    let config: serde_yaml::Value = serde_yaml::from_str(&config_str)?;

    // Minibeads uses "issue-prefix" in config.yaml
    let actual_prefix = config
        .get("issue-prefix")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow::anyhow!("issue-prefix not found in config.yaml"))?;

    let expected_prefix = reference.get_prefix();

    if actual_prefix != expected_prefix {
        return Err(anyhow::anyhow!(
            "Prefix mismatch: expected '{}', got '{}'",
            expected_prefix,
            actual_prefix
        ));
    }

    if verbose {
        println!("   ✓ config.yaml: prefix matches ('{}')", actual_prefix);
    }

    Ok(())
}

/// Verify minibeads state (markdown files)
fn verify_minibeads_state(
    beads_dir: &std::path::Path,
    reference: &ReferenceInterpreter,
    verbose: bool,
) -> Result<()> {
    let issues_dir = beads_dir.join("issues");
    if !issues_dir.exists() {
        // No issues created yet is valid
        if reference.get_final_state().is_empty() {
            if verbose {
                println!("   ✓ No issues directory (no issues created)");
            }
            return Ok(());
        } else {
            return Err(anyhow::anyhow!(
                "Reference has {} issues but issues/ directory does not exist",
                reference.get_final_state().len()
            ));
        }
    }

    // Read all markdown files
    let mut actual_issues = std::collections::HashMap::new();
    for entry in std::fs::read_dir(&issues_dir)? {
        let entry = entry?;
        let path = entry.path();
        if path.extension().map(|e| e == "md").unwrap_or(false) {
            let issue = parse_minibeads_issue(&path)?;
            actual_issues.insert(issue.id.clone(), issue);
        }
    }

    // Compare with reference
    compare_issue_states(&actual_issues, reference.get_final_state(), verbose)?;

    Ok(())
}

/// Verify upstream state (issues.jsonl)
fn verify_upstream_state(
    beads_dir: &std::path::Path,
    reference: &ReferenceInterpreter,
    verbose: bool,
) -> Result<()> {
    let jsonl_path = beads_dir.join("issues.jsonl");

    if !jsonl_path.exists() {
        // No issues.jsonl is valid if no issues were created
        if reference.get_final_state().is_empty() {
            if verbose {
                println!("   ✓ No issues.jsonl (no issues created)");
            }
            return Ok(());
        } else {
            return Err(anyhow::anyhow!(
                "Reference has {} issues but issues.jsonl does not exist",
                reference.get_final_state().len()
            ));
        }
    }

    // Parse JSONL file
    let mut actual_issues = std::collections::HashMap::new();
    let jsonl_content = std::fs::read_to_string(&jsonl_path)?;
    for line in jsonl_content.lines() {
        if line.trim().is_empty() {
            continue;
        }
        let issue: serde_json::Value = serde_json::from_str(line)?;
        let ref_issue = parse_jsonl_to_reference_issue(&issue)?;
        actual_issues.insert(ref_issue.id.clone(), ref_issue);
    }

    // Compare with reference
    compare_issue_states(&actual_issues, reference.get_final_state(), verbose)?;

    Ok(())
}

/// Parse a minibeads markdown issue file to ReferenceIssue
fn parse_minibeads_issue(path: &PathBuf) -> Result<minibeads::beads_generator::ReferenceIssue> {
    use minibeads::beads_generator::ReferenceIssue;

    let content = std::fs::read_to_string(path)?;

    // Split into frontmatter and body
    let parts: Vec<&str> = content.split("---").collect();
    if parts.len() < 3 {
        return Err(anyhow::anyhow!("Invalid markdown format in {:?}", path));
    }

    let frontmatter: serde_yaml::Value = serde_yaml::from_str(parts[1])?;
    let body = parts[2].trim();

    let id = if let Some(id_value) = frontmatter.get("id").and_then(|v| v.as_str()) {
        id_value.to_string()
    } else {
        // Extract ID from filename if not in frontmatter
        path.file_stem()
            .and_then(|s| s.to_str())
            .ok_or_else(|| anyhow::anyhow!("Cannot extract ID from filename {:?}", path))?
            .to_string()
    };

    let title = frontmatter
        .get("title")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow::anyhow!("title not found in {:?}", path))?
        .to_string();

    let status_str = frontmatter
        .get("status")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow::anyhow!("status not found in {:?}", path))?;
    let status = convert_status_from_str(status_str)?;

    let priority = frontmatter
        .get("priority")
        .and_then(|v| v.as_i64())
        .ok_or_else(|| anyhow::anyhow!("priority not found in {:?}", path))?
        as i32;

    let issue_type_str = frontmatter
        .get("issue_type")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow::anyhow!("issue_type not found in {:?}", path))?;
    let issue_type = convert_issue_type_from_str(issue_type_str)?;

    // Parse dependencies from frontmatter
    // In minibeads format, depends_on is a YAML map: { test-1: related, test-2: blocks }
    let mut depends_on = std::collections::HashMap::new();
    if let Some(deps) = frontmatter.get("depends_on").and_then(|v| v.as_mapping()) {
        for (dep_id_val, dep_type_val) in deps {
            let dep_id = dep_id_val.as_str().unwrap_or("").to_string();
            let dep_type_str = dep_type_val.as_str().unwrap_or("blocks");
            let dep_type = convert_dep_type_from_str(dep_type_str)?;
            depends_on.insert(dep_id, dep_type);
        }
    }

    Ok(ReferenceIssue {
        id,
        title,
        description: body.to_string(),
        status,
        priority,
        issue_type,
        depends_on,
    })
}

/// Parse JSONL issue to ReferenceIssue
fn parse_jsonl_to_reference_issue(
    issue: &serde_json::Value,
) -> Result<minibeads::beads_generator::ReferenceIssue> {
    use minibeads::beads_generator::ReferenceIssue;

    let id = issue
        .get("id")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow::anyhow!("id not found in JSONL issue"))?
        .to_string();

    let title = issue
        .get("title")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow::anyhow!("title not found for {}", id))?
        .to_string();

    let description = issue
        .get("description")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();

    let status_str = issue
        .get("status")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow::anyhow!("status not found for {}", id))?;
    let status = convert_status_from_str(status_str)?;

    let priority = issue
        .get("priority")
        .and_then(|v| v.as_i64())
        .ok_or_else(|| anyhow::anyhow!("priority not found for {}", id))? as i32;

    let issue_type_str = issue
        .get("issue_type")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow::anyhow!("issue_type not found for {}", id))?;
    let issue_type = convert_issue_type_from_str(issue_type_str)?;

    // Parse dependencies
    let mut depends_on = std::collections::HashMap::new();
    if let Some(deps) = issue.get("dependencies").and_then(|v| v.as_array()) {
        for dep in deps {
            let dep_id = dep
                .get("id")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let dep_type_str = dep.get("type").and_then(|v| v.as_str()).unwrap_or("blocks");
            let dep_type = convert_dep_type_from_str(dep_type_str)?;
            depends_on.insert(dep_id, dep_type);
        }
    }

    Ok(ReferenceIssue {
        id,
        title,
        description,
        status,
        priority,
        issue_type,
        depends_on,
    })
}

/// Convert status string to beads_generator::Status
fn convert_status_from_str(s: &str) -> Result<minibeads::beads_generator::Status> {
    use minibeads::beads_generator::Status;
    match s {
        "open" => Ok(Status::Open),
        "in_progress" => Ok(Status::InProgress),
        "blocked" => Ok(Status::Blocked),
        "closed" => Ok(Status::Closed),
        _ => Err(anyhow::anyhow!("Invalid status: {}", s)),
    }
}

/// Convert issue type string to beads_generator::IssueType
fn convert_issue_type_from_str(s: &str) -> Result<minibeads::beads_generator::IssueType> {
    use minibeads::beads_generator::IssueType;
    match s {
        "bug" => Ok(IssueType::Bug),
        "feature" => Ok(IssueType::Feature),
        "task" => Ok(IssueType::Task),
        "epic" => Ok(IssueType::Epic),
        "chore" => Ok(IssueType::Chore),
        _ => Err(anyhow::anyhow!("Invalid issue type: {}", s)),
    }
}

/// Convert dependency type string to beads_generator::DependencyType
fn convert_dep_type_from_str(s: &str) -> Result<minibeads::beads_generator::DependencyType> {
    use minibeads::beads_generator::DependencyType;
    match s {
        "blocks" => Ok(DependencyType::Blocks),
        "related" => Ok(DependencyType::Related),
        "parent-child" => Ok(DependencyType::ParentChild),
        _ => Err(anyhow::anyhow!("Invalid dependency type: {}", s)),
    }
}

/// Compare actual issues with reference state
fn compare_issue_states(
    actual: &std::collections::HashMap<String, minibeads::beads_generator::ReferenceIssue>,
    expected: &std::collections::HashMap<String, minibeads::beads_generator::ReferenceIssue>,
    verbose: bool,
) -> Result<()> {
    // Check counts match
    if actual.len() != expected.len() {
        return Err(anyhow::anyhow!(
            "Issue count mismatch: expected {}, got {}",
            expected.len(),
            actual.len()
        ));
    }

    if verbose && !expected.is_empty() {
        println!("   ✓ Issue count matches: {}", expected.len());
    }

    // Compare each issue
    for (id, expected_issue) in expected {
        let actual_issue = actual.get(id).ok_or_else(|| {
            anyhow::anyhow!("Issue {} exists in reference but not in actual state", id)
        })?;

        // Compare fields
        if actual_issue.title != expected_issue.title {
            return Err(anyhow::anyhow!(
                "Title mismatch for {}: expected '{}', got '{}'",
                id,
                expected_issue.title,
                actual_issue.title
            ));
        }

        if actual_issue.status != expected_issue.status {
            return Err(anyhow::anyhow!(
                "Status mismatch for {}: expected '{:?}', got '{:?}'",
                id,
                expected_issue.status,
                actual_issue.status
            ));
        }

        if actual_issue.priority != expected_issue.priority {
            return Err(anyhow::anyhow!(
                "Priority mismatch for {}: expected {}, got {}",
                id,
                expected_issue.priority,
                actual_issue.priority
            ));
        }

        if actual_issue.issue_type != expected_issue.issue_type {
            return Err(anyhow::anyhow!(
                "IssueType mismatch for {}: expected '{:?}', got '{:?}'",
                id,
                expected_issue.issue_type,
                actual_issue.issue_type
            ));
        }

        // Compare dependencies
        if actual_issue.depends_on.len() != expected_issue.depends_on.len() {
            return Err(anyhow::anyhow!(
                "Dependency count mismatch for {}: expected {}, got {}",
                id,
                expected_issue.depends_on.len(),
                actual_issue.depends_on.len()
            ));
        }

        for (dep_id, expected_dep_type) in &expected_issue.depends_on {
            let actual_dep_type = actual_issue.depends_on.get(dep_id).ok_or_else(|| {
                anyhow::anyhow!(
                    "Dependency {} -> {} exists in reference but not in actual state",
                    id,
                    dep_id
                )
            })?;

            if actual_dep_type != expected_dep_type {
                return Err(anyhow::anyhow!(
                    "Dependency type mismatch for {} -> {}: expected '{:?}', got '{:?}'",
                    id,
                    dep_id,
                    expected_dep_type,
                    actual_dep_type
                ));
            }
        }

        if verbose {
            println!("   ✓ Issue {} matches reference", id);
        }
    }

    Ok(())
}
