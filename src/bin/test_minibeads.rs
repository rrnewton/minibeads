//! Test toolkit for minibeads
//!
//! This binary provides various testing utilities for minibeads.
//!
//! Usage:
//!   test_minibeads random-actions [OPTIONS]
//!   test_minibeads --help

use anyhow::Result;
use clap::{Parser, Subcommand};
use minibeads::beads_generator::{
    ActionExecutor, ActionGenerator, BeadsAction, ReferenceInterpreter,
};
use rand::RngCore;
use std::io::Write;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

/// Verbosity level for logging
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum LogLevel {
    Normal,
    Verbose,
}

/// Logger that buffers output and can dump on failure
#[derive(Clone)]
struct Logger {
    buffer: Arc<Mutex<Vec<String>>>,
    verbosity: LogLevel,
    buffering: bool,
}

impl Logger {
    fn new(verbosity: LogLevel, buffering: bool) -> Self {
        Self {
            buffer: Arc::new(Mutex::new(Vec::new())),
            verbosity,
            buffering,
        }
    }

    /// Log a message at normal level
    fn log(&self, msg: String) {
        self.log_at_level(LogLevel::Normal, msg);
    }

    /// Log a message at verbose level (only shown if verbosity is Verbose)
    fn verbose(&self, msg: String) {
        self.log_at_level(LogLevel::Verbose, msg);
    }

    /// Log a message at the specified level
    fn log_at_level(&self, level: LogLevel, msg: String) {
        if self.buffering {
            // Buffer all messages regardless of level
            let mut buffer = self.buffer.lock().unwrap();
            buffer.push(msg);
        } else {
            // Only print if the message level is within our verbosity
            if level == LogLevel::Normal || self.verbosity == LogLevel::Verbose {
                println!("{}", msg);
            }
        }
    }

    /// Dump all buffered output to stdout
    fn dump(&self) {
        let buffer = self.buffer.lock().unwrap();
        for msg in buffer.iter() {
            println!("{}", msg);
        }
    }

    /// Clear the buffer without dumping
    fn clear(&self) {
        let mut buffer = self.buffer.lock().unwrap();
        buffer.clear();
    }
}

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
    /// Migration testing: generate numeric state, migrate to hash IDs, verify
    #[command(name = "migration-test")]
    MigrationTest {
        /// Seed for deterministic RNG
        #[arg(long)]
        seed: Option<u64>,

        /// Use entropy for random seed (non-deterministic)
        #[arg(long)]
        seed_from_entropy: bool,

        /// Number of actions to generate before migration
        #[arg(long, default_value = "50")]
        actions: usize,

        /// Enable verbose output (print action sequence and detailed checks)
        #[arg(long, short)]
        verbose: bool,
    },

    /// Bidirectional sync testing alternating between minibeads and upstream
    #[command(name = "sync-test")]
    SyncTest {
        /// Seed for deterministic RNG
        #[arg(long)]
        seed: Option<u64>,

        /// Use entropy for random seed (non-deterministic)
        #[arg(long)]
        seed_from_entropy: bool,

        /// Number of sync cycles to run
        #[arg(long, default_value = "5")]
        cycles: usize,

        /// Number of actions to generate per phase
        #[arg(long, default_value = "10")]
        actions_per_phase: usize,

        /// Enable verbose output (print action sequence and detailed checks)
        #[arg(long, short)]
        verbose: bool,
    },

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
        /// Mutually exclusive with --seconds
        #[arg(long, default_value = "1", conflicts_with = "seconds")]
        iters: usize,

        /// Run stress test for specified number of seconds
        /// Mutually exclusive with --iters
        #[arg(long, conflicts_with = "iters")]
        seconds: Option<u64>,

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

        /// Run stress tests in parallel on N cores (default: number of system cores)
        /// Only works with --seconds mode
        #[arg(long, value_name = "N", num_args = 0..=1, default_missing_value = "0", require_equals = true)]
        parallel: Option<usize>,

        /// Test JSONL import after upstream execution (only applies when --impl=upstream)
        /// Exports upstream database to JSONL and verifies minibeads can import it
        #[arg(long, default_value = "true", action = clap::ArgAction::Set)]
        test_import: bool,

        /// ID generation mode: numeric (sequential) or hash (content-based)
        /// Upstream only supports hash mode
        #[arg(long, default_value = "numeric")]
        ids: IdMode,
    },
}

#[derive(Debug, Clone, clap::ValueEnum)]
enum Implementation {
    Minibeads,
    Upstream,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, clap::ValueEnum)]
enum IdMode {
    Numeric,
    Hash,
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    // Determine worker thread count based on parallel flag
    // For parallel mode, we want worker threads = min(parallel_workers, num_cores)
    // to avoid wasting resources when parallel < cores
    let worker_threads = if let Commands::RandomActions {
        parallel: Some(n), ..
    } = &cli.command
    {
        let num_cores = std::thread::available_parallelism()
            .map(|n| n.get())
            .unwrap_or(1);
        let requested = if *n == 0 { num_cores } else { *n };
        std::cmp::min(requested, num_cores)
    } else {
        // Default: use all available cores for tokio runtime
        std::thread::available_parallelism()
            .map(|n| n.get())
            .unwrap_or(1)
    };

    // Build tokio runtime with configured worker threads
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(worker_threads)
        .enable_all()
        .build()?;

    runtime.block_on(async {
        match cli.command {
            Commands::MigrationTest {
                seed,
                seed_from_entropy,
                actions,
                verbose,
            } => run_migration_test(seed, seed_from_entropy, actions, verbose).await,
            Commands::SyncTest {
                seed,
                seed_from_entropy,
                cycles,
                actions_per_phase,
                verbose,
            } => run_sync_test(seed, seed_from_entropy, cycles, actions_per_phase, verbose).await,
            Commands::RandomActions {
                seed,
                seed_from_entropy,
                iters,
                seconds,
                actions_per_iter,
                r#impl,
                binary,
                verbose,
                parallel,
                test_import,
                ids,
            } => {
                run_random_actions(
                    seed,
                    seed_from_entropy,
                    iters,
                    seconds,
                    actions_per_iter,
                    r#impl,
                    binary,
                    verbose,
                    parallel,
                    test_import,
                    ids,
                )
                .await
            }
        }
    })
}

