//! Cargo test wrapper for random property-based testing
//!
//! This test invokes the test_minibeads binary with the random-actions subcommand.
//! For manual testing with custom parameters, use the binary directly:
//!
//!   cargo build --release --bin test_minibeads
//!   ./target/release/test_minibeads random-actions --seed 42 --verbose
//!   ./target/release/test_minibeads random-actions --seed 42 --impl upstream
//!   ./target/release/test_minibeads random-actions --seed-from-entropy --iters 10

use std::path::PathBuf;
use std::process::Command;

/// Build minibeads binaries in release mode
fn build_minibeads() -> PathBuf {
    // Build the mb binary
    let mb_build = Command::new("cargo")
        .args(["build", "--release", "--bin", "mb"])
        .status()
        .expect("Failed to build mb binary");
    assert!(mb_build.success(), "Failed to build mb binary");

    // Build the test_minibeads binary
    let test_build = Command::new("cargo")
        .args(["build", "--release", "--bin", "test_minibeads"])
        .status()
        .expect("Failed to build test_minibeads binary");
    assert!(
        test_build.success(),
        "Failed to build test_minibeads binary"
    );

    // Return path to test_minibeads binary
    std::env::current_dir()
        .expect("Failed to get current directory")
        .join("target/release/test_minibeads")
}

/// Build upstream bd binary
fn build_upstream() -> bool {
    let upstream_build = Command::new("make")
        .arg("upstream")
        .status()
        .expect("Failed to build upstream bd binary");

    if !upstream_build.success() {
        println!("⚠️  Skipping upstream test: make upstream failed");
        println!("   This is expected if beads/ submodule is not initialized");
        return false;
    }

    true
}

/// Run test_minibeads random-actions with specified arguments
fn run_test(binary_path: &PathBuf, args: &[&str], test_name: &str) {
    println!(
        "\nRunning: {} random-actions {}",
        binary_path.display(),
        args.join(" ")
    );

    let output = Command::new(binary_path)
        .arg("random-actions")
        .args(args)
        .output()
        .expect("Failed to execute test_minibeads");

    // Print output for debugging
    println!("{}", String::from_utf8_lossy(&output.stdout));
    if !output.status.success() {
        eprintln!("{}", String::from_utf8_lossy(&output.stderr));
    }

    assert!(
        output.status.success(),
        "{} failed with exit code: {:?}",
        test_name,
        output.status.code()
    );
}

/// Run test_minibeads sync-test with specified arguments
fn run_sync_test(binary_path: &PathBuf, args: &[&str], test_name: &str) {
    println!(
        "\nRunning: {} sync-test {}",
        binary_path.display(),
        args.join(" ")
    );

    let output = Command::new(binary_path)
        .arg("sync-test")
        .args(args)
        .output()
        .expect("Failed to execute test_minibeads sync-test");

    // Print output for debugging
    println!("{}", String::from_utf8_lossy(&output.stdout));
    if !output.status.success() {
        eprintln!("{}", String::from_utf8_lossy(&output.stderr));
    }

    assert!(
        output.status.success(),
        "{} failed with exit code: {:?}",
        test_name,
        output.status.code()
    );
}

#[test]
#[cfg(not(tarpaulin))] // Skip under coverage - this test invokes external binaries
fn test_random_actions_minibeads_numeric() {
    let binary_path = build_minibeads();
    run_test(
        &binary_path,
        &["--seed", "42", "--impl", "minibeads", "--ids", "numeric"],
        "Random test against minibeads with numeric IDs",
    );
}

#[test]
#[cfg(not(tarpaulin))] // Skip under coverage - this test invokes external binaries
fn test_random_actions_minibeads_hash() {
    let binary_path = build_minibeads();
    run_test(
        &binary_path,
        &["--seed", "42", "--impl", "minibeads", "--ids", "hash"],
        "Random test against minibeads with hash IDs",
    );
}

#[test]
#[cfg(not(tarpaulin))] // Skip under coverage - this test invokes external binaries
fn test_stress_minibeads_parallel_numeric() {
    let binary_path = build_minibeads();
    run_test(
        &binary_path,
        &[
            "--seed",
            "42",
            "--impl",
            "minibeads",
            "--seconds",
            "3",
            "--parallel=3",
            "--ids",
            "numeric",
        ],
        "Parallel stress test against minibeads with numeric IDs",
    );
}

#[test]
#[cfg(not(tarpaulin))] // Skip under coverage - this test invokes external binaries
fn test_stress_minibeads_parallel_hash() {
    let binary_path = build_minibeads();
    run_test(
        &binary_path,
        &[
            "--seed",
            "42",
            "--impl",
            "minibeads",
            "--seconds",
            "3",
            "--parallel=3",
            "--ids",
            "hash",
        ],
        "Parallel stress test against minibeads with hash IDs",
    );
}

#[test]
#[cfg(not(tarpaulin))] // Skip under coverage - this test invokes external binaries
fn test_random_actions_upstream() {
    if !build_upstream() {
        return;
    }
    let binary_path = build_minibeads();
    run_test(
        &binary_path,
        &[
            "--seed",
            "42",
            "--impl",
            "upstream",
            "--ids",
            "hash",
            "--test-import=false",
        ],
        "Random test against upstream bd with hash IDs",
    );
}

#[test]
#[cfg(not(tarpaulin))] // Skip under coverage - this test invokes external binaries
#[ignore] // Requires upstream bd to be built
fn test_stress_upstream_parallel() {
    if !build_upstream() {
        return;
    }
    let binary_path = build_minibeads();
    run_test(
        &binary_path,
        &[
            "--seed",
            "42",
            "--impl",
            "upstream",
            "--seconds",
            "3",
            "--parallel=3",
            "--test-import",
            "true",
            "--ids",
            "hash",
        ],
        "Parallel stress test against upstream bd with hash IDs and import",
    );
}

#[test]
#[cfg(not(tarpaulin))] // Skip under coverage - this test invokes external binaries
fn test_sync_stress() {
    // Check if upstream is available
    if !build_upstream() {
        return;
    }

    let binary_path = build_minibeads();
    run_sync_test(
        &binary_path,
        &[
            "--seed",
            "12345",
            "--cycles",
            "10",
            "--actions-per-phase",
            "15",
        ],
        "Bidirectional sync stress test (10 cycles, 15 actions/phase, ~3s)",
    );
}

#[test]
#[cfg(not(tarpaulin))] // Skip under coverage - this test invokes external binaries
fn test_migration_stress() {
    let binary_path = build_minibeads();
    run_migration_test(
        &binary_path,
        &["--seed", "54321", "--actions", "50"],
        "Hash ID migration stress test (50 actions, then migrate)",
    );
}

/// Run migration test: generate numeric state, migrate to hash, verify
fn run_migration_test(binary_path: &PathBuf, args: &[&str], test_name: &str) {
    println!(
        "\nRunning: {} migration-test {}",
        binary_path.display(),
        args.join(" ")
    );

    let output = Command::new(binary_path)
        .arg("migration-test")
        .args(args)
        .output()
        .expect("Failed to execute test_minibeads migration-test");

    // Print output for debugging
    println!("{}", String::from_utf8_lossy(&output.stdout));
    if !output.status.success() {
        eprintln!("{}", String::from_utf8_lossy(&output.stderr));
    }

    assert!(
        output.status.success(),
        "{} failed with exit code: {:?}",
        test_name,
        output.status.code()
    );
}
