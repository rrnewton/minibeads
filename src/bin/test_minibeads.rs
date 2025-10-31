//! Test toolkit for minibeads
//!
//! This binary provides various testing utilities for minibeads.
//!
//! Usage:
//!   test_minibeads random-actions [OPTIONS]
//!   test_minibeads --help

use anyhow::Result;
use clap::{Parser, Subcommand};
use minibeads::beads_generator::{ActionExecutor, ActionGenerator};
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
    verify_consistency(&executor, work_dir, verbose, use_no_db)?;

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

/// Verify the final state is consistent
fn verify_consistency(
    executor: &ActionExecutor,
    work_dir: &str,
    verbose: bool,
    use_no_db: bool,
) -> Result<()> {
    if verbose {
        println!("\nVerifying final state consistency...");
    }

    // Check that .beads directory exists
    let beads_dir = PathBuf::from(work_dir).join(".beads");
    if !beads_dir.exists() {
        return Err(anyhow::anyhow!(".beads directory does not exist"));
    }

    // For upstream with --no-db, verify that SQLite databases DO NOT exist
    if use_no_db {
        let db_files: Vec<_> = std::fs::read_dir(&beads_dir)?
            .filter_map(|e| e.ok())
            .filter(|e| e.path().extension().map(|ext| ext == "db").unwrap_or(false))
            .collect();

        if !db_files.is_empty() {
            return Err(anyhow::anyhow!(
                "SQLite database files found in .beads/ directory when using --no-db: {:?}",
                db_files.iter().map(|f| f.file_name()).collect::<Vec<_>>()
            ));
        }

        if verbose {
            println!("✓ Verified no SQLite database files exist (--no-db mode)");
        }
    }

    // Check that config.yaml exists
    let config_path = beads_dir.join("config.yaml");
    if !config_path.exists() {
        return Err(anyhow::anyhow!("config.yaml does not exist"));
    }

    // Try to list all issues
    let list_result = executor.execute(&minibeads::beads_generator::BeadsAction::List {
        status: None,
        priority: None,
    })?;

    if !list_result.success {
        return Err(anyhow::anyhow!(
            "Failed to list issues: {}",
            list_result.stderr
        ));
    }

    if verbose {
        println!("✅ Final state is consistent");
        println!("   Issues in database:\n{}", list_result.stdout);
    }

    Ok(())
}