/// Migration test - generate numeric state, migrate to hash IDs, verify
async fn run_migration_test(
    seed: Option<u64>,
    seed_from_entropy: bool,
    num_actions: usize,
    verbose: bool,
) -> Result<()> {
    // Sample entropy ONCE at the beginning if requested
    let base_seed = if seed_from_entropy {
        let mut rng = rand::thread_rng();
        let entropy_seed = rng.next_u64();
        println!("🎲 Sampled entropy seed: {}", entropy_seed);
        println!("   (Use --seed {} to reproduce this run)\n", entropy_seed);
        entropy_seed
    } else {
        seed.unwrap_or(42u64) // Default seed
    };

    println!("🔄 Hash ID Migration Test");
    println!("Seed: {}", base_seed);
    println!("Actions: {}", num_actions);
    println!();

    // Find binary path
    let exe_path = std::env::current_exe()?;
    let exe_dir = exe_path
        .parent()
        .expect("Failed to get executable directory");
    let minibeads_binary = exe_dir.join(format!("bd{}", std::env::consts::EXE_SUFFIX));

    // Validate binary exists
    if !minibeads_binary.exists() {
        eprintln!(
            "ERROR: Minibeads binary not found at: {}",
            minibeads_binary.display()
        );
        eprintln!("Build it first with: cargo build");
        std::process::exit(1);
    }

    println!("📦 Minibeads: {}", minibeads_binary.display());
    println!();

    // Create working directory
    let temp_dir = tempfile::tempdir()?;
    let work_dir = temp_dir.path();
    println!("📁 Working directory: {}", work_dir.display());
    println!();

    let verbosity = if verbose {
        LogLevel::Verbose
    } else {
        LogLevel::Normal
    };
    let logger = Logger::new(verbosity, false);

    // Phase 1: Initialize with numeric IDs and populate
    println!("{}", "=".repeat(70));
    println!("Phase 1: Initialize with numeric IDs and populate");
    println!("{}", "=".repeat(70));

    let mut generator = ActionGenerator::new_with_mode(base_seed, false); // numeric mode
    let mut reference = ReferenceInterpreter::new("test".to_string());

    let executor = ActionExecutor::new(
        minibeads_binary.to_str().unwrap(),
        work_dir.to_str().unwrap(),
        false,
    );

    let actions = generator.generate_sequence(num_actions);
    logger.log(format!("Generated {} actions", actions.len()));

    execute_actions(&executor, &actions, &mut reference, &logger)?;

    let initial_issue_count = reference.get_final_state().len();
    println!(
        "✅ Phase 1 complete: {} issues created with numeric IDs",
        initial_issue_count
    );

    // Phase 2: Migrate to hash IDs
    println!("\n{}", "=".repeat(70));
    println!("Phase 2: Migrate to hash IDs");
    println!("{}", "=".repeat(70));

    logger.log("Running bd mb-migrate...".to_string());

    let migrate_output = std::process::Command::new(&minibeads_binary)
        .current_dir(work_dir)
        .args(["mb-migrate"])
        .output()?;

    if !migrate_output.status.success() {
        eprintln!(
            "❌ Migration failed:\n{}",
            String::from_utf8_lossy(&migrate_output.stderr)
        );
        return Err(anyhow::anyhow!("Migration failed"));
    }

    let migration_output = String::from_utf8_lossy(&migrate_output.stdout);
    logger.log(format!("Migration output:\n{}", migration_output));

    // Extract number of migrated issues from output
    let migrated_count = migration_output
        .lines()
        .find(|line| line.contains("Successfully migrated"))
        .and_then(|line| {
            line.split_whitespace()
                .find(|word| word.parse::<usize>().is_ok())
                .and_then(|num_str| num_str.parse::<usize>().ok())
        })
        .unwrap_or(0);

    println!(
        "✅ Phase 2 complete: Migrated {} issues to hash IDs",
        migrated_count
    );

    // Phase 3: Verify migration
    println!("\n{}", "=".repeat(70));
    println!("Phase 3: Verify migration");
    println!("{}", "=".repeat(70));

    logger.log("Verifying migration integrity...".to_string());

    // Count issues after migration
    let list_output = std::process::Command::new(&minibeads_binary)
        .current_dir(work_dir)
        .args(["list"])
        .output()?;

    if !list_output.status.success() {
        return Err(anyhow::anyhow!(
            "Failed to list issues after migration:\n{}",
            String::from_utf8_lossy(&list_output.stderr)
        ));
    }

    let list_text = String::from_utf8_lossy(&list_output.stdout);
    let post_migration_count = list_text.lines().filter(|line| !line.is_empty()).count();

    // Verify issue count matches
    if post_migration_count != initial_issue_count {
        eprintln!(
            "❌ Issue count mismatch! Before: {}, After: {}",
            initial_issue_count, post_migration_count
        );
        return Err(anyhow::anyhow!(
            "Migration lost issues: {} before, {} after",
            initial_issue_count,
            post_migration_count
        ));
    }

    logger.log(format!(
        "✓ Issue count preserved: {} issues",
        post_migration_count
    ));

    // Verify all IDs are now hash-based
    let mut all_hash_ids = true;
    let mut numeric_ids = Vec::new();

    for line in list_text.lines() {
        if let Some(id) = line.split(':').next() {
            let id = id.trim();
            // Check if ID contains hex digits (a-f)
            if !id.is_empty() && !id.contains(|c: char| c.is_ascii_hexdigit() && c.is_alphabetic())
            {
                all_hash_ids = false;
                numeric_ids.push(id.to_string());
            }
        }
    }

    if !all_hash_ids {
        eprintln!(
            "❌ Not all IDs are hash-based! Found numeric IDs: {:?}",
            numeric_ids
        );
        return Err(anyhow::anyhow!("Migration did not convert all IDs"));
    }

    logger.log("✓ All issue IDs are hash-based".to_string());

    // Verify dependencies are intact by checking a few issues
    let show_output = std::process::Command::new(&minibeads_binary)
        .current_dir(work_dir)
        .args(["stats"])
        .output()?;

    if !show_output.status.success() {
        return Err(anyhow::anyhow!(
            "Failed to get stats after migration:\n{}",
            String::from_utf8_lossy(&show_output.stderr)
        ));
    }

    logger.log(format!(
        "Stats after migration:\n{}",
        String::from_utf8_lossy(&show_output.stdout).trim()
    ));

    println!("✅ Phase 3 complete: Migration verified");

    println!("\n{}", "=".repeat(70));
    println!(
        "✅ Migration test passed! {} issues successfully migrated",
        initial_issue_count
    );
    println!("{}", "=".repeat(70));

    Ok(())
}

