//! Random property-based testing for minibeads
//!
//! This test generates random sequences of beads commands and executes them
//! against minibeads to test robustness and correctness.
//!
//! Usage:
//!   cargo test --test random_minibeads -- --nocapture
//!   cargo test --test random_minibeads -- --nocapture --seed 42
//!   cargo test --test random_minibeads -- --nocapture --seed-from-entropy
//!   cargo test --test random_minibeads -- --nocapture --iters 10

mod beads_generator;

use anyhow::Result;
use beads_generator::{ActionExecutor, ActionGenerator};
use clap::Parser;
use rand::RngCore;
use std::path::PathBuf;

#[derive(Parser, Debug)]
#[command(name = "random_minibeads")]
#[command(about = "Random property-based testing for minibeads")]
struct Args {
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

    /// Path to minibeads binary (default: target/debug/bd)
    #[arg(long)]
    binary: Option<String>,

    /// Enable verbose output (print action sequence and detailed checks)
    #[arg(long, short)]
    verbose: bool,
}

fn main() -> Result<()> {
    // Parse CLI arguments
    let args = Args::parse();

    // Determine the binary path
    let binary_path = args.binary.unwrap_or_else(|| "target/debug/bd".to_string());

    // Check if binary exists
    if !PathBuf::from(&binary_path).exists() {
        eprintln!("ERROR: Binary not found at: {}", binary_path);
        eprintln!("Build it first with: cargo build");
        std::process::exit(1);
    }

    // Run iterations
    for iter in 0..args.iters {
        let seed = if args.seed_from_entropy {
            // Generate random seed
            let mut rng = rand::thread_rng();
            rng.next_u64()
        } else if let Some(s) = args.seed {
            // Use provided seed (possibly offset for multiple iterations)
            if args.iters > 1 {
                s.wrapping_add(iter as u64)
            } else {
                s
            }
        } else {
            // Default seed
            42u64.wrapping_add(iter as u64)
        };

        println!("\n{}", "=".repeat(60));
        println!("Iteration {}/{}", iter + 1, args.iters);
        println!("SEED: {}", seed);
        println!("{}\n", "=".repeat(60));

        // Run the test with this seed
        if let Err(e) = run_test(seed, args.actions_per_iter, &binary_path, args.verbose) {
            eprintln!("\n❌ TEST FAILED with SEED: {}", seed);
            eprintln!("Error: {:?}", e);
            eprintln!("\nTo reproduce this failure, run:");
            eprintln!(
                "  cargo test --test random_minibeads -- --nocapture --seed {}",
                seed
            );
            std::process::exit(1);
        }

        println!("✅ Iteration {} completed successfully\n", iter + 1);
    }

    println!("\n{}", "=".repeat(60));
    println!("✅ All {} iterations passed!", args.iters);
    println!("{}", "=".repeat(60));

    Ok(())
}

fn run_test(seed: u64, num_actions: usize, binary_path: &str, verbose: bool) -> Result<()> {
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
    let executor = ActionExecutor::new(binary_path, work_dir);

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
    verify_consistency(&executor, work_dir, verbose)?;

    Ok(())
}

/// Check if a failure is critical (unexpected) vs expected (validation error)
fn is_critical_failure(result: &beads_generator::ExecutionResult) -> bool {
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
fn verify_consistency(executor: &ActionExecutor, work_dir: &str, verbose: bool) -> Result<()> {
    if verbose {
        println!("\nVerifying final state consistency...");
    }

    // Check that .beads directory exists
    let beads_dir = PathBuf::from(work_dir).join(".beads");
    if !beads_dir.exists() {
        return Err(anyhow::anyhow!(".beads directory does not exist"));
    }

    // Check that config.yaml exists
    let config_path = beads_dir.join("config.yaml");
    if !config_path.exists() {
        return Err(anyhow::anyhow!("config.yaml does not exist"));
    }

    // Try to list all issues
    let list_result = executor.execute(&beads_generator::BeadsAction::List {
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

// Test entry point for cargo test
#[test]
fn test_random_minibeads() {
    // When run with `cargo test`, use default parameters
    let seed = 42u64;
    let actions_per_iter = 20;

    // Get absolute path to binary
    let binary_path = std::env::current_dir()
        .expect("Failed to get current directory")
        .join("target/debug/bd");
    let binary_path_str = binary_path.to_str().expect("Invalid path");

    println!("\nSEED: {}", seed);

    if !binary_path.exists() {
        panic!("Binary not found at: {}", binary_path.display());
    }

    if let Err(e) = run_test(seed, actions_per_iter, binary_path_str, false) {
        panic!("Test failed with SEED: {}\nError: {:?}", seed, e);
    }
}