/// Bidirectional sync test - alternates between minibeads and upstream
async fn run_sync_test(
    seed: Option<u64>,
    seed_from_entropy: bool,
    cycles: usize,
    actions_per_phase: usize,
    verbose: bool,
) -> Result<()> {
    use rand::RngCore;

    // Sample entropy ONCE at the beginning if requested
    let base_seed = if seed_from_entropy {
        let mut rng = rand::thread_rng();
        let entropy_seed = rng.next_u64();
        println!("🎲 Sampled entropy seed: {}", entropy_seed);
        println!("   (Use --seed {} to reproduce this run)\n", entropy_seed);
        entropy_seed
    } else {
        seed.unwrap_or(42u64) // Default seed
    };

    println!("🔄 Bidirectional Sync Test");
    println!("Seed: {}", base_seed);
    println!("Cycles: {}", cycles);
    println!("Actions per phase: {}", actions_per_phase);
    println!();

    // Find binary paths
    let exe_path = std::env::current_exe()?;
    let exe_dir = exe_path
        .parent()
        .expect("Failed to get executable directory");
    let minibeads_binary = exe_dir.join(format!("bd{}", std::env::consts::EXE_SUFFIX));

    // Find project root for upstream binary
    let mut current = exe_dir;
    while !current.join("Cargo.toml").exists() {
        current = current
            .parent()
            .unwrap_or_else(|| panic!("Could not find project root"));
    }
    let upstream_binary = current
        .join("beads")
        .join(format!("bd-upstream{}", std::env::consts::EXE_SUFFIX));

    // Validate binaries exist
    if !minibeads_binary.exists() {
        eprintln!(
            "ERROR: Minibeads binary not found at: {}",
            minibeads_binary.display()
        );
        eprintln!("Build it first with: cargo build");
        std::process::exit(1);
    }
    if !upstream_binary.exists() {
        eprintln!(
            "ERROR: Upstream binary not found at: {}",
            upstream_binary.display()
        );
        eprintln!("Build it first with: make upstream");
        std::process::exit(1);
    }

    println!("📦 Minibeads: {}", minibeads_binary.display());
    println!("📦 Upstream: {}", upstream_binary.display());
    println!();

    // Create working directory
    let temp_dir = tempfile::tempdir()?;
    let work_dir = temp_dir.path();
    println!("📁 Working directory: {}", work_dir.display());
    println!();

    let verbosity = if verbose {
        LogLevel::Verbose
    } else {
        LogLevel::Normal
    };
    let logger = Logger::new(verbosity, false);

    // Phase 1: Initialize with minibeads and populate with random actions
    println!("{}", "=".repeat(70));
    println!("Phase 1: Initialize with minibeads and populate");
    println!("{}", "=".repeat(70));

    let phase1_seed = base_seed;
    let mut generator = ActionGenerator::new_with_mode(phase1_seed, true); // hash mode for upstream compat
    let mut reference = ReferenceInterpreter::new_with_hash_ids("test".to_string());

    let executor = ActionExecutor::new(
        minibeads_binary.to_str().unwrap(),
        work_dir.to_str().unwrap(),
        false,
    );

    let actions = generator.generate_sequence(actions_per_phase);
    logger.log(format!("Generated {} initial actions", actions.len()));

    execute_actions(&executor, &actions, &mut reference, &logger)?;

    // Phase 2: Sync to create JSONL
    println!("\n{}", "=".repeat(70));
    println!("Phase 2: Sync to create JSONL");
    println!("{}", "=".repeat(70));

    run_minibeads_sync(work_dir, &minibeads_binary, &logger)?;

    // Main test loop: Alternate between minibeads and upstream
    for cycle in 1..=cycles {
        println!("\n{}", "=".repeat(70));
        println!("Cycle {}/{}: Upstream phase", cycle, cycles);
        println!("{}", "=".repeat(70));

        // Generate actions for upstream phase
        let upstream_seed = base_seed.wrapping_add((cycle * 2 - 1) as u64);
        let mut upstream_gen = ActionGenerator::new_with_mode(upstream_seed, true);
        let upstream_actions = upstream_gen.generate_sequence(actions_per_phase);

        logger.log(format!(
            "Generated {} actions for upstream",
            upstream_actions.len()
        ));

        // Execute with upstream
        let upstream_executor = ActionExecutor::new(
            upstream_binary.to_str().unwrap(),
            work_dir.to_str().unwrap(),
            false,
        );

        execute_actions(
            &upstream_executor,
            &upstream_actions,
            &mut reference,
            &logger,
        )?;

        // Flush upstream to JSONL
        run_upstream_sync_flush(work_dir, &upstream_binary, &logger)?;

        println!("\n{}", "=".repeat(70));
        println!("Cycle {}/{}: Minibeads phase", cycle, cycles);
        println!("{}", "=".repeat(70));

        // Generate actions for minibeads phase
        let minibeads_seed = base_seed.wrapping_add((cycle * 2) as u64);
        let mut minibeads_gen = ActionGenerator::new_with_mode(minibeads_seed, true);
        let minibeads_actions = minibeads_gen.generate_sequence(actions_per_phase);

        logger.log(format!(
            "Generated {} actions for minibeads",
            minibeads_actions.len()
        ));

        // Execute with minibeads
        execute_actions(&executor, &minibeads_actions, &mut reference, &logger)?;

        // Sync minibeads (bidirectional)
        run_minibeads_sync(work_dir, &minibeads_binary, &logger)?;

        // Verify consistency at end of cycle
        println!("\n🔍 Verifying consistency after cycle {}...", cycle);
        verify_sync_consistency(
            work_dir,
            &minibeads_binary,
            &upstream_binary,
            &reference,
            &logger,
        )?;
        println!("✅ Cycle {} completed successfully", cycle);
    }

    println!("\n{}", "=".repeat(70));
    println!("✅ All {} cycles completed successfully!", cycles);
    println!("{}", "=".repeat(70));

    Ok(())
}

#[allow(clippy::too_many_arguments)]
async fn run_random_actions(
    seed: Option<u64>,
    seed_from_entropy: bool,
    iters: usize,
    seconds: Option<u64>,
    actions_per_iter: usize,
    implementation: Implementation,
    binary: Option<String>,
    verbose: bool,
    parallel: Option<usize>,
    test_import: bool,
    ids: IdMode,
) -> Result<()> {
    // Sample entropy ONCE at the beginning if requested
    // After this point, everything is deterministic based on this seed
    let base_seed = if seed_from_entropy {
        let mut rng = rand::thread_rng();
        let entropy_seed = rng.next_u64();
        println!("🎲 Sampled entropy seed: {}", entropy_seed);
        println!("   (Use --seed {} to reproduce this run)\n", entropy_seed);
        entropy_seed
    } else {
        seed.unwrap_or(42u64) // Default seed
    };

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
                let binary_name = format!("bd{}", std::env::consts::EXE_SUFFIX);
                exe_dir.join(&binary_name).to_str().unwrap().to_string()
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

                let binary_name = format!("bd-upstream{}", std::env::consts::EXE_SUFFIX);
                current
                    .join("beads")
                    .join(&binary_name)
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

    // Validate ID mode compatibility with implementation
    if matches!(implementation, Implementation::Upstream) && ids == IdMode::Numeric {
        eprintln!("ERROR: Upstream bd only supports hash-based IDs");
        eprintln!("       Use --ids=hash when testing with --impl=upstream");
        std::process::exit(1);
    }

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

    // No longer use --no-db flag - let upstream use SQLite database
    // We'll export to JSONL after execution for verification
    let use_no_db = false;

    // Check for parallel execution
    if let Some(num_workers) = parallel {
        // Parallel execution only works with --seconds mode
        if seconds.is_none() {
            eprintln!("ERROR: --parallel requires --seconds mode");
            std::process::exit(1);
        }

        // Determine number of workers (0 means use all cores)
        let workers = if num_workers == 0 {
            std::thread::available_parallelism()
                .map(|n| n.get())
                .unwrap_or(1)
        } else {
            num_workers
        };

        println!("\n🚀 Parallel stress test mode: {} workers", workers);
        println!("Duration: {} seconds", seconds.unwrap());
        println!("Base seed: {}", base_seed);
        println!("Will stop on first failure across all workers\n");

        let is_upstream = matches!(implementation, Implementation::Upstream);
        return run_parallel_stress_test(
            base_seed,
            seconds.unwrap(),
            actions_per_iter,
            &binary_path,
            verbose,
            use_no_db,
            is_upstream,
            workers,
            &implementation,
            test_import,
            ids,
        )
        .await;
    }

    // Unified sequential testing loop (handles both time-based and iteration-based modes)
    let start_time = seconds.map(|_| std::time::Instant::now());
    let duration = seconds.map(std::time::Duration::from_secs);

    // Print mode-specific header
    if let Some(duration_secs) = seconds {
        println!(
            "\n⏱️  Stress test mode: running for {} seconds",
            duration_secs
        );
        println!("Will stop on first failure or when time expires\n");
    }

    let verbosity = if verbose {
        LogLevel::Verbose
    } else {
        LogLevel::Normal
    };

    let mut iter = 0usize;
    loop {
        // Check stopping condition
        if let Some(d) = duration {
            if start_time.unwrap().elapsed() >= d {
                break;
            }
        } else if iter >= iters {
            break;
        }

        iter += 1;

        // Deterministic seed based on base_seed + iteration
        let iter_seed = if iters == 1 && seconds.is_none() {
            base_seed // Single iteration mode: use exact seed
        } else {
            base_seed.wrapping_add(iter as u64)
        };

        // Print iteration header
        println!("\n{}", "=".repeat(60));
        if let Some(duration_secs) = seconds {
            let elapsed = start_time.unwrap().elapsed().as_secs();
            println!(
                "Iteration {} | Elapsed: {}s / {}s",
                iter, elapsed, duration_secs
            );
        } else {
            println!("Iteration {}/{}", iter, iters);
        }
        println!("SEED: {}", iter_seed);
        println!("{}\n", "=".repeat(60));

        // Run the test
        let logger = Logger::new(verbosity, false);
        let is_upstream = matches!(implementation, Implementation::Upstream);
        if let Err(e) = run_test(
            iter_seed,
            actions_per_iter,
            &binary_path,
            &logger,
            use_no_db,
            is_upstream,
            test_import,
            ids,
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
            if seconds.is_some() {
                eprintln!("\nStopping on first failure after {} iterations", iter);
            }
            std::process::exit(1);
        }

        println!("✅ Iteration {} completed successfully", iter);
    }

    // Print summary
    println!("\n{}", "=".repeat(60));
    if seconds.is_some() {
        let total_elapsed = start_time.unwrap().elapsed();
        println!(
            "✅ Stress test completed! {} iterations in {:.1}s",
            iter,
            total_elapsed.as_secs_f64()
        );
    } else {
        println!("✅ All {} iterations passed!", iters);
    }
    println!("{}", "=".repeat(60));

    Ok(())
}

/// Run parallel stress tests with multiple async worker tasks
#[allow(clippy::too_many_arguments)]
async fn run_parallel_stress_test(
    base_seed: u64,
    duration_secs: u64,
    actions_per_iter: usize,
    binary_path: &str,
    verbose: bool,
    use_no_db: bool,
    is_upstream: bool,
    workers: usize,
    implementation: &Implementation,
    test_import: bool,
    ids: IdMode,
) -> Result<()> {
    use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
    use std::sync::Arc;

    let start_time = std::time::Instant::now();
    let duration = tokio::time::Duration::from_secs(duration_secs);

    // Shared state for coordinating workers
    let should_stop = Arc::new(AtomicBool::new(false));
    let total_iterations = Arc::new(AtomicU64::new(0));

    // Spawn progress reporter task
    let progress_should_stop = Arc::clone(&should_stop);
    let progress_iterations = Arc::clone(&total_iterations);
    let progress_handle = tokio::spawn(async move {
        while !progress_should_stop.load(Ordering::Relaxed) {
            tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;
            let count = progress_iterations.load(Ordering::Relaxed);
            print!("\r⏱️  Progress: {} iterations completed", count);
            std::io::stdout().flush().ok();
        }
        // Clear the progress line when done
        print!("\r{}\r", " ".repeat(50));
        std::io::stdout().flush().ok();
    });

    // Spawn async worker tasks
    let mut handles = vec![];

    for worker_id in 0..workers {
        let should_stop = Arc::clone(&should_stop);
        let total_iterations = Arc::clone(&total_iterations);
        let binary_path = binary_path.to_string();

        let handle = tokio::spawn(async move {
            let mut local_iterations = 0u64;

            // Create buffering logger for parallel workers
            let verbosity = if verbose {
                LogLevel::Verbose
            } else {
                LogLevel::Normal
            };
            let logger = Logger::new(verbosity, true);

            // Each worker gets its own deterministic seed offset from the base seed
            // Worker 0: base_seed + 0
            // Worker 1: base_seed + 1000000
            // Worker 2: base_seed + 2000000, etc.
            let worker_base_seed = base_seed.wrapping_add((worker_id as u64) * 1000000);

            while start_time.elapsed() < duration && !should_stop.load(Ordering::Relaxed) {
                local_iterations += 1;
                let iter_seed = worker_base_seed.wrapping_add(local_iterations);

                // Run the test
                if let Err(e) = run_test(
                    iter_seed,
                    actions_per_iter,
                    &binary_path,
                    &logger,
                    use_no_db,
                    is_upstream,
                    test_import,
                    ids,
                ) {
                    // Signal all workers to stop
                    should_stop.store(true, Ordering::Relaxed);

                    // Dump buffered output on failure
                    logger.dump();

                    // Update total before returning
                    total_iterations.fetch_add(local_iterations, Ordering::Relaxed);

                    // Return error info
                    return Err((worker_id, iter_seed, e, local_iterations));
                }

                // Clear buffer after successful iteration to avoid memory buildup
                logger.clear();

                // Update progress counter after each successful iteration
                total_iterations.fetch_add(1, Ordering::Relaxed);
            }
            Ok::<_, (usize, u64, anyhow::Error, u64)>(local_iterations)
        });

        handles.push(handle);
    }

    // Wait for all workers to complete
    let mut failure: Option<(usize, u64, anyhow::Error, u64)> = None;

    for handle in handles {
        match handle.await {
            Ok(Ok(_)) => {}
            Ok(Err(err_info)) => {
                if failure.is_none() {
                    failure = Some(err_info);
                }
            }
            Err(e) => {
                eprintln!("Worker task panicked or was cancelled: {:?}", e);
            }
        }
    }

    // Signal progress reporter to stop
    should_stop.store(true, Ordering::Relaxed);

    let total_elapsed = start_time.elapsed();
    let total_iters = total_iterations.load(Ordering::Relaxed);

    // Wait for progress reporter to finish
    progress_handle.await.ok();

    // Report failure if any
    if let Some((worker_id, iter_seed, error, worker_iters)) = failure {
        eprintln!("\n❌ TEST FAILED in worker {}", worker_id);
        eprintln!("SEED: {}", iter_seed);
        eprintln!("Error: {:?}", error);
        eprintln!(
            "\nWorker {} completed {} iterations before failure",
            worker_id, worker_iters
        );
        eprintln!("Total iterations across all workers: {}", total_iters);
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

    // Success!
    println!("\n{}", "=".repeat(60));
    println!(
        "✅ Parallel stress test completed! {} iterations in {:.1}s",
        total_iters,
        total_elapsed.as_secs_f64()
    );
    println!(
        "   Throughput: {:.1} iterations/second",
        total_iters as f64 / total_elapsed.as_secs_f64()
    );
    println!("{}", "=".repeat(60));

    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn run_test(
    seed: u64,
    num_actions: usize,
    binary_path: &str,
    logger: &Logger,
    use_no_db: bool,
    is_upstream: bool,
    test_import: bool,
    ids: IdMode,
) -> Result<()> {
    // Create a temporary directory for this test
    let temp_dir = tempfile::tempdir()?;
    let work_dir = temp_dir.path().to_str().unwrap();

    logger.log(format!("Working directory: {}", work_dir));

    // Create action generator
    let use_hash_ids = ids == IdMode::Hash;
    let mut generator = ActionGenerator::new_with_mode(seed, use_hash_ids);

    // Generate action sequence
    let actions = generator.generate_sequence(num_actions);
    logger.log(format!("Generated {} actions", actions.len()));

    // Print action sequence if verbose
    logger.verbose("\n📋 Generated Action Sequence:".to_string());
    for (i, action) in actions.iter().enumerate() {
        logger.verbose(format!("  {}. {}", i + 1, action));
    }
    logger.verbose(String::new());

    // Create executor
    let executor = ActionExecutor::new(binary_path, work_dir, use_no_db);

    // Create reference interpreter to maintain golden state
    let mut reference = match ids {
        IdMode::Numeric => ReferenceInterpreter::new("test".to_string()),
        IdMode::Hash => ReferenceInterpreter::new_with_hash_ids("test".to_string()),
    };

    // Execute actions
    logger.log("\nExecuting actions (use --verbose for details)...".to_string());
    let mut success_count = 0;
    let mut failure_count = 0;

    for (i, action) in actions.iter().enumerate() {
        logger.verbose(format!("{:3}. {:?}", i + 1, action));

        match executor.execute(action) {
            Ok(result) => {
                if result.success {
                    success_count += 1;
                    // Update reference interpreter with successful actions
                    // For hash mode Create actions, replace the placeholder ID with actual ID
                    let action_for_ref = if let Some(ref actual_id) = result.actual_issue_id {
                        if let BeadsAction::Create {
                            expected_id,
                            title,
                            priority,
                            issue_type,
                            description,
                        } = action
                        {
                            if expected_id.contains("HASH") {
                                // Replace placeholder with actual ID for hash mode
                                BeadsAction::Create {
                                    expected_id: actual_id.clone(),
                                    title: title.clone(),
                                    priority: *priority,
                                    issue_type: *issue_type,
                                    description: description.clone(),
                                }
                            } else {
                                action.clone()
                            }
                        } else {
                            action.clone()
                        }
                    } else {
                        action.clone()
                    };

                    if let Err(e) = reference.execute(&action_for_ref) {
                        let err_msg = format!("     ❌ Reference interpreter failed: {:?}", e);
                        logger.log(err_msg.clone());
                        logger.dump(); // Dump buffer on failure
                        eprintln!("{}", err_msg);
                        return Err(e);
                    }
                } else {
                    // Some failures are expected (e.g., dependency cycles, duplicate deps)
                    // Only fail the test for unexpected errors
                    if is_critical_failure(&result) {
                        let err_msgs = vec![
                            "     ❌ CRITICAL FAILURE".to_string(),
                            format!("     Exit code: {:?}", result.exit_code),
                            format!("     Stderr: {}", result.stderr),
                        ];
                        for msg in &err_msgs {
                            logger.log(msg.clone());
                        }
                        logger.dump(); // Dump buffer on failure
                        for msg in &err_msgs {
                            eprintln!("{}", msg);
                        }
                        return Err(anyhow::anyhow!(
                            "Critical failure on action {}: {:?}",
                            i + 1,
                            action
                        ));
                    } else {
                        // Expected failure (e.g., validation error)
                        failure_count += 1;
                        logger.verbose(format!(
                            "     ⚠️  Expected failure: {}",
                            result.stderr.lines().next().unwrap_or("")
                        ));
                    }
                }
            }
            Err(e) => {
                let err_msg = format!("     ❌ Failed to execute action: {:?}", e);
                logger.log(err_msg.clone());
                logger.dump(); // Dump buffer on failure
                eprintln!("{}", err_msg);
                return Err(e);
            }
        }
    }

    logger.log(format!(
        "\nResults: {} successful, {} expected failures",
        success_count, failure_count
    ));

    // Verify final state consistency
    let (bytes_written, num_issues) = verify_consistency(
        &executor,
        work_dir,
        binary_path,
        logger,
        use_no_db,
        is_upstream,
        &mut reference,
    )?;

    // Print summary stats
    logger.log(format!("📊 Issues generated: {}", num_issues));
    logger.log(format!(
        "💾 Bytes written: {} ({:.1} KB)",
        bytes_written,
        bytes_written as f64 / 1024.0
    ));

    // If testing upstream and import test is enabled, verify JSONL import
    if is_upstream && test_import {
        logger.log("\n📦 Testing JSONL import capability...".to_string());
        test_jsonl_import(work_dir, binary_path, logger, &reference)?;
    }

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
/// Returns (bytes_written, num_issues) on success
fn verify_consistency(
    _executor: &ActionExecutor,
    work_dir: &str,
    binary_path: &str,
    logger: &Logger,
    _use_no_db: bool,
    is_upstream: bool,
    reference: &mut ReferenceInterpreter,
) -> Result<(u64, usize)> {
    logger.verbose(
        "\n🔍 Deep verification: comparing actual state with reference interpreter...".to_string(),
    );

    // Check that .beads directory exists
    let beads_dir = PathBuf::from(work_dir).join(".beads");
    if !beads_dir.exists() {
        return Err(anyhow::anyhow!(".beads directory does not exist"));
    }

    // Step 1: Recursively walk .beads directory and report files
    let (files, total_size) = walk_beads_directory(&beads_dir, logger)?;

    logger.verbose(format!(
        "   Found {} files, total size: {} bytes",
        files.len(),
        total_size
    ));

    // Step 2: Verify config.yaml and compare prefix
    // For upstream, we need to accept whatever prefix it chose (it ignores --prefix flag)
    // and update the reference interpreter to match
    verify_config(&beads_dir, reference, logger, is_upstream)?;

    // Step 3: Compare actual issues with reference state
    if is_upstream {
        // Upstream: Export database to JSONL in two ways and verify both match reference
        verify_upstream_dual_export(work_dir, binary_path, reference, logger)?;
    } else {
        // Minibeads: verify markdown files in issues/ directory
        verify_minibeads_state(&beads_dir, reference, logger)?;
    }

    logger.log("✅ Verification passed".to_string());

    // Return metrics
    let num_issues = reference.get_final_state().len();
    Ok((total_size, num_issues))
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
    logger: &Logger,
) -> Result<(Vec<FileInfo>, u64)> {
    let mut files = Vec::new();
    let mut total_size = 0u64;

    fn walk_recursive(
        dir: &std::path::Path,
        files: &mut Vec<FileInfo>,
        total_size: &mut u64,
        logger: &Logger,
    ) -> Result<()> {
        for entry in std::fs::read_dir(dir)? {
            let entry = entry?;
            let path = entry.path();
            let metadata = entry.metadata()?;

            if metadata.is_dir() {
                walk_recursive(&path, files, total_size, logger)?;
            } else {
                let size = metadata.len();
                *total_size += size;
                logger.verbose(format!("      {:6} bytes  {}", size, path.display()));
                files.push(FileInfo { path, size });
            }
        }
        Ok(())
    }

    walk_recursive(beads_dir, &mut files, &mut total_size, logger)?;
    Ok((files, total_size))
}

/// Parse and verify config.yaml using shared Storage code
fn verify_config(
    beads_dir: &std::path::Path,
    reference: &mut ReferenceInterpreter,
    logger: &Logger,
    is_upstream: bool,
) -> Result<()> {
    use minibeads::storage::Storage;

    // Use Storage::get_prefix() to read config - this is the single source of truth
    let storage = Storage::open(beads_dir.to_path_buf())?;
    let actual_prefix = storage.get_prefix()?;
    let expected_prefix = reference.get_prefix().to_string(); // Clone to avoid borrow issues

    if actual_prefix != expected_prefix {
        if is_upstream {
            // Upstream bd writes directory name to config.json but DOES use the requested
            // prefix for issue IDs. So tolerate config mismatch for upstream.
            logger.verbose(format!(
                "   ⚠️  Upstream config.json has prefix '{}' but uses '{}' for issue IDs",
                actual_prefix, expected_prefix
            ));
        } else {
            return Err(anyhow::anyhow!(
                "Prefix mismatch: expected '{}', got '{}'",
                expected_prefix,
                actual_prefix
            ));
        }
    } else {
        logger.verbose(format!(
            "   ✓ config.yaml: prefix matches ('{}')",
            actual_prefix
        ));
    }

    Ok(())
}

/// Verify minibeads state (markdown files)
fn verify_minibeads_state(
    beads_dir: &std::path::Path,
    reference: &ReferenceInterpreter,
    logger: &Logger,
) -> Result<()> {
    let issues_dir = beads_dir.join("issues");
    if !issues_dir.exists() {
        // No issues created yet is valid
        if reference.get_final_state().is_empty() {
            logger.verbose("   ✓ No issues directory (no issues created)".to_string());
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
    compare_issue_states(&actual_issues, reference.get_final_state(), logger)?;

    Ok(())
}

/// Verify upstream state with dual export verification
/// Exports database using both `bd export` and `bd sync --flush-only` and verifies both match reference
fn verify_upstream_dual_export(
    work_dir: &str,
    binary_path: &str,
    reference: &ReferenceInterpreter,
    logger: &Logger,
) -> Result<()> {
    logger.verbose("   Performing dual export verification...".to_string());

    // Path 1: Export using `bd export -o custom.jsonl`
    let custom_export_path = PathBuf::from(work_dir).join("custom.jsonl");
    logger.verbose(format!(
        "   Exporting via 'bd export -o {}'...",
        custom_export_path.display()
    ));

    let export_output = std::process::Command::new(binary_path)
        .current_dir(work_dir)
        .args(["export", "-o", custom_export_path.to_str().unwrap()])
        .output()?;

    if !export_output.status.success() {
        return Err(anyhow::anyhow!(
            "Failed to export database via 'bd export':\n{}",
            String::from_utf8_lossy(&export_output.stderr)
        ));
    }

    logger.verbose("   ✓ bd export completed".to_string());

    // Path 2: Export using `bd sync --flush-only` to create .beads/issues.jsonl
    logger.verbose("   Flushing via 'bd sync --flush-only'...".to_string());

    let sync_output = std::process::Command::new(binary_path)
        .current_dir(work_dir)
        .args(["sync", "--flush-only"])
        .output()?;

    if !sync_output.status.success() {
        return Err(anyhow::anyhow!(
            "Failed to flush database via 'bd sync --flush-only':\n{}",
            String::from_utf8_lossy(&sync_output.stderr)
        ));
    }

    logger.verbose("   ✓ bd sync --flush-only completed".to_string());

    // Parse both JSONL files
    let beads_dir = PathBuf::from(work_dir).join(".beads");
    let sync_jsonl_path = beads_dir.join("issues.jsonl");

    // Verify both files exist
    if !custom_export_path.exists() {
        return Err(anyhow::anyhow!("custom.jsonl not created by bd export"));
    }
    if !sync_jsonl_path.exists() {
        return Err(anyhow::anyhow!(
            ".beads/issues.jsonl not created by bd sync --flush-only"
        ));
    }

    // Parse custom export
    logger.verbose("   Parsing custom.jsonl...".to_string());
    let custom_issues = parse_jsonl_file(&custom_export_path)?;
    logger.verbose(format!(
        "   ✓ Parsed {} issues from custom.jsonl",
        custom_issues.len()
    ));

    // Parse sync export
    logger.verbose("   Parsing .beads/issues.jsonl...".to_string());
    let sync_issues = parse_jsonl_file(&sync_jsonl_path)?;
    logger.verbose(format!(
        "   ✓ Parsed {} issues from issues.jsonl",
        sync_issues.len()
    ));

    // Verify both exports match each other
    logger.verbose("   Verifying both exports match each other...".to_string());
    compare_issue_states(&custom_issues, &sync_issues, logger)?;
    logger.verbose("   ✓ Both exports match each other".to_string());

    // Verify both exports match reference
    logger.verbose("   Verifying exports match reference...".to_string());
    compare_issue_states(&custom_issues, reference.get_final_state(), logger)?;
    logger.verbose("   ✓ Exports match reference".to_string());

    Ok(())
}

/// Parse a JSONL file into a HashMap of ReferenceIssue
fn parse_jsonl_file(
    path: &PathBuf,
) -> Result<std::collections::HashMap<String, minibeads::beads_generator::ReferenceIssue>> {
    let mut issues = std::collections::HashMap::new();
    let jsonl_content = std::fs::read_to_string(path)?;

    for (line_num, line) in jsonl_content.lines().enumerate() {
        if line.trim().is_empty() {
            continue;
        }
        let issue: serde_json::Value = serde_json::from_str(line).map_err(|e| {
            anyhow::anyhow!(
                "Failed to parse JSONL line {} in {}: {}",
                line_num + 1,
                path.display(),
                e
            )
        })?;
        let ref_issue = parse_jsonl_to_reference_issue(&issue)?;
        issues.insert(ref_issue.id.clone(), ref_issue);
    }

    Ok(issues)
}

/// Parse a minibeads markdown issue file to ReferenceIssue using shared format code
fn parse_minibeads_issue(path: &PathBuf) -> Result<minibeads::beads_generator::ReferenceIssue> {
    use minibeads::beads_generator::ReferenceIssue;
    use minibeads::format::markdown_to_issue;

    // Extract issue ID from filename
    let issue_id = path
        .file_stem()
        .and_then(|s| s.to_str())
        .ok_or_else(|| anyhow::anyhow!("Cannot extract ID from filename {:?}", path))?;

    // Read file content
    let content = std::fs::read_to_string(path)?;

    // Use the standard markdown parser from the main codebase
    let issue = markdown_to_issue(issue_id, &content)?;

    // Convert from full Issue to simplified ReferenceIssue
    Ok(ReferenceIssue {
        id: issue.id,
        title: issue.title,
        description: issue.description,
        status: issue.status,
        priority: issue.priority,
        issue_type: issue.issue_type,
        depends_on: issue.depends_on,
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
    let status = status_str.parse()?;

    let priority = issue
        .get("priority")
        .and_then(|v| v.as_i64())
        .ok_or_else(|| anyhow::anyhow!("priority not found for {}", id))? as i32;

    let issue_type_str = issue
        .get("issue_type")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow::anyhow!("issue_type not found for {}", id))?;
    let issue_type = issue_type_str.parse()?;

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
            let dep_type = dep_type_str.parse()?;
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

/// Compare actual issues with reference state
fn compare_issue_states(
    actual: &std::collections::HashMap<String, minibeads::beads_generator::ReferenceIssue>,
    expected: &std::collections::HashMap<String, minibeads::beads_generator::ReferenceIssue>,
    logger: &Logger,
) -> Result<()> {
    use similar_asserts::assert_eq;

    // Check counts match
    if actual.len() != expected.len() {
        eprintln!("\n❌ Issue count mismatch!");
        eprintln!(
            "Expected {} issues, but got {} issues\n",
            expected.len(),
            actual.len()
        );

        // Show which issues are in expected but not actual, and vice versa
        let expected_ids: std::collections::HashSet<_> = expected.keys().collect();
        let actual_ids: std::collections::HashSet<_> = actual.keys().collect();

        let missing: Vec<_> = expected_ids.difference(&actual_ids).collect();
        let extra: Vec<_> = actual_ids.difference(&expected_ids).collect();

        if !missing.is_empty() {
            eprintln!(
                "Missing issues (in reference but not in actual): {:?}",
                missing
            );
        }
        if !extra.is_empty() {
            eprintln!("Extra issues (in actual but not in reference): {:?}", extra);
        }

        return Err(anyhow::anyhow!(
            "Issue count mismatch: expected {}, got {}",
            expected.len(),
            actual.len()
        ));
    }

    if !expected.is_empty() {
        logger.verbose(format!("   ✓ Issue count matches: {}", expected.len()));
    }

    // Compare each issue
    for (id, expected_issue) in expected {
        let actual_issue = actual.get(id).ok_or_else(|| {
            anyhow::anyhow!("Issue {} exists in reference but not in actual state", id)
        })?;

        // Use similar_asserts for colorful diffs on mismatch
        // Wrap in a catch_unwind to convert panic to Result
        let comparison_result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            // Compare title
            if actual_issue.title != expected_issue.title {
                eprintln!("\n❌ Title mismatch for issue {}:", id);
                assert_eq!(
                    &expected_issue.title, &actual_issue.title,
                    "Title mismatch for {}",
                    id
                );
            }

            // Compare status
            if actual_issue.status != expected_issue.status {
                eprintln!("\n❌ Status mismatch for issue {}:", id);
                assert_eq!(
                    &expected_issue.status, &actual_issue.status,
                    "Status mismatch for {}",
                    id
                );
            }

            // Compare priority
            if actual_issue.priority != expected_issue.priority {
                eprintln!("\n❌ Priority mismatch for issue {}:", id);
                assert_eq!(
                    &expected_issue.priority, &actual_issue.priority,
                    "Priority mismatch for {}",
                    id
                );
            }

            // Compare issue_type
            if actual_issue.issue_type != expected_issue.issue_type {
                eprintln!("\n❌ IssueType mismatch for issue {}:", id);
                assert_eq!(
                    &expected_issue.issue_type, &actual_issue.issue_type,
                    "IssueType mismatch for {}",
                    id
                );
            }

            // Compare dependencies
            if actual_issue.depends_on != expected_issue.depends_on {
                eprintln!("\n❌ Dependencies mismatch for issue {}:", id);

                // Convert to sorted vectors for better diff display
                let mut expected_deps: Vec<_> = expected_issue.depends_on.iter().collect();
                let mut actual_deps: Vec<_> = actual_issue.depends_on.iter().collect();
                expected_deps.sort_by_key(|(k, _)| *k);
                actual_deps.sort_by_key(|(k, _)| *k);

                assert_eq!(
                    &expected_deps, &actual_deps,
                    "Dependencies mismatch for {}",
                    id
                );
            }
        }));

        // If comparison panicked (assertion failed), convert to error
        if let Err(panic_info) = comparison_result {
            // The panic from similar_asserts already printed colorful output
            // Just return an error to stop the test
            if let Some(s) = panic_info.downcast_ref::<String>() {
                return Err(anyhow::anyhow!(
                    "Verification failed for issue {}: {}",
                    id,
                    s
                ));
            } else if let Some(s) = panic_info.downcast_ref::<&str>() {
                return Err(anyhow::anyhow!(
                    "Verification failed for issue {}: {}",
                    id,
                    s
                ));
            } else {
                return Err(anyhow::anyhow!("Verification failed for issue {}", id));
            }
        }

        logger.verbose(format!("   ✓ Issue {} matches reference", id));
    }

    Ok(())
}

/// Test JSONL import from upstream to minibeads
fn test_jsonl_import(
    work_dir: &str,
    upstream_binary: &str,
    logger: &Logger,
    reference: &ReferenceInterpreter,
) -> Result<()> {
    logger.verbose("   Exporting upstream database to JSONL...".to_string());

    // Find the minibeads binary
    let exe_path = std::env::current_exe().expect("Failed to get current executable path");
    let exe_dir = exe_path
        .parent()
        .expect("Failed to get executable directory");
    let binary_name = format!("bd{}", std::env::consts::EXE_SUFFIX);
    let minibeads_binary = exe_dir.join(&binary_name).to_str().unwrap().to_string();

    if !PathBuf::from(&minibeads_binary).exists() {
        return Err(anyhow::anyhow!(
            "Minibeads binary not found at: {}. Build it first with: cargo build",
            minibeads_binary
        ));
    }

    // Export upstream database to JSONL
    let export_path = PathBuf::from(work_dir).join("export.jsonl");
    let export_output = std::process::Command::new(upstream_binary)
        .current_dir(work_dir)
        .args(["export", "-o", export_path.to_str().unwrap()])
        .output()?;

    if !export_output.status.success() {
        return Err(anyhow::anyhow!(
            "Failed to export upstream database:\n{}",
            String::from_utf8_lossy(&export_output.stderr)
        ));
    }

    logger.verbose(format!("   ✓ Exported to {}", export_path.display()));

    // Parse the exported JSONL
    logger.verbose("   Parsing exported JSONL...".to_string());
    let jsonl_content = std::fs::read_to_string(&export_path)?;
    let mut exported_issues = std::collections::HashMap::new();

    for (line_num, line) in jsonl_content.lines().enumerate() {
        if line.trim().is_empty() {
            continue;
        }
        let issue: serde_json::Value = serde_json::from_str(line)
            .map_err(|e| anyhow::anyhow!("Failed to parse JSONL line {}: {}", line_num + 1, e))?;
        let ref_issue = parse_jsonl_to_reference_issue(&issue)?;
        exported_issues.insert(ref_issue.id.clone(), ref_issue);
    }

    logger.verbose(format!(
        "   ✓ Parsed {} issues from JSONL",
        exported_issues.len()
    ));

    // Verify exported JSONL matches reference
    logger.verbose("   Verifying exported JSONL matches reference...".to_string());
    compare_issue_states(&exported_issues, reference.get_final_state(), logger)?;
    logger.verbose("   ✓ Exported JSONL matches reference".to_string());

    // Create a fresh directory for import test
    let import_dir = tempfile::tempdir()?;
    let import_work_dir = import_dir.path().to_str().unwrap();

    logger.verbose(format!(
        "   Testing minibeads import in fresh directory: {}",
        import_work_dir
    ));

    // Initialize minibeads in import directory with same prefix
    let init_output = std::process::Command::new(&minibeads_binary)
        .current_dir(import_work_dir)
        .args(["init", "--prefix", reference.get_prefix()])
        .output()?;

    if !init_output.status.success() {
        return Err(anyhow::anyhow!(
            "Failed to init minibeads in import directory:\n{}",
            String::from_utf8_lossy(&init_output.stderr)
        ));
    }

    // Copy JSONL to import directory
    let import_jsonl_path = PathBuf::from(import_work_dir).join("export.jsonl");
    std::fs::copy(&export_path, &import_jsonl_path)?;

    // Import into minibeads
    logger.verbose("   Importing JSONL into minibeads...".to_string());
    let import_output = std::process::Command::new(&minibeads_binary)
        .current_dir(import_work_dir)
        .args(["import", "-i", import_jsonl_path.to_str().unwrap()])
        .output()?;

    if !import_output.status.success() {
        return Err(anyhow::anyhow!(
            "Failed to import JSONL into minibeads:\n{}",
            String::from_utf8_lossy(&import_output.stderr)
        ));
    }

    logger.verbose(format!(
        "   ✓ Import completed: {}",
        String::from_utf8_lossy(&import_output.stdout)
            .lines()
            .next()
            .unwrap_or("")
    ));

    // Verify minibeads imported correctly
    logger.verbose("   Verifying minibeads imported state...".to_string());
    let import_beads_dir = PathBuf::from(import_work_dir).join(".beads");
    verify_minibeads_state(&import_beads_dir, reference, logger)?;

    logger.log("✅ JSONL import test passed".to_string());

    Ok(())
}

/// Execute a sequence of actions and update reference
fn execute_actions(
    executor: &ActionExecutor,
    actions: &[BeadsAction],
    reference: &mut ReferenceInterpreter,
    logger: &Logger,
) -> Result<()> {
    let mut success_count = 0;
    let mut failure_count = 0;

    for (i, action) in actions.iter().enumerate() {
        logger.verbose(format!("{:3}. {:?}", i + 1, action));

        match executor.execute(action) {
            Ok(result) => {
                if result.success {
                    success_count += 1;
                    // Update reference interpreter with successful actions
                    let action_for_ref = if let Some(ref actual_id) = result.actual_issue_id {
                        if let BeadsAction::Create {
                            expected_id,
                            title,
                            priority,
                            issue_type,
                            description,
                        } = action
                        {
                            if expected_id.contains("HASH") {
                                BeadsAction::Create {
                                    expected_id: actual_id.clone(),
                                    title: title.clone(),
                                    priority: *priority,
                                    issue_type: *issue_type,
                                    description: description.clone(),
                                }
                            } else {
                                action.clone()
                            }
                        } else {
                            action.clone()
                        }
                    } else {
                        action.clone()
                    };

                    if let Err(e) = reference.execute(&action_for_ref) {
                        logger.log(format!("❌ Reference interpreter failed: {:?}", e));
                        return Err(e);
                    }
                } else if is_critical_failure(&result) {
                    logger.log(format!("❌ CRITICAL FAILURE: {}", result.stderr));
                    return Err(anyhow::anyhow!(
                        "Critical failure on action {}: {:?}",
                        i + 1,
                        action
                    ));
                } else {
                    failure_count += 1;
                    logger.verbose(format!(
                        "     ⚠️  Expected failure: {}",
                        result.stderr.lines().next().unwrap_or("")
                    ));
                }
            }
            Err(e) => {
                logger.log(format!("❌ Failed to execute action: {:?}", e));
                return Err(e);
            }
        }
    }

    logger.log(format!(
        "Results: {} successful, {} expected failures",
        success_count, failure_count
    ));
    Ok(())
}

/// Run minibeads sync (bidirectional)
fn run_minibeads_sync(
    work_dir: &std::path::Path,
    minibeads_binary: &std::path::Path,
    logger: &Logger,
) -> Result<()> {
    logger.log("Running minibeads sync...".to_string());

    let output = std::process::Command::new(minibeads_binary)
        .current_dir(work_dir)
        .args(["sync"])
        .output()?;

    if !output.status.success() {
        return Err(anyhow::anyhow!(
            "Minibeads sync failed:\n{}",
            String::from_utf8_lossy(&output.stderr)
        ));
    }

    logger.verbose(format!(
        "  {}",
        String::from_utf8_lossy(&output.stdout).trim()
    ));
    logger.log("✅ Minibeads sync completed".to_string());
    Ok(())
}

/// Run upstream sync --flush-only
fn run_upstream_sync_flush(
    work_dir: &std::path::Path,
    upstream_binary: &std::path::Path,
    logger: &Logger,
) -> Result<()> {
    logger.log("Running upstream sync --flush-only...".to_string());

    let output = std::process::Command::new(upstream_binary)
        .current_dir(work_dir)
        .args(["sync", "--flush-only"])
        .output()?;

    if !output.status.success() {
        return Err(anyhow::anyhow!(
            "Upstream sync --flush-only failed:\n{}",
            String::from_utf8_lossy(&output.stderr)
        ));
    }

    logger.verbose(format!(
        "  {}",
        String::from_utf8_lossy(&output.stdout).trim()
    ));
    logger.log("✅ Upstream sync --flush-only completed".to_string());
    Ok(())
}

/// Verify sync consistency by comparing both implementations' states with reference
fn verify_sync_consistency(
    work_dir: &std::path::Path,
    _minibeads_binary: &std::path::Path,
    upstream_binary: &std::path::Path,
    reference: &ReferenceInterpreter,
    logger: &Logger,
) -> Result<()> {
    let beads_dir = work_dir.join(".beads");

    // Verify minibeads markdown state
    logger.verbose("Verifying minibeads markdown state...".to_string());
    verify_minibeads_state(&beads_dir, reference, logger)?;
    logger.verbose("✅ Minibeads markdown matches reference".to_string());

    // Verify upstream by exporting and comparing
    logger.verbose("Verifying upstream state via export...".to_string());
    verify_upstream_dual_export(
        work_dir.to_str().unwrap(),
        upstream_binary.to_str().unwrap(),
        reference,
        logger,
    )?;
    logger.verbose("✅ Upstream state matches reference".to_string());

    // Verify .beads/issues.jsonl exists and matches (created by both implementations)
    let jsonl_path = beads_dir.join("issues.jsonl");
    if !jsonl_path.exists() {
        return Err(anyhow::anyhow!(
            ".beads/issues.jsonl does not exist after sync"
        ));
    }

    logger.verbose("Verifying .beads/issues.jsonl...".to_string());
    let jsonl_issues = parse_jsonl_file(&jsonl_path)?;
    compare_issue_states(&jsonl_issues, reference.get_final_state(), logger)?;
    logger.verbose("✅ .beads/issues.jsonl matches reference".to_string());

    Ok(())
}
